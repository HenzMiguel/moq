use crate::path;
use moq_pattern::{Pattern, Patterns};
use serde::{Deserialize, Serialize};
use serde_with::{OneOrMany, TimestampSeconds, formats::PreferMany, serde_as};

/// Reduce prefix grants to a canonical union: normalized, deduplicated, sorted,
/// with any grant covered by another dropped. The empty prefix covers everything.
fn normalize_prefixes(list: Vec<String>) -> Vec<String> {
	let mut set = std::collections::BTreeSet::new();
	for item in list {
		set.insert(path::normalize(&item));
	}
	let sorted: Vec<String> = set.into_iter().collect();
	let mut out: Vec<String> = Vec::new();
	for item in sorted {
		if out.iter().any(|kept| path::has_prefix(&item, kept)) {
			continue;
		}
		out.push(item);
	}
	out
}

/// The immutable v0 ceiling on what a key may grant, embedded in its JWK.
///
/// Paths in `publish` and `subscribe` are relative to `root`, matching token claim
/// semantics. A key signs a token only when every path the token grants sits at or
/// beneath one the scope allows, in the same role; see [`allows`](Self::allows).
///
/// The scope is fixed at key generation. Widening it means minting a new key, which
/// is the point: a leaked scoped key can never be talked into signing more than it
/// already could. A key with no scope at all is unrestricted, so keys minted before
/// scopes existed keep working.
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
pub struct ScopeV0 {
	/// The root for the publish/subscribe prefixes below.
	#[serde(default, skip_serializing_if = "String::is_empty")]
	pub root: String,

	/// Prefixes this key may grant to publishers.
	#[serde(default, rename = "put", skip_serializing_if = "Vec::is_empty")]
	#[serde_as(as = "OneOrMany<_, PreferMany>")]
	pub publish: Vec<String>,

	/// Prefixes this key may grant to subscribers.
	#[serde(default, rename = "get", skip_serializing_if = "Vec::is_empty")]
	#[serde_as(as = "OneOrMany<_, PreferMany>")]
	pub subscribe: Vec<String>,
}

impl ScopeV0 {
	/// Returns an error when the scope permits nothing, making the key unusable.
	pub fn validate(&self) -> crate::Result<()> {
		if self.publish.is_empty() && self.subscribe.is_empty() {
			return Err(crate::Error::UselessScope);
		}

		Ok(())
	}

	/// Whether every path `claims` grants is covered by this scope, per role.
	///
	/// Both sides are resolved against their own root before comparing, so the same
	/// grant expressed as `root: "demo"` + `put: ["room"]` or as `put: ["demo/room"]`
	/// is treated identically. Matching is segment-aware, so a scope of `live` does
	/// not cover `lively`, and the roles are checked independently: a publish-only
	/// scope never authorizes a subscribe grant.
	///
	/// An empty prefix covers everything beneath the scope root, so a scope of
	/// `root: "demo"` + `put: [""]` grants publish anywhere under `demo`.
	pub fn allows(&self, claims: &ClaimsV0) -> bool {
		covers(&self.root, &self.publish, &claims.root, &claims.publish)
			&& covers(&self.root, &self.subscribe, &claims.root, &claims.subscribe)
	}

	/// The claim version this scope applies to.
	pub fn version(&self) -> u8 {
		0
	}
}

/// Whether every `requested` path (relative to `claims_root`) sits beneath some
/// `granted` prefix (relative to `scope_root`).
///
/// An empty `granted` denies everything, which is what makes a publish-only scope
/// reject subscribe grants rather than ignoring them.
fn covers(scope_root: &str, granted: &[String], claims_root: &str, requested: &[String]) -> bool {
	let absolute = |root: &str, relative: &str| path::join(&path::normalize(root), &path::normalize(relative));

	requested.iter().all(|request| {
		let request = absolute(claims_root, request);
		granted
			.iter()
			.any(|grant| path::has_prefix(&request, &absolute(scope_root, grant)))
	})
}

/// The immutable v1 ceiling on what a key may grant, embedded in its JWK.
///
/// Patterns in `publish` and `subscribe` are relative to `root`. A key signs a
/// token only when every pattern the token grants is contained in some pattern
/// the scope allows, per role; containment is per member, so joint coverage by
/// several members does not count.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScopeV1 {
	/// The root the patterns below are relative to.
	pub root: String,

	/// Patterns this key may grant to publishers, relative to `root`.
	pub publish: Patterns,

	/// Patterns this key may grant to subscribers, relative to `root`.
	pub subscribe: Patterns,
}

impl ScopeV1 {
	/// Returns an error when the scope permits nothing, making the key unusable.
	pub fn validate(&self) -> crate::Result<()> {
		if self.publish.is_empty() && self.subscribe.is_empty() {
			return Err(crate::Error::UselessScope);
		}

		validate_root(&self.root)?;
		Ok(())
	}

	/// Whether every pattern `claims` grants is contained in this scope, per role.
	pub fn allows(&self, claims: &ClaimsV1) -> bool {
		let Ok(scope_publish) = self.publish.rooted(&self.root) else {
			return false;
		};
		let Ok(scope_subscribe) = self.subscribe.rooted(&self.root) else {
			return false;
		};
		let Ok(claims_publish) = claims.publish.rooted(&claims.root) else {
			return false;
		};
		let Ok(claims_subscribe) = claims.subscribe.rooted(&claims.root) else {
			return false;
		};
		scope_publish.covers(&claims_publish) && scope_subscribe.covers(&claims_subscribe)
	}

	/// The claim version this scope applies to.
	pub fn version(&self) -> u8 {
		1
	}
}

/// The versioned ceiling on what a key may grant, embedded in its JWK.
///
/// An unversioned scope is v0 with `put`/`get`; a `v: 1` scope carries
/// `publish`/`subscribe` patterns. Unknown versions and mixed fields fail closed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Scope {
	V0(ScopeV0),
	V1(ScopeV1),
}

impl Default for Scope {
	fn default() -> Self {
		Self::V0(ScopeV0::default())
	}
}

impl Scope {
	/// Returns an error when the scope permits nothing, making the key unusable.
	pub fn validate(&self) -> crate::Result<()> {
		match self {
			Self::V0(scope) => scope.validate(),
			Self::V1(scope) => scope.validate(),
		}
	}

	/// Whether every grant `claims` carries is covered by this scope, per role.
	///
	/// Cross-version pairs never allow: a v0 scope signs only v0 claims, and a
	/// v1 scope signs only v1 claims. An unscoped key is unrestricted; that case
	/// never reaches here.
	pub fn allows(&self, claims: &Claims) -> bool {
		match (self, claims) {
			(Self::V0(scope), Claims::V0(claims)) => scope.allows(claims),
			(Self::V1(scope), Claims::V1(claims)) => scope.allows(claims),
			_ => false,
		}
	}

	/// The claim version this scope applies to.
	pub fn version(&self) -> u8 {
		match self {
			Self::V0(scope) => scope.version(),
			Self::V1(scope) => scope.version(),
		}
	}

	/// The v0 scope, if this is one.
	pub fn as_v0(&self) -> Option<&ScopeV0> {
		match self {
			Self::V0(scope) => Some(scope),
			_ => None,
		}
	}

	/// The v1 scope, if this is one.
	pub fn as_v1(&self) -> Option<&ScopeV1> {
		match self {
			Self::V1(scope) => Some(scope),
			_ => None,
		}
	}
}

impl From<ScopeV0> for Scope {
	fn from(scope: ScopeV0) -> Self {
		Self::V0(scope)
	}
}

impl From<ScopeV1> for Scope {
	fn from(scope: ScopeV1) -> Self {
		Self::V1(scope)
	}
}

impl Serialize for Scope {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		match self {
			Self::V0(scope) => scope.serialize(serializer),
			Self::V1(scope) => serialize_scope_v1(scope, serializer),
		}
	}
}

impl<'de> Deserialize<'de> for Scope {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let value = serde_json::Value::deserialize(deserializer)?;
		parse_scope(&value).map_err(serde::de::Error::custom)
	}
}

fn parse_scope(value: &serde_json::Value) -> crate::Result<Scope> {
	let obj = value
		.as_object()
		.ok_or(crate::Error::Json("scope must be an object".into()))?;
	let version = match obj.get("v") {
		None => 0,
		Some(serde_json::Value::Number(n)) => n.as_u64().ok_or(crate::Error::MixedClaims)?,
		Some(_) => return Err(crate::Error::MixedClaims),
	};
	match version {
		0 => {
			if obj.contains_key("publish") || obj.contains_key("subscribe") {
				return Err(crate::Error::MixedClaims);
			}
			let mut scope: ScopeV0 = serde_json::from_value(value.clone())?;
			scope.publish = normalize_prefixes(scope.publish);
			scope.subscribe = normalize_prefixes(scope.subscribe);
			Ok(Scope::V0(scope))
		}
		1 => {
			if obj.contains_key("put") || obj.contains_key("get") {
				return Err(crate::Error::MixedClaims);
			}
			Ok(Scope::V1(parse_scope_v1(value)?))
		}
		version => Err(crate::Error::UnsupportedVersion(version)),
	}
}

fn serialize_scope_v1<S: serde::Serializer>(scope: &ScopeV1, serializer: S) -> Result<S::Ok, S::Error> {
	use serde::ser::SerializeStruct;
	let mut state = serializer.serialize_struct("ScopeV1", 4)?;
	state.serialize_field("v", &1)?;
	if !scope.root.is_empty() {
		state.serialize_field("root", &scope.root)?;
	}
	state.serialize_field("publish", &scope.publish)?;
	state.serialize_field("subscribe", &scope.subscribe)?;
	state.end()
}

fn parse_scope_v1(value: &serde_json::Value) -> crate::Result<ScopeV1> {
	let obj = value
		.as_object()
		.ok_or(crate::Error::Json("scope must be an object".into()))?;
	let root = match obj.get("root") {
		None => String::new(),
		Some(serde_json::Value::String(root)) => root.clone(),
		Some(_) => return Err(crate::Error::Json("scope root must be a string".into())),
	};
	validate_root(&root)?;
	let publish = parse_patterns_field(obj.get("publish"))?;
	let subscribe = parse_patterns_field(obj.get("subscribe"))?;
	let scope = ScopeV1 {
		root,
		publish,
		subscribe,
	};
	scope.validate()?;
	Ok(scope)
}

/// The v0 access a [`ClaimsV0`] grants at a specific path, with every prefix rebased so
/// it is relative to that path.
///
/// Produced by [`ClaimsV0::authorize`]. An empty string grants the path itself and
/// everything beneath it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Permissions {
	/// Paths the holder may subscribe to, relative to the authorized path.
	pub subscribe: Vec<String>,

	/// Paths the holder may publish to, relative to the authorized path.
	pub publish: Vec<String>,
}

/// The v1 access a [`ClaimsV1`] grants at a specific path, with every pattern rebased so
/// it is relative to that path.
///
/// Produced by [`ClaimsV1::authorize`]. The empty pattern matches only the path itself;
/// `**` matches the path and everything beneath it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
	/// Patterns the holder may subscribe to, relative to the authorized path.
	pub subscribe: Patterns,

	/// Patterns the holder may publish to, relative to the authorized path.
	pub publish: Patterns,
}

/// The versioned access [`Claims::authorize`] grants at a specific path.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Authorization {
	V0(Permissions),
	V1(Grants),
}

/// The v0 payload of a token: a root, plus the publish/subscribe prefixes granted beneath it.
///
/// Build one from [`Default`] with the `with_*` setters, sign it with
/// [`Key::sign`](crate::Key::sign), and scope it to a connection with
/// [`authorize`](Self::authorize).
///
/// ```no_run
/// let claims = moq_token::ClaimsV0::default()
///     .with_root("room/123")
///     .with_publish(["alice"])
///     .with_subscribe([""]);
/// ```
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct ClaimsV0 {
	/// The root for the publish/subscribe options below.
	/// It's mostly for compression and is optional, defaulting to the empty string.
	#[serde(default, rename = "root", skip_serializing_if = "String::is_empty")]
	pub root: String,

	/// If specified, the user can publish any matching broadcasts.
	/// If not specified, the user will not publish any broadcasts.
	#[serde(default, rename = "put", skip_serializing_if = "Vec::is_empty")]
	#[serde_as(as = "OneOrMany<_, PreferMany>")]
	pub publish: Vec<String>,

	/// If specified, the user can subscribe to any matching broadcasts.
	/// If not specified, the user will not receive announcements and cannot subscribe to any broadcasts.
	// NOTE: This can't be renamed to "sub" because that's a reserved JWT field.
	#[serde(default, rename = "get", skip_serializing_if = "Vec::is_empty")]
	#[serde_as(as = "OneOrMany<_, PreferMany>")]
	pub subscribe: Vec<String>,

	/// The expiration time of the token as a unix timestamp.
	#[serde(rename = "exp")]
	#[serde_as(as = "Option<TimestampSeconds<i64>>")]
	pub expires: Option<std::time::SystemTime>,

	/// The issued time of the token as a unix timestamp.
	#[serde(rename = "iat")]
	#[serde_as(as = "Option<TimestampSeconds<i64>>")]
	pub issued: Option<std::time::SystemTime>,
}

impl ClaimsV0 {
	/// Set the root that the publish/subscribe prefixes are relative to.
	pub fn with_root(mut self, root: impl Into<String>) -> Self {
		self.root = root.into();
		self
	}

	/// Grant publish access to these prefixes, relative to the root.
	pub fn with_publish(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
		self.publish = normalize_prefixes(paths.into_iter().map(Into::into).collect());
		self
	}

	/// Grant subscribe access to these prefixes, relative to the root.
	pub fn with_subscribe(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
		self.subscribe = normalize_prefixes(paths.into_iter().map(Into::into).collect());
		self
	}

	/// Expire the token at this time. Enforced by [`Key::verify`](crate::Key::verify).
	///
	/// Accepts an `Option` so a caller can pass one through without unwrapping it.
	pub fn with_expires(mut self, at: impl Into<Option<std::time::SystemTime>>) -> Self {
		self.expires = at.into();
		self
	}

	/// Record when the token was issued. Purely informational; nothing enforces it.
	///
	/// Accepts an `Option` so a caller can pass one through without unwrapping it.
	pub fn with_issued(mut self, at: impl Into<Option<std::time::SystemTime>>) -> Self {
		self.issued = at.into();
		self
	}

	/// Returns an error when the token grants nothing at all, making it useless.
	pub fn validate(&self) -> crate::Result<()> {
		if self.publish.is_empty() && self.subscribe.is_empty() {
			return Err(crate::Error::UselessToken);
		}

		Ok(())
	}

	/// The claim version.
	pub fn version(&self) -> u8 {
		0
	}

	/// The access these claims grant at `path`, rebased so each returned prefix is
	/// relative to `path`.
	///
	/// `path` and [`root`](Self::root) must overlap, in either direction:
	///
	/// - `path` extends the root (root `demo`, path `demo/room`), so the extra
	///   `room` narrows each prefix and drops the ones outside it.
	/// - `path` is a parent of the root (root `demo`, path ``), so `demo` is
	///   prepended to each prefix to keep it anchored where the token points.
	///
	/// Matching is segment-aware, so a root of `foo` does not cover `foobar`.
	/// Slashes at the boundaries are implicit: `/demo/` and `demo` are the same path.
	///
	/// Returns [`Error::RootMismatch`](crate::Error::RootMismatch) when the two don't
	/// overlap, and [`Error::NoAccess`](crate::Error::NoAccess) when they do but every
	/// prefix falls outside `path`.
	///
	/// This is authorization only. Verify the signature first with
	/// [`Key::verify`](crate::Key::verify), which is where expiry is enforced.
	pub fn authorize(&self, path: &str) -> crate::Result<Permissions> {
		let path = path::normalize(path);
		let root = path::normalize(&self.root);

		// Exactly one of these is non-empty: `suffix` is how far the path reaches
		// past the root, `prefix` is how far the root reaches past the path.
		let (suffix, prefix) = if let Some(suffix) = path::strip_prefix(&path, &root) {
			(suffix, "")
		} else if let Some(prefix) = path::strip_prefix(&root, &path) {
			("", prefix)
		} else {
			return Err(crate::Error::RootMismatch(path));
		};

		let scope = |paths: &[String]| -> Vec<String> {
			let mut out: Vec<String> = paths
				.iter()
				.filter_map(|granted| {
					let granted = path::join(prefix, &path::normalize(granted));

					if let Some(remaining) = path::strip_prefix(&granted, suffix) {
						// The grant covers the path; keep what's left below it.
						Some(remaining.to_string())
					} else if path::has_prefix(suffix, &granted) {
						// The grant stops short of the path but still contains it,
						// so everything below the path is granted.
						Some(String::new())
					} else {
						None
					}
				})
				.collect();
			out = normalize_prefixes(out);
			out
		};

		let permissions = Permissions {
			subscribe: scope(&self.subscribe),
			publish: scope(&self.publish),
		};

		if permissions.subscribe.is_empty() && permissions.publish.is_empty() {
			return Err(crate::Error::NoAccess(path));
		}

		Ok(permissions)
	}
}

/// The v1 payload of a token: a root, plus the publish/subscribe patterns granted beneath it.
///
/// Patterns are relative to `root` and exact: `foo` matches only `foo`, while a
/// subtree is `foo/**`. Build one from [`Default`] with the `with_*` setters,
/// sign it with [`Key::sign`](crate::Key::sign), and scope it to a connection
/// with [`authorize`](Self::authorize).
///
/// ```no_run
/// let chat: moq_token::Pattern = "*/chat".parse().unwrap();
/// let hang: moq_token::Pattern = "**/*.hang".parse().unwrap();
/// let claims = moq_token::ClaimsV1::default()
///     .with_root("pid")
///     .with_publish([chat])
///     .with_subscribe([hang]);
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ClaimsV1 {
	/// The root the patterns below are relative to.
	pub root: String,

	/// Patterns the holder may publish to, relative to `root`.
	pub publish: Patterns,

	/// Patterns the holder may subscribe to, relative to `root`.
	pub subscribe: Patterns,

	/// The expiration time of the token as a unix timestamp.
	pub expires: Option<std::time::SystemTime>,

	/// The issued time of the token as a unix timestamp.
	pub issued: Option<std::time::SystemTime>,
}

impl ClaimsV1 {
	/// Set the root that the publish/subscribe patterns are relative to.
	pub fn with_root(mut self, root: impl Into<String>) -> Self {
		self.root = root.into();
		self
	}

	/// Grant publish access to these patterns, relative to the root.
	pub fn with_publish(mut self, patterns: impl IntoIterator<Item = Pattern>) -> Self {
		self.publish = patterns.into_iter().collect();
		self
	}

	/// Grant subscribe access to these patterns, relative to the root.
	pub fn with_subscribe(mut self, patterns: impl IntoIterator<Item = Pattern>) -> Self {
		self.subscribe = patterns.into_iter().collect();
		self
	}

	/// Expire the token at this time. Enforced by [`Key::verify`](crate::Key::verify).
	///
	/// Accepts an `Option` so a caller can pass one through without unwrapping it.
	pub fn with_expires(mut self, at: impl Into<Option<std::time::SystemTime>>) -> Self {
		self.expires = at.into();
		self
	}

	/// Record when the token was issued. Purely informational; nothing enforces it.
	///
	/// Accepts an `Option` so a caller can pass one through without unwrapping it.
	pub fn with_issued(mut self, at: impl Into<Option<std::time::SystemTime>>) -> Self {
		self.issued = at.into();
		self
	}

	/// Returns an error when the token grants nothing at all, making it useless.
	pub fn validate(&self) -> crate::Result<()> {
		if self.publish.is_empty() && self.subscribe.is_empty() {
			return Err(crate::Error::UselessToken);
		}
		validate_root(&self.root)?;
		Ok(())
	}

	/// The claim version.
	pub fn version(&self) -> u8 {
		1
	}

	/// The access these claims grant at `path`, rebased so each returned pattern is
	/// relative to `path`.
	///
	/// The absolute grant is each pattern placed beneath [`root`](Self::root),
	/// then rebased at `path`. Rebase is set-valued: `**/a` at `a` is both the
	/// empty pattern and `**/a`. Returns [`Error::RootMismatch`](crate::Error::RootMismatch)
	/// when `path` and the root do not overlap, and
	/// [`Error::NoAccess`](crate::Error::NoAccess) when they do but no pattern
	/// matches beneath `path`.
	///
	/// This is authorization only. Verify the signature first with
	/// [`Key::verify`](crate::Key::verify), which is where expiry is enforced.
	pub fn authorize(&self, path: &str) -> crate::Result<Grants> {
		let normalized_path = path::normalize(path);
		let normalized_root = path::normalize(&self.root);

		let overlaps = path::strip_prefix(&normalized_path, &normalized_root).is_some()
			|| path::strip_prefix(&normalized_root, &normalized_path).is_some();
		if !overlaps {
			return Err(crate::Error::RootMismatch(normalized_path));
		}

		let publish = self
			.publish
			.rooted(&self.root)
			.map_err(|err| crate::Error::BadPattern(err.to_string()))?
			.rebase(path);
		let subscribe = self
			.subscribe
			.rooted(&self.root)
			.map_err(|err| crate::Error::BadPattern(err.to_string()))?
			.rebase(path);

		if publish.is_empty() && subscribe.is_empty() {
			return Err(crate::Error::NoAccess(normalized_path));
		}

		Ok(Grants { subscribe, publish })
	}
}

impl Serialize for ClaimsV1 {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serialize_claims_v1(self, serializer)
	}
}

impl<'de> Deserialize<'de> for ClaimsV1 {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let value = serde_json::Value::deserialize(deserializer)?;
		parse_claims_v1(&value).map_err(serde::de::Error::custom)
	}
}

fn validate_root(root: &str) -> crate::Result<()> {
	if root.contains('*') {
		return Err(crate::Error::InvalidRoot(root.to_string()));
	}
	// Reject empty segments the same way a literal path would: a root is a path,
	// never a pattern, so `*` and `**` cannot name it.
	if !root.is_empty() {
		Pattern::literal(root).map_err(|err| crate::Error::InvalidRoot(format!("{root}: {err}")))?;
	}
	Ok(())
}

fn serialize_claims_v1<S: serde::Serializer>(claims: &ClaimsV1, serializer: S) -> Result<S::Ok, S::Error> {
	use serde::ser::SerializeStruct;
	let mut state = serializer.serialize_struct("ClaimsV1", 6)?;
	state.serialize_field("v", &1)?;
	if !claims.root.is_empty() {
		state.serialize_field("root", &claims.root)?;
	}
	state.serialize_field("publish", &claims.publish)?;
	state.serialize_field("subscribe", &claims.subscribe)?;
	if let Some(expires) = claims.expires {
		let secs = expires
			.duration_since(std::time::UNIX_EPOCH)
			.map_err(serde::ser::Error::custom)?
			.as_secs() as i64;
		state.serialize_field("exp", &secs)?;
	}
	if let Some(issued) = claims.issued {
		let secs = issued
			.duration_since(std::time::UNIX_EPOCH)
			.map_err(serde::ser::Error::custom)?
			.as_secs() as i64;
		state.serialize_field("iat", &secs)?;
	}
	state.end()
}

fn system_time_from_secs(secs: i64) -> crate::Result<std::time::SystemTime> {
	let secs: u64 = secs
		.try_into()
		.map_err(|_| crate::Error::Json("timestamp out of range".into()))?;
	std::time::UNIX_EPOCH
		.checked_add(std::time::Duration::from_secs(secs))
		.ok_or(crate::Error::Json("timestamp out of range".into()))
}

fn parse_time_field(
	obj: &serde_json::Map<String, serde_json::Value>,
	field: &str,
) -> crate::Result<Option<std::time::SystemTime>> {
	match obj.get(field) {
		None => Ok(None),
		Some(serde_json::Value::Number(n)) => {
			let secs = n
				.as_i64()
				.ok_or(crate::Error::Json(format!("{field} must be a unix timestamp")))?;
			Ok(Some(system_time_from_secs(secs)?))
		}
		Some(_) => Err(crate::Error::Json(format!("{field} must be a unix timestamp"))),
	}
}

fn parse_patterns_field(value: Option<&serde_json::Value>) -> crate::Result<Patterns> {
	match value {
		None => Ok(Patterns::new()),
		Some(serde_json::Value::String(one)) => {
			let pattern: Pattern = one.parse()?;
			Ok([pattern].into_iter().collect())
		}
		Some(serde_json::Value::Array(list)) => {
			let mut out = Patterns::new();
			for item in list {
				let text = item
					.as_str()
					.ok_or(crate::Error::Json("patterns must be strings".into()))?;
				let pattern: Pattern = text.parse()?;
				out.insert(pattern);
			}
			Ok(out)
		}
		Some(_) => Err(crate::Error::Json("patterns must be a string or an array".into())),
	}
}

fn parse_claims_v1(value: &serde_json::Value) -> crate::Result<ClaimsV1> {
	let obj = value
		.as_object()
		.ok_or(crate::Error::Json("claims must be an object".into()))?;
	if obj.contains_key("put") || obj.contains_key("get") {
		return Err(crate::Error::MixedClaims);
	}
	let root = match obj.get("root") {
		None => String::new(),
		Some(serde_json::Value::String(root)) => root.clone(),
		Some(_) => return Err(crate::Error::Json("root must be a string".into())),
	};
	validate_root(&root)?;
	let publish = parse_patterns_field(obj.get("publish"))?;
	let subscribe = parse_patterns_field(obj.get("subscribe"))?;
	let expires = parse_time_field(obj, "exp")?;
	let issued = parse_time_field(obj, "iat")?;
	let claims = ClaimsV1 {
		root,
		publish,
		subscribe,
		expires,
		issued,
	};
	claims.validate()?;
	Ok(claims)
}

/// The versioned payload of a token.
///
/// Missing `v` decodes legacy `put`/`get` prefixes as [`Claims::V0`]; `v: 1`
/// decodes `publish`/`subscribe` patterns as [`Claims::V1`]. Unknown versions
/// and mixed fields fail closed.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Claims {
	V0(ClaimsV0),
	V1(ClaimsV1),
}

impl Default for Claims {
	fn default() -> Self {
		Self::V0(ClaimsV0::default())
	}
}

impl Claims {
	/// Returns an error when the token grants nothing at all, making it useless.
	pub fn validate(&self) -> crate::Result<()> {
		match self {
			Self::V0(claims) => claims.validate(),
			Self::V1(claims) => claims.validate(),
		}
	}

	/// The claim version: 0 for prefix grants, 1 for pattern grants.
	pub fn version(&self) -> u8 {
		match self {
			Self::V0(claims) => claims.version(),
			Self::V1(claims) => claims.version(),
		}
	}

	/// The expiration time of the token, if any.
	pub fn expires(&self) -> Option<std::time::SystemTime> {
		match self {
			Self::V0(claims) => claims.expires,
			Self::V1(claims) => claims.expires,
		}
	}

	/// The issued time of the token, if any.
	pub fn issued(&self) -> Option<std::time::SystemTime> {
		match self {
			Self::V0(claims) => claims.issued,
			Self::V1(claims) => claims.issued,
		}
	}

	/// The root the grants below are relative to.
	pub fn root(&self) -> &str {
		match self {
			Self::V0(claims) => &claims.root,
			Self::V1(claims) => &claims.root,
		}
	}

	/// The v0 claims, if this is a v0 token.
	pub fn as_v0(&self) -> Option<&ClaimsV0> {
		match self {
			Self::V0(claims) => Some(claims),
			_ => None,
		}
	}

	/// The v1 claims, if this is a v1 token.
	pub fn as_v1(&self) -> Option<&ClaimsV1> {
		match self {
			Self::V1(claims) => Some(claims),
			_ => None,
		}
	}

	/// The access these claims grant at `path`.
	///
	/// V0 returns rebased prefix lists; v1 returns rebased pattern sets. See
	/// [`ClaimsV0::authorize`] and [`ClaimsV1::authorize`].
	///
	/// This is authorization only. Verify the signature first with
	/// [`Key::verify`](crate::Key::verify), which is where expiry is enforced.
	pub fn authorize(&self, path: &str) -> crate::Result<Authorization> {
		match self {
			Self::V0(claims) => Ok(Authorization::V0(claims.authorize(path)?)),
			Self::V1(claims) => Ok(Authorization::V1(claims.authorize(path)?)),
		}
	}

	/// Set the root that the grants below are relative to.
	///
	/// Issuers stay on v0 until the M2 rollout; this keeps the v0 builder working
	/// on the versioned type. For v1 patterns, build a [`ClaimsV1`] explicitly.
	pub fn with_root(mut self, root: impl Into<String>) -> Self {
		match &mut self {
			Self::V0(claims) => claims.root = root.into(),
			Self::V1(claims) => claims.root = root.into(),
		}
		self
	}

	/// Grant v0 publish prefixes, relative to the root.
	///
	/// Panics on a v1 token: patterns cannot be expressed as prefixes. Build a
	/// [`ClaimsV1`] explicitly instead.
	pub fn with_publish(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
		match &mut self {
			Self::V0(claims) => claims.publish = normalize_prefixes(paths.into_iter().map(Into::into).collect()),
			Self::V1(_) => panic!("with_publish takes v0 prefixes; build ClaimsV1 for patterns"),
		}
		self
	}

	/// Grant v0 subscribe prefixes, relative to the root.
	///
	/// Panics on a v1 token: patterns cannot be expressed as prefixes. Build a
	/// [`ClaimsV1`] explicitly instead.
	pub fn with_subscribe(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
		match &mut self {
			Self::V0(claims) => claims.subscribe = normalize_prefixes(paths.into_iter().map(Into::into).collect()),
			Self::V1(_) => panic!("with_subscribe takes v0 prefixes; build ClaimsV1 for patterns"),
		}
		self
	}

	/// Expire the token at this time. Enforced by [`Key::verify`](crate::Key::verify).
	pub fn with_expires(mut self, at: impl Into<Option<std::time::SystemTime>>) -> Self {
		let at = at.into();
		match &mut self {
			Self::V0(claims) => claims.expires = at,
			Self::V1(claims) => claims.expires = at,
		}
		self
	}

	/// Record when the token was issued. Purely informational; nothing enforces it.
	pub fn with_issued(mut self, at: impl Into<Option<std::time::SystemTime>>) -> Self {
		let at = at.into();
		match &mut self {
			Self::V0(claims) => claims.issued = at,
			Self::V1(claims) => claims.issued = at,
		}
		self
	}
}

impl From<ClaimsV0> for Claims {
	fn from(claims: ClaimsV0) -> Self {
		Self::V0(claims)
	}
}

impl From<ClaimsV1> for Claims {
	fn from(claims: ClaimsV1) -> Self {
		Self::V1(claims)
	}
}

impl TryFrom<Claims> for ClaimsV0 {
	type Error = crate::Error;

	fn try_from(claims: Claims) -> crate::Result<Self> {
		match claims {
			Claims::V0(claims) => Ok(claims),
			Claims::V1(_) => Err(crate::Error::MixedClaims),
		}
	}
}

impl TryFrom<Claims> for ClaimsV1 {
	type Error = crate::Error;

	fn try_from(claims: Claims) -> crate::Result<Self> {
		match claims {
			Claims::V1(claims) => Ok(claims),
			Claims::V0(_) => Err(crate::Error::MixedClaims),
		}
	}
}

impl Serialize for Claims {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		match self {
			Self::V0(claims) => claims.serialize(serializer),
			Self::V1(claims) => claims.serialize(serializer),
		}
	}
}

impl<'de> Deserialize<'de> for Claims {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let value = serde_json::Value::deserialize(deserializer)?;
		parse_claims(&value).map_err(serde::de::Error::custom)
	}
}

fn parse_claims(value: &serde_json::Value) -> crate::Result<Claims> {
	let obj = value
		.as_object()
		.ok_or(crate::Error::Json("claims must be an object".into()))?;
	let version = match obj.get("v") {
		None => 0,
		Some(serde_json::Value::Number(n)) => n.as_u64().ok_or(crate::Error::MixedClaims)?,
		Some(_) => return Err(crate::Error::MixedClaims),
	};
	match version {
		0 => {
			if obj.contains_key("publish") || obj.contains_key("subscribe") {
				return Err(crate::Error::MixedClaims);
			}
			let mut claims: ClaimsV0 = serde_json::from_value(value.clone())?;
			claims.publish = normalize_prefixes(claims.publish);
			claims.subscribe = normalize_prefixes(claims.subscribe);
			Ok(Claims::V0(claims))
		}
		1 => Ok(Claims::V1(parse_claims_v1(value)?)),
		version => Err(crate::Error::UnsupportedVersion(version)),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use std::time::{Duration, SystemTime};

	fn create_test_claims() -> ClaimsV0 {
		ClaimsV0 {
			root: "test-path".to_string(),
			publish: vec!["test-pub".into()],
			subscribe: vec!["test-sub".into()],
			expires: Some(SystemTime::now() + Duration::from_secs(3600)),
			issued: Some(SystemTime::now()),
		}
	}

	fn v1_claims(root: &str, publish: &[&str], subscribe: &[&str]) -> ClaimsV1 {
		let parse = |list: &[&str]| list.iter().map(|text| text.parse().unwrap()).collect();
		ClaimsV1 {
			root: root.to_string(),
			publish: parse(publish),
			subscribe: parse(subscribe),
			expires: None,
			issued: None,
		}
	}

	#[test]
	fn v1_wire_shape_matches_the_quest() {
		let claims = Claims::V1(v1_claims("pid", &["*/chat"], &["**/*.hang"]));
		let json = serde_json::to_value(&claims).unwrap();
		assert_eq!(json["v"], 1);
		assert_eq!(json["root"], "pid");
		assert_eq!(json["publish"], serde_json::json!(["*/chat"]));
		assert_eq!(json["subscribe"], serde_json::json!(["**/*.hang"]));

		let roundtrip: Claims = serde_json::from_value(json).unwrap();
		assert_eq!(roundtrip, claims);
	}

	#[test]
	fn missing_v_reads_legacy_put_get() {
		let claims: Claims = serde_json::from_str(r#"{"root":"demo","put":["a"],"get":"b"}"#).unwrap();
		let Claims::V0(v0) = claims else { panic!("expected v0") };
		assert_eq!(v0.publish, ["a"]);
		assert_eq!(v0.subscribe, ["b"]);
	}

	#[test]
	fn unknown_versions_and_mixed_fields_fail_closed() {
		for json in [
			r#"{"v":2,"root":"demo","publish":["a"]}"#,
			r#"{"v":1,"root":"demo","put":["a"]}"#,
			r#"{"v":1,"root":"demo","get":["a"]}"#,
			r#"{"v":0,"root":"demo","publish":["a"]}"#,
			r#"{"root":"demo","put":["a"],"publish":["a"]}"#,
			r#"{"root":"demo","get":["a"],"subscribe":["a"]}"#,
			r#"{"v":"1","root":"demo","publish":["a"]}"#,
		] {
			assert!(serde_json::from_str::<Claims>(json).is_err(), "{json}");
		}
	}

	#[test]
	fn v1_rejects_invalid_patterns_and_roots() {
		assert!(serde_json::from_str::<Claims>(r#"{"v":1,"root":"demo","publish":["a//b"]}"#).is_err());
		assert!(serde_json::from_str::<Claims>(r#"{"v":1,"root":"a*b","publish":["c"]}"#).is_err());
		assert!(serde_json::from_str::<Claims>(r#"{"v":1,"root":"demo","publish":[]}"#).is_err());
	}

	#[test]
	fn v1_lists_normalize_to_reduced_unions() {
		let claims: Claims = serde_json::from_str(r#"{"v":1,"root":"demo","publish":["a/**","a/b","b","b"]}"#).unwrap();
		let Claims::V1(v1) = claims else { panic!("expected v1") };
		let texts: Vec<_> = v1.publish.iter().map(|pattern| pattern.as_str()).collect();
		assert_eq!(texts, ["a/**", "b"]);
	}

	#[test]
	fn v0_lists_normalize_to_reduced_unions() {
		let claims: Claims = serde_json::from_str(r#"{"root":"demo","put":["a/b","a","a","b"]}"#).unwrap();
		let Claims::V0(v0) = claims else { panic!("expected v0") };
		assert_eq!(v0.publish, ["a", "b"]);
	}

	#[test]
	fn v1_authorize_returns_exact_residuals_below_the_root() {
		let claims = v1_claims("pid", &["*/chat"], &["**/*.hang"]);
		let grants = claims.authorize("pid/alice").unwrap();
		let publish: Vec<_> = grants.publish.iter().map(|pattern| pattern.as_str()).collect();
		assert_eq!(publish, ["chat"]);
		let subscribe: Vec<_> = grants.subscribe.iter().map(|pattern| pattern.as_str()).collect();
		assert_eq!(subscribe, ["**/*.hang"]);
	}

	#[test]
	fn v1_authorize_returns_exact_residuals_above_the_root() {
		let claims = v1_claims("pid", &["*/chat"], &["**/*.hang"]);
		let grants = claims.authorize("").unwrap();
		let publish: Vec<_> = grants.publish.iter().map(|pattern| pattern.as_str()).collect();
		assert_eq!(publish, ["pid/*/chat"]);
		let subscribe: Vec<_> = grants.subscribe.iter().map(|pattern| pattern.as_str()).collect();
		assert_eq!(subscribe, ["pid/**/*.hang"]);
	}

	#[test]
	fn v1_authorize_is_set_valued() {
		let claims = v1_claims("", &[], &["**/a"]);
		let grants = claims.authorize("a").unwrap();
		let subscribe: Vec<_> = grants.subscribe.iter().map(|pattern| pattern.as_str()).collect();
		assert_eq!(subscribe, ["", "**/a"]);
	}

	#[test]
	fn v1_authorize_rejects_mismatch_and_no_access() {
		let claims = v1_claims("pid", &["*/chat"], &[]);
		assert!(matches!(claims.authorize("other"), Err(crate::Error::RootMismatch(_))));
		assert!(matches!(
			claims.authorize("pid/bob/extra"),
			Err(crate::Error::NoAccess(_))
		));
	}

	#[test]
	fn v1_scope_contains_subsets_and_rejects_escapes() {
		let scope = ScopeV1 {
			root: "pid".into(),
			publish: ["*/chat".parse().unwrap()].into_iter().collect(),
			subscribe: ["**/*.hang".parse().unwrap()].into_iter().collect(),
		};
		assert!(scope.allows(&v1_claims("pid", &["alice/chat"], &["pid/demo.hang"])));
		assert!(!scope.allows(&v1_claims("pid", &["alice/chat/extra"], &[])));
		assert!(!scope.allows(&v1_claims("pid", &[], &["other/demo.msf"])));
		assert!(!scope.allows(&v1_claims("other", &["alice/chat"], &[])));
	}

	#[test]
	fn scope_version_must_match_claims() {
		let v0_scope = Scope::V0(ScopeV0 {
			root: "pid".into(),
			publish: vec!["".into()],
			subscribe: vec![],
		});
		let v1_claims = Claims::V1(v1_claims("pid", &["a"], &[]));
		assert!(!v0_scope.allows(&v1_claims));

		let v1_scope = Scope::V1(ScopeV1 {
			root: "pid".into(),
			publish: ["**".parse().unwrap()].into_iter().collect(),
			subscribe: Patterns::new(),
		});
		let v0_claims = Claims::V0(ClaimsV0::default().with_root("pid").with_publish(["a"]));
		assert!(!v1_scope.allows(&v0_claims));
	}

	#[test]
	fn scope_allows_contained_claims() {
		let scope = ScopeV0 {
			root: "project".into(),
			publish: vec!["live".into()],
			subscribe: vec!["watch".into()],
		};
		let claims = ClaimsV0 {
			root: "project/live/room".into(),
			publish: vec!["".into()],
			subscribe: vec![],
			..Default::default()
		};
		assert!(scope.allows(&claims));
	}

	#[test]
	fn scope_rejects_sibling_and_role_escalation() {
		let scope = ScopeV0 {
			root: "project".into(),
			publish: vec!["live".into()],
			subscribe: vec![],
		};
		let sibling = ClaimsV0 {
			root: "project/lively".into(),
			publish: vec!["".into()],
			..Default::default()
		};
		let role = ClaimsV0 {
			root: "project/live".into(),
			subscribe: vec!["".into()],
			..Default::default()
		};
		assert!(!scope.allows(&sibling));
		assert!(!scope.allows(&role));
	}

	#[test]
	fn scope_ignores_how_the_root_is_split() {
		// The same grant, expressed three ways, must compare identically.
		let scope = ScopeV0 {
			root: "project".into(),
			publish: vec!["live".into()],
			subscribe: vec![],
		};

		for claims in [
			ClaimsV0 {
				root: "project".into(),
				publish: vec!["live/room".into()],
				..Default::default()
			},
			ClaimsV0 {
				root: String::new(),
				publish: vec!["project/live/room".into()],
				..Default::default()
			},
			ClaimsV0 {
				root: "/project/live/".into(),
				publish: vec!["/room".into()],
				..Default::default()
			},
		] {
			assert!(scope.allows(&claims), "{claims:?}");
		}
	}

	#[test]
	fn scope_rejects_escaping_above_its_root() {
		let scope = ScopeV0 {
			root: "project".into(),
			publish: vec!["live".into()],
			subscribe: vec![],
		};

		// A root above the scope's does not widen it, even though the empty prefix
		// would grant everything within the scope.
		let claims = ClaimsV0 {
			root: String::new(),
			publish: vec!["".into()],
			..Default::default()
		};
		assert!(!scope.allows(&claims));
	}

	#[test]
	fn scope_empty_prefix_grants_everything_beneath_it() {
		let scope = ScopeV0 {
			root: "project".into(),
			publish: vec![String::new()],
			subscribe: vec![],
		};
		let claims = ClaimsV0 {
			root: "project/anything/deep".into(),
			publish: vec!["".into()],
			..Default::default()
		};
		assert!(scope.allows(&claims));
	}

	#[test]
	fn scope_requires_every_requested_path() {
		// One allowed path does not carry an unallowed sibling along with it.
		let scope = ScopeV0 {
			root: "project".into(),
			publish: vec!["live".into()],
			subscribe: vec![],
		};
		let claims = ClaimsV0 {
			root: "project".into(),
			publish: vec!["live/room".into(), "other".into()],
			..Default::default()
		};
		assert!(!scope.allows(&claims));
	}

	#[test]
	fn scope_without_grants_is_useless() {
		assert!(matches!(ScopeV0::default().validate(), Err(crate::Error::UselessScope)));
		assert!(matches!(
			Scope::V0(ScopeV0::default()).validate(),
			Err(crate::Error::UselessScope)
		));
	}

	#[test]
	fn test_claims_validation_success() {
		let claims = create_test_claims();
		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_validation_no_publish_or_subscribe() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(),
			publish: vec![],
			subscribe: vec![],
			expires: None,
			issued: None,
		};

		let result = claims.validate();
		assert!(result.is_err());
		assert!(
			result
				.unwrap_err()
				.to_string()
				.contains("no publish or subscribe allowed; token is useless")
		);
	}

	#[test]
	fn test_claims_validation_only_publish() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(),
			publish: vec!["test-pub".into()],
			subscribe: vec![],
			expires: None,
			issued: None,
		};

		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_validation_only_subscribe() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(),
			publish: vec![],
			subscribe: vec!["test-sub".into()],
			expires: None,
			issued: None,
		};

		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_validation_path_not_prefix_relative_publish() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(),        // no trailing slash
			publish: vec!["relative-pub".into()], // relative path without leading slash
			subscribe: vec![],
			expires: None,
			issued: None,
		};

		let result = claims.validate();
		assert!(result.is_ok()); // Now passes because slashes are implicitly added
	}

	#[test]
	fn test_claims_validation_path_not_prefix_relative_subscribe() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(), // no trailing slash
			publish: vec![],
			subscribe: vec!["relative-sub".into()], // relative path without leading slash
			expires: None,
			issued: None,
		};

		let result = claims.validate();
		assert!(result.is_ok()); // Now passes because slashes are implicitly added
	}

	#[test]
	fn test_claims_validation_path_not_prefix_absolute_publish() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(),         // no trailing slash
			publish: vec!["/absolute-pub".into()], // absolute path with leading slash
			subscribe: vec![],
			expires: None,
			issued: None,
		};

		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_validation_path_not_prefix_absolute_subscribe() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(), // no trailing slash
			publish: vec![],
			subscribe: vec!["/absolute-sub".into()], // absolute path with leading slash
			expires: None,
			issued: None,
		};

		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_validation_path_not_prefix_empty_publish() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(), // no trailing slash
			publish: vec!["".into()],      // empty string
			subscribe: vec![],
			expires: None,
			issued: None,
		};

		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_validation_path_not_prefix_empty_subscribe() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(), // no trailing slash
			publish: vec![],
			subscribe: vec!["".into()], // empty string
			expires: None,
			issued: None,
		};

		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_validation_path_is_prefix() {
		let claims = ClaimsV0 {
			root: "test-path".to_string(),          // with trailing slash
			publish: vec!["relative-pub".into()],   // relative path is ok when path is prefix
			subscribe: vec!["relative-sub".into()], // relative path is ok when path is prefix
			expires: None,
			issued: None,
		};

		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_validation_empty_path() {
		let claims = ClaimsV0 {
			root: "".to_string(), // empty path
			publish: vec!["test-pub".into()],
			subscribe: vec![],
			expires: None,
			issued: None,
		};

		assert!(claims.validate().is_ok());
	}

	#[test]
	fn test_claims_serde() {
		let claims = create_test_claims();
		let json = serde_json::to_string(&claims).unwrap();
		let deserialized: ClaimsV0 = serde_json::from_str(&json).unwrap();

		assert_eq!(deserialized.root, claims.root);
		assert_eq!(deserialized.publish, claims.publish);
		assert_eq!(deserialized.subscribe, claims.subscribe);
	}

	#[test]
	fn test_claims_default() {
		let claims = ClaimsV0::default();
		assert_eq!(claims.root, "");
		assert!(claims.publish.is_empty());
		assert!(claims.subscribe.is_empty());
		assert_eq!(claims.expires, None);
		assert_eq!(claims.issued, None);
	}

	fn authorize_claims(root: &str, subscribe: &[&str], publish: &[&str]) -> ClaimsV0 {
		ClaimsV0 {
			root: root.to_string(),
			subscribe: subscribe.iter().map(|s| s.to_string()).collect(),
			publish: publish.iter().map(|s| s.to_string()).collect(),
			..Default::default()
		}
	}

	#[test]
	fn test_authorize_path_equals_root() {
		let claims = authorize_claims("room/123", &[""], &["alice"]);
		let permissions = claims.authorize("room/123").unwrap();

		assert_eq!(permissions.subscribe, [""]);
		assert_eq!(permissions.publish, ["alice"]);
	}

	#[test]
	fn test_authorize_path_extends_root() {
		// Connecting below the root consumes the matching part of each grant.
		let claims = authorize_claims("room/123", &["bob"], &["alice"]);
		let permissions = claims.authorize("room/123/alice").unwrap();

		assert_eq!(permissions.subscribe, Vec::<String>::new());
		assert_eq!(permissions.publish, [""]);
	}

	#[test]
	fn test_authorize_path_is_parent_of_root() {
		// Connecting above the root prepends it, keeping the grants anchored.
		let claims = authorize_claims("demo", &[""], &["alice"]);
		let permissions = claims.authorize("/").unwrap();

		assert_eq!(permissions.subscribe, ["demo"]);
		assert_eq!(permissions.publish, ["demo/alice"]);
	}

	#[test]
	fn test_authorize_empty_root() {
		// A root-scoped token grants everything it lists, wherever it connects.
		let claims = authorize_claims("", &["demo"], &[]);
		let permissions = claims.authorize("demo/room").unwrap();

		assert_eq!(permissions.subscribe, [""]);
		assert_eq!(permissions.publish, Vec::<String>::new());
	}

	#[test]
	fn test_authorize_slashes_are_implicit() {
		let claims = authorize_claims("/room/123/", &["/bob/"], &[]);
		let permissions = claims.authorize("//room/123//").unwrap();

		assert_eq!(permissions.subscribe, ["bob"]);
	}

	#[test]
	fn test_authorize_respects_segment_boundaries() {
		// "foo" must not cover "foobar".
		let claims = authorize_claims("foo", &[""], &[""]);
		assert!(matches!(claims.authorize("foobar"), Err(crate::Error::RootMismatch(_))));
	}

	#[test]
	fn test_authorize_unrelated_path() {
		let claims = authorize_claims("demo", &[""], &[""]);
		assert!(matches!(claims.authorize("other"), Err(crate::Error::RootMismatch(_))));
	}

	#[test]
	fn test_authorize_no_access_at_path() {
		// The path overlaps the root, but every grant sits outside it.
		let claims = authorize_claims("", &["demo"], &[]);
		assert!(matches!(claims.authorize("other"), Err(crate::Error::NoAccess(_))));
	}

	#[test]
	fn test_authorize_returns_reduced_sets() {
		let claims = authorize_claims("demo", &["a", "a/b", "b"], &[]);
		let permissions = claims.authorize("demo").unwrap();
		assert_eq!(permissions.subscribe, ["a", "b"]);
	}

	#[test]
	fn test_deserialize_string_as_vec() {
		let json = r#"{
			"root": "test",
			"put": "single-publish",
			"get": "single-subscribe"
		}"#;

		let claims: Claims = serde_json::from_str(json).unwrap();
		let Claims::V0(v0) = claims else { panic!("expected v0") };
		assert_eq!(v0.publish, vec!["single-publish"]);
		assert_eq!(v0.subscribe, vec!["single-subscribe"]);
	}

	#[test]
	fn test_deserialize_vec_as_vec() {
		let json = r#"{
			"root": "test",
			"put": ["pub1", "pub2"],
			"get": ["sub1", "sub2"]
		}"#;

		let claims: Claims = serde_json::from_str(json).unwrap();
		let Claims::V0(v0) = claims else { panic!("expected v0") };
		assert_eq!(v0.publish, vec!["pub1", "pub2"]);
		assert_eq!(v0.subscribe, vec!["sub1", "sub2"]);
	}

	#[test]
	fn test_deserialize_mixed() {
		let json = r#"{
			"root": "test",
			"put": "single",
			"get": ["multi1", "multi2"]
		}"#;

		let claims: Claims = serde_json::from_str(json).unwrap();
		let Claims::V0(v0) = claims else { panic!("expected v0") };
		assert_eq!(v0.publish, vec!["single"]);
		assert_eq!(v0.subscribe, vec!["multi1", "multi2"]);
	}
}

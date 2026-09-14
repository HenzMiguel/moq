/**
 * The versioned payload of a token: a root, plus the grants beneath it.
 *
 * Missing `v` decodes legacy `put`/`get` prefixes as v0; `v: 1` decodes
 * `publish`/`subscribe` patterns as v1. Unknown versions and mixed fields fail
 * closed.
 *
 * @module
 */

import { Pattern, Patterns } from "@moq/pattern";
import * as z from "@zod/mini";
import * as Path from "./path.ts";

/** A `put`/`get`/`publish`/`subscribe` claim is one path or many; normalize it to a list. */
function list(claim: string | string[] | undefined): string[] {
	if (claim === undefined) return [];
	return typeof claim === "string" ? [claim] : claim;
}

/** Reduce prefix grants to a canonical union: normalized, deduplicated, sorted, covered members dropped. */
function normalizePrefixes(items: string[]): string[] {
	const normalized = [...new Set(items.map((item) => Path.normalize(item)))].sort();
	return normalized.filter((item, index) => !normalized.slice(0, index).some((kept) => Path.hasPrefix(item, kept)));
}

function validateRoot(root: string): void {
	if (root.includes("*")) throw new Error(`Invalid root: ${JSON.stringify(root)}`);
	if (root !== "") Pattern.literal(root);
}

/** Throw on an invalid v1 root or pattern list, so schemas fail closed at parse. */
function validateScopeV1(root: string, publish: string[], subscribe: string[]): void {
	validateRoot(root);
	for (const text of [...publish, ...subscribe]) Pattern.parse(text);
}

/**
 * The immutable v0 ceiling on what a key may grant, embedded in its JWK.
 *
 * `root` is optional on the wire to match the Rust `moq-token` crate, which omits it
 * when the scope sits at the top level.
 */
export const ScopeV0Schema = z
	.strictObject({
		/** The root that `put` and `get` are relative to. Defaults to the empty string. */
		root: z._default(z.string(), ""),
		/** Prefixes this key may grant to publishers, relative to `root`. */
		put: z.optional(z.array(z.string())),
		/** Prefixes this key may grant to subscribers, relative to `root`. */
		get: z.optional(z.array(z.string())),
	})
	.check(
		z.refine((data) => (data.put?.length ?? 0) > 0 || (data.get?.length ?? 0) > 0, {
			message: "Either put or get must contain at least one prefix",
		}),
	);

export type ScopeV0 = z.infer<typeof ScopeV0Schema>;

/** The immutable v1 ceiling on what a key may grant: `publish`/`subscribe` patterns relative to `root`. */
export const ScopeV1Schema = z
	.strictObject({
		/** The claim version; always 1 for pattern scopes. */
		v: z.literal(1),
		/** The root that `publish` and `subscribe` are relative to. Defaults to the empty string. */
		root: z._default(z.string(), ""),
		/** Patterns this key may grant to publishers, relative to `root`. */
		publish: z.optional(z.union([z.string(), z.array(z.string())])),
		/** Patterns this key may grant to subscribers, relative to `root`. */
		subscribe: z.optional(z.union([z.string(), z.array(z.string())])),
	})
	.check(
		z.refine((data) => list(data.publish).length > 0 || list(data.subscribe).length > 0, {
			message: "Either publish or subscribe must grant at least one pattern",
		}),
		z.refine(
			(data) => {
				try {
					validateScopeV1(data.root, list(data.publish), list(data.subscribe));
					return true;
				} catch {
					return false;
				}
			},
			{ message: "Invalid root or pattern" },
		),
	);

export type ScopeV1 = z.infer<typeof ScopeV1Schema>;

/**
 * The versioned ceiling on what a key may grant, embedded in its JWK.
 *
 * An unversioned scope is v0 with `put`/`get`; a `v: 1` scope carries
 * `publish`/`subscribe` patterns. Unknown versions and mixed fields fail closed.
 */
export const ScopeSchema = z.union([ScopeV0Schema, ScopeV1Schema]);
export type Scope = z.infer<typeof ScopeSchema>;

/**
 * The v0 JWT claims: `root` plus `put`/`get` prefixes beneath it.
 *
 * `root` is optional on the wire: a token scoped to the top-level path omits it, so
 * it defaults to the empty string to match the Rust `moq-token` crate.
 */
export const ClaimsV0Schema = z
	.strictObject({
		/** The root that `put` and `get` are relative to. Defaults to the empty string. */
		root: z._default(z.string(), ""),
		/** Paths the holder may publish to, relative to `root`. */
		put: z.optional(z.union([z.string(), z.array(z.string())])),
		/** Paths the holder may subscribe to, relative to `root`. Named `get` because `sub` is a reserved JWT claim. */
		get: z.optional(z.union([z.string(), z.array(z.string())])),
		/** Expiration time, as a unix timestamp in seconds. */
		exp: z.optional(z.number()),
		/** Issued-at time, as a unix timestamp in seconds. */
		iat: z.optional(z.number()),
	})
	.check(
		// Emptiness, not just presence: `put: []` grants nothing, and the Rust crate
		// rejects such a token as useless. Checking `!== undefined` here would mint
		// tokens that Rust then refuses to verify.
		z.refine((data) => list(data.put).length > 0 || list(data.get).length > 0, {
			message: "Either put or get must grant at least one path",
		}),
	);

export type ClaimsV0 = z.infer<typeof ClaimsV0Schema>;

/** The v1 JWT claims: `root` plus exact `publish`/`subscribe` patterns beneath it. */
export const ClaimsV1Schema = z
	.strictObject({
		/** The claim version; always 1 for pattern claims. */
		v: z.literal(1),
		/** The root that `publish` and `subscribe` are relative to. Defaults to the empty string. */
		root: z._default(z.string(), ""),
		/** Patterns the holder may publish to, relative to `root`. */
		publish: z.optional(z.union([z.string(), z.array(z.string())])),
		/** Patterns the holder may subscribe to, relative to `root`. */
		subscribe: z.optional(z.union([z.string(), z.array(z.string())])),
		/** Expiration time, as a unix timestamp in seconds. */
		exp: z.optional(z.number()),
		/** Issued-at time, as a unix timestamp in seconds. */
		iat: z.optional(z.number()),
	})
	.check(
		z.refine((data) => list(data.publish).length > 0 || list(data.subscribe).length > 0, {
			message: "Either publish or subscribe must grant at least one pattern",
		}),
		z.refine(
			(data) => {
				try {
					validateScopeV1(data.root, list(data.publish), list(data.subscribe));
					return true;
				} catch {
					return false;
				}
			},
			{ message: "Invalid root or pattern" },
		),
	);

export type ClaimsV1 = z.infer<typeof ClaimsV1Schema>;

/**
 * JWT claims structure for moq-token, versioned.
 *
 * Missing `v` reads legacy `put`/`get`; `v: 1` reads `publish`/`subscribe`.
 * Issuers stay on v0 until the M2 rollout; v1 operations already sign, verify,
 * and authorize correctly here.
 */
export const ClaimsSchema = z.union([ClaimsV0Schema, ClaimsV1Schema]);

/**
 * JWT claims structure for moq-token
 */
export type Claims = z.infer<typeof ClaimsSchema>;

/**
 * The access a {@link Claims} grants at a specific path, with every prefix or pattern
 * rebased so it is relative to that path.
 *
 * Produced by {@link authorize}. An empty string grants the path itself and everything
 * beneath it for v0; for v1 the empty pattern matches only the path and `**` matches
 * the path and everything beneath it.
 */
export interface Permissions {
	/** Paths or patterns the holder may subscribe to, relative to the authorized path. */
	subscribe: string[];
	/** Paths or patterns the holder may publish to, relative to the authorized path. */
	publish: string[];
}

/**
 * The access `claims` grants at `path`, rebased so each returned prefix or pattern is
 * relative to `path`.
 *
 * V0 grants are prefixes: `path` and `claims.root` must overlap, in either direction,
 * and matching is segment-aware. V1 grants are exact patterns: each pattern is placed
 * beneath the root, then rebased at `path`, which is set-valued (a globstar pattern
 * rebased where its tail already matched yields both the empty pattern and itself).
 *
 * Throws when the two don't overlap, and when they do but every grant falls outside
 * `path`.
 *
 * This is authorization only. Verify the signature first with {@link verify}, which is
 * where expiry is enforced.
 *
 * @public
 */
export function authorize(claims: Claims, path: string): Permissions {
	if ("v" in claims && claims.v === 1) return authorizeV1(claims, path);
	return authorizeV0(claims, path);
}

function authorizeV0(claims: ClaimsV0, path: string): Permissions {
	const target = Path.normalize(path);
	const root = Path.normalize(claims.root);

	// Exactly one of these is non-empty: `suffix` is how far the path reaches past
	// the root, `prefix` is how far the root reaches past the path.
	let suffix: string;
	let prefix: string;

	const beyondRoot = Path.stripPrefix(target, root);
	const beyondPath = Path.stripPrefix(root, target);
	if (beyondRoot !== undefined) {
		[suffix, prefix] = [beyondRoot, ""];
	} else if (beyondPath !== undefined) {
		[suffix, prefix] = ["", beyondPath];
	} else {
		throw new Error(`path "${target}" does not overlap the token root "${root}"`);
	}

	const scope = (claim: string | string[] | undefined): string[] => {
		const scoped: string[] = [];
		for (const granted of list(claim)) {
			const full = Path.join(prefix, Path.normalize(granted));

			const remaining = Path.stripPrefix(full, suffix);
			if (remaining !== undefined) {
				// The grant covers the path; keep what's left below it.
				scoped.push(remaining);
			} else if (Path.hasPrefix(suffix, full)) {
				// The grant stops short of the path but still contains it, so
				// everything below the path is granted.
				scoped.push("");
			}
		}
		return normalizePrefixes(scoped);
	};

	const permissions: Permissions = { subscribe: scope(claims.get), publish: scope(claims.put) };
	if (permissions.subscribe.length === 0 && permissions.publish.length === 0) {
		throw new Error(`token grants no access to path "${target}"`);
	}

	return permissions;
}

function authorizeV1(claims: ClaimsV1, path: string): Permissions {
	validateRoot(claims.root);
	const target = Path.normalize(path);
	const root = Path.normalize(claims.root);

	const overlaps = Path.stripPrefix(target, root) !== undefined || Path.stripPrefix(root, target) !== undefined;
	if (!overlaps) throw new Error(`path "${target}" does not overlap the token root "${root}"`);

	const rebase = (claim: string | string[] | undefined): string[] => {
		const out = new Patterns();
		for (const text of list(claim)) {
			const absolute = Pattern.parse(text).rooted(claims.root);
			for (const residual of absolute.rebase(path)) out.insert(residual);
		}
		return out.toArray().map((pattern) => pattern.text);
	};

	const permissions: Permissions = { subscribe: rebase(claims.subscribe), publish: rebase(claims.publish) };
	if (permissions.subscribe.length === 0 && permissions.publish.length === 0) {
		throw new Error(`token grants no access to path "${target}"`);
	}
	return permissions;
}

/**
 * Whether every path or pattern `claims` grants is covered by `scope`, per role.
 *
 * V0 compares prefixes resolved against their own roots; v1 requires every claims
 * pattern, placed beneath its root, to be contained in some scope pattern beneath
 * its root. Cross-version pairs never allow. A publish-only scope never authorizes
 * a subscribe grant.
 *
 * Must stay in lockstep with `Scope::allows` in the Rust `moq-token` crate, which
 * checks the same keys.
 */
export function scopeAllows(scope: Scope, claims: Claims): boolean {
	const scopeIsV1 = "v" in scope && scope.v === 1;
	const claimsIsV1 = "v" in claims && claims.v === 1;
	if (scopeIsV1 !== claimsIsV1) return false;
	if (scopeIsV1 && claimsIsV1) {
		validateRoot(scope.root);
		validateRoot(claims.root);
		const covers = (
			granted: string | string[] | undefined,
			scopeRoot: string,
			requested: string | string[] | undefined,
			claimsRoot: string,
		): boolean => {
			const scopeAbsolute = new Patterns(list(granted).map((text) => Pattern.parse(text).rooted(scopeRoot)));
			const requestedAbsolute = list(requested).map((text) => Pattern.parse(text).rooted(claimsRoot));
			return requestedAbsolute.every((pattern) => scopeAbsolute.contains(pattern));
		};
		return (
			covers(scope.publish, scope.root, claims.publish, claims.root) &&
			covers(scope.subscribe, scope.root, claims.subscribe, claims.root)
		);
	}
	return (
		covers((scope as ScopeV0).root, (scope as ScopeV0).put ?? [], claims.root, list((claims as ClaimsV0).put)) &&
		covers((scope as ScopeV0).root, (scope as ScopeV0).get ?? [], claims.root, list((claims as ClaimsV0).get))
	);
}

/**
 * Whether every `requested` path (relative to `claimsRoot`) sits beneath some
 * `granted` prefix (relative to `scopeRoot`).
 *
 * An empty `granted` denies everything, which is what makes a publish-only scope
 * reject subscribe grants rather than ignoring them.
 */
function covers(scopeRoot: string, granted: string[], claimsRoot: string, requested: string[]): boolean {
	const absolute = (root: string, relative: string) => Path.join(Path.normalize(root), Path.normalize(relative));

	return requested.every((request) => {
		const path = absolute(claimsRoot, request);
		return granted.some((grant) => Path.hasPrefix(path, absolute(scopeRoot, grant)));
	});
}

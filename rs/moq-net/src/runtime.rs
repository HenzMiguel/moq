//! Timer integration for session and origin drivers.
//!
//! [`crate::Client::connect`] and [`crate::Server::accept`] take [`Timers`] and
//! return a [`crate::Driver`] for the caller to poll or spawn. This crate never
//! spawns tasks. Timer providers supply a clock and re-armable registrations,
//! without choosing a transport or executor.
//!
//! `Test` (behind `test-runtime`) provides a virtual clock that advances only
//! when the test requests it.

use std::task::Poll;

/// The instant type used for deadlines and annotations.
///
/// [`std::time::Instant`] on native. The browser has no monotonic std clock, so
/// wasm substitutes an equivalent backed by `performance.now()`.
#[cfg(not(target_family = "wasm"))]
pub type Instant = std::time::Instant;
/// The instant type used for deadlines and annotations (wasm shim).
#[cfg(target_family = "wasm")]
pub type Instant = web_async::time::Instant;

/// A single re-armable timer registration, produced by [`Timers::timer`].
///
/// This is the primitive [`Deadline`] wraps; implement it, use `Deadline`.
/// Arming is synchronous and in-memory: no I/O submission, no async
/// cancellation. Implementations typically keep a slot in the runtime's timer
/// wheel (or wrap a tokio `Sleep`).
pub trait Timer {
	/// Arm, re-arm, or disarm (`None`) the timer.
	///
	/// Re-arming an elapsed timer for a later instant makes it pend again;
	/// re-arming for an instant already in the past leaves it elapsed.
	fn set(&mut self, at: Option<Instant>);

	/// Ready once the armed instant has passed, registering `waiter` otherwise.
	///
	/// Fused: an elapsed timer keeps reporting `Ready` until re-armed. A
	/// disarmed timer never fires.
	fn poll(&mut self, waiter: &kio::Waiter) -> Poll<()>;
}

/// The timer half of a runtime: mint [`Timer`]s and read the clock they follow.
///
/// Shared by session and origin drivers, independently of their transport or
/// executor. Clones must be cheap (a ZST or a reference count).
pub trait Timers: Clone {
	/// The timer registration this runtime hands out.
	type Timer: Timer;

	/// A new, disarmed timer.
	fn timer(&self) -> Self::Timer;

	/// The current instant.
	///
	/// Defaults to the real clock, which every production runtime should keep.
	/// It exists so `Test` can substitute a virtual clock: code that arms
	/// relative deadlines (`now + interval`) or compares against armed instants
	/// must read *this* clock, or a test that advances virtual time leaves those
	/// instants in the past. Pure annotations (activity stamps, logs) may read
	/// [`Instant::now`] directly.
	fn now(&self) -> Instant {
		Instant::now()
	}
}

/// A boxed [`Timer`], minted by [`AnyTimers`].
pub(crate) struct BoxTimer(MaybeSendTimer);

#[cfg(not(target_family = "wasm"))]
type MaybeSendTimer = Box<dyn Timer + Send>;
#[cfg(target_family = "wasm")]
type MaybeSendTimer = Box<dyn Timer>;

impl Timer for BoxTimer {
	fn set(&mut self, at: Option<Instant>) {
		self.0.set(at);
	}

	fn poll(&mut self, waiter: &kio::Waiter) -> Poll<()> {
		self.0.poll(waiter)
	}
}

/// A type-erased [`Timers`], for shared state that cannot be generic over the
/// runtime (the origin's task machinery). One allocation per minted timer.
pub(crate) struct AnyTimers(MaybeSendErased);

#[cfg(not(target_family = "wasm"))]
type MaybeSendErased = std::sync::Arc<dyn ErasedTimers + Send + Sync>;
#[cfg(target_family = "wasm")]
type MaybeSendErased = std::sync::Arc<dyn ErasedTimers>;

trait ErasedTimers {
	fn now(&self) -> Instant;
	fn timer(&self) -> BoxTimer;
}

#[cfg(not(target_family = "wasm"))]
impl<T> ErasedTimers for T
where
	T: Timers,
	T::Timer: Send + 'static,
{
	fn now(&self) -> Instant {
		Timers::now(self)
	}

	fn timer(&self) -> BoxTimer {
		BoxTimer(Box::new(Timers::timer(self)))
	}
}

#[cfg(target_family = "wasm")]
impl<T> ErasedTimers for T
where
	T: Timers,
	T::Timer: 'static,
{
	fn now(&self) -> Instant {
		Timers::now(self)
	}

	fn timer(&self) -> BoxTimer {
		BoxTimer(Box::new(Timers::timer(self)))
	}
}

impl AnyTimers {
	#[cfg(not(target_family = "wasm"))]
	pub(crate) fn new<T>(timers: T) -> Self
	where
		T: Timers + Send + Sync + 'static,
		T::Timer: Send + 'static,
	{
		Self(std::sync::Arc::new(timers))
	}

	#[cfg(target_family = "wasm")]
	pub(crate) fn new<T>(timers: T) -> Self
	where
		T: Timers + 'static,
		T::Timer: 'static,
	{
		Self(std::sync::Arc::new(timers))
	}
}

impl Clone for AnyTimers {
	fn clone(&self) -> Self {
		Self(self.0.clone())
	}
}

impl Timers for AnyTimers {
	type Timer = BoxTimer;

	fn timer(&self) -> Self::Timer {
		self.0.timer()
	}

	fn now(&self) -> Instant {
		self.0.now()
	}
}

/// A late-bound [`AnyTimers`]: shared state created before the runtime is known
/// (the origin's, at [`crate::origin::Producer::new`]) holds this, and
/// [`crate::origin::Driver::run`] fills it in.
#[derive(Clone, Default)]
pub(crate) struct TimersSlot(std::sync::Arc<std::sync::OnceLock<AnyTimers>>);

impl TimersSlot {
	/// Install the timers, ignoring a second install (clones share one slot).
	pub(crate) fn install(&self, timers: AnyTimers) {
		let _ = self.0.set(timers);
	}

	/// The installed timers. Panics before install, so only call from work that
	/// the driver polls (it installs before it can poll anything).
	pub(crate) fn get(&self) -> AnyTimers {
		self.0.get().expect("origin driver is not running").clone()
	}
}

/// A wall-clock deadline: the ergonomic layer over [`Timer`].
///
/// Arm it with an [`Instant`], poll it from a `poll_*` function, re-arm or
/// disarm as the deadline moves. Re-setting the instant it already holds does
/// nothing, so a poll loop can recompute its deadline every turn without
/// restarting the countdown.
pub struct Deadline<R: Timers> {
	at: Option<Instant>,
	timer: R::Timer,
}

impl<R: Timers> Deadline<R> {
	/// A disarmed deadline, which never fires until [`set`](Self::set) arms it.
	pub fn new(runtime: &R) -> Self {
		Self {
			at: None,
			timer: runtime.timer(),
		}
	}

	/// A deadline armed for `at`.
	pub fn at(runtime: &R, at: Instant) -> Self {
		let mut deadline = Self::new(runtime);
		deadline.set(Some(at));
		deadline
	}

	/// A deadline armed for `duration` past the runtime's [`now`](Timers::now).
	///
	/// A duration the clock cannot represent (e.g. [`std::time::Duration::MAX`])
	/// leaves the deadline disarmed, so it never fires rather than panicking on
	/// the overflow.
	pub fn after(runtime: &R, duration: std::time::Duration) -> Self {
		let mut deadline = Self::new(runtime);
		deadline.set(runtime.now().checked_add(duration));
		deadline
	}

	/// Arm, re-arm, or disarm (`None`) the deadline.
	pub fn set(&mut self, at: Option<Instant>) {
		if self.at == at {
			return;
		}
		self.at = at;
		self.timer.set(at);
	}

	/// The instant this fires at, or `None` while disarmed.
	pub fn deadline(&self) -> Option<Instant> {
		self.at
	}

	/// Poll the deadline, registering `waiter` so the poll re-fires once it
	/// elapses. `Ready` once the instant has passed, `Pending` before then and
	/// while disarmed.
	pub fn poll(&mut self, waiter: &kio::Waiter) -> Poll<()> {
		if self.at.is_none() {
			return Poll::Pending;
		}
		self.timer.poll(waiter)
	}

	/// Wait for the deadline to elapse. Parks forever while disarmed.
	pub async fn wait(&mut self) {
		kio::wait(|waiter| self.poll(waiter)).await
	}
}

impl<R: Timers> std::fmt::Debug for Deadline<R> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Deadline").field("at", &self.at).finish()
	}
}

#[cfg(feature = "test-runtime")]
mod test;
#[cfg(feature = "test-runtime")]
pub use test::Test;

/// A tokio-backed runtime for this crate's own unit tests, so the existing
/// `tokio::time::pause`/`advance` tests keep their semantics: `now` reads
/// tokio's (pausable) clock and timers are tokio sleeps, which paused tests
/// auto-advance. Production code uses `moq_tokio::runtime::Runtime` instead; this one is
/// compiled only into the test harness (integration tests carry their own copy
/// in `tests/support`).
#[cfg(all(test, not(target_family = "wasm")))]
pub(crate) mod tokio_test {
	use std::{pin::Pin, task::Poll};

	use super::{Instant, Timer};

	#[derive(Clone, Default)]
	pub(crate) struct Tokio;

	impl Tokio {
		pub fn new() -> Self {
			Self
		}
	}

	impl super::Timers for Tokio {
		type Timer = TokioTimer;

		fn timer(&self) -> Self::Timer {
			TokioTimer { at: None, sleep: None }
		}

		fn now(&self) -> Instant {
			tokio::time::Instant::now().into_std()
		}
	}

	pub(crate) struct TokioTimer {
		at: Option<Instant>,
		// Allocated on the first poll after arming, then re-armed in place via
		// `Sleep::reset`. Construction is deferred because it panics without a
		// live tokio time driver, and only the poll is guaranteed to run inside
		// the runtime.
		sleep: Option<Pin<Box<tokio::time::Sleep>>>,
	}

	impl Timer for TokioTimer {
		fn set(&mut self, at: Option<Instant>) {
			self.at = at;
			// Reuse the allocation when there is one; `reset` also clears
			// `is_elapsed`.
			if let (Some(at), Some(sleep)) = (at, &mut self.sleep) {
				sleep.as_mut().reset(tokio::time::Instant::from_std(at));
			}
		}

		fn poll(&mut self, waiter: &kio::Waiter) -> Poll<()> {
			let Some(at) = self.at else { return Poll::Pending };
			let sleep = self
				.sleep
				.get_or_insert_with(|| Box::pin(tokio::time::sleep_until(tokio::time::Instant::from_std(at))));
			if sleep.is_elapsed() {
				return Poll::Ready(());
			}
			waiter.poll_future(sleep.as_mut())
		}
	}
}

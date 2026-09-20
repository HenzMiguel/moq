//! Drive a session on the caller's executor.

use std::task::Poll;

use crate::{Error, runtime::Timers};

/// The future driving a [`crate::Session`]'s protocol state.
///
/// Returned by [`crate::Client::connect`] and [`crate::Server::accept`]. Poll it,
/// either as a [`Future`] or by stepping [`poll`](Self::poll) from a
/// [`kio`]-style poll function. It resolves when the session ends, and keeps
/// returning that same result if polled again.
///
/// It holds no session handle: dropping the last [`crate::Session`] requests
/// closure on the next poll. Dropping the driver cancels the session, and
/// [`crate::Session::closed`] resolves with [`Error::Cancel`]. Its `Send`-ness
/// follows its transport and timer provider.
#[must_use = "the session makes no progress unless its driver is polled"]
pub struct Driver<S: crate::transport::poll::Session, R: Timers> {
	state: State<S, R>,
	// Retains the waiter across `Future` polls so its kio registrations stay
	// live. Kept out of `State` so the borrow `hold` hands back doesn't
	// collide with the `&mut` that polling the state needs.
	park: kio::Park,
}

/// The protocol half of a machine, one variant per negotiated wire protocol.
///
/// The lite driver is a named machine, so the machine's `Send`-ness follows
/// the transport (a pinned `!Send` transport yields a `!Send` machine that
/// stays on its thread). The ietf driver is still a boxed future; the box
/// demands `Send` on native, which is why the ietf path requires a
/// [`Boxable`](crate::transport::poll::Boxable) transport until it too becomes
/// a named machine.
pub(crate) enum Protocol<S: crate::transport::poll::Session, R: Timers> {
	/// Boxed for size only: a concrete box, so `Send` stays inferred.
	Lite(Box<crate::lite::Driver<S, R>>),
	Ietf(crate::util::MaybeSendBox<'static, Result<(), Error>>),
}

/// Everything the machine polls, split from the park so the two borrow
/// disjointly.
pub(crate) struct State<S: crate::transport::poll::Session, R: Timers> {
	pub(crate) protocol: Protocol<S, R>,
	// The session supervisor, polled alongside the protocol: it executes the
	// handles' close requests, publishes the transport's terminal error, and
	// samples stats. It finishes once the transport reports closed, and the
	// machine is not done until it has: the protocol's terminal transport close
	// is what `Session::closed` observes, so resolving before it is published
	// would leave waiters parked on a machine nobody polls again. `None` once
	// finished, since a completed machine must not be polled again.
	pub(crate) supervisor: Option<crate::session::Supervisor<S, R>>,
	// Cached so a poll after completion doesn't re-poll a finished protocol.
	pub(crate) result: Option<Result<(), Error>>,
}

impl<S: crate::transport::poll::Session, R: Timers> Driver<S, R> {
	pub(crate) fn new(state: State<S, R>) -> Self {
		Self {
			state,
			park: kio::Park::default(),
		}
	}

	/// Drive the protocol one step, registering `waiter` for the next wakeup.
	///
	/// The `poll_*` counterpart of `.await`ing the machine, for runtimes
	/// composing it into their own [`kio`]-style poll loops.
	pub fn poll(&mut self, waiter: &kio::Waiter) -> Poll<Result<(), Error>> {
		self.state.poll(waiter)
	}
}

impl<S: crate::transport::poll::Session, R: Timers> Protocol<S, R> {
	fn poll(&mut self, waiter: &kio::Waiter) -> Poll<Result<(), Error>> {
		match self {
			Self::Lite(driver) => driver.poll(waiter),
			Self::Ietf(driver) => waiter.poll_future(driver.as_mut()),
		}
	}
}

impl<S: crate::transport::poll::Session, R: Timers> State<S, R> {
	fn poll(&mut self, waiter: &kio::Waiter) -> Poll<Result<(), Error>> {
		if let Some(supervisor) = &mut self.supervisor
			&& supervisor.poll(waiter).is_ready()
		{
			self.supervisor = None;
		}

		if self.result.is_none()
			&& let Poll::Ready(result) = self.protocol.poll(waiter)
		{
			self.result = Some(result);
			// The protocol's last act was closing the transport, which wakes the
			// supervisor's close watch; poll it now instead of waiting a turn.
			if let Some(supervisor) = &mut self.supervisor
				&& supervisor.poll(waiter).is_ready()
			{
				self.supervisor = None;
			}
		}

		match (&self.result, &self.supervisor) {
			(Some(result), None) => Poll::Ready(result.clone()),
			_ => Poll::Pending,
		}
	}
}

// Every part of the machine is a poll machine driven through `&mut` (the one
// pinned piece, the boxed ietf future, is `Unpin` itself), so nothing relies
// on address stability and the machine may move freely between polls.
impl<S: crate::transport::poll::Session, R: Timers> Unpin for Driver<S, R> {}

impl<S: crate::transport::poll::Session, R: Timers> Future for Driver<S, R> {
	type Output = Result<(), Error>;

	fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
		let this = &mut *self;
		// Disjoint field borrows: `hold` borrows the park for as long as the
		// waiter lives, while the state is polled through its own `&mut`.
		let waiter = this.park.hold(cx);
		this.state.poll(waiter)
	}
}

impl<S: crate::transport::poll::Session, R: Timers> std::fmt::Debug for Driver<S, R> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Driver")
			.field("done", &self.state.result.is_some())
			.finish()
	}
}

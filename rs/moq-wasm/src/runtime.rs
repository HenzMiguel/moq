//! Browser-backed timers for MoQ drivers.

use std::{pin::Pin, task::Poll};

/// Supplies the browser clock and timers to MoQ drivers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Runtime;

impl moq_net::Timers for Runtime {
	type Timer = Timer;

	fn timer(&self) -> Self::Timer {
		Timer { at: None, sleep: None }
	}
}

/// A `web_async::time::Sleep` driven through the [`moq_net::runtime::Timer`]
/// contract.
pub struct Timer {
	at: Option<moq_net::runtime::Instant>,
	// Allocated on the first poll after arming, then re-armed in place.
	sleep: Option<Pin<Box<web_async::time::Sleep>>>,
}

impl moq_net::runtime::Timer for Timer {
	fn set(&mut self, at: Option<moq_net::runtime::Instant>) {
		self.at = at;
		// Reuse the allocation when there is one; `reset` also clears the
		// elapsed state.
		if let (Some(at), Some(sleep)) = (at, &mut self.sleep) {
			sleep.as_mut().reset(at);
		}
	}

	fn poll(&mut self, waiter: &moq_net::kio::Waiter) -> Poll<()> {
		let Some(at) = self.at else { return Poll::Pending };
		let sleep = self
			.sleep
			.get_or_insert_with(|| Box::pin(web_async::time::sleep_until(at)));
		if sleep.is_elapsed() {
			return Poll::Ready(());
		}
		waiter.poll_future(sleep.as_mut())
	}
}

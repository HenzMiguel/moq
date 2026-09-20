//! Tokio-backed timers for MoQ drivers.

use std::{pin::Pin, task::Poll};

/// Supplies Tokio's clock and timers to MoQ drivers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Runtime;

impl Runtime {
	/// A handle to Tokio's clock and timers.
	pub fn new() -> Self {
		Self
	}
}

impl moq_net::Timers for Runtime {
	type Timer = Timer;

	fn timer(&self) -> Self::Timer {
		Timer { at: None, sleep: None }
	}

	fn now(&self) -> moq_net::runtime::Instant {
		// Tokio's clock, so `tokio::time::pause` tests stay coherent; it reads
		// the real clock outside a paused runtime.
		tokio::time::Instant::now().into_std()
	}
}

/// A tokio sleep driven through the [`moq_net::runtime::Timer`] contract.
pub struct Timer {
	at: Option<moq_net::runtime::Instant>,
	// Allocated on the first poll after arming, then re-armed in place via
	// `Sleep::reset`. Construction is deferred because it panics without a live
	// tokio time driver, and only the poll is guaranteed to run inside the
	// runtime.
	sleep: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl moq_net::runtime::Timer for Timer {
	fn set(&mut self, at: Option<moq_net::runtime::Instant>) {
		self.at = at;
		// Reuse the allocation when there is one; `reset` also clears
		// `is_elapsed`.
		if let (Some(at), Some(sleep)) = (at, &mut self.sleep) {
			sleep.as_mut().reset(tokio::time::Instant::from_std(at));
		}
	}

	fn poll(&mut self, waiter: &moq_net::kio::Waiter) -> Poll<()> {
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

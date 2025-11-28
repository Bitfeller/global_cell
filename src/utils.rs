// Miscellaneous utility functions for Cells.
use futures_util::future::poll_fn;
use std::task::Poll;

/// Offers the current thread's remaining time slice to other threads.
/// Generalized to any async runtime.
pub(crate) async fn yield_now() {
    poll_fn(|cx| {
        cx.waker().wake_by_ref();
        Poll::<()>::Pending
    }).await;
}
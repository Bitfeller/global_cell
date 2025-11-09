// Miscellaneous utility functions for Cells.
use std::future::Future;

/// Offers the current thread's remaining time slice to other threads.
/// Generalized to any async runtime.
#[cfg(not(any(feature = "tokio", feature = "async-std")))]
pub(crate) fn yield_now() -> impl Future<Output = ()> {
    struct YieldNow(bool);
    impl Future for YieldNow {
        type Output = ();

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            if self.0 {
                std::task::Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        }
    }

    YieldNow(false)
}
#[cfg(feature = "tokio")]
pub(crate) fn yield_now() -> impl Future<Output = ()> {
    tokio::task::yield_now()
}
#[cfg(feature = "async-std")]
pub(crate) fn yield_now() -> impl Future<Output = ()> {
    async_std::task::yield_now()
}
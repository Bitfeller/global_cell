// use tokio::sync::{ RwLock };
use std::{cell::UnsafeCell, hint::likely, mem::MaybeUninit, sync::atomic::{ AtomicU8, Ordering }};
use parking_lot::RwLock;
/// RawCell is a low-level primitive to offer performant access and interior mutability,
/// while being statically assignable and shareable across threads (send + sync).
/// 
/// RawCell is, by nature, unsafe; protection against race conditions is
/// undefined, providing unified write/read access to all threads that have access to it.
/// 
/// RawCell is async-aware and based on tokio primitives.
/// 
/// # PERFORMANCE
/// - AtomicU8: lock-free state management; fast
/// - UnsafeCell: zero-cost interior mutability; fast
/// - MaybeUninit: zero-cost uninitialized memory; fast
/// - RwLock: multiple concurrent readers, single writer; fast reads, moderate writes; SIGNIFICANT OVERHEAD
/// 
/// RwLock is the most significant performance bottleneck in RawCell. A potentially better alternative
/// if the cell will be used in mostly write-heavy contexts, a single-lock Mutex would be better.
/// 
/// # INITIALIZATION
/// By default, the cell itself is not initialized; RawCell assumes a lazy implementation unless its
/// init field is explicitly called by the user at some point in time.
/// 
/// Even when initialized, the inner value is not necessarily guarenteed to be set; that is,
/// it may be "empty" or essentially set to None. In that case, reads/writes to the inner value
/// will return an Err.

#[repr(C, align(128))]
pub struct RawCell<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<RwLock<T>>>,
}

unsafe impl<T: Send> Send for RawCell<T> {}
unsafe impl<T: Sync> Sync for RawCell<T> {}

impl<T: Send + Sync> RawCell<T> {
    const UNINIT: u8 = 0;
    const INITING: u8 = 1;
    const INITED: u8 = 2;
    const MAX_SPIN: u8 = 8;

    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(Self::UNINIT),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    #[inline(always)]
    pub fn default() -> Self
    where
        T: Default,
    {
        let cell = Self::new();
        unsafe {
            (*cell.value.get()).as_mut_ptr().write(RwLock::new(T::default()));
        }
        cell.state.store(Self::INITED, Ordering::Release);
        cell
    }

    #[inline(always)]
    pub fn is_initialized(&self) -> bool {
        self.state.load(Ordering::Acquire) == Self::INITED
    }

    /// Blocking init, optimized
    #[inline(always)]
    pub fn init<F>(&self, init_fn: F)
    where 
        F: FnOnce() -> T,
    {
        // Fast path: check if already initialized
        if likely(self.is_initialized()) {
            return;
        }

        let mut step = 0;
        loop {
            let state = self.state.load(Ordering::Acquire);

            match state {
                Self::INITED => return,
                Self::UNINIT => {
                    if self.state.compare_exchange(
                        Self::UNINIT,
                        Self::INITING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ).is_ok() {
                        let value = init_fn();

                        unsafe {
                            (*self.value.get()).as_mut_ptr().write(RwLock::new(value));
                        }
                        self.state.store(Self::INITED, Ordering::Release);
                        return;
                    }
                }
                Self::INITING => {
                    if step > Self::MAX_SPIN {
                        std::thread::yield_now();
                    } else {
                        for _ in 0..(1 << step) {
                            std::hint::spin_loop();
                        }
                    }
                    return;
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            }

            step += 1;
        }
    }

    /// Init, optimized
    #[inline(always)]
    pub async fn init_async<F, Fut>(&self, init_fn: F)
    where 
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        // Fast path: check if already initialized
        if likely(self.is_initialized()) {
            return;
        }

        let mut step = 0;
        loop {
            let state = self.state.load(Ordering::Acquire);

            match state {
                Self::INITED => return,
                Self::UNINIT => {
                    if self.state.compare_exchange(
                        Self::UNINIT,
                        Self::INITING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ).is_ok() {
                        let value = init_fn().await;

                        unsafe {
                            (*self.value.get()).as_mut_ptr().write(RwLock::new(value));
                        }
                        self.state.store(Self::INITED, Ordering::Release);
                        return;
                    }
                }
                Self::INITING => {
                    if step > Self::MAX_SPIN {
                        tokio::task::yield_now().await;
                    } else {
                        for _ in 0..(1 << step) {
                            std::hint::spin_loop();
                        }
                    }
                    return;
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            }

            step += 1;
        }
    }

    /// Fetches the inner value, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn inner(&self) -> &RwLock<T> {
        debug_assert!(self.is_initialized(), "RawCell is not initialized");
        (*self.value.get()).assume_init_ref()
    }

    /// Fetches the inner value mutably, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn inner_mut(&self) -> &mut RwLock<T> {
        debug_assert!(self.is_initialized(), "RawCell is not initialized");
        (*self.value.get()).assume_init_mut()
    }
}
use std::{
    cell::UnsafeCell, 
    hint::{likely, unlikely}, 
    mem::MaybeUninit, 
    panic::AssertUnwindSafe, 
    sync::{atomic::{AtomicBool, AtomicU8, Ordering}},
    future::Future
};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard, Mutex};
use futures_util::future::FutureExt;
use crate::{traits::CellTrait, utils::yield_now};

/// A guard that resets the state of the RawCell if dropped before initialization completes.
/// 
/// Two key assumptions are made with this guard:
/// - The lock is only dropped if initialization fails or panics, in which case the destructor will be ran automatically.
///     - If initialization succeeds, the guard can be "forgotten", or the guard can be dropped before
///       initialization states are updated. For performance cells, the guard should be forgotten.
/// - The intended reset state for all failures is UNINIT, being 0.
#[repr(transparent)]
struct ResetOnDrop<'a> {
    state: &'a AtomicU8,
}
impl<'a> Drop for ResetOnDrop<'a> {
    fn drop(&mut self) {
        self.state.store(0, Ordering::Release);
    }
}

/// # RawCell
/// RawCell is a low-level primitive to offer performant access and interior mutability,
/// while being statically assignable and shareable across threads (send + sync).
/// 
/// RawCell is, by nature, unsafe; protection against race conditions is
/// undefined, providing unified write/read access to all threads that have access to it.
/// 
/// RawCell is async-aware and is tokio-compatible.
/// 
/// ## Performance
/// - AtomicU8: lock-free state management; fast
/// - UnsafeCell: zero-cost interior mutability; fast
/// - MaybeUninit: zero-cost uninitialized memory; fast
/// - RwLock: (parking lot) multiple concurrent readers, single writer; fast reads, moderate writes; SIGNIFICANT OVERHEAD
/// 
/// RwLock is the most significant performance bottleneck in RawCell. A potentially better alternative
/// if the cell will be used in mostly write-heavy contexts, a single-lock Mutex would be better.
/// 
/// ## Initialization
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

impl<T> RawCell<T> {
    const UNINIT: u8 = 0;
    const INITING: u8 = 1;
    const INITED: u8 = 2;

    const MAX_SPIN: u8 = 8;
}
impl<T: Send + Sync> const CellTrait<T> for RawCell<T> {
    #[inline(always)]
    fn new() -> Self {
        Self::new()
    }
}
/// Require T: Send + Sync for concurrent access operations. Why?
/// - RwLock prefers T: Send + Sync to be Send + Sync itself.
/// - RawCell is designed for concurrent access; without Send + Sync, it would be
///   unsafe to share across threads.
/// - Enforcing Send + Sync at the type level prevents misuse and potential data races.
/// 
/// RawCell itself does not require T: Send + Sync, as the caller may
/// decide to manually implement uses for RawCell and require an auto-destructor, which is already implemented.
/// That's just not worth the hassle usually, so just use Send + Sync.
impl<T: Send + Sync> RawCell<T> {
    /// Creates a new, uninitialized RawCell.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(Self::UNINIT),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Creates a new, initialized RawCell with the default value of T.
    /// RawCell will be initialized upon creation.
    #[inline(always)]
    pub fn default() -> Self
    where
        T: Default,
    {
        Self {
            state: AtomicU8::new(Self::INITED),
            value: UnsafeCell::new(MaybeUninit::new(RwLock::new(T::default()))),
        }
    }

    /// Checks if the RawCell has been initialized.
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
                        // Safe to use ResetOnDrop: init_fn panics causes destructor,
                        // UNINIT state = 0, guard forgotten on success
                        let guard = ResetOnDrop { state: &self.state };
                        let value = init_fn();

                        // If init_fn panicked, the guard would have dropped and set the cell back to UNINIT;
                        // if we're here, init_fn succeeded.

                        unsafe {
                            (*self.value.get()).as_mut_ptr().write(RwLock::new(value));
                        }
                        std::mem::forget(guard);
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
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            }

            step += 1;
        }
    }

    /// Async init, optimized
    #[inline(always)]
    pub async fn init_async<F, Fut>(&self, init_fn: F)
    where 
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
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
                        // Safe to use ResetOnDrop: init_fn panics causes destructor,
                        // UNINIT state = 0, guard forgotten on success
                        let guard = ResetOnDrop { state: &self.state };

                        // Here, instead of letting the future poll itself, we manually catch
                        // the unwind to ensure the guard is dropped correctly.
                        let value = AssertUnwindSafe(init_fn())
                            .catch_unwind()
                            .await;

                        if let Ok(value) = value {
                            unsafe {
                                (*self.value.get()).as_mut_ptr().write(RwLock::new(value));
                            }
                            std::mem::forget(guard);
                            self.state.store(Self::INITED, Ordering::Release);
                        } else {
                            std::panic::resume_unwind(value.err().unwrap());
                        }
                        return;
                    }
                }
                Self::INITING => {
                    if step > Self::MAX_SPIN {
                        yield_now().await;
                    } else {
                        for _ in 0..(1 << step) {
                            std::hint::spin_loop();
                        }
                    }
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

    /// Fetches a read lock to the inner value, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn read(&self) -> RwLockReadGuard<'_, T> {
        debug_assert!(self.is_initialized(), "RawCell is not initialized");
        (*self.value.get()).assume_init_ref().read()
    }

    /// Fetches a write lock to the inner value, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn write(&self) -> RwLockWriteGuard<'_, T> {
        debug_assert!(self.is_initialized(), "RawCell is not initialized");
        (*self.value.get()).assume_init_ref().write()
    }
}
impl<T> Drop for RawCell<T> {
    fn drop(&mut self) {
        // We can't use Send + Sync related bounds here.
        // Automatically determine initialization and dropping.
        if self.state.load(Ordering::Acquire) == Self::INITED {
            unsafe {
                std::ptr::drop_in_place(
                    (*self.value.get()).assume_init_mut()
                )
            }
        }
    }
}

/// # RawMutexCell
/// RawMutexCell is a low-level primitive based on RawCell, but geared towards
/// write-heavy access patterns. Instead of RwLock, the Cell utilizes a Mutex internally
/// for unified read/write access.
/// 
/// Refer to RawCell documentation for more information.
/// 
/// This does come with downsides, that being:
/// - slower read access
/// - no concurrent access
/// - easier deadlocks
#[repr(C, align(128))]
pub struct RawMutexCell<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<Mutex<T>>>,
}
unsafe impl<T: Send> Send for RawMutexCell<T> {}
unsafe impl<T: Sync> Sync for RawMutexCell<T> {}
impl<T> const CellTrait<T> for RawMutexCell<T> {
    #[inline(always)]
    fn new() -> Self {
        Self::new()
    }
}
/// RawMutexCell doesn't require T: Send + Sync at the type level, since all reads/writes
/// are parallel. For async initializations, RawMutexCell automatically uses a
/// `state` to determine whether it is safe to update the Mutex.
/// 
/// As for async/multiple access to the Mutex itself, the caller must prevent UB themselves;
/// optionally, T can implement Send + Sync.
impl<T> RawMutexCell<T> {
    const UNINIT: u8 = 0;
    const INITING: u8 = 1;
    const INITED: u8 = 2;

    const MAX_SPIN: u8 = 8;

    /// Creates a new, uninitialized RawMutexCell.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(Self::UNINIT),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Creates a new, initialized RawMutexCell with the default value of T.
    /// RawMutexCell will be initialized upon creation.
    #[inline(always)]
    pub fn default() -> Self
    where
        T: Default,
    {
        Self {
            state: AtomicU8::new(Self::INITED),
            value: UnsafeCell::new(MaybeUninit::new(Mutex::new(T::default()))),
        }
    }

    /// Checks if the RawMutexCell has been initialized.
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
        let mut step = 0;
        loop {
            match self.state.compare_exchange(
                Self::UNINIT,
                Self::INITING,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Safe to use ResetOnDrop: init_fn panics causes destructor,
                    // UNINIT state = 0, guard forgotten on success
                    let guard = ResetOnDrop { state: &self.state };
                    let value = init_fn();

                    unsafe {
                        (*self.value.get()).write(Mutex::new(value));
                    }
                    std::mem::forget(guard);
                    self.state.store(Self::INITED, Ordering::Release);
                    return;
                }
                Err(state) => match state {
                    Self::INITING => {
                        if step > Self::MAX_SPIN {
                            std::thread::yield_now();
                        } else {
                            for _ in 0..(1 << step) {
                                std::hint::spin_loop();
                            }
                        }
                    }
                    Self::INITED => return,
                    _ => unsafe { std::hint::unreachable_unchecked() },
                },
            }

            step += 1;
        }
    }

    /// Async init, optimized
    #[inline(always)]
    pub async fn init_async<F, Fut>(&self, init_fn: F)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let mut step = 0;
        loop {
            match self.state.compare_exchange(
                Self::UNINIT,
                Self::INITING,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Safe to use ResetOnDrop: init_fn panics causes destructor,
                    // UNINIT state = 0, guard forgotten on success
                    let guard = ResetOnDrop { state: &self.state };
                    // catch unwind to ensure guard is dropped correctly
                    let value = AssertUnwindSafe(init_fn())
                        .catch_unwind()
                        .await;

                    if let Ok(value) = value {
                        unsafe {
                            (*self.value.get()).write(Mutex::new(value));
                        }
                        std::mem::forget(guard);
                        self.state.store(Self::INITED, Ordering::Release);
                    } else {
                        std::panic::resume_unwind(value.err().unwrap());
                    }
                    return;
                }
                Err(state) => match state {
                    Self::INITING => {
                        if step > Self::MAX_SPIN {
                            yield_now().await;
                        } else {
                            for _ in 0..(1 << step) {
                                std::hint::spin_loop();
                            }
                        }
                    }
                    Self::INITED => return,
                    _ => unsafe { std::hint::unreachable_unchecked() },
                },
            }

            step += 1;
        }
    }

    /// Fetches the inner value, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn inner(&self) -> &Mutex<T> {
        debug_assert!(self.is_initialized(), "RawMutexCell is not initialized");
        &*(*self.value.get()).as_ptr()
    }

    /// Fetches the inner value mutably, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn inner_mut(&self) -> &mut Mutex<T> {
        debug_assert!(self.is_initialized(), "RawMutexCell is not initialized");
        &mut *(*self.value.get()).as_mut_ptr()
    }
}
impl<T> Drop for RawMutexCell<T> {
    fn drop(&mut self) {
        if self.is_initialized() {
            unsafe {
                std::ptr::drop_in_place(self.inner_mut());
            }
        }
    }
}

/// # RawOnceCell
/// A RawCell variant that can only be initialized once. Helpful for a lazy-initialized static.
/// RawOnceCell can never be dropped once initialized.
#[repr(C, align(128))]
pub struct RawOnceCell<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<T>>,
}
unsafe impl<T: Send> Send for RawOnceCell<T> {}
unsafe impl<T: Sync> Sync for RawOnceCell<T> {}
impl<T> const CellTrait<T> for RawOnceCell<T> {
    #[inline(always)]
    fn new() -> Self {
        Self::new()
    }
}
impl<T> RawOnceCell<T> {
    const UNINIT: u8 = 0;
    const INITING: u8 = 1;
    const INITED: u8 = 2;

    /// Creates a new, uninitialized RawOnceCell.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(Self::UNINIT),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Checks if the RawOnceCell has been initialized.
    #[inline(always)]
    pub fn is_initialized(&self) -> bool {
        self.state.load(Ordering::Acquire) == Self::INITED
    }

    /// Initializes the RawOnceCell if it has not been initialized yet.
    #[inline(always)]
    pub fn init<F>(&self, init_fn: F)
    where
        F: FnOnce() -> T,
    {
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
                        // Safe to use ResetOnDrop: init_fn panics causes destructor,
                        // UNINIT state = 0, guard forgotten on success
                        let guard = ResetOnDrop { state: &self.state };
                        unsafe {
                            (*self.value.get()).as_mut_ptr().write(init_fn());
                        }
                        std::mem::forget(guard);
                        self.state.store(Self::INITED, Ordering::Release);
                        return;
                    }
                }
                Self::INITING => {
                    if step > 8 {
                        std::thread::yield_now();
                    } else {
                        for _ in 0..(1 << step) {
                            std::hint::spin_loop();
                        }
                    }
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            }

            step += 1;
        }
    }

    /// Initializes the RawOnceCell asynchronously if it has not been initialized yet.
    #[inline(always)]
    pub async fn init_async<F, Fut>(&self, init_fn: F)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
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
                        // Safe to use ResetOnDrop: init_fn panics causes destructor,
                        // UNINIT state = 0, guard forgotten on success
                        let guard = ResetOnDrop { state: &self.state };
                        // catch unwind to ensure guard is dropped correctly
                        let value = AssertUnwindSafe(init_fn())
                            .catch_unwind()
                            .await;

                        if let Ok(value) = value {
                            unsafe {
                                (*self.value.get()).as_mut_ptr().write(value);
                            }
                            std::mem::forget(guard);
                            self.state.store(Self::INITED, Ordering::Release);
                        } else {
                            std::panic::resume_unwind(value.err().unwrap());
                        }
                        return;
                    }
                }
                Self::INITING => {
                    if step > 8 {
                        std::thread::yield_now();
                    } else {
                        for _ in 0..(1 << step) {
                            std::hint::spin_loop();
                        }
                    }
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            }

            step += 1;
        }
    }

    /// Initialize the RawOnceCell if not already initialized, returning a reference to the inner value.
    #[inline(always)]
    pub fn get_or_init<F>(&self, init_fn: F) -> &'static T
    where
        F: FnOnce() -> T,
    {
        if !self.is_initialized() {
            self.init(init_fn);
        }
        unsafe { self.inner() }
    }

    /// Fetches the inner value, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn inner(&self) -> &'static T {
        debug_assert!(self.is_initialized(), "RawOnceCell is not initialized");
        &*(*self.value.get()).as_ptr()
    }
}
// Drop is intentionally not implemented for RawOnceCell to prevent dropping once initialized.

/// # RawDirectCell
/// RawDirectCell is a low-level primitive that offers direct access to its inner value,
/// without any locks or synchronization mechanisms. It is designed for single-threaded
/// contexts or scenarios where the user can guarantee exclusive access to the cell.
/// 
/// RawDirectCell doesn't support asynchronous operations or concurrent initializations;
/// it is the user's responsibility to ensure safe access patterns.
#[repr(C, align(128))]
pub struct RawDirectCell<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    state: AtomicBool
}
impl<T> const CellTrait<T> for RawDirectCell<T> {
    #[inline(always)]
    fn new() -> Self {
        Self::new()
    }
}
impl<T> RawDirectCell<T> {
    /// Creates a new, uninitialized RawDirectCell.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            state: AtomicBool::new(false),
        }
    }

    /// Initializes the RawDirectCell with the provided value.
    #[inline(always)]
    pub fn init<F>(&self, init_fn: F)
    where
        F: FnOnce() -> T,
    {
        if self.is_initialized() {
            return;
        }

        unsafe {
            (*self.value.get()).as_mut_ptr().write(init_fn());
        }
        self.state.store(true, Ordering::Release);
    }

    /// Determines if the RawDirectCell has been initialized.
    #[inline(always)]
    pub fn is_initialized(&self) -> bool {
        self.state.load(Ordering::Acquire)
    }

    /// Fetches the inner value, initializing it if necessary.
    #[inline(always)]
    pub fn get_or_init<F>(&self, init_fn: F) -> &T
    where
        F: FnOnce() -> T,
    {
        if unlikely(!self.is_initialized()) {
            self.init(init_fn);
        }
        unsafe { self.inner() }
    }

    /// Fetches the inner value, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn inner(&self) -> &T {
        &*(*self.value.get()).as_ptr()
    }

    /// Fetches the inner value mutably, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn inner_mut(&self) -> &mut T {
        &mut *(*self.value.get()).as_mut_ptr()
    }
}
impl<T> Drop for RawDirectCell<T> {
    fn drop(&mut self) {
        if self.is_initialized() {
            unsafe {
                std::ptr::drop_in_place(self.inner_mut());
            }
        }
    }
}

/// # RawOptionCell
/// A RawCell variant that can hold an Option<T>, allowing for "empty" states even
/// after initialization.
#[repr(C, align(128))]
pub struct RawOptionCell<T> {
    inner: RawCell<Option<T>>,
}
unsafe impl<T: Send> Send for RawOptionCell<T> {}
unsafe impl<T: Sync> Sync for RawOptionCell<T> {}
impl<T: Send + Sync> const CellTrait<Option<T>> for RawOptionCell<T> {
    #[inline(always)]
    fn new() -> Self {
        Self::new()
    }
}
impl<T: Send + Sync> RawOptionCell<T> {
    /// Creates a new, uninitialized RawOptionCell.
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            inner: RawCell::new(),
        }
    }

    /// Initializes the RawOptionCell with the provided value.
    #[inline(always)]
    pub fn init<F>(&self, init_fn: F)
    where
        F: FnOnce() -> Option<T>,
    {
        self.inner.init(init_fn);
    }

    /// Async initializes the RawOptionCell with the provided value.
    #[inline(always)]
    pub async fn init_async<F, Fut>(&self, init_fn: F)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Option<T>>,
    {
        self.inner.init_async(init_fn).await;
    }

    /// Fetches a read lock to the inner Option<T>, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn read(&self) -> RwLockReadGuard<'_, Option<T>> {
        self.inner.read()
    }

    /// Fetches a write lock to the inner Option<T>, assuming initialization has already occurred.
    #[inline(always)]
    pub unsafe fn write(&self) -> RwLockWriteGuard<'_, Option<T>> {
        self.inner.write()
    }
}
// Drop is automatically handled by RawCell's Drop implementation.
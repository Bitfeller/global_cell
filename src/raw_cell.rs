use tokio::sync::{ RwLock, Mutex, RwLockReadGuard, RwLockWriteGuard };
use std::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{ AtomicU8, Ordering }};
use crate::error::CellError;

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
pub(crate) struct RawCell<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<RwLock<Option<T>>>>,

    init: UnsafeCell<Option<fn() -> T>>,
}
unsafe impl<T: Send> Send for RawCell<T> {}
unsafe impl<T: Sync> Sync for RawCell<T> {}

impl<T: Send + Sync> RawCell<T> {
    const UNINIT: u8 = 0;
    const INITING: u8 = 1;
    const INITED: u8 = 2;
    const SETTING_INIT: u8 = 3;

    /// Create a new RawCell that is empty (uninitialized).
    /// The cell self-initializes before first use, using the provided initializer function.
    pub(crate) const fn new() -> Self {
        Self {
            state: AtomicU8::new(Self::UNINIT),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            init: UnsafeCell::new(None)
        }
    }

    /// Create a RawCell, initialized with the default value of T.
    pub(crate) fn default() -> Self 
    where T: Default
    {
        // Initialize the cell.
        let cell = Self::new();
        // SAFETY: We are the only thread that can initialize the cell at this point.
        unsafe {
            *cell.value.get() = MaybeUninit::new(RwLock::new(Some(T::default())));
        }
        cell.state.store(Self::INITED, Ordering::Release);
        cell
    }

    /// Create a new RawCell with the provided initializer function.
    /// The cell self-initializes before first use, using the provided initializer function.
    pub(crate) const fn with_initializer(init: fn() -> T) -> Self
    {
        Self {
            state: AtomicU8::new(Self::UNINIT),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            init: UnsafeCell::new(Some(init))
        }
    }

    /// Supply an initializer function to the RawCell.
    /// This can only be done if the cell is not already initialized.
    /// If the cell is already initialized, this will return an Err.
    pub(crate) async fn set_initializer(&self, init: fn() -> T) -> Result<(), CellError> {
        match self.state.compare_exchange(
            Self::UNINIT,
            Self::SETTING_INIT,
            Ordering::AcqRel,
            Ordering::Acquire
        ) {
            Ok(_) => {
                // We are the only thread that can write to init at this point, meaning we
                // can safely write without checks.
                unsafe {
                    *self.init.get() = Some(init);
                }
                self.state.store(Self::UNINIT, Ordering::Release);
                Ok(())
            },
            Err(curr) => match curr {
                Self::SETTING_INIT => Err(CellError::InitializerBeingSet),
                Self::INITING => Err(CellError::Initializing),
                Self::INITED => Err(CellError::AlreadyInitialized),
                _ => unreachable!()
            }
        }
    }

    /// Initialize the RawCell.
    /// If an initializer function is provided, it will be used to initialize the cell.
    /// Otherwise, the cell will be "empty" (not initialized). Reads/writes to uninitialized values return an Err.
    pub(crate) async fn init(&self) -> Result<(), CellError> {
        loop {
            // Fast path: already initialized
            if self.state.load(Ordering::Acquire) == Self::INITED {
                return Ok(());
            }

            // Try to become the initializer
            if self.state.compare_exchange(
                Self::UNINIT,
                Self::INITING,
                Ordering::AcqRel,
                Ordering::Acquire
            ).is_ok() {
                // We are the initializer
                // Acquire the mutex and take the initializer if present
                let value = {
                    // SAFETY: We are the only thread that can write to init at this point, meaning we
                    // can safely read the fn without checks.
                    if let Some(init) = unsafe { &*self.init.get() } {
                        Some(init())
                    } else {
                        None
                    }
                };
                // SAFETY: We are the only thread that can write to value at this point, meaning we
                // can safely write without checks.
                unsafe {
                    (*self.value.get()).as_mut_ptr().write(RwLock::new(value));
                }
                self.state.store(Self::INITED, Ordering::Release);
                return Ok(());
            } else {
                // Check if someone is setting the initializer
                if self.state.load(Ordering::Acquire) == Self::SETTING_INIT {
                    // Wait for them to finish
                    while self.state.load(Ordering::Acquire) == Self::SETTING_INIT {
                        tokio::task::yield_now().await;
                    }
                    // Try again to become the initializer
                    continue;
                }

                // Wait for initialization to complete
                while self.state.load(Ordering::Acquire) != Self::INITED {
                    tokio::task::yield_now().await;
                }
                return Ok(());
            }
        }
    }

    /// Initialize the function with an empty value, ignoring any initializer function.
    /// This can only be done if the cell is not already initialized.
    pub(crate) async fn init_empty(&self) -> Result<(), CellError> {
        loop {
            // Fast path: already initialized
            if self.state.load(Ordering::Acquire) == Self::INITED {
                return Ok(());
            }

            // Try to become the initializer
            if self.state.compare_exchange(
                Self::UNINIT,
                Self::INITING,
                Ordering::AcqRel,
                Ordering::Acquire
            ).is_ok() {
                // We are the initializer
                // SAFETY: We are the only thread that can write to value at this point, meaning we
                // can safely write without checks.
                unsafe {
                    (*self.value.get()).as_mut_ptr().write(RwLock::new(None));
                }
                self.state.store(Self::INITED, Ordering::Release);
                return Ok(());
            } else {
                // Check if someone is setting the initializer
                if self.state.load(Ordering::Acquire) == Self::SETTING_INIT {
                    // Wait for them to finish
                    while self.state.load(Ordering::Acquire) == Self::SETTING_INIT {
                        tokio::task::yield_now().await;
                    }
                    // Try again to become the initializer
                    continue;
                }

                // Wait for initialization to complete
                while self.state.load(Ordering::Acquire) != Self::INITED {
                    tokio::task::yield_now().await;
                }
                return Ok(());
            }
        }
    }

    /// Get a read lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
    pub(crate) async unsafe fn read(&self) -> RwLockReadGuard<'_, Option<T>> {
        // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid RwLock.
        let lock = unsafe { &*(*self.value.get()).as_ptr() };
        return lock.read().await;
    }
    
    /// Get a write lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
    pub(crate) async unsafe fn write(&self) -> RwLockWriteGuard<'_, Option<T>> {
        // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid RwLock.
        let lock = unsafe { &*(*self.value.get()).as_ptr() };
        return lock.write().await;
    }

    /// Make a blocking read lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
    pub(crate) unsafe fn blocking_read(&self) -> RwLockReadGuard<'_, Option<T>> {
        // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid RwLock.
        let lock = unsafe { &*(*self.value.get()).as_ptr() };
        return lock.blocking_read();
    }

    /// Make a blocking write lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
    pub(crate) unsafe fn blocking_write(&self) -> RwLockWriteGuard<'_, Option<T>> {
        // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid RwLock.
        let lock = unsafe { &*(*self.value.get()).as_ptr() };
        return lock.blocking_write();
    }

    /// Attempt to immediately acquire a read lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
    pub(crate) unsafe fn try_read(&self) -> Option<RwLockReadGuard<'_, Option<T>>> {
        // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid RwLock.
        let lock = unsafe { &*(*self.value.get()).as_ptr() };
        return lock.try_read().ok();
    }

    /// Attempt to immediately acquire a write lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
    pub(crate) unsafe fn try_write(&self) -> Option<RwLockWriteGuard<'_, Option<T>>> {
        // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid RwLock.
        let lock = unsafe { &*(*self.value.get()).as_ptr() };
        return lock.try_write().ok();
    }

    /// Empty the contents of the cell, leaving the cell initialized but with no value.
    /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
    pub(crate) async unsafe fn clear(&self) {
        let mut write_guard = self.write().await;
        *write_guard = None;
    }

    /// Check if the cell is initialized.
    pub(crate) fn is_initialized(&self) -> bool {
        self.state.load(Ordering::Acquire) == Self::INITED
    }

    /// Check if the cell has a value.
    /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
    pub(crate) async unsafe fn has_value(&self) -> bool {
        let read_guard = self.read().await;
        read_guard.is_some()
    }
}

/// RawCellMutex is a low-level primitive to offer performant access and interior mutability,
/// while being statically assignable and shareable across threads (send + sync).
/// Refer to the definitions of RawCell for more information.
/// RawCellMutex is meant to reflect a Mutex-based implementation of RawCell,
/// rather than using RwLock, for write-heavy contexts.
#[allow(dead_code)]
pub(crate) struct RawCellMutex<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<Mutex<Option<T>>>>,

    init: UnsafeCell<Option<fn() -> T>>,
}
// unsafe impl<T: Send> Send for RawCellMutex<T> {}
// unsafe impl<T: Sync> Sync for RawCellMutex<T> {}
// impl<T: Send + Sync> RawCellMutex<T> {
//     const UNINIT: u8 = 0;
//     const INITING: u8 = 1;
//     const INITED: u8 = 2;
//     const SETTING_INIT: u8 = 3;

//     /// Create a new RawCellMutex that is empty (uninitialized).
//     /// The cell self-initializes before first use, using the provided initializer function.
//     pub(crate) const fn new() -> Self {
//         Self {
//             state: AtomicU8::new(Self::UNINIT),
//             value: UnsafeCell::new(MaybeUninit::uninit()),
//             init: UnsafeCell::new(None)
//         }
//     }

//     /// Create a new RawCellMutex with the provided initializer function.
//     /// The cell self-initializes before first use, using the provided initializer function.
//     pub(crate) const fn with_initializer(init: fn() -> T) -> Self
//     {
//         Self {
//             state: AtomicU8::new(Self::UNINIT),
//             value: UnsafeCell::new(MaybeUninit::uninit()),
//             init: UnsafeCell::new(Some(init))
//         }
//     }

//     /// Supply an initializer function to the RawCellMutex.
//     /// This can only be done if the cell is not already initialized.
//     /// If the cell is already initialized, this will return an Err.
//     pub(crate) async fn set_initializer(&self, init: fn() -> T) -> Result<(), CellError> {
//         match self.state.compare_exchange(
//             Self::UNINIT,
//             Self::SETTING_INIT,
//             Ordering::AcqRel,
//             Ordering::Acquire
//         ) {
//             Ok(_) => {
//                 // We are the only thread that can write to init at this point, meaning we
//                 // can safely write without checks.
//                 unsafe {
//                     *self.init.get() = Some(init);
//                 }
//                 self.state.store(Self::UNINIT, Ordering::Release);
//                 Ok(())
//             },
//             Err(curr) => match curr {
//                 Self::SETTING_INIT => Err(CellError::InitializerBeingSet),
//                 Self::INITING => Err(CellError::Initializing),
//                 Self::INITED => Err(CellError::AlreadyInitialized),
//                 _ => unreachable!()
//             }
//         }
//     }

//     /// Initialize the RawCellMutex.
//     /// If an initializer function is provided, it will be used to initialize the cell.
//     /// Otherwise, the cell will be "empty" (not initialized). Reads/writes to uninitialized values return an Err.
//     pub(crate) async fn init(&self) -> Result<(), CellError> {
//         loop {
//             // Fast path: already initialized
//             if self.state.load(Ordering::Acquire) == Self::INITED {
//                 return Ok(());
//             }

//             // Try to become the initializer
//             if self.state.compare_exchange(
//                 Self::UNINIT,
//                 Self::INITING,
//                 Ordering::AcqRel,
//                 Ordering::Acquire
//             ).is_ok() {
//                 // We are the initializer
//                 // Acquire the mutex and take the initializer if present
//                 let value = {
//                     // SAFETY: We are the only thread that can write to init at this point, meaning we
//                     // can safely read the fn without checks.
//                     if let Some(init) = unsafe { &*self.init.get() } {
//                         Some(init())
//                     } else {
//                         None
//                     }
//                 };
//                 // SAFETY: We are the only thread that can write to value at this point, meaning we
//                 // can safely write without checks.
//                 unsafe {
//                     (*self.value.get()).as_mut_ptr().write(Mutex::new(value));
//                 }
//                 self.state.store(Self::INITED, Ordering::Release);
//                 return Ok(());
//             } else {
//                 // Check if someone is setting the initializer
//                 if self.state.load(Ordering::Acquire) == Self::SETTING_INIT {
//                     // Wait for them to finish
//                     while self.state.load(Ordering::Acquire) == Self::SETTING_INIT {
//                         tokio::task::yield_now().await;
//                     }
//                     // Try again to become the initializer
//                     continue;
//                 }

//                 // Wait for initialization to complete
//                 while self.state.load(Ordering::Acquire) != Self::INITED {
//                     tokio::task::yield_now().await;
//                 }
//                 return Ok(());
//             }
//         }
//     }

//     /// Initialize the function with an empty value, ignoring any initializer function.
//     /// This can only be done if the cell is not already initialized.
//     pub(crate) async fn init_empty(&self) -> Result<(), CellError> {
//         loop {
//             // Fast path: already initialized
//             if self.state.load(Ordering::Acquire) == Self::INITED {
//                 return Ok(());
//             }

//             // Try to become the initializer
//             if self.state.compare_exchange(
//                 Self::UNINIT,
//                 Self::INITING,
//                 Ordering::AcqRel,
//                 Ordering::Acquire
//             ).is_ok() {
//                 // We are the initializer
//                 // SAFETY: We are the only thread that can write to value at this point, meaning we
//                 // can safely write without checks.
//                 unsafe {
//                     (*self.value.get()).as_mut_ptr().write(Mutex::new(None));
//                 }
//                 self.state.store(Self::INITED, Ordering::Release);
//                 return Ok(());
//             } else {
//                 // Check if someone is setting the initializer
//                 if self.state.load(Ordering::Acquire) == Self::SETTING_INIT {
//                     // Wait for them to finish
//                     while self.state.load(Ordering::Acquire) == Self::SETTING_INIT {
//                         tokio::task::yield_now().await;
//                     }
//                     // Try again to become the initializer
//                     continue;
//                 }

//                 // Wait for initialization to complete
//                 while self.state.load(Ordering::Acquire) != Self::INITED {
//                     tokio::task::yield_now().await;
//                 }
//                 return Ok(());
//             }
//         }
//     }

//     /// Get a lock on the inner value.
//     /// If the cell is not initialized, it will be initialized using the provided initializer function.
//     /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
//     pub(crate) async unsafe fn lock(&self) -> MutexGuard<'_, Option<T>> {
//         // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid Mutex.
//         let lock = unsafe { &*(*self.value.get()).as_ptr() };
//         return lock.lock().await;
//     }

//     /// Make a blocking lock on the inner value.
//     /// If the cell is not initialized, it will be initialized using the provided initializer function.
//     pub(crate) fn lock_blocking(&self) -> MutexGuard<'_, Option<T>> {
//         // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid Mutex.
//         let lock = unsafe { &*(*self.value.get()).as_ptr() };
//         return lock.blocking_lock();
//     }

//     /// Attempt to immediately acquire a lock on the inner value.
//     /// If the cell is not initialized, it will be initialized using the provided initializer function.
//     pub(crate) fn try_lock(&self) -> Option<MutexGuard<'_, Option<T>>> {
//         // SAFETY: We have ensured that the cell is initialized, meaning value is now a valid Mutex.
//         let lock = unsafe { &*(*self.value.get()).as_ptr() };
//         match lock.try_lock() {
//             Ok(guard) => Some(guard),
//             Err(_) => None,
//         }
//     }

//     /// Empty the contents of the cell, leaving the cell initialized but with no value.
//     /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
//     pub(crate) async unsafe fn clear(&self) {
//         let mut write_guard = self.lock().await;
//         *write_guard = None;
//     }

//     /// Check if the cell is initialized.
//     pub(crate) fn is_initialized(&self) -> bool {
//         self.state.load(Ordering::Acquire) == Self::INITED
//     }

//     /// Check if the cell has a value.
//     /// SAFETY: It is up to the caller to ensure that the cell is already initialized.
//     pub(crate) async unsafe fn has_value(&self) -> bool {
//         let read_guard = self.lock().await;
//         read_guard.is_some()
//     }
// }
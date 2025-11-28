use std::{fmt::Debug, hint::unlikely, ops::{Deref, DerefMut}, panic::AssertUnwindSafe};
use parking_lot::{RwLockReadGuard, RwLockWriteGuard};
use crate::{raw_cell::{RawCell, RawMutexCell, RawOnceCell}, traits::{CellTrait, ManagedCell}};
use futures_util::future::FutureExt;

/// A direct read-only lock to a Cell-based structure.
pub struct CellLock<'a, T> {
    lock: RwLockReadGuard<'a, T>,
}
impl<T> CellLock<'_, T> {
    /// Offers &T directly.
    pub fn get(&self) -> &T {
        &*self.lock
    }

    // Offers a raw pointer to T. Does not guarantee ptr's validity beyond the lifetime of the lock.
    pub unsafe fn as_ptr(&self) -> *const T {
        &*self.lock as *const T
    }

    /// Offers a copy of T.
    pub fn copy(&self) -> T 
    where
        T: Copy
    {
        *self.lock
    }

    /// Offers a clone of T.
    pub fn cloned(&self) -> T 
    where
        T: Clone
    {
        self.lock.clone()
    }
}
impl<T> Deref for CellLock<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.lock
    }
}
impl<T: Debug> Debug for CellLock<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CellLock")
            .field("value", &*self.lock)
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}
impl<'a, T> From<RwLockReadGuard<'a, T>> for CellLock<'a, T> {
    fn from(lock: RwLockReadGuard<'a, T>) -> Self {
        Self { lock }
    }
}

/// A direct mutable lock to a Cell-based structure.
pub struct MutCellLock<'a, T> {
    lock: RwLockWriteGuard<'a, T>,
}
impl<T> MutCellLock<'_, T> {
    /// Offers &mut T directly.
    pub fn get_mut(&mut self) -> &mut T {
        &mut *self.lock
    }

    /// Offers &T directly.
    pub fn get_immut(&self) -> &T {
        &*self.lock
    }

    // Offers a raw pointer to T. Does not guarantee ptr's validity beyond the lifetime of the lock.
    pub unsafe fn as_mut_ptr(&mut self) -> *mut T {
        &mut *self.lock as *mut T
    }

    /// Offers a copy of T.
    pub fn copy(&self) -> T 
    where
        T: Copy
    {
        *self.lock
    }

    /// Offers a clone of T.
    pub fn cloned(&self) -> T 
    where
        T: Clone
    {
        self.lock.clone()
    }
}
impl<T> Deref for MutCellLock<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.lock
    }
}
impl<T> DerefMut for MutCellLock<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.lock
    }
}
impl<'a, T> From<RwLockWriteGuard<'a, T>> for MutCellLock<'a, T> {
    fn from(lock: RwLockWriteGuard<'a, T>) -> Self {
        Self { lock }
    }
}
pub struct CellMutexLock<'a, T> {
    lock: parking_lot::lock_api::MutexGuard<'a, parking_lot::RawMutex, T>,
}
impl<'a, T> CellMutexLock<'a, T> {
    /// Offers &mut T directly.
    pub fn get_mut(&mut self) -> &mut T {
        &mut *self.lock
    }

    /// Offers &T directly.
    pub fn get_immut(&self) -> &T {
        &*self.lock
    }

    // Offers a raw pointer to T. Does not guarantee ptr's validity beyond the lifetime of the lock.
    pub unsafe fn as_mut_ptr(&mut self) -> *mut T {
        &mut *self.lock as *mut T
    }

    /// Offers a copy of T.
    pub fn copy(&self) -> T 
    where
        T: Copy
    {
        *self.lock
    }

    /// Offers a clone of T.
    pub fn cloned(&self) -> T 
    where
        T: Clone
    {
        self.lock.clone()
    }

    fn from(lock: parking_lot::lock_api::MutexGuard<'a, parking_lot::RawMutex, T>) -> Self {
        Self { lock }
    }
}
impl<T> Deref for CellMutexLock<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.lock
    }
}
impl<T> DerefMut for CellMutexLock<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.lock
    }
}

pub struct Cell<T> {
    raw: RawCell<T>,
    init_fn: Option<fn() -> T>,
    init_async_fn: Option<fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>>,
}
impl<T: Send + Sync> const CellTrait<T> for Cell<T> {
    fn new() -> Self {
        Self::new()
    }
}
impl<T: Send + Sync> ManagedCell<T> for Cell<T> {
    fn init(&self, value: T) {
        self.init(value)
    }
    fn is_initialized(&self) -> bool {
        self.is_initialized()
    }
}
impl<T: Send + Sync> Cell<T> {
    pub const fn new() -> Self {
        Self {
            raw: RawCell::new(),
            init_fn: None,
            init_async_fn: None,
        }
    }
    pub const fn with_fn(init_fn: fn() -> T) -> Self {
        Self {
            raw: RawCell::new(),
            init_fn: Some(init_fn),
            init_async_fn: None,
        }
    }
    pub const fn with_async_fn(init_async_fn: fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>) -> Self {
        Self {
            raw: RawCell::new(),
            init_fn: None,
            init_async_fn: Some(init_async_fn),
        }
    }
    pub const fn default() -> Self 
    where
        T: Default
    {
        Self {
            raw: RawCell::new(),
            init_fn: Some(T::default),
            init_async_fn: None,
        }
    }

    pub fn init(&self, value: T) {
        self.raw.init(|| value)
    }
    pub async fn init_async(&self, value: T) {
        self.raw.init_async(async || value).await
    }
    pub fn init_default(&self) 
    where
        T: Default
    {
        self.raw.init(|| T::default())
    }
    pub fn init_with<F>(&self, f: F) 
    where
        F: FnOnce() -> T
    {
        self.raw.init(f)
    }
    pub async fn init_async_with<F, Fut>(&self, f: F) 
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>
    {
        self.raw.init_async(f).await
    }

    pub fn is_initialized(&self) -> bool {
        self.raw.is_initialized()
    }

    /// Attempt to initialize a cell. If it can't, an err is returned.
    pub fn try_init(&self) -> Result<(), &'static str> {
        match self.init_fn {
            Some(init_fn) => {
                std::panic::catch_unwind(AssertUnwindSafe(|| {
                    self.raw.init(init_fn)
                }))
                    .map_err(|_| "Cell init function panicked")?;
                Ok(())
            },
            None => match self.init_async_fn {
                Some(init_async_fn) => {
                    futures_executor::block_on(AssertUnwindSafe(async {
                        self.raw.init_async(init_async_fn).await
                    })
                        .catch_unwind())
                        .map_err(|_| "Cell async init function panicked")?;
                    Ok(())
                },
                None => Err("Cell has no init function"),
            },
        }
    }
    /// Attempt to initialize a cell asynchronously. If it can't, an err is returned.
    pub async fn try_init_async(&self) -> Result<(), &'static str> {
        match self.init_async_fn {
            Some(init_async_fn) => {
                let res = AssertUnwindSafe(async {
                    self.raw.init_async(init_async_fn).await
                })
                    .catch_unwind()
                    .await;
                res.map_err(|_| "Cell async init function panicked")?;
                Ok(())
            },
            None => match self.init_fn {
                Some(init_fn) => {
                    std::panic::catch_unwind(AssertUnwindSafe(|| {
                        self.raw.init(init_fn)
                    }))
                        .map_err(|_| "Cell init function panicked")?;
                    Ok(())
                },
                None => Err("Cell has no async init function"),
            },
        }
    }

    pub fn read(&self) -> CellLock<'_, T> {
        if unlikely(!self.is_initialized()) {
            self.try_init().expect("Cell was not initialized, and initialization failed");
        }
        // SAFETY: the cell has been initialized.
        CellLock::from(unsafe { self.raw.read() })
    }
    pub fn write(&self) -> MutCellLock<'_, T> {
        if unlikely(!self.is_initialized()) {
            self.try_init().expect("Cell was not initialized, and initialization failed");
        }
        // SAFETY: the cell has been initialized.
        MutCellLock::from(unsafe { self.raw.write() })
    }

    pub async fn read_async(&self) -> CellLock<'_, T> {
            if unlikely(!self.is_initialized()) {
                self.try_init_async().await.expect("Cell was not initialized, and initialization failed");
            }
            // SAFETY: the cell has been initialized.
            CellLock::from(unsafe { self.raw.read() })
    }
    pub async fn write_async(&self) -> MutCellLock<'_, T> {
        if unlikely(!self.is_initialized()) {
            self.try_init_async().await.expect("Cell was not initialized, and initialization failed");
        }
        // SAFETY: the cell has been initialized.
        MutCellLock::from(unsafe { self.raw.write() })
    }

    /// Execute a closure with a read lock.
    pub fn with<F, R>(&self, f: F) -> R 
    where
        F: FnOnce(&T) -> R
    {
        let lock = self.read();
        f(&*lock)
    }

    /// Execute a closure with a write lock.
    pub fn with_mut<F, R>(&self, f: F) -> R 
    where
        F: FnOnce(&mut T) -> R
    {
        let mut lock = self.write();
        f(&mut *lock)
    }

    /// Execute an async closure with a read lock.
    pub async fn with_async<F, Fut, R>(&self, f: F) -> R 
    where
        F: FnOnce(&T) -> Fut,
        Fut: std::future::Future<Output = R>
    {
        let lock = self.read_async().await;
        f(&*lock).await
    }

    /// Execute an async closure with a write lock.
    pub async fn with_mut_async<F, Fut, R>(&self, f: F) -> R 
    where
        F: FnOnce(&mut T) -> Fut,
        Fut: std::future::Future<Output = R>
    {
        let mut lock = self.write_async().await;
        f(&mut *lock).await
    }

    /// Clone the inner value.
    /// Requires T: Clone.
    pub fn cloned(&self) -> T 
    where
        T: Clone
    {
        let lock = self.read();
        lock.clone()
    }

    /// Copy the inner value.
    /// Requires T: Copy.
    pub fn copy(&self) -> T 
    where
        T: Copy
    {
        let lock = self.read();
        *lock
    }

    /// Sets the value of the cell.
    pub fn set(&self, value: T) {
        let mut lock = self.write();
        *lock = value;
    }
}
impl<T: Send + Sync> Cell<Option<T>> {
    /// Takes the value out of the cell, leaving None in its place.
    pub fn take(&self) -> Option<T> {
        let mut lock = self.write();
        lock.take()
    }
}
impl<T: Send + Sync> Default for Cell<T> {
    /// Creates a new Cell with a default value.
    fn default() -> Self {
        Self::new()
    }
}
impl<T: Send + Sync + Debug> Debug for Cell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cell")
            .field("initialized", &self.is_initialized())
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}

pub struct MutexCell<T> {
    raw: RawMutexCell<T>,
    init_fn: Option<fn() -> T>,
    init_async_fn: Option<fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>>,
}
impl<T: Send> const CellTrait<T> for MutexCell<T> {
    fn new() -> Self {
        Self::new()
    }
}
impl<T: Send> ManagedCell<T> for MutexCell<T> {
    fn init(&self, value: T) {
        self.init(value)
    }
    fn is_initialized(&self) -> bool {
        self.is_initialized()
    }
}
impl<T: Send> MutexCell<T> {
    pub const fn new() -> Self {
        Self {
            raw: RawMutexCell::new(),
            init_fn: None,
            init_async_fn: None,
        }
    }
    pub const fn with_fn(init_fn: fn() -> T) -> Self {
        Self {
            raw: RawMutexCell::new(),
            init_fn: Some(init_fn),
            init_async_fn: None,
        }
    }
    pub const fn with_async_fn(init_async_fn: fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>) -> Self {
        Self {
            raw: RawMutexCell::new(),
            init_fn: None,
            init_async_fn: Some(init_async_fn),
        }
    }
    pub const fn default() -> Self 
    where
        T: Default
    {
        Self {
            raw: RawMutexCell::new(),
            init_fn: Some(T::default),
            init_async_fn: None,
        }
    }

    pub fn init(&self, value: T) {
        self.raw.init(|| value)
    }
    pub async fn init_async(&self, value: T) {
        self.raw.init_async(async || value).await
    }
    pub fn init_default(&self) 
    where
        T: Default
    {
        self.raw.init(|| T::default())
    }
    pub fn init_with<F>(&self, f: F) 
    where
        F: FnOnce() -> T
    {
        self.raw.init(f)
    }
    pub async fn init_async_with<F, Fut>(&self, f: F) 
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>
    {
        self.raw.init_async(f).await
    }
    pub fn is_initialized(&self) -> bool {
        self.raw.is_initialized()
    }

    /// Attempt to initialize a cell. If it can't, an err is returned.
    pub fn try_init(&self) -> Result<(), &'static str> {
        match self.init_fn {
            Some(init_fn) => {
                std::panic::catch_unwind(AssertUnwindSafe(|| {
                    self.raw.init(init_fn)
                }))
                    .map_err(|_| "MutexCell init function panicked")?;
                Ok(())
            },
            None => match self.init_async_fn {
                Some(init_async_fn) => {
                    futures_executor::block_on(AssertUnwindSafe(async {
                        self.raw.init_async(init_async_fn).await
                    })
                        .catch_unwind())
                        .map_err(|_| "MutexCell async init function panicked")?;
                    Ok(())
                },
                None => Err("MutexCell has no init function"),
            },
        }
    }

    /// Attempt to initialize a cell asynchronously. If it can't, an err is returned.
    pub async fn try_init_async(&self) -> Result<(), &'static str> {
        match self.init_async_fn {
            Some(init_async_fn) => {
                let res = AssertUnwindSafe(async {
                    self.raw.init_async(init_async_fn).await
                })
                    .catch_unwind()
                    .await;
                res.map_err(|_| "MutexCell async init function panicked")?;
                Ok(())
            },
            None => match self.init_fn {
                Some(init_fn) => {
                    std::panic::catch_unwind(AssertUnwindSafe(|| {
                        self.raw.init(init_fn)
                    }))
                        .map_err(|_| "MutexCell init function panicked")?;
                    Ok(())
                },
                None => Err("MutexCell has no async init function"),
            },
        }
    }

    /// Locks the cell and returns a mutable reference to the inner value.
    pub fn lock(&self) -> CellMutexLock<'_, T> {
        if unlikely(!self.is_initialized()) {
            self.try_init().expect("MutexCell was not initialized, and initialization failed");
        }
        // SAFETY: the cell has been initialized.
        CellMutexLock::from(unsafe { self.raw.inner().lock() })
    }

    /// Locks the cell asynchronously and returns a mutable reference to the inner value.
    /// FIXME: This is not truly async, as parking_lot does not support async locking.
    pub async fn lock_async(&self) -> CellMutexLock<'_, T> {
        if unlikely(!self.is_initialized()) {
            self.try_init_async().await.expect("MutexCell was not initialized, and initialization failed");
        }
        CellMutexLock::from(unsafe { self.raw.inner().lock() })
    }

    /// Access the inner value.
    pub fn with<F, R>(&self, f: F) -> R 
    where
        F: FnOnce(&T) -> R
    {
        let lock = self.lock();
        f(&*lock)
    }

    /// Asynchronously access the inner value mutably.
    pub fn with_mut<F, R>(&self, f: F) -> R 
    where
        F: FnOnce(&mut T) -> R
    {
        let mut lock = self.lock();
        f(&mut *lock)
    }

    /// Asynchronously access the inner value.
    pub async fn with_async<F, Fut, R>(&self, f: F) -> R 
    where
        F: FnOnce(&T) -> Fut,
        Fut: std::future::Future<Output = R>
    {
        let lock = self.lock_async().await;
        f(&*lock).await
    }

    /// Asynchronously access the inner value mutably.
    pub async fn with_mut_async<F, Fut, R>(&self, f: F) -> R 
    where
        F: FnOnce(&mut T) -> Fut,
        Fut: std::future::Future<Output = R>
    {
        let mut lock = self.lock_async().await;
        f(&mut *lock).await
    }

    /// Clone the inner value.
    /// Requires T: Clone.
    pub fn cloned(&self) -> T 
    where
        T: Clone
    {
        let lock = self.lock();
        lock.clone()
    }

    /// Copy the inner value.
    /// Requires T: Copy.
    pub fn copied(&self) -> T 
    where
        T: Copy
    {
        let lock = self.lock();
        *lock
    }
}
impl<T: Send + Default> Default for MutexCell<T> {
    /// Creates a new MutexCell with a default value.
    fn default() -> Self {
        Self::default()
    }
}
impl<T: Send + Debug> Debug for MutexCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutexCell")
            .field("initialized", &self.is_initialized())
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}

pub struct OnceCell<T> {
    raw: RawOnceCell<T>,
    init_fn: Option<fn() -> T>,
    init_async_fn: Option<fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>>,
}
impl<T: Send + Sync> const CellTrait<T> for OnceCell<T> {
    fn new() -> Self {
        Self::new()
    }
}
impl<T: Send + Sync> ManagedCell<T> for OnceCell<T> {
    fn init(&self, value: T) {
        self.init(value)
    }
    fn is_initialized(&self) -> bool {
        self.is_initialized()
    }
}
impl<T: Send + Sync> OnceCell<T> {
    pub const fn new() -> Self {
        Self {
            raw: RawOnceCell::new(),
            init_fn: None,
            init_async_fn: None,
        }
    }
    pub const fn with_fn(init_fn: fn() -> T) -> Self {
        Self {
            raw: RawOnceCell::new(),
            init_fn: Some(init_fn),
            init_async_fn: None,
        }
    }
    pub const fn with_async_fn(init_async_fn: fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>) -> Self {
        Self {
            raw: RawOnceCell::new(),
            init_fn: None,
            init_async_fn: Some(init_async_fn),
        }
    }
    pub const fn default() -> Self 
    where
        T: Default
    {
        Self {
            raw: RawOnceCell::new(),
            init_fn: Some(T::default),
            init_async_fn: None,
        }
    }

    pub fn init(&self, value: T) {
        self.raw.init(|| value)
    }
    pub async fn init_async(&self, value: T) {
        self.raw.init_async(async || value).await
    }
    pub fn init_default(&self) 
    where
        T: Default
    {
        self.raw.init(|| T::default())
    }
    pub fn init_with<F>(&self, f: F) 
    where
        F: FnOnce() -> T
    {
        self.raw.init(f)
    }
    pub async fn init_async_with<F, Fut>(&self, f: F) 
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>
    {
        self.raw.init_async(f).await
    }
    pub fn is_initialized(&self) -> bool {
        self.raw.is_initialized()
    }
}
impl<T: Send + Sync + Default> Default for OnceCell<T> {
    /// Creates a new OnceCell with a default value.
    fn default() -> Self {
        Self::default()
    }
}
impl<T: Send + Sync> OnceCell<T> {
    /// Attempt to initialize a cell. If it can't, an err is returned.
    pub fn try_init(&self) -> Result<(), &'static str> {
        match self.init_fn {
            Some(init_fn) => {
                std::panic::catch_unwind(AssertUnwindSafe(|| {
                    self.raw.init(init_fn)
                }))
                    .map_err(|_| "OnceCell init function panicked")?;
                Ok(())
            },
            None => match self.init_async_fn {
                Some(init_async_fn) => {
                    futures_executor::block_on(AssertUnwindSafe(async {
                        self.raw.init_async(init_async_fn).await
                    })
                        .catch_unwind())
                        .map_err(|_| "OnceCell async init function panicked")?;
                    Ok(())
                },
                None => Err("OnceCell has no init function"),
            },
        }
    }

    /// Attempt to initialize a cell asynchronously. If it can't, an err is returned.
    pub async fn try_init_async(&self) -> Result<(), &'static str> {
        match self.init_async_fn {
            Some(init_async_fn) => {
                let res = AssertUnwindSafe(async {
                    self.raw.init_async(init_async_fn).await
                })
                    .catch_unwind()
                    .await;
                res.map_err(|_| "OnceCell async init function panicked")?;
                Ok(())
            },
            None => match self.init_fn {
                Some(init_fn) => {
                    std::panic::catch_unwind(AssertUnwindSafe(|| {
                        self.raw.init(init_fn)
                    }))
                        .map_err(|_| "OnceCell init function panicked")?;
                    Ok(())
                },
                None => Err("OnceCell has no async init function"),
            },
        }
    }
}
impl<T: Send + Sync + Debug> Debug for OnceCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnceCell")
            .field("initialized", &self.is_initialized())
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}
use crate::raw_cell::RawCell;
use crate::error::CellError;
use tokio::sync::{ RwLockReadGuard, RwLockWriteGuard };

/// CellLock: A lock acquired on a Cell when accessing.
/// Released when dropped.
pub struct CellLock<'a, T: Send + Sync> {
    guard: RwLockReadGuard<'a, Option<T>>,
}
impl<'a, T: Send + Sync> CellLock<'a, T> {
    /// Get a reference to the value inside the cell.
    /// Returns None if the cell is not set.
    pub fn get(&self) -> &T {
        self.guard.as_ref().unwrap()
    }
}
impl<'a, T: Send + Sync> std::ops::Deref for CellLock<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}
impl<'a, T: Send + Sync + PartialEq> PartialEq for CellLock<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

/// MutCellLock: A mutable lock acquired on a Cell when accessing.
/// Released when dropped.
pub struct MutCellLock<'a, T: Send + Sync> {
    guard: RwLockWriteGuard<'a, Option<T>>,
}
impl<'a, T: Send + Sync> MutCellLock<'a, T> {
    /// Create a new MutCellLock from a RwLockWriteGuard.
    fn from(guard: RwLockWriteGuard<'a, Option<T>>) -> Self {
        Self { guard }
    }

    /// Get an immutable reference to the value inside the cell.
    /// Returns None if the cell is not set.
    pub fn get(&self) -> &T {
        self.guard.as_ref().unwrap()
    }

    /// Get a mutable reference to the value inside the cell.
    /// Returns None if the cell is not set.
    pub fn get_mut(&mut self) -> &mut T {
        self.guard.as_mut().unwrap()
    }
}
impl<'a, T: Send + Sync> std::ops::Deref for MutCellLock<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}
impl<'a, T: Send + Sync> std::ops::DerefMut for MutCellLock<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().unwrap()
    }
}
impl<'a, T: Send + Sync + PartialEq> PartialEq for MutCellLock<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

/// Cell: A global cell that stores a value of type T.
/// The cell can be accessed and modified from anywhere in the program.
/// The cell is thread-safe and can be used in multi-threaded environments.
pub struct Cell<T> {
    raw: RawCell<T>,
}
unsafe impl<T: Send> Send for Cell<T> {}
unsafe impl<T: Sync> Sync for Cell<T> {}
impl<T: Send + Sync> Cell<T> {
    /// Create a new Cell that is empty (uninitialized).
    /// The cell self-initializes before first use, using the provided initializer function.
    pub const fn new() -> Self {
        Self {
            raw: RawCell::new(),
        }
    }

    /// Create a new Cell with the provided initializer function.
    /// The cell self-initializes before first use, using the provided initializer function.
    pub const fn with_initializer(init: fn() -> T) -> Self {
        Self {
            raw: RawCell::with_initializer(init),
        }
    }

    /// Supply an initializer function to the Cell.
    /// This can only be done if the cell is not already initialized.
    /// If the cell is already initialized, this will return an Err.
    pub async fn set_initializer(&self, init: fn() -> T) -> Result<(), CellError> {
        self.raw.set_initializer(init).await
    }

    /// Initialize the Cell.
    /// If an initializer function is provided, it will be used to initialize the cell.
    /// Otherwise, the cell will be "empty" (not initialized). Reads/writes to uninitialized values return an Err.
    pub async fn init(&self) -> Result<(), CellError> {
        self.raw.init().await
    }

    /// Get a read lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// We'll make sure the cell is first initialized before allowing access.
    pub async fn read(&self) -> Result<CellLock<'_, T>, CellError> {
        self.raw.init().await?;
        // SAFETY: We have ensured that the cell is initialized.
        let guard = unsafe { self.raw.read().await };
        Ok(CellLock { guard })
    }

    /// Get a write lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// We'll make sure the cell is first initialized before allowing access.
    pub async fn write(&self) -> Result<MutCellLock<'_, T>, CellError> {
        self.raw.init().await?;
        // SAFETY: We have ensured that the cell is initialized.
        let guard = unsafe { self.raw.write().await };
        Ok(MutCellLock::from(guard))
    }

    /// Make a blocking read lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// # PERFORMANCE
    /// This function isn't very performant in tokio contexts. It requires that a 
    /// Tokio context is already running, and it will spawn a blocking task to
    /// await the initialization of the cell.
    /// If possible, the async `read` method should always be used instead.
    pub fn blocking_read(&self) -> Result<CellLock<'_, T>, CellError> {
        // We have to ensure that the cell has already been initialized.
        // Spawn a blocking task to await the initialization, using tokio.
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(self.raw.init())
        })?;
        // SAFETY: We have ensured that the cell is initialized.
        let guard = unsafe { self.raw.blocking_read() };
        Ok(CellLock { guard })
    }

    /// Make a blocking write lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// # PERFORMANCE
    /// This function isn't very performant in tokio contexts. It requires that a
    /// Tokio context is already running, and it will spawn a blocking task to
    /// await the initialization of the cell.
    /// If possible, the async `write` method should always be used instead.
    pub fn blocking_write(&self) -> Result<MutCellLock<'_, T>, CellError> {
        // We have to ensure that the cell has already been initialized.
        // Spawn a blocking task to await the initialization, using tokio.
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(self.raw.init())
        })?;
        // SAFETY: We have ensured that the cell is initialized.
        let guard = unsafe { self.raw.blocking_write() };
        Ok(MutCellLock::from(guard))
    }

    /// Attempt to immediately acquire the read lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// **NOTE**: If the cell isn't already initialized, the function will return an Err.
    pub fn try_read(&self) -> Result<CellLock<'_, T>, CellError> {
        // We have to ensure that the cell has already been initialized.
        if !self.is_initialized() {
            return Err(CellError::Uninitialized);
        }
        // SAFETY: We have ensured that the cell is initialized.
        unsafe {
            match self.raw.try_read() {
                Some(guard) => Ok(CellLock { guard }),
                None => Err(CellError::LockUnavailable),
            }
        }
    }

    /// Attempt to immediately acquire the write lock on the inner value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// **NOTE**: If the cell isn't already initialized, the function will return an Err.
    pub fn try_write(&self) -> Result<MutCellLock<'_, T>, CellError> {
        // We have to ensure that the cell has already been initialized.
        if !self.is_initialized() {
            return Err(CellError::Uninitialized);
        }
        // SAFETY: We have ensured that the cell is initialized.
        unsafe {
            match self.raw.try_write() {
                Some(guard) => Ok(MutCellLock::from(guard)),
                None => Err(CellError::LockUnavailable),
            }
        }
    }

    /// Check if the cell is initialized.
    pub fn is_initialized(&self) -> bool {
        self.raw.is_initialized()
    }

    /// Alias for is_initialized.
    pub fn is_init(&self) -> bool {
        self.is_initialized()
    }

    /// Check if the cell has a value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// We'll make sure the cell is first initialized before allowing access.
    pub async fn has_value(&self) -> Result<bool, CellError> {
        self.raw.init().await?;
        // SAFETY: We have ensured that the cell is initialized.
        let has_value = unsafe { self.raw.has_value().await };
        Ok(has_value)
    }

    /// Empty the contents of the cell, leaving the cell initialized but with no value.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// We'll make sure the cell is first initialized before allowing access.
    pub async fn clear(&self) -> Result<(), CellError> {
        // In this case, we're clearing the cell regardless of its value,
        // so we shouldn't initialize using any initializer function.
        self.raw.init_empty().await?;
        // SAFETY: We have ensured that the cell is initialized.
        unsafe { self.raw.clear().await; }
        Ok(())
    }

    /// Set the value of the cell.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// We'll make sure the cell is first initialized before allowing access.
    pub async fn set(&self, value: T) -> Result<(), CellError> {
        self.raw.init().await?;
        // SAFETY: We have ensured that the cell is initialized.
        unsafe {
            let mut guard = self.raw.write().await;
            *guard = Some(value);
        }
        Ok(())
    }

    /// Take the value out of the cell, leaving the cell empty.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    /// We'll make sure the cell is first initialized before allowing access.
    pub async fn take(&self) -> Result<Option<T>, CellError> {
        self.raw.init().await?;
        // SAFETY: We have ensured that the cell is initialized.
        let value = unsafe {
            let mut guard = self.raw.write().await;
            guard.take()
        };
        Ok(value)
    }

    /// Peek at the value of the cell without locking it.
    /// If the cell is not initialized, it will be initialized using the provided initializer function.
    pub async fn peek(&self) -> Option<T>
    where
        T: Clone,
    {
        self.raw.init().await.ok()?;
        // SAFETY: We have ensured that the cell is initialized.
        unsafe { self.raw.read().await.clone() }
    }

    /// Peek at the value of the cell without locking it, using Copy instead of Clone.
    pub async fn peek_copy(&self) -> Option<T>
    where
        T: Copy,
    {
        self.raw.init().await.ok()?;
        // SAFETY: We have ensured that the cell is initialized.
        unsafe { self.raw.read().await.as_ref().copied() }
    }

    /// Apply a function to the inner value, if it is set.
    /// Otherwise, return None.
    pub async fn with<F, R>(&self, func: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        self.raw.init().await.ok()?;
        // SAFETY: We have ensured that the cell is initialized.
        unsafe { self.raw.read().await.as_ref().map(func) }
    }

    /// Apply a mutable function to the inner value, if it is set.
    /// Otherwise, return None.
    pub async fn with_mut<F, R>(&self, func: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        self.raw.init().await.ok()?;
        // SAFETY: We have ensured that the cell is initialized.
        unsafe { self.raw.write().await.as_mut().map(func) }
    }

    /// Apply a function to the inner value, if it is set.
    /// Returns an error if the cell is not initialized.
    pub async fn try_with<F, R>(&self, func: F) -> Result<Option<R>, CellError>
    where
        F: FnOnce(&T) -> R,
    {
        self.raw.init().await?;
        // SAFETY: We have ensured that the cell is initialized.
        unsafe { Ok(self.raw.read().await.as_ref().map(func)) }
    }

    /// Apply a mutable function to the inner value, if it is set.
    /// Returns an error if the cell is not initialized.
    pub async fn try_with_mut<F, R>(&self, func: F) -> Result<Option<R>, CellError>
    where
        F: FnOnce(&mut T) -> R,
    {
        self.raw.init().await?;
        // SAFETY: We have ensured that the cell is initialized.
        unsafe { Ok(self.raw.write().await.as_mut().map(func)) }
    }
}
impl<T: Send + Sync + Default> Cell<T> {
    /// Create a new Cell that is uninitialized but will automatically initialize with the default value of T.
    pub const fn to_default() -> Self {
        Self {
            raw: RawCell::with_initializer(T::default),
        }
    }

    /// Create a new Cell that is initialized with the default value of T.
    pub fn default() -> Self {
        Self {
            raw: RawCell::default(),
        }
    }
}
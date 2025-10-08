use std::fmt::{ Display, Formatter };
use std::error::Error;
use std::sync::Arc;
use tokio::sync::{ OnceCell, RwLock, RwLockReadGuard, RwLockWriteGuard };
#[cfg(feature = "serialize")]
use serde::{ Serialize, Deserialize };
#[cfg(feature = "watch")]
use tokio::sync::watch;

#[cfg(feature = "test-crate")]
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum CellError {
    NotSet,
    AlreadySet,
    #[cfg(feature = "watch")]
    NotifierInitFailed,
}
impl Display for CellError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CellError::NotSet => write!(f, "Cell is not set"),
            CellError::AlreadySet => write!(f, "Cell is already set"),
            #[cfg(feature = "watch")]
            CellError::NotifierInitFailed => write!(f, "Notifier initialization failed"),
        }
    }
}
impl Error for CellError {}

/// CellLock: A lock acquired on a Cell when accessing.
/// Released when dropped.
pub struct CellLock<'a, T: Send + Sync> {
    guard: RwLockReadGuard<'a, Option<T>>,
}
impl<'a, T: Send + Sync> CellLock<'a, T> {
    /// Create a new CellLock from a RwLockReadGuard.
    fn from(guard: RwLockReadGuard<'a, Option<T>>) -> Self {
        Self { guard }
    }

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
pub struct Cell<T: Send + Sync> {
    cell: OnceCell<Arc<RwLock<Option<T>>>>,
    #[cfg(feature = "watch")]
    notifier: OnceCell<watch::Sender<()>>,
}
impl<T: Send + Sync> Cell<T> {
    /// Creates a new empty Cell instance.
    pub const fn new() -> Self {
        Self {
            cell: OnceCell::const_new(),
            #[cfg(feature = "watch")]
            notifier: OnceCell::const_new(),
        }
    }

    /// Creates a new Cell instance with the given initial value.
    pub fn from(value: T) -> Self {
        let cell = OnceCell::new();
        if cell.set(Arc::new(RwLock::new(Some(value)))).is_err() {
            panic!("Cell::from failed: cell OnceCell semaphore held a bad permit");
        }
        Self {
            cell,
            #[cfg(feature = "watch")]
            notifier: {
                let notifier = OnceCell::new();
                let (tx, _rx) = watch::channel(());
                if notifier.set(tx).is_err() {
                    panic!("Cell::from failed: notifier OnceCell semaphore held a bad permit");
                }
                notifier
            }
        }
    }

    /// Initialize the cell.
    /// Sets the default value of the cell to None.
    /// 
    /// This does not have to be called before using the cell, but can be used to ensure that the cell is initialized before use,
    /// for performance reasons.
    ///
    /// Consecutive calls to init() will return an error. Use is_init() to check if already initialized.
    ///
    /// Note: this does not guarantee that the inner value is set.
    /// Check is_set() to determine if there is actually an inner value.
    pub async fn init(&self) -> Result<(), CellError> {
        if self.cell.get().is_some() {
            return Ok(());
        }
        let lock = Arc::new(RwLock::new(None));
        self.cell.set(lock)
            .map_err(|_| CellError::AlreadySet)?;
        #[cfg(feature = "watch")]
        {
            let (tx, _rx) = watch::channel(());
            self.notifier.set(tx)
                .map_err(|_| CellError::NotifierInitFailed)?;
        }
        Ok(())
    }

    /// Sets the value of the cell; overrides the previous value if there was any.
    pub async fn set(&self, value: T) -> Result<(), CellError> {
        if !self.is_init() {
            self.init().await?;
        }
        #[cfg(feature = "watch")]
        {
            if let Some(notifier) = self.notifier.get() {
                let _ = notifier.send(());
            }
        }
        if let Some(lock) = self.cell.get() {
            let mut guard = lock.write().await;
            *guard = Some(value);
        }
        Ok(())
    }

    /// Take the value of the cell, leaving None in its place.
    pub async fn take(&self) -> Result<T, CellError> {
        if let Some(lock) = self.cell.get() {
            let mut guard = lock.write().await;
            #[cfg(feature = "watch")]
            {
                if let Some(notifier) = self.notifier.get() {
                    let _ = notifier.send(());
                }
            }
            guard.take()
                .ok_or(CellError::NotSet)
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Empty the cell, setting its value to None.
    pub async fn clear(&self) -> Result<(), CellError> {
        if let Some(lock) = self.cell.get() {
            let mut guard = lock.write().await;
            #[cfg(feature = "watch")]
            {
                if let Some(notifier) = self.notifier.get() {
                    let _ = notifier.send(());
                }
            }
            *guard = None;
            Ok(())
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Determine has been initialized.
    /// Even if the outer cell has been initialized, this does not guarantee that the inner value is set.
    /// Use is_set() instead to determine if there is actually an inner value set.
    pub fn is_init(&self) -> bool {
        self.cell.get().is_some()
    }

    /// Determine if the inner value is set.
    /// Returns false if the outer cell has not been initialized.
    pub async fn is_set(&self) -> bool {
        if let Some(lock) = self.cell.get() {
            let guard = lock.read().await;
            guard.is_some()
        } else {
            false
        }
    }
    
    /// Acquire a read lock on the cell.
    /// Returns an error if the cell is not initialized.
    pub async fn read(&self) -> Result<CellLock<'_, T>, CellError> {
        if let Some(lock) = self.cell.get() {
            let guard = lock.read().await;
            if guard.is_none() {
                return Err(CellError::NotSet);
            }
            Ok(CellLock::from(guard))
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Acquire a write lock on the cell.
    /// Returns an error if the cell is not initialized.
    pub async fn write(&self) -> Result<MutCellLock<'_, T>, CellError> {
        if let Some(lock) = self.cell.get() {
            let guard = lock.write().await;
            if guard.is_none() {
                return Err(CellError::NotSet);
            }
            Ok(MutCellLock::from(guard))
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Acquire a blocking read lock on the cell.
    pub fn block_read(&self) -> Result<CellLock<'_, T>, CellError> {
        if let Some(lock) = self.cell.get() {
            let guard = lock.blocking_read();
            if guard.is_none() {
                return Err(CellError::NotSet);
            }
            Ok(CellLock::from(guard))
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Acquire a blocking write lock on the cell.
    pub fn block_write(&self) -> Result<MutCellLock<'_, T>, CellError> {
        if let Some(lock) = self.cell.get() {
            let guard = lock.blocking_write();
            if guard.is_none() {
                return Err(CellError::NotSet);
            }
            Ok(MutCellLock::from(guard))
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Apply a function to the inner value, if it is set.
    /// Otherwise, return None.
    pub async fn with<F, R>(&self, func: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        if let Some(lock) = self.cell.get() {
            let guard = lock.read().await;
            guard.as_ref().map(func)
        } else {
            None
        }
    }

    /// Apply a mutable function to the inner value, if it is set.
    /// Otherwise, return None.
    pub async fn with_mut<F, R>(&self, func: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        if let Some(lock) = self.cell.get() {
            let mut guard = lock.write().await;
            guard.as_mut().map(func)
        } else {
            None
        }
    }

    /// Apply a function to the inner value, if it is set.
    /// Returns an error if the cell is not initialized.
    pub async fn try_with<F, R>(&self, func: F) -> Result<Option<R>, CellError>
    where
        F: FnOnce(&T) -> R,
    {
        if let Some(lock) = self.cell.get() {
            let guard = lock.read().await;
            Ok(guard.as_ref().map(func))
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Apply a mutable function to the inner value, if it is set.
    /// Returns an error if the cell is not initialized.
    pub async fn try_with_mut<F, R>(&self, func: F) -> Result<Option<R>, CellError>
    where
        F: FnOnce(&mut T) -> R,
    {
        if let Some(lock) = self.cell.get() {
            let mut guard = lock.write().await;
            Ok(guard.as_mut().map(func))
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Subscribe to changes to the cell.
    /// Returns a watch::Receiver that will receive a notification whenever the cell is changed.
    /// Returns an error if the cell is not initialized.
    #[cfg(feature = "watch")]
    pub fn subscribe(&self) -> Result<watch::Receiver<()>, CellError> {
        if let Some(notifier) = self.notifier.get() {
            Ok(notifier.subscribe())
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Peek at the inner value without acquiring a lock.
    /// Returns None if the cell is not initialized or if the inner value is not set.
    /// Requires T: Clone.
    pub fn get(&self) -> Option<T> 
    where
        T: Clone,
    {
        if let Some(lock) = self.cell.get() {
            let guard = lock.blocking_read();
            guard.as_ref().cloned()
        } else {
            None
        }
    }

    /// Peek at the inner value without acquiring a lock.
    /// Returns None if the cell is not initialized or if the inner value is not set.
    /// Requires T: Copy.
    /// This is more efficient than get() for Copy types.
    pub fn get_copy(&self) -> Option<T> 
    where
        T: Copy,
    {
        if let Some(lock) = self.cell.get() {
            let guard = lock.blocking_read();
            guard.as_ref().copied()
        } else {
            None
        }
    }
}
impl<T: Send + Sync + Default> Default for Cell<T> {
    fn default() -> Self {
        Self::from(T::default())
    }
}
#[cfg(feature = "serialize")]
impl<T: Send + Sync + Serialize> Serialize for Cell<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(lock) = self.cell.get() {
            let guard = lock.blocking_read();
            match &*guard {
                Some(value) => value.serialize(serializer),
                None => serializer.serialize_none(),
            }
        } else {
            serializer.serialize_none()
        }
    }
}
#[cfg(feature = "serialize")]
impl<'de, T: Send + Sync + Deserialize<'de>> Deserialize<'de> for Cell<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Option::<T>::deserialize(deserializer)?;
        if let Some(v) = value {
            Ok(Self::from(v))
        } else {
            Ok(Self::new())
        }
    }
}
impl<T: Send + Sync + std::fmt::Debug> std::fmt::Debug for Cell<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let lock = self.block_read();
        let mut binding = f.debug_struct("Cell");
        let dbg = binding
            .field("is_init", &self.is_init());
        if let Ok(guard) = lock {
            dbg.field("value", guard.get());
        }
        dbg.finish()
    }
}
use std::fmt::{ Display, Formatter };
use std::error::Error;
use std::sync::Arc;
use tokio::sync::{ OnceCell, RwLock };

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum CellError {
    NotSet,
    AlreadySet,
    Poisoned,
}
impl Display for CellError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CellError::NotSet => write!(f, "Cell is not set"),
            CellError::AlreadySet => write!(f, "Cell is already set"),
            CellError::Poisoned => write!(f, "Cell is poisoned"),
        }
    }
}
impl Error for CellError {}

/// CellLock: A lock acquired on a Cell when accessing.
/// Released when dropped.
pub struct CellLock<'a, T: Send + Sync> {
    guard: tokio::sync::RwLockReadGuard<'a, Option<T>>,
}
impl<'a, T: Send + Sync> CellLock<'a, T> {
    /// Get a reference to the value inside the cell.
    /// Returns None if the cell is not set.
    pub fn get(&self) -> Option<&T> {
        self.guard.as_ref()
    }
}

/// Cell: A global cell that stores a value of type T.
/// The cell can be accessed and modified from anywhere in the program.
/// The cell is thread-safe and can be used in multi-threaded environments.
/// The cell is implemented using a Mutex to ensure that only one thread can access the cell at a time.
pub struct Cell<T: Send + Sync> {
    cell: OnceCell<Arc<RwLock<Option<T>>>>,

}
impl<T: Send + Sync> Cell<T> {
    /// Creates a new empty Cell instance.
    pub const fn new() -> Self {
        Self {
            cell: OnceCell::const_new(),
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
        Ok(())
    }

    /// Sets the value of the cell; overrides the previous value if there was any.
    pub async fn set(&self, value: T) -> Result<(), CellError> {
        if let Some(lock) = self.cell.get() {
            let mut guard = lock.write().await;
            *guard = Some(value);
        } else {
            let lock = Arc::new(RwLock::new(Some(value)));
            self.cell.set(lock)
                .map_err(|_| CellError::Poisoned)?;
        }
        Ok(())
    }

    /// Take the value of the cell, leaving None in its place.
    pub async fn take(&self) -> Result<T, CellError> {
        if let Some(lock) = self.cell.get() {
            let mut guard = lock.write().await;
            guard.take()
                .ok_or(CellError::NotSet)
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
    pub async fn lock(&self) -> Result<CellLock<'_, T>, CellError> {
        if let Some(lock) = self.cell.get() {
            let guard = lock.read().await;
            Ok(CellLock { guard })
        } else {
            Err(CellError::NotSet)
        }
    }

    /// Acquire a write lock on the cell.
    /// Returns an error if the cell is not initialized.
    
}
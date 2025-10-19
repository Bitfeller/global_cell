use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum CellError {
    NotSet,
    AlreadySet,

    LockUnavailable,
    
    AlreadyInitialized,
    Initializing,
    InitializerBeingSet,
    Uninitialized,
    
    #[cfg(feature = "watch")]
    NotifierInitFailed,
}
impl Display for CellError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CellError::NotSet => write!(f, "Cell is not set"),
            CellError::AlreadySet => write!(f, "Cell is already set"),

            CellError::LockUnavailable => write!(f, "Cell lock is unavailable"),

            CellError::AlreadyInitialized => write!(f, "Cell is already initialized"),
            CellError::Initializing => write!(f, "Cell is currently initializing"),
            CellError::InitializerBeingSet => write!(f, "Cell initializer is being set"),
            CellError::Uninitialized => write!(f, "Cell is uninitialized"),

            #[cfg(feature = "watch")]
            CellError::NotifierInitFailed => write!(f, "Notifier initialization failed"),
        }
    }
}
impl Error for CellError {}
/// Defines a common set of behaviors for all cell-like structures that contain a value.
/// Different cell implementations may serve entirely different purposes, which is why
/// there are not many common behaviors.
pub const trait CellTrait<T> {
    /// Creates a new, uninitialized cell.
    fn new() -> Self where Self: Sized;
}

/// # ManagedCell
/// 
/// Often times, performant bare-bones Cell types may not respect value boundaries.
/// That is, internally, they may hold multiple mutable references (similar to UnsafeCell),
/// or may allow for uninitialized values to be read.
/// 
/// These cases are not stable for the caller, in which they are expecting the cell to respect
/// and manage the initialization and validity of the value contained within.
/// ManagedCell describes any cell that respects the boundaries and memory constraints of the value;
/// but they may not be the most performant implementations.
/// 
/// ManagedCell, therefore, also offers another common behavior: that all will implement
/// some form of initialization method to ensure the value is valid, as well as a method
/// of determining whether the cell is initialized.
pub trait ManagedCell<T>: CellTrait<T> {
    /// Initializes the cell with a value, applying any necessary transformations or validations.
    fn init(&self, value: T);

    /// Checks if the cell is initialized.
    fn is_initialized(&self) -> bool;
}
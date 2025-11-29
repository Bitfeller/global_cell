# Global Cell
A thread-safe, async-friendly global mutable cell implementation for Rust, built on primitives.

## Features
- **Thread-safe**: Built to be shared and accessed quickly between threads
- **Async-friendly**: Designed on async/await patterns with most async platforms
- **Global state**: Can be used as a static global variable, especially with the featured OnceCell
- **Easy mutability**: Easily read/write access ensures type + cell safety at compile-time

## Installation
Install the binary and utilize.
The crate will be shortly published to crates.io!

## Usage

### Basic Usage
```rust
use global_cell::Cell;

static MY_CELL: Cell<u32> = Cell::new();

#[tokio::main]
async fn main() {
    // Set the cell value
    MY_CELL.set(42);

    // Read the value
    MY_CELL.with(|value| {
        println!("Value: {}", value);
    });
}
```

### Working with Mutable Data
```rust
use global_cell::Cell;

#[tokio::main]
async fn main() {
    let cell = Cell::new();
    cell.set("Hello".to_string());

    // Get a write lock and modify
    let mut lock = cell.write_async().await;
    lock.push_str(" world");
    drop(lock); // lock.drop() is also available as a shortcut

    // Read the modified value
    let lock = cell.read();
    assert_eq!(*lock, "Hello world");
}
```

### Using `with_mut` for Modifications
```rust
use global_cell::Cell;

#[tokio::main]
async fn main() {
    let cell = Cell::new();
    cell.set(vec![1, 2, 3]);

    // Modify the value in place
    cell.with_mut(|vec| {
        vec.push(4);
    });
}
```

### Using locks to capture and/or modify
```rust
use global_cell::Cell;

#[tokio::main]
async fn main() {
    let cell = Cell::new();
    cell.set(vec![1, 2, 3]);

    let lock = cell.read();
    println!("{}", lock[0]);

    // Drop the lock so that we can acquire a write lock
    drop(lock);

    let write_lock = cell.write();
    lock.push(4);
}
```

## API Overview (for most cell types, including Cell)

### Cell Operations
- `new()` - (const) Create a new empty cell
- `from(value)` - (const) Create a cell with an initial value
- `set(value)` - Set or update the cell value
- `cloned()` - Get a clone of the value (requires `T: Clone`)
- `copy()` - Get a copy of the value (requires `T: Copy`)
- `take()` - for Option<T>: take the inner value, leaving `None`
- `clear()` - for Option<T>: clear the cell value

### Locking
- `read()` - Acquire a blocking read lock
- `write()` - Acquire a blocking write lock
- `read_async()` - Acquire an async read lock
- `write_async()` - Acquire an async write lock

### Functional Access
- `with(func)` - Apply a function to the value
- `with_mut(func)` - Apply a function that requires a write lock to the value
- `with_async(func)` - Apply an async function to the value, acquiring an async lock
- `with_mut_async(func)` - Apply an async function that requires a write lock to the value, acquiring an async lock

### Status
- `is_initialized()` - Check if the cell is initialized
- `is_init()` - synonym for is_initialized

## License
This project is licensed under the MIT License or Apache License 2.0, at your option.

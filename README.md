# Global Cell
A thread-safe, async-friendly global mutable cell implementation for Rust, built on Tokio primitives.

## Features
- **Thread-safe**: Built to be shared and accessed quickly between threads
- **Async-friendly**: Designed on async/await patterns with Tokio
- **Global state**: Can be used as a static global variable
- **Easy mutability**: Easily read/write access ensures type + cell safety at compile-time
- **Optional features**:
  - `serialize`: Serde serialization support
  - `watch`: Subscribe to cell changes with watch channels

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
    MY_CELL.set(42).await.unwrap();

    // Read the value
    MY_CELL.with(|value| {
        println!("Value: {}", value);
    }).await;
}
```

### Working with Mutable Data
```rust
use global_cell::Cell;

#[tokio::main]
async fn main() {
    let cell = Cell::new();
    cell.set("Hello".to_string()).await.unwrap();

    // Get a write lock and modify
    let mut lock = cell.write().await.unwrap();
    lock.push_str(" world");
    drop(lock);

    // Read the modified value
    let lock = cell.read().await.unwrap();
    assert_eq!(*lock, "Hello world");
}
```

### Using `with_mut` for Modifications
```rust
use global_cell::Cell;

#[tokio::main]
async fn main() {
    let cell = Cell::new();
    cell.set(vec![1, 2, 3]).await.unwrap();

    // Modify the value in place
    cell.with_mut(|vec| {
        vec.push(4);
    }).await;
}
```

### Using locks to capture and/or modify
```rust
use global_cell::Cell;

#[tokio::main]
async fn main() {
    let cell = Cell::new();
    cell.set(vec![1, 2, 3]).await.unwrap();

    let lock = cell.read().await.unwrap();
    println!("{}", lock[0]);

    // Drop the lock so that we can acquire a write lock
    drop(lock);

    let write_lock = cell.write().await.unwrap();
    lock.push(4);
}
```

## API Overview

### Cell Operations
- `new()` - Create a new empty cell
- `from(value)` - Create a cell with an initial value
- `set(value)` - Set or update the cell value
- `get()` - Get a clone of the value (requires `T: Clone`)
- `get_copy()` - Get a copy of the value (requires `T: Copy`)
- `take()` - Take the value, leaving `None`
- `clear()` - Clear the cell value

### Locking
- `read()` - Acquire an async read lock
- `write()` - Acquire an async write lock
- `block_read()` - Acquire a blocking read lock
- `block_write()` - Acquire a blocking write lock

### Functional Access
- `with(func)` - Apply a function to the value
- `with_mut(func)` - Apply a mutable function to the value
- `try_with(func)` - Like `with`, but returns `Result`
- `try_with_mut(func)` - Like `with_mut`, but returns `Result`

### Status
- `is_init()` - Check if the cell is initialized
- `is_set()` - Check if a value is set

### Watch (with `watch` feature)
- `subscribe()` - Subscribe to cell changes

## Features
Enable optional features in your `Cargo.toml`:

```toml
[dependencies]
global_cell = { version = "0.1.0", features = ["serialize", "watch"] }
```

## License
This project is licensed under the MIT License or Apache License 2.0, at your option.

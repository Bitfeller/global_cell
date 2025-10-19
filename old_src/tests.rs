use crate::Cell;

static GLOBAL_CELL: Cell<u32> = Cell::new();
static OVERWRITE_CELL: Cell<u32> = Cell::new();

#[tokio::test]
async fn test_global() {
    GLOBAL_CELL.set(42).await.unwrap();

    GLOBAL_CELL
        .with(|cell| {
            assert_eq!(*cell, 42);
        })
        .await;
}

#[tokio::test]
async fn test_overwrite() {
    GLOBAL_CELL.set(10).await.unwrap();
    GLOBAL_CELL.set(20).await.unwrap();

    GLOBAL_CELL
        .with(|cell| {
            assert_eq!(*cell, 20);
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn inner_cell() {
    let inner_cell = Cell::new();
    inner_cell.set("Hello".to_string()).await.unwrap();

    let mut lock = inner_cell.write().await.unwrap();

    // Modify the inner value
    lock.push_str(" world");
    drop(lock);

    let lock = inner_cell.read().await.unwrap();
    assert_eq!(*lock, "Hello world");
}

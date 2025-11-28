use crate::RawCell;

#[tokio::test]
async fn dbg_multi_init() {
    static RAW_CELL: RawCell<u32> = RawCell::<u32>::new();
    // spawn multiple tasks to test concurrent initialization
    let handles: Vec<_> = (0..150)
        .map(|_| {
            let cell_ref = &RAW_CELL;
            tokio::spawn(async move {
                cell_ref.init_async(|| async {
                    42
                }).await;
            })
        })
        .collect();

    // Wait for all tasks to complete
    for handle in handles {
        let _ = handle.await;
    }
    // Verify initialization
    unsafe {
        let value = RAW_CELL.inner().read();
        assert_eq!(*value, 42);
    }
}

#[tokio::test]
async fn dbg_build_fail() {
    static RAW_CELL: RawCell<u32> = RawCell::<u32>::new();
    let init_fut = RAW_CELL.init_async(|| async {
        // Simulate a failure during initialization
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        panic!("Initialization failed");
    });

    let result = tokio::spawn(init_fut).await;
    assert!(result.is_err(), "Expected initialization to fail");
    assert!(!RAW_CELL.is_initialized(), "RawCell should not be initialized after failure");

    // make sure we can re-initialize after failure
    RAW_CELL.init_async(|| async { 100 }).await;
    unsafe {
        let value = RAW_CELL.inner().read();
        assert_eq!(*value, 100, "RawCell should be initialized to 100 after re-initialization");
    }
}
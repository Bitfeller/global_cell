use tokio::sync::{OnceCell, RwLock};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use crate::raw_cell::RawCell;

static RAW_CELL: RawCell<u32> = RawCell::new();

/// To see the stdout output, run with `cargo test -- --nocapture`
#[tokio::test]
async fn dbg_compare_speeds() {
    RAW_CELL.init(|| { 0u32 });
    let arc = unsafe { RAW_CELL.inner() };
    for i in 0..10000 {
        let mut lock = arc.write();
        *lock = i;
    }
}

/// To see the stdout output, run with `cargo test -- --nocapture`
#[tokio::test]
async fn dbg_read_speeds() {
    let raw_cell: RawCell<u32> = RawCell::new();

    raw_cell.init(|| { 0u32 });
    let arc = unsafe { raw_cell.inner() };
    for _ in 0..100000 {
        let read_guard = arc.read();
        let _value = *read_guard;
    }
}

#[tokio::test]
async fn dbg_compare_mass_init_speeds() {
    // We are going to compare initialization times here, but also include the time taken to read the value after initialization.
    let trials = 10000;
    let raw_won = Arc::new(AtomicUsize::new(0));
    let old_won = Arc::new(AtomicUsize::new(0));
    let ties = Arc::new(AtomicUsize::new(0));
    for _ in 0..trials {
        let raw_cell: RawCell<u32> = RawCell::new();
        let old_cell: OnceCell<Arc<RwLock<Option<u32>>>> = OnceCell::const_new();

        let raw_start = tokio::time::Instant::now();
        raw_cell.init(|| { 42u32 });
        let arc = unsafe { raw_cell.inner() };
        let _ = *arc.read();
        let raw_duration = raw_start.elapsed();

        let old_start = tokio::time::Instant::now();
        old_cell
            .get_or_init(|| async { Arc::new(RwLock::new(Some(42u32))) })
            .await;
        let old_cell_ref = old_cell.get().unwrap();
        let _ = *old_cell_ref.read().await;
        let old_duration = old_start.elapsed();

        if raw_duration < old_duration {
            raw_won.fetch_add(1, Ordering::Relaxed);
        } else if raw_duration > old_duration {
            old_won.fetch_add(1, Ordering::Relaxed);
        } else {
            ties.fetch_add(1, Ordering::Relaxed);
        }
    }

    let raw_wins = raw_won.load(Ordering::Relaxed);
    let old_wins = old_won.load(Ordering::Relaxed);
    let tie_count = ties.load(Ordering::Relaxed);
    println!("RAW_CELL wins: {}, ONCE_CELL wins: {}; TIES: {}", raw_wins, old_wins, tie_count);
    println!(
        "RAW_CELL win rate: {:.2}%, ONCE_CELL win rate: {:.2}%",
        (raw_wins as f64 / trials as f64) * 100.0,
        (old_wins as f64 / trials as f64) * 100.0
    );
}

/// To see the stdout output, run with `cargo test -- --nocapture`
#[tokio::test]
async fn dbg_compare_init_speeds() {
    // We are going to STRICTLY compare initialization times here.
    let trials = 100000;
    let raw_won = Arc::new(AtomicUsize::new(0));
    let old_won = Arc::new(AtomicUsize::new(0));
    let ties = Arc::new(AtomicUsize::new(0));
    for _ in 0..trials {
        let raw_cell: RawCell<u32> = RawCell::new();
        let old_cell: OnceCell<Arc<RwLock<Option<u32>>>> = OnceCell::const_new();

        let raw_start = tokio::time::Instant::now();
        raw_cell.init(|| { 42u32 });
        let raw_duration = raw_start.elapsed();

        let old_start = tokio::time::Instant::now();
        old_cell
            .get_or_init(|| async { Arc::new(RwLock::new(Some(42u32))) })
            .await;
        let old_duration = old_start.elapsed();

        if raw_duration < old_duration {
            raw_won.fetch_add(1, Ordering::Relaxed);
        } else if raw_duration > old_duration {
            old_won.fetch_add(1, Ordering::Relaxed);
        } else {
            ties.fetch_add(1, Ordering::Relaxed);
        }
    }

    let raw_wins = raw_won.load(Ordering::Relaxed);
    let old_wins = old_won.load(Ordering::Relaxed);
    let tie_count = ties.load(Ordering::Relaxed);
    println!("RAW_CELL wins: {}, ONCE_CELL wins: {}; TIES: {}", raw_wins, old_wins, tie_count);
    println!(
        "RAW_CELL win rate: {:.2}%, ONCE_CELL win rate: {:.2}%",
        (raw_wins as f64 / trials as f64) * 100.0,
        (old_wins as f64 / trials as f64) * 100.0
    );
}

#[tokio::test]
async fn dbg_raw_cell_concurrent_init() {
    let raw_cell: Arc<RawCell<u32>> = Arc::new(RawCell::new());
    let mut handles = vec![];
    for _ in 0..10 {
        let cell_clone = raw_cell.clone();
        let handle = tokio::spawn(async move {
            cell_clone.init(|| { 99u32 });
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.await.unwrap();
    }
    let arc = unsafe { raw_cell.inner() };
    let read_guard = arc.read();
    assert_eq!(*read_guard, 99u32);
}
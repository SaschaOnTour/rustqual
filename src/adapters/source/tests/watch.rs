use crate::adapters::source::watch::{create_file_watcher, run_watch_loop};
use std::sync::mpsc;

#[test]
fn test_run_watch_loop_calls_on_change_initially() {
    let (_tx, rx) = mpsc::channel();
    let mut called = false;
    // Drop tx so recv() returns Err immediately, breaking the loop
    drop(_tx);
    run_watch_loop(rx, || {
        called = true;
    });
    assert!(called, "on_change should be called initially");
}

#[test]
fn test_run_watch_loop_calls_on_change_on_event() {
    let (tx, rx) = mpsc::channel();
    let mut call_count = 0;
    // Send one event, then drop tx to break the loop after processing
    tx.send(()).unwrap();
    drop(tx);
    run_watch_loop(rx, || {
        call_count += 1;
    });
    // Initial call + one event = 2 calls
    assert_eq!(call_count, 2);
}

#[test]
fn test_create_file_watcher_valid_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let result = create_file_watcher(tmp.path());
    assert!(result.is_ok());
}

#[test]
fn test_create_file_watcher_returns_result() {
    let tmp = tempfile::TempDir::new().unwrap();
    let (rx, _watcher) = create_file_watcher(tmp.path()).unwrap();
    // Verify we get the receiver back
    assert!(rx.try_recv().is_err()); // no events yet
}

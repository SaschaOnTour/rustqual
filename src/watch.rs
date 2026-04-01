use std::path::Path;

use crate::config::Config;
use crate::pipeline::analyze_and_output;

/// Debounce delay in milliseconds between watch re-analyses.
const DEBOUNCE_MS: u64 = 500;

/// Create a file watcher for the given path.
/// Operation: error handling logic for watcher creation.
pub(crate) fn create_file_watcher(
    path: &Path,
) -> Result<(std::sync::mpsc::Receiver<()>, notify::RecommendedWatcher), i32> {
    use notify::{RecursiveMode, Watcher};

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove() {
                    let _ = tx.send(());
                }
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Error starting file watcher: {e}");
                return Err(1);
            }
        };

    if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
        eprintln!("Error watching path: {e}");
        return Err(1);
    }

    Ok((rx, watcher))
}

/// Event loop: wait for file changes, call the handler on each.
/// Operation: loop + channel logic, calls on_change parameter (not own call).
fn run_watch_loop(rx: std::sync::mpsc::Receiver<()>, mut on_change: impl FnMut()) {
    on_change();
    loop {
        if rx.recv().is_err() {
            break;
        }
        // Drain queued events
        while rx.try_recv().is_ok() {}
        std::thread::sleep(std::time::Duration::from_millis(DEBOUNCE_MS));
        while rx.try_recv().is_ok() {}
        eprintln!("\n--- Re-analyzing... ---");
        on_change();
    }
}

/// Run watch mode: re-analyze on file changes.
/// Integration: orchestrates create_file_watcher, run_watch_loop, analyze_and_output.
pub(crate) fn run_watch_mode(
    cli: &super::Cli,
    config: &Config,
    output_format: &super::OutputFormat,
) -> Result<(), i32> {
    eprintln!(
        "Watching {} for changes... (Ctrl+C to stop)",
        cli.path.display()
    );
    let (rx, _watcher) = create_file_watcher(&cli.path)?;
    run_watch_loop(rx, || {
        analyze_and_output(
            &cli.path,
            config,
            output_format,
            cli.verbose,
            cli.suggestions,
        );
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{create_file_watcher, run_watch_loop};
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
}

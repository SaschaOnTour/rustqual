use std::path::Path;

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
pub(crate) fn run_watch_loop(rx: std::sync::mpsc::Receiver<()>, mut on_change: impl FnMut()) {
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

/// Run watch mode: wire a watcher to `path` and invoke `on_change` on every event.
/// Integration: orchestrates create_file_watcher and run_watch_loop.
pub(crate) fn run_watch_mode(path: &Path, on_change: impl FnMut()) -> Result<(), i32> {
    eprintln!(
        "Watching {} for changes... (Ctrl+C to stop)",
        path.display()
    );
    let (rx, _watcher) = create_file_watcher(path)?;
    run_watch_loop(rx, on_change);
    Ok(())
}

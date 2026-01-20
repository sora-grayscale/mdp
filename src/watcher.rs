use notify::RecursiveMode;
use notify_debouncer_mini::{DebouncedEventKind, new_debouncer};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tokio::sync::broadcast;

/// Watch a file for changes and send notifications
pub fn watch_file<P: AsRef<Path>>(path: P, tx: broadcast::Sender<()>) -> notify::Result<()> {
    let path = path.as_ref().to_path_buf();
    let (debounce_tx, debounce_rx) = channel();

    // Create a debouncer with 200ms delay
    let mut debouncer = new_debouncer(Duration::from_millis(200), debounce_tx)?;

    // Watch the file
    debouncer
        .watcher()
        .watch(&path, RecursiveMode::NonRecursive)?;

    println!("Watching for changes: {}", path.display());

    // Process events
    loop {
        match debounce_rx.recv() {
            Ok(Ok(events)) => {
                for event in events {
                    if event.kind == DebouncedEventKind::Any {
                        println!("File changed, reloading...");
                        let _ = tx.send(());
                    }
                }
            }
            Ok(Err(e)) => {
                eprintln!("Watch error: {:?}", e);
            }
            Err(e) => {
                eprintln!("Channel error: {:?}", e);
                break;
            }
        }
    }

    // Keep debouncer alive
    drop(debouncer);
    Ok(())
}

/// Watch a file asynchronously using tokio
pub async fn watch_file_async<P: AsRef<Path>>(
    path: P,
    tx: broadcast::Sender<()>,
) -> notify::Result<()> {
    let path = path.as_ref().to_path_buf();

    println!("Watching for changes: {}", path.display());

    // Spawn blocking task for file watching - debouncer must live inside the blocking task
    tokio::task::spawn_blocking(move || {
        let (debounce_tx, debounce_rx) = channel();

        // Create a debouncer with 200ms delay
        let mut debouncer = match new_debouncer(Duration::from_millis(200), debounce_tx) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to create debouncer: {}", e);
                return;
            }
        };

        // Watch the file
        if let Err(e) = debouncer
            .watcher()
            .watch(&path, RecursiveMode::NonRecursive)
        {
            eprintln!("Failed to watch file: {}", e);
            return;
        }

        loop {
            match debounce_rx.recv() {
                Ok(Ok(events)) => {
                    for event in events {
                        if event.kind == DebouncedEventKind::Any {
                            println!("File changed, reloading...");
                            let _ = tx.send(());
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("Watch error: {:?}", e);
                }
                Err(_) => {
                    break;
                }
            }
        }

        // Keep debouncer alive until loop exits
        drop(debouncer);
    });

    Ok(())
}

/// Watch a directory recursively for .md file changes
pub async fn watch_directory_async<P: AsRef<Path>>(
    path: P,
    tx: broadcast::Sender<()>,
) -> notify::Result<()> {
    let path = path.as_ref().to_path_buf();

    println!("Watching directory for changes: {}", path.display());

    // Spawn blocking task for directory watching
    tokio::task::spawn_blocking(move || {
        let (debounce_tx, debounce_rx) = channel();

        // Create a debouncer with 200ms delay
        let mut debouncer = match new_debouncer(Duration::from_millis(200), debounce_tx) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to create debouncer: {}", e);
                return;
            }
        };

        // Watch the directory recursively
        if let Err(e) = debouncer.watcher().watch(&path, RecursiveMode::Recursive) {
            eprintln!("Failed to watch directory: {}", e);
            return;
        }

        loop {
            match debounce_rx.recv() {
                Ok(Ok(events)) => {
                    // Filter for markdown files only
                    let md_changed = events.iter().any(|e| {
                        e.kind == DebouncedEventKind::Any
                            && e.path
                                .extension()
                                .is_some_and(|ext| ext == "md" || ext == "markdown")
                    });

                    if md_changed {
                        println!("Markdown file changed, reloading...");
                        let _ = tx.send(());
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("Watch error: {:?}", e);
                }
                Err(_) => {
                    break;
                }
            }
        }

        drop(debouncer);
    });

    Ok(())
}

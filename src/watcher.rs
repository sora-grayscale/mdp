use notify::RecursiveMode;
use notify_debouncer_mini::{DebouncedEventKind, new_debouncer};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Duration;
use tokio::sync::broadcast;

use crate::server::{ServerState, WsMessage};

/// Watch a file for changes and send notifications
/// Watches the parent directory to handle editors that replace files (vim, etc.)
pub fn watch_file<P: AsRef<Path>>(path: P, tx: broadcast::Sender<()>) -> notify::Result<()> {
    let path = path
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| path.as_ref().to_path_buf());
    let parent = path.parent().unwrap_or(&path).to_path_buf();
    let file_name = path.file_name().map(|n| n.to_os_string());

    let (debounce_tx, debounce_rx) = channel();

    // Create a debouncer with 200ms delay
    let mut debouncer = new_debouncer(Duration::from_millis(200), debounce_tx)?;

    // Watch the parent directory to handle file replacement
    debouncer
        .watcher()
        .watch(&parent, RecursiveMode::NonRecursive)?;

    println!("Watching for changes: {}", path.display());

    // Process events
    loop {
        match debounce_rx.recv() {
            Ok(Ok(events)) => {
                // Filter events for the target file only
                let has_target_event = events.iter().any(|e| {
                    e.kind == DebouncedEventKind::Any
                        && e.path.file_name().map(|n| n.to_os_string()) == file_name
                });

                if has_target_event {
                    println!("File changed, reloading...");
                    let _ = tx.send(());
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
/// Watches the parent directory to handle editors that replace files (vim, etc.)
pub async fn watch_file_async<P: AsRef<Path>>(
    path: P,
    tx: broadcast::Sender<WsMessage>,
) -> notify::Result<()> {
    let path = path
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| path.as_ref().to_path_buf());
    let parent = path.parent().unwrap_or(&path).to_path_buf();
    let file_name = path.file_name().map(|n| n.to_os_string());

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

        // Watch the parent directory to handle file replacement
        if let Err(e) = debouncer
            .watcher()
            .watch(&parent, RecursiveMode::NonRecursive)
        {
            eprintln!("Failed to watch directory: {}", e);
            return;
        }

        loop {
            match debounce_rx.recv() {
                Ok(Ok(events)) => {
                    // Filter events for the target file only
                    let has_target_event = events.iter().any(|e| {
                        e.kind == DebouncedEventKind::Any
                            && e.path.file_name().map(|n| n.to_os_string()) == file_name
                    });

                    if has_target_event {
                        println!("File changed, reloading...");
                        let _ = tx.send(WsMessage::Reload);
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

/// Watch a directory recursively for .md file changes with tree update support
pub async fn watch_directory_with_tree_update<P: AsRef<Path>>(
    path: P,
    tx: broadcast::Sender<WsMessage>,
    state: Arc<ServerState>,
) -> notify::Result<()> {
    let path = path.as_ref().to_path_buf();

    println!("Watching directory for changes: {}", path.display());

    // Get initial file paths for comparison (detects renames, not just count changes)
    let initial_paths: HashSet<String> = {
        let tree = state.file_tree.read().await;
        tree.files
            .iter()
            .map(|f| f.relative_path.to_string_lossy().to_string())
            .collect()
    };

    // Create channel for sending events from blocking thread to async handler
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<bool>(16);

    // Spawn blocking task for directory watching (only file system operations)
    let path_clone = path.clone();
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
        if let Err(e) = debouncer
            .watcher()
            .watch(&path_clone, RecursiveMode::Recursive)
        {
            eprintln!("Failed to watch directory: {}", e);
            return;
        }

        loop {
            match debounce_rx.recv() {
                Ok(Ok(events)) => {
                    // Filter for markdown files only
                    let has_md_events = events.iter().any(|e| {
                        e.kind == DebouncedEventKind::Any
                            && e.path
                                .extension()
                                .is_some_and(|ext| ext == "md" || ext == "markdown")
                    });

                    if has_md_events {
                        // Send event to async handler (non-blocking)
                        if event_tx.blocking_send(true).is_err() {
                            break;
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

        drop(debouncer);
    });

    // Async handler for processing events (runs on async runtime, not blocking pool)
    let mut last_paths = initial_paths;
    tokio::spawn(async move {
        while event_rx.recv().await.is_some() {
            // Rebuild file tree and get new file paths
            if let Err(e) = state.rebuild_file_tree().await {
                eprintln!("Failed to rebuild file tree: {}", e);
                continue;
            }

            let new_paths: HashSet<String> = {
                let tree = state.file_tree.read().await;
                tree.files
                    .iter()
                    .map(|f| f.relative_path.to_string_lossy().to_string())
                    .collect()
            };

            // Check if file paths changed (handles add, remove, and rename)
            if new_paths != last_paths {
                println!(
                    "File tree changed ({} -> {} files), updating sidebar...",
                    last_paths.len(),
                    new_paths.len()
                );
                let _ = tx.send(WsMessage::TreeUpdate);
                last_paths = new_paths;
            } else {
                // Just content changed
                println!("Markdown file changed, reloading...");
                let _ = tx.send(WsMessage::Reload);
            }
        }
    });

    Ok(())
}

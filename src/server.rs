use axum::{
    Json, Router,
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{RwLock, broadcast};

use crate::files::FileTree;
use crate::renderer::html::HtmlRenderer;
use crate::watcher::watch_file_async;

/// Timeout in seconds before shutting down when all clients disconnect
const SHUTDOWN_TIMEOUT_SECS: u64 = 3;

#[derive(Serialize)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Serialize)]
pub struct FileListResponse {
    pub files: Vec<FileInfo>,
    pub base_path: String,
}

#[derive(Deserialize)]
pub struct ViewQuery {
    pub file: Option<String>,
}

/// Message types for WebSocket communication
#[derive(Clone, Debug)]
pub enum WsMessage {
    Reload,
    TreeUpdate,
}

pub struct ServerState {
    pub file_tree: RwLock<FileTree>,
    pub base_path: PathBuf,
    pub title: String,
    pub reload_tx: broadcast::Sender<WsMessage>,
    pub shutdown_tx: broadcast::Sender<()>,
    pub connection_count: AtomicUsize,
    pub show_toc: bool,
}

impl ServerState {
    async fn render_html(&self, file_path: Option<&str>) -> String {
        // Get file info while holding lock briefly
        let (absolute_path, relative_path, is_single_file, file_tree_clone) = {
            let file_tree = self.file_tree.read().await;
            let file = if let Some(path) = file_path {
                file_tree.find_file(path)
            } else {
                file_tree.default_file()
            };

            if let Some(f) = file {
                (
                    Some(f.absolute_path.clone()),
                    Some(f.relative_path.to_string_lossy().to_string()),
                    file_tree.is_single_file(),
                    if file_tree.is_single_file() {
                        None
                    } else {
                        Some(file_tree.clone())
                    },
                )
            } else {
                (None, None, file_tree.is_single_file(), None)
            }
        };
        // Lock released here, now do I/O

        let (content, current_file) = if let Some(path) = absolute_path {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            (content, relative_path)
        } else {
            ("# No file selected".to_string(), None)
        };

        let renderer = HtmlRenderer::new(&self.title).with_toc(self.show_toc);

        if is_single_file {
            renderer.render(&content)
        } else if let Some(tree) = file_tree_clone {
            renderer.render_with_sidebar(&content, &tree, current_file.as_deref())
        } else {
            renderer.render(&content)
        }
    }

    async fn render_content_only(&self, file_path: &str) -> Option<String> {
        // Get file path while holding lock briefly
        let absolute_path = {
            let file_tree = self.file_tree.read().await;
            file_tree.find_file(file_path)?.absolute_path.clone()
        };
        // Lock released here, now do I/O

        let content = std::fs::read_to_string(&absolute_path).ok()?;
        let renderer = HtmlRenderer::new(&self.title).with_toc(self.show_toc);
        Some(renderer.render_content(&content))
    }

    /// Rebuild the file tree from the base path
    pub async fn rebuild_file_tree(&self) -> Result<(), std::io::Error> {
        let new_tree = FileTree::from_directory(&self.base_path)?;
        let mut file_tree = self.file_tree.write().await;
        *file_tree = new_tree;
        Ok(())
    }
}

pub async fn start_server(
    file_tree: FileTree,
    title: &str,
    port: u16,
    watch: bool,
    show_toc: bool,
) -> std::io::Result<()> {
    let (reload_tx, _) = broadcast::channel::<WsMessage>(16);
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);

    let base_path = file_tree.base_path.clone();
    let is_single_file = file_tree.is_single_file();

    let state = Arc::new(ServerState {
        file_tree: RwLock::new(file_tree.clone()),
        base_path: base_path.clone(),
        title: title.to_string(),
        reload_tx: reload_tx.clone(),
        shutdown_tx: shutdown_tx.clone(),
        connection_count: AtomicUsize::new(0),
        show_toc,
    });

    // Start file watcher if watch mode is enabled
    if watch {
        if is_single_file {
            // Watch single file
            if let Some(file) = file_tree.default_file() {
                let watch_path = file.absolute_path.clone();
                let watch_tx = reload_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = watch_file_async(&watch_path, watch_tx).await {
                        eprintln!("Failed to start file watcher: {}", e);
                    }
                });
            }
        } else {
            // Watch entire directory with tree update support
            let watch_path = base_path.clone();
            let watch_tx = reload_tx.clone();
            let watch_state = state.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::watcher::watch_directory_with_tree_update(
                    &watch_path,
                    watch_tx,
                    watch_state,
                )
                .await
                {
                    eprintln!("Failed to start directory watcher: {}", e);
                }
            });
        }
    }

    let app = Router::new()
        .route("/", get(serve_html))
        .route("/view", get(serve_html))
        .route("/api/files", get(serve_file_list))
        .route("/api/content", get(serve_content))
        .route("/assets/github.css", get(serve_css))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    println!("Server running at http://{}", addr);
    if watch {
        println!("Live reload enabled - changes will auto-refresh");
    }
    println!("Press Ctrl+C to stop (or close browser tab)");

    // Open browser
    if let Err(e) = open::that(format!("http://{}", addr)) {
        eprintln!("Failed to open browser: {}", e);
        println!("Please open http://{} in your browser", addr);
    }

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Wait for shutdown signal
            let _ = shutdown_rx.recv().await;
            println!("\nShutting down server...");
        })
        .await?;

    Ok(())
}

async fn serve_html(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<ViewQuery>,
) -> (HeaderMap, Html<String>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    (
        headers,
        Html(state.render_html(query.file.as_deref()).await),
    )
}

async fn serve_file_list(State(state): State<Arc<ServerState>>) -> Json<FileListResponse> {
    let file_tree = state.file_tree.read().await;
    let files = file_tree
        .files
        .iter()
        .map(|f| FileInfo {
            path: f.relative_path.to_string_lossy().to_string(),
            name: f.name.clone(),
            is_dir: false,
        })
        .collect();

    Json(FileListResponse {
        files,
        base_path: file_tree.base_path.to_string_lossy().to_string(),
    })
}

#[derive(Deserialize)]
pub struct ContentQuery {
    pub file: String,
}

async fn serve_content(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<ContentQuery>,
) -> Response {
    match state.render_content_only(&query.file).await {
        Some(content) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            headers.insert(
                header::CONTENT_TYPE,
                "text/html; charset=utf-8".parse().unwrap(),
            );
            (headers, content).into_response()
        }
        None => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}

async fn serve_css() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css")],
        HtmlRenderer::get_css(),
    )
        .into_response()
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<ServerState>>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<ServerState>) {
    // Increment connection count
    state.connection_count.fetch_add(1, Ordering::SeqCst);

    let mut rx = state.reload_tx.subscribe();

    // Send initial connection confirmation
    let _ = socket.send(Message::Text("connected".to_string())).await;

    loop {
        tokio::select! {
            // Wait for reload/tree-update signal
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        let msg_text = match msg {
                            WsMessage::Reload => "reload",
                            WsMessage::TreeUpdate => "tree-update",
                        };
                        if socket.send(Message::Text(msg_text.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            // Handle incoming messages (for ping/pong)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    // Decrement connection count
    let prev_count = state.connection_count.fetch_sub(1, Ordering::SeqCst);

    // If this was the last connection, start shutdown timer
    if prev_count == 1 {
        let shutdown_tx = state.shutdown_tx.clone();
        let state_for_timer = state.clone();

        tokio::spawn(async move {
            // Wait for timeout
            tokio::time::sleep(tokio::time::Duration::from_secs(SHUTDOWN_TIMEOUT_SECS)).await;

            // Check if still no connections
            if state_for_timer.connection_count.load(Ordering::SeqCst) == 0 {
                println!("All browser tabs closed. Shutting down...");
                let _ = shutdown_tx.send(());
            }
        });
    }
}

/// Find an available port starting from the given port
pub fn find_available_port(start_port: u16) -> u16 {
    for port in start_port..start_port + 100 {
        if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok() {
            return port;
        }
    }
    start_port
}

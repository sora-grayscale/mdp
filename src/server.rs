use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::files::FileTree;
use crate::renderer::html::HtmlRenderer;
use crate::watcher::watch_file_async;

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

pub struct ServerState {
    pub file_tree: FileTree,
    pub title: String,
    pub reload_tx: broadcast::Sender<()>,
}

impl ServerState {
    fn render_html(&self, file_path: Option<&str>) -> String {
        let file = if let Some(path) = file_path {
            self.file_tree.find_file(path)
        } else {
            self.file_tree.default_file()
        };

        let (content, current_file) = if let Some(f) = file {
            let content = std::fs::read_to_string(&f.absolute_path).unwrap_or_default();
            (content, Some(f.relative_path.to_string_lossy().to_string()))
        } else {
            ("# No file selected".to_string(), None)
        };

        let renderer = HtmlRenderer::new(&self.title);

        if self.file_tree.is_single_file() {
            renderer.render(&content)
        } else {
            renderer.render_with_sidebar(&content, &self.file_tree, current_file.as_deref())
        }
    }

    fn render_content_only(&self, file_path: &str) -> Option<String> {
        let file = self.file_tree.find_file(file_path)?;
        let content = std::fs::read_to_string(&file.absolute_path).ok()?;
        let renderer = HtmlRenderer::new(&self.title);
        Some(renderer.render_content(&content))
    }
}

pub async fn start_server(
    file_tree: FileTree,
    title: &str,
    port: u16,
    watch: bool,
) -> std::io::Result<()> {
    let (reload_tx, _) = broadcast::channel::<()>(16);

    let state = Arc::new(ServerState {
        file_tree: file_tree.clone(),
        title: title.to_string(),
        reload_tx: reload_tx.clone(),
    });

    // Start file watcher if watch mode is enabled
    if watch {
        let watch_tx = reload_tx.clone();

        if file_tree.is_single_file() {
            // Watch single file
            if let Some(file) = file_tree.default_file() {
                let watch_path = file.absolute_path.clone();
                tokio::spawn(async move {
                    if let Err(e) = watch_file_async(&watch_path, watch_tx).await {
                        eprintln!("Failed to start file watcher: {}", e);
                    }
                });
            }
        } else {
            // Watch entire directory
            let watch_path = file_tree.base_path.clone();
            tokio::spawn(async move {
                if let Err(e) = crate::watcher::watch_directory_async(&watch_path, watch_tx).await {
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
    println!("Press Ctrl+C to stop");

    // Open browser
    if let Err(e) = open::that(format!("http://{}", addr)) {
        eprintln!("Failed to open browser: {}", e);
        println!("Please open http://{} in your browser", addr);
    }

    axum::serve(listener, app).await?;

    Ok(())
}

async fn serve_html(
    State(state): State<Arc<ServerState>>,
    Query(query): Query<ViewQuery>,
) -> (HeaderMap, Html<String>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    (headers, Html(state.render_html(query.file.as_deref())))
}

async fn serve_file_list(State(state): State<Arc<ServerState>>) -> Json<FileListResponse> {
    let files = state
        .file_tree
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
        base_path: state.file_tree.base_path.to_string_lossy().to_string(),
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
    match state.render_content_only(&query.file) {
        Some(content) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            headers.insert(header::CONTENT_TYPE, "text/html; charset=utf-8".parse().unwrap());
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

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<ServerState>) {
    let mut rx = state.reload_tx.subscribe();

    // Send initial connection confirmation
    let _ = socket.send(Message::Text("connected".to_string())).await;

    loop {
        tokio::select! {
            // Wait for reload signal
            result = rx.recv() => {
                match result {
                    Ok(_) => {
                        // Send reload signal to client
                        if socket.send(Message::Text("reload".to_string())).await.is_err() {
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

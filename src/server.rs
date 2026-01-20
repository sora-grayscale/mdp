use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::renderer::html::HtmlRenderer;
use crate::watcher::watch_file_async;

pub struct ServerState {
    pub markdown_path: PathBuf,
    pub title: String,
    pub reload_tx: broadcast::Sender<()>,
}

impl ServerState {
    fn render_html(&self) -> String {
        let content = std::fs::read_to_string(&self.markdown_path).unwrap_or_default();
        let renderer = HtmlRenderer::new(&self.title);
        renderer.render(&content)
    }
}

pub async fn start_server(markdown_path: PathBuf, title: &str, port: u16, watch: bool) -> std::io::Result<()> {
    let (reload_tx, _) = broadcast::channel::<()>(16);

    let state = Arc::new(ServerState {
        markdown_path: markdown_path.clone(),
        title: title.to_string(),
        reload_tx: reload_tx.clone(),
    });

    // Start file watcher if watch mode is enabled
    if watch {
        let watch_tx = reload_tx.clone();
        let watch_path = markdown_path.clone();
        tokio::spawn(async move {
            if let Err(e) = watch_file_async(&watch_path, watch_tx).await {
                eprintln!("Failed to start file watcher: {}", e);
            }
        });
    }

    let app = Router::new()
        .route("/", get(serve_html))
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

async fn serve_html(State(state): State<Arc<ServerState>>) -> (HeaderMap, Html<String>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    (headers, Html(state.render_html()))
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

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;

use crate::renderer::html::HtmlRenderer;

pub struct ServerState {
    pub html_content: String,
}

pub async fn start_server(markdown: &str, title: &str, port: u16) -> std::io::Result<()> {
    let renderer = HtmlRenderer::new(title);
    let html_content = renderer.render(markdown);

    let state = Arc::new(ServerState { html_content });

    let app = Router::new()
        .route("/", get(serve_html))
        .route("/assets/github.css", get(serve_css))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    println!("Server running at http://{}", addr);

    // Open browser
    if let Err(e) = open::that(format!("http://{}", addr)) {
        eprintln!("Failed to open browser: {}", e);
        println!("Please open http://{} in your browser", addr);
    }

    axum::serve(listener, app).await?;

    Ok(())
}

async fn serve_html(State(state): State<Arc<ServerState>>) -> Html<String> {
    Html(state.html_content.clone())
}

async fn serve_css() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css")],
        HtmlRenderer::get_css(),
    )
        .into_response()
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

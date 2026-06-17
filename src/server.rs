use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::SinkExt;
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::assets::ServerAssets;
use crate::pty::PtySession;
use crate::terminal;

/// Shared state passed to every request. Currently just enough for the WS handler.
#[derive(Clone)]
pub struct ServerState {
    inner: Arc<Inner>,
}

struct Inner {
    shell: String,
    shell_args: Vec<String>,
}

impl ServerState {
    pub fn new(shell: String, shell_args: Vec<String>) -> Self {
        Self {
            inner: Arc::new(Inner { shell, shell_args }),
        }
    }

    pub async fn bind(&self, addr: SocketAddr) -> Result<TcpListener> {
        let listener = TcpListener::bind(addr).await?;
        Ok(listener)
    }

    fn shell(&self) -> &str {
        &self.inner.shell
    }

    fn shell_args(&self) -> &[String] {
        &self.inner.shell_args
    }
}

/// Build the axum router.
fn router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(serve_root))
        .route("/healthz", get(health))
        .route("/{*path}", get(serve_asset))
        .route("/ws", get(ws_upgrade))
        .with_state(state)
}

pub async fn serve<F>(listener: TcpListener, state: ServerState, shutdown: F) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let app = router(state);
    info!("server ready");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn serve_root() -> Response {
    serve_key("index.html")
}

async fn serve_asset(Path(path): Path<String>) -> Response {
    // Defense in depth on top of `..`. axum already URL-decodes the path before
    // it reaches us, but reject absolute-style keys and embedded NULs in case
    // any downstream lookup ever normalizes differently than expected.
    if path.contains("..") || path.starts_with('/') || path.contains('\0') {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }
    serve_key(&path)
}

fn serve_key(key: &str) -> Response {
    match ServerAssets::get(key) {
        Some(asset) => {
            let mime = mime_guess::from_path(key).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                asset.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn ws_upgrade(
    State(state): State<ServerState>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    let shell = state.shell().to_string();
    let args = state.shell_args().to_vec();
    ws.on_upgrade(move |mut socket| async move {
        // Spawn the PTY first; if it fails we close the WS cleanly so the
        // browser sees a Close frame instead of hanging on a 101'd connection.
        // The failure reason is logged in the binary log (operator-visible).
        let pty = match PtySession::spawn(&shell, &args, None, 80, 24) {
            Ok(pty) => pty,
            Err(err) => {
                warn!(error = ?err, "could not spawn PTY for new connection");
                let _ = socket.close().await;
                return;
            }
        };
        terminal::run(socket, pty).await;
    })
}

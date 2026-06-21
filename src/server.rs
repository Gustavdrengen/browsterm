use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::info;

use crate::assets::ServerAssets;
use crate::fs;
use crate::pty::PtySession;
use crate::terminal;

/// Server-assigned tab identifier. 0 is reserved as a sentinel; tabs
/// start at 1 and increment monotonically, wrapping back to 1 on
/// `u32::MAX` so we never hand out the same id twice within reason.
pub type TabId = u32;

/// One tab's worth of server-side state. `PtySession` is already
/// `Clone`-via-`Arc`, so a per-tab forwarder task borrowing a clone is
/// cheap and shares the same underlying PTY with the dispatch path
/// (resize / input targets land on the same PTY the forwarder is
/// reading from).
#[derive(Clone)]
pub struct TabRecord {
    pub label: String,
    pub pty: PtySession,
}

/// Shared state passed to every request. Holds the tab roster so a WS
/// reconnect greets the new socket with the same PTYs already running.
/// The roster is wiped when the server process exits — there is no
/// disk persistence yet (Tier-3 polish).
#[derive(Clone)]
pub struct ServerState {
    inner: Arc<Inner>,
}

struct Inner {
    shell: String,
    shell_args: Vec<String>,
    tabs: Mutex<HashMap<TabId, TabRecord>>,
    next_tab_id: Mutex<u32>,
    next_default_label_n: Mutex<u32>,
}

impl ServerState {
    pub fn new(shell: String, shell_args: Vec<String>) -> Self {
        Self {
            inner: Arc::new(Inner {
                shell,
                shell_args,
                tabs: Mutex::new(HashMap::new()),
                next_tab_id: Mutex::new(1),
                next_default_label_n: Mutex::new(1),
            }),
        }
    }

    pub async fn bind(&self, addr: SocketAddr) -> Result<TcpListener> {
        let listener = TcpListener::bind(addr).await?;
        Ok(listener)
    }

    pub fn shell(&self) -> &str {
        &self.inner.shell
    }

    pub fn shell_args(&self) -> &[String] {
        &self.inner.shell_args
    }

    pub fn tabs(&self) -> &Mutex<HashMap<TabId, TabRecord>> {
        &self.inner.tabs
    }

    pub async fn allocate_tab_id(&self) -> TabId {
        let mut n = self.inner.next_tab_id.lock().await;
        let id = *n;
        *n = if id == u32::MAX { 1 } else { id + 1 };
        id
    }

    pub async fn next_default_label(&self) -> String {
        let mut n = self.inner.next_default_label_n.lock().await;
        let label = format!("Terminal {}", *n);
        *n = n.checked_add(1).unwrap_or(1);
        label
    }
}

fn router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(serve_root))
        .route("/healthz", get(health))
        .route("/api/fs/list", get(fs::list))
        .route("/api/fs/file", get(fs::file))
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
    ws.on_upgrade(move |socket| async move {
        // Defer PTY spawn until the WS upgrade lands and `ensure_first_tab`
        // finds an empty roster. On reconnect / page-reload, the roster
        // is already populated by previous sessions; the new socket gets
        // a `hello` envelope listing what's running.
        terminal::run(socket, state).await;
    })
}

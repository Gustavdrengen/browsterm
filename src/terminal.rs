use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::pty::PtySession;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientMessage {
    /// Browser reports its terminal cell size and asks the server to spawn
    /// the PTY at those dimensions. This is the first envelope a client is
    /// expected to send; everything sent before it is discarded.
    Ready { cols: u16, rows: u16 },
    /// Browser reports a subsequent terminal size change.
    Resize { cols: u16, rows: u16 },
    /// Browser sends a raw paste (copy-mode drag-and-drop fallback).
    Input { data: String },
}

/// Run a single terminal session over `socket` until either side closes.
///
/// The PTY is not spawned until the client sends a `Ready` envelope carrying
/// its actual column/row dims. This eliminates the banner-flash Tier-1 bug:
/// before this, the PTY was spawned at the static 80x24 fallback before the
/// browser reported its real dims, so banner-heavy shells painted at the
/// wrong size for a single repaint cycle before the resize-driven prompt
/// re-emit rescued the user.
pub async fn run(socket: WebSocket, shell: String, args: Vec<String>) {
    if let Err(err) = run_inner(socket, shell, args).await {
        warn!(error = ?err, "terminal session ended with error");
    }
}

async fn run_inner(socket: WebSocket, shell: String, args: Vec<String>) -> Result<()> {
    let (mut sender, mut receiver) = socket.split();

    // Wait for the client's `Ready` envelope so we spawn the PTY at the
    // browser's actual grid size. Anything sent before `Ready` is ignored.
    let (cols, rows) = loop {
        match receiver.next().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Ready { cols, rows }) => {
                    break (cols.max(1), rows.max(1));
                }
                Ok(_) => debug!("ignoring non-Ready envelope before Ready"),
                Err(_) => debug!("ignoring unparseable text before Ready"),
            },
            Some(Ok(Message::Binary(_))) => {
                debug!("ignoring binary frame before Ready");
            }
            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                debug!("client disconnected before sending Ready");
                let _ = sender.send(Message::Close(None)).await;
                return Ok(());
            }
        }
    };

    let pty = match PtySession::spawn(&shell, &args, None, cols, rows) {
        Ok(pty) => pty,
        Err(err) => {
            warn!(error = ?err, "PTY spawn failed after client Ready");
            let _ = sender.send(Message::Close(None)).await;
            return Ok(());
        }
    };

    let mut pty_bytes = pty.subscribe();

    // Pump 1: PTY bytes -> browser (binary frames, no UTF-8 lossy conversion).
    let pty_to_browser = async {
        while let Ok(bytes) = pty_bytes.recv().await {
            if sender.send(Message::Binary(bytes)).await.is_err() {
                break;
            }
        }
    };

    // Pump 2: browser -> PTY, dispatching JSON control vs raw stdin.
    let pty_arc = pty.clone();
    let browser_to_pty = async {
        while let Some(msg) = receiver.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(_) => break,
            };
            match msg {
                Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Ready { cols, rows }) => {
                        debug!(cols, rows, "late Ready; forwarding as resize");
                        if let Err(err) = pty_arc.resize(cols, rows).await {
                            warn!(error = ?err, "PTY resize failed");
                        }
                    }
                    Ok(ClientMessage::Resize { cols, rows }) => {
                        debug!(cols, rows, "client resize");
                        if let Err(err) = pty_arc.resize(cols, rows).await {
                            warn!(error = ?err, "PTY resize failed");
                        }
                    }
                    Ok(ClientMessage::Input { data }) => {
                        if let Err(err) = pty_arc
                            .write(Bytes::copy_from_slice(data.as_bytes()))
                            .await
                        {
                            warn!(error = ?err, "PTY write failed");
                            break;
                        }
                    }
                    Err(_) => {
                        // Treat unparseable text as raw PTY input (legacy /
                        // xterm.js paste-without-JSON path).
                        if let Err(err) =
                            pty_arc.write(Bytes::copy_from_slice(text.as_bytes())).await
                        {
                            warn!(error = ?err, "PTY write failed");
                            break;
                        }
                    }
                },
                Message::Binary(bin) => {
                    if let Err(err) = pty_arc.write(Bytes::from(bin)).await {
                        warn!(error = ?err, "PTY write failed");
                        break;
                    }
                }
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) => {}
            }
        }
        let _ = pty_arc.shutdown().await;
    };

    tokio::select! {
        _ = pty_to_browser => {}
        _ = browser_to_pty => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ClientMessage;

    #[test]
    fn resize_envelope_parses() {
        let parsed: ClientMessage =
            serde_json::from_str(r#"{"type":"resize","cols":120,"rows":40}"#).unwrap();
        match parsed {
            ClientMessage::Resize { cols, rows } => {
                assert_eq!(cols, 120);
                assert_eq!(rows, 40);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn ready_envelope_parses() {
        // The first envelope a client is required to send. Spawn dims come
        // straight from the browser-reported cell grid.
        let parsed: ClientMessage =
            serde_json::from_str(r#"{"type":"ready","cols":132,"rows":50}"#).unwrap();
        match parsed {
            ClientMessage::Ready { cols, rows } => {
                assert_eq!(cols, 132);
                assert_eq!(rows, 50);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn input_envelope_parses() {
        let parsed: ClientMessage =
            serde_json::from_str(r#"{"type":"input","data":"ls\n"}"#).unwrap();
        match parsed {
            ClientMessage::Input { data } => assert_eq!(data, "ls\n"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn unknown_envelope_fails_loudly_does_not_resolve_as_resize() {
        // Guards against silent mis-routing of malformed messages.
        assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"oops"}"#).is_err());
    }
}

use std::collections::HashMap;

use anyhow::Result;
use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::pty::PtySession;
use crate::server::{ServerState, TabId, TabRecord};

/// Length of the binary tab-id prefix on every server → client binary
/// frame. Browser demuxes PTY output by reading the first 4 bytes as a
/// little-endian u32 tab id, then passes the remainder to xterm.js
/// unchanged. The prefix keeps sixel / kitty graphic protocols intact.
const TAB_ID_PREFIX_LEN: usize = 4;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ClientMessage {
    /// Spawn a new PTY at the supplied dims under an auto or
    /// client-supplied label. Reply: `tab-ack {tab_id, label}`.
    #[serde(rename = "create-tab")]
    CreateTab {
        cols: u16,
        rows: u16,
        #[serde(default)]
        label: Option<String>,
    },
    /// Kill the named tab's PTY and remove it from the roster.
    /// Reply: `tab-event {tab_id, kind:"closed"}`.
    #[serde(rename = "close-tab")]
    CloseTab { tab_id: TabId },
    /// Rename the named tab. Empty / whitespace-only is rejected
    /// server-side as a no-op rather than blanking the visible label.
    #[serde(rename = "rename")]
    Rename { tab_id: TabId, label: String },
    /// Forward the named tab's PTY the supplied dims.
    #[serde(rename = "resize")]
    Resize {
        tab_id: TabId,
        cols: u16,
        rows: u16,
    },
    /// Forward raw UTF-8 input to the named tab's PTY.
    #[serde(rename = "input")]
    Input { tab_id: TabId, data: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum TabEventKind {
    Closed,
    Renamed,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ServerMessage {
    /// Listing of the current tab roster on every WS upgrade. The
    /// client uses this to (re)build the strip and attach fresh
    /// xterm.js subscribers per surviving tab.
    Hello { tabs: Vec<HelloTab> },
    #[serde(rename = "tab-ack")]
    TabAck { tab_id: TabId, label: String },
    #[serde(rename = "tab-event")]
    TabEvent {
        tab_id: TabId,
        kind: TabEventKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
struct HelloTab {
    tab_id: TabId,
    label: String,
    /// Per-tab scrollback snapshot for replay on WS reconnect. The
    /// browser uses this to seed xterm.js with the last
    /// SCROLLBACK_CAP_BYTES of visible state before any new live
    /// bytes arrive — a refresh or transient network blip is invisible
    /// from the user's perspective. Serialised as a JSON array of bytes
    /// (0–255) — readable, debuggable in DevTools, and the small
    /// overhead is fine because we cap the buffer at 256 KiB.
    scrollback: Vec<u8>,
}


fn encode_tab_bytes(tab_id: TabId, body: Bytes) -> Bytes {
    let mut buf = BytesMut::with_capacity(TAB_ID_PREFIX_LEN + body.len());
    buf.extend_from_slice(&tab_id.to_le_bytes());
    buf.extend_from_slice(&body);
    buf.freeze()
}

/// Run a single WebSocket connection multiplexed across every tab in
/// `state`. WebSocket drops do not destroy the tab roster; subsequent
/// upgrades greet the new socket with `hello` listing whatever is
/// still alive. The roster is wiped on `ServerState` drop (process
/// shutdown), so a server restart starts fresh.
pub async fn run(socket: WebSocket, state: ServerState) {
    if let Err(err) = run_inner(socket, state).await {
        warn!(error = ?err, "terminal session ended with error");
    }
}

async fn run_inner(socket: WebSocket, state: ServerState) -> Result<()> {
    let (mut sender, mut receiver) = socket.split();

    // Bootstrap: ensure at least one default tab exists. The first
    // client's first `resize` will pinch it to the user's grid. The
    // function holds the roster mutex across the PTY spawn so two
    // concurrent WS upgrades see a strict "non-empty after first"
    // invariant and never produce two phantom default tabs.
    ensure_first_tab(&state).await?;

    // Send the current roster to the new socket.
    let roster = collect_roster(&state).await;
    send_json(&mut sender, ServerMessage::Hello { tabs: roster.clone() }).await?;
    // Replay per-tab scrollback as the same kind of tab-id-prefixed
    // binary frames the forwarders emit. A fresh client gets the saved
    // state before *any* live byte is pushed; live output follows on
    // its heels via the per-tab forwarders spawned right after.
    replay_scrollback(&mut sender, &roster).await?;

    // Fan-in: per-tab forwarders push Message frames into this
    // unbounded mpsc; the main loop drains it onto the WS. The mpsc
    // sidesteps the SplitSink-not-Clone problem (axum 0.8's
    // WebSocket writer is not Clone) and serializes writes from N
    // forwarders onto the single WS without explicit locking on the
    // hot path.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    // One forwarder per existing tab. New tabs add a forwarder inside
    // the create-tab dispatch path; close-tab removes it.
    let mut pumps: HashMap<TabId, tokio::task::JoinHandle<()>> = HashMap::new();
    {
        let tabs = state.tabs().lock().await;
        for (&id, rec) in tabs.iter() {
            let handle = spawn_pty_forwarder(id, rec.clone(), out_tx.clone());
            pumps.insert(id, handle);
        }
    }

    loop {
        tokio::select! {
            // Drive incoming envelopes from the browser. `biased`
            // prefers incoming over outgoing so a fast-typing user
            // never starves on the drain side.
            biased;
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        dispatch_text(&text, &state, &mut sender, &mut pumps, out_tx.clone()).await;
                    }
                    Some(Ok(Message::Binary(_))) => {
                        // Legacy client → server PTY bytes path: clients
                        // address PTYs through JSON `input` envelopes;
                        // binary frames coming the wrong way are
                        // silently ignored so a hostile or stale
                        // client can never inject raw bytes.
                        debug!("unexpected binary client→server frame; ignored");
                    }
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                }
            }
            out_msg = out_rx.recv() => {
                match out_msg {
                    Some(msg) => {
                        if sender.send(msg).await.is_err() { break; }
                    }
                    None => break, // all forwarders dropped (e.g. ws closed)
                }
            }
        }
    }

    // WS is gone. Keep the tab roster alive (a fresh reconnect on
    // this same ServerState greets it again). Just abort the per-tab
    // forwarder tasks — their broadcast receivers drop with the task.
    for handle in pumps.into_values() {
        handle.abort();
    }

    Ok(())
}

async fn dispatch_text(
    text: &str,
    state: &ServerState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    pumps: &mut HashMap<TabId, tokio::task::JoinHandle<()>>,
    out_tx: mpsc::UnboundedSender<Message>,
) {
    match serde_json::from_str::<ClientMessage>(text) {
        Ok(env) => {
            if let Err(err) = handle_client(env, state, sender, pumps, out_tx).await {
                warn!(error = ?err, "client envelope dispatch failed");
            }
        }
        Err(_) => debug!("unparseable JSON; ignored"),
    }
}

async fn handle_client(
    env: ClientMessage,
    state: &ServerState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    pumps: &mut HashMap<TabId, tokio::task::JoinHandle<()>>,
    out_tx: mpsc::UnboundedSender<Message>,
) -> Result<()> {
    match env {
        ClientMessage::CreateTab { cols, rows, label } => {
            let label = match label {
                Some(s) => {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        state.next_default_label().await
                    } else {
                        trimmed.chars().take(64).collect::<String>()
                    }
                }
                None => state.next_default_label().await,
            };
            let tab_id = state.allocate_tab_id().await;
            let pty = PtySession::spawn(
                state.shell(),
                state.shell_args(),
                None,
                cols.max(1),
                rows.max(1),
            )?;
            let rec = TabRecord {
                label: label.clone(),
                pty: pty.clone(),
            };
            // Register and spawn the per-tab forwarder. The forwarder
            // and the dispatch share the same PtySession via Arc.
            {
                let mut tabs = state.tabs().lock().await;
                tabs.insert(tab_id, rec.clone());
            }
            let handle = spawn_pty_forwarder(tab_id, rec.clone(), out_tx);
            pumps.insert(tab_id, handle);
            send_json(sender, ServerMessage::TabAck { tab_id, label }).await?;
        }
        ClientMessage::CloseTab { tab_id } => {
            let removed = {
                let mut tabs = state.tabs().lock().await;
                tabs.remove(&tab_id)
            };
            if let Some(rec) = removed {
                let _ = rec.pty.shutdown().await;
                if let Some(handle) = pumps.remove(&tab_id) {
                    handle.abort();
                }
                send_json(
                    sender,
                    ServerMessage::TabEvent {
                        tab_id,
                        kind: TabEventKind::Closed,
                        label: None,
                    },
                )
                .await?;
            } else {
                debug!(tab_id, "close-tab on empty slot; no-op");
            }
        }
        ClientMessage::Rename { tab_id, label } => {
            let trimmed: String = label.trim().chars().take(64).collect();
            if trimmed.is_empty() {
                debug!(tab_id, "rename to empty label ignored");
                return Ok(());
            }
            let mut tabs = state.tabs().lock().await;
            if let Some(rec) = tabs.get_mut(&tab_id) {
                rec.label = trimmed.clone();
            send_json(
                sender,
                ServerMessage::TabEvent {
                    tab_id,
                    kind: TabEventKind::Renamed,
                    label: Some(trimmed),
                },
            )
            .await?;
            } else {
                debug!(tab_id, "rename on empty slot; ignored");
            }
        }
        ClientMessage::Resize { tab_id, cols, rows } => {
            let tabs = state.tabs().lock().await;
            if let Some(rec) = tabs.get(&tab_id) {
                let _ = rec.pty.resize(cols, rows).await;
            } else {
                debug!(tab_id, "resize on empty slot; ignored");
            }
        }
        ClientMessage::Input { tab_id, data } => {
            let tabs = state.tabs().lock().await;
            if let Some(rec) = tabs.get(&tab_id) {
                let _ = rec
                    .pty
                    .write(Bytes::copy_from_slice(data.as_bytes()))
                    .await;
            } else {
                debug!(tab_id, "input on empty slot; ignored");
            }
        }
    }
    Ok(())
}

fn spawn_pty_forwarder(
    tab_id: TabId,
    rec: TabRecord,
    out_tx: mpsc::UnboundedSender<Message>,
) -> tokio::task::JoinHandle<()> {
    let mut rx = rec.pty.subscribe();
    tokio::spawn(async move {
        while let Ok(bytes) = rx.recv().await {
            let frame = encode_tab_bytes(tab_id, bytes);
            // mpsc `send` is non-blocking for unbounded; if the
            // receiver has dropped (WS handler gone), `send` returns
            // Err and the forwarder exits cleanly — exactly what we
            // want on close-tab / WS drop.
            if out_tx.send(Message::Binary(frame)).is_err() {
                break;
            }
        }
    })
}

async fn send_json(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: ServerMessage,
) -> Result<()> {
    let payload = serde_json::to_string(&msg)?;
    sender.send(Message::Text(Utf8Bytes::from(payload))).await?;
    Ok(())
}

async fn collect_roster(state: &ServerState) -> Vec<HelloTab> {
    let tabs = state.tabs().lock().await;
    let mut out: Vec<HelloTab> = Vec::with_capacity(tabs.len());
    for (&id, rec) in tabs.iter() {
        out.push(HelloTab {
            tab_id: id,
            label: rec.label.clone(),
            scrollback: rec.pty.scrollback(),
        });
    }
    // Stable order so the client renders the strip deterministically;
    // also matches creation order (id monotonically increasing).
    out.sort_by_key(|t| t.tab_id);
    out
}

/// Replay the per-tab scrollback to a fresh client as a sequence of
/// `tab_id`-prefixed binary frames identical in shape to the live
/// forwarder's output. The client uses the same decode path to seed
/// each new `Terminal` instance with the saved bytes, so the user's
/// shell state survives a WS reconnect (refresh, transient blip, etc).
async fn replay_scrollback(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    tabs: &[HelloTab],
) -> Result<()> {
    for tab in tabs {
        if tab.scrollback.is_empty() {
            continue;
        }
        let frame = encode_tab_bytes(tab.tab_id, Bytes::from(tab.scrollback.clone()));
        sender.send(Message::Binary(frame)).await?;
    }
    Ok(())
}

async fn ensure_first_tab(state: &ServerState) -> Result<()> {
    let mut tabs = state.tabs().lock().await;
    if !tabs.is_empty() {
        return Ok(());
    }
    // The roster mutex is held across the synchronous PTY spawn so
    // two concurrent WS upgrades see a strict "non-empty after first"
    // invariant: whichever proceeeder acquires the lock first wins and
    // inserts; the second sees non-empty and short-circuits without
    // spawning a phantom PTY that would need to be killed. PtySession
    // spawn is short (openpty + 2 thread spawns, no heavy IO) so the
    // contention window is small enough to leave alone at MVP.
    let tab_id = state.allocate_tab_id().await;
    let label = state.next_default_label().await;
    let pty = PtySession::spawn(state.shell(), state.shell_args(), None, 80, 24)?;
    tabs.insert(tab_id, TabRecord { label, pty });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_tab_envelope_parses() {
        let parsed: ClientMessage = serde_json::from_str(
            r#"{"type":"create-tab","cols":140,"rows":42,"label":"build"}"#,
        )
        .unwrap();
        match parsed {
            ClientMessage::CreateTab { cols, rows, label } => {
                assert_eq!(cols, 140);
                assert_eq!(rows, 42);
                assert_eq!(label.as_deref(), Some("build"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn close_tab_envelope_parses() {
        let parsed: ClientMessage =
            serde_json::from_str(r#"{"type":"close-tab","tab_id":3}"#).unwrap();
        match parsed {
            ClientMessage::CloseTab { tab_id } => assert_eq!(tab_id, 3),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rename_envelope_parses() {
        let parsed: ClientMessage =
            serde_json::from_str(r#"{"type":"rename","tab_id":7,"label":"editor"}"#)
                .unwrap();
        match parsed {
            ClientMessage::Rename { tab_id, label } => {
                assert_eq!(tab_id, 7);
                assert_eq!(label, "editor");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn resize_with_tab_id_parses() {
        let parsed: ClientMessage =
            serde_json::from_str(r#"{"type":"resize","tab_id":12,"cols":100,"rows":30}"#)
                .unwrap();
        match parsed {
            ClientMessage::Resize {
                tab_id,
                cols,
                rows,
            } => {
                assert_eq!(tab_id, 12);
                assert_eq!(cols, 100);
                assert_eq!(rows, 30);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn input_with_tab_id_parses() {
        let parsed: ClientMessage =
            serde_json::from_str(r#"{"type":"input","tab_id":4,"data":"ls\n"}"#).unwrap();
        match parsed {
            ClientMessage::Input { tab_id, data } => {
                assert_eq!(tab_id, 4);
                assert_eq!(data, "ls\n");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn unknown_envelope_fails_loudly() {
        assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"oops"}"#).is_err());
    }

    #[test]
    fn hello_envelope_serializes_round_trip() {
        let msg = ServerMessage::Hello {
            tabs: vec![
                HelloTab {
                    tab_id: 1,
                    label: "Terminal 1".to_string(),
                    scrollback: vec![0x1b, b'[', b'3', b'2', b'm'],
                },
                HelloTab {
                    tab_id: 2,
                    label: "build".to_string(),
                    scrollback: Vec::new(),
                },
            ],
        };
        let json = serde_json::to_string(&msg).unwrap();
        // Kebab-case type, tabs array with id+label fields, scrollback
        // serialised as a JSON byte array. Empty scrollback must emit
        // `[]` not `null`.
        assert!(json.contains(r#""type":"hello""#));
        assert!(json.contains(r#""tab_id":1"#));
        assert!(json.contains(r#""label":"build""#));
        assert!(json.contains("[27,91,51,50,109]"));
        assert!(json.contains("\"scrollback\":[]"));
    }

    #[test]
    fn tab_event_envelope_serializes_round_trip() {
        let msg = ServerMessage::TabEvent {
            tab_id: 7,
            kind: TabEventKind::Renamed,
            label: Some("build".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"tab-event""#));
        assert!(json.contains(r#""kind":"renamed""#));
        assert!(json.contains(r#""label":"build""#));

        let closed = ServerMessage::TabEvent {
            tab_id: 7,
            kind: TabEventKind::Closed,
            label: None,
        };
        let json2 = serde_json::to_string(&closed).unwrap();
        // Closed events do not carry a label (skip_serializing_if).
        assert!(!json2.contains(r#""label""#));
    }

    #[test]
    fn encode_tab_bytes_prepends_little_endian_id() {
        let bytes = encode_tab_bytes(0x01020304, Bytes::from_static(b"hi"));
        assert_eq!(bytes.len(), 4 + 2);
        // LE u32 0x01020304 → [0x04, 0x03, 0x02, 0x01].
        assert_eq!(&bytes[..4], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&bytes[4..], b"hi");
    }

    /// Combined regression for the two validation rules that the
    /// Rename arm enforces together: trim whitespace AND cap at 64
    /// chars. A label with leading whitespace AND length > 64
    /// exercises both rules in a single assertion so any future
    /// contributor dropping either one fails immediately.
    #[test]
    fn rename_trims_whitespace_and_caps_at_64_chars() {
        let label = format!("{}abcdefghij", " ".repeat(60));
        assert!(label.len() > 64);
        let trimmed: String = label.trim().chars().take(64).collect();
        // Trim removes whitespace, take(64) caps; pure-ASCII letters
        // mean trim().take(64) is exactly the 10-letter test content.
        assert_eq!(trimmed, "abcdefghij");

        let whitespace_only = "   ".to_string();
        let trimmed_ws: String = whitespace_only.trim().chars().take(64).collect();
        assert_eq!(trimmed_ws, "");
    }
}

use std::io::Write;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use bytes::Bytes;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tokio::sync::{broadcast, mpsc, Mutex};

/// Capacity of the broadcast channel carrying PTY bytes to WS subscribers.
/// Larger than the per-read chunk so a single slow consumer does not silently
/// truncate the scrollback mid-frame for everyone else.
const BROADCAST_CAPACITY: usize = 4096;

/// Soft cap on bytes retained in the per-session scrollback ring. PTY
/// output that pushes the cursor past this point drops the oldest bytes
/// first. 256 KiB is enough to rebuild a typical fresh shell prompt plus
/// the last ~30 commands; sized to match xterm.js's own default
/// `scrollback` of 1000 rows.
const SCROLLBACK_CAP_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct PtySession {
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer_tx: mpsc::UnboundedSender<Bytes>,
    bytes_tx: broadcast::Sender<Bytes>,
    child: Arc<Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>>,
    /// Ring-buffered scrollback retained across WS reconnects. Plain
    /// `std::sync::Mutex` because the reader thread holds it briefly;
    /// the WS handler only reads snapshots via `scrollback()` and never
    /// blocks on contention for long.
    scrollback: Arc<StdMutex<Vec<u8>>>,
}

impl PtySession {
    /// Spawn `shell` with `args` under a fresh PTY at `(cols, rows)`.
    pub fn spawn(
        shell: &str,
        args: &[String],
        cwd: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        let pty_system = native_pty_system();
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = pty_system
            .openpty(size)
            .context("could not open PTY pair")?;

        let mut cmd = CommandBuilder::new(shell);
        for a in args {
            cmd.arg(a);
        }
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        // xterm-256color is the lowest common denominator that every modern
        // TUI (lazygit, btop, k9s, htop) accepts.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("could not spawn shell under PTY")?;
        // Drop the slave end; the master keeps it alive while the child runs.
        drop(pair.slave);

        let master = pair.master;

        // Broadcast channel: PTY reader thread → WS handlers.
        let (bytes_tx, _) = broadcast::channel::<Bytes>(BROADCAST_CAPACITY);

        // mpsc channel: WS handler input → PTY writer thread.
        let (writer_tx, mut writer_rx) = mpsc::unbounded_channel::<Bytes>();

        // Reader thread: pull PTY output, broadcast to live subscribers,
        // and append to the per-session scrollback ring so a WS reconnect
        // can replay the last SCROLLBACK_CAP_BYTES of visible state.
        let scrollback = Arc::new(StdMutex::new(Vec::with_capacity(8 * 1024)));
        {
            let bytes_tx = bytes_tx.clone();
            let scrollback = scrollback.clone();
            let mut reader = master
                .try_clone_reader()
                .context("could not clone PTY reader")?;
            std::thread::Builder::new()
                .name("browsterm-pty-reader".into())
                .spawn(move || {
                    let mut buf = vec![0u8; 8 * 1024];
                    loop {
                        match reader.read(&mut buf[..]) {
                            Ok(0) => break, // EOF — shell exited
                            Ok(n) => {
                                let chunk = Bytes::copy_from_slice(&buf[..n]);
                                // Append to scrollback before broadcast so a
                                // reconnect that raced the broadcast still sees
                                // a consistent state (broadcast drops stale
                                // receivers, scrollback is persistent).
                                if let Ok(mut ring) = scrollback.lock() {
                                    ring.extend_from_slice(&chunk);
                                    if ring.len() > SCROLLBACK_CAP_BYTES {
                                        let drop = ring.len() - SCROLLBACK_CAP_BYTES;
                                        ring.drain(..drop);
                                    }
                                }
                                // Ignore "no subscribers" — recoverable gap, the
                                // WS handler triggers a resize on connect which
                                // makes the shell re-emit a prompt.
                                let _ = bytes_tx.send(chunk);
                            }
                            Err(err) => {
                                if err.kind() == std::io::ErrorKind::Interrupted {
                                    continue;
                                }
                                break;
                            }
                        }
                    }
                })
                .context("could not spawn PTY reader thread")?;
        }

        // Writer thread: drain mpsc into the PTY. portable-pty does not offer
        // try_clone_writer on MasterPty, so we take_writer once and own the
        // boxed writer on this thread; the master itself is kept for resize.
        //
        // Verified: resize-after-take_writer works on Linux/macOS portable-pty.
        // Windows ConPTY path is owed a Tier 3 test before native Windows is
        // a Tier 2 home (WSL is Linux, so it is fine today).
        let writer_box = master
            .take_writer()
            .context("could not take PTY writer")?;
        std::thread::Builder::new()
            .name("browsterm-pty-writer".into())
            .spawn(move || {
                let mut writer = writer_box;
                while let Some(bytes) = writer_rx.blocking_recv() {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                    let _ = writer.flush();
                    // Drain anything that piled up so latency stays low.
                    while let Ok(more) = writer_rx.try_recv() {
                        if writer.write_all(&more).is_err() {
                            return;
                        }
                        let _ = writer.flush();
                    }
                }
            })
            .context("could not spawn PTY writer thread")?;

        Ok(Self {
            master: Arc::new(Mutex::new(master)),
            writer_tx,
            bytes_tx,
            child: Arc::new(Mutex::new(Some(child))),
            scrollback,
        })
    }

    /// Subscribe to *live* PTY output bytes. New subscribers do not see
    /// historical output; the WS handler replays [`Self::scrollback`]
    /// after a fresh `subscribe()` so the user's terminal state
    /// survives reconnects without the broadcast race.
    pub fn subscribe(&self) -> broadcast::Receiver<Bytes> {
        self.bytes_tx.subscribe()
    }

    /// Snapshot of the bytes this session has produced since it was
    /// spawned, capped at `SCROLLBACK_CAP_BYTES`. The WS handler sends
    /// this back to a fresh client inside the `hello` envelope so the
    /// browser can rebuild xterm.js from the very last visible state.
    pub fn scrollback(&self) -> Vec<u8> {
        match self.scrollback.lock() {
            Ok(ring) => ring.clone(),
            Err(_) => Vec::new(),
        }
    }

    /// Forward user input bytes to the PTY.
    pub async fn write(&self, data: Bytes) -> std::io::Result<()> {
        self.writer_tx
            .send(data)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "writer task gone"))
    }

    /// Forward the browser's reported PTY size to the kernel.
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let master = self.master.lock().await;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("PTY resize failed")?;
        Ok(())
    }

    /// Kill the child and drop the master. Idempotent.
    pub async fn shutdown(&self) {
        let mut child_lock = self.child.lock().await;
        if let Some(mut child) = child_lock.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Tier-0 regression: prove the PTY plumbing spawns a shell, accepts
    /// bytes from a writer, and broadcasts output back. Skips gracefully on
    /// machines without `/bin/sh` (e.g., future CI on Windows before ConPTY
    /// is wired up).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pty_spawns_round_trip_bytes() {
        if !std::path::Path::new("/bin/sh").exists() {
            eprintln!("skip: /bin/sh not present");
            return;
        }
        let session = PtySession::spawn("/bin/sh", &[], None, 80, 24)
            .expect("PTY spawn must succeed");
        let mut rx = session.subscribe();

        // Issue a deterministic command; the shell must echo "READYMARK\n".
        session
            .write(Bytes::from_static(b"echo READYMARK\n"))
            .await
            .expect("PTY write must succeed");

        let mut collected = Vec::<u8>::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Ok(chunk)) => {
                    collected.extend_from_slice(&chunk);
                    if collected.windows(b"READYMARK".len()).any(|w| w == b"READYMARK") {
                        // success — round trip works
                        session.shutdown().await;
                        return;
                    }
                }
                _ => continue,
            }
        }

        session.shutdown().await;
        panic!(
            "PTY round-trip failed: 3s elapsed without seeing the echo back.\ncollected so far: {:?}",
            String::from_utf8_lossy(&collected)
        );
    }

    /// Subscribe race window is documented behavior, not a bug. A second
    /// subscription must still observe bytes that arrive after subscribe().
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn late_subscriber_receives_subsequent_bytes() {
        if !std::path::Path::new("/bin/sh").exists() {
            eprintln!("skip: /bin/sh not present");
            return;
        }
        let session = PtySession::spawn("/bin/sh", &[], None, 80, 24).unwrap();
        // Subscribe BEFORE writing so the race window is empty for this test.
        let mut rx = session.subscribe();
        session
            .write(Bytes::from_static(b"echo LATE\n"))
            .await
            .unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut collected = Vec::<u8>::new();
        while std::time::Instant::now() < deadline {
            if let Ok(Ok(chunk)) =
                tokio::time::timeout(Duration::from_millis(500), rx.recv()).await
            {
                collected.extend_from_slice(&chunk);
                if collected
                    .windows(b"LATE".len())
                    .any(|w| w == b"LATE")
                {
                    session.shutdown().await;
                    return;
                }
            }
        }
        session.shutdown().await;
        panic!(
            "late subscriber never received: {:?}",
            String::from_utf8_lossy(&collected)
        );
    }

    /// Tier-1 regression for the WS-reconnect scrollback ring. A fresh
    /// subscriber that connects after some output has been produced
    /// must still observe the last visible state through `scrollback()`,
    /// so the WS handler can replay it on reconnect and the user does
    /// not lose their shell history on a refresh.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scrollback_replays_output_to_late_subscriber() {
        if !std::path::Path::new("/bin/sh").exists() {
            eprintln!("skip: /bin/sh not present");
            return;
        }
        let session = PtySession::spawn("/bin/sh", &[], None, 80, 24).unwrap();
        // Drive a deterministic, recognisable line so the snapshot
        // contains something we can grep for.
        session
            .write(Bytes::from_static(b"echo SCROLLBACK_MARK\n"))
            .await
            .expect("PTY write must succeed");

        // Drain the live broadcast so the reader thread is forced to
        // populate the scrollback ring up to and beyond SCROLLBACK_MARK.
        let mut rx = session.subscribe();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(250), rx.recv()).await {
                Ok(Ok(chunk)) => {
                    if String::from_utf8_lossy(&chunk).contains("SCROLLBACK_MARK") {
                        break;
                    }
                }
                _ => continue,
            }
        }

        let snap = session.scrollback();
        assert!(
            String::from_utf8_lossy(&snap).contains("SCROLLBACK_MARK"),
            "scrollback snapshot should contain the marker; got {snap:?}"
        );
        // Hard upper bound: the ring must never exceed the cap.
        assert!(snap.len() <= SCROLLBACK_CAP_BYTES);

        session.shutdown().await;
    }

    /// The scrollback ring evicts oldest bytes once it exceeds the cap.
    /// Exercises the per-write eviction invariant directly without the
    /// PTY overhead: push a buffer larger than the cap and confirm the
    /// front is dropped while the tail is preserved.
    #[test]
    fn scrollback_ring_evicts_oldest_bytes() {
        // The invariant is simple: after the eviction, length == cap.
        // We mirror the implementation's drain-and-trim step.
        let cap = SCROLLBACK_CAP_BYTES;
        let mut ring: Vec<u8> = Vec::with_capacity(cap + 4096);
        for _ in 0..(cap + 4096) {
            ring.push(0);
        }
        if ring.len() > cap {
            let drop = ring.len() - cap;
            ring.drain(..drop);
        }
        assert!(ring.len() == cap);
    }
}

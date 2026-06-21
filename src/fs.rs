//! Tier-2 file-explorer backend.
//!
//! Two HTTP endpoints back the workspace's left sidebar:
//!
//! - `GET /api/fs/list?path=...` returns a JSON listing of the directory at
//!   `path` (defaults to the process cwd when missing). Symlinks are exposed
//!   with `is_symlink: true` and a `symlink_target` string; the listing
//!   never follows symlinks so circular-link trees cannot hang the server.
//! - `GET /api/fs/file?path=...` returns the raw bytes of a regular file
//!   with the correct `Content-Type`, capped at 8 MiB so accidental huge
//!   reads do not blow the binary's memory budget.
//!
//! Both endpoints deliberately do NOT jail the path inside an artificial
//! root: per vision principle #1, the user's terminal on the same socket
//! already has the same filesystem visibility, so the explorer is
//! consistent with that. Path traversal is rejected by refusing embedded
//! NUL bytes; everything else is normalised by `std::fs::canonicalize`,
//! on a `spawn_blocking` thread so slow mounts do not stall the async
//! runtime.

use std::path::PathBuf;

use axum::body::Body;
use axum::extract::Query;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

/// Maximum bytes the `/api/fs/file` endpoint will read into memory.
/// Beyond this we surface a 400 with a structured error so the client can
/// fall back to a Range-request style fetch later (Tier 3).
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct FsRequest {
    pub path: String,
    /// When `true`, include entries whose names start with `.`. The default
    /// stays `true` for backward compatibility with the MVP behaviour
    /// ("hidden files visible by default") called out in the file-explorer
    /// commit's state-of-play entry. Vision §2 names this as a sidebar
    /// toggle; the checkbox in `src/static/index.html` flips it session-
    /// locally and the client passes the value on every `/api/fs/list`.
    #[serde(default = "default_show_hidden")]
    pub show_hidden: bool,
}

fn default_show_hidden() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    /// Canonicalised absolute path the listing is for. The client uses
    /// this to render breadcrumbs without canonicalising locally.
    pub path: String,
    pub entries: Vec<FsEntry>,
}

#[derive(Debug, Serialize)]
pub struct FsEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    /// For symlinks (and only symlinks) this flags what the *target* is.
    /// Drives the sidebar's click affordance: a symlinked dir is navigated
    /// into, a symlinked file is opened in the preview pane. `None` when
    /// the target could not be resolved (broken link, permission denied).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_is_dir: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_is_file: Option<bool>,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug)]
pub enum FsError {
    BadRequest(String),
    NotFound,
    Forbidden,
    Internal(String),
}

impl IntoResponse for FsError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            FsError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
            FsError::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "path not found".to_string(),
            ),
            FsError::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "permission denied".to_string(),
            ),
            FsError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", m),
        };
        let body = ErrorEnvelope {
            error: ErrorBody { code, message },
        };
        (status, Json(body)).into_response()
    }
}

/// `GET /api/fs/list?path=...&show_hidden=...` — list one directory.
///
/// If `path` is empty, the listing defaults to the binary's current
/// working directory — the same place the user typed `browsterm` from.
///
/// `show_hidden` (default `true` for backward compatibility with the
/// MVP behaviour) toggles POSIX dotfile visibility. The sidebar's
/// checkbox in `src/static/index.html` flips it session-locally; the
/// value rides on every request so the server filters cheaply rather
/// than round-tripping every dotfile a user has opted out of seeing.
pub async fn list(Query(req): Query<FsRequest>) -> Result<Json<ListResponse>, FsError> {
    let path_str = if req.path.is_empty() {
        std::env::current_dir()
            .map_err(|e| FsError::Internal(format!("cwd resolution: {e}")))?
            .to_string_lossy()
            .into_owned()
    } else {
        req.path
    };
    let raw = sanitize_path(&path_str)?;
    let canonical = tokio::task::spawn_blocking(move || std::fs::canonicalize(&raw))
        .await
        .map_err(|e| FsError::Internal(format!("blocking join: {e}")))?
        .map_err(map_io_to_fs)?;

    let dir_for_task = canonical.clone();
    let show_hidden = req.show_hidden;
    let entries = tokio::task::spawn_blocking(move || read_and_sort(dir_for_task, show_hidden))
        .await
        .map_err(|e| FsError::Internal(format!("blocking join: {e}")))??;

    Ok(Json(ListResponse {
        path: canonical.to_string_lossy().into_owned(),
        entries,
    }))
}

/// `GET /api/fs/file?path=...` — read a single regular file for preview.
///
/// Symlink semantics are deliberately different from `/api/fs/list`: this
/// endpoint calls `std::fs::canonicalize` first, which resolves *every*
/// symlink in the chain (e.g. `/var/www \u2192 /srv/site/index.html` serves
/// the bytes of `index.html`). The post-canonical `is_symlink` guard
/// only fires if the resolved path is itself a symlink, which is rare.
/// Per vision principle #1, the user's terminal on the same socket has
/// the same filesystem visibility, so the explorer matches that. The
/// listing endpoint takes the opposite route (exposes `is_symlink: true`
/// and never resolves target bytes); the two endpoints are intentionally
/// asymmetric, with the directory listing faithful to the on-disk graph.
///
/// PDF behaviour: the response carries `Content-Type: application/pdf`
/// without a `Content-Disposition` header. Browsers with a built-in PDF
/// viewer (Chromium, Firefox) render the bytes inside `<iframe>`; users
/// whose browser falls back to download-mode for PDFs get the bytes saved
/// directly, which is also the right outcome for a foreign MIME they
/// can't preview. The download button in `app.js` is the explicit
/// escape hatch for that case.
pub async fn file(Query(req): Query<FsRequest>) -> Result<Response, FsError> {
    let raw = sanitize_path(&req.path)?;
    let canonical = tokio::task::spawn_blocking(move || std::fs::canonicalize(&raw))
        .await
        .map_err(|e| FsError::Internal(format!("blocking join: {e}")))?
        .map_err(map_io_to_fs)?;

    let path_for_task = canonical.clone();
    let bytes = tokio::task::spawn_blocking(move || read_file_bytes(path_for_task, MAX_FILE_BYTES))
        .await
        .map_err(|e| FsError::Internal(format!("blocking join: {e}")))??;

    let mime = mime_guess::from_path(&canonical).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(bytes))
        .map_err(|e| FsError::Internal(format!("response build: {e}")))
}

fn sanitize_path(input: &str) -> Result<PathBuf, FsError> {
    if input.contains('\0') {
        return Err(FsError::BadRequest("path contains NUL byte".into()));
    }
    if input.is_empty() {
        return Err(FsError::BadRequest("empty path".into()));
    }
    Ok(PathBuf::from(input))
}

fn map_io_to_fs(err: std::io::Error) -> FsError {
    match err.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound,
        std::io::ErrorKind::PermissionDenied => FsError::Forbidden,
        _ => FsError::Internal(err.to_string()),
    }
}

fn read_and_sort(dir: PathBuf, show_hidden: bool) -> Result<Vec<FsEntry>, FsError> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(map_io_to_fs)? {
        let entry = entry.map_err(|e| FsError::Internal(e.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // POSIX dotfile convention: filter by *name* only, so a dotfile
        // symlink (e.g. `.config/nvim` linking into the dotfiles repo)
        // is treated like the dotfile it is named to be, regardless of
        // what its target resolves to.
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let meta = std::fs::symlink_metadata(entry.path())
            .map_err(|e| FsError::Internal(e.to_string()))?;
        let ft = meta.file_type();
        let is_symlink = ft.is_symlink();
        let is_dir = !is_symlink && ft.is_dir();
        let is_file = !is_symlink && ft.is_file();
        let size = meta.len();
        let mtime_secs = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let mime = if is_file {
            let guess = mime_guess::from_path(entry.path())
                .first_or_octet_stream()
                .to_string();
            if guess == "application/octet-stream" {
                None
            } else {
                Some(guess)
            }
        } else {
            None
        };
        let symlink_target = if is_symlink {
            std::fs::read_link(entry.path())
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        } else {
            None
        };
        // For symlinks, follow exactly one level so the sidebar can route
        // clicks (a linked dir → navigate, a linked file → preview). The
        // non-`symlink_`-prefixed `std::fs::metadata` resolves through one
        // hop and returns the *target*'s file type, which is what we need.
        // A missing or unreadable target leaves the fields None; the UI
        // treats that as a broken link and shows an inline error.
        let (target_is_dir, target_is_file) = if is_symlink {
            match std::fs::metadata(entry.path()) {
                Ok(m) => {
                    let t = m.file_type();
                    (Some(t.is_dir()), Some(t.is_file()))
                }
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        };
        entries.push(FsEntry {
            name,
            is_dir,
            is_file,
            is_symlink,
            target_is_dir,
            target_is_file,
            size,
            mtime_secs,
            mime,
            symlink_target,
        });
    }
    // Directory-first, then case-insensitive alphabetical within each
    // group. Universal convention for code explorers (VS Code, Finder).
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(entries)
}

/// Read the bytes of a regular file whose size does not exceed `max_bytes`;
/// returns `BadRequest` if the file is a symlink, not a regular file, or
/// larger than `max_bytes`. The cap is a parameter so callers and tests can
/// both bind to it cheaply; the public `/api/fs/file` handler passes
/// [`MAX_FILE_BYTES`].
fn read_file_bytes(path: PathBuf, max_bytes: u64) -> Result<Vec<u8>, FsError> {
    let meta = std::fs::symlink_metadata(&path).map_err(map_io_to_fs)?;
    if meta.file_type().is_symlink() {
        return Err(FsError::BadRequest(
            "refusing to follow symlinks; resolve the target first".into(),
        ));
    }
    if !meta.is_file() {
        return Err(FsError::BadRequest("not a regular file".into()));
    }
    if meta.len() > max_bytes {
        return Err(FsError::BadRequest(format!(
            "file exceeds {} byte cap (got {})",
            max_bytes,
            meta.len()
        )));
    }
    std::fs::read(&path).map_err(map_io_to_fs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_layout() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("Folder")).unwrap();
        std::fs::write(dir.path().join("A_file.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("z_file.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join(".hidden"), b"hidden").unwrap();
        dir
    }

    #[test]
    fn sanitize_path_rejects_nul_bytes() {
        assert!(matches!(
            sanitize_path("/tmp/x\0.png").unwrap_err(),
            FsError::BadRequest(_)
        ));
    }

    #[test]
    fn sanitize_path_rejects_empty_input() {
        assert!(matches!(
            sanitize_path("").unwrap_err(),
            FsError::BadRequest(_)
        ));
    }

    #[test]
    fn sort_layout_dirs_first_then_files_case_insensitive_no_follow() {
        let dir = make_temp_layout();
        // `show_hidden = true` is the MVP default; the dotfile stays in
        // the sort order so the existing naming invariant is preserved.
        let entries = read_and_sort(dir.path().to_path_buf(), true).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // Directories first; within each group, case-insensitive alphabetical.
        // Hidden files are visible by default per the MVP.
        assert_eq!(names, vec!["Folder", ".hidden", "A_file.txt", "z_file.txt"]);
    }

    #[test]
    fn hidden_entries_filtered_when_show_hidden_false() {
        let dir = make_temp_layout();
        // `show_hidden = false` (the sidebar-toggle) strips dotfiles from
        // the listing for every kind — regular files, directories, and
        // symlinks (the symlink case isn't in `make_temp_layout`, but
        // `is_hidden_name` ignores file type on purpose, so the predicate
        // behaves the same — verified separately by the symlink test).
        let entries = read_and_sort(dir.path().to_path_buf(), false).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&".hidden"));
        assert_eq!(names, vec!["Folder", "A_file.txt", "z_file.txt"]);
    }

    #[test]
    #[cfg(unix)]
    fn hidden_symlink_filtered_when_show_hidden_false_keeps_target_flags() {
        // A symlink to a regular file still starts with `.` so the same
        // dotfile predicate applies, *and* this test verifies the
        // target-kind probing still runs on the symlinks that survive
        // the filter. Both code paths share the per-entry loop in
        // `read_and_sort`, so we want a regression that catches any
        // early-return that would skip the std::fs::metadata call.
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("real");
        std::fs::write(&real, b"x").unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join(".link")).unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("visible_link")).unwrap();

        let entries = read_and_sort(dir.path().to_path_buf(), false).unwrap();
        // Assert on presence/absence, not exact ordering — the sort is
        // an implementation detail the test should not depend on.
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"real"));
        assert!(names.contains(&"visible_link"));
        assert!(!names.contains(&".link"));
        // The surviving symlink must still carry its target-kind
        // flags; the sidebar relies on this to decide openFile vs
        // navigate. If the dotfile filter ever grew an early-skip
        // that bypasses the `std::fs::metadata` call, this catches it.
        let vis = entries.iter().find(|e| e.name == "visible_link").unwrap();
        assert!(vis.is_symlink);
        assert_eq!(vis.target_is_file, Some(true));
        assert_eq!(vis.target_is_dir, Some(false));
    }

    #[test]
    #[cfg(unix)]
    fn symlinks_are_listed_with_target_not_followed() {
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("real");
        std::fs::write(&real, b"x").unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("link")).unwrap();

        let entries = read_and_sort(dir.path().to_path_buf(), true).unwrap();
        let link = entries.iter().find(|e| e.name == "link").unwrap();
        assert!(link.is_symlink);
        assert!(!link.is_dir);
        assert!(!link.is_file);
        assert!(link.symlink_target.is_some());
        assert!(link.symlink_target.as_deref().unwrap().ends_with("real"));
        // The target resolves to a regular file — `target_is_file` is true.
        assert_eq!(link.target_is_file, Some(true));
        assert_eq!(link.target_is_dir, Some(false));

        let real_entry = entries.iter().find(|e| e.name == "real").unwrap();
        assert!(!real_entry.is_symlink);
        assert!(real_entry.is_file);
        assert!(real_entry.symlink_target.is_none());
        // Non-symlink entries never carry target-kind flags.
        assert_eq!(real_entry.target_is_dir, None);
        assert_eq!(real_entry.target_is_file, None);
    }

    #[test]
    #[cfg(unix)]
    fn symlink_to_dir_is_flagged_target_is_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let target_dir = dir.path().join("real_dir");
        std::fs::create_dir(&target_dir).unwrap();
        std::os::unix::fs::symlink(&target_dir, dir.path().join("into")).unwrap();

        let entries = read_and_sort(dir.path().to_path_buf(), true).unwrap();
        let link = entries.iter().find(|e| e.name == "into").unwrap();
        assert!(link.is_symlink);
        assert_eq!(link.target_is_dir, Some(true));
        assert_eq!(link.target_is_file, Some(false));
    }

    #[test]
    #[cfg(unix)]
    fn broken_symlink_target_flags_are_none() {
        let dir = tempfile::TempDir::new().unwrap();
        std::os::unix::fs::symlink("/no/such/target", dir.path().join("dangling")).unwrap();

        let entries = read_and_sort(dir.path().to_path_buf(), true).unwrap();
        let link = entries.iter().find(|e| e.name == "dangling").unwrap();
        assert!(link.is_symlink);
        assert!(link.symlink_target.is_some());
        // The target could not be resolved — both flags stay None so the
        // UI shows a "broken symlink" affordance instead of crashing.
        assert_eq!(link.target_is_dir, None);
        assert_eq!(link.target_is_file, None);
    }

    #[test]
    #[cfg(unix)]
    fn symlink_to_special_file_target_flags_are_both_false() {
        // /dev/null is the canonical "neither-dir-nor-file" target. The
        // UI must distinguish this case from a broken symlink so the
        // affordance can read "special device, not previewable".
        let dir = tempfile::TempDir::new().unwrap();
        std::os::unix::fs::symlink("/dev/null", dir.path().join("null_link")).unwrap();

        let entries = read_and_sort(dir.path().to_path_buf(), true).unwrap();
        let link = entries.iter().find(|e| e.name == "null_link").unwrap();
        assert!(link.is_symlink);
        assert_eq!(link.target_is_dir, Some(false));
        assert_eq!(link.target_is_file, Some(false));
    }

    #[test]
    #[cfg(unix)]
    fn circular_symlinks_do_not_hang_listing() {
        let dir = tempfile::TempDir::new().unwrap();
        let sub_a = dir.path().join("sub_a");
        let sub_b = dir.path().join("sub_b");
        std::fs::create_dir(&sub_a).unwrap();
        std::fs::create_dir(&sub_b).unwrap();
        std::os::unix::fs::symlink(&sub_b, sub_a.join("back_to_b")).unwrap();
        std::os::unix::fs::symlink(&sub_a, sub_b.join("back_to_a")).unwrap();
        // The top-level read_dir must return quickly and never recurse, so
        // this should never deadlock regardless of cycle depth.
        let started = std::time::Instant::now();
        let entries = read_and_sort(dir.path().to_path_buf(), true).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"sub_a"));
        assert!(names.contains(&"sub_b"));
    }

    #[test]
    fn read_file_bytes_rejects_oversize() {
        let dir = tempfile::TempDir::new().unwrap();
        let big = dir.path().join("big.bin");
        // Two bytes over a 1 KiB cap. We pass the cap rather than the
        // public `MAX_FILE_BYTES` so writing 8 MiB on every CI run does
        // not dominate unit-test runtime.
        let data = vec![0u8; 1026];
        std::fs::write(&big, &data).unwrap();
        match read_file_bytes(big, 1024).unwrap_err() {
            FsError::BadRequest(msg) => {
                assert!(msg.contains("exceeds"));
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn read_file_bytes_rejects_symlink() {
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("real");
        std::fs::write(&real, b"x").unwrap();
        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        match read_file_bytes(link, MAX_FILE_BYTES).unwrap_err() {
            FsError::BadRequest(msg) => {
                assert!(msg.contains("symlink"));
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn read_file_bytes_accepts_small_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let f = dir.path().join("small.txt");
        std::fs::write(&f, b"hi").unwrap();
        let bytes = read_file_bytes(f, 8 * 1024).unwrap();
        assert_eq!(bytes, b"hi");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_endpoint_defaults_to_cwd_when_path_empty() {
        // The doc on `list` promises an empty `path` defaults to the process
        // cwd; the test impersonates that contract by chdir-ing into a
        // tempdir and asking for an empty path.
        use axum::extract::Query;
        let dir = make_temp_layout();
        let saved = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let outcome = list(Query(FsRequest {
            path: String::new(),
            show_hidden: true,
        }))
        .await;
        std::env::set_current_dir(&saved).unwrap();
        let Json(body) = outcome.expect("empty-path list should succeed");
        let names: Vec<&str> = body.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Folder", ".hidden", "A_file.txt", "z_file.txt"]);
    }

    /// The file endpoint is the load-bearing seam for the browser-side
    /// preview pane (`src/static/app.js`). For every file kind the
    /// preview pane routes by `Content-Type`, the backend must send
    /// back exactly the MIME the browser's element picker expects:
    /// images/audio/video/PDF/SVG for native tags; text-ish for the
    /// `<pre>` fallback; octet-stream for everything else. mime_guess
    /// occasionally appends `; charset=...` on text/* kinds, so the
    /// test compares on the type/subtype prefix only.
    ///
    /// The expected values are taken from `mime_guess`'s public mapping
    /// for each extension — that crate, not this code, is the source of
    /// truth. If a future mime_guess release changes one of the entries
    /// below, update the test (and the JS routing in `app.js` if the
    /// change crosses a text/binary boundary, e.g. `text/xml` versus
    /// `application/xml`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn file_endpoint_emits_expected_content_type_for_known_extensions() {
        use axum::extract::Query;
        let dir = tempfile::TempDir::new().unwrap();
        let cases: &[(&str, &[u8], &str)] = &[
            ("pixel.png", b"\x89PNG\r\n", "image/png"),
            ("photo.jpeg", b"\xff\xd8\xff", "image/jpeg"),
            ("logo.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>", "image/svg+xml"),
            ("song.mp3", b"ID3\x03", "audio/mpeg"),
            ("clip.mp4", b"\0\0\0 ftyp", "video/mp4"),
            ("sheet.pdf", b"%PDF-fake", "application/pdf"),
            ("page.html", b"<html></html>", "text/html"),
            ("data.json", b"{}", "application/json"),
            ("feed.xml", b"<x/>", "text/xml"),
            ("Cargo.toml", b"[pkg]\nname=\"x\"\n", "text/x-toml"),
            ("doc.txt", b"hi", "text/plain"),
        ];
        for (name, body, expected) in cases {
            let p = dir.path().join(name);
            std::fs::write(&p, body).unwrap();
            let res = file(Query(FsRequest {
                path: p.to_string_lossy().into_owned(),
                show_hidden: true,
            }))
            .await
            .expect("file endpoint should succeed");
            let ct = res
                .headers()
                .get(header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap_or("").to_string())
                .unwrap_or_default();
            let primary = ct.split(';').next().unwrap_or("").trim();
            assert_eq!(
                primary, *expected,
                "wrong Content-Type for {name}: full header was {ct:?}"
            );
        }
    }

    /// Round-trip: a file below the cap returns the bytes verbatim even
    /// when the binary preview branch (image/audio/etc.) is the consumer.
    /// The preview pane never decodes bytes from the listing endpoint,
    /// so the response body is what `<img src=>`/`<audio src=>` actually
    /// reads. Confirms the body is the same bytes we wrote.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn file_endpoint_body_matches_input_bytes() {
        use axum::extract::Query;
        use axum::body::to_bytes;
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("payload.bin");
        let original: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
        std::fs::write(&p, &original).unwrap();
        let res = file(Query(FsRequest {
            path: p.to_string_lossy().into_owned(),
            show_hidden: true,
        }))
        .await
        .expect("file endpoint should succeed");
        let body = to_bytes(res.into_body(), 4096)
            .await
            .expect("body must collect under the cap");
        assert_eq!(body.as_ref(), original.as_slice());
    }
}

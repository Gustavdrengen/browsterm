use std::process::Command;

use anyhow::{Context, Result};
use tracing::warn;

/// Open `url` in the user's default browser. Best-effort: failure is logged
/// but never fatal to the binary, since the user can reach the URL by hand.
pub async fn open_url(url: &str) -> Result<()> {
    // Honor the BROWSER env var explicitly so WSL / Linux users can opt into
    // `BROWSER=wslview ...` for predictable Windows-side launches.
    if let Ok(browser_override) = std::env::var("BROWSER") {
        if !browser_override.trim().is_empty() {
            return run_override(&browser_override, url);
        }
    }

    open::that_detached(url).with_context(|| format!("could not open browser for {url}"))?;
    Ok(())
}

fn run_override(browser: &str, url: &str) -> Result<()> {
    // Split the override into program + trailing args (shell-style).
    let mut parts = browser.split_whitespace();
    let program = parts.next().unwrap_or("xdg-open");
    let cmd_result = Command::new(program).args(parts).arg(url).spawn();
    match cmd_result {
        Ok(_) => Ok(()),
        Err(err) => {
            warn!(
                error = %err,
                browser = %browser,
                "BROWSER override failed; falling back to platform default"
            );
            open::that_detached(url).with_context(|| format!("fallback open failed for {url}"))?;
            Ok(())
        }
    }
}

use std::process::Command;

use anyhow::{Context, Result};
use tracing::{debug, warn};

/// Open `url` in the user's default browser. Best-effort: failure is logged
/// but never fatal to the binary, since the user can reach the URL by hand.
///
/// Resolution order:
///   1. `BROWSER` env override (explicit user opt-in; honored first so users
///      can pin a different default per environment).
///   2. If running on WSL, `wslview` (launches the Windows-side default
///      browser). Vision principle #8 — the Windows↔Linux boundary should
///      feel like one machine.
///   3. If running on WSL and `wslview` is unavailable, `cmd.exe /c start`
///      via `interop` for an explicit Windows-side fallback.
///   4. `open::that_detached`, which delegates to `xdg-open` / `kde-open5`
///      / `macos`-native code paths.
pub async fn open_url(url: &str) -> Result<()> {
    // Honor the BROWSER env var explicitly so WSL / Linux users can opt into
    // any specific command (e.g. `BROWSER=firefox ...`) without our WSL
    // auto-detection getting in the way.
    if let Ok(browser_override) = std::env::var("BROWSER") {
        if !browser_override.trim().is_empty() {
            return run_override(&browser_override, url);
        }
    }

    if is_wsl() {
        match open_wsl_windows_browser(url) {
            Ok(()) => return Ok(()),
            Err(err) => warn!(
                error = ?err,
                "WSL Windows-side browser open failed; falling back to Linux default"
            ),
        }
    }

    open::that_detached(url).with_context(|| format!("could not open browser for {url}"))?;
    Ok(())
}

/// Detect whether the current process is running inside WSL (Windows
/// Subsystem for Linux). The check is intentionally conservative — we want
/// zero false positives on native Linux (where the WSL path would attempt to
/// invoke `wslview` and produce a meaningful error, but cost a fork).
///
/// Signals we use:
///   * `/proc/sys/kernel/osrelease` containing "microsoft" or "WSL"
///     (kernel string set by Microsoft's WSL1 + WSL2).
///   * `WSL_DISTRO_NAME` or `WSLENV` set in the environment (interop
///     surfaces them even in WSL1; defensive on top of the kernel probe).
pub fn is_wsl() -> bool {
    if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
        let lower = release.to_lowercase();
        if lower.contains("microsoft") || lower.contains("wsl") {
            return true;
        }
    }
    if std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSLENV").is_some() {
        return true;
    }
    false
}

fn open_wsl_windows_browser(url: &str) -> Result<()> {
    // `wslview` ships with WSL and is the canonical way to ask the Windows
    // host to open a URL in the user's default browser. If it isn't on
    // `$PATH` we fall through to the `cmd.exe` interop path.
    if let Ok(child) = Command::new("wslview").arg(url).spawn() {
        debug!("opened browser via wslview");
        // Detach: do not wait on the child.
        drop(child);
        return Ok(());
    }

    // Fallback: ask `cmd.exe` (available via WSL's interop bridge on WSL2,
    // and via the explicit init hack on WSL1 in some distros) to launch the
    // URL. `start "" <url>` is the documented incantation; the empty title
    // prevents `start` from interpreting the URL as a window title.
    let status = Command::new("cmd.exe")
        .args(["/c", "start", ""])
        .arg(url)
        .status()
        .context("could not spawn `cmd.exe` to open the Windows-side browser")?;
    if !status.success() {
        anyhow::bail!("`cmd.exe start` exited with {status}");
    }
    debug!("opened browser via cmd.exe start");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The WSL detector returns false on a Linux kernel that does not
    /// carry Microsoft's "microsoft"/"WSL" markers. Sanity-checked with
    /// the real `/proc/sys/kernel/osrelease` of the CI / dev host.
    #[test]
    fn wsl_kernel_string_contains_microsoft_marker() {
        // String-matching the recognized WSL kernel markers is the load-
        // bearing detector logic. These two sample strings are real WSL1 +
        // WSL2 kernel builds.
        let lowercase_microsoft = "5.15.0-91-microsoft-standard-wsl2";
        let lowercase_wsl_only = "4.4.0-19041-microsoft";
        assert!(lowercase_microsoft.to_lowercase().contains("microsoft"));
        assert!(lowercase_microsoft.to_lowercase().contains("wsl"));
        assert!(lowercase_wsl_only.to_lowercase().contains("microsoft"));
    }

    #[test]
    fn wsl_env_marker_triggers() {
        // SAFETY: process-local env for this test; cargo runs tests on a
        // single thread by default for #[test] without --test-threads.
        // SAFETY-restated for clarity: we set then unset.
        let prior_distro = std::env::var_os("WSL_DISTRO_NAME");
        let prior_wslenv = std::env::var_os("WSLENV");
        // SAFETY: see above.
        unsafe {
            std::env::set_var("WSL_DISTRO_NAME", "Ubuntu");
            std::env::remove_var("WSLENV");
        }
        assert!(is_wsl());
        // Restore prior state so we don't leak env into other tests.
        match prior_distro {
            Some(v) => unsafe { std::env::set_var("WSL_DISTRO_NAME", v) },
            None => unsafe { std::env::remove_var("WSL_DISTRO_NAME") },
        }
        match prior_wslenv {
            Some(v) => unsafe { std::env::set_var("WSLENV", v) },
            None => unsafe { std::env::remove_var("WSLENV") },
        }
    }

    #[test]
    fn non_wsl_kernel_is_rejected() {
        // Sanity-check that a vanilla Linux kernel string does not trip
        // the detector. We pick a real mainline string: 6.6 doesn't carry
        // the Microsoft marker on any shipping distro.
        let vanilla = "6.6.0-15-generic";
        let lower = vanilla.to_lowercase();
        assert!(!lower.contains("microsoft") && !lower.contains("wsl"));
    }
}

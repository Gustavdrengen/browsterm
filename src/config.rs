use clap::Parser;

/// Browsterm CLI flags. See `README.md` for documentation.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "browsterm",
    about = "A first-class graphical terminal for any Linux machine, in your browser.",
    version
)]
pub struct Cli {
    /// Address to bind on. Loopback by default per vision principle #1.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to bind on. 0 selects a random free port.
    #[arg(long, default_value_t = 0)]
    pub port: u16,

    /// Shell command to launch in each terminal pane.
    /// Defaults to $SHELL; falls back to /bin/sh on Unix if $SHELL is unset.
    #[arg(long)]
    pub shell: Option<String>,

    /// Arguments passed to the shell. Empty by default.
    #[arg(long, value_delimiter = ' ', num_args = 0..)]
    pub shell_args: Vec<String>,

    /// Do not auto-open a browser tab on launch.
    #[arg(long, default_value_t = false)]
    pub no_browser: bool,

    /// `tracing-subscriber` filter, e.g. `info`, `debug`, `warn,browsterm=trace`.
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

impl Cli {
    /// Resolve the shell command, applying the documented fallback.
    pub fn resolved_shell(&self) -> (String, Vec<String>) {
        if let Some(shell) = self.shell.clone() {
            return (shell, self.shell_args.clone());
        }
        if let Ok(shell) = std::env::var("SHELL") {
            if !shell.is_empty() {
                return (shell, Vec::new());
            }
        }
        // Linux / macOS fallback. Windows would need its own branch when we add it.
        ("/bin/sh".to_string(), Vec::new())
    }
}

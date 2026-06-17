//! Compile-time embedding of the workspace UI HTML / CSS / JS that we author.
//!
//! Third-party browser libraries (xterm.js, fit addon, and friends later) load
//! over the network and are cached by the browser. The binary stays offline-
//! usable after first load because the browser caches CDN responses, and
//! because nothing in this binary auto-updates — that's a product property,
//! not an artifact of vendoring.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "src/static"]
pub struct ServerAssets;

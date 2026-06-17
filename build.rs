fn main() {
    // Tell cargo to rerun this build script if any embedded asset changes.
    // rust-embed itself watches its target folder, but explicit rerun-if-changed
    // makes incremental builds feel predictable when the agent edits only assets.
    println!("cargo:rerun-if-changed=src/static");
}

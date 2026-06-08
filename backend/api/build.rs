use std::path::Path;

fn main() {
    // rust-embed reads frontend/dist at macro-expansion time, so cargo would
    // otherwise keep a stale release bundle when only the built UI changed.
    // Re-run (and re-embed) whenever the built assets change.
    println!("cargo:rerun-if-changed=../../frontend/dist");

    // In release builds the UI is embedded into the binary. Fail fast with a
    // clear message if it has not been built yet — scripts/start.sh builds the
    // frontend first, so this only trips on a manual `cargo build --release`.
    // Debug builds are unaffected (rust-embed serves dist from disk there).
    if std::env::var("PROFILE").as_deref() == Ok("release")
        && !Path::new("../../frontend/dist/index.html").exists()
    {
        panic!(
            "frontend/dist/index.html missing — build the frontend before a release build \
             (run scripts/start.sh, or `pnpm --dir frontend build`)."
        );
    }
}

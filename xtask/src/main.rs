//! Cross-platform task runner (replaces Makefile / scripts/start.sh).
//!
//! Usage:
//!   cargo dev                 # build UI, then build + run the release server
//!   cargo xtask dev -- --flag # forward extra args to the api binary
//!   cargo xtask build         # build frontend + release backend
//!   cargo xtask clean         # remove frontend/dist and cargo artifacts
//!
//! Set SKIP_FRONTEND=1 to reuse an existing frontend/dist.
//!
//! Configuration is loaded from .env in the repository root (variables already
//! present in the environment take precedence). On Unix the cargo runner
//! (scripts/dev-runner.sh) does the same for direct `cargo run` / `cargo test`
//! invocations; on Windows this loader is the only mechanism.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let task = args.next().unwrap_or_default();
    let extra: Vec<String> = args.filter(|a| a != "--").collect();

    let root = workspace_root();
    // Ignore a missing .env: required variables may come from the shell.
    let _ = dotenvy::from_path(root.join(".env"));

    let result = match task.as_str() {
        "dev" | "run" => dev(&root, &extra),
        "build" => build(&root),
        "clean" => clean(&root),
        _ => {
            eprintln!("usage: cargo xtask <dev|build|clean> [-- <api args>]");
            eprintln!("       cargo dev   (alias for `cargo xtask dev`)");
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn dev(root: &Path, extra: &[String]) -> Result<(), String> {
    ensure_frontend_dist(root)?;

    println!("==> Building & starting backend (release)…");
    let mut cmd = cargo();
    cmd.current_dir(root)
        .args(["run", "--release", "-p", "api", "--"])
        .args(extra);
    run(cmd)
}

fn build(root: &Path) -> Result<(), String> {
    ensure_frontend_dist(root)?;

    println!("==> Building backend (release)…");
    let mut cmd = cargo();
    cmd.current_dir(root)
        .args(["build", "--release", "-p", "api"]);
    run(cmd)
}

fn clean(root: &Path) -> Result<(), String> {
    let dist = root.join("frontend").join("dist");
    if dist.exists() {
        std::fs::remove_dir_all(&dist)
            .map_err(|e| format!("failed to remove {}: {e}", dist.display()))?;
    }

    let mut cmd = cargo();
    cmd.current_dir(root).arg("clean");
    run(cmd)
}

/// Builds the frontend unless SKIP_FRONTEND=1, then verifies that
/// frontend/dist/index.html exists so rust-embed has something to bundle.
fn ensure_frontend_dist(root: &Path) -> Result<(), String> {
    let frontend = root.join("frontend");

    if env::var("SKIP_FRONTEND").as_deref() == Ok("1") {
        println!("==> Skipping frontend build (SKIP_FRONTEND=1)");
    } else {
        println!("==> Building frontend (release)…");
        if !frontend.join("node_modules").is_dir() {
            let mut install = bun();
            install
                .current_dir(&frontend)
                .args(["install", "--frozen-lockfile"]);
            run(install)?;
        }
        let mut build = bun();
        // --bun shims `node` invocations inside the build script to bun.
        build.current_dir(&frontend).args(["run", "--bun", "build"]);
        run(build)?;
    }

    if !frontend.join("dist").join("index.html").is_file() {
        return Err("frontend/dist/index.html missing — cannot embed the UI. \
             Run without SKIP_FRONTEND=1 to build the frontend first."
            .to_string());
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    // xtask lives in <root>/xtask, so the repository root is one level up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest dir has a parent")
        .to_path_buf()
}

fn cargo() -> Command {
    Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
}

fn bun() -> Command {
    // Bun ships a native bun.exe on Windows, so no cmd.exe shim is needed.
    Command::new("bun")
}

fn run(mut cmd: Command) -> Result<(), String> {
    let status = cmd
        .status()
        .map_err(|e| format!("failed to start {:?}: {e}", cmd.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{:?} exited with {status}", cmd.get_program()))
    }
}

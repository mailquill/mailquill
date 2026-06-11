# Thin compatibility wrapper — the actual task logic lives in the
# cross-platform cargo xtask (see xtask/src/main.rs). Prefer `cargo dev`,
# `cargo xtask build`, and `cargo xtask clean` directly.
.PHONY: build run clean

build:
	cargo xtask build

# Single-command startup: build the UI, then run the release server with the
# frontend embedded via rust-embed.
run:
	cargo dev

clean:
	cargo xtask clean

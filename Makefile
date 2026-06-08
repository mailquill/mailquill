.PHONY: build build-frontend build-backend run clean

build: build-frontend build-backend

# Single-command startup: build the UI, then run the release server with the
# frontend embedded via rust-embed.
run:
	./scripts/start.sh

build-frontend:
	cd frontend && pnpm run build

build-backend: build-frontend
	cd backend && cargo build --release

clean:
	cd frontend && rm -rf dist
	cd backend && cargo clean

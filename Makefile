.PHONY: all client run-client daemon run-daemon test check tailscale clean help

all: check test

# Run Slint Native GPU UI client
client: run-client
run-client:
	cargo run -p nexus-client

# Run Headless Tokio Daemon actor
daemon: run-daemon
run-daemon:
	cargo run -p nexus-daemon

# Verify workspace strict zero-warnings policy
check:
	cargo check --workspace

# Run all workspace unit tests
test:
	cargo test --workspace

# Display Tailscale P2P Mesh IP status
tailscale:
	@if command -v tailscale >/dev/null 2>&1; then \
		echo "🌐 Tailscale IPv4: $$(tailscale ip -4 2>/dev/null || echo 'Not connected')"; \
		tailscale status 2>/dev/null || true; \
	else \
		echo "Tailscale is not installed in current PATH."; \
	fi

clean:
	cargo clean

help:
	@echo "Nexus Suite Developer Commands:"
	@echo "  make client      - Build & run Slint GUI Client"
	@echo "  make daemon      - Build & run Headless Tokio Daemon"
	@echo "  make check       - Verify workspace compilation"
	@echo "  make test        - Run unit tests across all crates"
	@echo "  make tailscale   - Display Tailscale P2P mesh status"

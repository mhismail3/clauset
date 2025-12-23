# Clauset

A secure Claude Code session management dashboard accessible from iPhone via Tailscale.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Mac Mini (Always On)                     │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    Clauset Server (Rust)                    ││
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ ││
│  │  │ REST API    │  │ WebSocket   │  │ Static File Server  │ ││
│  │  │ (Axum)      │  │ (Real-time) │  │ (PWA Assets)        │ ││
│  │  └─────────────┘  └─────────────┘  └─────────────────────┘ ││
│  └─────────────────────────────────────────────────────────────┘│
│                              ↕                                   │
│                    Tailscale Network                             │
└──────────────────────────────────────────────────────────────────┘
                               ↕
                    ┌──────────────────────┐
                    │   iPhone (Safari)    │
                    │   ┌──────────────┐   │
                    │   │  PWA         │   │
                    │   │  (SolidJS)   │   │
                    │   └──────────────┘   │
                    └──────────────────────┘
```

## Features

- **Multi-session management**: Create, view, and manage multiple Claude Code sessions
- **Hybrid interface**: Chat-style UI with option to drop into raw terminal
- **Real-time streaming**: Live Claude responses via WebSocket
- **Mobile-first PWA**: Installable on iPhone home screen
- **Tailscale security**: Access only via your private Tailscale network

## Quick Start

### Prerequisites

- Rust 1.75+ (for the backend)
- Node.js 20+ (for the frontend)
- Claude Code CLI installed (`/opt/homebrew/bin/claude`)
- Tailscale configured on both Mac Mini and iPhone

### Development

```bash
# Terminal 1: Run the Rust backend
cargo run -p clauset-server

# Terminal 2: Run the frontend dev server
cd frontend && npm install && npm run dev
```

Access the dashboard at `http://localhost:5173` (development) or via your Tailscale IP.

### Production Build

```bash
# Build everything
cargo build --release
cd frontend && npm run build

# Run the server (serves frontend from ./frontend/dist)
./target/release/clauset-server
```

### Rebuilding After Changes

```bash
pkill -f clauset-server
cargo build --release
./target/release/clauset-server
```

### Logging

The server supports multiple logging modes via CLI flags:

```bash
./target/release/clauset-server        # Production (minimal logs)
./target/release/clauset-server -v     # Verbose (operational detail)
./target/release/clauset-server -d     # Debug (troubleshooting)
./target/release/clauset-server -q     # Quiet (warnings/errors only)
./target/release/clauset-server --log-format json  # JSON output
```

See [docs/logging.md](docs/logging.md) for full documentation on log targets and debugging specific subsystems.

### Auto-start on Boot

```bash
./scripts/install-launchd.sh
```

This creates a launchd service that starts the server automatically when your Mac Mini boots.

## Configuration

Edit `config/default.toml`:

```toml
host = "0.0.0.0"
port = 8080
static_dir = "./frontend/dist"
claude_path = "/opt/homebrew/bin/claude"
max_concurrent_sessions = 10
default_model = "sonnet"
```

## Access from iPhone

1. Ensure both your Mac Mini and iPhone are on the same Tailscale network
2. Open Safari on your iPhone
3. Navigate to `http://<mac-mini-tailscale-ip>:8080`
4. Tap "Add to Home Screen" to install as a PWA

## Project Structure

```
clauset/
├── crates/
│   ├── clauset-types/     # Shared type definitions
│   ├── clauset-core/      # Session & process management
│   └── clauset-server/    # HTTP/WebSocket server
├── frontend/              # SolidJS PWA
├── config/                # Server configuration
└── scripts/               # Deployment scripts
```

## Tech Stack

**Backend:**
- Rust with Axum (HTTP/WebSocket)
- SQLite (session persistence)
- portable-pty (terminal mode)

**Frontend:**
- SolidJS (reactive UI)
- Tailwind CSS v4 (styling)
- xterm.js (terminal emulation)
- Vite + PWA plugin

## License

MIT

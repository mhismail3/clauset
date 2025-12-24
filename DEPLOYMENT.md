# Clauset Deployment Guide

This guide covers setting up Clauset for persistent operation on macOS (Mac Mini) with a dual-environment workflow: **production** (always running via launchd) and **beta** (development/testing in terminal).

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Mac Mini                                     │
│                                                                      │
│  ┌─────────────────────────┐    ┌─────────────────────────────┐     │
│  │   PRODUCTION (launchd)  │    │   BETA (terminal)           │     │
│  │                         │    │                             │     │
│  │  Port: 8080             │    │  Port: 8081                 │     │
│  │  Binary: ~/.local/bin/  │    │  Binary: ./target/release/  │     │
│  │  Config: production.toml│    │  Config: beta.toml          │     │
│  │  DB: sessions.db        │    │  DB: sessions-beta.db       │     │
│  │  Logs: ~/.local/share/  │    │  Logs: terminal stdout      │     │
│  │                         │    │                             │     │
│  │  ┌─────────────────┐    │    │  ┌─────────────────────┐    │     │
│  │  │ Rust Backend    │    │    │  │ Rust Backend        │    │     │
│  │  │ + Static Files  │    │    │  │ (API only)          │    │     │
│  │  └─────────────────┘    │    │  └─────────────────────┘    │     │
│  │                         │    │            ↑                │     │
│  └─────────────────────────┘    │            │ proxy          │     │
│             ↑                   │  ┌─────────────────────┐    │     │
│             │                   │  │ Vite Dev Server     │    │     │
│   Access: :8080                 │  │ Port: 5173          │    │     │
│   (API + Frontend)              │  │ (Hot Reload)        │    │     │
│                                 │  └─────────────────────┘    │     │
│                                 │            ↑                │     │
│                                 │   Access: :5173             │     │
│                                 └─────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────┘
```

| Environment | Backend Port | Frontend Access | Database | Use Case |
|-------------|--------------|-----------------|----------|----------|
| Production  | 8080         | `http://<host>:8080` | `sessions.db` | Always-on, accessed via iPhone/Tailscale |
| Beta        | 8081         | `http://localhost:5173` | `sessions-beta.db` | Development with hot reload |

---

## Prerequisites

Before setup, ensure you have:

- **Rust 1.75+**: `rustup update stable`
- **Node.js 20+**: `node --version`
- **Claude Code CLI**: Installed at `/opt/homebrew/bin/claude`
- **Tailscale** (optional): For remote access from iPhone

---

## Initial Setup

### Step 1: Clone and Build

```bash
# Clone the repository
cd ~/Downloads/projects
git clone <repo-url> clauset
cd clauset

# Build the release binary
cargo build --release

# Build the frontend
cd frontend
npm install
npm run build
cd ..
```

### Step 2: Install the Service

Run the install command to set up launchd and the CLI:

```bash
./scripts/clauset install
```

This performs the following:
1. Creates `~/.local/bin/` directory
2. Creates `~/.local/share/clauset/` for logs and database
3. Copies the release binary to `~/.local/bin/clauset-server`
4. Creates the launchd plist at `~/Library/LaunchAgents/com.clauset.server.plist`
5. Symlinks `clauset` CLI to `~/.local/bin/clauset`
6. Starts the production service

### Step 3: Add to PATH

Add `~/.local/bin` to your shell profile if not already present:

```bash
# For zsh (~/.zshrc)
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc

# For bash (~/.bash_profile)
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bash_profile
source ~/.bash_profile
```

### Step 4: Verify Installation

```bash
clauset status
```

Expected output:
```
Clauset Service Status
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ Production service: RUNNING (PID: 12345)
  Port: 8080
  Logs: /Users/<you>/.local/share/clauset/server.log
  Uptime: 00:01:23
  Beta server: not running

Recent logs:
  2024-01-15T10:30:00 INFO clauset::startup - Starting server on 0.0.0.0:8080
```

---

## Managing the Production Service

### Service Commands

| Command | Description |
|---------|-------------|
| `clauset status` | Show service status, PID, uptime, recent logs |
| `clauset start` | Start the production service |
| `clauset stop` | Stop the production service |
| `clauset restart` | Stop and start the service |
| `clauset logs` | Tail production logs (Ctrl+C to stop) |
| `clauset errors` | Show error log contents |

### What launchd Provides

The launchd configuration (`~/Library/LaunchAgents/com.clauset.server.plist`) ensures:

- **Auto-start on boot**: Service starts when you log in
- **Auto-restart on crash**: If the process exits unexpectedly, launchd restarts it
- **Logging**: stdout → `~/.local/share/clauset/server.log`, stderr → `server.err`

### Manual launchctl Commands

If you need to interact with launchd directly:

```bash
# Load (start) the service
launchctl load ~/Library/LaunchAgents/com.clauset.server.plist

# Unload (stop) the service
launchctl unload ~/Library/LaunchAgents/com.clauset.server.plist

# Check if loaded
launchctl list | grep clauset
```

### Log Locations

| File | Contents |
|------|----------|
| `~/.local/share/clauset/server.log` | Application logs (info, warnings) |
| `~/.local/share/clauset/server.err` | Error output and stack traces |
| `~/.local/share/clauset/sessions.db` | Production SQLite database |

### Viewing Logs

```bash
# Tail logs in real-time
clauset logs

# View last 100 lines
tail -100 ~/.local/share/clauset/server.log

# Search for errors
grep -i error ~/.local/share/clauset/server.log

# View error log
clauset errors
```

---

## Beta Development Workflow

Beta development uses a completely isolated environment with its own database and port.

### Step 1: Start the Beta Backend

In **Terminal 1**:

```bash
clauset beta
```

This:
- Builds the release binary if needed
- Starts the server on port **8081**
- Uses `config/beta.toml` configuration
- Uses separate database: `~/.local/share/clauset/sessions-beta.db`
- Logs to terminal stdout (verbose mode enabled)

Output:
```
▸ Starting beta server on port 8081...
  Config: config/beta.toml
  Database: ~/.local/share/clauset/sessions-beta.db

Press Ctrl+C to stop
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
2024-01-15T10:35:00 INFO clauset::startup - Starting server on 0.0.0.0:8081
```

### Step 2: Start the Frontend Dev Server

In **Terminal 2**:

```bash
cd frontend
CLAUSET_BACKEND_PORT=8081 npm run dev
```

This:
- Starts Vite dev server on port **5173**
- Enables hot module replacement (HMR)
- Proxies `/api/*` and `/ws/*` to the beta backend on 8081

### Step 3: Access Beta

Open your browser to:

```
http://localhost:5173
```

You now have:
- Hot reload for frontend changes (instant updates)
- Backend changes require restarting `clauset beta`
- Completely isolated from production data

### Beta Development Tips

**Frontend-only changes:**
```bash
# Just edit files - Vite hot reloads automatically
# No restart needed
```

**Backend changes:**
```bash
# Terminal 1: Ctrl+C to stop beta
# Make your Rust changes
# Restart:
clauset beta
```

**Quick backend rebuild without restart:**
```bash
# In another terminal, while beta is running:
cargo build --release

# Then Ctrl+C and restart clauset beta
```

**Run backend tests:**
```bash
cargo test
```

**Run frontend type check:**
```bash
cd frontend
npm run check  # if configured
```

---

## Deploying to Production

When you're happy with your beta changes, deploy them to production.

### Deploy Command

```bash
clauset deploy
```

This performs the following steps in order:

1. **Run tests**: `cargo test` — aborts if any test fails
2. **Build release binary**: `cargo build --release`
3. **Build frontend**: `cd frontend && npm run build`
4. **Stop production service**: `launchctl unload ...`
5. **Copy binary**: `cp target/release/clauset-server ~/.local/bin/`
6. **Start production service**: `launchctl load ...`

Output:
```
Deploying to Production
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
▸ Running tests...
   Compiling clauset-server v0.1.0
   ...
   test result: ok. 43 passed; 0 failed
✓ Tests passed
▸ Building release binary...
✓ Binary built
▸ Building frontend...
✓ Frontend built
▸ Stopping production service...
▸ Installing binary to /Users/<you>/.local/bin/clauset-server...
✓ Binary installed
▸ Starting production service...
✓ Production service started

✓ Deploy complete!
```

### Verify Deployment

```bash
clauset status
clauset logs
```

Access production at `http://<mac-mini-ip>:8080` or via Tailscale.

---

## Rollback Procedure

If a deployment causes issues, you can rollback to a previous version.

### Option 1: Git Checkout + Redeploy

```bash
# Find the last working commit
git log --oneline -10

# Checkout that commit
git checkout <commit-hash>

# Redeploy
clauset deploy
```

### Option 2: Quick Binary Rollback

If you have a known-good binary backed up:

```bash
# Stop production
clauset stop

# Restore old binary
cp /path/to/backup/clauset-server ~/.local/bin/clauset-server

# Restart
clauset start
```

### Option 3: Frontend-Only Rollback

If only the frontend is broken:

```bash
# Checkout old frontend code
git checkout <commit-hash> -- frontend/

# Rebuild frontend only
cd frontend && npm run build && cd ..

# Restart service (picks up new static files)
clauset restart
```

### Creating Backups Before Deploy

Add this to your workflow before deploying:

```bash
# Backup current binary
cp ~/.local/bin/clauset-server ~/.local/bin/clauset-server.backup

# Backup current frontend
cp -r frontend/dist frontend/dist.backup

# Then deploy
clauset deploy
```

---

## Configuration Files

### config/production.toml

```toml
# Server settings
host = "0.0.0.0"
port = 8080

# Path to frontend static files (relative to working directory)
static_dir = "./frontend/dist"

# Claude CLI path
claude_path = "/opt/homebrew/bin/claude"

# Database path (production sessions)
db_path = "~/.local/share/clauset/sessions.db"

# Maximum concurrent sessions
max_concurrent_sessions = 10

# Default model for new sessions
default_model = "sonnet"
```

### config/beta.toml

```toml
# Server settings (different port from production)
host = "0.0.0.0"
port = 8081

# Path to frontend static files
static_dir = "./frontend/dist"

# Claude CLI path
claude_path = "/opt/homebrew/bin/claude"

# Database path (separate from production!)
db_path = "~/.local/share/clauset/sessions-beta.db"

# Maximum concurrent sessions
max_concurrent_sessions = 10

# Default model for new sessions
default_model = "sonnet"
```

### Customizing Configuration

You can override settings via CLI flags:

```bash
# Use custom config file
./target/release/clauset-server --config /path/to/custom.toml

# Override port
./target/release/clauset-server --config config/beta.toml --port 9000

# Verbose logging
./target/release/clauset-server -v

# Debug logging
./target/release/clauset-server -d
```

---

## Accessing from iPhone via Tailscale

### Setup

1. Install Tailscale on both Mac Mini and iPhone
2. Ensure both devices are on the same Tailnet
3. Note your Mac Mini's Tailscale IP (e.g., `100.x.y.z`)

### Access

On iPhone Safari, navigate to:

```
http://100.x.y.z:8080
```

### Save as PWA

1. Tap the Share button in Safari
2. Select "Add to Home Screen"
3. Name it "Claude Sessions"
4. Tap Add

The app will now appear on your home screen and launch in full-screen mode.

---

## Troubleshooting

### Service Won't Start

```bash
# Check error log
clauset errors

# Check if port is in use
lsof -i :8080

# Check launchd status
launchctl list | grep clauset

# Try running manually to see errors
~/.local/bin/clauset-server --config /path/to/clauset/config/production.toml -v
```

### Database Locked

```bash
# Stop all instances
clauset stop
pkill -f clauset-server

# Check for lock files
ls -la ~/.local/share/clauset/

# Restart
clauset start
```

### Frontend Not Loading

```bash
# Verify frontend was built
ls -la frontend/dist/

# Rebuild if missing
cd frontend && npm run build && cd ..

# Restart service
clauset restart
```

### Beta Not Connecting to Backend

```bash
# Verify beta backend is running
curl http://localhost:8081/api/health

# Check Vite is proxying correctly
# Look for proxy logs in Vite output

# Verify environment variable
echo $CLAUSET_BACKEND_PORT  # should be 8081
```

### High Memory/CPU Usage

```bash
# Check resource usage
ps aux | grep clauset

# View active sessions
curl http://localhost:8080/api/sessions

# Restart to clear state
clauset restart
```

---

## Uninstalling

To completely remove the Clauset service:

```bash
clauset uninstall
```

This removes:
- launchd plist
- `~/.local/bin/clauset` symlink
- `~/.local/bin/clauset-server` binary

**Data is preserved** in `~/.local/share/clauset/`. To remove data:

```bash
rm -rf ~/.local/share/clauset/
```

---

## Quick Reference

```bash
# === Production ===
clauset status          # Check if running
clauset start           # Start service
clauset stop            # Stop service
clauset restart         # Restart service
clauset logs            # Tail logs
clauset errors          # Show error log
clauset deploy          # Build and deploy

# === Beta Development ===
clauset beta            # Start beta backend (port 8081)
CLAUSET_BACKEND_PORT=8081 npm run dev --prefix frontend  # Start frontend

# === Access URLs ===
# Production: http://<host>:8080
# Beta:       http://localhost:5173

# === Setup ===
clauset install         # First-time setup
clauset uninstall       # Remove service
```

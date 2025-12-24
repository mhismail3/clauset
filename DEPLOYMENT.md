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
| Beta (dev mode) | 8081     | `http://<host>:5173` | `sessions-beta.db` | Development with hot reload |
| Beta (--serve)  | 8081     | `http://<host>:8081` | `sessions-beta.db` | Test production-like setup remotely |

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

### Option A: Development Mode (Hot Reload)

Best for active frontend development with instant updates.

**Terminal 1 — Start beta backend:**
```bash
clauset beta
```

**Terminal 2 — Start Vite dev server:**
```bash
cd frontend
CLAUSET_BACKEND_PORT=8081 npm run dev
```

**Access URLs:**
| From | URL |
|------|-----|
| Local machine | `http://localhost:5173` |
| Other devices (Tailscale) | `http://<mac-mini-ip>:5173` |

The Vite dev server listens on all interfaces (`0.0.0.0`) and proxies API/WebSocket requests to the beta backend. Your iPhone can connect to port 5173 and get hot reload too!

### Option B: Production-Like Mode (Built Frontend)

Best for testing the exact production setup before deploying.

**Single terminal:**
```bash
clauset beta --serve
```

This:
- Builds the frontend (`npm run build`)
- Starts the server on port **8081**
- Serves static files from `frontend/dist/`
- Works exactly like production, just on a different port with a separate database

**Access URL:**
```
http://<mac-mini-ip>:8081
```

No hot reload, but you're testing the real production configuration.

### Comparison

| Mode | Command | Access | Hot Reload | Use Case |
|------|---------|--------|------------|----------|
| Dev | `clauset beta` + `npm run dev` | `:5173` | ✅ Yes | Active frontend development |
| Prod-like | `clauset beta --serve` | `:8081` | ❌ No | Test before deploy, remote testing |

### Beta Environment Details

Both modes use:
- **Config**: `config/beta.toml`
- **Port**: 8081
- **Database**: `~/.local/share/clauset/sessions-beta.db` (separate from production!)
- **Logs**: Terminal stdout (verbose mode)

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

Each deployment records the git commit hash. If something breaks, rollback to the last known-good state.

### Quick Rollback (Recommended)

```bash
clauset rollback
```

This will:
1. Show the current vs deployed commit
2. Ask for confirmation
3. Stash any uncommitted changes (recoverable with `git stash pop`)
4. Restore working directory to the deployed commit
5. Rebuild backend and frontend
6. Restart the production service

### Check Deployed Commit

```bash
clauset status
```

Shows the currently deployed commit hash and message:
```
✓ Production service: RUNNING (PID: 12345)
  Port: 8080
  Deployed: 4156cf0 - Fix clauset CLI symlink resolution
```

### Manual Rollback Options

**If you know the specific commit:**
```bash
git checkout <commit-hash> -- .
cargo build --release && cd frontend && npm run build && cd ..
clauset restart
```

**Frontend-only fix:**
```bash
git checkout <commit-hash> -- frontend/
cd frontend && npm run build && cd ..
clauset restart
```

### Recovering Stashed Changes

If rollback stashed your changes and you want them back:
```bash
git stash list          # See stashed changes
git stash pop           # Restore most recent stash
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

## Power Failure Recovery

This section documents how to configure your Mac Mini for automatic recovery after power outages.

### Recovery Chain

When power is restored after an outage, the following sequence occurs:

```
Power restored
    → Mac auto-boots (auto-boot=true in NVRAM)
    → macOS auto-restarts (autorestart=1 in pmset)
    → FileVault unlock screen (if enabled) - requires password
    → Auto-login (if FileVault disabled and autologin configured)
    → LaunchAgent loads (RunAtLoad=true)
    → Clauset service starts
    → If service crashes, launchd restarts it (KeepAlive)
```

### Prerequisites

Your Mac should already have these power settings configured:

```bash
# Verify power settings
pmset -g | grep autorestart  # Should be 1
pmset -g | grep sleep        # Should be 0
nvram -p | grep auto-boot    # Should be true
```

If not set, configure them:

```bash
# Auto-restart after power failure
sudo pmset -a autorestart 1

# Prevent sleep
sudo pmset -a sleep 0

# Auto-boot when power is connected (usually enabled by default)
sudo nvram auto-boot=true
```

### FileVault Consideration

If FileVault (disk encryption) is enabled, automatic login is blocked. After a power outage:
- Mac boots to FileVault unlock screen
- Enter your password once to unlock the disk
- macOS boots and the service starts automatically

To check FileVault status:
```bash
fdesetup status
```

### Enabling Automatic Login (Optional)

**Note:** Only works if FileVault is disabled. Reduces security - anyone with physical access gets full account access.

```bash
# Enable auto-login for your user
sudo sysadminctl -autologin set -userName <your-username>

# Check status
sysadminctl -autologin status
```

### launchd Resilience Features

The launchd configuration includes:

| Setting | Value | Purpose |
|---------|-------|---------|
| `RunAtLoad` | true | Start service when user logs in |
| `KeepAlive.SuccessfulExit` | false | Restart if service crashes |
| `ThrottleInterval` | 10 | Wait 10 seconds between restart attempts |

### Testing Recovery

To simulate a power failure recovery:

```bash
# Reboot the Mac
sudo shutdown -r now
```

After reboot, verify the service is running:

```bash
clauset status
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
clauset beta            # Start beta backend only (port 8081)
clauset beta --serve    # Start beta with built frontend (port 8081)
clauset rollback        # Restore to last deployed commit
CLAUSET_BACKEND_PORT=8081 npm run dev --prefix frontend  # Vite dev server

# === Access URLs ===
# Production:     http://<host>:8080
# Beta (dev):     http://<host>:5173  (Vite, hot reload)
# Beta (--serve): http://<host>:8081  (built frontend, no hot reload)

# === Setup ===
clauset install         # First-time setup
clauset uninstall       # Remove service
```

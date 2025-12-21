#!/bin/bash
# Install Clauset as a launchd service for auto-start on boot

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
PLIST_NAME="com.clauset.server"
PLIST_PATH="$HOME/Library/LaunchAgents/$PLIST_NAME.plist"
BINARY_PATH="$PROJECT_DIR/target/release/clauset-server"

# Check if binary exists
if [ ! -f "$BINARY_PATH" ]; then
    echo "Error: Binary not found at $BINARY_PATH"
    echo "Please run 'cargo build --release' first."
    exit 1
fi

# Create LaunchAgents directory if it doesn't exist
mkdir -p "$HOME/Library/LaunchAgents"

# Create plist file
cat > "$PLIST_PATH" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$PLIST_NAME</string>

    <key>ProgramArguments</key>
    <array>
        <string>$BINARY_PATH</string>
    </array>

    <key>WorkingDirectory</key>
    <string>$PROJECT_DIR</string>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>

    <key>StandardOutPath</key>
    <string>$HOME/.local/share/clauset/server.log</string>

    <key>StandardErrorPath</key>
    <string>$HOME/.local/share/clauset/server.err</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>clauset_server=info,clauset_core=info</string>
    </dict>
</dict>
</plist>
EOF

# Create log directory
mkdir -p "$HOME/.local/share/clauset"

echo "Created launchd plist at: $PLIST_PATH"

# Load the service
launchctl unload "$PLIST_PATH" 2>/dev/null || true
launchctl load "$PLIST_PATH"

echo "Service loaded. Clauset server will now start on boot."
echo ""
echo "Useful commands:"
echo "  Start:   launchctl load $PLIST_PATH"
echo "  Stop:    launchctl unload $PLIST_PATH"
echo "  Status:  launchctl list | grep clauset"
echo "  Logs:    tail -f ~/.local/share/clauset/server.log"

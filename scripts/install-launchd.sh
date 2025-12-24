#!/bin/bash
# Install Clauset as a launchd service for auto-start on boot
# This is a convenience wrapper - the main CLI is `clauset`

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing Clauset service..."
echo ""

# Run the main install command
exec "$SCRIPT_DIR/clauset" install

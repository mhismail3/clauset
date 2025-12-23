#!/bin/bash
# Clauset Hook Script - Forwards Claude Code events to Clauset dashboard
#
# This script is called by Claude Code hooks and sends structured events
# to the Clauset server for real-time activity tracking.
#
# Environment variables (set by Clauset when spawning Claude):
#   CLAUSET_SESSION_ID - Clauset's internal session UUID
#   CLAUSET_URL - Base URL of Clauset server (default: http://localhost:8080)
#
# The script is lightweight and exits immediately if CLAUSET_SESSION_ID is not set,
# so it has zero impact on non-Clauset Claude sessions.

set -euo pipefail

# Read JSON input from stdin (Claude provides hook data as JSON)
INPUT=$(cat)

# Exit immediately if not a Clauset-managed session
# This makes the hook a no-op for regular Claude usage
if [[ -z "${CLAUSET_SESSION_ID:-}" ]]; then
    exit 0
fi

# Configuration with defaults
CLAUSET_URL="${CLAUSET_URL:-http://localhost:8080}"

# Add Clauset session ID to the payload
# Uses jq for reliable JSON manipulation
PAYLOAD=$(echo "$INPUT" | jq -c --arg sid "$CLAUSET_SESSION_ID" '. + {clauset_session_id: $sid}')

# Fire-and-forget HTTP POST to Clauset
# - Run in background (&) so we don't block Claude
# - Short timeout (0.5s) to fail fast if server is down
# - Suppress all output to avoid polluting Claude's terminal
# - || true to never return non-zero (don't trigger hook failure)
(curl -s -X POST "$CLAUSET_URL/api/hooks" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD" \
    --max-time 0.5 \
    --connect-timeout 0.2 \
    &>/dev/null || true) &

# Exit successfully - never block Claude
exit 0

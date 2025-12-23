#!/bin/bash
# Clauset Hooks Setup Script
#
# This script installs Claude Code hooks for Clauset integration.
# Run this once to enable real-time activity tracking in the Clauset dashboard.
#
# Prerequisites:
#   - jq (for JSON processing)
#   - curl (for HTTP requests)
#
# Usage:
#   ./setup-hooks.sh [--uninstall]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_DIR="$HOME/.claude"
HOOKS_DIR="$CLAUDE_DIR/hooks"
SETTINGS_FILE="$CLAUDE_DIR/settings.json"
HOOK_SCRIPT="$HOOKS_DIR/clauset-hook.sh"
HOOK_SETTINGS="$SCRIPT_DIR/clauset-hooks-settings.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_prerequisites() {
    local missing=()

    if ! command -v jq &>/dev/null; then
        missing+=("jq")
    fi

    if ! command -v curl &>/dev/null; then
        missing+=("curl")
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        echo "Please install them first:"
        echo "  brew install ${missing[*]}"
        exit 1
    fi
}

install_hooks() {
    log_info "Installing Clauset hooks..."

    # Create directories
    mkdir -p "$HOOKS_DIR"

    # Copy hook script
    cp "$SCRIPT_DIR/clauset-hook.sh" "$HOOK_SCRIPT"
    chmod +x "$HOOK_SCRIPT"
    log_info "Installed hook script to $HOOK_SCRIPT"

    # Merge hook settings
    if [[ -f "$SETTINGS_FILE" ]]; then
        log_info "Merging hooks into existing settings..."

        # Backup existing settings
        cp "$SETTINGS_FILE" "$SETTINGS_FILE.backup"
        log_info "Backed up existing settings to $SETTINGS_FILE.backup"

        # Merge hooks - existing hooks take precedence for same event types
        # We add clauset hooks, but don't overwrite existing hooks
        local new_hooks
        new_hooks=$(jq -s '
            .[0] as $existing |
            .[1] as $new |
            $existing * {
                hooks: (
                    ($existing.hooks // {}) as $eh |
                    ($new.hooks // {}) as $nh |
                    $nh * $eh
                )
            }
        ' "$SETTINGS_FILE" "$HOOK_SETTINGS")

        echo "$new_hooks" > "$SETTINGS_FILE"
        log_info "Merged hook settings into $SETTINGS_FILE"
    else
        log_info "Creating new settings file..."
        cp "$HOOK_SETTINGS" "$SETTINGS_FILE"
        log_info "Created $SETTINGS_FILE"
    fi

    log_info "Clauset hooks installed successfully!"
    echo ""
    echo "The hooks are now active for all Claude Code sessions."
    echo "When running Claude through Clauset, activity will be tracked in real-time."
    echo ""
    echo "Note: Hooks only send data when CLAUSET_SESSION_ID is set,"
    echo "so regular Claude usage is unaffected."
}

uninstall_hooks() {
    log_info "Uninstalling Clauset hooks..."

    # Remove hook script
    if [[ -f "$HOOK_SCRIPT" ]]; then
        rm "$HOOK_SCRIPT"
        log_info "Removed $HOOK_SCRIPT"
    fi

    # Remove hooks from settings
    if [[ -f "$SETTINGS_FILE" ]]; then
        log_info "Removing Clauset hooks from settings..."

        # Backup
        cp "$SETTINGS_FILE" "$SETTINGS_FILE.backup"

        # Remove hooks that point to clauset-hook.sh
        local cleaned
        cleaned=$(jq '
            .hooks |= (
                if . then
                    to_entries | map(
                        .value |= map(
                            .hooks |= map(
                                select(.command != "~/.claude/hooks/clauset-hook.sh")
                            ) |
                            select(.hooks | length > 0)
                        ) |
                        select(length > 0)
                    ) | from_entries
                else
                    .
                end
            )
        ' "$SETTINGS_FILE")

        echo "$cleaned" > "$SETTINGS_FILE"
        log_info "Removed Clauset hooks from $SETTINGS_FILE"
    fi

    log_info "Clauset hooks uninstalled successfully!"
}

# Main
check_prerequisites

if [[ "${1:-}" == "--uninstall" ]]; then
    uninstall_hooks
else
    install_hooks
fi

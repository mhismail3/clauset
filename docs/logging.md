# Logging & Observability

Clauset uses Rust's `tracing` framework with hierarchical log targets for fine-grained control.

## Quick Start

```bash
# Production mode (default) - minimal clean logs
clauset-server

# Verbose - operational detail
clauset-server -v

# Debug - troubleshooting info
clauset-server -d

# Trace - everything including high-frequency events
clauset-server --trace

# Quiet - warnings/errors only
clauset-server -q

# JSON output for log aggregation
clauset-server --log-format json

# Category-specific debugging
clauset-server --log activity::state=debug
clauset-server --log ws=trace,session=debug
```

## CLI Flags

| Flag | Description |
|------|-------------|
| `-v`, `--verbose` | INFO + operational detail |
| `-d`, `--debug` | DEBUG level for all targets |
| `--trace` | TRACE level (high-frequency logs) |
| `-q`, `--quiet` | WARN/ERROR only |
| `--log <TARGET=LEVEL>` | Override specific target (repeatable) |
| `--log-format <text\|json>` | Output format (default: text) |

**Priority**: CLI flags > RUST_LOG env > default preset

## Log Targets

| Target | Description | Default Level |
|--------|-------------|---------------|
| `clauset::startup` | Server initialization | INFO |
| `clauset::api` | HTTP API requests | INFO |
| `clauset::ws` | WebSocket lifecycle | INFO |
| `clauset::ws::ping` | WebSocket keepalive | TRACE |
| `clauset::session` | Session lifecycle | INFO |
| `clauset::process` | Claude CLI process | INFO |
| `clauset::activity` | Activity state machine | WARN |
| `clauset::activity::state` | Detailed state tracking | DEBUG |
| `clauset::activity::stats` | Stats parsing | DEBUG |
| `clauset::parser` | Output parsing | DEBUG |
| `clauset::events` | Event processor | INFO |
| `clauset::hooks` | Hook processing | INFO |

## Presets

### Production (default)
Key lifecycle events only. Suitable for normal operation.

### Verbose (`-v`)
All INFO logs plus operational detail. Use when monitoring active sessions.

### Debug (`-d`)
Full DEBUG output excluding high-frequency traces. Use for troubleshooting.

### Trace (`--trace`)
Everything including per-chunk terminal logs. Use for deep investigation.

### Quiet (`-q`)
Only WARN and ERROR. Use in production with external log aggregation.

## Common Scenarios

**Debug activity state flickering:**
```bash
clauset-server --log activity::state=debug
```

**Debug WebSocket issues:**
```bash
clauset-server --log ws=debug
# Or with keepalive details:
clauset-server --log ws=trace
```

**Debug session lifecycle:**
```bash
clauset-server --log session=debug,process=debug
```

**Debug hook updates:**
```bash
clauset-server --log hooks=debug
```

**Production with JSON logging:**
```bash
clauset-server --log-format json
```

## Environment Variable

For advanced use, `RUST_LOG` is still supported:

```bash
RUST_LOG="clauset::activity::state=debug,clauset::ws=trace" clauset-server
```

CLI flags take precedence over `RUST_LOG`.

## Log Levels

| Level | Use Case |
|-------|----------|
| ERROR | Failures requiring attention |
| WARN | Unexpected but handled conditions |
| INFO | Key lifecycle events |
| DEBUG | Detailed operational data |
| TRACE | High-frequency events (per-chunk) |

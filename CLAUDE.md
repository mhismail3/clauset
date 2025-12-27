# Clauset Project Rules

## Test-Driven Development (REQUIRED)

All changes MUST follow test-driven development practices:

1. **Before modifying code**: Understand existing test coverage for the affected area
2. **When adding features**: Write tests first, then implement the feature
3. **When fixing bugs**: Write a failing test that reproduces the bug, then fix it
4. **When removing functionality**: Update or remove corresponding tests
5. **When refactoring**: Ensure all existing tests still pass

### Test Commands

```bash
# Backend tests (Rust)
cargo test --workspace

# Frontend tests (TypeScript/Vitest)
cd frontend && npm test

# Run specific test file
cargo test --package clauset-server --test hook_pipeline
cd frontend && npm test -- src/stores/__tests__/messages.test.ts
```

### Test Locations

- **Backend unit tests**: `crates/*/src/**/*.rs` (inline `#[cfg(test)]` modules)
- **Backend integration tests**: `crates/clauset-server/tests/`
- **Frontend unit tests**: `frontend/src/**/__tests__/*.test.ts`
- **Test fixtures**: `tests/fixtures/hook_events/*.json`

### Minimum Requirements

- All new public functions must have at least one test
- All bug fixes must include a regression test
- Integration tests required for cross-module functionality
- Property-based tests (proptest) for data structures with invariants

## Forbidden Commands

NEVER run any production deployment or restart commands. This includes but is not limited to:

- `clauset restart`
- `clauset deploy`
- `clauset stop`
- `clauset start`
- Any command that affects the production deployment

If deployment or restart is needed, inform the user and let them run these commands manually.

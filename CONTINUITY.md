# Continuity Ledger

## Goal
Implement comprehensive interaction tracking system with:
- Interaction timeline (user prompts + tool invocations)
- File diffs (before/after snapshots)
- Cross-session full-text search
- Cost analytics per interaction
- Rollback capability

Success criteria: All features from docs/FEATURE_PLAN.md implemented with no regressions.

## Constraints/Assumptions
- 30-day retention policy
- Modified files only (Write/Edit tools)
- Hook integration already exists (HookEventPayload)
- Use same SQLite database file, separate tables
- Content-addressed storage with SHA256 + zstd compression

## Key Decisions
- Created separate `InteractionStore` module (not extending SessionStore)
- FTS5 for full-text search on user_prompt + assistant_summary + tool_name
- Reference counting triggers for file content deduplication
- Foreign key from interactions → sessions (requires sessions table to exist)
- Using `similar` crate for diff computation

## State

### Done
- Phase 1: Database Schema & Core Infrastructure
  - Created `clauset_types::interaction` module with all type definitions
  - Created `clauset_core::interaction_store` module with schema, CRUD, cleanup
  - FTS5 virtual tables and triggers for full-text search
  - Reference counting for content deduplication
  - All tests passing

- Phase 2: Interaction Capture Engine
  - Created `InteractionProcessor` in clauset-server
  - Integrated with hook processing via hooks.rs
  - Captures interactions on UserPromptSubmit
  - Captures tool invocations on PreToolUse/PostToolUse
  - Captures before/after file snapshots for Write/Edit tools
  - Completes interactions on Stop event

- Phase 3: Diff Engine
  - Created `diff.rs` module with `compute_diff()` and `generate_unified_diff()`
  - Added `FileChangeWithDiff` type
  - Added `get_file_changes_with_diffs()` and `get_unified_diff()` methods
  - 6 new diff tests passing

- Phase 4: Cross-Session Search
  - Added FTS5 search methods: `search_interactions()`, `search_tool_invocations()`
  - Added file path pattern search: `search_files_by_path()`
  - Added combined `global_search()` method
  - Types: `SearchField`, `SearchResult`, `FilePathMatch`, `GlobalSearchResults`

- Phase 5: Cost Analytics
  - Added `get_session_analytics()` for per-session stats
  - Added `get_daily_cost_breakdown()` for cost over time
  - Added `get_tool_cost_breakdown()` for per-tool analysis
  - Added `get_analytics_summary()` for aggregate stats
  - Added `get_most_expensive_interactions()` for outlier detection
  - Types: `SessionAnalytics`, `DailyCostEntry`, `ToolCostEntry`, `AnalyticsSummary`

- Phase 6: API Endpoints
  - Created `routes/interactions.rs` with all endpoints:
    - GET /api/sessions/{id}/interactions - list session interactions
    - GET /api/interactions/{id} - get interaction detail
    - GET /api/sessions/{id}/files-changed - list changed files
    - GET /api/diff - compute diff between snapshots
    - GET /api/search - cross-session search
    - GET /api/analytics - cost analytics summary
    - GET /api/analytics/expensive - most expensive interactions
    - GET /api/analytics/storage - storage statistics
  - Added `get_snapshot_content()` and `get_all_session_ids()` to store
  - All 43 tests passing, workspace compiles

- Phase 7: Timeline/Interaction UI Components
  - Created `InteractionCard.tsx` - displays individual interactions with expandable details
  - Created `TimelineView.tsx` - lists interactions in chronological order with session stats
  - Added to Session page as "history" tab

- Phase 8: Diff Viewer Component
  - Created `DiffViewer.tsx` - unified diff display with syntax highlighting
  - Shows line-by-line changes with added/removed/context coloring
  - Includes stats bar and hunk headers

- Phase 9: Search UI
  - Created `SearchModal.tsx` - cross-session full-text search
  - Supports scoped search (prompts, files, tools, all)
  - Displays results grouped by type with links to interactions

- Phase 10: Analytics Dashboard
  - Created `Analytics.tsx` page with summary stats, charts, and storage info
  - Daily cost chart (bar graph)
  - Tool usage breakdown
  - Sessions by cost list
  - Storage statistics with compression ratio

- Integration
  - Added `/analytics` route to router
  - Added search button and analytics link to Sessions page header
  - Added "history" tab to Session page view toggle
  - All frontend builds successfully, all backend tests pass

- Create New Project in New Session Modal
  - Added POST /api/projects endpoint with validation in `projects.rs`
  - Registered route in `main.rs`
  - Added `api.projects.create()` method in `api.ts`
  - Replaced `<select>` with combobox in `NewSessionModal.tsx`
  - Shows "Will create new project: {name}" indicator when typing new name
  - Frontend UI tested and working

- New Session Modal Styling Revamp
  - Custom combobox for both project and model dropdowns (replaced browser default)
  - Project field starts empty (no auto-population)
  - Retro card styling (border + offset shadow) matching site design
  - Backdrop blur effect
  - Escape key support for closing modal
  - PWA safe area handling for mobile
  - Monospace font (JetBrains Mono) for title and inputs matching session cards

### Now
- Search functionality fully working

### Next
- Test on iOS device to verify keyboard handling works correctly

## Bug Fixes Applied
- **Views stacking on Session page**: Changed from CSS `hidden` class to Solid.js `<Show when={}>` for Chat/History views (inline styles override CSS classes)
- **Analytics page not scrolling**: Changed from `min-height: 100vh` to `height: 100%` with `overflow-y: auto`
- **Search button not tappable**: Fixed Solid.js anti-pattern - replaced conditional return (`if (!props.isOpen) return null`) with proper `<Show when={}>` wrapper in SearchModal.tsx
- **FTS5 syntax error with special characters**: Added `escape_fts5_query()` helper that wraps queries in double quotes for phrase search
- **FTS5 search returning no results**: Fixed incorrect join condition - changed `i.id = fts.rowid` to `i.rowid = fts.rowid` (id is UUID, rowid is integer)
- **Cost/token data not captured**: Added `complete_interaction_with_costs()` to `InteractionStore`, modified `InteractionProcessor` to track starting costs and compute deltas when interactions complete, updated hooks route to pass session costs to the processor
- **Cost capture timing (late terminal output)**: Added `update_latest_interaction_costs()` method that's called from event_processor when terminal output with cost changes arrives after Stop hook
- **Session header decluttered**: Replaced ACTIVE badge with colored status dot (green=ready, orange=thinking, gray=stopped)
- **Inflated output token counts on dashboard**: Fixed regex patterns in `buffer.rs` to require K suffix (was `K?` which matched false positives like "804/993 files"). Added sanity validation (< 1000K) and removed `Math.max()` from frontend token updates in `sessions.ts`. Added one-time DB migration to reset any sessions with > 1M tokens. Files changed: `crates/clauset-core/src/buffer.rs`, `frontend/src/stores/sessions.ts`, `crates/clauset-core/src/db.rs`

## Open Questions
- None currently

## Working Set
- `crates/clauset-types/src/interaction.rs` - type definitions
- `crates/clauset-core/src/interaction_store.rs` - database layer
- `crates/clauset-core/src/diff.rs` - diff computation
- `crates/clauset-server/src/interaction_processor.rs` - hook integration
- `crates/clauset-server/src/routes/interactions.rs` - API endpoints
- `docs/FEATURE_PLAN.md` - comprehensive implementation plan

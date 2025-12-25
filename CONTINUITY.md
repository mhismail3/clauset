# Continuity Ledger

## Goal
Implement chat mode that displays Claude's messages as chat bubbles while maintaining 1-to-1 mapping with terminal sessions.

Success criteria:
- User messages display as chat bubbles (from UserPromptSubmit hook)
- Claude's responses display as chat bubbles (parsed from terminal output)
- Tool calls display inline (collapsible, from PreToolUse/PostToolUse hooks)
- Real-time streaming as Claude types
- Toggle between term/chat views on same session
- No regressions to terminal mode
- NO use of `claude -p` or API/SDK - always terminal sessions

## Constraints/Assumptions
- Terminal PTY sessions remain source of truth (no API/SDK)
- Existing hooks provide: UserPromptSubmit, PreToolUse, PostToolUse, Stop
- Chat mode is a view layer over terminal data
- Hybrid extraction: real-time terminal parsing + transcript verification
- Collapsible tool calls in chat UI

## Key Decisions
- New `ChatMessageProcessor` in clauset-core for message extraction
- New `chat.rs` types module for ChatMessage, ChatToolCall, MessageRole
- State machine tracks: Idle → BuildingAssistant → ToolInProgress → Idle
- WebSocket broadcasts ChatMessage events (separate from terminal output)
- Transcript file used for verification on Stop hook

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

- Chat Persistence, UI Fixes, and Cleanup (5 issues)
  - Issue 1: Chat history persistence - database tables + localStorage cache
  - Issue 2: Chat view as default mode, tabs renamed to "chat | terminal | history"
  - Issue 3: Removed Chat Mode toggle from New Session modal
  - Issue 4: Fixed keyboard overlap in New Session modal (useKeyboard hook)
  - Issue 5: Multi-line chat textbox expansion (max 10 lines, then scroll)

- Chat UI/UX Overhaul (matching dashboard retro theme)
  - MessageBubble.tsx: Retro offset shadows, borders on user/assistant bubbles
  - ToolCallView: Colored left accent bar by tool type, chevron expand icon, card styling
  - MarkdownContent: Full markdown support (bold, italic, headers, lists, links, code blocks)
  - InputBar.tsx: Glass container, retro-styled textarea with focus glow, icon-only send button
  - Session.tsx: Empty state card with retro styling and chat icon
  - Typography: Added Source Serif 4 (serif font) for chat prose, keeping monospace for code
  - User bubbles: Muted darker orange (#9a4a2e) with visible border (#7a3a22)
  - iOS keyboard: Fixed container push-up with visualViewport.offsetTop tracking

### Now
- Initial prompt fix completed

### Initial Prompt Fix (Just Completed)
- **Problem**: Initial prompt in "Create Session" modal shows in terminal but doesn't execute (Enter key not triggering submit)
- **Root cause**: Initial prompt used different code path than chat messages - wrote directly to PTY in spawn_terminal() reader thread
- **Fix**: Route initial prompt through same code path as chat messages (send_input())
- Files changed:
  - `crates/clauset-core/src/process.rs` - Removed prompt field from SpawnOptions, removed direct PTY write in spawn_terminal()
  - `crates/clauset-core/src/session.rs` - Added send_input() call after session starts with 1s delay for Claude TUI to be ready

### Previous
- Terminal width fix for chat-default mode completed

### Terminal Width Fix for Chat-Default Mode (Comprehensive Fix)
- **Problem**: When chat mode is the default tab, terminal displays at ~20-40 cols instead of full width
- **Root cause**: Multiple issues:
  1. `fitAddon.proposeDimensions()` returns minimum 20x5 for hidden containers
  2. These wrong dimensions were sent to server immediately on WebSocket connect
  3. PTY created with wrong dimensions, Claude Code renders welcome at that width
- **Fix**: Three-part solution:
  1. `terminalSizing.ts` - Don't trust fitAddon if container is hidden (clientWidth=0)
  2. `TerminalView.tsx` - Don't send dimensions to server if container is hidden
  3. `ws.ts` - Defer initial sync request until dimensions are known (non-zero)
- Files changed:
  - `frontend/src/lib/terminalSizing.ts` - Check container visibility before using fitAddon
  - `frontend/src/components/terminal/TerminalView.tsx` - Check visibility before onResize, added isVisible prop/effect
  - `frontend/src/lib/ws.ts` - Defer initial sync until dimensions set, default dims to 0x0
  - `frontend/src/pages/Session.tsx` - Pass isVisible prop

### Previous
- PWA viewport overscroll fix implemented

### PWA Viewport Overscroll Fix
- Created `frontend/src/lib/preventOverscroll.ts` - JavaScript hook to prevent iOS PWA rubber-banding
- Added `usePreventOverscroll()` call in `App.tsx`
- Intercepts `touchmove` events and prevents default when no legitimate scrollable container exists
- Handles scroll boundary detection (at top/bottom) to prevent escape from scrollable areas
- Does NOT modify CSS - preserves all safe-area styling that prevents status bar at bottom

### Previous
- Production/Beta deployment system implemented

### Production/Beta Deployment System (Just Completed)
- Added `--config` and `--port` CLI flags to clauset-server
- Created `config/production.toml` (port 8080) and `config/beta.toml` (port 8081, separate DB)
- Created `scripts/clauset` management CLI with: status, start, stop, restart, logs, beta, deploy, install
- Updated `frontend/vite.config.ts` to support `CLAUSET_BACKEND_PORT` env var for beta proxying
- Workflow: `clauset beta` runs isolated test server, `clauset deploy` promotes to production via launchd

### Verified Working
- User messages display as chat bubbles (from UserPromptSubmit hook)
- Claude's responses display as chat bubbles (from transcript JSONL on Stop hook)
- "Thinking..." indicator shows while Claude is processing
- Streaming content deltas work correctly
- Toggle between term/chat views on same session

### Bug Fixes Applied This Session
- **Terminal parsing disabled**: Removed `process_terminal_output()` call from event_processor - terminal output is too noisy (spinners, ANSI codes, status lines) to reliably extract Claude's prose text.
- **Transcript-based response extraction**: Added `transcript_path` to `HookEvent::Stop` variant. On Stop hook, reads Claude's response from the transcript JSONL file and adds it to the chat message before marking complete.
- **Fixed transcript JSONL parsing**: Transcript format is nested `{"type":"assistant", "message":{"content":[{"type":"text", "text":"..."}]}}`. Updated `read_last_assistant_response()` to check outer `type` field and navigate to nested `message.content` array.
- **Fixed WebSocket ping/pong mismatch**: Frontend sends JSON `{type: 'ping'}` messages but server only handled protocol-level WebSocket pings. Added JSON ping handling in `global_ws.rs` - recv_task parses `{type:'ping'}` and sends `{type:'pong'}` response via channel to send_task.
- **Fixed session WebSocket ping/pong**: Same issue as global WebSocket - added Pong response in `websocket.rs` by sending `WsServerMessage::Pong { timestamp }` via outgoing_tx channel.

### Previous Bug Fixes
- **Chat Enter key not executing in terminal**: Updated `send_input()` in process.rs to match the initial prompt pattern (lines 378-386). The fix sends text and `\r` SEPARATELY with a 50ms delay and flush between them. Claude Code's TUI needs the Enter key to arrive as a distinct input event, not concatenated with the text.
- **Chat events not appearing in frontend**: Fixed serde serialization conflict. Both `WsServerMessage` and `ChatEvent` used `#[serde(tag = "type")]`, causing the two `type` fields to conflict. Changed `WsServerMessage::ChatEvent(ChatEvent)` (tuple variant) to `WsServerMessage::ChatEvent { event: ChatEvent }` (struct variant). Updated frontend to access `msg.event` instead of `msg.ChatEvent`.
- **Chat message parsing error (tool_calls undefined)**: Removed `skip_serializing_if = "Vec::is_empty"` from `tool_calls` field in `ChatMessage` struct. The frontend expected `tool_calls` to always be an array, but when empty, serde omitted the field entirely causing `undefined.map()` error in JavaScript.
- **Duplicate user messages in chat**: Removed local `addMessage()` call in frontend's `handleSendMessage()`. Messages now only come from the UserPromptSubmit hook, ensuring a single source of truth.
- **No thinking indicator in chat**: Added thinking state detection to MessageBubble component. When an assistant message has `isStreaming: true` but no content, shows animated "Thinking..." indicator.
- **No streaming response in chat**: Wired up `chat_processor.process_terminal_output()` in event_processor.rs to extract Claude's prose from terminal output and broadcast ContentDelta events to chat view.

### Done (Chat Mode Implementation - Steps 1-5)
- Created `crates/clauset-types/src/chat.rs` with ChatMessage, ChatToolCall, ChatRole types
- Created `crates/clauset-core/src/chat_processor.rs` with state machine for message extraction
- Added `ProcessEvent::Chat` variant to process.rs
- Added `broadcast_event()` method to SessionManager
- Added ChatProcessor to AppState and integrated with hooks.rs
- Added `WsServerMessage::ChatEvent` variant to ws.rs
- Updated websocket.rs and global_ws.rs to forward Chat events
- Updated event_processor.rs to handle Chat events
- Added `handleChatEvent()` function to frontend messages.ts store
- Updated Session.tsx to handle chat_event WebSocket messages
- Replaced "Terminal mode active" notice with "No messages yet" empty state

### Next
- Tool calls display inline (collapsible, from PreToolUse/PostToolUse hooks)
- Real-time streaming as Claude types (currently batch on Stop hook)
- Clean up debug logging in ws.ts

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
- **FTS5 prefix/partial matching**: Implemented real-time "search as you type" with prefix matching. Updated `escape_fts5_query()` to add `*` wildcard suffix. Added `prefix='2 3'` indexes to FTS5 tables. Added migration to drop/recreate FTS tables. Reduced frontend debounce to 150ms and min query length to 1 char.

## Open Questions
- None currently

## Working Set
- `crates/clauset-types/src/chat.rs` - chat message types (new)
- `crates/clauset-core/src/chat_processor.rs` - message extraction (new)
- `crates/clauset-server/src/routes/hooks.rs` - forward events to ChatProcessor
- `crates/clauset-server/src/ws/session.rs` - broadcast chat messages
- `frontend/src/stores/messages.ts` - frontend message handlers
- `frontend/src/pages/Session.tsx` - wire up handlers, remove notice
- Plan file: `~/.claude/plans/fuzzy-forging-engelbart.md`

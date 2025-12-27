# Continuity Ledger

## Goal
Session persistence across backend restarts using ~/.claude as source of truth - **COMPLETED**

Success criteria:
- Sessions survive backend restarts/redeploys
- Resume uses Claude's real session ID
- Import terminal sessions into Clauset
- Terminal history visible after resume
- Chat history visible after resume
- Clear error when session cannot be resumed

## Constraints/Assumptions
- Claude stores session data in ~/.claude/history.jsonl and ~/.claude/projects/
- Clauset must use Claude's real session ID for resume to work
- Terminal buffer must be persisted for seamless resume experience
- Chat history already persists to SQLite (only terminal buffer was missing)

## Key Decisions
- Use Uuid::nil() as sentinel for "no session ID captured yet"
- Save terminal buffer on session stop (not periodically, to reduce I/O)
- Parse ~/.claude/history.jsonl to list sessions for import
- Use resume_session_id option in create_session for imported sessions

## State

### Done (Session Persistence - December 2024)
- Phase 1: Capture Claude's real session ID from System init event
  - Added update_claude_session_id() in db.rs
  - Added set_claude_session_id() in session.rs
  - Capture session ID in websocket.rs on System init event
  - Use Uuid::nil() initially in session creation
- Phase 2: Read sessions from ~/.claude
  - Created claude_sessions.rs with ClaudeSessionReader
  - Added /api/claude-sessions endpoint
  - Added /api/sessions/import endpoint
- Phase 3: Validate & Handle Resume Errors
  - Added SessionNotResumable error type
  - Validate claude_session_id before resume
- Phase 4: Terminal buffer persistence
  - Added terminal_buffers table
  - Save buffer on session stop (persist_session_activity)
  - Load buffer on resume
- Phase 5: Frontend Import UI
  - Added import tab in NewSessionModal
  - List Claude sessions from ~/.claude
  - Import sessions with one click
- Phase 5: Better resume error handling
  - Specific error messages for non-resumable sessions
  - Suggest starting new session when resume fails

### Previous Implementation Work
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
- Native TUI Menu Support - **COMPLETE**

### Native TUI Menu Support (Just Completed)
- **Goal**: Render Claude Code's built-in TUI menus (/model, /config, etc.) natively in chat mode
- **Problem**: Built-in commands bypass hook system - menus render only in terminal as ANSI escape sequences
- **Solution**: Parse terminal output to detect menu patterns, render as native UI overlay
- **Plan file**: `~/.claude/plans/robust-humming-meadow.md`

#### Implementation:
1. **Phase 1: Types Module** (`crates/clauset-types/src/tui_menu.rs`)
   - TuiMenuOption, TuiMenuType, TuiMenu structs
   - TuiMenuEvent enum (MenuPresented, MenuDismissed)
   - 24 comprehensive tests

2. **Phase 2: Menu Parser** (`crates/clauset-core/src/tui_menu_parser.rs`)
   - State machine with 3 states: Idle, Accumulating, MenuActive
   - Generic detection: numbered options, footer patterns ("Enter to confirm")
   - ANSI code stripping for clean parsing
   - 26 unit tests

3. **Phase 3: Backend Integration**
   - `ws.rs`: Added TuiMenuSelect/TuiMenuCancel client messages, TuiMenu server message
   - `process.rs`: Added ProcessEvent::TuiMenu variant
   - `buffer.rs`: Integrated TuiMenuParser, returns detected menus from append()
   - `event_processor.rs`: Broadcasts MenuPresented events when menus detected
   - `websocket.rs`: Handles TuiMenuSelect (sends arrow keys + Enter), TuiMenuCancel (sends Escape)

4. **Phase 4: Frontend**
   - `stores/tui_menu.ts`: State management (getTuiMenuState, handleTuiMenuEvent, clearTuiMenuState)
   - `components/tui_menu/TuiMenuOverlay.tsx`: Modal overlay with keyboard navigation
   - `components/tui_menu/TuiMenuOverlay.css`: Retro styling matching theme
   - `pages/Session.tsx`: WebSocket handler, overlay integration, disabled input during menu

5. **Phase 5: Tests**
   - Backend: 143 tests in clauset-types, 26 tests in tui_menu_parser
   - Frontend: 176 tests total (includes new tui_menu.test.ts)

#### Files Created:
- `crates/clauset-types/src/tui_menu.rs`
- `crates/clauset-core/src/tui_menu_parser.rs`
- `frontend/src/stores/tui_menu.ts`
- `frontend/src/stores/__tests__/tui_menu.test.ts`
- `frontend/src/components/tui_menu/TuiMenuOverlay.tsx`
- `frontend/src/components/tui_menu/TuiMenuOverlay.css`

#### Files Modified:
- `crates/clauset-types/src/lib.rs` - export tui_menu
- `crates/clauset-types/src/ws.rs` - TuiMenu messages
- `crates/clauset-core/src/lib.rs` - export TuiMenuParser
- `crates/clauset-core/src/process.rs` - ProcessEvent::TuiMenu
- `crates/clauset-core/src/buffer.rs` - Parser integration
- `crates/clauset-core/src/session.rs` - Updated append_terminal_output return type
- `crates/clauset-server/src/event_processor.rs` - Broadcast menu events
- `crates/clauset-server/src/websocket.rs` - Handle select/cancel messages
- `frontend/src/pages/Session.tsx` - TuiMenuOverlay integration

### Previous
- Chat Mode Refinements - **COMPLETE**

### Chat Mode Refinements (Just Completed)
Following user feedback, addressed several UX issues:

1. **Settings Menu Z-Index Fix**
   - Header z-index: 50, dropdown z-index: 1000
   - Fixed overlap with content below

2. **Removed SubagentPanel**
   - Panel only showed briefly during agent execution (not useful)
   - Replaced with detailed SubagentCompleted message bubbles

3. **Enhanced Subagent Messages**
   - Added `SubagentCompleted` event with detailed output (agent_type, description, result)
   - Backend: Added to ws.rs, process.rs, hooks.rs (detects Task tool PostToolUse)
   - Frontend: Renders as assistant message with green border, header showing agent type & description, result in italics
   - Files: hooks.rs, websocket.rs, event_processor.rs, messages.ts, MessageBubble.tsx

4. **QuickActionsMenu Component**
   - Lightning bolt button left of input field
   - Opens menu with: /clear, /compact, /cost, /model, /plan, /help
   - Each item shows icon, label, description, command
   - File: `frontend/src/components/chat/QuickActionsMenu.tsx`

5. **Slash Commands as User Message Bubbles**
   - All user input now appears immediately in chat (including slash commands)
   - Added `addUserMessage()` function to messages.ts
   - Added deduplication in `handleChatEvent()` to prevent duplicates from UserPromptSubmit hook
   - Files: messages.ts, Session.tsx

### Non-Critical Chat Mode Features (Previously Completed)
All non-critical features from the plan have been implemented:

1. **StatusBar Component** (`frontend/src/components/session/StatusBar.tsx`)
   - Displays: Model | $Cost | InputK/OutputK | Cache: Xr/Yw | Context %
   - Color-coded context percentage (green < 60%, yellow 60-80%, red > 80%)
   - Mini progress bar for visual context usage

2. **TodoWidget Component** (`frontend/src/components/session/TodoWidget.tsx`)
   - Extracts todos from most recent TodoWrite tool call in messages
   - Shows pending/in_progress/completed counts in header
   - Status icons: ○ pending, ◐ in_progress, ✓ completed
   - Expandable/collapsible panel

3. **SubagentPanel Component** (`frontend/src/components/session/SubagentPanel.tsx`)
   - Tracks active subagents from subagent_started/subagent_stopped events
   - Shows agent type and elapsed time
   - Animated pulse indicator for active agents
   - Expandable/collapsible panel

4. **KeyboardShortcutsModal** (`frontend/src/components/session/KeyboardShortcutsModal.tsx`)
   - Shows keyboard shortcuts grouped by category (Input, Navigation, Views, Terminal Mode)
   - Opened from Settings menu or "?" button
   - Escape key to close

5. **SessionSettingsMenu** (`frontend/src/components/session/SessionSettingsMenu.tsx`)
   - Gear icon in session header
   - Shows current model and mode
   - Quick access to keyboard shortcuts
   - Tip about using "/" for commands

6. **AllowedToolsPanel** (`frontend/src/components/session/AllowedToolsPanel.tsx`)
   - UI ready for allowed tools display (backend data flow pending)
   - Categorizes tools: Read, Write, Execute, Web, Other
   - Color-coded by category
   - Expandable/collapsible panel

**Session.tsx Integration:**
- Added imports for all 6 components
- Added state: `activeSubagents`, `hookData` (cache tokens, allowed tools), `showShortcuts`
- Added `todos` memo extracting from messages
- StatusBar, SubagentPanel, TodoWidget, AllowedToolsPanel in chat view
- KeyboardShortcutsModal at component root
- SessionSettingsMenu in header

**Note:** AllowedToolsPanel shows when `hookData().allowedTools` is populated. Currently no backend sends this data - component is ready for future enhancement.

### Chat Mode Feature Parity (Previous)
- **Goal**: Add missing affordances to bring chat mode to parity with CLI (permissions, interrupt, mode indicator)
- **Plan file**: `~/.claude/plans/synthetic-strolling-fountain.md`

#### Implemented Features:
1. **Permission Approval UI** - Inline Allow/Deny/Allow All buttons on permission_request messages
   - `MessageBubble.tsx` - Added permission buttons with green/red/blue styling
   - `messages.ts` - Added `responded` field and `markPermissionResponded()` function
   - `Session.tsx` - Added `handlePermissionResponse()` that sends WebSocket message

2. **Stop/Interrupt Button** - Replace send button with red stop square when Claude is processing
   - `InputBar.tsx` - Added `isProcessing` and `onInterrupt` props
   - Shows stop button (■) when processing and no text entered
   - `Session.tsx` - Added `isClaudeProcessing()` check and `handleInterrupt()` handler

3. **Plan Mode Indicator** - Purple "Plan" badge in session header when in plan mode
   - `Session.tsx` - Added `mode` signal, handles `mode_change` WebSocket message
   - Badge appears next to project name when mode is 'plan'

#### Backend Support (Previously Completed):
- `crates/clauset-types/src/ws.rs` - Added `PermissionResponse`, `Interrupt` client messages, `ModeChange` server message
- `crates/clauset-server/src/websocket.rs` - Handlers for PermissionResponse and Interrupt
- `crates/clauset-server/src/routes/hooks.rs` - Mode detection from EnterPlanMode/ExitPlanMode tool use
- `crates/clauset-core/src/process.rs` - Added `ProcessEvent::ModeChange`
- All 87 backend tests pass

### Test Suite Implementation - **COMPLETE**

### Test Suite Implementation Progress
- **Goal**: 290+ tests for 100% Claude Code CLI parity verification
- **Starting point**: 72 tests
- **Final count**: 447 tests (520% increase, exceeds target by 54%)
- **Plan file**: `~/.claude/plans/graceful-puzzling-russell.md`

#### Completed Phases:
1. **Phase 1: Foundation** - COMPLETE
   - Backend test dependencies (proptest, tokio-test, axum-test, tempfile)
   - Frontend test stack (vitest, @solidjs/testing-library, mock-socket)
   - vitest.config.ts with SolidJS support
   - Test setup file with localStorage, WebSocket, ResizeObserver mocks
   - 12 hook event JSON fixtures

2. **Phase 2: Backend Unit Tests** - COMPLETE (293 tests)
   - clauset-core: 139 tests (buffer, session, chat_processor, interaction)
   - clauset-types: 123 tests (ws serialization, hooks, interactive)
   - clauset-server: 27 integration tests (hook→WS pipeline)
   - Other: 4 tests

3. **Phase 3: Frontend Unit Tests** - COMPLETE (154 tests)
   - messages.test.ts: 44 tests (CRUD, streaming, tool calls, ChatEvent handling)
   - ws.test.ts: 56 tests (connection state machine, sync protocol, sequence tracking, gap recovery, ACK batching, heartbeat, dimension updates)
   - interactive.test.ts: 47 tests (prompt state, question navigation, multi-select, answer ordering)
   - example.test.ts: 7 tests (infrastructure verification)

4. **Phase 4: Integration Tests** - COMPLETE (27 tests)
   - hook_pipeline.rs: All 12 hook event types tested
   - Session lifecycle, context window extraction, tool events
   - Concurrent events, performance, error handling

#### Skipped (deferred):
- Phase 5: Playwright E2E tests
- Phase 6: GitHub Actions CI workflow

### Previous Work
- Claude Code CLI Parity Implementation - **COMPLETED**

### Claude Code CLI Parity (Completed)
- **Goal**: Full feature parity with Claude Code CLI in Clauset chat mode
- **Approach**: Reverse-engineered cli.js (v2.0.76) to understand hook system, event types, JSON structures
- **Plan file**: `~/.claude/plans/graceful-puzzling-russell.md` (612 lines of documentation)

#### All Phases Completed:
1. **Phase 1: Types** - Added ContextWindow, CurrentUsage, ModelInfo, WorkspaceInfo types to hooks.rs
2. **Phase 1: SubagentStart** - Added SubagentStart hook event type with agent_id, agent_type fields
3. **Phase 1: PostToolUseFailure** - Added hook event type with error, is_timeout, is_interrupt fields
4. **Phase 2: Token Tracking** - Now using context_window from hooks for accurate token counts (replaces fragile regex parsing)
5. **Phase 2: Event Broadcasting** - Added ProcessEvent and WsServerMessage variants for:
   - SubagentStarted/SubagentStopped (Task tool tracking)
   - ToolError (tool failure display)
   - ContextCompacting (compaction notices)
   - PermissionRequest (permission dialogs)
   - ContextUpdate (accurate cache token stats)
6. **Phase 3: WebSocket Events** - All new events forwarded to frontend via websocket.rs
7. **Phase 4: Frontend Display** - Updated frontend to display new events:
   - Added `system` role to Message interface with systemType and metadata
   - Added handler functions: handleSubagentStarted, handleSubagentStopped, handleToolError, handleContextCompacting, handlePermissionRequest
   - Updated Session.tsx to handle new WebSocket message types (including permission_request, context_update)
   - Updated MessageBubble.tsx to render system messages as centered pills with icons:
     - Tool errors: Red with ⚠ icon
     - Context compacting: Yellow with ⟳ icon
     - Subagent started: Blue with ▶ icon
     - Subagent completed: Green with ✓ icon
     - Permission request: Purple with 🔐 icon
8. **Phase 5: Regex Removal** - Added `hook_context_received` flag to buffer.rs; skip regex parsing when hook data is available
9. **Phase 6: TodoWrite Visualization** - Added TodoWrite tool rendering in ToolCallView with status icons (○ pending, ◐ in_progress, ✓ completed)
10. **Phase 7: Permission Request Display** - PermissionRequest events broadcast from hooks and display as purple system messages
11. **Phase 8: Plan Mode Indicator** - Added EnterPlanMode/ExitPlanMode tool rendering with plan mode icons
12. **Phase 9: MCP Tool Display** - Added MCP tool parsing (mcp__{server}__{tool}), purple accent color, special icons, shortened display names
13. **Phase 10: Cache Token Stats** - ContextUpdate events include cache_read_tokens and cache_creation_tokens, stored in session state

#### Key Files Modified:
- `crates/clauset-types/src/hooks.rs` - Added ContextWindow, new event types
- `crates/clauset-types/src/ws.rs` - Added new WsServerMessage variants
- `crates/clauset-core/src/process.rs` - Added new ProcessEvent variants
- `crates/clauset-core/src/buffer.rs` - Added `update_context_from_hook()`
- `crates/clauset-core/src/session.rs` - Added `update_context_from_hook()`
- `crates/clauset-server/src/routes/hooks.rs` - Use context_window, broadcast new events
- `crates/clauset-server/src/websocket.rs` - Forward new events to frontend
- `crates/clauset-server/src/event_processor.rs` - Handle new event variants

### Previous Work
- Interactive slash command support - multi-question carousel IMPLEMENTED

### Interactive Slash Command Support with Multi-Question Carousel (Just Implemented)
- **Goal**: Add native UI for interactive slash command prompts in chat mode with swipeable card carousel
- **Problem**: Slash commands like /model, /config use AskUserQuestion tool which renders as raw ANSI in terminal
- **Solution**: Intercept AskUserQuestion at PreToolUse hook, batch all questions into single prompt, render as swipeable carousel
- **User Experience**:
  - All questions appear as navigation dots at top
  - Swipe left/right or use arrow keys to navigate between questions
  - Answer each question (single-select auto-advances, multi-select has "Next" button)
  - Green checkmarks on dots show which questions are answered
  - "Send All Responses" button appears when all questions answered
  - Answers sent to terminal one by one with 100ms delay
- **Implementation**:
  1. `crates/clauset-types/src/interactive.rs` - InteractivePrompt batches multiple InteractiveQuestion
  2. `hooks.rs` - Batches all questions into single PromptPresented event (not one per question)
  3. `websocket.rs` - Uses PromptPresented variant
  4. `frontend/src/stores/interactive.ts` - PromptSession with per-question answers Map
  5. `frontend/src/components/interactive/InteractiveCarousel.tsx` - Swipeable carousel with dots, arrows, "Send All"
  6. `frontend/src/pages/Session.tsx` - Uses InteractiveCarousel, sends answers sequentially
  7. `frontend/src/index.css` - Carousel styling (dots, arrows, submit-all button)
- **Files created**:
  - `crates/clauset-types/src/interactive.rs`
  - `frontend/src/stores/interactive.ts`
  - `frontend/src/components/interactive/QuestionCard.tsx` (legacy, kept for reference)
  - `frontend/src/components/interactive/InteractiveCarousel.tsx` (new carousel)
- **Files modified**:
  - `crates/clauset-types/src/lib.rs` - export interactive module
  - `crates/clauset-types/src/ws.rs` - message types
  - `crates/clauset-core/src/process.rs` - ProcessEvent::Interactive
  - `crates/clauset-server/src/routes/hooks.rs` - batch questions into single PromptPresented
  - `crates/clauset-server/src/websocket.rs` - PromptPresented handling
  - `crates/clauset-server/src/event_processor.rs` - handle Interactive event
  - `frontend/src/pages/Session.tsx` - InteractiveCarousel integration
  - `frontend/src/index.css` - carousel styling
- **Next**: Test with slash commands that ask multiple questions

### Terminal Command Picker Flicker Fix (Completed)
- **Problem**: Terminal flickered showing welcome box when scrolling down in slash command picker (typing "/" then pressing down arrow until menu scrolls)
- **Root cause**: `scrollIntoView({ behavior: 'smooth' })` in CommandPicker.tsx caused ~300ms of continuous animation frames. The CommandPicker (fixed position, z-index 100) sits over the terminal, and the continuous repaints during smooth scroll interfered with xterm's compositor layer, causing its viewport to momentarily scroll to top of scrollback buffer.
- **Key insight**: The `initialSyncDone` flag was working correctly - this was NOT about sync requests. It was a rendering/compositor interference issue.
- **Fix applied**:
  1. Removed `behavior: 'smooth'` from `scrollIntoView({ block: 'nearest' })` - instant scrolling eliminates animation frame interference
  2. Added `contain: 'layout paint'` CSS property to CommandPicker container - isolates its layout/paint from affecting terminal
- **File changed**: `frontend/src/components/commands/CommandPicker.tsx`

### Terminal Flicker Fix - Disable Post-Initial Resyncs (Just Completed)
- **Problem**: Terminal still flickered when using Claude Code's command picker - it would show correctly for a moment then flash back to showing the welcome box
- **Root cause**: Multiple code paths were calling `doFitAndResize()` which triggered `setTerminalDimensions()` → `sendSyncRequest()`. The sync request told the server to send the ENTIRE terminal buffer, causing xterm to redraw from the beginning (showing welcome box instead of current state)
- **Solution**: "Fire and forget" approach - after initial sync, STOP sending dimension updates
  - Added `initialSyncDone` flag in ws.ts
  - After first sync response, set flag to true
  - `setTerminalDimensions()` returns early if flag is set (no more sync requests)
  - Reset flag on connection close (so reconnects work)
- **Files changed**:
  - `frontend/src/lib/ws.ts`:
    - Added `initialSyncDone` flag
    - Modified `handleSyncResponse()` to set flag after first sync
    - Modified `setTerminalDimensions()` to skip sync requests after initial
    - Reset flag in `ws.onclose` handler

### Previous Terminal Flicker Fix (Same Session)
- Added dimension-based filtering to `handleResize()` in TerminalView.tsx
- Only process resize events when container width/height actually changed
- Prevents ResizeObserver feedback loop

### Terminal Flicker Fix - Comprehensive (Just Completed)
- **Problem**: Terminal still flickered when interacting with Claude Code's autocomplete
- **Root causes identified**:
  1. **Double sync requests**: `handleTerminalResize()` called both `setTerminalDimensions()` and `requestResync()` - double the server updates!
  2. **No debouncing of dimension updates**: Rapid dimension changes (keyboard animation) each triggered immediate PTY resize
  3. **Keyboard callbacks bypassed debounce**: `onShow`/`onHide` called `doFitAndResize()` directly instead of debounced `handleResize()`
- **Fixes applied**:
  1. **Session.tsx**: Removed duplicate `requestResync()` call - `setTerminalDimensions()` already handles sync
  2. **ws.ts**: Added 150ms debounce to `setTerminalDimensions()` to coalesce rapid dimension changes
  3. **TerminalView.tsx**: Keyboard callbacks now use debounced `handleResize()` instead of immediate `doFitAndResize()`
- **Result**: Single debounced resize event instead of multiple rapid PTY resizes
- Files changed:
  - `frontend/src/pages/Session.tsx` - Removed duplicate resync
  - `frontend/src/lib/ws.ts` - Added dimension update debouncing (150ms)
  - `frontend/src/components/terminal/TerminalView.tsx` - Use debounced resize for keyboard callbacks

### Previous Terminal Fixes
- Removed scroll manipulation from `writeToTerminal()` to prevent fighting with TUI applications

### Previous
- Terminal dimension calculation fix complete

### Terminal Dimension Fix (Just Completed)
- **Problem**: Initial terminal dimensions too large, causing Claude Code welcome box to render incorrectly (wrapped/broken borders)
- **Root cause**: `ws.ts` `getDeviceDefaultDimensions()` didn't account for all padding layers:
  - TerminalView outer padding: 24px horizontal (12px each side)
  - terminalSizing effectiveWidth reduction: 24px (12px each side)
  - Result: Initial estimate 43 cols, actual container only 41 cols → 2 column mismatch
- **Fix**: Made initial estimates more conservative (better to start smaller and grow):
  - `horizontalPadding`: 24px → 60px (accounts for TerminalView padding + terminalSizing reduction + safety margin)
  - `uiChromeHeight`: 176px → 242px (more accurate accounting of header, toolbar, safe areas)
  - `estimatedCharHeight`: fontSize * 1.25 → fontSize * 1.3 (matches actual measured height)
  - Also uses dynamic `getRecommendedFontSize()` instead of hardcoded 14
- **Result**: Initial estimate now slightly smaller than actual (39 cols vs 41), content fits correctly
- Files changed:
  - `frontend/src/lib/ws.ts` - Updated `getDeviceDefaultDimensions()` with conservative padding estimates

### Terminal Toolbar Improvements (Just Completed)
- **Fix 1**: Toolbar not scrollable (horizontal swipe blocked)
  - Root cause: `preventOverscroll.ts` only checked `overflowY`, blocked all horizontal touch moves
  - Fix: Added horizontal scroll detection - checks `overflowX` and `scrollWidth > clientWidth`
  - Now properly allows horizontal scrolling in elements with `overflow-x: auto`
- **Fix 2**: Added "/" button to toolbar for quick command access
- **Fix 3**: Reordered toolbar buttons: /, esc, tab, ↑, ↓, ←, →, enter, ctrl (modifier last)
- **Fix 4**: Increased default terminal font sizes for better readability
  - iPhone SE/mini: 14px (was 11px)
  - Standard phones: 15px (was 12px)
  - Tablets/desktop: 16px (was 13px)
- **Fix 5**: Removed font resize buttons (A-/A+) - fixed font size only
  - Removed `adjustFontSize()` function
  - Changed `fontSize` from signal to constant
- Files changed:
  - `frontend/src/lib/preventOverscroll.ts` - Added horizontal scroll support
  - `frontend/src/lib/fonts.ts` - Increased font sizes in `getRecommendedFontSize()`
  - `frontend/src/components/terminal/TerminalView.tsx` - Updated toolbar layout

### Terminal Fixes (Previous Session)
- **Issue 1**: Terminal scrolls to top when slash commands show submenu with keyboard visible
  - Added `writeToTerminal()` wrapper that preserves scroll position during keyboard transitions
  - Tracks if we were at bottom before write, restores position after xterm's internal scroll
  - Uses `isKeyboardTransitioning` signal to coordinate with keyboard open/close
- **Issue 2**: Terminal flickering with keyboard visible
  - Added `will-change: height` during keyboard transitions for GPU optimization
  - Added `contain: layout size` to prevent layout thrashing
- **Issue 3**: Added "enter" button to toolbar
  - Added `{ label: 'enter', code: '\r' }` to SPECIAL_KEYS array
- **Issue 4**: Toolbar buttons bringing up keyboard
  - Removed `terminal?.focus()` calls from `sendSpecialKey()` and `sendCtrlKey()`
  - Added `onTouchStart` with `e.preventDefault()` to all toolbar buttons

### Slash Command Picker (Just Implemented)
- **Goal**: Implement Claude Code slash command picker in chat interface
- **Trigger**: Type "/" in chat input or use "/" button
- **Features**:
  - Discovers all commands: built-in (~40), user commands, skills, plugin commands
  - Keyboard navigation: arrow keys, Enter to select, Escape to cancel, Tab to complete
  - Search/filter as you type (e.g., "/com" shows /commit, /compact)
  - Commands with arguments insert + cursor for args
  - Commands without arguments execute immediately
  - Output streams in chat view (uses existing infrastructure)
- **Backend files created**:
  - `crates/clauset-types/src/command.rs` - Command, CommandCategory, CommandFrontmatter types
  - `crates/clauset-core/src/command_discovery.rs` - Discovery logic with 30-second cache
  - `crates/clauset-server/src/routes/commands.rs` - GET /api/commands endpoint
- **Backend files modified**:
  - `crates/clauset-types/src/lib.rs` - Export command module
  - `crates/clauset-core/src/lib.rs` - Export CommandDiscovery
  - `crates/clauset-core/Cargo.toml` - Add serde_yaml dependency
  - `Cargo.toml` - Add serde_yaml to workspace dependencies
  - `crates/clauset-server/src/state.rs` - Add CommandDiscovery to AppState
  - `crates/clauset-server/src/main.rs` - Register /api/commands route
  - `crates/clauset-server/src/routes/mod.rs` - Export commands module
- **Frontend files created**:
  - `frontend/src/stores/commands.ts` - Commands store with filtering, navigation
  - `frontend/src/components/commands/CommandPicker.tsx` - Picker UI component
- **Frontend files modified**:
  - `frontend/src/lib/api.ts` - Command types and API endpoint
  - `frontend/src/components/chat/InputBar.tsx` - "/" trigger, keyboard handling, picker integration

### Previous
- Prompt Library feature complete and committed (commit b6e6bfa)

### Prompt Library Fixes (Just Completed)
- **Issue 1**: HTTP 404 on `/api/prompts` endpoint
  - Root cause: Beta server running old binary without prompts routes
  - Fix: Rebuilt server (multi-byte UTF-8 truncation bug also fixed)
- **Issue 2**: Copy button not working in Prompt Library modal
  - Root cause: Clipboard API requires HTTPS or localhost, fails over HTTP
  - Fix: Added `execCommand` fallback for non-secure contexts in `prompts.ts`
- Files changed:
  - `crates/clauset-types/src/prompt.rs` - Fixed UTF-8 char boundary bug in truncate_preview
  - `frontend/src/stores/prompts.ts` - Added clipboard API fallback

### Prompt Library Feature (Previously Completed)
- **Goal**: Index every prompt sent to Claude Code, display in chronological library with expand/copy
- **UI**: FAB menu in bottom-right corner with "Prompt Library" and "New Session" options
- **Implementation**:
  1. Backend: Added `prompts` table with content hash deduplication
  2. Backend: Added `PromptIndexer` for backfill from `~/.claude/` transcripts on first run
  3. Backend: Added GET /api/prompts and GET /api/prompts/{id} endpoints
  4. Backend: Prompt indexing on UserPromptSubmit hook with real-time broadcast
  5. Frontend: Added prompts store with pagination/expand/copy functionality
  6. Frontend: Created PromptLibraryModal component with infinite scroll
  7. Frontend: Converted single FAB to expandable menu
  8. WebSocket: Added NewPrompt event for real-time updates
- Files created:
  - `crates/clauset-types/src/prompt.rs` - Prompt and PromptSummary types
  - `crates/clauset-core/src/prompt_indexer.rs` - Backfill logic
  - `crates/clauset-server/src/routes/prompts.rs` - API routes
  - `frontend/src/stores/prompts.ts` - SolidJS store
  - `frontend/src/components/prompts/PromptLibraryModal.tsx` - Modal UI
- Files modified:
  - `crates/clauset-core/src/interaction_store.rs` - prompts table + CRUD
  - `crates/clauset-types/src/hooks.rs` - Added cwd to UserPromptSubmit
  - `crates/clauset-server/src/routes/hooks.rs` - Index prompts on hook
  - `crates/clauset-types/src/ws.rs` - Added NewPrompt message
  - `crates/clauset-server/src/global_ws.rs` - Forward NewPrompt events
  - `frontend/src/lib/api.ts` - Prompt API types
  - `frontend/src/lib/globalWs.ts` - Handle new_prompt events
  - `frontend/src/pages/Sessions.tsx` - FAB menu + modal integration

### Previous

### Import Session Enhancement (Just Completed)
- **Problem**: Import from terminal created empty session shell - no chat history, status "Created" instead of "Stopped"
- **Fix**:
  1. Added `read_transcript()` method to `ClaudeSessionReader` to parse Claude's JSONL transcripts
  2. Updated import endpoint to read transcript and store messages in chat_messages table
  3. Set imported session status to "Stopped" (ready to resume)
- Files changed:
  - `crates/clauset-core/src/claude_sessions.rs` - Added `TranscriptMessage` type and `read_transcript()` method
  - `crates/clauset-server/src/routes/sessions.rs` - Enhanced `import_session()` to import chat history

### Previous
- Terminal mode session ID capture fix completed

### Terminal Mode Session ID Capture
- **Problem**: Session ID capture only worked in StreamJson mode (via `ProcessEvent::Claude` events in websocket.rs)
- **Root cause**: Terminal mode doesn't emit JSON events - only raw PTY output. The existing code in `websocket.rs` listened for `ProcessEvent::Claude(ClaudeEvent::System)` events which never arrive in Terminal mode.
- **Fix**: Capture Claude's session ID from hook events (SessionStart, UserPromptSubmit, etc.) - every hook includes `claude_session_id`
- Files changed:
  - `crates/clauset-server/src/routes/hooks.rs` - Added `extract_claude_session_id()` helper; capture session ID on first hook
  - `crates/clauset-core/src/db.rs` - Updated `update_claude_session_id()` to only update if current value is nil (idempotent)

### Previous
- Chat line break preservation fix completed

### Chat Line Break Preservation Fix
- **Problem**: Chat view collapsed all line breaks - text displayed as single paragraph even when terminal showed proper formatting
- **Root cause**: `parseTextBlocks()` in MarkdownContent skipped empty lines (`if (line.trim())`) and rendered each line as separate `<span>` elements
- **Fix**:
  1. Accumulate consecutive text lines into paragraphs, flush on empty line
  2. Added new `paragraph` block type with `white-space: pre-wrap`
  3. Use `<p>` tags with proper margins for paragraph separation
- File changed: `frontend/src/components/chat/MessageBubble.tsx`

### Previous
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

### Terminal Width Fix for Chat-Default Mode (Final Fix)
- **Problem**: When chat mode is the default tab, terminal displays at wrong width
- **Root cause**: Race condition - server creates PTY before client sends dimensions
- **Fix**: Two-phase approach:
  1. Send device-appropriate defaults IMMEDIATELY on connect (45 cols for iPhone, 80 for desktop)
  2. Send accurate dimensions when terminal becomes visible
- Files changed:
  - `frontend/src/lib/ws.ts` - Added `getDeviceDefaultDimensions()`, send immediately on connect, update on change
  - `frontend/src/lib/terminalSizing.ts` - Don't trust fitAddon if container is hidden
  - `frontend/src/components/terminal/TerminalView.tsx` - Defer server notification until visible
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

# Clauset Feature Plan: Interaction Timeline, Diffs, Search & Analytics

## Executive Summary

This comprehensive plan extends Clauset with interaction tracking, file diff viewing, cross-session search, cost analytics, and rollback capabilities. The design builds upon the existing robust infrastructure (reliable streaming, connection state management, hook integration) rather than replacing it.

**Key Design Principles:**
1. **Build on existing infrastructure** - Extend the hook system, WebSocket protocol, and store patterns already in place
2. **Non-blocking operations** - All persistence happens asynchronously via `tokio::spawn`
3. **Mobile-first** - All UI optimized for iPhone Safari with safe areas and touch interactions
4. **30-day retention** - Auto-cleanup to manage storage growth
5. **Deduplication** - Content-addressed storage for file snapshots

---

## Current State

### What Already Exists
| Component | Status | Details |
|-----------|--------|---------|
| Hook endpoint | ✅ | POST /api/hooks with 9 event types |
| Real-time broadcast | ✅ | WebSocket ActivityUpdate events |
| Activity persistence | ⚠️ Partial | Only last 5 actions stored |
| Reliable streaming | ✅ | Sequence numbers, ACKs, gap recovery |
| Connection resilience | ✅ | 8-state machine, iOS lifecycle handling |

### What Needs to Be Added
- **interactions** table for full prompt→response history
- **tool_invocations** table for detailed tool call log
- **file_snapshots** + **file_contents** tables for diffing
- **FTS5** virtual tables for full-text search
- **Frontend** timeline, diff viewer, search, analytics components

---

## Phase 1: Database Schema & Migrations

### New Tables

```sql
-- 1. Interactions: Track each prompt→response cycle
CREATE TABLE IF NOT EXISTS interactions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    sequence_number INTEGER NOT NULL,
    user_prompt TEXT NOT NULL,
    assistant_summary TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    cost_usd_delta REAL NOT NULL DEFAULT 0.0,
    input_tokens_delta INTEGER NOT NULL DEFAULT 0,
    output_tokens_delta INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',  -- 'active', 'completed', 'failed'
    error_message TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX idx_interactions_session_id ON interactions(session_id);
CREATE INDEX idx_interactions_started_at ON interactions(started_at);
CREATE UNIQUE INDEX idx_interactions_session_seq ON interactions(session_id, sequence_number);

-- 2. Tool Invocations: Detailed tool call log
CREATE TABLE IF NOT EXISTS tool_invocations (
    id TEXT PRIMARY KEY,
    interaction_id TEXT NOT NULL,
    tool_use_id TEXT,
    sequence_number INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    tool_input TEXT NOT NULL,              -- JSON
    tool_output_preview TEXT,              -- First 1KB
    file_path TEXT,
    is_error INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_ms INTEGER,
    FOREIGN KEY (interaction_id) REFERENCES interactions(id) ON DELETE CASCADE
);

CREATE INDEX idx_tool_invocations_interaction ON tool_invocations(interaction_id);
CREATE INDEX idx_tool_invocations_tool_name ON tool_invocations(tool_name);
CREATE INDEX idx_tool_invocations_file_path ON tool_invocations(file_path);

-- 3. File Contents: Deduplicated content storage (content-addressed)
CREATE TABLE IF NOT EXISTS file_contents (
    content_hash TEXT PRIMARY KEY,         -- SHA256 hex
    compressed_content BLOB NOT NULL,      -- zstd compressed
    original_size INTEGER NOT NULL,
    compression_ratio REAL,
    created_at TEXT NOT NULL,
    reference_count INTEGER NOT NULL DEFAULT 1
);

-- 4. File Snapshots: Point-in-time file state
CREATE TABLE IF NOT EXISTS file_snapshots (
    id TEXT PRIMARY KEY,
    interaction_id TEXT NOT NULL,
    tool_invocation_id TEXT,
    file_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    snapshot_type TEXT NOT NULL,           -- 'before' or 'after'
    file_size INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (interaction_id) REFERENCES interactions(id) ON DELETE CASCADE,
    FOREIGN KEY (content_hash) REFERENCES file_contents(content_hash)
);

CREATE INDEX idx_snapshots_interaction ON file_snapshots(interaction_id);
CREATE INDEX idx_snapshots_path ON file_snapshots(file_path);
CREATE INDEX idx_snapshots_hash ON file_snapshots(content_hash);

-- 5. FTS5 Virtual Tables
CREATE VIRTUAL TABLE IF NOT EXISTS interactions_fts USING fts5(
    user_prompt, assistant_summary,
    content='interactions', content_rowid='rowid'
);

CREATE VIRTUAL TABLE IF NOT EXISTS tool_invocations_fts USING fts5(
    file_path, tool_input, tool_name,
    content='tool_invocations', content_rowid='rowid'
);
```

### Migration Strategy

Follow existing pattern in `db.rs`:

```rust
fn migrate_interactions(&self, conn: &Connection) -> Result<()> {
    if !Self::table_exists(conn, "interactions") {
        conn.execute_batch(/* SQL above */)?;
        tracing::info!(target: "clauset::db", "Created interactions table");
    }
    Ok(())
}
```

### Files to Modify
- `crates/clauset-core/src/db.rs` - Add tables, migrations, CRUD methods
- `crates/clauset-types/src/lib.rs` - Add `Interaction`, `ToolInvocation`, `FileSnapshot` structs
- `crates/clauset-core/Cargo.toml` - Add `sha2`, `zstd` dependencies

### Storage Estimates (30-day retention)
| Table | Records/Day | Size/Day | 30-Day Total |
|-------|-------------|----------|--------------|
| interactions | 200 | 300 KB | 9 MB |
| tool_invocations | 2,000 | 2.4 MB | 72 MB |
| file_snapshots | 500 | 100 KB | 3 MB |
| file_contents | 30 unique | 90 KB | 2.7 MB |
| FTS indexes | - | 1 MB | 30 MB |
| **Total** | - | ~4 MB | **~120 MB** |

---

## Phase 2: Interaction Capture Engine

### Hook Handler Enhancement

Extend `/api/hooks` to persist full interaction lifecycle:

```rust
// crates/clauset-core/src/interaction.rs (NEW)

pub struct InteractionTracker {
    active_interactions: RwLock<HashMap<Uuid, Interaction>>,
    pending_tools: RwLock<HashMap<String, PendingToolInvocation>>,
    db: Arc<SessionStore>,
}

impl InteractionTracker {
    pub async fn on_hook_event(&mut self, event: HookEventPayload) {
        match event.hook_event_name.as_str() {
            "UserPromptSubmit" => self.start_interaction(event).await,
            "PreToolUse" => self.start_tool(event).await,
            "PostToolUse" => self.complete_tool(event).await,
            "Stop" => self.complete_interaction(event).await,
            _ => {}
        }
    }
}
```

### File Snapshot Capture

For Write/Edit tools only (per user preference):

```rust
// crates/clauset-core/src/snapshot.rs (NEW)

const MAX_SNAPSHOT_SIZE: u64 = 1_048_576; // 1 MB

pub async fn capture_file_snapshot(path: &Path) -> Option<FileSnapshot> {
    let metadata = fs::metadata(path).await.ok()?;
    if metadata.len() > MAX_SNAPSHOT_SIZE { return None; }

    let content = fs::read(path).await.ok()?;
    let hash = sha256_hex(&content);
    let compressed = zstd::encode_all(&content[..], 3).ok()?;

    Some(FileSnapshot { hash, content: compressed, ... })
}
```

### Hook Handler Flow

```
UserPromptSubmit → Create interaction record
                 ↓
PreToolUse (Write/Edit) → Capture "before" snapshot async
                        ↓
PostToolUse → Capture "after" snapshot, record invocation
            ↓
Stop → Complete interaction, calculate cost/token deltas
```

### New WebSocket Messages

```rust
// crates/clauset-types/src/ws.rs

InteractionStarted { interaction_id, sequence, prompt_preview },
InteractionCompleted { interaction_id, summary, cost_delta, files_changed },
ToolRecorded { interaction_id, tool_name, file_path },
```

### Files to Create/Modify
- `crates/clauset-core/src/interaction.rs` (NEW)
- `crates/clauset-core/src/snapshot.rs` (NEW)
- `crates/clauset-server/src/routes/hooks.rs` - Add async persistence
- `crates/clauset-types/src/ws.rs` - Add message types
- `crates/clauset-server/src/state.rs` - Add InteractionTracker to AppState

---

## Phase 3: Diff Engine

### Diff Computation Module

```rust
// crates/clauset-core/src/diff.rs (NEW)

use similar::TextDiff;

pub struct DiffEngine;

impl DiffEngine {
    pub fn compute(
        old_content: &[u8],
        new_content: &[u8],
        options: &DiffOptions,
    ) -> Result<DiffResult> {
        if is_binary(old_content) || is_binary(new_content) {
            return Ok(DiffResult::binary());
        }

        let diff = TextDiff::from_lines(
            &String::from_utf8_lossy(old_content),
            &String::from_utf8_lossy(new_content),
        );

        // Convert to hunks with context lines
        // Return mobile-friendly paginated result
    }
}
```

### API Endpoints

```
GET /api/sessions/{id}/interactions
    → List all interactions with summary

GET /api/interactions/{id}
    → Full interaction with tool invocations

GET /api/diff?from={snapshot_id}&to={snapshot_id}
    → Compute diff between two snapshots

GET /api/sessions/{id}/files-changed
    → All files modified in session with change counts
```

### Response Format (Mobile-Optimized)

```json
{
  "diff": {
    "file_path": "src/lib.rs",
    "hunks": [
      {
        "old_start": 10, "new_start": 10,
        "lines": [
          {"kind": "context", "content": "fn main() {"},
          {"kind": "remove", "content": "    old_code();"},
          {"kind": "add", "content": "    new_code();"}
        ]
      }
    ],
    "stats": {"additions": 1, "deletions": 1, "hunks": 1},
    "truncated": false
  }
}
```

### Dependencies
```toml
# crates/clauset-core/Cargo.toml
similar = "2.6"
```

---

## Phase 4: Cross-Session Search

### Search Module

```rust
// crates/clauset-core/src/search.rs (NEW)

pub struct SearchEngine {
    conn: Mutex<Connection>,
}

impl SearchEngine {
    pub fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        page: usize,
    ) -> Result<SearchResponse> {
        let fts_query = sanitize_fts_query(query);

        // FTS5 query with BM25 ranking
        let sql = r#"
            SELECT i.*, highlight(interactions_fts, 0, '<mark>', '</mark>') as snippet
            FROM interactions_fts
            JOIN interactions i ON interactions_fts.rowid = i.rowid
            WHERE interactions_fts MATCH ?1
            ORDER BY bm25(interactions_fts)
            LIMIT ?2 OFFSET ?3
        "#;

        // Execute and return paginated results
    }
}
```

### API Endpoint

```
GET /api/search?q={query}&scope={all|prompts|files}&project={path}&limit=20
```

### Response Format

```json
{
  "results": [
    {
      "type": "interaction",
      "session_id": "uuid",
      "snippet": "...implement <mark>authentication</mark>...",
      "timestamp": "2024-01-15T10:30:00Z",
      "context": { "project_path": "/Users/dev/my-project" }
    }
  ],
  "total_count": 42,
  "has_more": true
}
```

---

## Phase 5: Cost Analytics

### Analytics Module

```rust
// crates/clauset-core/src/analytics.rs (NEW)

pub struct CostAnalytics {
    pub total_cost: f64,
    pub by_project: Vec<ProjectCost>,
    pub by_model: Vec<ModelCost>,
    pub by_day: Vec<DailyCost>,
}

pub async fn get_analytics(
    db: &SessionStore,
    range: DateRange,
) -> Result<CostAnalytics> {
    // Aggregate from sessions table (existing cost data)
    // + interactions table for per-interaction breakdown
}
```

### API Endpoint

```
GET /api/analytics?range={week|month|all}
```

---

## Phase 6: Frontend - Timeline View

### Component Hierarchy

```
Session.tsx
  └── TimelineView (NEW - third tab option)
       ├── TimelineHeader
       │    ├── InteractionCounter
       │    └── CostSummary
       └── TimelineList (virtualized)
            └── InteractionCard (repeated)
                 ├── PromptPreview
                 ├── ToolCallsList
                 ├── FilesChangedList
                 └── CostBadge
```

### Store Design

```typescript
// frontend/src/stores/interactions.ts (NEW)

interface InteractionsState {
  bySession: Record<string, Interaction[]>;
  loading: Record<string, boolean>;
}

// Path-based updates for granular reactivity
export function updateInteraction(sessionId, id, updates) {
  const idx = store.bySession[sessionId].findIndex(i => i.id === id);
  setStore('bySession', sessionId, idx, updates);
}
```

### WebSocket Integration

```typescript
// Extend globalWs.ts message handling

case 'interaction_started':
  addInteraction(session_id, { id, prompt, status: 'active', ... });
  break;

case 'interaction_completed':
  updateInteraction(session_id, id, { status: 'completed', ... });
  break;
```

---

## Phase 7: Frontend - Diff Viewer

### Component Structure

```
DiffViewer (modal)
  ├── DiffHeader
  │    ├── FileSelector (tabs, swipeable)
  │    └── InteractionPicker (from/to dropdowns)
  ├── UnifiedDiffView
  │    └── DiffHunk (collapsible)
  │         └── DiffLine (syntax highlighted)
  └── DiffFooter (stats, file pager)
```

### Mobile UX

- Swipe navigation between files
- Collapsible hunks to reduce scrolling
- Syntax highlighting via Prism.js (lightweight)
- Maximum 500 lines per response to prevent scroll jank

---

## Phase 8: Frontend - Search Modal

### Component Structure

```
SearchModal (overlay)
  ├── SearchInput (debounced, autofocus)
  ├── FilterChips (All | Prompts | Files | Commands)
  ├── SearchResults (virtualized)
  │    └── ResultCard (highlighted snippet)
  └── EmptyState / LoadingState
```

### Features

- 300ms debounce on input
- FTS5 highlighting with `<mark>` tags
- Paginated with "Load More"
- Tap result to navigate to session/interaction

---

## Phase 9: Frontend - Analytics Dashboard

### Component Structure

```
Analytics (route /analytics)
  ├── DateRangeToggle (Week | Month | All Time)
  ├── TotalCostCard (hero)
  ├── DailySpendingChart (simple bar chart, no external lib)
  ├── BreakdownByProject (horizontal bars)
  └── BreakdownByModel (horizontal bars)
```

### Design

- Simple SVG bar charts (no chart library needed)
- Touch-friendly 44px minimum tap targets
- 2-minute cache on analytics data

---

## Phase 10: Rollback Feature

### Implementation Strategy

**For Git Repos:**
1. Store git HEAD commit hash when interaction starts
2. Rollback = `git checkout <hash> -- <files>`
3. Only affect files touched in that interaction

**For Non-Git Directories:**
1. Use file snapshots from `file_contents` table
2. Decompress and restore "before" snapshots
3. Only for files with captured snapshots

### API Endpoint

```
POST /api/interactions/{id}/rollback
```

### Request/Response

```json
// Request
{ "dry_run": true }  // Preview what would change

// Response
{
  "files_to_restore": [
    { "path": "src/lib.rs", "action": "restore", "size": 1234 }
  ],
  "warnings": ["File has been modified since snapshot"]
}
```

### UI

- "↩ Undo" button on InteractionCard
- Confirmation modal with affected files list
- Dry-run preview before actual rollback

---

## Implementation Order

### Sprint 1: Foundation (Week 1-2)
| Task | Files | Priority |
|------|-------|----------|
| Database migrations | `db.rs` | P0 |
| Type definitions | `types/lib.rs` | P0 |
| InteractionTracker | `interaction.rs` (new) | P0 |
| File snapshot capture | `snapshot.rs` (new) | P0 |
| Hook handler extension | `routes/hooks.rs` | P0 |

### Sprint 2: Backend Features (Week 3-4)
| Task | Files | Priority |
|------|-------|----------|
| Diff computation | `diff.rs` (new) | P0 |
| Search with FTS5 | `search.rs` (new) | P1 |
| Analytics queries | `analytics.rs` (new) | P1 |
| API endpoints | `main.rs` | P0 |
| WebSocket messages | `ws.rs` | P0 |

### Sprint 3: Frontend Core (Week 5-6)
| Task | Files | Priority |
|------|-------|----------|
| Interactions store | `stores/interactions.ts` | P0 |
| Timeline view | `components/timeline/` | P0 |
| InteractionCard | `InteractionCard.tsx` | P0 |
| WebSocket handlers | `globalWs.ts` | P0 |

### Sprint 4: Frontend Features (Week 7-8)
| Task | Files | Priority |
|------|-------|----------|
| Diff viewer | `components/diff/` | P0 |
| Search modal | `SearchModal.tsx` | P1 |
| Analytics page | `pages/Analytics.tsx` | P1 |
| Rollback UI | `InteractionCard.tsx` | P2 |

---

## Testing Strategy

### Unit Tests
- Diff computation with various file types
- FTS5 query sanitization
- File snapshot compression/decompression

### Integration Tests
- Hook → InteractionTracker → Database flow
- WebSocket message broadcasting
- Search result pagination

### E2E Tests (Manual on iPhone Safari)
1. Create session, send 3-4 prompts that edit files
2. Open timeline, verify all interactions appear
3. Tap file badge, verify diff viewer opens
4. Compare interactions #1 and #3
5. Search for prompt text, verify results
6. View analytics, verify cost breakdown
7. Rollback interaction #2, verify files restored

### Regression Checks
- Existing terminal streaming still works
- Connection resilience (disconnect/reconnect)
- iOS lifecycle (background/foreground)

---

## File Change Summary

### Backend (Rust) - New Files
```
crates/clauset-core/src/
  ├── interaction.rs    (InteractionTracker)
  ├── snapshot.rs       (File snapshot capture)
  ├── diff.rs           (Diff computation)
  ├── search.rs         (FTS5 search)
  └── analytics.rs      (Cost analytics)
```

### Backend (Rust) - Modified Files
```
crates/clauset-core/src/db.rs           (+500 lines)
crates/clauset-core/Cargo.toml          (+3 deps)
crates/clauset-types/src/lib.rs         (+100 lines)
crates/clauset-types/src/ws.rs          (+50 lines)
crates/clauset-server/src/main.rs       (+100 lines)
crates/clauset-server/src/routes/hooks.rs (+150 lines)
crates/clauset-server/src/state.rs      (+20 lines)
```

### Frontend (TypeScript) - New Files
```
frontend/src/
  ├── stores/
  │    ├── interactions.ts
  │    ├── diffs.ts
  │    ├── analytics.ts
  │    └── search.ts
  ├── pages/
  │    └── Analytics.tsx
  └── components/
       ├── timeline/
       │    ├── TimelineView.tsx
       │    ├── TimelineList.tsx
       │    ├── InteractionCard.tsx
       │    ├── ToolCallItem.tsx
       │    └── FileBadge.tsx
       ├── diff/
       │    ├── DiffViewer.tsx
       │    ├── DiffHeader.tsx
       │    ├── DiffHunk.tsx
       │    └── DiffLine.tsx
       └── search/
            ├── SearchModal.tsx
            ├── SearchInput.tsx
            └── SearchResult.tsx
```

### Frontend (TypeScript) - Modified Files
```
frontend/src/pages/Session.tsx          (+50 lines)
frontend/src/lib/api.ts                 (+100 lines)
frontend/src/lib/globalWs.ts            (+50 lines)
frontend/src/index.tsx                  (+5 lines)
frontend/src/index.css                  (+100 lines)
```

---

## Dependencies to Add

### Backend
```toml
# crates/clauset-core/Cargo.toml
sha2 = "0.10"      # SHA-256 hashing
zstd = "0.13"      # Compression
similar = "2.6"    # Diff algorithm
```

### Frontend
```json
// frontend/package.json
{
  "prismjs": "^1.29.0"  // Syntax highlighting (optional)
}
```

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Storage growth | High | 30-day retention, content deduplication |
| Hook latency | Medium | Async persistence via tokio::spawn |
| Large diffs | Medium | 500 line limit, truncation flag |
| FTS query injection | Low | Query sanitization |
| Rollback data loss | High | Dry-run preview, confirmation modal |

---

## Success Criteria

1. **Timeline**: All interactions visible within 100ms of completion
2. **Diffs**: Compute and display in <500ms for typical files
3. **Search**: Results appear within 200ms of query
4. **Analytics**: Dashboard loads in <1s
5. **Rollback**: Successfully restore files with user confirmation
6. **Storage**: <150MB after 30 days of typical usage
7. **Mobile**: All features work on iPhone Safari with smooth scrolling

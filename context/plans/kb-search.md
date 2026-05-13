# Plan: Knowledge Base with Search

**Status:** Proposed
**Depends on:** kb-chrome-extension (for data source), system-daemon-wiring

## Summary
Build a searchable knowledge base on top of Chrome data, notes, and meeting records. Start with simple full-text search, then layer in vector embeddings for semantic similarity.

## Tasks

### T01 — Unified search index table
- Create `kb_entries` table: id, source_type (bookmark|page|note|meeting), source_id, title, content TEXT, url TEXT, tags TEXT, created_at, updated_at
- This is a denormalized search index populated by triggers or application logic
- Add FTS5 virtual table for full-text search: `CREATE VIRTUAL TABLE kb_fts USING fts5(title, content, tags, content=kb_entries, content_rowid=id)`

### T02 — Populate search index from Chrome data
- After saving Chrome bookmarks/pages, upsert into `kb_entries`
- Title, content (page text), tags flow into the FTS index
- Daemon background task: rebuild index periodically

### T03 — Populate search index from notes and meetings
- Knowledge CLI `kb add note "text"` — saves a note, indexes it
- Meeting exports (from personal-meetings plan) feed into the index
- CLI: `kb note list`, `kb note add <text>`, `kb note delete <id>`

### T04 — Search CLI
- CLI: `kb search <query>` — FTS5 search across all sources
- Results: ranked by relevance, grouped by source type
- Show title, snippet (FTS5 snippet()), source, date
- CLI: `kb search --source bookmark` — filter by source type

### T05 — Vector embeddings for semantic search
- Integrate a local embedding model (e.g., `candle` + `all-MiniLM-L6-v2`) or use Gemini embedding API
- Store embeddings in a `kb_embeddings` table (id, entry_id, embedding BLOB, model, dimensions)
- CLI: `kb search --semantic <query>` — cosine similarity search
- Fall back to FTS5 when embeddings not available

### T06 — Knowledge graph (relationships)
- Add `kb_links` table: id, from_entry_id, to_entry_id, relationship TEXT (e.g., "related", "mentions", "source")
- CLI: `kb relate <id> <other_id> [type]` — create a link
- CLI: `kb graph <id>` — show linked entries (depth 1-2)
- Later: auto-detect links via entity extraction

### T07 — Daemon background jobs
- Periodic re-indexing: keep FTS index fresh
- Embedding generation: batch process un-embedded entries
- Configurable intervals in `maia.json`

### T08 — Validation & cleanup
- Test FTS5 search returns relevant results
- Test semantic search (if embeddings configured)
- Test knowledge graph CLI
- Test with large dataset (stress test FTS performance)
- Verify all `kb` CLI commands work through daemon socket

## Done checks
- `kb search "query"` returns ranked results from bookmarks, pages, notes, meetings
- `kb search --semantic "concept"` returns semantically similar entries
- Notes can be added and searched
- Knowledge graph links can be created and traversed
- Search index stays in sync with source data

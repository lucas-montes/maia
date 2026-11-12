# Statement entry lifecycle

How a dropped file becomes entries, what decides each entry's verdict, and when
a verdict is recomputed. The server-side truth lives in `src/statements/store.rs`.

## The two kinds of entry status

`bank_statement_lines.status` is one of six values, and they split into two
kinds:

| Status | Kind | Set by | Survives a re-read? |
|---|---|---|---|
| `imported` | user decision | `mark_imported`, once the import books a transaction | yes |
| `skipped` | user decision | `mark_skipped`, when the user deselects the entry | yes |
| `pending` | derived | staging, or a re-read | no — recomputed |
| `informational` | derived | the parser marked the row as carrying no money movement (`LineKind::is_informational`, i.e. a trade) | no — recomputed |
| `duplicate` | derived | another **live** statement holds the same `dedupe_key` | no — recomputed |
| `error` | derived | the import could not book the line (`mark_error`) | no — recomputed |

`imported` and `skipped` are the only statuses a re-read preserves. Every other
status is a function of the file's bytes plus the state of the other
statements, so it is thrown away and recomputed rather than trusted.

## What claims content

A line's cross-statement identity is `dedupe_key`. Two lines with the same key
are the same money, so at most one of them may be imported.

**Only a live statement claims its lines.** "Live" means both of:

- `deleted_at IS NULL` — the statement was not discarded
- `status <> 'ignored'` — the user said "not this file" without deleting it

The claim lookup, in `save_statement`:

```sql
SELECT l.id FROM bank_statement_lines l
JOIN bank_statements s ON s.id = l.statement_id
WHERE l.dedupe_key=?1 AND l.statement_id<>?2 AND l.status<>'duplicate'
  AND s.deleted_at IS NULL AND s.status <> 'ignored'
ORDER BY l.created_at LIMIT 1
```

A line that finds a claim stages as `duplicate` and names the line it was kept
from in `duplicate_of`: it is visible in the review but never importable. A
line that finds no claim stages as `pending`.

This is the only duplicate lookup in the codebase. `describe_duplicates` in
`src/statements/preview.rs` reads already-stored `duplicate_of` links and
performs no lookup of its own.

## Staging and re-reading

`save_statement` has three branches, keyed on `(source, source_key)` and the
file's bytes:

| Branch | When | Effect |
|---|---|---|
| no-op | same `source_key`, same `source_hash`, statement not deleted, and it still holds a `pending`/`imported`/`skipped` entry | reports `skipped_reason = "already ingested"`; nothing is re-read |
| re-read | same `source_key` with different bytes, or the statement was deleted, or it holds no entry the user can act on | header updated, every **derived** line deleted, the fresh lines inserted and their verdicts recomputed |
| new | no such statement | statement inserted, lines staged |

The re-read branch deletes `pending`, `informational`, `duplicate` and `error`
lines and keeps `imported` and `skipped`. The delete must happen *before* the
insert: `bank_statement_lines` is unique on `(statement_id, fingerprint)`, so a
surviving line would make the re-insert a no-op and keep its stale verdict
forever.

### Where the bytes live in the key

The file sources put the file's hash in the `source_key` —
`credit_agricole:<account>:<hash>`, `firstrade:<account>:<hash>`,
`trading212:csv:<account>:<hash>`. So for a file, the same `source_key` means
the same bytes, and re-dropping a file always takes the no-op or the re-read
branch — never the "new statement" branch.

The T212 **feed** is the exception: its key is the month bucket
(`trading212:<account>:<month>`) and its hash covers the whole payload, so
every month of one fetch shares that hash. A feed sync whose payload changed
therefore re-reads every month, and one whose payload did not changes nothing.

### When a statement is read again

The no-op is a statement-level decision, so it is all-or-nothing: a statement
that holds one `pending` entry is a no-op as a whole, and its `duplicate`
entries are not recomputed. Only a statement with nothing the user can act on
is read again.

```mermaid
flowchart TD
    A[File dropped] --> B{source_key known?}
    B -- same bytes, statement live --> C{Still an entry to act on?}
    C -- pending, imported or skipped --> D[No-op: "already ingested"]
    C -- nothing --> E[Re-read]
    B -- same bytes, statement deleted --> E
    B -- different bytes --> E
    B -- never seen --> F[New statement]
    E --> G[Delete derived lines: pending, informational, duplicate, error]
    G --> H[Keep imported and skipped]
    H --> I[Insert the fresh lines]
    F --> I
    I --> J{Another live statement holds the dedupe_key?}
    J -- yes --> K[Status duplicate, duplicate_of set]
    J -- no --> L[Status pending]
```

## Transfer matches

`transfer_matches` points at the lines it pairs with `ON DELETE CASCADE`, so
deleting a derived line removes the match that used it. `match_pending` runs
right after a staging batch (`src/statements/mod.rs`), so matches are rebuilt
from the fresh lines rather than restored.

## Where a file sits

`reconcile_files` derives each file's root from the state of its statements
rather than remembering it, and moves the file to match:

| statements for that `stored_path`          | root          |
|---------------------------------------------|---------------|
| any live statement with an unsettled entry | `staged/`     |
| every live statement imported/skipped      | `parsed/`     |
| every statement soft-deleted or ignored    | `discarded/`  |

"settled" means every entry is `imported` or `skipped`. The test is the
complement (`status NOT IN ('imported','skipped')`), so it stays correct as
statuses are added. A `pending` entry is work the user has not done; an
`error` is an import that failed; a `duplicate` is an entry another statement
still claims, which the user may yet release; an `informational` entry is one
the review never offers for import. None of them is an import, so none of them
lets the file reach `parsed/`.

The placement is derived, so it moves in both directions: a file that gains an
unsettled entry walks back to `staged/` on the next pass.

## Known gaps

Tracked in `context/plans/finance-model.md`:

- **Rows already stored as `duplicate` are not repaired (plan T6).** A line that
  a since-discarded statement claimed keeps its verdict until migration v12
  runs.
- **The no-op is all-or-nothing (accepted).** A statement holding one `pending`
  entry and one stale `duplicate` entry is a no-op, so the stale duplicate
  survives. Only an all-duplicate statement is read again.
- **An informational-only feed month is touched on every sync (accepted).** It
  holds no entry the user can act on, so it is read again. Nothing is lost — the
  entries come back with the same ids — but the statement's `updated_at` moves.
- **A file with informational entries waits in `staged/` (accepted).** The
  review offers no way to act on a trade, so such a file has no user action
  available until trades become movable.

## Where the code lives

| Concern | File |
|---|---|
| staging, verdicts, import | `src/statements/store.rs` |
| file placement (`staged/`, `parsed/`, `discarded/`) | `src/statements/watch.rs` |
| review payload, ignore/discard endpoints | `src/handlers/statements.rs` |
| what the import will do | `src/statements/preview.rs` |
| entry shape and `LineKind` | `src/statements/draft.rs` |

See also: [overview.md](../overview.md), [context-map.md](../context-map.md),
[glossary.md](../glossary.md)

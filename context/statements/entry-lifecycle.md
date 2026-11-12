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
| no-op | same `source_key`, same `source_hash`, statement not deleted | reports `skipped_reason = "already ingested"`; nothing is re-read |
| re-read | same `source_key`, different bytes, or the statement was deleted | header updated, every **derived** line deleted, the fresh lines inserted and their verdicts recomputed |
| new | no such statement | statement inserted, lines staged |

The re-read branch deletes `pending`, `informational`, `duplicate` and `error`
lines and keeps `imported` and `skipped`. The delete must happen *before* the
insert: `bank_statement_lines` is unique on `(statement_id, fingerprint)`, so a
surviving line would make the re-insert a no-op and keep its stale verdict
forever.

```mermaid
flowchart TD
    A[File dropped] --> B{source_key known?}
    B -- same bytes, statement live --> C[No-op: "already ingested"]
    B -- same bytes, statement deleted --> D[Re-read]
    B -- different bytes --> D
    B -- never seen --> E[New statement]
    D --> F[Delete derived lines: pending, informational, duplicate, error]
    F --> G[Keep imported and skipped]
    G --> H[Insert the fresh lines]
    E --> H
    H --> I{Another live statement holds the dedupe_key?}
    I -- yes --> J[Status duplicate, duplicate_of set]
    I -- no --> K[Status pending]
```

## Transfer matches

`transfer_matches` points at the lines it pairs with `ON DELETE CASCADE`, so
deleting a derived line removes the match that used it. `match_pending` runs
right after a staging batch (`src/statements/mod.rs`), so matches are rebuilt
from the fresh lines rather than restored.

## Known gaps

Tracked in `context/plans/finance-model.md`:

- **Unchanged bytes never re-read (plan T3).** The no-op branch returns before
  any line is looked at, so a statement whose entries are all `duplicate` stays
  that way when the same file is dropped again. The re-read is correct; nothing
  triggers it yet.
- **Rows already stored as `duplicate` are not repaired (plan T6).** A line that
  a since-discarded statement claimed keeps its verdict until migration v12
  runs.
- **An all-duplicate file moves to `parsed/` (plan T4).** `collect_file_states`
  counts a file as settled when no line is `pending` or `error`, and a
  `duplicate` line is neither, so the file looks imported although nothing was
  imported.

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

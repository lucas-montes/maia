# Knowledge Base — Unified Search & Hand-Rolled Similarity

## Change summary
Treat notes and Chrome-captured URLs as one searchable knowledge base in the desktop app. The Chrome
side of this needs less work than the roadmap wording suggested: the existing manual shortcuts
(`send-url`/`send-url-and-content`) already are the intended capture mechanism — no new passive
`chrome.bookmarks.onCreated`-style listening is wanted. The real work is the knowledge-base logic
itself: hand-rolled (no third-party embedding API) similarity scoring over notes + URLs' content, a
"find related items" feature, and a search/query view in the desktop app — explicitly *not* built on
Gemini embeddings, by direction, so the groundwork here is a pluggable scoring mechanism with TF-IDF +
cosine similarity as the first hand-implemented algorithm, room left for others later.

## Success criteria
- [ ] Verify the existing `send-url`/`send-url-and-content` Chrome shortcuts and their server-side
      handling are complete and working end to end (this may turn out to need little or no fixing —
      confirm rather than assume either way).
- [ ] A hand-rolled TF-IDF index built from notes' title+body and URLs' title+captured content
      (tokenize, compute term frequencies, inverse document frequency across the whole corpus, store a
      sparse term-weight representation per item).
- [ ] A cosine-similarity "find related items" feature: given one note or URL, rank every other item
      (notes and URLs together) by TF-IDF cosine similarity, surfaced in the desktop UI as a "related"
      panel.
- [ ] A free-text search/query view: tokenize the query the same way, score every item against it by the
      same TF-IDF weighting, return ranked results spanning both notes and URLs.
- [ ] The scoring mechanism is structured so a second hand-rolled algorithm (e.g. tag/keyword overlap,
      or a simple co-occurrence graph) could be added later without redoing the indexing groundwork —
      doesn't have to be built now, just not architecturally foreclosed.
- [ ] `cargo test -p maia` covers the TF-IDF math directly (a small hand-computable corpus produces the
      expected term weights and similarity ranking) — this is exactly the kind of logic that's easy to
      get subtly wrong and should be nailed down with real test vectors, not just "it compiles."

## Constraints and non-goals
**Constraints:**
- No external embedding API (Gemini or otherwise) for similarity scoring — hand-rolled algorithms only,
  per explicit direction. The existing `ai` crate (Gemini) stays out of this plan entirely.
- Desktop-only UI, matching where knowledge-base work has always lived.
- Notes and URLs are indexed together as one corpus — a search/related-items result can mix both types.

**Non-goals:**
- No new Chrome auto-capture (bookmarks/reading-list/history listeners) — the manual shortcuts are the
  capture mechanism, confirmed as sufficient.
- No AI-synthesized answers (no RAG) — this is ranked retrieval only, not a question-answering feature.
- No mobile search UI.
- Does not touch meeting transcripts (separate plan) even though those would be a natural future
  addition to the same corpus once they exist.

## Assumptions
- Tokenization is simple (lowercase, split on non-alphanumeric, maybe a small stopword list) — not
  stemming/lemmatization, which would be a meaningfully bigger hand-rolled undertaking on its own.
  Worth flagging if real results turn out noisy enough that stemming becomes worth adding later.
- The TF-IDF index is recomputed on demand (e.g. on each search, or on a cache invalidated by any
  note/url change) rather than incrementally maintained — simplest correct approach for a personal-scale
  corpus; revisit if the corpus grows large enough that recomputing becomes slow.
- "URLs' captured content" means whatever text the `send-url-and-content` flow already stores (the
  `urls` table's content, if any) — not a fresh fetch/re-scrape of the page at index time.

## Task stack (T01..T04)

- [ ] T01: Verify the existing Chrome capture flow (status:open)
  - Goal: Walk `send-url`/`send-url-and-content` end to end (extension → server) and confirm it works
    as intended; fix anything found broken, but don't expand scope beyond what's already there.
  - Done when: both shortcuts produce the expected `urls` row (with content, for the content variant)
    reliably.

- [ ] T02: Tokenizer + TF-IDF index (status:open)
  - Goal: A tokenizer and a TF-IDF computation over the combined notes+URLs corpus, with hand-written
    unit tests against a small known corpus (hand-computed expected weights).
  - Done when: test vectors match hand-calculated TF-IDF values exactly.

- [ ] T03: Cosine similarity — related items + search (status:open)
  - Goal: Cosine similarity scoring against the TF-IDF vectors; a "related items" query (given one
    item, rank the rest) and a free-text search query (tokenize the query, score against the corpus).
  - Done when: a small corpus with an obviously-related pair of items (e.g. two notes sharing distinctive
    vocabulary) ranks them as each other's top match; an unrelated item ranks low.

- [ ] T04: Desktop search/knowledge-base view (status:open)
  - Goal: A search box returning ranked mixed notes+URLs results; a "related" panel on note/URL detail
    views.
  - Done when: searching a real term from your own notes/bookmarks surfaces sensible results in the UI.

## Risks / gotchas
- TF-IDF is sensitive to corpus size and vocabulary overlap — with a small personal corpus, very common
  words might still dominate if the stopword list is too thin, and very short notes/URLs (just a title,
  no body) will score poorly almost everywhere. Worth checking real results against real data early,
  not just synthetic test vectors.
- Recomputing the whole index on every search is fine until the corpus is large enough that it isn't —
  there's no number given here for "large enough"; just keep an eye on it once this is actually in use
  rather than pre-optimizing now.

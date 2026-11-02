# Statement lifecycle folders.

Drop CSVs into `<source>` subfolders under `dump/` and the server stages them
in the background. Nothing manual, no upload screen needed:

    dump/credit_agricole/   # Crédit Agricole `.csv` exports
    dump/trading212/        # Trading 212 `.csv` exports
    dump/firstrade/         # Firstrade `.csv` exports

The subfolder name picks the parser (`SourceId::parse`); the server does not
sniff content. When a file has been handled it is moved — never copied — so the
dropped-in folder always empties itself:

- parsed → `parsed/<source>/<same-name>` with `stored_path` on the statement
  rows repointed to the new location. A name that is already taken gets a `-2`
  suffix instead of overwriting, because a previous statement row may still
  point at that exact path.
- failed → `failed/<source>/<same-name>` plus a `<name>.log` next to it with
  the timestamp and the parse error. The latest error wins when the same file
  name fails twice.

Everything else keeps out of trouble: files dropped directly in `dump/` (not
inside a source folder) park in `failed/unknown/` with a note; PDFs, images and
other non-CSV files in a source folder park in that source's `failed/` with
"unsupported file type"; a misspelled source folder is only warned about in the
log, never touched.

Restaging is idempotent: the content hash is the statement's `source_key` seed,
so re-dropping the same export stages nothing new.

In the checkout the parsed and failed trees are runtime data (gitignored); only
the `dump/` scaffolding is kept. The server also keeps hash-named copies beside
the database (`statements/<source>/` for feed documents), which belong to its
own store, not to these folders.

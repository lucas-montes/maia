Statement drop folders.

Put Crédit Agricole CSV exports in `inbox/credit_agricole/` and Firstrade CSVs
in `inbox/firstrade/`. The server polls each folder with the parser named by
`statements.watch_dirs` in `maia.json`, stages what it finds, and leaves it to
the Statements screen to import.

The server keeps a copy of every input under `statements/<source>/`, beside the
database. That directory is its own, not a drop folder: the watched paths are
the ones under `inbox/`, so a stored copy is never ingested again.

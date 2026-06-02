# Glossary

| Term | Definition |
|---|---|
| **Crud** | Proc-macro in `maia-macros` that derives `create`, `find`, `update`, `delete`, `list` for SQLite models |
| **StructuredOutput** | Proc-macro in `ai-derive` that generates Gemini API schema for structured JSON extraction |
| **MET** | Metabolic Equivalent of Task — measures energy cost of physical activities |
| **WatchedDir** | A directory the daemon monitors via inotify for new files (receipt images, bank statements, etc.) |
| **Receipt** | AI-extracted structured data from a receipt image (store, date, total, products) |
| **BMR/TDEE** | Basal Metabolic Rate / Total Daily Energy Expenditure — calorie calculators |
| **Knowledge Base** | System for storing notes, bookmarks, and links with similarity search (not yet implemented) |
| **Native Messaging Host** | Chrome extension companion binary that receives browser events and saves them locally |
| **Saved URL** | A link captured from the Chrome tool or added manually, stored as a DB-only record with title, URL, source, tags, and is_new flag. No snapshot files or note/goal linking — opened externally via `tauri-plugin-opener`. |

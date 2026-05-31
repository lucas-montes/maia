# Maia Desktop UI

This app is the Tauri-based desktop shell for Maia. It uses plain HTML, CSS, and TypeScript with local persistence for the current implementation pass.

## What’s in the app

- Dashboard with activity summaries
- Todos with add, search, filter, edit, complete, and delete
- Receipts with selection, validation, and status actions
- Notes with add, edit, search, and delete
- Settings with local toggles and demo-data reset

## Run locally

```bash
cd /home/lucas/Projects/maia/maia-ui
pnpm install
pnpm tauri dev
```

## Notes

- Data is saved to browser storage for now.
- The UI is ready for later wiring to Maia backend commands and shared crates.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

# Maia browser extension

Saves the current page's URL — and optionally its HTML — straight into
Maia's `urls` resource on `sync-server`, via `POST /urls`. No native
messaging host, no background daemon: the extension talks to the server
directly over HTTP.

## Install

Brave (and any other Chromium-based browser) only loads this as an
"unpacked" extension — it isn't published to the Chrome Web Store (see
`context/plans/kb-chrome-extension.md` for why). Pick one of the two setups
below, then follow [Load it into the browser](#load-it-into-the-browser).

### Option A — plain checkout, no Nix

Nothing to build. The extension's files already live in this directory.

### Option B — via the `~/.dotfiles` home-manager setup

If you use the `maia` flake input and the `home-manager/modules/browser.nix`
wiring, `nix build .#chrome-ext` (run from this repo) packages this
directory as a flake output, and a `home.file` entry keeps
`~/.local/share/maia/chrome-ext` pointed at the current build. This matters
for one reason: you pick the directory in "Load unpacked" exactly once,
ever. After that, any change here just needs:

```sh
cd ~/.dotfiles && nix flake lock --update-input maia && home-manager switch
```

followed by a click of "Reload" on the extension's card — never re-picking
the directory.

## Load it into the browser

1. Open `brave://extensions` (or `chrome://extensions`).
2. Enable **Developer mode** (top-right toggle).
3. Click **Load unpacked** and select:
   - Option A: this `chrome_ext/` directory.
   - Option B: `~/.local/share/maia/chrome-ext`.

## Configure

Right-click the extension's icon → **Options** (or `brave://extensions` →
*Details* → *Extension options*). Set:

- **Server base URL** — e.g. `http://localhost:3030`, matching
  `services.maia`'s `bind_addr`/`port`.
- **API key** — the same value as `services.maia.apiKey`.

Saving requests permission to reach that one origin; nothing else is asked
for.

## Use it

- **Keyboard shortcuts**: `Ctrl+Shift+Y` saves the current tab's URL,
  `Ctrl+Shift+U` saves the URL plus the page's HTML
  (`document.documentElement.outerHTML`, captured client-side — not a full
  asset-inclusive page archive).
- **Right-click menu**: "Save Page URL to Maia" / "Save Page URL and
  Content to Maia" on any page.
- **Side panel** (toolbar icon): lists links still marked unread, with
  "Open" and "Mark read" actions, plus a status line for the last
  save/connection outcome.

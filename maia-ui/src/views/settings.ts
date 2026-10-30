import { api, type StatementFeedSettings } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { syncAtom, settingsAtom } from "../lib/stores";
import { backToDashboard, panelClose, panelOpen, statusBanner } from "./shared";
import { refreshIcons } from "../lib/icons";

/** `managed` fields come from the deployment config, so the form shows them
 *  without letting them be typed into: the service reads the same file, and a
 *  value edited here would be silently discarded on save. */
const CONFIG_FIELD_DEFS: { key: string; label: string; placeholder: string; managed?: boolean }[] = [
  { key: "database", label: "Database File", placeholder: "maia.db", managed: true },
  { key: "receipts_path", label: "Receipts Path", placeholder: "receipts" },
  { key: "nutriments_path", label: "Nutriments Path", placeholder: "" },
  { key: "model_api", label: "Model API Key", placeholder: "", managed: true },
  { key: "receipts.archived_dir", label: "Receipts Archive Directory", placeholder: "receipts_archive" },
];

function extractConfigFields(jsonStr: string): Record<string, string> {
  const fields: Record<string, string> = {};
  for (const def of CONFIG_FIELD_DEFS) fields[def.key] = "";
  try {
    const obj = JSON.parse(jsonStr);
    for (const key of Object.keys(fields)) {
      let val: unknown = obj;
      for (const part of key.split(".")) {
        if (val && typeof val === "object" && part in (val as Record<string, unknown>)) {
          val = (val as Record<string, unknown>)[part];
        } else {
          val = undefined;
          break;
        }
      }
      fields[key] = val !== null && val !== undefined ? String(val) : "";
    }
  } catch {
    // If JSON is invalid, keep defaults
  }
  return fields;
}

function buildConfigJson(fields: Record<string, string>): string {
  const obj: Record<string, unknown> = {};
  for (const def of CONFIG_FIELD_DEFS) {
    // Deployment-owned fields are shown but never serialised. Including them
    // would make save_config reject the whole save, since it refuses those keys
    // by name rather than dropping them silently.
    if (def.managed) continue;
    const val = fields[def.key]?.trim() || "";
    const parts = def.key.split(".");
    if (parts.length === 1) {
      obj[def.key] = val;
    } else if (parts.length === 2) {
      if (!obj[parts[0]]) obj[parts[0]] = {};
      (obj[parts[0]] as Record<string, unknown>)[parts[1]] = val || null;
    }
  }
  return JSON.stringify(obj, null, 2);
}

/** Settings — config form editor with deployment-managed fields. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Settings", "Configure your Maia workspace")}
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-3 text-sm font-medium text-ink-200">Configuration</h3>
        <p class="mb-4 text-xs text-ink-400">
          Desktop paths are saved to <code class="rounded bg-surface-800 px-1 py-0.5 font-mono text-ink-300">~/.config/maia/config.json</code>.
          Fields marked as coming from the deployment config are read from <code class="rounded bg-surface-800 px-1 py-0.5 font-mono text-ink-300">/etc/maia/config.json</code>,
          which the service reads too.
        </p>
        <div data-slot="status"></div>
        <div class="mt-4 space-y-4" data-slot="fields"></div>
        <div class="mt-6 flex items-center gap-3">
          <button class="btn btn-primary btn-sm" type="button" data-action="save-settings">Save Settings</button>
          <button class="btn btn-ghost btn-sm" type="button" data-action="reload-settings">Reload from File</button>
          ${backToDashboard()}
        </div>
      </div>
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-3 text-sm font-medium text-ink-200">Trading 212</h3>
        <p class="mb-3 text-xs text-ink-400">
          Credentials live in the per-user overlay on the server, not in this app's config file.
          They are written once and never read back, so this form only reports whether a secret is
          set. Saving takes effect after the sync server restarts.
        </p>
        <div data-slot="feed-status"></div>
        <div class="mt-3 space-y-4" data-slot="feed-form"></div>
        <div class="mt-4 flex items-center gap-3">
          <button class="btn btn-primary btn-sm" type="button" data-action="save-feed">Save Trading 212</button>
        </div>
      </div>
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-3 text-sm font-medium text-ink-200">Sync Server</h3>
        <p class="mb-3 text-xs text-ink-400" data-slot="sync-status"></p>
        <p class="text-xs text-ink-500">The full pairing panel, with a QR code for the phone, is under <strong>Sync</strong>.</p>
      </div>
    ${panelClose}
  `;
  const fieldsEl = root.querySelector("[data-slot='fields']") as HTMLElement;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const syncEl = root.querySelector("[data-slot='sync-status']") as HTMLElement;
  const feedStatusEl = root.querySelector("[data-slot='feed-status']") as HTMLElement;
  const feedFormEl = root.querySelector("[data-slot='feed-form']") as HTMLElement;

  let feed: StatementFeedSettings["trading212"] | null = null;
  let feedBusy = false;
  let feedMessage: { ok: boolean; text: string } | null = null;

  /** The Trading 212 block, rendered separately from the main config form: it
   *  saves through its own endpoint and needs "is a secret set?" rather than a
   *  value. */
  const renderFeed = (): void => {
    if (!feed) {
      setHtml(
        feedStatusEl,
        feedMessage
          ? `<div class="banner banner-err">${escapeHtml(feedMessage.text)}</div>`
          : `<div class="banner">Loading Trading 212 settings...</div>`,
      );
      setHtml(feedFormEl, "");
      (root.querySelector("[data-action='save-feed']") as HTMLButtonElement).disabled = true;
      return;
    }

    const status = !feed.enabled
      ? `<span class="badge badge-muted">disabled</span>`
      : !(feed.apiKeySet && feed.apiSecretSet)
        ? `<span class="badge badge-warn">credentials missing</span>`
        : `<span class="badge badge-ok">configured</span>`;
    setHtml(
      feedStatusEl,
      `<div class="flex flex-wrap items-center gap-2 text-xs text-ink-400">
        ${status}
        <span>${escapeHtml(feed.environment)}</span>
        <span>·</span>
        <span>${feed.lookbackDays} day${feed.lookbackDays === 1 ? "" : "s"} lookback</span>
        ${feedMessage ? `<span class="${feedMessage.ok ? "text-ok-400" : "text-err-400"}">· ${escapeHtml(feedMessage.text)}</span>` : ""}
      </div>`,
    );

    // A secret that is set is described rather than filled in, so the input
    // stays empty and a save without typing keeps the stored value.
    const keyHint = feed.apiKeySet ? "•••••••• already set — leave blank to keep" : "not set";
    const secretHint = feed.apiSecretSet ? "•••••••• already set — leave blank to keep" : "not set";

    setHtml(
      feedFormEl,
      `<div class="flex flex-wrap items-center gap-4">
        <label class="flex cursor-pointer items-center gap-2 text-sm text-ink-300">
          <input type="checkbox" id="feed-enabled" class="checkbox checkbox-sm" ${feed.enabled ? "checked" : ""} />
          Enable Trading 212 feed
        </label>
        <label class="field-label mb-0" for="feed-environment">Environment</label>
        <select id="feed-environment" class="select select-sm select-bordered">
          <option value="live" ${feed.environment === "live" ? "selected" : ""}>live</option>
          <option value="demo" ${feed.environment === "demo" ? "selected" : ""}>demo</option>
        </select>
        <label class="field-label mb-0" for="feed-lookback">Lookback days</label>
        <input type="number" id="feed-lookback" min="1" max="3650" value="${feed.lookbackDays}"
          class="input input-sm input-bordered w-28" />
      </div>
      <div class="grid gap-4 sm:grid-cols-2">
        <div>
          <label class="field-label" for="feed-api-key">API key</label>
          <input type="password" id="feed-api-key" autocomplete="off" spellcheck="false"
            placeholder="${escapeHtml(keyHint)}" class="input input-bordered w-full font-mono" />
        </div>
        <div>
          <label class="field-label" for="feed-api-secret">API secret</label>
          <input type="password" id="feed-api-secret" autocomplete="off" spellcheck="false"
            placeholder="${escapeHtml(secretHint)}" class="input input-bordered w-full font-mono" />
        </div>
      </div>
      <label class="flex cursor-pointer items-center gap-2 text-sm text-ink-400">
        <input type="checkbox" id="feed-clear" class="checkbox checkbox-sm" />
        Clear stored credentials
      </label>`,
    );

    (root.querySelector("[data-action='save-feed']") as HTMLButtonElement).disabled = feedBusy;
    (root.querySelector("[data-action='save-feed']") as HTMLElement).textContent = feedBusy
      ? "Saving..."
      : "Save Trading 212";
  };

  const render = (): void => {
    const s = settingsAtom.get();
    setHtml(
      fieldsEl,
      CONFIG_FIELD_DEFS.map(
        (def) => `
        <div>
          <label class="field-label" for="cfg-${def.key}">${escapeHtml(def.label)}${
            def.managed ? ` <span class="font-normal text-ink-500">(from the deployment config)</span>` : ""
          }</label>
          <input type="text" id="cfg-${def.key}" value="${escapeHtml(s.configFields[def.key] || "")}"
            placeholder="${escapeHtml(def.placeholder)}" ${def.managed ? "readonly disabled" : ""}
            class="input input-bordered w-full ${def.managed ? "cursor-not-allowed opacity-60" : ""}" />
        </div>`,
      ).join(""),
    );
    setHtml(statusEl, statusBanner(s));
    const sync = syncAtom.get();
    const listening = sync.info?.isListening ?? false;
    setHtml(
      syncEl,
      `<span class="${listening ? "text-ok-400" : "text-err-400"}">●</span> ${listening ? "running" : "not reachable"} <span class="text-ink-500">· port ${sync.info?.port || "?"}</span>`,
    );
    const busy = s.status === "loading" || s.status === "saving";
    (root.querySelector("[data-action='save-settings']") as HTMLButtonElement).disabled = busy;
    (root.querySelector("[data-action='reload-settings']") as HTMLButtonElement).disabled = busy;
    (root.querySelector("[data-action='save-settings']") as HTMLElement).textContent =
      s.status === "saving" ? "Saving..." : "Save Settings";
    renderFeed();
    refreshIcons(root);
  };

  const load = async (): Promise<void> => {
    settingsAtom.set({ ...settingsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.readConfig();
      if (res.success && res.data) {
        settingsAtom.set({
          ...settingsAtom.get(),
          configJson: JSON.stringify(JSON.parse(res.data), null, 2),
          configFields: extractConfigFields(res.data),
          status: "idle",
          message: "",
        });
      } else {
        const fields: Record<string, string> = {};
        for (const def of CONFIG_FIELD_DEFS) fields[def.key] = "";
        settingsAtom.set({
          ...settingsAtom.get(),
          configFields: fields,
          configJson: buildConfigJson(fields),
          status: "idle",
          message: "Loaded default configuration (maia.json not found).",
        });
      }
    } catch (e) {
      settingsAtom.set({ ...settingsAtom.get(), status: "error", message: `Failed to load config: ${e}` });
    }
  };

  const loadFeed = async (): Promise<void> => {
    feedBusy = true;
    renderFeed();
    try {
      const res = await api.getStatementFeedSettings();
      if (res.success && res.data) {
        feed = res.data.trading212;
      } else {
        feedMessage = { ok: false, text: res.error ?? "Could not load Trading 212 settings" };
      }
    } catch (e) {
      feedMessage = { ok: false, text: `Could not load Trading 212 settings: ${e}` };
    } finally {
      feedBusy = false;
      renderFeed();
    }
  };

  const saveFeed = async (): Promise<void> => {
    if (!feed) return;
    const enabled = (root.querySelector("#feed-enabled") as HTMLInputElement).checked;
    const environment = (root.querySelector("#feed-environment") as HTMLSelectElement).value;
    const lookbackDays = Number(
      (root.querySelector("#feed-lookback") as HTMLInputElement).value || feed.lookbackDays,
    );
    const apiKey = (root.querySelector("#feed-api-key") as HTMLInputElement).value.trim();
    const apiSecret = (root.querySelector("#feed-api-secret") as HTMLInputElement).value.trim();
    const clearCredentials = (root.querySelector("#feed-clear") as HTMLInputElement).checked;

    if (!Number.isFinite(lookbackDays) || lookbackDays < 1 || lookbackDays > 3650) {
      feedMessage = { ok: false, text: "Lookback must be between 1 and 3650 days." };
      renderFeed();
      return;
    }
    if (clearCredentials && (apiKey || apiSecret)) {
      feedMessage = {
        ok: false,
        text: "Clear credentials and enter new ones are mutually exclusive.",
      };
      renderFeed();
      return;
    }

    feedBusy = true;
    feedMessage = null;
    renderFeed();
    try {
      const res = await api.saveStatementFeedSettings({
        trading212: {
          enabled,
          environment,
          lookbackDays,
          // Empty means "keep what is stored"; only a typed value replaces it.
          ...(apiKey ? { apiKey } : {}),
          ...(apiSecret ? { apiSecret } : {}),
          clearCredentials,
        },
      });
      if (!res.success) throw new Error(res.error ?? "Save failed");
      // The running server still has the old values until it restarts, so
      // re-reading them would make a successful save look discarded. Reflect
      // what was just written, and say plainly that a restart applies it.
      feed = {
        ...feed,
        enabled,
        environment,
        lookbackDays,
        apiKeySet: clearCredentials ? false : Boolean(apiKey) || feed.apiKeySet,
        apiSecretSet: clearCredentials ? false : Boolean(apiSecret) || feed.apiSecretSet,
      };
      feedMessage = { ok: true, text: "Saved. Restart the sync server to apply." };
    } catch (e) {
      feedMessage = { ok: false, text: `Save failed: ${e}` };
    } finally {
      feedBusy = false;
      renderFeed();
    }
  };

  const save = async (): Promise<void> => {
    const fields: Record<string, string> = {};
    for (const def of CONFIG_FIELD_DEFS) {
      const input = root.querySelector<HTMLInputElement>(`#cfg-${def.key}`);
      fields[def.key] = input?.value ?? "";
    }
    const json = buildConfigJson(fields);
    settingsAtom.set({ ...settingsAtom.get(), status: "saving", message: "" });
    try {
      const res = await api.saveConfig(json);
      if (res.success) {
        settingsAtom.set({ ...settingsAtom.get(), status: "saved", message: "Settings saved successfully.", configJson: json });
      } else {
        settingsAtom.set({ ...settingsAtom.get(), status: "error", message: res.error ?? "Failed to save config" });
      }
    } catch (e) {
      settingsAtom.set({ ...settingsAtom.get(), status: "error", message: `Failed to save config: ${e}` });
    }
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    if (target.closest("[data-action='save-settings']")) {
      void save();
      return;
    }
    if (target.closest("[data-action='reload-settings']")) {
      void load();
      return;
    }
    if (target.closest("[data-action='save-feed']")) {
      void saveFeed();
    }
  });

  render();
  void load();
  void loadFeed();
  const unsubs = [settingsAtom.subscribe(render), syncAtom.subscribe(render)];
  return () => unsubs.forEach((u) => u());
}

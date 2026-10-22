import { api } from "../lib/api";
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
  { key: "bank_statements_path", label: "Bank Statements Path", placeholder: "" },
  { key: "investments_statements_path", label: "Investments Statements Path", placeholder: "" },
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
      if (val === "" && (def.key === "bank_statements_path" || def.key === "investments_statements_path")) {
        obj[def.key] = null;
      } else {
        obj[def.key] = val;
      }
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
        <h3 class="mb-3 text-sm font-medium text-ink-200">Sync Server</h3>
        <p class="mb-3 text-xs text-ink-400" data-slot="sync-status"></p>
        <p class="text-xs text-ink-500">The full pairing panel, with a QR code for the phone, is under <strong>Sync</strong>.</p>
      </div>
    ${panelClose}
  `;
  const fieldsEl = root.querySelector("[data-slot='fields']") as HTMLElement;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const syncEl = root.querySelector("[data-slot='sync-status']") as HTMLElement;

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
    }
  });

  render();
  void load();
  const unsubs = [settingsAtom.subscribe(render), syncAtom.subscribe(render)];
  return () => unsubs.forEach((u) => u());
}

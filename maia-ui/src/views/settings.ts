import {
  api,
  type ModelSettings,
  type NixSettings,
  type SecretFlags,
  type SecretsPatch,
  type SettingsData,
  type TaskName,
} from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { syncAtom } from "../lib/stores";
import { panelClose, panelOpen } from "./shared";

/**
 * Settings.
 *
 * Two stores, and every field says which one it writes to:
 *  - **Nix** — everything that is not a secret. Saved into the JSON file the
 *    host's flake imports into `services.maia`; applies on the next
 *    `nixos-rebuild switch`, so each field shows the running value beside a
 *    pending one.
 *  - **Secret** — the sync key, model keys and Trading 212 credentials. Saved
 *    to the per-user overlay and only ever reported back as set / not set.
 *
 * Each section saves on its own.
 */

type SectionId = "server" | "models" | "tasks" | "statements" | "trading212" | "storage";

const SECTIONS: { id: SectionId; label: string }[] = [
  { id: "server", label: "Server" },
  { id: "models", label: "Models" },
  { id: "tasks", label: "Tasks" },
  { id: "statements", label: "Statements" },
  { id: "trading212", label: "Trading 212" },
  { id: "storage", label: "Storage" },
];

/** Which top-level Nix keys each section owns. */
const NIX_KEYS: Record<SectionId, (keyof NixSettings)[]> = {
  server: ["port", "openFirewall"],
  models: ["models"],
  tasks: ["tasks"],
  statements: ["statementsRoot", "statementsIntakeDir", "baseCurrency"],
  trading212: ["trading212"],
  storage: ["mediaDir", "notesDir", "archiveDir", "seedDir"],
};

const TASKS: { id: TaskName; label: string; hint: string; vision: boolean }[] = [
  { id: "receiptOcr", label: "Receipt OCR", hint: "Reads receipt photos. Vision models only.", vision: true },
  { id: "receiptMatching", label: "Receipt matching", hint: "Proposes the store, transaction and products for a receipt.", vision: false },
  { id: "statementExtraction", label: "Statement extraction", hint: "Reads the activity table out of Firstrade PDF statements.", vision: false },
];

const PRESETS: { id: string; label: string; provider: ModelSettings["provider"]; baseUrl: string }[] = [
  { id: "gemini", label: "Gemini", provider: "gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta/models" },
  { id: "openai", label: "OpenAI", provider: "openai", baseUrl: "https://api.openai.com/v1" },
  { id: "openrouter", label: "OpenRouter", provider: "openai", baseUrl: "https://openrouter.ai/api/v1" },
  { id: "ollama", label: "Ollama (local)", provider: "openai", baseUrl: "http://localhost:11434/v1" },
];

/** The draft for one model row, including what only the form knows. */
interface ModelRow extends ModelSettings {
  id: string;
  /** Not in the settings file yet. */
  isNew: boolean;
  apiKey: string;
  test?: { busy: boolean; ok?: boolean; text?: string };
}

interface SectionState {
  busy: boolean;
  message: { ok: boolean; text: string } | null;
}

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value ?? null));
const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

function randomKey(): string {
  const bytes = new Uint8Array(24);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function presetFor(model: ModelSettings): string {
  const url = (model.baseUrl ?? "").replace(/\/+$/, "");
  return PRESETS.find((p) => p.provider === model.provider && p.baseUrl === url)?.id ?? "custom";
}

export function mount(root: HTMLElement): () => void {
  let data: SettingsData | null = null;
  let loadError: string | null = null;
  /** The editable values: the settings file over the running values. */
  let draft: NixSettings = {};
  /** What `draft` started as, to tell which sections changed. */
  let baseline: NixSettings = {};
  let models: ModelRow[] = [];
  let removedModels: string[] = [];
  const secretsDraft = { syncApiKey: "", t212Key: "", t212Secret: "", t212Clear: false };
  let restartNeeded = false;
  const sections: Record<SectionId, SectionState> = Object.fromEntries(
    SECTIONS.map((s) => [s.id, { busy: false, message: null }]),
  ) as Record<SectionId, SectionState>;

  const editable = () => Boolean(data?.settingsFile) && !data?.pendingError;

  /** The value a key currently has: the settings file's, else the running one. */
  const resolved = (source: NixSettings | null, key: keyof NixSettings): unknown =>
    source && key in source ? source[key] : (data?.effective as NixSettings)[key];

  const resetDrafts = (): void => {
    if (!data) return;
    const pending = data.pending ?? {};
    const all = SECTIONS.flatMap((s) => NIX_KEYS[s.id]);
    draft = {};
    for (const key of all) (draft as Record<string, unknown>)[key] = clone(resolved(pending, key));
    // Feed and task objects are merged key by key in the module, so start
    // from the running values and lay the file's on top.
    draft.trading212 = { ...(data.effective.trading212 ?? {}), ...(pending.trading212 ?? {}) };
    draft.tasks = { ...(data.effective.tasks ?? {}), ...(pending.tasks ?? {}) };
    baseline = clone(draft);
    models = rowsFrom(draft.models);
    removedModels = [];
  };

  const rowsFrom = (saved: Record<string, ModelSettings> | undefined): ModelRow[] =>
    Object.entries(saved ?? {}).map(([id, m]) => ({
      id,
      provider: m.provider ?? "gemini",
      baseUrl: m.baseUrl ?? PRESETS.find((p) => p.provider === (m.provider ?? "gemini"))!.baseUrl,
      model: m.model,
      vision: Boolean(m.vision),
      isNew: false,
      apiKey: "",
    }));

  const modelsFromRows = (rows: ModelRow[] = models): Record<string, ModelSettings> =>
    Object.fromEntries(
      rows.map((m) => [m.id.trim(), { provider: m.provider, baseUrl: m.baseUrl || null, model: m.model.trim(), vision: Boolean(m.vision) }]),
    );

  const isDirty = (id: SectionId): boolean => {
    if (id === "models") {
      const typedKey = models.some((m) => m.apiKey.trim());
      // Compare as rows, so a default base URL filled into the form does not
      // count as a change to an entry that leaves it out.
      return typedKey || removedModels.length > 0 || !same(modelsFromRows(), modelsFromRows(rowsFrom(baseline.models)));
    }
    const nix = NIX_KEYS[id].some((k) => !same(draft[k], baseline[k]));
    if (id === "server") return nix || Boolean(secretsDraft.syncApiKey.trim());
    if (id === "trading212")
      return nix || Boolean(secretsDraft.t212Key.trim() || secretsDraft.t212Secret.trim()) || secretsDraft.t212Clear;
    return nix;
  };

  /** Keys in the settings file whose value differs from what is running. */
  const pendingKeys = (): string[] => {
    if (!data?.pending) return [];
    const eff = data.effective as Record<string, unknown>;
    return Object.entries(data.pending)
      .filter(([key, value]) => {
        if (key === "trading212" || key === "tasks") {
          const running = (eff[key] ?? {}) as Record<string, unknown>;
          return Object.entries(value ?? {}).some(([k, v]) => !same(v, running[k]));
        }
        if (key === "models" && !(key in eff)) return true;
        if (key === "models") {
          const running = (eff.models ?? {}) as Record<string, ModelSettings>;
          const file = (value ?? {}) as Record<string, ModelSettings>;
          const norm = (m: Record<string, ModelSettings>) =>
            Object.fromEntries(Object.entries(m).map(([id, x]) => [id, { provider: x.provider ?? "gemini", model: x.model, vision: Boolean(x.vision) }]));
          return !same(norm(file), norm(running));
        }
        return key in eff && !same(value, eff[key]);
      })
      .map(([key]) => key);
  };

  // ---- Rendering helpers ----

  const chip = (kind: "nix" | "secret") =>
    kind === "nix"
      ? `<span class="badge badge-info" title="Saved to the Nix settings file; applies after a rebuild">Nix</span>`
      : `<span class="badge badge-muted" title="Saved to ${escapeHtml(data?.overlayFile ?? "the overlay")}; never in Nix">secret</span>`;

  /** "pending rebuild · live: X" when the file and the running value differ. */
  const pendingNote = (key: keyof NixSettings, sub?: string): string => {
    if (!data?.pending) return "";
    const file = sub ? (data.pending[key] as Record<string, unknown> | undefined)?.[sub] : data.pending[key];
    if (file === undefined) return "";
    const live = sub
      ? ((data.effective as Record<string, unknown>)[key] as Record<string, unknown> | undefined)?.[sub]
      : (data.effective as Record<string, unknown>)[key];
    if (same(file, live)) return "";
    // A list or a map does not fit in a badge; say only that it differs.
    if (typeof live === "object" && live !== null)
      return `<span class="badge badge-warn" title="In the settings file, not running yet">pending rebuild</span>`;
    const shown = live === null || live === undefined || live === "" ? "default" : String(live);
    return `<span class="badge badge-warn" title="In the settings file, not running yet">pending rebuild · live: ${escapeHtml(shown)}</span>`;
  };

  const disabledAttr = () => (editable() ? "" : "disabled");

  const textField = (key: keyof NixSettings, label: string, hint: string, opts: { placeholder?: string } = {}) => `
    <div>
      <label class="field-label flex flex-wrap items-center gap-2" for="f-${key}">${escapeHtml(label)} ${chip("nix")} ${pendingNote(key)}</label>
      <input type="text" id="f-${key}" data-nix="${key}" value="${escapeHtml(String(draft[key] ?? ""))}"
        placeholder="${escapeHtml(opts.placeholder ?? "module default")}" spellcheck="false" ${disabledAttr()}
        class="input input-bordered w-full font-mono text-sm" />
      ${hint ? `<p class="mt-1 text-xs text-ink-500">${hint}</p>` : ""}
    </div>`;

  const secretField = (id: string, label: string, isSet: boolean, value: string, extra = "") => `
    <div>
      <label class="field-label flex flex-wrap items-center gap-2" for="${id}">${escapeHtml(label)} ${chip("secret")}
        ${isSet ? `<span class="badge badge-ok">set</span>` : `<span class="badge badge-muted">not set</span>`}</label>
      <div class="flex gap-2">
        <input type="password" id="${id}" data-secret="${id}" autocomplete="off" spellcheck="false" value="${escapeHtml(value)}"
          placeholder="${isSet ? "•••••••• leave blank to keep" : "not set"}" class="input input-bordered w-full font-mono text-sm" />
        ${extra}
      </div>
    </div>`;

  const sectionFooter = (id: SectionId) => {
    const state = sections[id];
    const dirty = isDirty(id);
    return `<div class="mt-4 flex flex-wrap items-center gap-3">
      <button class="btn btn-primary btn-sm" type="button" data-save="${id}" ${state.busy || !dirty ? "disabled" : ""}>
        ${state.busy ? "Saving..." : "Save"}</button>
      ${dirty && !state.busy ? `<button class="btn btn-ghost btn-sm" type="button" data-revert="${id}">Revert</button>` : ""}
      ${state.message ? `<span class="text-xs ${state.message.ok ? "text-ok-400" : "text-err-400"}">${escapeHtml(state.message.text)}</span>` : ""}
    </div>`;
  };

  const section = (id: SectionId, title: string, intro: string, body: string) => `
    <section id="settings-${id}" class="scroll-mt-4 rounded border border-surface-800 bg-base-100 p-4">
      <h3 class="mb-1 text-sm font-medium text-ink-200">${escapeHtml(title)}</h3>
      <p class="mb-4 text-xs text-ink-400">${intro}</p>
      <div class="space-y-4">${body}</div>
      ${id === "storage" ? "" : sectionFooter(id)}
      ${id === "storage" && editable() ? sectionFooter(id) : ""}
    </section>`;

  // ---- Sections ----

  const renderServer = (): string => {
    const sync = syncAtom.get();
    const listening = sync.info?.isListening ?? false;
    const s = data!.secrets;
    return section(
      "server",
      "Server",
      `<span class="${listening ? "text-ok-400" : "text-err-400"}">●</span> ${listening ? "running" : "not reachable"}
       <span class="text-ink-500">· port ${sync.info?.port ?? "?"}</span>. The pairing QR code for the phone is under <strong>Sync</strong>.`,
      `<div class="grid gap-4 sm:grid-cols-2">
        <div>
          <label class="field-label flex flex-wrap items-center gap-2" for="f-port">Port ${chip("nix")} ${pendingNote("port")}</label>
          <input type="number" id="f-port" data-nix="port" min="1" max="65535" value="${escapeHtml(String(draft.port ?? ""))}"
            ${disabledAttr()} class="input input-bordered w-32" />
        </div>
        <div>
          <span class="field-label flex flex-wrap items-center gap-2">Firewall ${chip("nix")} ${pendingNote("openFirewall")}</span>
          <label class="flex cursor-pointer items-center gap-2 text-sm text-ink-300">
            <input type="checkbox" data-nix="openFirewall" class="checkbox checkbox-sm" ${draft.openFirewall ? "checked" : ""} ${disabledAttr()} />
            Open the port on the LAN so the phone can reach it
          </label>
        </div>
      </div>
      ${secretField(
        "syncApiKey",
        "Sync API key",
        s.syncApiKey || s.syncApiKeyFromNix,
        secretsDraft.syncApiKey,
        `<button class="btn btn-ghost btn-sm" type="button" data-action="generate-key">Generate</button>`,
      )}
      ${s.syncApiKeyFromNix
        ? `<p class="text-xs text-warn-400">The running key still comes from an older Nix config, and the next rebuild drops it. Save it here first. Generate a new one if you like, then update the phone.</p>`
        : `<p class="text-xs text-ink-500">Changing it unpairs the phone until you enter the new key there.</p>`}`,
    );
  };

  const renderModels = (): string => {
    const keys = data!.secrets.models;
    const rows = models
      .map((m, i) => {
        const preset = presetFor(m);
        const keySet = keys[m.id] ?? false;
        const test = m.test;
        return `<div class="rounded border border-surface-800 p-3" data-model-row="${i}">
          <div class="grid gap-3 md:grid-cols-[minmax(8rem,1fr)_minmax(9rem,1fr)_2fr]">
            <div>
              <label class="field-label" for="m-id-${i}">Name</label>
              <input type="text" id="m-id-${i}" data-model="${i}" data-field="id" value="${escapeHtml(m.id)}" spellcheck="false"
                ${m.isNew ? "" : "readonly"} placeholder="gemini-flash" class="input input-bordered input-sm w-full font-mono ${m.isNew ? "" : "opacity-70"}" />
            </div>
            <div>
              <label class="field-label" for="m-preset-${i}">Provider</label>
              <select id="m-preset-${i}" data-model="${i}" data-field="preset" class="select select-sm select-bordered w-full">
                ${PRESETS.map((p) => `<option value="${p.id}" ${p.id === preset ? "selected" : ""}>${escapeHtml(p.label)}</option>`).join("")}
                <option value="custom" ${preset === "custom" ? "selected" : ""}>Custom (OpenAI-compatible)</option>
              </select>
            </div>
            <div>
              <label class="field-label" for="m-url-${i}">Base URL</label>
              <input type="text" id="m-url-${i}" data-model="${i}" data-field="baseUrl" value="${escapeHtml(m.baseUrl ?? "")}" spellcheck="false"
                class="input input-bordered input-sm w-full font-mono" />
            </div>
          </div>
          <div class="mt-3 grid gap-3 md:grid-cols-[2fr_2fr_auto] md:items-end">
            <div>
              <label class="field-label" for="m-model-${i}">Model</label>
              <input type="text" id="m-model-${i}" data-model="${i}" data-field="model" value="${escapeHtml(m.model)}" spellcheck="false"
                placeholder="${m.provider === "gemini" ? "gemini-2.5-flash" : "qwen2.5vl:7b"}" class="input input-bordered input-sm w-full font-mono" />
            </div>
            <div>
              <label class="field-label flex items-center gap-2" for="m-key-${i}">API key ${chip("secret")}
                ${keySet ? `<span class="badge badge-ok">set</span>` : `<span class="badge badge-muted">${preset === "ollama" ? "none needed" : "not set"}</span>`}</label>
              <input type="password" id="m-key-${i}" data-model="${i}" data-field="apiKey" value="${escapeHtml(m.apiKey)}" autocomplete="off"
                placeholder="${keySet ? "•••••••• leave blank to keep" : "not set"}" class="input input-bordered input-sm w-full font-mono" />
            </div>
            <div class="flex items-center gap-3">
              <label class="flex cursor-pointer items-center gap-2 text-sm text-ink-300">
                <input type="checkbox" data-model="${i}" data-field="vision" class="checkbox checkbox-sm" ${m.vision ? "checked" : ""} />
                Reads images
              </label>
            </div>
          </div>
          <div class="mt-3 flex flex-wrap items-center gap-2">
            <button class="btn btn-ghost btn-sm" type="button" data-action="test-model" data-index="${i}" ${test?.busy ? "disabled" : ""}>
              ${test?.busy ? "Testing..." : "Test connection"}</button>
            <button class="btn btn-ghost btn-sm text-err-400" type="button" data-action="remove-model" data-index="${i}">Remove</button>
            ${test && !test.busy ? `<span class="text-xs ${test.ok ? "text-ok-400" : "text-err-400"}">${escapeHtml(test.text ?? "")}</span>` : ""}
          </div>
        </div>`;
      })
      .join("");
    return section(
      "models",
      "Models",
      `Endpoints the server can call: Gemini, or anything OpenAI-compatible (OpenAI, OpenRouter, Ollama…). The endpoints go to Nix ${chip("nix")} and the keys to the overlay ${chip("secret")}. ${pendingNote("models")}`,
      `${rows || `<p class="text-xs text-ink-500">No models yet.</p>`}
       <button class="btn btn-ghost btn-sm" type="button" data-action="add-model">+ Add model</button>`,
    );
  };

  const renderTasks = (): string => {
    const saved = (data!.pending?.models ?? data!.effective.models ?? {}) as Record<string, ModelSettings>;
    const body = TASKS.map((task) => {
      const ids = draft.tasks?.[task.id] ?? [];
      const options = Object.entries(saved)
        .filter(([id, m]) => !ids.includes(id) && (!task.vision || m.vision))
        .map(([id]) => `<option value="${escapeHtml(id)}">${escapeHtml(id)}</option>`)
        .join("");
      const list = ids
        .map(
          (id, i) => `<li class="flex items-center gap-2 rounded border border-surface-800 px-2 py-1 text-sm">
            <span class="w-5 text-xs text-ink-500">${i + 1}.</span>
            <span class="flex-1 font-mono">${escapeHtml(id)}</span>
            ${saved[id] ? "" : `<span class="badge badge-err">not saved</span>`}
            <button class="btn btn-ghost btn-xs" type="button" data-task="${task.id}" data-move="-1" data-index="${i}" ${i === 0 ? "disabled" : ""} aria-label="Move up">↑</button>
            <button class="btn btn-ghost btn-xs" type="button" data-task="${task.id}" data-move="1" data-index="${i}" ${i === ids.length - 1 ? "disabled" : ""} aria-label="Move down">↓</button>
            <button class="btn btn-ghost btn-xs" type="button" data-task="${task.id}" data-drop="${i}" aria-label="Remove">✕</button>
          </li>`,
        )
        .join("");
      return `<div>
        <div class="field-label flex flex-wrap items-center gap-2">${escapeHtml(task.label)} ${pendingNote("tasks", task.id)}</div>
        <p class="mb-2 text-xs text-ink-500">${escapeHtml(task.hint)} ${ids.length ? "Tried in this order until one answers." : "Off: no model assigned."}</p>
        <ol class="space-y-1">${list}</ol>
        ${options && editable()
          ? `<select class="select select-sm select-bordered mt-2" data-task-add="${task.id}"><option value="">+ Add a model…</option>${options}</select>`
          : ""}
      </div>`;
    }).join("");
    return section(
      "tasks",
      "Tasks",
      `Which saved models each feature uses, as a fallback order. The review screens show which model produced each result. ${chip("nix")}`,
      body,
    );
  };

  const renderStatements = (): string =>
    section(
      "statements",
      "Statements",
      "Where bank and broker exports are picked up, and the currency imports are booked in.",
      `${textField("statementsRoot", "Statements folder", "Holds the <code>dump/</code>, <code>staged/</code>, <code>parsed/</code>, <code>discarded/</code> and <code>failed/</code> trees. Leave empty for the default under your XDG data directory.")}
       ${textField("statementsIntakeDir", "Intake folder", "Name an export for its source (<code>firstrade-2026-11.csv</code>, <code>t212_feb_aug.csv</code>) and drop it here. Only recognised files are moved out, so this can be <code>~/Downloads</code>.")}
       <div>
         <label class="field-label flex flex-wrap items-center gap-2" for="f-baseCurrency">Base currency ${chip("nix")} ${pendingNote("baseCurrency")}</label>
         <input type="text" id="f-baseCurrency" data-nix="baseCurrency" maxlength="3" value="${escapeHtml(String(draft.baseCurrency ?? ""))}"
           ${disabledAttr()} class="input input-bordered w-24 font-mono uppercase" />
       </div>`,
    );

  const renderTrading212 = (): string => {
    const feed = draft.trading212 ?? {};
    const s = data!.secrets;
    return section(
      "trading212",
      "Trading 212",
      "Polls the Trading 212 history API. The feed settings are stored in Nix and the credentials in the overlay.",
      `<div class="flex flex-wrap items-end gap-4">
        <label class="flex cursor-pointer items-center gap-2 text-sm text-ink-300">
          <input type="checkbox" data-feed="enable" class="checkbox checkbox-sm" ${feed.enable ? "checked" : ""} ${disabledAttr()} />
          Enabled ${chip("nix")} ${pendingNote("trading212", "enable")}
        </label>
        <div>
          <label class="field-label flex items-center gap-2" for="f-t212-env">Environment ${pendingNote("trading212", "environment")}</label>
          <select id="f-t212-env" data-feed="environment" class="select select-sm select-bordered" ${disabledAttr()}>
            <option value="live" ${feed.environment === "live" ? "selected" : ""}>live</option>
            <option value="demo" ${feed.environment === "demo" ? "selected" : ""}>demo</option>
          </select>
        </div>
        <div>
          <label class="field-label flex items-center gap-2" for="f-t212-lookback">Lookback days ${pendingNote("trading212", "lookbackDays")}</label>
          <input type="number" id="f-t212-lookback" data-feed="lookbackDays" min="1" max="3650" value="${escapeHtml(String(feed.lookbackDays ?? ""))}"
            ${disabledAttr()} class="input input-sm input-bordered w-28" />
        </div>
      </div>
      <div class="grid gap-4 sm:grid-cols-2">
        ${secretField("t212Key", "API key", s.trading212Key, secretsDraft.t212Key)}
        ${secretField("t212Secret", "API secret", s.trading212Secret, secretsDraft.t212Secret)}
      </div>
      <label class="flex cursor-pointer items-center gap-2 text-sm text-ink-400">
        <input type="checkbox" data-secret="t212Clear" class="checkbox checkbox-sm" ${secretsDraft.t212Clear ? "checked" : ""} />
        Clear stored credentials
      </label>`,
    );
  };

  const renderStorage = (): string => {
    const e = data!.effective;
    const readOnly = (label: string, value: string | null | undefined, hint: string) => `
      <div>
        <div class="field-label">${escapeHtml(label)}</div>
        <code class="block break-all rounded bg-surface-800 px-2 py-1 font-mono text-xs text-ink-300">${escapeHtml(value || "—")}</code>
        ${hint ? `<p class="mt-1 text-xs text-ink-500">${hint}</p>` : ""}
      </div>`;
    return section(
      "storage",
      "Storage",
      "Service state lives in the data directory, which systemd owns. Media, notes and the archive can be moved.",
      `${readOnly("Data directory", e.dataDir, "Holds the database. Move it in your NixOS configuration (<code>services.maia.dataDir</code>), together with the database file. It is not editable here because systemd owns the state directory.")}
       ${readOnly("Database", e.database, "")}
       ${textField("mediaDir", "Exercise media", "Read-only to the service, so it can point into a checkout.")}
       ${textField("notesDir", "Notes", "Where notes are mirrored as Markdown.")}
       ${textField("archiveDir", "Receipt archive", "Where archived receipt images are moved.")}
       ${textField("seedDir", "Exercise catalogue seed", "Empty means no seeding. Only used when the exercises table is empty.", { placeholder: "off" })}
       ${readOnly("Nix settings file", data!.settingsFile, "")}
       ${readOnly("Secrets overlay", data!.overlayFile, "")}`,
    );
  };

  const renderBanner = (): string => {
    if (!data) return "";
    const parts: string[] = [];
    if (!data.settingsFile) {
      parts.push(`<div class="banner">
        Nix settings are read-only: this host's configuration has no <code>services.maia.settingsFile</code>.
        Import a JSON file into <code>services.maia</code> and point <code>settingsFile</code> at it to edit them here.
        Secrets can still be changed.</div>`);
    } else if (data.pendingError) {
      parts.push(`<div class="banner banner-err">${escapeHtml(data.pendingError)}</div>`);
    }
    const pending = pendingKeys();
    if (pending.length && data.rebuildCommand) {
      parts.push(`<div class="banner banner-warn space-y-2">
        <div><strong>${pending.length} setting${pending.length === 1 ? "" : "s"} waiting for a rebuild</strong>
          <span class="text-ink-400">(${pending.map(escapeHtml).join(", ")})</span></div>
        ${data.settingsFileTracked
          ? ""
          : `<div class="text-xs">The settings file is not tracked by git, so the flake will not see it. Run <code class="break-all">git add ${escapeHtml(data.settingsFile ?? "")}</code> first.</div>`}
        <div class="flex flex-wrap items-center gap-2">
          <code class="break-all rounded bg-surface-800 px-2 py-1 font-mono text-xs text-ink-200">${escapeHtml(data.rebuildCommand)}</code>
          <button class="btn btn-ghost btn-xs" type="button" data-action="copy-rebuild">Copy</button>
        </div>
      </div>`);
    }
    if (restartNeeded) {
      parts.push(`<div class="banner banner-warn flex flex-wrap items-center gap-2">
        Secrets saved. They take effect after <code class="font-mono">sudo systemctl restart maia</code> and reopening this app.
        <button class="btn btn-ghost btn-xs" type="button" data-action="copy-restart">Copy</button>
      </div>`);
    }
    return parts.join("");
  };

  const render = (): void => {
    const scroll = root.querySelector<HTMLElement>("[data-slot='sections']")?.scrollTop ?? 0;
    if (!data) {
      setHtml(
        root,
        `${panelOpen("Settings", "Configure your Maia workspace")}
          <div class="banner ${loadError ? "banner-err" : ""}">${escapeHtml(loadError ?? "Loading settings...")}</div>
        ${panelClose}`,
      );
      return;
    }
    setHtml(
      root,
      `${panelOpen("Settings", "Configure your Maia workspace")}
        <div class="space-y-3" data-slot="banner">${renderBanner()}</div>
        <div class="grid gap-4 md:grid-cols-[10rem_minmax(0,1fr)]">
          <nav class="flex flex-wrap gap-1 self-start md:sticky md:top-0 md:flex-col" aria-label="Settings sections">
            ${SECTIONS.map(
              (s) => `<button type="button" data-jump="${s.id}"
                class="rounded px-2 py-1 text-left text-sm text-ink-300 hover:bg-surface-800 hover:text-ink-100">
                ${escapeHtml(s.label)}${isDirty(s.id) ? ` <span class="text-warn-400" title="Unsaved changes">●</span>` : ""}</button>`,
            ).join("")}
          </nav>
          <div class="min-w-0 space-y-4" data-slot="sections">
            ${renderServer()}${renderModels()}${renderTasks()}${renderStatements()}${renderTrading212()}${renderStorage()}
          </div>
        </div>
      ${panelClose}`,
    );
    const sectionsEl = root.querySelector<HTMLElement>("[data-slot='sections']");
    if (sectionsEl) sectionsEl.scrollTop = scroll;
  };

  /** Re-renders only the bits that depend on dirtiness, so typing keeps focus. */
  const refreshChrome = (): void => {
    for (const s of SECTIONS) {
      const btn = root.querySelector<HTMLButtonElement>(`[data-save="${s.id}"]`);
      if (btn) btn.disabled = sections[s.id].busy || !isDirty(s.id);
      const nav = root.querySelector<HTMLElement>(`[data-jump="${s.id}"]`);
      if (nav) nav.innerHTML = `${escapeHtml(s.label)}${isDirty(s.id) ? ` <span class="text-warn-400" title="Unsaved changes">●</span>` : ""}`;
    }
  };

  // ---- Loading and saving ----

  const load = async (): Promise<void> => {
    try {
      const res = await api.readSettings();
      if (!res.success || !res.data) throw new Error(res.error ?? "Could not read settings");
      data = res.data;
      loadError = null;
      resetDrafts();
    } catch (e) {
      loadError = `Could not read settings: ${e instanceof Error ? e.message : e}`;
    }
    render();
  };

  const say = (id: SectionId, ok: boolean, text: string) => {
    sections[id].message = { ok, text };
  };

  const saveNix = async (patch: Record<string, unknown>): Promise<void> => {
    if (!Object.keys(patch).length) return;
    const res = await api.saveNixSettings(patch);
    if (!res.success) throw new Error(res.error ?? "Save failed");
    data!.pending = res.data ?? data!.pending;
  };

  const saveSecretPatch = async (patch: SecretsPatch): Promise<void> => {
    if (!Object.keys(patch).length) return;
    const res = await api.saveSecrets(patch);
    if (!res.success || !res.data) throw new Error(res.error ?? "Save failed");
    data!.secrets = res.data as SecretFlags;
    restartNeeded = true;
  };

  /** The section's changed Nix keys. Empty strings are "use the default". */
  const nixPatch = (id: SectionId): Record<string, unknown> => {
    const patch: Record<string, unknown> = {};
    for (const key of NIX_KEYS[id]) {
      if (same(draft[key], baseline[key])) continue;
      const value = draft[key];
      patch[key] = typeof value === "string" && value.trim() === "" ? null : value;
    }
    return patch;
  };

  const save = async (id: SectionId): Promise<void> => {
    const state = sections[id];
    state.busy = true;
    state.message = null;
    render();
    try {
      if (id === "models") {
        const ids = models.map((m) => m.id.trim());
        if (ids.some((x) => !x)) throw new Error("every model needs a name");
        if (new Set(ids).size !== ids.length) throw new Error("two models share a name");
        if (models.some((m) => !m.model.trim())) throw new Error("every model needs a model id");
        const patch: Record<string, unknown> = { models: modelsFromRows() };
        // Taking a model away also takes it off the tasks, or the module
        // would refuse the file.
        if (removedModels.length) {
          const tasks: Record<string, string[]> = {};
          for (const t of TASKS) tasks[t.id] = (draft.tasks?.[t.id] ?? []).filter((x) => !removedModels.includes(x));
          draft.tasks = tasks;
          patch.tasks = tasks;
        }
        await saveNix(patch);
        const keys: Record<string, string | null> = {};
        for (const m of models) if (m.apiKey.trim()) keys[m.id.trim()] = m.apiKey.trim();
        for (const gone of removedModels) keys[gone] = null;
        await saveSecretPatch(Object.keys(keys).length ? { models: keys } : {});
      } else {
        if (id === "statements" && draft.baseCurrency) draft.baseCurrency = String(draft.baseCurrency).toUpperCase();
        await saveNix(nixPatch(id));
        const secrets: SecretsPatch = {};
        if (id === "server" && secretsDraft.syncApiKey.trim()) secrets.syncApiKey = secretsDraft.syncApiKey.trim();
        if (id === "trading212") {
          if (secretsDraft.t212Clear) {
            if (secretsDraft.t212Key.trim() || secretsDraft.t212Secret.trim())
              throw new Error("Clear the credentials or enter new ones, not both.");
            secrets.trading212Key = null;
            secrets.trading212Secret = null;
          } else {
            if (secretsDraft.t212Key.trim()) secrets.trading212Key = secretsDraft.t212Key.trim();
            if (secretsDraft.t212Secret.trim()) secrets.trading212Secret = secretsDraft.t212Secret.trim();
          }
        }
        await saveSecretPatch(secrets);
      }
      // Everything this section showed is now what the files say.
      for (const key of NIX_KEYS[id]) (baseline as Record<string, unknown>)[key] = clone(draft[key]);
      if (id === "models") {
        baseline.models = modelsFromRows();
        draft.models = modelsFromRows();
        baseline.tasks = clone(draft.tasks);
        for (const m of models) {
          m.isNew = false;
          m.apiKey = "";
        }
        removedModels = [];
      }
      if (id === "server") secretsDraft.syncApiKey = "";
      if (id === "trading212") Object.assign(secretsDraft, { t212Key: "", t212Secret: "", t212Clear: false });
      say(id, true, data!.rebuildCommand && pendingKeys().length ? "Saved. Rebuild to apply." : "Saved.");
    } catch (e) {
      say(id, false, e instanceof Error ? e.message : String(e));
    } finally {
      state.busy = false;
      render();
    }
  };

  const revert = (id: SectionId): void => {
    for (const key of NIX_KEYS[id]) (draft as Record<string, unknown>)[key] = clone(baseline[key]);
    if (id === "models") {
      models = rowsFrom(baseline.models);
      removedModels = [];
    }
    if (id === "server") secretsDraft.syncApiKey = "";
    if (id === "trading212") Object.assign(secretsDraft, { t212Key: "", t212Secret: "", t212Clear: false });
    sections[id].message = null;
    render();
  };

  const testModel = async (index: number): Promise<void> => {
    const m = models[index];
    if (!m) return;
    m.test = { busy: true };
    render();
    try {
      const res = await api.testModel({ id: m.id.trim(), provider: m.provider, baseUrl: m.baseUrl, model: m.model.trim(), apiKey: m.apiKey.trim() || undefined });
      if (!res.success || !res.data) throw new Error(res.error ?? "Test failed");
      m.test = res.data.ok
        ? { busy: false, ok: true, text: `Answered in ${res.data.latencyMs} ms` }
        : { busy: false, ok: false, text: res.data.error ?? "No usable answer" };
    } catch (e) {
      m.test = { busy: false, ok: false, text: e instanceof Error ? e.message : String(e) };
    }
    render();
  };

  // ---- Events ----

  const onInput = (event: Event): void => {
    const target = event.target as HTMLInputElement | HTMLSelectElement;
    // Text boxes report on `input`, so typing updates the draft at once;
    // selects and checkboxes on `change`. Never both, or a pick lands twice.
    const discrete = target instanceof HTMLSelectElement || (target instanceof HTMLInputElement && target.type === "checkbox");
    if (discrete !== (event.type === "change")) return;
    const value = target instanceof HTMLInputElement && target.type === "checkbox" ? target.checked : target.value;
    const nix = target.dataset.nix as keyof NixSettings | undefined;
    if (nix) {
      (draft as Record<string, unknown>)[nix] = nix === "port" ? (value === "" ? null : Number(value)) : value;
    } else if (target.dataset.feed) {
      const key = target.dataset.feed;
      draft.trading212 = { ...draft.trading212, [key]: key === "lookbackDays" ? Number(value) : value };
    } else if (target.dataset.secret) {
      const key = target.dataset.secret as keyof typeof secretsDraft;
      (secretsDraft as Record<string, unknown>)[key] = value;
    } else if (target.dataset.model !== undefined) {
      const m = models[Number(target.dataset.model)];
      const field = target.dataset.field!;
      if (field === "preset") {
        const preset = PRESETS.find((p) => p.id === value);
        if (preset) {
          m.provider = preset.provider;
          m.baseUrl = preset.baseUrl;
        } else {
          m.provider = "openai";
        }
        render();
        return;
      }
      (m as unknown as Record<string, unknown>)[field] = value;
      if (field === "vision") render();
    } else if (target.dataset.taskAdd && value) {
      const task = target.dataset.taskAdd as TaskName;
      draft.tasks = { ...draft.tasks, [task]: [...(draft.tasks?.[task] ?? []), String(value)] };
      render();
      return;
    }
    refreshChrome();
  };

  const onClick = (event: Event): void => {
    const target = (event.target as HTMLElement).closest<HTMLElement>("button");
    if (!target) return;
    const d = target.dataset;
    if (d.jump) {
      root.querySelector(`#settings-${d.jump}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
    } else if (d.save) {
      void save(d.save as SectionId);
    } else if (d.revert) {
      revert(d.revert as SectionId);
    } else if (d.task) {
      const task = d.task as TaskName;
      const ids = [...(draft.tasks?.[task] ?? [])];
      const i = Number(d.index ?? d.drop);
      if (d.drop !== undefined) ids.splice(Number(d.drop), 1);
      else {
        const j = i + Number(d.move);
        [ids[i], ids[j]] = [ids[j], ids[i]];
      }
      draft.tasks = { ...draft.tasks, [task]: ids };
      render();
    } else if (d.action === "add-model") {
      models.push({ id: "", provider: "gemini", baseUrl: PRESETS[0].baseUrl, model: "", vision: true, isNew: true, apiKey: "" });
      render();
      root.querySelector<HTMLInputElement>(`#m-id-${models.length - 1}`)?.focus();
    } else if (d.action === "remove-model") {
      const [gone] = models.splice(Number(d.index), 1);
      if (gone && !gone.isNew) removedModels.push(gone.id);
      render();
    } else if (d.action === "test-model") {
      void testModel(Number(d.index));
    } else if (d.action === "generate-key") {
      secretsDraft.syncApiKey = randomKey();
      render();
      const input = root.querySelector<HTMLInputElement>("#syncApiKey");
      if (input) input.type = "text";
    } else if (d.action === "copy-rebuild" && data?.rebuildCommand) {
      void navigator.clipboard.writeText(data.rebuildCommand);
      target.textContent = "Copied";
    } else if (d.action === "copy-restart") {
      void navigator.clipboard.writeText("sudo systemctl restart maia");
      target.textContent = "Copied";
    }
  };

  root.addEventListener("input", onInput);
  root.addEventListener("change", onInput);
  root.addEventListener("click", onClick);
  render();
  void load();
  const unsub = syncAtom.subscribe(() => {
    if (data) render();
  });
  return () => {
    unsub();
    root.removeEventListener("input", onInput);
    root.removeEventListener("change", onInput);
    root.removeEventListener("click", onClick);
  };
}

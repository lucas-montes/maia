import { api, type ModelSettings, type SecretFlags, type SecretsPatch, type SettingsData, type TaskName } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { syncAtom } from "../lib/stores";
import { panelClose, panelOpen } from "./shared";

/**
 * Settings.
 *
 * Two kinds of value, and every field says which it is:
 *  - **Nix**: everything that is not a secret. Set under `services.maia` in
 *    the host's NixOS configuration and only shown here, as running.
 *  - **Secret**: the sync key, model keys and Trading 212 credentials. Saved
 *    to the per-user overlay and only ever reported back as set / not set.
 *
 * Only the sections holding secrets have a Save button, and each saves on its own.
 */

type SectionId = "server" | "models" | "tasks" | "statements" | "trading212" | "storage";
type SecretSection = "server" | "models" | "trading212";

const SECTIONS: { id: SectionId; label: string }[] = [
  { id: "server", label: "Server" },
  { id: "models", label: "Models" },
  { id: "tasks", label: "Tasks" },
  { id: "statements", label: "Statements" },
  { id: "trading212", label: "Trading 212" },
  { id: "storage", label: "Storage" },
];

const SECRET_SECTIONS: SecretSection[] = ["server", "models", "trading212"];
const hasSecrets = (id: SectionId): id is SecretSection => (SECRET_SECTIONS as SectionId[]).includes(id);

const TASKS: { id: TaskName; label: string; hint: string }[] = [
  { id: "receiptOcr", label: "Receipt OCR", hint: "Reads receipt photos. Vision models only." },
  { id: "receiptMatching", label: "Receipt matching", hint: "Proposes the store, transaction and products for a receipt." },
  { id: "statementExtraction", label: "Statement extraction", hint: "Reads the activity table out of Firstrade PDF statements." },
];

const PROVIDER_LABELS: { provider: ModelSettings["provider"]; baseUrl: string; label: string }[] = [
  { provider: "gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta", label: "Gemini" },
  { provider: "openai", baseUrl: "https://api.openai.com/v1", label: "OpenAI" },
  { provider: "openai", baseUrl: "https://openrouter.ai/api/v1", label: "OpenRouter" },
  { provider: "openai", baseUrl: "http://localhost:11434/v1", label: "Ollama (local)" },
];

function providerLabel(model: ModelSettings): string {
  // Gemini configs saved before the Interactions API point at `…/v1beta/models`.
  let url = (model.baseUrl ?? "").replace(/\/+$/, "");
  if (model.provider === "gemini") url = url.replace(/\/models$/, "");
  return PROVIDER_LABELS.find((p) => p.provider === model.provider && p.baseUrl === url)?.label ?? "OpenAI-compatible";
}

interface SectionState {
  busy: boolean;
  message: { ok: boolean; text: string } | null;
}

function randomKey(): string {
  const bytes = new Uint8Array(24);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function mount(root: HTMLElement): () => void {
  let data: SettingsData | null = null;
  let loadError: string | null = null;
  const secretsDraft = { syncApiKey: "", t212Key: "", t212Secret: "", t212Clear: false };
  /** Model id → key typed but not saved. */
  let modelKeys: Record<string, string> = {};
  const tests: Record<string, { busy: boolean; ok?: boolean; text?: string }> = {};
  let restartNeeded = false;
  const sections: Record<SectionId, SectionState> = Object.fromEntries(
    SECTIONS.map((s) => [s.id, { busy: false, message: null }]),
  ) as Record<SectionId, SectionState>;

  const models = (): Record<string, ModelSettings> => (data?.effective.models ?? {}) as Record<string, ModelSettings>;

  const isDirty = (id: SectionId): boolean => {
    if (id === "server") return Boolean(secretsDraft.syncApiKey.trim());
    if (id === "models") return Object.values(modelKeys).some((k) => k.trim());
    if (id === "trading212")
      return Boolean(secretsDraft.t212Key.trim() || secretsDraft.t212Secret.trim()) || secretsDraft.t212Clear;
    return false;
  };

  // ---- Rendering helpers ----

  const chip = (kind: "nix" | "secret", option?: string) =>
    kind === "nix"
      ? `<span class="badge badge-info" title="Set in your NixOS configuration${option ? `: services.maia.${escapeHtml(option)}` : ""}">Nix</span>`
      : `<span class="badge badge-muted" title="Saved to ${escapeHtml(data?.overlayFile ?? "the overlay")}; never in Nix">secret</span>`;

  /** A running Nix value, shown read-only. */
  const nixValue = (label: string, value: unknown, option: string, hint = "", fallback = "module default") => {
    const shown = value === null || value === undefined || value === "" ? fallback : String(value);
    return `
    <div>
      <div class="field-label flex flex-wrap items-center gap-2">${escapeHtml(label)} ${chip("nix", option)}</div>
      <code class="block break-all rounded bg-surface-800 px-2 py-1 font-mono text-xs text-ink-300">${escapeHtml(shown)}</code>
      ${hint ? `<p class="mt-1 text-xs text-ink-500">${hint}</p>` : ""}
    </div>`;
  };

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
      ${hasSecrets(id) ? sectionFooter(id) : ""}
    </section>`;

  // ---- Sections ----

  const renderServer = (): string => {
    const sync = syncAtom.get();
    const listening = sync.info?.isListening ?? false;
    const e = data!.effective;
    const s = data!.secrets;
    return section(
      "server",
      "Server",
      `<span class="${listening ? "text-ok-400" : "text-err-400"}">●</span> ${listening ? "running" : "not reachable"}
       <span class="text-ink-500">· port ${sync.info?.port ?? "?"}</span>. The pairing QR code for the phone is under <strong>Sync</strong>.`,
      `<div class="grid gap-4 sm:grid-cols-2">
        ${nixValue("Port", e.port, "port")}
        ${nixValue("Firewall", e.openFirewall === null || e.openFirewall === undefined ? null : e.openFirewall ? "open on the LAN" : "closed", "openFirewall")}
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
    const rows = Object.entries(models())
      .map(([id, m]) => {
        const keySet = keys[id] ?? false;
        const label = providerLabel(m);
        const test = tests[id];
        return `<div class="rounded border border-surface-800 p-3">
          <div class="flex flex-wrap items-center gap-2 text-sm">
            <span class="font-mono text-ink-200">${escapeHtml(id)}</span>
            <span class="text-ink-500">·</span>
            <span class="text-ink-300">${escapeHtml(label)}</span>
            <span class="font-mono text-ink-300">${escapeHtml(m.model)}</span>
            ${m.vision ? `<span class="badge badge-muted">reads images</span>` : ""}
          </div>
          <div class="mt-1 break-all font-mono text-xs text-ink-500">${escapeHtml(m.baseUrl ?? "")}</div>
          <div class="mt-3 grid gap-3 md:grid-cols-[2fr_auto] md:items-end">
            <div>
              <label class="field-label flex items-center gap-2" for="m-key-${escapeHtml(id)}">API key ${chip("secret")}
                ${keySet ? `<span class="badge badge-ok">set</span>` : `<span class="badge badge-muted">${label === "Ollama (local)" ? "none needed" : "not set"}</span>`}</label>
              <input type="password" id="m-key-${escapeHtml(id)}" data-model-key="${escapeHtml(id)}" value="${escapeHtml(modelKeys[id] ?? "")}" autocomplete="off"
                placeholder="${keySet ? "•••••••• leave blank to keep" : "not set"}" class="input input-bordered input-sm w-full font-mono" />
            </div>
            <div class="flex flex-wrap items-center gap-2">
              <button class="btn btn-ghost btn-sm" type="button" data-action="test-model" data-id="${escapeHtml(id)}" ${test?.busy ? "disabled" : ""}>
                ${test?.busy ? "Testing..." : "Test connection"}</button>
              ${test && !test.busy ? `<span class="text-xs ${test.ok ? "text-ok-400" : "text-err-400"}">${escapeHtml(test.text ?? "")}</span>` : ""}
            </div>
          </div>
        </div>`;
      })
      .join("");
    return section(
      "models",
      "Models",
      `Endpoints the server can call: Gemini, or anything OpenAI-compatible (OpenAI, OpenRouter, Ollama…).
       They are declared in <code>services.maia.models</code> ${chip("nix", "models")}; their keys are saved here ${chip("secret")}.`,
      rows || `<p class="text-xs text-ink-500">No models. Declare them in <code>services.maia.models</code>.</p>`,
    );
  };

  const renderTasks = (): string => {
    const saved = models();
    const tasks = data!.effective.tasks ?? {};
    const body = TASKS.map((task) => {
      const ids = tasks[task.id] ?? [];
      const list = ids
        .map(
          (id, i) => `<li class="flex items-center gap-2 rounded border border-surface-800 px-2 py-1 text-sm">
            <span class="w-5 text-xs text-ink-500">${i + 1}.</span>
            <span class="flex-1 font-mono">${escapeHtml(id)}</span>
            ${saved[id] ? "" : `<span class="badge badge-err">not declared</span>`}
          </li>`,
        )
        .join("");
      return `<div>
        <div class="field-label">${escapeHtml(task.label)}</div>
        <p class="mb-2 text-xs text-ink-500">${escapeHtml(task.hint)} ${ids.length ? "Tried in this order until one answers." : "Off: no model assigned."}</p>
        <ol class="space-y-1">${list}</ol>
      </div>`;
    }).join("");
    return section(
      "tasks",
      "Tasks",
      `Which models each feature uses, as a fallback order. The review screens show which model produced each result. ${chip("nix", "tasks")}`,
      body,
    );
  };

  const renderStatements = (): string => {
    const e = data!.effective;
    return section(
      "statements",
      "Statements",
      "Where bank and broker exports are picked up, and the currency imports are booked in.",
      `${nixValue("Statements folder", e.statementsRoot, "statementsRoot", "Holds the <code>dump/</code>, <code>staged/</code>, <code>parsed/</code>, <code>discarded/</code> and <code>failed/</code> trees.")}
       ${nixValue("Intake folder", e.statementsIntakeDir, "statementsIntakeDir", "Name an export for its source (<code>firstrade-2026-11.csv</code>, <code>t212_feb_aug.csv</code>) and drop it here. Only recognised files are moved out, so this can be <code>~/Downloads</code>.", "off")}
       ${nixValue("Base currency", e.baseCurrency, "baseCurrency")}`,
    );
  };

  const renderTrading212 = (): string => {
    const feed = data!.effective.trading212 ?? {};
    const s = data!.secrets;
    return section(
      "trading212",
      "Trading 212",
      "Polls the Trading 212 history API. The feed is configured in Nix and the credentials are saved here.",
      `<div class="grid gap-4 sm:grid-cols-3">
        ${nixValue("Enabled", feed.enable ? "yes" : "no", "trading212.enable")}
        ${nixValue("Environment", feed.environment, "trading212.environment")}
        ${nixValue("Lookback days", feed.lookbackDays, "trading212.lookbackDays")}
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
    return section(
      "storage",
      "Storage",
      "Service state lives in the data directory, which systemd owns.",
      `${nixValue("Data directory", e.dataDir, "dataDir", "Holds the database.")}
       ${nixValue("Database", e.database, "dataDir")}
       ${nixValue("Exercise media", e.mediaDir, "mediaDir", "Read-only to the service, so it can point into a checkout.")}
       ${nixValue("Notes", e.notesDir, "notesDir", "Where notes are mirrored as Markdown.")}
       ${nixValue("Receipt archive", e.archiveDir, "archiveDir", "Where archived receipt images are moved.")}
       ${nixValue("Exercise catalogue seed", e.seedDir, "seedDir", "Only used when the exercises table is empty.", "off")}
       <div>
         <div class="field-label flex items-center gap-2">Secrets overlay ${chip("secret")}</div>
         <code class="block break-all rounded bg-surface-800 px-2 py-1 font-mono text-xs text-ink-300">${escapeHtml(data!.overlayFile)}</code>
       </div>`,
    );
  };

  const renderBanner = (): string => {
    if (!data) return "";
    const parts = [
      `<div class="banner">Values marked <strong>Nix</strong> are set under <code>services.maia</code> in your NixOS configuration and apply on the next <code>nixos-rebuild switch</code>. Only secrets are saved from here.</div>`,
    ];
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
    } catch (e) {
      loadError = `Could not read settings: ${e instanceof Error ? e.message : e}`;
    }
    render();
  };

  const clearDraft = (id: SectionId): void => {
    if (id === "server") secretsDraft.syncApiKey = "";
    if (id === "models") modelKeys = {};
    if (id === "trading212") Object.assign(secretsDraft, { t212Key: "", t212Secret: "", t212Clear: false });
  };

  const save = async (id: SectionId): Promise<void> => {
    const state = sections[id];
    state.busy = true;
    state.message = null;
    render();
    try {
      const patch: SecretsPatch = {};
      if (id === "server" && secretsDraft.syncApiKey.trim()) patch.syncApiKey = secretsDraft.syncApiKey.trim();
      if (id === "models") {
        const keys: Record<string, string> = {};
        for (const [model, key] of Object.entries(modelKeys)) if (key.trim()) keys[model] = key.trim();
        if (Object.keys(keys).length) patch.models = keys;
      }
      if (id === "trading212") {
        if (secretsDraft.t212Clear) {
          if (secretsDraft.t212Key.trim() || secretsDraft.t212Secret.trim())
            throw new Error("Clear the credentials or enter new ones, not both.");
          patch.trading212Key = null;
          patch.trading212Secret = null;
        } else {
          if (secretsDraft.t212Key.trim()) patch.trading212Key = secretsDraft.t212Key.trim();
          if (secretsDraft.t212Secret.trim()) patch.trading212Secret = secretsDraft.t212Secret.trim();
        }
      }
      if (Object.keys(patch).length) {
        const res = await api.saveSecrets(patch);
        if (!res.success || !res.data) throw new Error(res.error ?? "Save failed");
        data!.secrets = res.data as SecretFlags;
        restartNeeded = true;
      }
      clearDraft(id);
      state.message = { ok: true, text: "Saved." };
    } catch (e) {
      state.message = { ok: false, text: e instanceof Error ? e.message : String(e) };
    } finally {
      state.busy = false;
      render();
    }
  };

  const testModel = async (id: string): Promise<void> => {
    const m = models()[id];
    if (!m) return;
    tests[id] = { busy: true };
    render();
    try {
      const res = await api.testModel({ id, provider: m.provider, baseUrl: m.baseUrl, model: m.model, apiKey: modelKeys[id]?.trim() || undefined });
      if (!res.success || !res.data) throw new Error(res.error ?? "Test failed");
      tests[id] = res.data.ok
        ? { busy: false, ok: true, text: `Answered in ${res.data.latencyMs} ms` }
        : { busy: false, ok: false, text: res.data.error ?? "No usable answer" };
    } catch (e) {
      tests[id] = { busy: false, ok: false, text: e instanceof Error ? e.message : String(e) };
    }
    render();
  };

  // ---- Events ----

  const onInput = (event: Event): void => {
    const target = event.target as HTMLInputElement;
    // Text boxes report on `input`, checkboxes on `change`. Never both.
    if ((target.type === "checkbox") !== (event.type === "change")) return;
    const value = target.type === "checkbox" ? target.checked : target.value;
    if (target.dataset.secret) {
      const key = target.dataset.secret as keyof typeof secretsDraft;
      (secretsDraft as Record<string, unknown>)[key] = value;
    } else if (target.dataset.modelKey !== undefined) {
      modelKeys[target.dataset.modelKey] = String(value);
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
      clearDraft(d.revert as SectionId);
      sections[d.revert as SectionId].message = null;
      render();
    } else if (d.action === "test-model" && d.id) {
      void testModel(d.id);
    } else if (d.action === "generate-key") {
      secretsDraft.syncApiKey = randomKey();
      render();
      const input = root.querySelector<HTMLInputElement>("#syncApiKey");
      if (input) input.type = "text";
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

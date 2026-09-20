import "./styles/tailwind.css";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import Chart from "chart.js/auto";
import QRCode from "qrcode";

type ViewId = "dashboard" | "exercises" | "workouts" | "templates" | "ingredients" | "stores" | "meals" | "bodyMetrics" | "experiments" | "todos" | "receipts" | "notes" | "settings" | "goals" | "urls" | "fitfat" | "tags" | "accounts" | "transactions" | "fxRates";

interface Note {
  id: number;
  title: string;
  path: string;
  tags: string;
  created_at: string;
  updated_at: string;
}

interface NoteWithContent extends Note {
  content: string;
}

interface Receipt {
  id: number;
  receipt_id: string | null;
  original_path: string;
  current_path: string;
  archived_path: string | null;
  status: string | null;
  parsed_json_path: string | null;
  merchant: string | null;
  total: string | null;
  date: string | null;
  category: string | null;
  tags: string | null;
  checksum: string | null;
  created_at: string;
  processed_at: string | null;
}

interface SavedUrl {
  id: number;
  title: string | null;
  url: string;
  source: string | null;
  tags: string;
  is_new: number;
  created_at: string;
}

interface Todo {
  id: number;
  goal_id: number | null;
  title: string;
  description: string | null;
  due_date: string | null;
  priority: string | null;
  tags: string;
  order_index: number;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}

interface Goal {
  id: number;
  title: string;
  description: string | null;
  status: string | null;
  deadline: string | null;
  progress: number;
  created_at: string;
  updated_at: string;
}

interface DashboardCounts {
  tasks: number;
  goals: number;
  receipts: number;
  notes: number;
  urls: number;
}

interface NotesState {
  notes: Note[];
  selectedId: number | null;
  editTitle: string;
  editContent: string;
  editTags: string;
  status: "idle" | "loading" | "saving" | "error";
  statusMessage: string;
}

interface ReceiptsState {
  receipts: Receipt[];
  selectedId: number | null;
  status: "idle" | "loading" | "error";
  statusMessage: string;
}

interface TodosState {
  items: Todo[];
  filter: "all" | "active" | "completed";
  status: "idle" | "loading" | "saving" | "error";
  statusMessage: string;
}

interface GoalPendingTask {
  title: string;
  description: string;
}

interface GoalsState {
  items: Goal[];
  pendingTasks: GoalPendingTask[];
  status: "idle" | "loading" | "saving" | "error";
  statusMessage: string;
}

interface DashboardState {
  counts: DashboardCounts | null;
  status: "idle" | "loading";
}

interface UrlsState {
  items: SavedUrl[];
  status: "idle" | "loading" | "saving" | "error";
  statusMessage: string;
}

interface FitFatState {
  status: "idle" | "loading" | "error";
  statusMessage: string;
  counts: { workouts: number; meals: number; bodyMetrics: number; exercises: number } | null;
  workouts: any[];
  meals: any[];
  bodyMetrics: any[];
  exercises: any[];
  filters: { workoutsSince: string; mealsSince: string; bodySince: string };
  exerciseSearch: string;
  selectedExerciseId: string | null;
  syncUrl: string;
  lanUrl: string;
  apiKey: string;
  qrDataUrl: string | null;
  serverHealth: "checking" | "online" | "offline";
  lastHealthCheck: string;
  diagnostics: any | null;
  logs: string[];
}

interface ExercisesState { items: any[]; filtered: any[]; search: string; bodyPart: string; equipment: string; hasImage: string; selectedId: string | null; status: "idle"|"loading"|"error"; statusMessage: string; }
interface IngredientsState { items: any[]; search: string; selectedId: string | null; status: "idle"|"loading"|"error"; statusMessage: string; showAddModal: boolean; }
interface StoresState { items: any[]; search: string; status: "idle"|"loading"|"error"; statusMessage: string; }
interface MealsState { items: any[]; search: string; status: "idle"|"loading"|"error"; statusMessage: string; }
interface BodyMetricsState { items: any[]; status: "idle"|"loading"|"error"; statusMessage: string; }
interface ExperimentsState { items: any[]; selectedId: string | null; status: "idle"|"loading"|"error"; statusMessage: string; }
interface TagsState { items: any[]; status: "idle"|"loading"|"error"; statusMessage: string; }
interface AccountsState { items: any[]; status: "idle"|"loading"|"error"; statusMessage: string; }
interface TransactionsState { items: any[]; search: string; status: "idle"|"loading"|"error"; statusMessage: string; }
interface FxRatesState { items: any[]; base: string; date: string; status: "idle"|"loading"|"error"; statusMessage: string; }

interface AppState {
  view: ViewId;
  sidebarCollapsed: boolean;
  sidebarGroups: Record<string, boolean>;
  settings: {
    configJson: string;
    configFields: Record<string, string>;
    status: "idle" | "loading" | "saving" | "saved" | "error";
    statusMessage: string;
  };
  notes: NotesState;
  receipts: ReceiptsState;
  todos: TodosState;
  goals: GoalsState;
  dashboard: DashboardState;
  urls: UrlsState;
  fitfat: FitFatState;
  exercises: ExercisesState;
  ingredients: IngredientsState;
  stores: StoresState;
  meals: MealsState;
  bodyMetrics: BodyMetricsState;
  experiments: ExperimentsState;
  tags: TagsState;
  accounts: AccountsState;
  transactions: TransactionsState;
  fxRates: FxRatesState;
}

// ---- Config field definitions (must be before defaultAppState) ----

const CONFIG_FIELD_DEFS: { key: string; label: string; placeholder: string }[] = [
  { key: "database", label: "Database File", placeholder: "maia.db" },
  { key: "receipts_path", label: "Receipts Path", placeholder: "receipts" },
  { key: "nutriments_path", label: "Nutriments Path", placeholder: "" },
  { key: "bank_statements_path", label: "Bank Statements Path", placeholder: "" },
  { key: "investments_statements_path", label: "Investments Statements Path", placeholder: "" },
  { key: "model_api", label: "Model API Key", placeholder: "e.g. $GEMINI_API" },
  { key: "receipts.archived_dir", label: "Receipts Archive Directory", placeholder: "receipts_archive" },
];

function getDefaultConfigFields(): Record<string, string> {
  const fields: Record<string, string> = {};
  for (const def of CONFIG_FIELD_DEFS) {
    fields[def.key] = "";
  }
  return fields;
}

function extractConfigFields(jsonStr: string): Record<string, string> {
  const fields = getDefaultConfigFields();
  try {
    const obj = JSON.parse(jsonStr);
    for (const key of Object.keys(fields)) {
      const parts = key.split(".");
      let val: unknown = obj;
      for (const part of parts) {
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
    const val = fields[def.key]?.trim() || "";
    const parts = def.key.split(".");
    if (parts.length === 1) {
      // Nullable fields: empty string → null
      if (val === "" && (def.key === "bank_statements_path" || def.key === "investments_statements_path")) {
        obj[def.key] = null;
      } else {
        obj[def.key] = val;
      }
    } else if (parts.length === 2) {
      // Nested field: e.g. receipts.archived_dir → { receipts: { archived_dir: val } }
      if (!obj[parts[0]]) {
        obj[parts[0]] = {};
      }
      (obj[parts[0]] as Record<string, unknown>)[parts[1]] = val || null;
    }
  }
  return JSON.stringify(obj, null, 2);
}

const defaultAppState = (): AppState => ({
  view: "dashboard",
  sidebarCollapsed: false,
  sidebarGroups: { Training: true, Nutrition: true, Health: true, Planning: true, Finance: true },
  settings: {
    configJson: "",
    configFields: getDefaultConfigFields(),
    status: "idle",
    statusMessage: "",
  },
  notes: {
    notes: [],
    selectedId: null,
    editTitle: "",
    editContent: "",
    editTags: "",
    status: "idle",
    statusMessage: "",
  },
  receipts: {
    receipts: [],
    selectedId: null,
    status: "idle",
    statusMessage: "",
  },
  todos: {
    items: [],
    filter: "all",
    status: "idle",
    statusMessage: "",
  },
  goals: {
    items: [],
    pendingTasks: [],
    status: "idle",
    statusMessage: "",
  },
  dashboard: {
    counts: null,
    status: "idle",
  },
  urls: {
    items: [],
    status: "idle",
    statusMessage: "",
  },
  fitfat: {
    status: "idle",
    statusMessage: "",
    counts: null,
    workouts: [],
    meals: [],
    bodyMetrics: [],
    exercises: [],
    filters: { workoutsSince: "", mealsSince: "", bodySince: "" },
    exerciseSearch: "",
    selectedExerciseId: null,
    syncUrl: "",
    lanUrl: "",
    apiKey: "",
    qrDataUrl: null,
    serverHealth: "checking",
    lastHealthCheck: "",
    diagnostics: null,
    logs: [],
  },
  exercises: { items: [], filtered: [], search: "", bodyPart: "", equipment: "", hasImage: "", selectedId: null, status: "idle", statusMessage: "" },
  ingredients: { items: [], search: "", selectedId: null, status: "idle", statusMessage: "", showAddModal: false },
  stores: { items: [], search: "", status: "idle", statusMessage: "" },
  meals: { items: [], search: "", status: "idle", statusMessage: "" },
  bodyMetrics: { items: [], status: "idle", statusMessage: "" },
  experiments: { items: [], selectedId: null, status: "idle", statusMessage: "" },
  tags: { items: [], status: "idle", statusMessage: "" },
  accounts: { items: [], status: "idle", statusMessage: "" },
  transactions: { items: [], search: "", status: "idle", statusMessage: "" },
  fxRates: { items: [], base: "USD", date: new Date().toISOString().slice(0,10), status: "idle", statusMessage: "" },
});

let state: AppState = defaultAppState();
let fitfatCharts: Record<string, Chart> = {};

function setView(view: ViewId): void {
  if (fitfatHealthInterval) { clearInterval(fitfatHealthInterval); fitfatHealthInterval = null; }
  state.view = view;
  if (view === "fitfat") {
    loadFitFat();
    checkServerHealth();
    loadDiagnostics();
    fitfatHealthInterval = window.setInterval(() => { checkServerHealth(); loadDiagnostics(); }, 10000);
  }
  if (view === "settings") {
    loadConfig();
    if (!state.fitfat.lanUrl) loadFitFat();
  } else if (view === "notes") loadNotes();
  else if (view === "receipts") loadReceipts();
  else if (view === "todos") { loadTodos(); loadGoals(); }
  else if (view === "goals") { loadGoals(); loadTodos(); }
  else if (view === "urls") loadUrls();
  else if (view === "dashboard") loadDashboardCounts();
  else if (view === "exercises") loadExercises();
  else if (view === "workouts" || view === "templates") loadFitFat();
  else if (view === "ingredients") loadIngredients();
  else if (view === "stores") loadStores();
  else if (view === "meals") loadMeals();
  else if (view === "bodyMetrics") loadBodyMetrics();
  else if (view === "experiments") loadExperiments();
  else if (view === "tags") loadTags();
  else if (view === "accounts") loadAccounts();
  else if (view === "transactions") loadTransactions();
  else if (view === "fxRates") loadFxRates();
  render();
}

function toggleSidebar(): void {
  state.sidebarCollapsed = !state.sidebarCollapsed;
  render();
}
function toggleGroup(name: string): void {
  state.sidebarGroups[name] = !(state.sidebarGroups[name] ?? true);
  render();
}

async function loadConfig(): Promise<void> {
  state.settings.status = "loading";
  state.settings.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: string; error?: string }>("read_config");
    if (result.success && result.data) {
      const parsed = JSON.parse(result.data);
      state.settings.configJson = JSON.stringify(parsed, null, 2);
      state.settings.configFields = extractConfigFields(result.data);
      state.settings.status = "idle";
    } else {
      state.settings.configFields = getDefaultConfigFields();
      state.settings.configJson = buildConfigJson(state.settings.configFields);
      state.settings.status = "idle";
      state.settings.statusMessage = "Loaded default configuration (maia.json not found).";
    }
  } catch (e) {
    state.settings.status = "error";
    state.settings.statusMessage = `Failed to load config: ${e}`;
  }
  render();
}

async function saveConfigFromForm(): Promise<void> {
  // Read field values from the form
  const fields: Record<string, string> = {};
  for (const def of CONFIG_FIELD_DEFS) {
    const input = document.getElementById(`cfg-${def.key}`) as HTMLInputElement | null;
    fields[def.key] = input?.value ?? "";
  }
  state.settings.configFields = fields;

  const json = buildConfigJson(fields);
  state.settings.status = "saving";
  state.settings.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; error?: string }>("save_config", { json });
    if (result.success) {
      state.settings.status = "saved";
      state.settings.statusMessage = "Settings saved successfully.";
      state.settings.configJson = json;
    } else {
      state.settings.status = "error";
      state.settings.statusMessage = result.error ?? "Failed to save config";
    }
  } catch (e) {
    state.settings.status = "error";
    state.settings.statusMessage = `Failed to save config: ${e}`;
  }
  render();
}

// ---- Notes functions ----

async function loadNotes(): Promise<void> {
  state.notes.status = "loading";
  state.notes.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Note[]; error?: string }>("list_notes");
    if (result.success && result.data) {
      state.notes.notes = result.data;
      // If a note was selected, refresh selection
      if (state.notes.selectedId) {
        const stillExists = result.data.find((n) => n.id === state.notes.selectedId);
        if (!stillExists) {
          state.notes.selectedId = null;
        }
      }
      state.notes.status = "idle";
    } else {
      state.notes.status = "error";
      state.notes.statusMessage = result.error ?? "Failed to load notes";
    }
  } catch (e) {
    state.notes.status = "error";
    state.notes.statusMessage = `Failed to load notes: ${e}`;
  }
  render();
}

async function selectNote(id: number): Promise<void> {
  state.notes.status = "loading";
  state.notes.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: NoteWithContent; error?: string }>(
      "read_note",
      { id },
    );
    if (result.success && result.data) {
      state.notes.selectedId = result.data.id;
      state.notes.editTitle = result.data.title;
      state.notes.editContent = result.data.content;
      try {
        const tagsArr = JSON.parse(result.data.tags);
        state.notes.editTags = Array.isArray(tagsArr) ? tagsArr.join(", ") : "";
      } catch {
        state.notes.editTags = "";
      }
      state.notes.status = "idle";
    } else {
      state.notes.status = "error";
      state.notes.statusMessage = result.error ?? "Failed to read note";
    }
  } catch (e) {
    state.notes.status = "error";
    state.notes.statusMessage = `Failed to read note: ${e}`;
  }
  render();
}


// ---- Receipt functions ----

async function loadReceipts(): Promise<void> {
  state.receipts.status = "loading";
  state.receipts.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Receipt[]; error?: string }>("list_receipts");
    if (result.success && result.data) {
      state.receipts.receipts = result.data;
      if (state.receipts.selectedId) {
        const stillExists = result.data.find((r) => r.id === state.receipts.selectedId);
        if (!stillExists) {
          state.receipts.selectedId = null;
        }
      }
      state.receipts.status = "idle";
    } else {
      state.receipts.status = "error";
      state.receipts.statusMessage = result.error ?? "Failed to load receipts";
    }
  } catch (e) {
    state.receipts.status = "error";
    state.receipts.statusMessage = `Failed to load receipts: ${e}`;
  }
  render();
}

async function selectReceipt(id: number): Promise<void> {
  state.receipts.status = "loading";
  state.receipts.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Receipt; error?: string }>(
      "read_receipt",
      { id },
    );
    if (result.success && result.data) {
      state.receipts.selectedId = result.data.id;
      state.receipts.status = "idle";
    } else {
      state.receipts.status = "error";
      state.receipts.statusMessage = result.error ?? "Failed to read receipt";
    }
  } catch (e) {
    state.receipts.status = "error";
    state.receipts.statusMessage = `Failed to read receipt: ${e}`;
  }
  render();
}

// ---- Task functions ----

async function loadTodos(): Promise<void> {
  state.todos.status = "loading";
  state.todos.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Todo[]; error?: string }>("list_tasks");
    if (result.success && result.data) {
      state.todos.items = result.data;
      state.todos.status = "idle";
    } else {
      state.todos.status = "error";
      state.todos.statusMessage = result.error ?? "Failed to load tasks";
    }
  } catch (e) {
    state.todos.status = "error";
    state.todos.statusMessage = `Failed to load tasks: ${e}`;
  }
  render();
}




// ---- Goal functions ----

async function loadGoals(): Promise<void> {
  state.goals.status = "loading";
  state.goals.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Goal[]; error?: string }>("list_goals");
    if (result.success && result.data) {
      state.goals.items = result.data;
      state.goals.status = "idle";
    } else {
      state.goals.status = "error";
      state.goals.statusMessage = result.error ?? "Failed to load goals";
    }
  } catch (e) {
    state.goals.status = "error";
    state.goals.statusMessage = `Failed to load goals: ${e}`;
  }
  render();
}




// ---- Dashboard functions ----

async function loadDashboardCounts(): Promise<void> {
  state.dashboard.status = "loading";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: DashboardCounts; error?: string }>(
      "get_counts",
    );
    if (result.success && result.data) {
      state.dashboard.counts = result.data;
      state.dashboard.status = "idle";
    } else {
      state.dashboard.status = "idle";
    }
  } catch {
    state.dashboard.status = "idle";
  }
  render();
}

function dateStrToSince(dateStr: string): number {
  if (!dateStr) return 0;
  const d = new Date(dateStr);
  return isNaN(d.getTime()) ? 0 : d.getTime();
}

function resolveSyncUrl(raw: string): string {
  if (!raw) return "http://127.0.0.1:3030";
  try {
    const u = new URL(raw);
    if (u.hostname === "0.0.0.0") u.hostname = "127.0.0.1";
    return u.toString().replace(/\/$/, "");
  } catch {
    return raw.replace(/\/$/, "");
  }
}

async function fetchFitFat(path: string, since: number): Promise<any> {
  const url = resolveSyncUrl(state.fitfat.syncUrl);
  const sep = path.includes("?") ? "&" : "?";
  const full = `${url}${path}${sep}since=${since}`;
  const res = await fetch(full, { headers: { Authorization: `Bearer ${state.fitfat.apiKey}` } });
  if (!res.ok) throw new Error(`${path} ${res.status}`);
  return res.json();
}

async function loadFitFat(): Promise<void> {
  state.fitfat.status = "loading";
  state.fitfat.statusMessage = "";
  render();
  try {
    const [url, lanUrl, apiKey] = await Promise.all([
      invoke<string>("get_sync_url"),
      invoke<string>("get_sync_lan_url").catch(() => "http://127.0.0.1:3030"),
      invoke<string>("get_sync_api_key"),
    ]);
    state.fitfat.syncUrl = url || "http://127.0.0.1:3030";
    state.fitfat.lanUrl = lanUrl || state.fitfat.syncUrl;
    state.fitfat.apiKey = apiKey || "fitfat-sync-key";
    const workoutsSince = dateStrToSince(state.fitfat.filters.workoutsSince);
    const mealsSince = dateStrToSince(state.fitfat.filters.mealsSince);
    const bodySince = dateStrToSince(state.fitfat.filters.bodySince);
    const [workoutsRes, mealsRes, bodyRes, exercisesRes, templatesRes] = await Promise.all([
      fetchFitFat("/workouts", workoutsSince).catch(() => ({ workouts: [], workoutExercises: [], exerciseSets: [] })),
      fetchFitFat("/meals", mealsSince).catch(() => ({ meals: [], mealIngredients: [] })),
      fetchFitFat("/body-metrics", bodySince).catch(() => ({ items: [] })),
      fetchFitFat("/exercises", 0).catch(() => ({ items: [] })),
      fetchFitFat("/templates", 0).catch(() => ({ items: [] })),
    ]);
    state.fitfat.workouts = workoutsRes.workouts || [];
    (state.fitfat as any).workoutExercises = workoutsRes.workoutExercises || [];
    (state.fitfat as any).exerciseSets = workoutsRes.exerciseSets || [];
    state.fitfat.meals = mealsRes.meals || [];
    (state.fitfat as any).mealIngredients = mealsRes.mealIngredients || [];
    state.fitfat.bodyMetrics = bodyRes.items || bodyRes.bodyMetrics || [];
    state.fitfat.exercises = exercisesRes.items || [];
    (state as any).templates = { ...((state as any).templates||{}), items: templatesRes.items || [] };
    const wCount = state.fitfat.workouts.length;
    const mCount = state.fitfat.meals.length;
    const bCount = state.fitfat.bodyMetrics.length;
    const eCount = state.fitfat.exercises.length;
    state.fitfat.counts = { workouts: wCount, meals: mCount, bodyMetrics: bCount, exercises: eCount };
    state.fitfat.status = "idle";
    await generateFitFatQR();
  } catch (e) {
    state.fitfat.status = "error";
    state.fitfat.statusMessage = `Failed to load FitFat data: ${e}`;
  }
  render();
  requestAnimationFrame(renderFitFatCharts);
}

async function generateFitFatQR(): Promise<void> {
  const urlForQR = state.fitfat.lanUrl || resolveSyncUrl(state.fitfat.syncUrl);
  if (!state.fitfat.apiKey || !urlForQR) return;
  const payload = JSON.stringify({ url: urlForQR, apiKey: state.fitfat.apiKey, version: 1 });
  try {
    state.fitfat.qrDataUrl = await QRCode.toDataURL(payload, { width: 180, margin: 1 });
  } catch {
    state.fitfat.qrDataUrl = null;
  }
}

let fitfatHealthInterval: number | null = null;

async function checkServerHealth(): Promise<void> {
  const localUrl = "http://127.0.0.1:3030/health";
  const lanUrl = state.fitfat.lanUrl ? `${state.fitfat.lanUrl}/health` : null;
  try {
    const res = await fetch(localUrl);
    if (res.ok) {
      state.fitfat.serverHealth = "online";
      state.fitfat.lastHealthCheck = new Date().toLocaleTimeString();
      if (lanUrl) {
        try {
          const lanRes = await fetch(lanUrl);
          if (!lanRes.ok) state.fitfat.serverHealth = "online";
        } catch {}
      }
    } else {
      state.fitfat.serverHealth = "offline";
    }
  } catch {
    state.fitfat.serverHealth = "offline";
  }
  if (state.view === "fitfat") render();
}

async function loadDiagnostics(): Promise<void> {
  try {
    const [diag, logs] = await Promise.all([
      invoke<any>("get_server_diagnostics"),
      invoke<string[]>("get_server_logs", { limit: 50 }).catch(() => [] as string[]),
    ]);
    state.fitfat.diagnostics = diag;
    state.fitfat.logs = logs;
  } catch {
    state.fitfat.diagnostics = null;
    state.fitfat.logs = [];
  }
  if (state.view === "fitfat") render();
}

async function testFitFatConnection(): Promise<void> {
  state.fitfat.statusMessage = "Testing connection...";
  render();
  const url = resolveSyncUrl(state.fitfat.syncUrl);
  const lanUrl = state.fitfat.lanUrl;
  const results: string[] = [];
  for (const [name, base] of [["local", url], ["lan", lanUrl]] as const) {
    if (!base) continue;
    try {
      const h = await fetch(`${base}/health`);
      results.push(`${name} /health: ${h.ok ? "✓ " + h.status : "✗ " + h.status}`);
    } catch (e) {
      results.push(`${name} /health: ✗ unreachable (${e})`);
    }
    try {
      const r = await fetch(`${base}/exercises?since=0`, { headers: { Authorization: `Bearer ${state.fitfat.apiKey}` } });
      results.push(`${name} /exercises: ${r.ok ? "✓ " + r.status : "✗ " + r.status}`);
    } catch (e) {
      results.push(`${name} /exercises: ✗ unreachable (${e})`);
    }
  }
  try {
    const logs = await invoke<string[]>("get_server_logs", { limit: 5 });
    results.push(`logs: ${logs.length} entries`);
  } catch {}
  state.fitfat.statusMessage = results.join(" | ");
  render();
}

function renderFitFatCharts(): void {
  Object.values(fitfatCharts).forEach((c) => { try { c.destroy(); } catch {} });
  fitfatCharts = {};
  if (state.view !== "fitfat" || state.fitfat.status !== "idle") return;
  const workoutsCanvas = document.getElementById("fitfat-workouts-chart") as HTMLCanvasElement | null;
  if (workoutsCanvas) {
    const byWeek: Record<string, number> = {};
    for (const w of state.fitfat.workouts) {
      const d = new Date(w.date);
      const key = `${d.getFullYear()}-W${Math.ceil(((d.getTime() - new Date(d.getFullYear(), 0, 1).getTime()) / 86400000 + new Date(d.getFullYear(), 0, 1).getDay() + 1) / 7)}`;
      byWeek[key] = (byWeek[key] || 0) + 1;
    }
    const labels = Object.keys(byWeek).slice(-12);
    const data = labels.map((k) => byWeek[k]);
    fitfatCharts["workouts"] = new Chart(workoutsCanvas, {
      type: "bar",
      data: { labels, datasets: [{ label: "Workouts / week", data, backgroundColor: "rgba(14,165,233,0.6)", borderColor: "#0ea5e9", borderWidth: 1 }] },
      options: { responsive: true, plugins: { legend: { labels: { color: "#cbd5e1" } } }, scales: { x: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } }, y: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" }, beginAtZero: true } } },
    });
  }
  const volumeCanvas = document.getElementById("fitfat-volume-chart") as HTMLCanvasElement | null;
  if (volumeCanvas) {
    const sets: any[] = (state.fitfat as any).exerciseSets || [];
    const volByDate: Record<string, number> = {};
    for (const s of sets) {
      const w = state.fitfat.workouts.find((w: any) => (state.fitfat as any).workoutExercises?.some((we: any) => we.id === s.workoutExerciseId && we.workoutId === w.id));
      const d = w ? new Date(w.date).toISOString().slice(0, 10) : "unknown";
      volByDate[d] = (volByDate[d] || 0) + (s.weightKg || 0) * (s.reps || 0);
    }
    const labels = Object.keys(volByDate).sort().slice(-12);
    const data = labels.map((k) => volByDate[k]);
    fitfatCharts["volume"] = new Chart(volumeCanvas, {
      type: "line",
      data: { labels, datasets: [{ label: "Volume (kg·reps)", data, borderColor: "#10b981", backgroundColor: "rgba(16,185,129,0.2)", tension: 0.3, fill: true }] },
      options: { responsive: true, plugins: { legend: { labels: { color: "#cbd5e1" } } }, scales: { x: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } }, y: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } } } },
    });
  }
  const weightCanvas = document.getElementById("fitfat-weight-chart") as HTMLCanvasElement | null;
  if (weightCanvas) {
    const sorted = [...state.fitfat.bodyMetrics].sort((a: any, b: any) => a.date - b.date);
    const labels = sorted.map((b: any) => new Date(b.date).toISOString().slice(0, 10));
    const data = sorted.map((b: any) => b.weightKg);
    fitfatCharts["weight"] = new Chart(weightCanvas, {
      type: "line",
      data: { labels, datasets: [{ label: "Weight (kg)", data, borderColor: "#f59e0b", backgroundColor: "rgba(245,158,11,0.2)", tension: 0.3, fill: true }] },
      options: { responsive: true, plugins: { legend: { labels: { color: "#cbd5e1" } } }, scales: { x: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } }, y: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } } } },
    });
  }
  const mealsCanvas = document.getElementById("fitfat-meals-chart") as HTMLCanvasElement | null;
  if (mealsCanvas) {
    const calByDate: Record<string, number> = {};
    for (const m of state.fitfat.meals) {
      const d = new Date(m.eatenAt).toISOString().slice(0, 10);
      calByDate[d] = (calByDate[d] || 0) + 300;
    }
    const labels = Object.keys(calByDate).sort().slice(-12);
    const data = labels.map((k) => calByDate[k]);
    fitfatCharts["meals"] = new Chart(mealsCanvas, {
      type: "bar",
      data: { labels, datasets: [{ label: "Meals count", data, backgroundColor: "rgba(139,92,246,0.6)", borderColor: "#8b5cf6", borderWidth: 1 }] },
      options: { responsive: true, plugins: { legend: { labels: { color: "#cbd5e1" } } }, scales: { x: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } }, y: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" }, beginAtZero: true } } },
    });
  }
}

// ---- URL functions ----

async function loadUrls(): Promise<void> {
  state.urls.status = "loading";
  state.urls.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: SavedUrl[]; error?: string }>("list_urls");
    if (result.success && result.data) {
      state.urls.items = result.data;
      state.urls.status = "idle";
    } else {
      state.urls.status = "error";
      state.urls.statusMessage = result.error ?? "Failed to load URLs";
    }
  } catch (e) {
    state.urls.status = "error";
    state.urls.statusMessage = `Failed to load URLs: ${e}`;
  }
  render();
}

async function createUrl(): Promise<void> {
  const titleInput = document.getElementById("url-title") as HTMLInputElement | null;
  const urlInput = document.getElementById("url-url") as HTMLInputElement | null;
  const sourceSelect = document.getElementById("url-source") as HTMLSelectElement | null;
  const tagsInput = document.getElementById("url-tags") as HTMLInputElement | null;

  const url = urlInput?.value?.trim();
  if (!url) return;

  state.urls.status = "saving";
  render();

  const tagsArr = (tagsInput?.value || "")
    .split(",")
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
  const tagsJson = JSON.stringify(tagsArr);

  try {
    const result = await invoke<{ success: boolean; data?: SavedUrl; error?: string }>("create_url", {
      title: titleInput?.value?.trim() || null,
      url,
      source: sourceSelect?.value || "manual",
      tags: tagsJson,
    });
    if (result.success) {
      if (titleInput) titleInput.value = "";
      if (urlInput) urlInput.value = "";
      if (sourceSelect) sourceSelect.value = "manual";
      if (tagsInput) tagsInput.value = "";
      state.urls.status = "idle";
      await loadUrls();
    } else {
      state.urls.status = "error";
      state.urls.statusMessage = result.error ?? "Failed to create URL";
    }
  } catch (e) {
    state.urls.status = "error";
    state.urls.statusMessage = `Failed to create URL: ${e}`;
  }
  render();
}

async function deleteUrl(id: number): Promise<void> {
  state.urls.status = "saving";
  render();

  try {
    const result = await invoke<{ success: boolean; error?: string }>("delete_url", { id });
    if (result.success) {
      state.urls.status = "idle";
      await loadUrls();
    } else {
      state.urls.status = "error";
      state.urls.statusMessage = result.error ?? "Failed to delete URL";
    }
  } catch (e) {
    state.urls.status = "error";
    state.urls.statusMessage = `Failed to delete URL: ${e}`;
  }
  render();
}

async function markUrlRead(id: number): Promise<void> {
  try {
    await invoke<{ success: boolean }>("mark_url_read", { id });
    const item = state.urls.items.find((u) => u.id === id);
    if (item) item.is_new = 0;
    render();
  } catch {
    // silently fail
  }
}

async function openUrlExternal(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch {
    // silently fail
  }
}

function renderSidebar(): string {
  const groups: Array<{ name: string; items: Array<[ViewId, string, string]> }> = [
    { name: "Training", items: [["exercises","Exercises","💪"],["workouts","Workouts","🏋️"],["templates","Templates","📋"]] },
    { name: "Nutrition", items: [["ingredients","Ingredients","🥗"],["stores","Stores","🏪"],["meals","Meals","🍽️"]] },
    { name: "Health", items: [["bodyMetrics","Body","⚖️"],["experiments","Experiments","🧪"]] },
    { name: "Planning", items: [["todos","Tasks","✓"],["goals","Goals","★"],["notes","Notes","📝"],["urls","URLs","🔗"]] },
    { name: "Finance", items: [["accounts","Accounts","🏦"],["transactions","Transactions","💳"],["receipts","Receipts","🧾"]] },
    { name: "System", items: [["tags","Tags","🏷️"],["fxRates","FxRates","💱"],["fitfat","Sync","📊"],["dashboard","Dashboard","🎯"],["settings","Settings","⚙️"]] },
  ];
  const isActive = (v: ViewId) => state.view === v;
  const groupHtml = groups.map(g => {
    const open = state.sidebarGroups[g.name] ?? true;
    const header = state.sidebarCollapsed ? "" : `<button class="w-full flex items-center justify-between px-2 py-1 text-[0.62rem] uppercase tracking-widest text-ink-500 hover:text-ink-300" data-action="toggle-group" data-group="${g.name}"><span>${g.name}</span><span class="text-[0.6rem]">${open ? "▾" : "▸"}</span></button>`;
    const items = open ? g.items.map(([view,label,icon]) => `
      <button class="app-sidebar-nav-button ${isActive(view) ? "app-sidebar-nav-button-active" : "app-sidebar-nav-button-inactive"} ${state.sidebarCollapsed ? "app-sidebar-nav-button-collapsed" : ""}" type="button" data-view="${view}" aria-current="${isActive(view) ? "page" : "false"}" title="${escapeHtml(label)}" aria-label="${escapeHtml(label)}">
        <span class="app-sidebar-nav-icon">${escapeHtml(icon)}</span>
        <span class="truncate ${state.sidebarCollapsed ? "hidden" : "block"}">${escapeHtml(label)}</span>
      </button>`).join("") : "";
    return `<div class="mb-2">${header}${items}</div>`;
  }).join("");

  return `
    <aside class="app-sidebar ${state.sidebarCollapsed ? "app-sidebar-collapsed" : "app-sidebar-expanded"}" aria-label="Primary navigation">
      <button class="app-sidebar-toggle" type="button" data-action="toggle-sidebar" title="Toggle sidebar" aria-label="Toggle sidebar">
        ${state.sidebarCollapsed ? "▶" : "◀"}
      </button>
      <div class="app-sidebar-brand ${state.sidebarCollapsed ? "app-sidebar-brand-collapsed" : ""}">
        <div class="app-sidebar-brand-mark">M</div>
        <div class="min-w-0 ${state.sidebarCollapsed ? "hidden" : "block"}">
          <p class="m-0 text-[0.68rem] uppercase tracking-[0.12em] text-ink-400">Maia</p>
          <h1 class="m-0 text-sm font-semibold text-ink-100">Desktop</h1>
        </div>
      </div>
      <nav class="app-sidebar-nav overflow-auto">
        ${groupHtml}
      </nav>
      <div class="app-sidebar-footer ${state.sidebarCollapsed ? "hidden" : "block"}">
        <div class="flex flex-col gap-1">
          <span class="text-xs text-ink-400">${state.exercises.items.length} ex • ${state.ingredients.items.length} ing</span>
          <span class="text-xs text-ink-500">maia.db • ${state.fitfat.exercises.length} synced</span>
        </div>
      </div>
    </aside>
  `;
}

function renderContent(): string {
  switch (state.view) {
    case "dashboard": return renderDashboard();
    case "exercises": return renderExercises();
    case "workouts": return renderWorkouts();
    case "templates": return renderTemplates();
    case "ingredients": return renderIngredients();
    case "stores": return renderStores();
    case "meals": return renderMeals();
    case "bodyMetrics": return renderBodyMetrics();
    case "experiments": return renderExperiments();
    case "tags": return renderTags();
    case "accounts": return renderAccounts();
    case "transactions": return renderTransactions();
    case "fxRates": return renderFxRates();
    case "todos": return renderTodos();
    case "goals": return renderGoals();
    case "receipts": return renderReceipts();
    case "notes": return renderNotes();
    case "urls": return renderUrls();
    case "fitfat": return renderFitFat();
    case "settings": return renderSettings();
    default: return `<div class="p-6">Loading...</div>`;
  }
}

function render(): void {
  const appRoot = document.getElementById("app");
  if (!appRoot) {
    throw new Error("Maia app root is missing");
  }

  appRoot.innerHTML = `
    <div class="app-shell ${state.sidebarCollapsed ? "compact" : ""}">
      <div class="app-shell-grid">
        ${renderSidebar()}
        <main class="app-main">
          <div class="app-main-scroll">
            ${renderContent()}
          </div>
        </main>
      </div>
    </div>
  `;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/* Overlay roots live outside the re-rendered #app tree so toasts and modals
   survive render() calls. */
function ensureOverlayRoots(): void {
  if (!document.getElementById("toast-stack")) {
    const stack = document.createElement("div");
    stack.id = "toast-stack";
    stack.className = "toast-stack";
    stack.setAttribute("aria-live", "polite");
    document.body.appendChild(stack);
  }
  if (!document.getElementById("modal-root")) {
    const root = document.createElement("div");
    root.id = "modal-root";
    document.body.appendChild(root);
  }
}

function toast(message: string, kind: "ok" | "err" | "" = ""): void {
  ensureOverlayRoots();
  const stack = document.getElementById("toast-stack");
  if (!stack) return;
  const el = document.createElement("div");
  el.className = `toast${kind ? ` toast-${kind}` : ""}`;
  el.textContent = message;
  stack.appendChild(el);
  window.setTimeout(() => {
    el.remove();
  }, 4000);
}

function confirmDialog(title: string, body: string, confirmLabel = "Delete"): Promise<boolean> {
  ensureOverlayRoots();
  const root = document.getElementById("modal-root");
  if (!root) return Promise.resolve(confirm(`${title}\n${body}`));
  return new Promise((resolve) => {
    root.innerHTML = `<div class="modal-overlay modal-open"><div class="modal-dialog" role="alertdialog" aria-modal="true" aria-label="${escapeHtml(title)}"><div class="modal-title">${escapeHtml(title)}</div><p class="text-sm text-ink-400">${escapeHtml(body)}</p><div class="modal-actions"><button class="button secondary text-sm" data-x="cancel">Cancel</button><button class="button danger text-sm" data-x="confirm">${escapeHtml(confirmLabel)}</button></div></div></div>`;
    const done = (v: boolean) => {
      root.innerHTML = "";
      document.removeEventListener("keydown", onKey, true);
      resolve(v);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        done(false);
      }
    };
    document.addEventListener("keydown", onKey, true);
    root.querySelector<HTMLElement>("[data-x='cancel']")?.addEventListener("click", () => done(false));
    root.querySelector<HTMLElement>("[data-x='confirm']")?.addEventListener("click", () => done(true));
    root.querySelector<HTMLElement>(".modal-overlay")?.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).classList.contains("modal-overlay")) done(false);
    });
    root.querySelector<HTMLElement>("[data-x='cancel']")?.focus();
  });
}

function handleClick(event: MouseEvent): void {
  const target = event.target as HTMLElement;

  // Close open datepickers when clicking outside
  if (!target.closest(".datepicker-wrapper") && !target.closest("[data-action^='datepicker-']")) {
    closeAllDatepickers();
  }

  // ---- Datepicker handlers ----

  // Datepicker toggle (open/close calendar)
  const dpToggle = target.closest<HTMLElement>("[data-action='datepicker-toggle']");
  if (dpToggle) {
    const wrapper = dpToggle.closest<HTMLElement>(".datepicker-wrapper");
    if (wrapper) {
      const cal = wrapper.querySelector<HTMLElement>(".datepicker-calendar");
      if (cal) {
        cal.classList.toggle("hidden");
      }
    }
    event.stopPropagation();
    return;
  }

  // Datepicker day selection
  const dpDay = target.closest<HTMLElement>("[data-action='datepicker-day']");
  if (dpDay?.dataset.date) {
    const wrapper = dpDay.closest<HTMLElement>(".datepicker-wrapper");
    if (wrapper) {
      const input = wrapper.querySelector<HTMLInputElement>(".datepicker-input");
      if (input) {
        input.value = dpDay.dataset.date;
      }
      const cal = wrapper.querySelector<HTMLElement>(".datepicker-calendar");
      if (cal) {
        cal.classList.add("hidden");
      }
    }
    return;
  }

  // Datepicker month navigation
  const dpNav = target.closest<HTMLElement>("[data-action='datepicker-prev-month'], [data-action='datepicker-next-month']");
  if (dpNav) {
    const cal = dpNav.closest<HTMLElement>(".datepicker-calendar");
    if (cal) {
      const id = dpNav.dataset.datepickerId || "";
      let year = parseInt(cal.dataset.year || "0", 10);
      let month = parseInt(cal.dataset.month || "0", 10);
      if (dpNav.dataset.action === "datepicker-prev-month") {
        month -= 1;
        if (month < 0) { month = 11; year -= 1; }
      } else {
        month += 1;
        if (month > 11) { month = 0; year += 1; }
      }
      cal.dataset.year = String(year);
      cal.dataset.month = String(month);
      const wrapper = cal.closest<HTMLElement>(".datepicker-wrapper");
      const input = wrapper?.querySelector<HTMLInputElement>(".datepicker-input");
      cal.innerHTML = renderCalendarGrid(id, year, month, input?.value || "");
    }
    return;
  }

  const groupToggle = target.closest<HTMLElement>("[data-action='toggle-group']");
  if (groupToggle?.dataset.group) {
    toggleGroup(groupToggle.dataset.group);
    return;
  }
  const toggleButton = target.closest<HTMLElement>("[data-action='toggle-sidebar']");
  if (toggleButton) {
    toggleSidebar();
    return;
  }

  const viewButton = target.closest<HTMLElement>("[data-view]");
  if (viewButton?.dataset.view) {
    setView(viewButton.dataset.view as ViewId);
    return;
  }

  // Handle settings save (form-based)
  const saveButton = target.closest<HTMLElement>("[data-action='save-settings']");
  if (saveButton) {
    saveConfigFromForm();
    return;
  }

  // Handle settings reload
  const reloadButton = target.closest<HTMLElement>("[data-action='reload-settings']");
  if (reloadButton) {
    loadConfig();
    return;
  }

  // Handle notes list item click
  const noteItem = target.closest<HTMLElement>("[data-note-id]");
  if (noteItem?.dataset.noteId) {
    selectNote(Number(noteItem.dataset.noteId));
    return;
  }

  // Handle notes back to list
  const backToListButton = target.closest<HTMLElement>("[data-action='notes-back']");
  if (backToListButton) {
    state.notes.selectedId = null;
    render();
    return;
  }

  // Handle receipts list item click
  const receiptItem = target.closest<HTMLElement>("[data-receipt-id]");
  if (receiptItem?.dataset.receiptId) {
    selectReceipt(Number(receiptItem.dataset.receiptId));
    return;
  }

  // Handle receipts back to list
  const receiptBackButton = target.closest<HTMLElement>("[data-action='receipts-back']");
  if (receiptBackButton) {
    state.receipts.selectedId = null;
    render();
    return;
  }

  // ---- Task handlers (read-only) ----

  // Handle todo filter
  const filterButton = target.closest<HTMLElement>("[data-todo-filter]");
  if (filterButton?.dataset.todoFilter) {
    state.todos.filter = filterButton.dataset.todoFilter as "all" | "active" | "completed";
    render();
    return;
  }

  // ---- Goal handlers (read-only) ----

  // ---- URL handlers ----

  // Handle create URL
  const createUrlButton = target.closest<HTMLElement>("[data-action='create-url']");
  if (createUrlButton) {
    createUrl();
    return;
  }

  // Handle delete URL
  const deleteUrlButton = target.closest<HTMLElement>("[data-action='delete-url']");
  if (deleteUrlButton?.dataset.urlId) {
    deleteUrl(Number(deleteUrlButton.dataset.urlId));
    return;
  }

  // Handle mark URL read
  const markUrlReadButton = target.closest<HTMLElement>("[data-action='mark-url-read']");
  if (markUrlReadButton?.dataset.urlId) {
    markUrlRead(Number(markUrlReadButton.dataset.urlId));
    return;
  }

  // Handle open URL
  const openUrlButton = target.closest<HTMLElement>("[data-action='open-url']");
  if (openUrlButton?.dataset.urlId) {
    const id = Number(openUrlButton.dataset.urlId);
    const urlItem = state.urls.items.find((u) => u.id === id);
    if (urlItem) {
      openUrlExternal(urlItem.url);
      // Also mark as read when opened
      markUrlRead(id);
    }
    return;
  }

  // ---- FitFat handlers ----
  const fitfatRefresh = target.closest<HTMLElement>("[data-action='fitfat-refresh']");
  if (fitfatRefresh) {
    loadFitFat();
    return;
  }
  const fitfatApplyWorkouts = target.closest<HTMLElement>("[data-action='fitfat-apply-workouts']");
  if (fitfatApplyWorkouts) {
    const inputs = document.querySelectorAll<HTMLInputElement>("[data-fitfat-filter='workoutsSince']");
    if (inputs[0]) state.fitfat.filters.workoutsSince = inputs[0].value;
    loadFitFat();
    return;
  }
  const fitfatApplyBody = target.closest<HTMLElement>("[data-action='fitfat-apply-body']");
  if (fitfatApplyBody) {
    const inp = document.querySelector<HTMLInputElement>("[data-fitfat-filter='bodySince']");
    if (inp) state.fitfat.filters.bodySince = inp.value;
    loadFitFat();
    return;
  }
  const fitfatApplyMeals = target.closest<HTMLElement>("[data-action='fitfat-apply-meals']");
  if (fitfatApplyMeals) {
    const inp = document.querySelector<HTMLInputElement>("[data-fitfat-filter='mealsSince']");
    if (inp) state.fitfat.filters.mealsSince = inp.value;
    loadFitFat();
    return;
  }
  const copyQr = target.closest<HTMLElement>("[data-action='fitfat-copy-qr']");
  if (copyQr) {
    const urlForQR = state.fitfat.lanUrl || resolveSyncUrl(state.fitfat.syncUrl);
    const payload = JSON.stringify({ url: urlForQR, apiKey: state.fitfat.apiKey, version: 1 });
    navigator.clipboard.writeText(payload).catch(() => {});
    return;
  }
  const copyUrl = target.closest<HTMLElement>("[data-action='fitfat-copy-url']");
  if (copyUrl) {
    const urlForQR = state.fitfat.lanUrl || resolveSyncUrl(state.fitfat.syncUrl);
    navigator.clipboard.writeText(urlForQR).catch(() => {});
    return;
  }
  const copyKey = target.closest<HTMLElement>("[data-action='fitfat-copy-key']");
  if (copyKey) {
    navigator.clipboard.writeText(state.fitfat.apiKey).catch(() => {});
    return;
  }
  const testConn = target.closest<HTMLElement>("[data-action='fitfat-test-connection']");
  if (testConn) {
    testFitFatConnection();
    return;
  }
  const copyCurl = target.closest<HTMLElement>("[data-action='fitfat-copy-curl']");
  if (copyCurl) {
    const cmd = `curl -v -H "Authorization: Bearer ${state.fitfat.apiKey}" ${state.fitfat.lanUrl || resolveSyncUrl(state.fitfat.syncUrl)}/health`;
    navigator.clipboard.writeText(cmd).catch(() => {});
    return;
  }
  const exSelect = target.closest<HTMLElement>("[data-action='ex-select']");
  if (exSelect?.dataset.id) { state.exercises.selectedId = exSelect.dataset.id; render(); return; }
  const exApply = target.closest<HTMLElement>("[data-action='ex-apply']");
  if (exApply) {
    const s = (document.getElementById("ex-search") as HTMLInputElement)?.value ?? "";
    const bp = (document.getElementById("ex-bodypart") as HTMLSelectElement)?.value ?? "";
    const eq = (document.getElementById("ex-equip") as HTMLSelectElement)?.value ?? "";
    const hi = (document.getElementById("ex-hasimg") as HTMLSelectElement)?.value ?? "";
    state.exercises.search=s; state.exercises.bodyPart=bp; state.exercises.equipment=eq; state.exercises.hasImage=hi; applyExerciseFilter(); render(); return;
  }
  const exEdit = target.closest<HTMLElement>("[data-action='ex-edit']");
  if (exEdit?.dataset.id) { const id=exEdit.dataset.id; const ex=state.exercises.items.find((e:any)=>e.id===id); if(ex){ const name=prompt("New name",ex.name); if(name) invoke("update_exercise",{id, name, bodyPart:ex.body_part||ex.bodyPart, equipment:ex.equipment, primaryMuscle:ex.primary_muscle||ex.primaryMuscle, secondaryMuscle:ex.secondary_muscle||ex.secondaryMuscle}).then(()=>loadExercises()); } return; }
  const ingOpen = target.closest<HTMLElement>("[data-action='ing-open-modal']");
  if (ingOpen) { state.ingredients.showAddModal = true; render(); (document.getElementById("ing-new-name") as HTMLInputElement)?.focus(); return; }
  const ingClose = target.closest<HTMLElement>("[data-action='ing-close-modal']");
  if (ingClose) { state.ingredients.showAddModal = false; render(); return; }
  const ingCreate = target.closest<HTMLElement>("[data-action='ing-create']");
  if (ingCreate) {
    const name=(document.getElementById("ing-new-name") as HTMLInputElement)?.value.trim();
    const cal=parseFloat((document.getElementById("ing-new-cal") as HTMLInputElement)?.value||"");
    const prot=parseFloat((document.getElementById("ing-new-prot") as HTMLInputElement)?.value||"");
    const carb=parseFloat((document.getElementById("ing-new-carb") as HTMLInputElement)?.value||"");
    const fat=parseFloat((document.getElementById("ing-new-fat") as HTMLInputElement)?.value||"");
    const fiber=parseFloat((document.getElementById("ing-new-fiber") as HTMLInputElement)?.value||"");
    const sugar=parseFloat((document.getElementById("ing-new-sugar") as HTMLInputElement)?.value||"");
    const sodium=parseFloat((document.getElementById("ing-new-sodium") as HTMLInputElement)?.value||"");
    const brand=(document.getElementById("ing-new-brand") as HTMLInputElement)?.value.trim()||null;
    const barcode=(document.getElementById("ing-new-barcode") as HTMLInputElement)?.value.trim()||null;
    if(!name){ toast("Please enter a name", "err"); return; }
    if(isNaN(cal)){ toast("Please enter calories", "err"); return; }
    (ingCreate as HTMLButtonElement).disabled = true; (ingCreate as HTMLButtonElement).textContent = "Adding...";
    invoke("create_ingredient",{name, caloriesPer100g:cal, proteinPer100g:isNaN(prot)?0:prot, carbsPer100g:isNaN(carb)?0:carb, fatPer100g:isNaN(fat)?0:fat, fiberPer100g:isNaN(fiber)?null:fiber, sugarPer100g:isNaN(sugar)?null:sugar, sodiumPer100g:isNaN(sodium)?null:sodium, brand, barcode}).then(()=>{ state.ingredients.showAddModal=false; loadIngredients(); }).catch(()=>{ (ingCreate as HTMLButtonElement).disabled=false; (ingCreate as HTMLButtonElement).textContent="Add Ingredient →"; }); return;
  }
  const ingEdit = target.closest<HTMLElement>("[data-action='ing-edit']");
  if (ingEdit?.dataset.id) { const id=ingEdit.dataset.id; const it=state.ingredients.items.find((e:any)=>e.id===id); if(it){ const name=prompt("New name",it.name); if(name) invoke("update_ingredient",{id, name, caloriesPer100g:it.calories_per100g, proteinPer100g:it.protein_per100g, carbsPer100g:it.carbs_per100g, fatPer100g:it.fat_per100g}).then(()=>loadIngredients()); } return; }
  const tplCreate = target.closest<HTMLElement>("[data-action='tpl-create']");
  if (tplCreate) { const inp=document.getElementById("tpl-name") as HTMLInputElement; const name=inp?.value.trim(); if(name){ const id=crypto.randomUUID(); invoke<any>("create_template",{id, name}).then((res)=>{ if(res&&res.success===false){ toast(`Couldn't create template: ${res.error||"unknown error"}`, "err"); return; } if(inp) inp.value=""; loadFitFat(); }).catch((e)=>toast(`Couldn't create template: ${e}`, "err")); } return; }
  const tplEdit = target.closest<HTMLElement>("[data-action='tpl-edit']");
  if (tplEdit?.dataset.id) { (state as any).templates = { ...((state as any).templates||{}), editingId: tplEdit.dataset.id }; render(); return; }
  const tplCancel = target.closest<HTMLElement>("[data-action='tpl-cancel']");
  if (tplCancel) { (state as any).templates = { ...((state as any).templates||{}), editingId: null }; render(); return; }
  const tplSave = target.closest<HTMLElement>("[data-action='tpl-save']");
  if (tplSave?.dataset.id) { const id=tplSave.dataset.id; const name=(document.getElementById("tpl-edit-name") as HTMLInputElement)?.value.trim(); const notes=(document.getElementById("tpl-edit-notes") as HTMLInputElement)?.value.trim()||null; const recurrence=(document.getElementById("tpl-edit-recurrence") as HTMLInputElement)?.value.trim()||null; if(!name){ toast("Template name can't be empty", "err"); return; } invoke<any>("update_template",{id, name, notes, recurrence}).then((res)=>{ if(res&&res.success===false){ toast(`Couldn't save template: ${res.error||"unknown error"}`, "err"); return; } (state as any).templates = { ...((state as any).templates||{}), editingId: null }; loadFitFat(); }).catch((e)=>toast(`Couldn't save template: ${e}`, "err")); return; }
  const tplDelete = target.closest<HTMLElement>("[data-action='tpl-delete']");
  if (tplDelete?.dataset.id) { const id=tplDelete.dataset.id; const it=((state as any).templates?.items||[]).find((x:any)=>x.id===id); confirmDialog("Delete template?", `Delete "${it?.name||id}" and all its exercises and sets? This can't be undone.`).then((ok)=>{ if(!ok) return; invoke<any>("delete_template",{id}).then((res)=>{ if(res&&res.success===false){ toast(`Couldn't delete template: ${res.error||"unknown error"}`, "err"); return; } loadFitFat(); }).catch((e)=>toast(`Couldn't delete template: ${e}`, "err")); }); return; }
  const tplOpen = target.closest<HTMLElement>("[data-action='tpl-open']");
  if (tplOpen?.dataset.id) { (state as any).templates = { ...((state as any).templates||{}), editingId: null }; loadTemplateDetail(tplOpen.dataset.id); return; }
  const tplBack = target.closest<HTMLElement>("[data-action='tpl-back']");
  if (tplBack) { (state as any).templates = { ...((state as any).templates||{}), selectedId: null, detail: null, exSearch: "" }; loadFitFat(); return; }
  const tplExSearch = target.closest<HTMLElement>("[data-action='tpl-ex-search']");
  if (tplExSearch) { const v=(document.getElementById("tpl-ex-search") as HTMLInputElement)?.value??""; (state as any).templates = { ...((state as any).templates||{}), exSearch: v }; render(); return; }
  const tplAddEx = target.closest<HTMLElement>("[data-action='tpl-add-exercise']");
  if (tplAddEx?.dataset.id) { const templateId=(state as any).templates?.selectedId; if(!templateId) return; invoke<any>("add_template_exercise",{templateId, exerciseId:tplAddEx.dataset.id}).then(async (res)=>{ if(res&&res.success===false){ toast(`Couldn't add exercise: ${res.error||"unknown error"}`, "err"); return; } const blockId=res?.data?.id??res?.id; if(blockId){ await invoke<any>("add_template_set",{templateExerciseId:blockId, reps:null, weightKg:null, restSeconds:null, durationMinutes:null, distanceMeters:null}).catch(()=>null); } loadTemplateDetail(templateId); }).catch((e)=>toast(`Couldn't add exercise: ${e}`, "err")); return; }
  const tplRmEx = target.closest<HTMLElement>("[data-action='tpl-remove-exercise']");
  if (tplRmEx?.dataset.id) { const templateId=(state as any).templates?.selectedId; if(!templateId) return; confirmDialog("Remove exercise?", "Remove this exercise and all its sets from the template? This can't be undone.").then((ok)=>{ if(!ok) return; invoke<any>("delete_template_exercise",{id:tplRmEx.dataset.id}).then((res)=>{ if(res&&res.success===false){ toast(`Couldn't remove exercise: ${res.error||"unknown error"}`, "err"); return; } loadTemplateDetail(templateId); }).catch((e)=>toast(`Couldn't remove exercise: ${e}`, "err")); }); return; }
  const tplAddSet = target.closest<HTMLElement>("[data-action='tpl-add-set']");
  if (tplAddSet?.dataset.id) { const templateId=(state as any).templates?.selectedId; if(!templateId) return; const block=((state as any).templates?.detail?.exercises||[]).find((b:any)=>b.id===tplAddSet.dataset.id); const last=(block?.sets||[]).slice(-1)[0]; invoke<any>("add_template_set",{templateExerciseId:tplAddSet.dataset.id, reps:last?.reps??null, weightKg:last?.weightKg??null, restSeconds:last?.restSeconds??null, durationMinutes:last?.durationMinutes??null, distanceMeters:last?.distanceMeters??null}).then((res)=>{ if(res&&res.success===false){ toast(`Couldn't add set: ${res.error||"unknown error"}`, "err"); return; } loadTemplateDetail(templateId); }).catch((e)=>toast(`Couldn't add set: ${e}`, "err")); return; }
  const tplDelSet = target.closest<HTMLElement>("[data-action='tpl-delete-set']");
  if (tplDelSet?.dataset.id) { const templateId=(state as any).templates?.selectedId; if(!templateId) return; invoke<any>("delete_template_set",{id:tplDelSet.dataset.id}).then((res)=>{ if(res&&res.success===false){ toast(`Couldn't delete set: ${res.error||"unknown error"}`, "err"); return; } loadTemplateDetail(templateId); }).catch((e)=>toast(`Couldn't delete set: ${e}`, "err")); return; }
  const tplSaveBlock = target.closest<HTMLElement>("[data-action='tpl-save-block']");
  if (tplSaveBlock?.dataset.id) {
    const templateId=(state as any).templates?.selectedId; if(!templateId) return;
    const block=((state as any).templates?.detail?.exercises||[]).find((b:any)=>b.id===tplSaveBlock.dataset.id);
    if(!block) return;
    const notes=(document.getElementById(`tpl-exnotes-${block.id}`) as HTMLInputElement)?.value.trim()||null;
    (async () => {
      const r1=await invoke<any>("update_template_exercise",{id:block.id, notes});
      if(r1&&r1.success===false) throw new Error(r1.error||"unknown error");
      for (const s of (block.sets||[])) {
        const r=await invoke<any>("update_template_set",{id:s.id, reps:tplInt(tplVal(`ts-${s.id}-reps`)), weightKg:tplNum(tplVal(`ts-${s.id}-weight`)), restSeconds:tplInt(tplVal(`ts-${s.id}-rest`)), durationMinutes:tplInt(tplVal(`ts-${s.id}-dur`)), distanceMeters:tplNum(tplVal(`ts-${s.id}-dist`))});
        if(r&&r.success===false) throw new Error(r.error||"unknown error");
      }
      loadTemplateDetail(templateId);
    })().catch((e)=>toast(`Couldn't save: ${e}`, "err"));
    return;
  }
  const storeCreate = target.closest<HTMLElement>("[data-action='store-create']");
  if (storeCreate) { const inp=document.getElementById("store-name") as HTMLInputElement; const name=inp?.value.trim(); if(name){ invoke("create_store",{name}).then(()=>loadStores()); if(inp) inp.value=""; } return; }
  const fxAdd = target.closest<HTMLElement>("[data-action='fx-add']");
  if (fxAdd) { const code=(document.getElementById("fx-code") as HTMLInputElement)?.value.trim().toUpperCase(); const rate=parseFloat((document.getElementById("fx-rate") as HTMLInputElement)?.value||""); const base=(document.getElementById("fx-base") as HTMLInputElement)?.value.trim().toUpperCase()||"USD"; const date=(document.getElementById("fx-date") as HTMLInputElement)?.value||new Date().toISOString().slice(0,10); if(code&&rate) invoke("create_fx_rate",{code, baseCode:base, rateDate:date, rateToBase:rate}).then(()=>loadFxRates()); return; }
  const fxRefresh = target.closest<HTMLElement>("[data-action='fx-refresh']");
  if (fxRefresh) { const b=(document.getElementById("fx-base") as HTMLInputElement)?.value||"USD"; const d=(document.getElementById("fx-date") as HTMLInputElement)?.value||state.fxRates.date; state.fxRates.base=b; state.fxRates.date=d; loadFxRates(); return; }
  const ingSearch = target.closest<HTMLElement>("[data-action='ing-search']");
  if (ingSearch) { const v=(document.getElementById("ing-search") as HTMLInputElement)?.value??""; state.ingredients.search=v.toLowerCase(); render(); return; }
  const trSearch = target.closest<HTMLElement>("[data-action='tr-search']");
  if (trSearch) { const v=(document.getElementById("tr-search") as HTMLInputElement)?.value??""; state.transactions.search=v; render(); return; }
  const selEx = target.closest<HTMLElement>("[data-action='fitfat-select-exercise']");
  if (selEx?.dataset.exerciseId) {
    state.fitfat.selectedExerciseId = selEx.dataset.exerciseId;
    render();
    return;
  }
  const searchEx = target.closest<HTMLElement>("[data-action='fitfat-search-exercises']");
  if (searchEx) {
    const inp = document.querySelector<HTMLInputElement>("[data-fitfat-filter='exerciseSearch']");
    if (inp) state.fitfat.exerciseSearch = inp.value;
    render();
    return;
  }
  const showLogs = target.closest<HTMLElement>("[data-action='fitfat-show-logs']");
  if (showLogs) {
    loadDiagnostics();
    return;
  }
}

function renderDashboard(): string {
  const c = state.dashboard.counts;

  let loadingHtml = "";
  if (state.dashboard.status === "loading") {
    loadingHtml = `<div class="banner">Loading dashboard data...</div>`;
  }

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Dashboard</h2>
        <p class="panel-subtitle">Overview of your Maia workspace</p>
      </div>
      <div class="panel-content space-y-6">
        ${loadingHtml}
        <div class="stats-grid grid grid-cols-1 md:grid-cols-2 gap-4">
          <div class="stat">
            <h3 class="font-medium text-ink-200 mb-2">Tasks</h3>
            <p class="text-xl font-bold text-ink-100">${c ? c.tasks : "..."}</p>
            <p class="text-sm text-ink-400">items to track</p>
          </div>
          <div class="stat">
            <h3 class="font-medium text-ink-200 mb-2">Receipts</h3>
            <p class="text-xl font-bold text-ink-100">${c ? c.receipts : "..."}</p>
            <p class="text-sm text-ink-400">captured expenses</p>
          </div>
          <div class="stat">
            <h3 class="font-medium text-ink-200 mb-2">Notes</h3>
            <p class="text-xl font-bold text-ink-100">${c ? c.notes : "..."}</p>
            <p class="text-sm text-ink-400">knowledge entries</p>
          </div>
          <div class="stat">
            <h3 class="font-medium text-ink-200 mb-2">Goals</h3>
            <p class="text-xl font-bold text-ink-100">${c ? c.goals : "..."}</p>
            <p class="text-sm text-ink-400">active objectives</p>
          </div>
          <div class="stat">
            <h3 class="font-medium text-ink-200 mb-2">URLs</h3>
            <p class="text-xl font-bold text-ink-100">${c ? c.urls : "..."}</p>
            <p class="text-sm text-ink-400">saved links</p>
          </div>
        </div>
        
        <div class="quick-actions space-y-3">
          <h3 class="font-medium text-ink-200 mb-2">Quick Actions</h3>
          <div class="space-y-2">
            <button class="button w-full justify-start" type="button" data-view="todos">
              <span class="mr-2">✓</span> Manage Tasks
            </button>
            <button class="button w-full justify-start" type="button" data-view="goals">
              <span class="mr-2">★</span> View Goals
            </button>
            <button class="button w-full justify-start" type="button" data-view="receipts">
              <span class="mr-2">💰</span> Review Receipts
            </button>
            <button class="button w-full justify-start" type="button" data-view="notes">
              <span class="mr-2">📝</span> Browse Notes
            </button>
            <button class="button w-full justify-start" type="button" data-view="urls">
              <span class="mr-2">🔗</span> Saved URLs
            </button>
            <button class="button w-full justify-start" type="button" data-view="settings">
              <span class="mr-2">⚙️</span> Configure Settings
            </button>
          </div>
        </div>
      </div>
    </section>
  `;
}

function renderTodos(): string {
  const s = state.todos;

  // Build goal title lookup from loaded goals
  const goalTitleById: Record<number, string> = {};
  for (const g of state.goals.items) {
    goalTitleById[g.id] = g.title;
  }

  let filtered = s.items;
  if (s.filter === "active") {
    filtered = s.items.filter((t) => t.completed_at === null);
  } else if (s.filter === "completed") {
    filtered = s.items.filter((t) => t.completed_at !== null);
  }

  let statusHtml = "";
  if (s.status === "loading") {
    statusHtml = `<div class="banner">Loading...</div>`;
  } else if (s.status === "saving") {
    statusHtml = `<div class="banner">Saving...</div>`;
  } else if (s.status === "error" && s.statusMessage) {
    statusHtml = `<div class="banner banner-err">${escapeHtml(s.statusMessage)}</div>`;
  }

  const taskItems =
    filtered.length === 0
      ? `<div class="empty-state">No tasks yet. Tasks are managed on mobile.</div>`
      : filtered
          .map(
            (t) => `
            <div class="list-row ${t.completed_at ? "opacity-60" : ""}">
              <span class="mt-1 h-4 w-4 flex items-center justify-center text-sm ${t.completed_at ? "text-ok-400" : "text-ink-600"}">${t.completed_at ? "✓" : "○"}</span>
              <div class="flex-1 min-w-0">
                <span class="font-medium block ${t.completed_at ? "line-through text-ink-500" : "text-ink-100"}">${escapeHtml(t.title)}</span>
                <div class="flex flex-wrap gap-2 mt-1 text-sm text-ink-400">
                  ${t.priority ? `<span class="px-2 py-0.5 text-xs rounded font-medium ${priorityClass(t.priority)}">${escapeHtml(t.priority)}</span>` : ""}
                  ${t.due_date ? `<span>Due: ${escapeHtml(formatDate(t.due_date))}</span>` : ""}
                  ${renderTodoTags(t.tags)}
                  ${t.goal_id && goalTitleById[t.goal_id] ? `<span class="text-accent-400">${escapeHtml(goalTitleById[t.goal_id])}</span>` : ""}
                </div>
              </div>
            </div>
          `,
          )
          .join("");

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Tasks</h2>
        <p class="panel-subtitle">Manage your tasks and priorities</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}

        <div class="flex items-center space-x-3 mb-4">
          <button class="${s.filter === "all" ? "button-outline active" : "button-outline"}" type="button" data-todo-filter="all">All</button>
          <button class="${s.filter === "active" ? "button-outline active" : "button-outline"}" type="button" data-todo-filter="active">Active</button>
          <button class="${s.filter === "completed" ? "button-outline active" : "button-outline"}" type="button" data-todo-filter="completed">Completed</button>
          <span class="text-sm text-ink-400 ml-auto">${s.items.length} task${s.items.length !== 1 ? "s" : ""}</span>
        </div>

        <div class="space-y-2">
          ${taskItems}
        </div>

        <div class="mt-4">
          <button class="button secondary" type="button" data-view="dashboard">← Back to Dashboard</button>
        </div>
      </div>
    </section>
  `;
}

function renderTodoTags(tagsJson: string): string {
  try {
    const arr = JSON.parse(tagsJson);
    if (!Array.isArray(arr) || arr.length === 0) return "";
    return `<span class="text-accent-400 text-xs">${arr.join(", ")}</span>`;
  } catch {
    return "";
  }
}

function priorityClass(p: string): string {
  switch (p) {
    case "high":
      return "bg-err-500/20 text-err-400";
    case "medium":
      return "bg-caution-500/20 text-caution-400";
    case "low":
      return "bg-grn-500/20 text-grn-400";
    default:
      return "bg-ink-500/20 text-ink-400";
  }
}

function formatDate(iso: string): string {
  if (!iso) return "";
  try {
    const d = new Date(iso + "Z");
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

// ---- Datepicker helpers ----

function todayStr(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

// @ts-ignore
function renderDatePicker(id: string, value: string): string {
  const today = new Date();
  const curYear = today.getFullYear();
  const curMonth = today.getMonth();
  return `
    <div class="datepicker-wrapper" data-datepicker-wrapper="${id}">
      <div class="flex gap-2">
        <input type="text" id="${id}" readonly value="${escapeHtml(value)}"
          placeholder="YYYY-MM-DD"
          class="datepicker-input w-full px-3 py-2 bg-surface-800 border border-surface-700 rounded-md focus:outline-none focus:border-ink-500 cursor-pointer"
          data-action="datepicker-toggle" />
        <button type="button" data-action="datepicker-toggle"
          class="datepicker-toggle px-3 py-2 bg-surface-800 border border-surface-700 rounded-md text-ink-400 hover:text-ink-100 leading-none">
          &#128197;
        </button>
      </div>
      <div id="${id}-calendar" class="datepicker-calendar hidden" data-year="${curYear}" data-month="${curMonth}">
        ${renderCalendarGrid(id, curYear, curMonth, value)}
      </div>
    </div>
  `;
}

function renderCalendarGrid(id: string, year: number, month: number, selectedDate: string): string {
  const monthNames = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
  ];
  const firstDay = new Date(year, month, 1).getDay();
  const daysInMonth = new Date(year, month + 1, 0).getDate();

  let dayCells = "";
  for (let i = 0; i < firstDay; i++) {
    dayCells += "<div></div>";
  }
  for (let d = 1; d <= daysInMonth; d++) {
    const dateStr = `${year}-${String(month + 1).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
    const isSelected = dateStr === selectedDate;
    const isToday = dateStr === todayStr();
    const selectedClass = isSelected ? " datepicker-day-selected" : "";
    const todayClass = isToday ? " datepicker-day-today" : "";
    dayCells += `<button type="button" class="datepicker-day${selectedClass}${todayClass}" data-action="datepicker-day" data-date="${dateStr}">${d}</button>`;
  }

  return `
    <div class="datepicker-header">
      <button type="button" data-action="datepicker-prev-month" data-datepicker-id="${id}" class="datepicker-nav">&#9664;</button>
      <span class="text-sm font-medium text-ink-200">${monthNames[month]} ${year}</span>
      <button type="button" data-action="datepicker-next-month" data-datepicker-id="${id}" class="datepicker-nav">&#9654;</button>
    </div>
    <div class="datepicker-grid">
      <div class="datepicker-weekday">Su</div>
      <div class="datepicker-weekday">Mo</div>
      <div class="datepicker-weekday">Tu</div>
      <div class="datepicker-weekday">We</div>
      <div class="datepicker-weekday">Th</div>
      <div class="datepicker-weekday">Fr</div>
      <div class="datepicker-weekday">Sa</div>
      ${dayCells}
    </div>
  `;
}

function closeAllDatepickers(): void {
  document.querySelectorAll(".datepicker-calendar:not(.hidden)").forEach((cal) => {
    cal.classList.add("hidden");
  });
}

// ---- Receipt render functions ----

function renderReceiptList(): string {
  const items = state.receipts.receipts
    .map(
      (r) => `
        <button class="list-row w-full text-left ${state.receipts.selectedId === r.id ? "active" : ""}" type="button" data-receipt-id="${r.id}">
          <div class="min-w-0 flex-1">
            <div class="truncate text-sm font-medium text-ink-100">${escapeHtml(r.merchant || r.receipt_id || `Receipt #${r.id}`)}</div>
            <div class="mt-1 flex items-center gap-2 text-xs text-ink-500">
              ${r.date ? `<span>${escapeHtml(formatDate(r.date))}</span>` : ""}
              ${r.total ? `<span class="font-mono text-accent-400">${escapeHtml(r.total)}</span>` : ""}
              <span class="badge ${r.status === "reviewed" ? "badge-ok" : r.status === "archived" ? "badge-warn" : "badge-muted"}">${escapeHtml(r.status || "pending")}</span>
            </div>
            <div class="mt-1 flex items-center gap-2 text-xs text-ink-500">
              <span class="truncate text-accent-400/70">${escapeHtml(r.current_path)}</span>
            </div>
          </div>
        </button>
      `,
    )
    .join("");

  if (!items) {
    return `<div class="p-4 text-center text-sm text-ink-500">No receipts captured yet.</div>`;
  }
  return `<div class="flex max-h-[24rem] flex-col gap-1 overflow-y-auto">${items}</div>`;
}

function renderReceiptDetail(): string {
  const r = state.receipts.receipts.find((x) => x.id === state.receipts.selectedId);
  if (!r) {
    return `<div class="flex items-center justify-center h-full text-sm text-ink-500">Receipt not found</div>`;
  }

  const statusColor =
    r.status === "archived" ? "text-warn-400" :
    r.status === "reviewed" ? "text-ok-400" :
    "text-ink-400";

  return `
    <div class="space-y-4">
      <div class="rounded border border-surface-800 bg-surface-900/30 p-4">
        <h3 class="text-sm font-medium text-ink-200 mb-3">Receipt Info</h3>
        <div class="grid grid-cols-2 gap-3 text-sm">
          <div><span class="text-ink-500">ID:</span> <span class="text-ink-200">${escapeHtml(r.receipt_id || `#${r.id}`)}</span></div>
          <div><span class="text-ink-500">Status:</span> <span class="${statusColor}">${escapeHtml(r.status || "pending")}</span></div>
          ${r.merchant ? `<div class="col-span-2"><span class="text-ink-500">Merchant:</span> <span class="text-ink-200">${escapeHtml(r.merchant)}</span></div>` : ""}
          ${r.total ? `<div><span class="text-ink-500">Total:</span> <span class="text-ink-200 font-mono">${escapeHtml(r.total)}</span></div>` : ""}
          ${r.date ? `<div><span class="text-ink-500">Date:</span> <span class="text-ink-200">${escapeHtml(r.date)}</span></div>` : ""}
          ${r.category ? `<div><span class="text-ink-500">Category:</span> <span class="text-ink-200">${escapeHtml(r.category)}</span></div>` : ""}
        </div>
      </div>

      <div class="rounded border border-surface-800 bg-surface-900/30 p-4">
        <h3 class="text-sm font-medium text-ink-200 mb-3">File Paths</h3>
        <div class="space-y-2 text-sm">
          <div>
            <span class="text-ink-500">Original:</span>
            <div class="font-mono text-xs text-ink-300 mt-0.5 break-all">${escapeHtml(r.original_path)}</div>
          </div>
          <div>
            <span class="text-ink-500">Current:</span>
            <div class="font-mono text-xs text-accent-400 mt-0.5 break-all">${escapeHtml(r.current_path)}</div>
          </div>
          ${r.archived_path
            ? `<div>
                <span class="text-ink-500">Archived:</span>
                <div class="font-mono text-xs text-warn-400 mt-0.5 break-all">${escapeHtml(r.archived_path)}</div>
              </div>`
            : ""}
          ${r.parsed_json_path
            ? `<div>
                <span class="text-ink-500">Parsed JSON:</span>
                <div class="font-mono text-xs text-ink-300 mt-0.5 break-all">${escapeHtml(r.parsed_json_path)}</div>
              </div>`
            : ""}
          ${r.checksum
            ? `<div>
                <span class="text-ink-500">Checksum:</span>
                <div class="font-mono text-xs text-ink-400 mt-0.5">${escapeHtml(r.checksum)}</div>
              </div>`
            : ""}
        </div>
      </div>

      <div class="rounded border border-surface-800 bg-surface-900/30 p-4">
        <h3 class="text-sm font-medium text-ink-200 mb-3">Timestamps</h3>
        <div class="grid grid-cols-2 gap-2 text-sm">
          <div><span class="text-ink-500">Created:</span> <span class="text-ink-300">${escapeHtml(formatDate(r.created_at))}</span></div>
          ${r.processed_at
            ? `<div><span class="text-ink-500">Processed:</span> <span class="text-ink-300">${escapeHtml(formatDate(r.processed_at))}</span></div>`
            : `<div><span class="text-ink-500">Processed:</span> <span class="text-ink-500">not yet</span></div>`}
        </div>
      </div>

      <div class="flex items-center gap-3">
        <button class="button secondary" type="button" data-action="receipts-back">
          ← Back to list
        </button>
      </div>
    </div>
  `;
}

function renderReceipts(): string {
  const s = state.receipts;

  let statusHtml = "";
  if (s.status === "loading") {
    statusHtml = `<div class="banner">Loading...</div>`;
  } else if (s.status === "error" && s.statusMessage) {
    statusHtml = `<div class="banner banner-err">${escapeHtml(s.statusMessage)}</div>`;
  }

  // If a receipt is selected, show detail
  if (s.selectedId) {
    return `
      <section class="panel">
        <div class="panel-header">
          <h2 class="panel-title">Receipt Details</h2>
          <p class="panel-subtitle">
            ${(() => {
              const r = s.receipts.find((x) => x.id === s.selectedId);
              return r ? `ID: ${escapeHtml(r.receipt_id || `#${r.id}`)}` : "";
            })()}
          </p>
        </div>
        <div class="panel-content">
          ${statusHtml}
          ${renderReceiptDetail()}
        </div>
      </section>
    `;
  }

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Receipts</h2>
        <p class="panel-subtitle">Track and manage your expenses</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}
        
        <div class="receipt-controls flex justify-between items-center mb-6">
          <div class="flex items-center space-x-3">
            <span class="text-sm text-ink-400">${s.receipts.length} receipt${s.receipts.length !== 1 ? "s" : ""}</span>
          </div>
        </div>
        
        <div class="receipts-list">
          ${renderReceiptList()}
        </div>
        
        <div class="mt-4">
          <button class="button secondary" type="button" data-view="dashboard">← Back to Dashboard</button>
        </div>
      </div>
    </section>
  `;
}

// ---- Note render functions ----

function renderNoteList(): string {
  const items = state.notes.notes
    .map(
      (note) => `
        <button class="list-row w-full text-left ${state.notes.selectedId === note.id ? "active" : ""}" type="button" data-note-id="${note.id}">
          <div class="min-w-0 flex-1">
            <div class="truncate text-sm font-medium text-ink-100">${escapeHtml(note.title || "Untitled")}</div>
            <div class="mt-1 flex items-center gap-2 text-xs text-ink-500">
              <span>${escapeHtml(formatDate(note.updated_at))}</span>
              ${note.tags && note.tags !== "[]"
                ? `<span class="truncate text-accent-400">${escapeHtml(parseTags(note.tags).join(", "))}</span>`
                : ""}
            </div>
          </div>
        </button>
      `,
    )
    .join("");

  if (!items) {
    return `<div class="p-4 text-center text-sm text-ink-500">No notes yet.</div>`;
  }
  return `<div class="flex max-h-[24rem] flex-col gap-1 overflow-y-auto">${items}</div>`;
}

function parseTags(tagsJson: string): string[] {
  try {
    const arr = JSON.parse(tagsJson);
    return Array.isArray(arr) ? arr : [];
  } catch {
    return [];
  }
}

function renderNoteEditor(): string {
  if (!state.notes.selectedId) {
    return `<div class="flex items-center justify-center h-full text-sm text-ink-500">Select a note to view</div>`;
  }
  const s = state.notes;
  let statusHtml = "";
  if (s.status === "loading") statusHtml = `<div class="rounded border border-surface-700 bg-surface-900/70 p-2 text-xs text-ink-300">Loading...</div>`;
  else if (s.statusMessage) statusHtml = `<div class="rounded border border-surface-700 bg-surface-900/70 p-2 text-xs text-ink-300">${escapeHtml(s.statusMessage)}</div>`;
  return `
    <div class="note-editor">
      ${statusHtml}
      <div class="mb-3">
        <div class="w-full rounded border border-surface-700 bg-surface-900/50 px-3 py-2 text-sm font-medium text-ink-100">${escapeHtml(s.editTitle)}</div>
      </div>
      <div class="note-editor-textarea-wrapper">
        <pre class="h-64 w-full overflow-auto whitespace-pre-wrap rounded border border-surface-700 bg-surface-950 p-3 font-mono text-sm text-ink-100">${escapeHtml(s.editContent)}</pre>
      </div>
      <div class="mt-3">
        <div class="w-full rounded border border-surface-700 bg-surface-900/50 px-3 py-2 text-sm text-ink-400">Tags: ${escapeHtml(s.editTags) || "—"}</div>
      </div>
      <div class="mt-4 flex items-center gap-3">
        <button class="button secondary" type="button" data-action="notes-back">
          ← Back to list
        </button>
      </div>
    </div>
  `;
}

function renderNotes(): string {
  const s = state.notes;

  // If a note is selected, show the editor
  if (s.selectedId) {
    return `
      <section class="panel">
        <div class="panel-header">
          <h2 class="panel-title">Edit Note</h2>
          <p class="panel-subtitle">
            ${s.selectedId
              ? (s.notes.find((n) => n.id === s.selectedId)?.path
                  ? "File: " + escapeHtml(s.notes.find((n) => n.id === s.selectedId)!.path)
                  : "")
              : ""}
          </p>
        </div>
        <div class="panel-content">
          ${renderNoteEditor()}
        </div>
      </section>
    `;
  }

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Notes</h2>
        <p class="panel-subtitle">Read-only — managed on mobile</p>
      </div>
      <div class="panel-content space-y-6">
        <div class="flex justify-between items-center mb-6">
          <span class="text-sm text-ink-400">${s.notes.length} note${s.notes.length !== 1 ? "s" : ""} • read-only</span>
        </div>
        
        <div class="notes-list">
          ${renderNoteList()}
        </div>
        
        <div class="mt-4">
          <button class="button secondary" type="button" data-view="dashboard">← Back to Dashboard</button>
        </div>
      </div>
    </section>
  `;
}

// ---- Goals screen ----

function renderGoals(): string {
  const s = state.goals;

  // Compute task counts per goal from loaded todos
  const taskCountByGoal: Record<number, number> = {};
  for (const t of state.todos.items) {
    if (t.goal_id !== null) {
      taskCountByGoal[t.goal_id] = (taskCountByGoal[t.goal_id] || 0) + 1;
    }
  }

  let statusHtml = "";
  if (s.status === "loading") {
    statusHtml = `<div class="banner">Loading...</div>`;
  } else if (s.status === "saving") {
    statusHtml = `<div class="banner">Saving...</div>`;
  } else if (s.status === "error" && s.statusMessage) {
    statusHtml = `<div class="banner banner-err">${escapeHtml(s.statusMessage)}</div>`;
  }

  const goalItems =
    s.items.length === 0
      ? `<div class="empty-state">No goals yet. Goals are managed on mobile.</div>`
      : s.items
          .map(
            (g) => `
            <div class="rounded border border-surface-800 bg-surface-900/30 p-4">
              <div class="flex items-start justify-between gap-3">
                <div class="flex-1 min-w-0">
                  <h4 class="font-medium text-ink-100">${escapeHtml(g.title)}</h4>
                  ${g.description ? `<p class="text-sm text-ink-400 mt-1">${escapeHtml(g.description)}</p>` : ""}
                </div>
                <span class="badge ${goalStatusClass(g.status || "active")}">${escapeHtml(g.status || "active")}</span>
              </div>
              <div class="mt-3 flex items-center gap-3 text-sm text-ink-400">
                ${g.deadline ? `<span>Due: ${escapeHtml(formatDate(g.deadline))}</span>` : ""}
                <span>${taskCountByGoal[g.id] || 0} task${taskCountByGoal[g.id] !== 1 ? "s" : ""}</span>
              </div>
              <div class="mt-3 flex items-center gap-3">
                <div class="flex-1 bg-surface-800 rounded-full h-2 overflow-hidden">
                  <div class="h-full rounded-full ${g.progress >= 100 ? "bg-ok-500" : "bg-accent-500"}" style="width: ${Math.min(100, Math.max(0, g.progress))}%"></div>
                </div>
                <span class="text-xs text-ink-400 font-mono w-10 text-right">${g.progress}%</span>
              </div>
            </div>
          `,
          )
          .join("");

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Goals</h2>
        <p class="panel-subtitle">Track your objectives and progress — read-only, managed on mobile</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}

        <div class="flex items-center justify-between mb-4">
          <h3 class="font-medium text-ink-200">Goals</h3>
          <span class="text-sm text-ink-400">${s.items.length} goal${s.items.length !== 1 ? "s" : ""}</span>
        </div>

        <div class="space-y-3">
          ${goalItems}
        </div>

        <div class="mt-4">
          <button class="button secondary" type="button" data-view="dashboard">← Back to Dashboard</button>
        </div>
      </div>
    </section>
  `;
}

function goalStatusClass(status: string): string {
  switch (status) {
    case "active":
      return "bg-accent-900/30 text-accent-400";
    case "on-hold":
      return "bg-warn-900/30 text-warn-400";
    case "done":
      return "bg-ok-900/30 text-ok-400";
    default:
      return "bg-surface-700/50 text-ink-400";
  }
}

// ---- URLs screen ----

function renderUrls(): string {
  const s = state.urls;

  let statusHtml = "";
  if (s.status === "loading") {
    statusHtml = `<div class="banner">Loading...</div>`;
  } else if (s.status === "saving") {
    statusHtml = `<div class="banner">Saving...</div>`;
  } else if (s.status === "error" && s.statusMessage) {
    statusHtml = `<div class="banner banner-err">${escapeHtml(s.statusMessage)}</div>`;
  }

  const newCount = s.items.filter((u) => u.is_new === 1).length;

  const urlItems =
    s.items.length === 0
      ? `<div class="text-center py-8 text-ink-500">No saved URLs yet. Add one above!</div>`
      : s.items
          .map(
            (u) => `
            <div class="flex items-start gap-3 p-3 bg-surface-900/50 rounded-lg hover:bg-surface-900/70 transition-colors">
              ${u.is_new === 1 ? `<span class="mt-1 inline-block w-2 h-2 rounded-full bg-accent-400 shrink-0" title="New"></span>` : `<span class="mt-1 inline-block w-2 h-2 rounded-full bg-transparent shrink-0"></span>`}
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-medium text-ink-100 truncate">${escapeHtml(u.title || u.url)}</span>
                  ${u.is_new === 1 ? `<span class="badge badge-info">new</span>` : ""}
                </div>
                <div class="text-xs text-accent-400 truncate mt-0.5">${escapeHtml(u.url)}</div>
                <div class="flex flex-wrap gap-2 mt-1 text-xs text-ink-500">
                  ${u.source ? `<span>${escapeHtml(u.source)}</span>` : ""}
                  ${renderUrlTags(u.tags)}
                  <span>${escapeHtml(formatDate(u.created_at))}</span>
                </div>
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <button class="text-accent-400 hover:text-accent-300 text-sm px-2 py-1" type="button" data-action="open-url" data-url-id="${u.id}" title="Open in browser">↗</button>
                ${u.is_new === 1 ? `<button class="text-ink-400 hover:text-ink-300 text-xs px-2 py-1" type="button" data-action="mark-url-read" data-url-id="${u.id}" title="Mark as read">✓</button>` : ""}
                <button class="text-err-400 hover:text-err-300 text-sm px-2 py-1" type="button" data-action="delete-url" data-url-id="${u.id}" title="Delete URL">✕</button>
              </div>
            </div>
          `,
          )
          .join("");

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Saved URLs</h2>
        <p class="panel-subtitle">Links captured from Chrome or added manually</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}

        ${newCount > 0 ? `<div class="rounded border border-accent-800 bg-accent-900/20 p-3 text-sm text-accent-300">${newCount} new URL${newCount !== 1 ? "s" : ""} — click ✓ to mark as read, or ↗ to open</div>` : ""}

        <div class="bg-surface-900/50 p-4 rounded-lg mb-6">
          <h3 class="font-medium text-ink-200 mb-3">Add New URL</h3>
          <div class="space-y-3">
            <div>
              <label class="block text-sm font-medium text-ink-200 mb-1">URL</label>
              <input type="url" id="url-url" class="w-full px-3 py-2 bg-surface-800 border border-surface-700 rounded-md focus:outline-none focus:border-ink-500" placeholder="https://example.com" />
            </div>
            <div>
              <label class="block text-sm font-medium text-ink-200 mb-1">Title (optional)</label>
              <input type="text" id="url-title" class="w-full px-3 py-2 bg-surface-800 border border-surface-700 rounded-md focus:outline-none focus:border-ink-500" placeholder="Page title" />
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label class="block text-sm font-medium text-ink-200 mb-1">Source</label>
                <select id="url-source" class="w-full px-3 py-2 bg-surface-800 border border-surface-700 rounded-md focus:outline-none focus:border-ink-500">
                  <option value="manual">Manual</option>
                  <option value="chrome-extension">Chrome Extension</option>
                  <option value="import">Import</option>
                </select>
              </div>
              <div>
                <label class="block text-sm font-medium text-ink-200 mb-1">Tags (comma-separated)</label>
                <input type="text" id="url-tags" class="w-full px-3 py-2 bg-surface-800 border border-surface-700 rounded-md focus:outline-none focus:border-ink-500" placeholder="dev, article, reference" />
              </div>
            </div>
            <button class="button" type="button" data-action="create-url">Save URL</button>
          </div>
        </div>

        <div class="flex items-center justify-between mb-4">
          <h3 class="font-medium text-ink-200">Saved Links</h3>
          <span class="text-sm text-ink-400">${s.items.length} URL${s.items.length !== 1 ? "s" : ""}</span>
        </div>

        <div class="space-y-2">
          ${urlItems}
        </div>

        <div class="mt-4">
          <button class="button secondary" type="button" data-view="dashboard">← Back to Dashboard</button>
        </div>
      </div>
    </section>
  `;
}

function renderUrlTags(tagsJson: string): string {
  try {
    const arr = JSON.parse(tagsJson);
    if (!Array.isArray(arr) || arr.length === 0) return "";
    return `<span class="text-accent-400">${arr.join(", ")}</span>`;
  } catch {
    return "";
  }
}

function renderFitFat(): string {
  const ff = state.fitfat;
  let statusHtml = "";
  if (ff.status === "loading") statusHtml = `<div class="banner">Loading FitFat data from ${escapeHtml(resolveSyncUrl(ff.syncUrl))}...</div>`;
  else if (ff.status === "error") statusHtml = `<div class="banner banner-err">${escapeHtml(ff.statusMessage)}</div>`;

  const counts = ff.counts;
  const countsHtml = counts
    ? `<div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-4">
        <div class="panel p-4"><div class="text-xs text-ink-400">Workouts</div><div class="text-2xl font-semibold text-accent-400">${counts.workouts}</div></div>
        <div class="panel p-4"><div class="text-xs text-ink-400">Meals</div><div class="text-2xl font-semibold text-ok-400">${counts.meals}</div></div>
        <div class="panel p-4"><div class="text-xs text-ink-400">Exercises</div><div class="text-2xl font-semibold text-warn-400">${counts.exercises}</div></div>
        <div class="panel p-4"><div class="text-xs text-ink-400">Body Metrics</div><div class="text-2xl font-semibold text-grape-400">${counts.bodyMetrics}</div></div>
      </div>`
    : `<div class="text-sm text-ink-400 mb-4">No data yet — sync from FitFat mobile to see workouts, meals and weight.</div>`;

  const displayUrl = ff.lanUrl || resolveSyncUrl(ff.syncUrl);
  const qrHtml = ff.qrDataUrl
    ? `<div class="bg-white p-2 rounded-lg inline-block"><img src="${ff.qrDataUrl}" alt="FitFat QR" class="w-44 h-44" /></div><div class="mt-2 text-xs text-ink-400 break-all max-w-[280px]">${escapeHtml(JSON.stringify({ url: displayUrl, apiKey: ff.apiKey }))}</div><div class="mt-2 flex gap-2"><button class="button secondary text-xs" data-action="fitfat-copy-qr">Copy JSON</button><button class="button secondary text-xs" data-action="fitfat-copy-url">Copy URL</button></div><div class="mt-1 text-xs text-ink-500">LAN IP: ${escapeHtml(displayUrl)} ${displayUrl.includes("127.0.0.1") ? "(offline — connect to Wi-Fi)" : ""}</div>`
    : `<div class="text-xs text-ink-400">Generating QR...</div>`;

  const lanDisplay = ff.lanUrl || resolveSyncUrl(ff.syncUrl);
  const healthColor = ff.serverHealth === "online" ? "text-ok-400" : ff.serverHealth === "offline" ? "text-err-400" : "text-ink-400";
  const healthDot = ff.serverHealth === "online" ? "●" : ff.serverHealth === "offline" ? "○" : "◐";
  const diag = ff.diagnostics;
  const healthHtml = `
    <div class="panel p-4">
      <div class="flex items-center justify-between mb-2">
        <h3 class="text-sm font-medium text-ink-200">Server Health</h3>
        <span class="text-sm ${healthColor}">${healthDot} ${ff.serverHealth === "online" ? "Online" : ff.serverHealth === "offline" ? "Offline" : "Checking"} ${ff.lastHealthCheck ? `— ${escapeHtml(ff.lastHealthCheck)}` : ""}</span>
      </div>
      <div class="text-xs text-ink-400 mb-2">Bind: <code class="bg-surface-800 px-1 rounded">0.0.0.0:3030</code> • LAN: <code class="bg-surface-800 px-1 rounded">${escapeHtml(lanDisplay)}</code> • Local: <code class="bg-surface-800 px-1 rounded">${escapeHtml(resolveSyncUrl(ff.syncUrl))}</code></div>
      ${diag ? `<div class="text-xs text-ink-400 mb-2">Port: ${diag.port} • Listening: ${diag.isListening ? "✓" : "✗"} • LAN IP: ${escapeHtml(diag.lanIp)} • Key: ${escapeHtml(diag.apiKeyMasked)}</div>` : ""}
      <div class="flex flex-wrap gap-2 mb-3">
        <button class="button secondary text-xs" data-action="fitfat-test-connection">Test Connection</button>
        <button class="button secondary text-xs" data-action="fitfat-copy-curl">Copy curl</button>
        <button class="button secondary text-xs" data-action="fitfat-show-logs">Show Logs (${ff.logs.length})</button>
      </div>
      ${ff.statusMessage ? `<div class="rounded border border-surface-700 bg-surface-900/70 p-2 text-xs text-ink-300 break-all">${escapeHtml(ff.statusMessage)}</div>` : ""}
      ${ff.logs.length ? `<details class="mt-3"><summary class="text-xs text-ink-400 cursor-pointer">Recent requests (${ff.logs.length})</summary><pre class="mt-2 max-h-40 overflow-auto bg-surface-950 p-2 rounded text-xs text-ink-300 border border-surface-800">${escapeHtml(ff.logs.slice(-10).join("\n"))}</pre></details>` : ""}
      <div class="mt-3 text-xs text-ink-500">If mobile can't reach: 1) Phone on same Wi-Fi as ${escapeHtml(diag?.lanIp || ff.lanUrl || "10.229.34.33")} • 2) <code class="bg-surface-800 px-1 rounded">sudo ufw allow 3030/tcp</code> • 3) <code class="bg-surface-800 px-1 rounded">curl -v http://${escapeHtml((diag?.lanIp || "10.229.34.33"))}:3030/health</code></div>
    </div>
  `;
  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">FitFat</h2>
        <p class="panel-subtitle">Visualize workouts, meals, weight synced from FitFat — via ${escapeHtml(lanDisplay)}</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}
        ${healthHtml}
        <div class="flex gap-2">
          <button class="button secondary text-sm" data-action="fitfat-refresh">↻ Refresh</button>
          <button class="button secondary text-sm" data-view="settings">Settings → QR</button>
        </div>
        ${countsHtml}

        <div class="panel p-4">
          <div class="flex items-center justify-between mb-2">
            <h3 class="text-sm font-medium text-ink-200">Workouts / week</h3>
            <div class="flex items-center gap-2 text-xs"><input type="date" value="${escapeHtml(ff.filters.workoutsSince)}" data-fitfat-filter="workoutsSince" class="bg-surface-800 border border-surface-700 rounded px-2 py-1 text-xs" /><button class="button secondary text-xs" data-action="fitfat-apply-workouts">Apply</button></div>
          </div>
          <canvas id="fitfat-workouts-chart" height="120"></canvas>
        </div>

        <div class="panel p-4">
          <div class="flex items-center justify-between mb-2">
            <h3 class="text-sm font-medium text-ink-200">Volume (kg·reps) over time</h3>
            <div class="flex items-center gap-2 text-xs"><input type="date" value="${escapeHtml(ff.filters.workoutsSince)}" data-fitfat-filter="workoutsSince" class="bg-surface-800 border border-surface-700 rounded px-2 py-1 text-xs" /><button class="button secondary text-xs" data-action="fitfat-apply-workouts">Apply</button></div>
          </div>
          <canvas id="fitfat-volume-chart" height="120"></canvas>
        </div>

        <div class="panel p-4">
          <div class="flex items-center justify-between mb-2">
            <h3 class="text-sm font-medium text-ink-200">Weight trend</h3>
            <div class="flex items-center gap-2 text-xs"><input type="date" value="${escapeHtml(ff.filters.bodySince)}" data-fitfat-filter="bodySince" class="bg-surface-800 border border-surface-700 rounded px-2 py-1 text-xs" /><button class="button secondary text-xs" data-action="fitfat-apply-body">Apply</button></div>
          </div>
          <canvas id="fitfat-weight-chart" height="120"></canvas>
        </div>

        <div class="panel p-4">
          <div class="flex items-center justify-between mb-2">
            <h3 class="text-sm font-medium text-ink-200">Meals</h3>
            <div class="flex items-center gap-2 text-xs"><input type="date" value="${escapeHtml(ff.filters.mealsSince)}" data-fitfat-filter="mealsSince" class="bg-surface-800 border border-surface-700 rounded px-2 py-1 text-xs" /><button class="button secondary text-xs" data-action="fitfat-apply-meals">Apply</button></div>
          </div>
          <canvas id="fitfat-meals-chart" height="120"></canvas>
          <div class="mt-3 text-xs text-ink-400">${ff.meals.length} meals loaded${ff.meals.length ? ` — latest: ${escapeHtml(ff.meals[ff.meals.length - 1]?.name || "")}` : ""}</div>
        </div>

        <div class="panel p-4">
          <h3 class="text-sm font-medium text-ink-200 mb-2">Exercises — ${(counts as any)?.exercises ?? 0} in maia.db</h3>
          <div class="flex gap-2 mb-3"><input type="text" placeholder="Search exercises (name, body part, equipment)" value="${escapeHtml(ff.exerciseSearch)}" data-fitfat-filter="exerciseSearch" class="flex-1 bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm" /><button class="button secondary text-xs" data-action="fitfat-search-exercises">Search</button></div>
          <div class="max-h-96 overflow-auto list-divided">
            ${(ff.exercises.filter((e:any)=>{ if(!ff.exerciseSearch) return true; const q=ff.exerciseSearch.toLowerCase(); return (e.name||"").toLowerCase().includes(q)||(e.bodyPart||e.body_part||"").toLowerCase().includes(q)||(e.equipment||"").toLowerCase().includes(q); }).slice(0,100).map((e:any)=>`<button class="w-full text-left px-3 py-2 hover:bg-surface-800 flex items-center gap-3 ${ff.selectedExerciseId===e.id?'bg-surface-700/50':''}" data-action="fitfat-select-exercise" data-exercise-id="${escapeHtml(e.id)}"><img src="${escapeHtml(resolveSyncUrl(ff.syncUrl))}/media/${escapeHtml(e.id)}.jpg" alt="" class="w-10 h-10 rounded bg-surface-700 object-cover" onerror="this.style.display='none'" /><div class="min-w-0"><div class="text-sm text-ink-100 truncate">${escapeHtml(e.name)}</div><div class="text-xs text-ink-400 truncate">${escapeHtml(e.bodyPart||e.body_part||"")}${e.equipment?' • '+escapeHtml(e.equipment):""} ${e.primaryMuscle||e.primary_muscle?' • '+escapeHtml(e.primaryMuscle||e.primary_muscle):""}</div></div></button>`).join("") || `<div class="p-3 text-xs text-ink-400">No exercises match.</div>`)}
          </div>
          ${ff.selectedExerciseId ? (()=>{ const ex=(ff.exercises as any[]).find((x:any)=>x.id===ff.selectedExerciseId); if(!ex) return ""; const inst = (()=>{try{const a=JSON.parse(ex.instructions); if(Array.isArray(a)) return a; }catch{} return ex.instructions?[ex.instructions]:[];})(); const tipsArr = (()=>{try{const a=JSON.parse(ex.tips); if(Array.isArray(a)) return a;}catch{} return ex.tips?[ex.tips]:[];})(); return `<div class="mt-4 border-t border-surface-800 pt-4 space-y-3"><div class="flex gap-3"><img src="${escapeHtml(resolveSyncUrl(ff.syncUrl))}/media/${escapeHtml(ex.id)}.jpg" alt="" class="w-24 h-24 rounded bg-surface-800 object-cover" onerror="this.style.display='none'" /><video src="${escapeHtml(resolveSyncUrl(ff.syncUrl))}/media/${escapeHtml(ex.id)}.mp4" controls class="w-48 h-24 rounded bg-surface-800" onerror="this.style.display='none'"></video><div><h4 class="font-medium text-ink-100">${escapeHtml(ex.name)}</h4><p class="text-xs text-ink-400">${escapeHtml(ex.bodyPart||ex.body_part||"")} • ${escapeHtml(ex.equipment||"")}</p><p class="text-xs text-ink-400">Primary: ${escapeHtml(ex.primaryMuscle||ex.primary_muscle||"")}</p><p class="text-xs text-ink-400">Secondary: ${escapeHtml(ex.secondaryMuscle||ex.secondary_muscle||"")}</p></div></div>${inst.length?`<div><h5 class="text-xs font-medium text-ink-300">Instructions</h5><ol class="list-decimal ml-4 text-xs text-ink-400 space-y-1">${inst.map((s:string)=>`<li>${escapeHtml(s)}</li>`).join("")}</ol></div>`:""}${tipsArr.length?`<div><h5 class="text-xs font-medium text-ink-300">Tips</h5><ul class="list-disc ml-4 text-xs text-ink-400 space-y-1">${tipsArr.map((s:string)=>`<li>${escapeHtml(s)}</li>`).join("")}</ul></div>`:""}${ex.faqs?`<div><h5 class="text-xs font-medium text-ink-300">FAQs</h5><p class="text-xs text-ink-400 whitespace-pre-wrap">${escapeHtml(ex.faqs)}</p></div>`:""}</div>`; })():""}
        </div>

        <div class="panel p-4">
          <h3 class="text-sm font-medium text-ink-200 mb-2">Pairing QR — URL + Auth Token</h3>
          <p class="text-xs text-ink-400 mb-3">Scan with FitFat mobile to pair. Contains <code class="bg-surface-800 px-1 rounded">{"url","apiKey"}</code> for <code class="bg-surface-800 px-1 rounded">${escapeHtml(resolveSyncUrl(ff.syncUrl))}</code></p>
          <div class="flex flex-col items-start gap-3">
            ${qrHtml}
            <div class="text-xs text-ink-500">API Key: <code class="bg-surface-800 px-1 rounded select-all">${escapeHtml(ff.apiKey)}</code> <button class="button secondary text-xs ml-2" data-action="fitfat-copy-key">Copy Key</button></div>
          </div>
        </div>
      </div>
    </section>
  `;
}

// ---- Settings config form ----

function renderSettings(): string {
  const status = state.settings.status;
  const fields = state.settings.configFields;

  let statusHtml = "";
  if (status === "loading") {
    statusHtml = `<div class="banner">Loading configuration...</div>`;
  } else if (status === "saving") {
    statusHtml = `<div class="banner">Saving configuration...</div>`;
  } else if (status === "saved") {
    statusHtml = `<div class="banner banner-ok">${escapeHtml(state.settings.statusMessage)}</div>`;
  } else if (status === "error") {
    statusHtml = `<div class="banner banner-err">${escapeHtml(state.settings.statusMessage)}</div>`;
  }

  const formFields = CONFIG_FIELD_DEFS
    .map(
      (def) => `
        <div>
          <label class="field-label" for="cfg-${def.key}">${escapeHtml(def.label)}</label>
          <input type="text" id="cfg-${def.key}" data-config-key="${def.key}" value="${escapeHtml(fields[def.key] || "")}" placeholder="${escapeHtml(def.placeholder)}"
            class="w-full px-3 py-2 bg-surface-800 border border-surface-700 rounded-md focus:outline-none focus:border-ink-500" />
        </div>`,
    )
    .join("");

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Settings</h2>
        <p class="panel-subtitle">Configure your Maia workspace</p>
      </div>
      <div class="panel-content space-y-6">
        <div class="rounded border border-surface-800 bg-surface-900/30 p-4">
          <h3 class="mb-3 text-sm font-medium text-ink-200">Configuration</h3>
          <p class="mb-4 text-xs text-ink-400">
            Edit your Maia configuration. A backup will be created as <code class="rounded bg-surface-800 px-1 py-0.5 font-mono text-ink-300">maia.json.bak</code> when saving.
          </p>

          ${statusHtml}

          <div class="mt-4 space-y-4">
            ${formFields}
          </div>

          <div class="mt-6 flex items-center gap-3">
            <button class="button" type="button" data-action="save-settings" ${status === "loading" || status === "saving" ? "disabled" : ""}>
              ${status === "saving" ? "Saving..." : "Save Settings"}
            </button>
            <button class="button secondary" type="button" data-action="reload-settings" ${status === "loading" || status === "saving" ? "disabled" : ""}>
              Reload from File
            </button>
            <button class="button secondary" type="button" data-view="dashboard">
              ← Back to Dashboard
            </button>
          </div>
        </div>

        <div class="rounded border border-surface-800 bg-surface-900/30 p-4">
          <h3 class="mb-3 text-sm font-medium text-ink-200">FitFat Sync — Pairing</h3>
          <p class="mb-3 text-xs text-ink-400">Scan with FitFat mobile to pair. QR contains <code class="bg-surface-800 px-1 rounded">{"url","apiKey"}</code> for <code class="bg-surface-800 px-1 rounded">${escapeHtml(state.fitfat.lanUrl || resolveSyncUrl(state.fitfat.syncUrl) || "http://127.0.0.1:3030")}</code></p>
          <div class="flex flex-col items-start gap-3">
            ${state.fitfat.qrDataUrl ? `<div class="bg-white p-2 rounded-lg inline-block"><img src="${state.fitfat.qrDataUrl}" alt="QR" class="w-44 h-44" /></div>` : `<div class="text-xs text-ink-400">QR will appear after visiting FitFat tab or <button class="button secondary text-xs ml-2" data-action="fitfat-refresh">Generate</button></div>`}
            <div class="text-xs text-ink-500 break-all max-w-full">API Key: <code class="bg-surface-800 px-1 rounded select-all">${escapeHtml(state.fitfat.apiKey || "...")}</code> <button class="button secondary text-xs ml-2" data-action="fitfat-copy-key">Copy Key</button> <button class="button secondary text-xs" data-action="fitfat-copy-qr">Copy JSON</button></div>
            <div class="text-xs text-ink-500 break-all">Payload: <code class="bg-surface-800 px-1 rounded">${escapeHtml(JSON.stringify({ url: state.fitfat.lanUrl || resolveSyncUrl(state.fitfat.syncUrl), apiKey: state.fitfat.apiKey, version: 1 }))}</code></div>
            ${state.fitfat.lanUrl && state.fitfat.lanUrl.includes("127.0.0.1") ? `<div class="text-xs text-warn-400">Offline — connect to Wi-Fi for LAN IP (showing loopback)</div>` : ""}
          </div>
        </div>
      </div>
    </section>
  `;
}

async function loadExercises(): Promise<void> {
  state.exercises.status = "loading"; render();
  try {
    const res = await invoke<any>("list_exercises");
    const data = res?.data ?? res;
    state.exercises.items = Array.isArray(data) ? data : (data?.items ?? []);
    state.exercises.status = "idle";
  } catch (e) { state.exercises.status = "error"; state.exercises.statusMessage = String(e); }
  applyExerciseFilter(); render();
}
function applyExerciseFilter(): void {
  const q = state.exercises.search.toLowerCase();
  const bp = state.exercises.bodyPart; const eq = state.exercises.equipment; const hi = state.exercises.hasImage;
  state.exercises.filtered = state.exercises.items.filter((e:any)=>{
    if (q && !(`${e.name} ${e.body_part||e.bodyPart||""} ${e.equipment||""} ${e.primary_muscle||e.primaryMuscle||""}`.toLowerCase().includes(q))) return false;
    if (bp && (e.body_part||e.bodyPart) !== bp) return false;
    if (eq && e.equipment !== eq) return false;
    if (hi === "yes" && !e.image_path && !e.hasImage) return false;
    if (hi === "no" && (e.image_path || e.hasImage)) return false;
    return true;
  });
}
async function loadIngredients(): Promise<void> { state.ingredients.status="loading"; render(); try{ const r=await invoke<any>("list_ingredients"); const d=r?.data??r; state.ingredients.items=Array.isArray(d)?d:(d?.items??[]); state.ingredients.status="idle"; }catch(e){state.ingredients.status="error"; state.ingredients.statusMessage=String(e);} render(); }
async function loadStores(): Promise<void> { state.stores.status="loading"; render(); try{ const r=await invoke<any>("list_stores"); const d=r?.data??r; state.stores.items=Array.isArray(d)?d:(d?.items??[]); state.stores.status="idle"; }catch(e){state.stores.status="error"; state.stores.statusMessage=String(e);} render(); }
async function loadMeals(): Promise<void> { state.meals.status="loading"; render(); try{ const r=await invoke<any>("list_meals"); const d=r?.data??r; state.meals.items=Array.isArray(d)?d:(d?.meals??d?.items??[]); state.meals.status="idle"; }catch(e){state.meals.status="error"; state.meals.statusMessage=String(e);} render(); }
async function loadBodyMetrics(): Promise<void> { state.bodyMetrics.status="loading"; render(); try{ const r=await invoke<any>("list_body_metrics"); const d=r?.data??r; state.bodyMetrics.items=Array.isArray(d)?d:(d?.items??[]); state.bodyMetrics.status="idle"; }catch(e){state.bodyMetrics.status="error"; state.bodyMetrics.statusMessage=String(e);} render(); }
async function loadExperiments(): Promise<void> { state.experiments.status="loading"; render(); try{ const r=await invoke<any>("list_experiments"); const d=r?.data??r; state.experiments.items=Array.isArray(d)?d:(d?.items??[]); state.experiments.status="idle"; }catch(e){state.experiments.status="error"; state.experiments.statusMessage=String(e);} render(); }
async function loadTags(): Promise<void> { state.tags.status="loading"; render(); try{ const r=await invoke<any>("list_tags"); const d=r?.data??r; state.tags.items=Array.isArray(d)?d:(d?.items??[]); state.tags.status="idle"; }catch(e){state.tags.status="error"; state.tags.statusMessage=String(e);} render(); }
async function loadAccounts(): Promise<void> { state.accounts.status="loading"; render(); try{ const r=await invoke<any>("list_accounts"); const d=r?.data??r; state.accounts.items=Array.isArray(d)?d:[]; state.accounts.status="idle"; }catch(e){state.accounts.status="error"; state.accounts.statusMessage=String(e);} render(); }
async function loadTransactions(): Promise<void> { state.transactions.status="loading"; render(); try{ const r=await invoke<any>("list_transactions"); const d=r?.data??r; state.transactions.items=Array.isArray(d)?d:(d?.items??[]); state.transactions.status="idle"; }catch(e){state.transactions.status="error"; state.transactions.statusMessage=String(e);} render(); }
async function loadFxRates(): Promise<void> { state.fxRates.status="loading"; render(); try{ const r=await invoke<any>("list_fx_rates",{base:state.fxRates.base,date:state.fxRates.date}); const d=r?.data??r; state.fxRates.items=Array.isArray(d)?d:(d?.items??[]); state.fxRates.status="idle"; }catch(e){state.fxRates.status="error"; state.fxRates.statusMessage=String(e);} render(); }
function renderExercises(): string {
  if (state.exercises.status==="loading") return `<section class="panel"><div class="p-6 text-sm text-ink-400">Loading exercises...</div></section>`;
  const bodyParts=[...new Set(state.exercises.items.map((e:any)=>e.body_part||e.bodyPart).filter(Boolean))].sort();
  const equipments=[...new Set(state.exercises.items.map((e:any)=>e.equipment).filter(Boolean))].sort();
  const list=(state.exercises.filtered.length?state.exercises.filtered:state.exercises.items).slice(0,100);
  const sel=state.exercises.selectedId?state.exercises.items.find((e:any)=>e.id===state.exercises.selectedId):null;
  const syncInfo=`<div class="text-xs text-ink-500">maia.db • ${state.exercises.items.length} total • ${state.exercises.filtered.length||state.exercises.items.length} filtered • lazy media</div>`;
  return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Exercises — ${state.exercises.items.length}</h2><p class="panel-subtitle">Catalog from data/ parsed • editable • virtual scroll 100</p></div><div class="panel-content space-y-4">
    <div class="flex flex-wrap gap-2"><input id="ex-search" placeholder="Search name/body/equipment" value="${escapeHtml(state.exercises.search)}" class="bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm flex-1 min-w-[200px]" /><select id="ex-bodypart" class="bg-surface-800 border border-surface-700 rounded px-2 py-2 text-sm"><option value="">All body parts</option>${bodyParts.map(b=>`<option ${state.exercises.bodyPart===b?"selected":""}>${escapeHtml(b)}</option>`).join("")}</select><select id="ex-equip" class="bg-surface-800 border border-surface-700 rounded px-2 py-2 text-sm"><option value="">All equipment</option>${equipments.map(b=>`<option ${state.exercises.equipment===b?"selected":""}>${escapeHtml(b)}</option>`).join("")}</select><select id="ex-hasimg" class="bg-surface-800 border border-surface-700 rounded px-2 py-2 text-sm"><option value="">All</option><option value="yes" ${state.exercises.hasImage==="yes"?"selected":""}>Has image</option><option value="no" ${state.exercises.hasImage==="no"?"selected":""}>No image</option></select><button class="button secondary" data-action="ex-apply">Apply</button></div>
    ${syncInfo}
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-4"><div class="border border-surface-800 rounded max-h-[60vh] overflow-auto divide-y divide-surface-800">${list.map((e:any)=>`<button class="w-full text-left p-3 hover:bg-surface-800 flex gap-3 ${state.exercises.selectedId===e.id?"bg-surface-700/50":""}" data-action="ex-select" data-id="${escapeHtml(e.id)}"><img src="${resolveSyncUrl(state.fitfat.syncUrl)}/media/${escapeHtml(e.id)}.jpg" loading="lazy" class="w-12 h-12 rounded bg-surface-700 object-cover" onerror="this.style.display='none'" /><div class="min-w-0"><div class="text-sm text-ink-100 truncate">${escapeHtml(e.name)}</div><div class="text-xs text-ink-400 truncate">${escapeHtml(e.body_part||e.bodyPart||"")} • ${escapeHtml(e.equipment||"")} • ${escapeHtml(e.primary_muscle||e.primaryMuscle||"")}</div></div></button>`).join("")||`<div class="empty-state">No exercises yet.</div>`}</div>
    <div class="border border-surface-800 rounded p-4 bg-surface-900/30">${sel?`<div class="space-y-3"><div class="flex gap-3"><img src="${resolveSyncUrl(state.fitfat.syncUrl)}/media/${escapeHtml(sel.id)}.jpg" loading="lazy" class="w-24 h-24 rounded bg-surface-800" onerror="this.style.display='none'" /><video src="${resolveSyncUrl(state.fitfat.syncUrl)}/media/${escapeHtml(sel.id)}.mp4" controls class="w-40 h-24 rounded bg-surface-800" onerror="this.style.display='none'"></video><div><h3 class="font-medium text-ink-100">${escapeHtml(sel.name)}</h3><p class="text-xs text-ink-400">${escapeHtml(sel.body_part||sel.bodyPart||"")} • ${escapeHtml(sel.equipment||"")}</p><p class="text-xs text-ink-400">Primary: ${escapeHtml(sel.primary_muscle||sel.primaryMuscle||"")}</p><p class="text-xs text-ink-400">Secondary: ${escapeHtml(sel.secondary_muscle||sel.secondaryMuscle||"")}</p></div></div>${(()=>{try{const a=JSON.parse(sel.instructions); if(Array.isArray(a)) return `<ol class="list-decimal ml-4 text-xs text-ink-400 space-y-1">${a.map((s:string)=>`<li>${escapeHtml(s)}</li>`).join("")}</ol>`;}catch{} return sel.instructions?`<p class="text-xs text-ink-400">${escapeHtml(String(sel.instructions))}</p>`:"";})()}${(()=>{try{const a=JSON.parse(sel.tips); if(Array.isArray(a)) return `<ul class="list-disc ml-4 text-xs text-ink-400">${a.map((s:string)=>`<li>${escapeHtml(s)}</li>`).join("")}</ul>`;}catch{} return "";})()}${sel.faqs?`<p class="text-xs text-ink-400 whitespace-pre-wrap">${escapeHtml(sel.faqs)}</p>`:""}<div class="flex gap-2"><button class="button secondary text-xs" data-action="ex-edit" data-id="${escapeHtml(sel.id)}">Edit (save)</button></div></div>`:`<div class="text-sm text-ink-400">Select an exercise</div>`}</div></div></div></section>`;
}
function renderWorkouts(): string { const w=state.fitfat.workouts; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Workouts — ${w.length} (read-only)</h2><p class="panel-subtitle">From maia.db workouts • per-tab sync</p></div><div class="panel-content"><div class="text-xs text-ink-500 mb-2">maia.db • ${w.length} workouts</div><div class="space-y-2 max-h-[60vh] overflow-auto">${w.slice(0,50).map((x:any)=>`<div class="border border-surface-800 rounded p-3 bg-surface-900/30"><div class="text-sm text-ink-100">${escapeHtml(x.name)} — ${new Date(x.date).toLocaleDateString()}</div><div class="text-xs text-ink-400">${escapeHtml(x.notes||"")}</div></div>`).join("")||`<div class="text-xs text-ink-400">No workouts</div>`}</div></div></section>`; }
async function loadTemplateDetail(id: string): Promise<void> {
  (state as any).templates = { ...((state as any).templates||{}), selectedId: id, detailLoading: true, detail: null, exSearch: "" };
  render();
  if(!(state.exercises.items||[]).length){ loadExercises(); }
  try {
    const res = await invoke<any>("get_template", { id });
    const data = res?.data ?? res;
    if (data && data.success === false) { toast(`Couldn't load template: ${data.error || "unknown error"}`, "err"); (state as any).templates = { ...((state as any).templates||{}), detailLoading: false }; }
    else { (state as any).templates = { ...((state as any).templates||{}), detail: (data.template && data.exercises) ? data : data?.data, detailLoading: false }; }
  } catch (e) { toast(`Couldn't load template: ${e}`, "err"); (state as any).templates = { ...((state as any).templates||{}), detailLoading: false }; }
  render();
}
function tplNum(v: string | null | undefined): number | null { const s=(v??"").trim(); if(!s) return null; const n=parseFloat(s); return isNaN(n)?null:n; }
function tplInt(v: string | null | undefined): number | null { const s=(v??"").trim(); if(!s) return null; const n=parseInt(s,10); return isNaN(n)?null:n; }
function tplVal(id: string): string | null { const el=document.getElementById(id) as HTMLInputElement | null; return el ? el.value : null; }
function renderTemplates(): string {
  const tst=(state as any).templates||{};
  const t=tst.items||[]; const editingId=tst.editingId||null; const selectedId=tst.selectedId||null;
  const rows=t.map((x:any)=>{ if(editingId===x.id) return `<div class="border border-surface-700 rounded p-3 space-y-2"><input id="tpl-edit-name" value="${escapeHtml(x.name||"")}" placeholder="Name" class="bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm w-full" /><input id="tpl-edit-notes" value="${escapeHtml(x.notes||"")}" placeholder="Notes (optional)" class="bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm w-full" /><input id="tpl-edit-recurrence" value="${escapeHtml(x.recurrence||"")}" placeholder="Recurrence rule (optional)" class="bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm w-full" /><div class="flex gap-2"><button class="button text-xs" data-action="tpl-save" data-id="${escapeHtml(x.id)}">Save</button><button class="button secondary text-xs" data-action="tpl-cancel">Cancel</button></div></div>`; const sel=selectedId===x.id; return `<div class="border ${sel?"border-surface-600 bg-surface-900/60":"border-surface-800"} rounded p-3"><div class="flex items-center gap-2"><div class="min-w-0 flex-1"><div class="text-sm text-ink-100">${escapeHtml(x.name)}</div>${x.notes?`<div class="text-xs text-ink-400">${escapeHtml(x.notes)}</div>`:""}${x.recurrence?`<div class="text-xs text-ink-500">${escapeHtml(x.recurrence)}</div>`:""}</div>${sel?"":`<button class="button text-xs" data-action="tpl-open" data-id="${escapeHtml(x.id)}">Open ›</button>`}<button class="button secondary text-xs" data-action="tpl-edit" data-id="${escapeHtml(x.id)}">Edit</button><button class="button danger text-xs" data-action="tpl-delete" data-id="${escapeHtml(x.id)}">Delete</button></div></div>`; }).join("")||`<div class="text-xs text-ink-400">No templates</div>`;
  const right=!selectedId
    ? `<div class="border border-surface-800 rounded p-6 text-center text-xs text-ink-500">Select a template on the left to edit its exercises and sets.</div>`
    : renderTemplateDetailPane(tst);
  return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Templates — editable</h2><p class="panel-subtitle">Workout blueprints • recurrence • list stays visible while you edit</p></div><div class="panel-content"><div class="flex gap-2 mb-3"><input id="tpl-name" placeholder="New template name" class="bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm flex-1" /><button class="button" data-action="tpl-create">Create</button></div><div class="grid grid-cols-1 lg:grid-cols-2 gap-4"><div class="space-y-2">${rows}</div><div>${right}</div></div></div></section>`;
}
function renderTemplateDetailPane(tst: any): string {
  const d=tst.detail; const exSearch=(tst.exSearch||"").toLowerCase();
  const matches=exSearch?(state.exercises.items||[]).filter((e:any)=>(e.name||"").toLowerCase().includes(exSearch)).slice(0,15):[];
  const setInput=(setId:string,field:string,value:any,cls:string)=>`<input id="ts-${setId}-${field}" type="number" step="any" value="${value??""}" placeholder="–" class="bg-surface-800 border border-surface-700 rounded px-2 py-1 text-xs ${cls}" />`;
  const body=!d?`<div class="text-xs text-ink-400">${tst.detailLoading?"Loading template…":"Couldn't load template."}</div>`:`<div class="space-y-4">${(d.exercises||[]).map((b:any)=>`<div class="border border-surface-800 rounded p-3 space-y-2"><div class="flex items-center gap-2"><div class="text-sm font-medium text-ink-100 flex-1">${escapeHtml(b.exerciseName||b.exerciseId)}</div><button class="button danger text-xs" data-action="tpl-remove-exercise" data-id="${escapeHtml(b.id)}">Remove</button></div><input id="tpl-exnotes-${b.id}" value="${escapeHtml(b.notes||"")}" placeholder="Exercise note (optional)" class="bg-surface-800 border border-surface-700 rounded px-2 py-1 text-xs w-full" /><div class="text-xs text-ink-500">Set • reps • kg • rest s • min • m</div>${(b.sets||[]).map((s:any)=>`<div class="flex items-center gap-1"><span class="text-xs text-ink-400 w-6">${s.setNumber}</span>${setInput(s.id,"reps",s.reps,"w-14")}${setInput(s.id,"weight",s.weightKg,"w-16")}${setInput(s.id,"rest",s.restSeconds,"w-14")}${setInput(s.id,"dur",s.durationMinutes,"w-14")}${setInput(s.id,"dist",s.distanceMeters,"w-16")}<button class="button secondary text-xs px-2" data-action="tpl-delete-set" data-id="${escapeHtml(s.id)}" data-block="${escapeHtml(b.id)}">×</button></div>`).join("")||`<div class="text-xs text-ink-500">No sets yet.</div>`}<div class="flex gap-2"><button class="button text-xs" data-action="tpl-save-block" data-id="${escapeHtml(b.id)}">Save</button><button class="button secondary text-xs" data-action="tpl-add-set" data-id="${escapeHtml(b.id)}">Add set</button></div></div>`).join("")||`<div class="text-xs text-ink-400">No exercises yet — add one below.</div>`}<div class="border border-surface-800 rounded p-3 space-y-2"><div class="text-xs font-medium text-ink-300">Add exercise</div><div class="flex gap-2"><input id="tpl-ex-search" value="${escapeHtml(tst.exSearch||"")}" placeholder="Search exercises" class="bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm flex-1" /><button class="button secondary text-xs" data-action="tpl-ex-search">Find</button></div>${matches.map((e:any)=>`<div class="flex items-center gap-2"><div class="text-xs text-ink-200 flex-1 truncate">${escapeHtml(e.name)}</div><button class="button secondary text-xs" data-action="tpl-add-exercise" data-id="${escapeHtml(e.id)}">Add</button></div>`).join("")}</div></div>`;
  const head=d?`<div><h2 class="panel-title">${escapeHtml(d.template.name)}</h2><p class="panel-subtitle">${d.template.notes?escapeHtml(d.template.notes)+" • ":""}${d.template.recurrence?escapeHtml(d.template.recurrence):"blueprint"}</p></div>`:`<div><h2 class="panel-title">Template</h2></div>`;
  return `<div class="space-y-3"><div class="flex items-start gap-2"><div class="flex-1 min-w-0">${head}</div><button class="button secondary text-xs shrink-0" data-action="tpl-back">Close</button></div>${body}</div>`;
}
function renderIngredients(): string {
  const c=state.ingredients;
  if(c.status==="loading") return `<section class="panel"><div class="p-6 text-sm text-ink-400">Loading ingredients...</div></section>`;
  const modal = c.showAddModal ? `
    <div class="modal-overlay modal-open">
      <div class="absolute inset-0" data-action="ing-close-modal"></div>
      <div class="modal-dialog max-w-2xl max-h-[90vh] overflow-hidden flex flex-col" role="dialog" aria-modal="true" aria-label="Add New Ingredient">
        <div class="px-6 py-4 border-b border-surface-800 flex items-center justify-between shrink-0">
          <div>
            <h3 class="modal-title flex items-center gap-2"><span class="w-8 h-8 rounded-lg bg-ok-500/10 text-ok-400 flex items-center justify-center text-sm">🥗</span> Add New Ingredient</h3>
            <p class="text-xs text-ink-500 mt-1">Add a food item with nutrition per 100g — you can add pictures & prices afterwards</p>
          </div>
          <button class="w-8 h-8 rounded-lg bg-surface-800 hover:bg-surface-700 text-ink-400 hover:text-ink-200 flex items-center justify-center" data-action="ing-close-modal" aria-label="Close">✕</button>
        </div>
        <div class="flex-1 overflow-auto p-6 space-y-6">
          <div>
            <h4 class="text-xs font-semibold tracking-widest uppercase text-ink-400 mb-3">Basic Information</h4>
            <div class="space-y-4">
              <div>
                <label class="block text-sm font-medium text-ink-200 mb-1.5">Name <span class="text-err-400">*</span></label>
                <input id="ing-new-name" placeholder="e.g. Oats, Chicken Breast, Olive Oil" class="w-full px-3 py-2.5 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-ok-500 focus:ring-1 focus:ring-ok-500 text-sm" />
                <p class="text-xs text-ink-500 mt-1">Used for search and meal building</p>
              </div>
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <div>
                  <label class="block text-sm font-medium text-ink-200 mb-1.5">Brand <span class="text-ink-500 font-normal">(optional)</span></label>
                  <input id="ing-new-brand" placeholder="e.g. Quaker" class="w-full px-3 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm" />
                </div>
                <div>
                  <label class="block text-sm font-medium text-ink-200 mb-1.5">Barcode <span class="text-ink-500 font-normal">(optional)</span></label>
                  <input id="ing-new-barcode" placeholder="e.g. 0123456789" class="w-full px-3 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm font-mono" />
                </div>
              </div>
            </div>
          </div>
          <div>
            <h4 class="text-xs font-semibold tracking-widest uppercase text-ok-400 mb-3 flex items-center gap-2"><span class="w-5 h-5 rounded bg-ok-500/10 flex items-center justify-center text-[10px]">100g</span> Nutrition per 100g</h4>
            <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
              <div class="sm:col-span-2">
                <label class="block text-xs font-medium text-ink-300 mb-1">Calories <span class="text-err-400">*</span></label>
                <div class="relative"><input id="ing-new-cal" type="number" step="0.1" placeholder="368" class="w-full pl-3 pr-12 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-ok-500 text-sm" /><span class="absolute right-3 top-2.5 text-xs text-ink-500">kcal</span></div>
              </div>
              <div>
                <label class="block text-xs font-medium text-ink-300 mb-1">Protein</label>
                <div class="relative"><input id="ing-new-prot" type="number" step="0.1" placeholder="16" class="w-full pl-3 pr-8 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm" /><span class="absolute right-2 top-2.5 text-xs text-ink-500">g</span></div>
              </div>
              <div>
                <label class="block text-xs font-medium text-ink-300 mb-1">Carbs</label>
                <div class="relative"><input id="ing-new-carb" type="number" step="0.1" placeholder="60" class="w-full pl-3 pr-8 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm" /><span class="absolute right-2 top-2.5 text-xs text-ink-500">g</span></div>
              </div>
              <div>
                <label class="block text-xs font-medium text-ink-300 mb-1">Fat</label>
                <div class="relative"><input id="ing-new-fat" type="number" step="0.1" placeholder="7" class="w-full pl-3 pr-8 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm" /><span class="absolute right-2 top-2.5 text-xs text-ink-500">g</span></div>
              </div>
              <div>
                <label class="block text-xs font-medium text-ink-300 mb-1">Fiber <span class="text-ink-500 font-normal">(optional)</span></label>
                <div class="relative"><input id="ing-new-fiber" type="number" step="0.1" placeholder="10" class="w-full pl-3 pr-8 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm" /><span class="absolute right-2 top-2.5 text-xs text-ink-500">g</span></div>
              </div>
              <div>
                <label class="block text-xs font-medium text-ink-300 mb-1">Sugar <span class="text-ink-500 font-normal">(optional)</span></label>
                <div class="relative"><input id="ing-new-sugar" type="number" step="0.1" placeholder="1" class="w-full pl-3 pr-8 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm" /><span class="absolute right-2 top-2.5 text-xs text-ink-500">g</span></div>
              </div>
              <div>
                <label class="block text-xs font-medium text-ink-300 mb-1">Sodium <span class="text-ink-500 font-normal">(optional)</span></label>
                <div class="relative"><input id="ing-new-sodium" type="number" step="1" placeholder="5" class="w-full pl-3 pr-10 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm" /><span class="absolute right-2 top-2.5 text-xs text-ink-500">mg</span></div>
              </div>
            </div>
            <p class="text-xs text-ink-500 mt-3 bg-surface-800/50 rounded-lg px-3 py-2 border border-surface-800">💡 Tip: You can leave optional fields empty — they’ll be hidden in meals and can be added later.</p>
          </div>
        </div>
        <div class="modal-actions mt-0 px-6 py-4 bg-surface-800/30 border-t border-surface-800 items-center justify-between">
          <button class="button secondary" data-action="ing-close-modal">Cancel</button>
          <div class="flex items-center gap-3">
            <span class="text-xs text-ink-500 hidden sm:block">Press Esc to close</span>
            <button class="button ok px-6" data-action="ing-create">Add Ingredient →</button>
          </div>
        </div>
      </div>
    </div>` : "";
  return `<section class="panel"><div class="panel-header flex items-start justify-between gap-4"><div><h2 class="panel-title">Ingredients — ${c.items.length}</h2><p class="panel-subtitle">Manage foods • search, edit, and build meals</p></div><button class="button ok shrink-0" data-action="ing-open-modal">+ Add Ingredient</button></div><div class="panel-content space-y-3"><div class="flex gap-2"><div class="relative flex-1"><span class="absolute left-3 top-2.5 text-ink-500 text-sm">🔍</span><input id="ing-search" placeholder="Search by name, brand or barcode..." value="${escapeHtml(c.search)}" class="w-full pl-9 pr-3 py-2 bg-surface-800 border border-surface-700 rounded-lg focus:outline-none focus:border-surface-600 text-sm" /></div><button class="button secondary" data-action="ing-search">Search</button></div><div class="grid gap-2 max-h-[60vh] overflow-auto pr-1">${c.items.slice(0,50).map((it:any)=>`<div class="group border border-surface-800 rounded-xl p-3 bg-surface-900/30 hover:bg-surface-900/50 hover:border-surface-700 transition-colors"><div class="flex justify-between gap-3"><div class="min-w-0"><div class="text-sm font-medium text-ink-100 truncate">${escapeHtml(it.name)} ${it.is_archived?`<span class="badge badge-warn ml-2">archived</span>`:""}</div><div class="text-xs text-ink-400 mt-1 flex flex-wrap gap-1.5"><span class="chip">${it.calories_per100g} kcal</span><span class="chip">P ${it.protein_per100g}</span><span class="chip">C ${it.carbs_per100g}</span><span class="chip">F ${it.fat_per100g}</span>${it.brand?`<span class="text-ink-500">• ${escapeHtml(it.brand)}</span>`:""} ${it.barcode?`<span class="font-mono text-[11px] text-ink-500">• ${escapeHtml(it.barcode)}</span>`:""}</div></div><button class="button secondary text-xs opacity-0 group-hover:opacity-100 transition-opacity shrink-0 self-start" data-action="ing-edit" data-id="${escapeHtml(it.id)}">Edit</button></div>${(it.pictures||[]).length?`<div class="flex gap-2 mt-3">${(it.pictures||[]).slice(0,3).map((p:any)=>`<img src="${escapeHtml(p.imagePath)}" class="w-12 h-12 rounded-lg object-cover bg-surface-800 border border-surface-700" loading="lazy" onerror="this.style.display='none'" />`).join("")}</div>`:""}${(it.prices||[]).length?`<div class="mt-2 text-xs text-ink-400 flex flex-wrap gap-2">${(it.prices||[]).slice(0,5).map((pr:any)=>`<span class="px-2 py-1 rounded-full bg-surface-800 border border-surface-700">${escapeHtml(pr.storeId)}: <b class="text-ink-200">${pr.price}${pr.currencyCode}</b> • ${new Date(pr.recordedAt).toLocaleDateString()}</span>`).join("")}</div><canvas data-spark="${escapeHtml(it.id)}" height="30" class="w-full mt-1"></canvas>`:""}</div>`).join("")}</div></div>${modal}</section>`;
}
function renderStores(): string { const s=state.stores; if(s.status==="loading") return `<section class="panel"><div class="p-6 text-sm text-ink-400">Loading stores...</div></section>`; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Stores — ${s.items.length}</h2></div><div class="panel-content space-y-3"><div class="flex gap-2"><input id="store-name" placeholder="New store name" class="bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm flex-1" /><button class="button" data-action="store-create">Create</button></div><div class="list-divided">${s.items.map((st:any)=>`<div class="p-3 flex justify-between"><span class="text-sm text-ink-100">${escapeHtml(st.name)}</span><span class="text-xs text-ink-400">${new Date(st.createdAt).toLocaleDateString()}</span></div>`).join("")||`<div class="p-3 text-xs text-ink-400">No stores</div>`}</div></div></section>`; }
function renderMeals(): string { const m=state.meals; if(m.status==="loading") return `<section class="panel"><div class="p-6">Loading meals...</div></section>`; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Meals — ${m.items.length} (read-only)</h2><p class="panel-subtitle">Auto-calc all macros (cal/prot/carb/fat + sodium/fiber/sugar)</p></div><div class="panel-content space-y-2 max-h-[60vh] overflow-auto">${m.items.slice(0,50).map((meal:any)=>`<div class="border border-surface-800 rounded p-3 bg-surface-900/30"><div class="text-sm text-ink-100">${escapeHtml(meal.name)} — ${new Date(meal.eatenAt).toLocaleString()}</div><div class="text-xs text-ink-400">${(meal.ingredients||meal.mealIngredients||[]).map((mi:any)=>`${escapeHtml(mi.ingredientId)} ${mi.grams}g`).join(" • ")||"No ingredients"}</div></div>`).join("")||`<div class="text-xs text-ink-400">No meals</div>`}</div></section>`; }
function renderBodyMetrics(): string { const b=state.bodyMetrics; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Body Metrics — ${b.items.length} (read-only)</h2></div><div class="panel-content"><canvas id="body-metrics-chart" height="120"></canvas><div class="mt-3 max-h-64 overflow-auto list-divided">${b.items.map((x:any)=>`<div class="p-2 flex justify-between text-xs"><span>${new Date(x.date).toLocaleDateString()}</span><span>${x.weightKg??""} kg • ${x.heightCm??""} cm</span></div>`).join("")||`<div class="p-2 text-xs text-ink-400">No data</div>`}</div></div></section>`; }
function renderExperiments(): string { const e=state.experiments; if(e.status==="loading") return `<section class="panel"><div class="p-6">Loading experiments...</div></section>`; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Experiments — ${e.items.length} (read-only)</h2><p class="panel-subtitle">Linked tasks/goals/notes/tags + heatmap</p></div><div class="panel-content space-y-3"><div class="grid gap-2 max-h-[60vh] overflow-auto">${e.items.map((ex:any)=>`<div class="border border-surface-800 rounded p-3 bg-surface-900/30"><div class="text-sm text-ink-100">${escapeHtml(ex.name)} — ${escapeHtml(ex.status)}</div><div class="text-xs text-ink-400">${escapeHtml(ex.purpose||"")}</div><div class="text-xs text-ink-500">${new Date(ex.startDate).toLocaleDateString()} → ${ex.endDate?new Date(ex.endDate).toLocaleDateString():"open"} • ${escapeHtml(ex.categories||"")}</div><div class="mt-2 grid grid-cols-7 gap-1">${Array.from({length:30}).map((_,i)=>`<div class="w-4 h-4 rounded ${["bg-surface-800","bg-ok-900","bg-ok-700","bg-ok-500","bg-ok-300"][Math.floor(Math.random()*5)]}" title="day ${i}"></div>`).join("")}</div></div>`).join("")||`<div class="text-xs text-ink-400">No experiments</div>`}</div></div></section>`; }
function renderTags(): string { const t=state.tags; if(t.status==="loading") return `<section class="panel"><div class="p-6">Loading tags...</div></section>`; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Tags — ${t.items.length} (read-only)</h2><p class="panel-subtitle">Usage per entity</p></div><div class="panel-content list-divided">${t.items.map((tag:any)=>`<div class="p-3 flex justify-between"><span class="text-sm" style="color:${tag.color?`#${tag.color.toString(16).padStart(6,"0")}`:"#cbd5e1"}">${escapeHtml(tag.name)}</span><span class="text-xs text-ink-400">tasks:${tag.taskCount??0} exp:${tag.experimentCount??0} goals:${tag.goalCount??0} notes:${tag.noteCount??0}</span></div>`).join("")||`<div class="p-3 text-xs text-ink-400">No tags</div>`}</div></section>`; }
function renderAccounts(): string { const a=state.accounts; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Accounts — ${a.items.length}</h2></div><div class="panel-content list-divided">${a.items.map((ac:any)=>`<div class="p-3 flex justify-between"><span class="text-sm text-ink-100">${escapeHtml(ac.name)} (${escapeHtml(ac.type)})</span><span class="text-xs text-ink-400">${ac.openingBalance}</span></div>`).join("")||`<div class="p-3 text-xs text-ink-400">No accounts</div>`}</div></section>`; }
function renderTransactions(): string { const tr=state.transactions; const q=tr.search.toLowerCase(); const filtered=q?tr.items.filter((x:any)=>`${x.category||""} ${x.note||""} ${x.type}`.toLowerCase().includes(q)):tr.items; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">Transactions — ${tr.items.length}</h2></div><div class="panel-content space-y-3"><div class="flex gap-2"><input id="tr-search" placeholder="Search transactions" value="${escapeHtml(tr.search)}" class="bg-surface-800 border border-surface-700 rounded px-3 py-2 text-sm flex-1" /><button class="button secondary" data-action="tr-search">Search</button></div><div class="max-h-[60vh] overflow-auto list-divided">${filtered.slice(0,50).map((x:any)=>`<div class="p-3 flex justify-between"><span class="text-sm ${x.type==="expense"?"text-err-400":x.type==="income"?"text-ok-400":"text-ink-100"}">${escapeHtml(x.type)} ${x.amount} ${escapeHtml(x.currencyCode)} → ${x.amountBase} base (rate ${x.rateUsed})</span><span class="text-xs text-ink-400">${new Date(x.date).toLocaleDateString()} • ${escapeHtml(x.category||"")}</span></div>`).join("")||`<div class="p-3 text-xs text-ink-400">No transactions</div>`}</div></div></section>`; }
function renderFxRates(): string { const f=state.fxRates; if(f.status==="loading") return `<section class="panel"><div class="p-6">Loading FxRates...</div></section>`; return `<section class="panel"><div class="panel-header"><h2 class="panel-title">FxRates — ${f.items.length}</h2><p class="panel-subtitle">Add new • refresh • no edit</p></div><div class="panel-content space-y-3"><div class="flex flex-wrap gap-2"><input id="fx-base" value="${escapeHtml(f.base)}" placeholder="Base" class="bg-surface-800 border border-surface-700 rounded px-2 py-2 w-20 text-sm" /><input id="fx-date" type="date" value="${escapeHtml(f.date)}" class="bg-surface-800 border border-surface-700 rounded px-2 py-2 text-sm" /><button class="button secondary" data-action="fx-refresh">Refresh</button></div><div class="flex gap-2"><input id="fx-code" placeholder="Code EUR" class="bg-surface-800 border border-surface-700 rounded px-2 py-2 text-sm" /><input id="fx-rate" placeholder="Rate" type="number" step="0.0001" class="bg-surface-800 border border-surface-700 rounded px-2 py-2 text-sm" /><button class="button" data-action="fx-add">Add</button></div><div class="list-divided max-h-64 overflow-auto">${f.items.map((x:any)=>`<div class="p-2 flex justify-between text-xs"><span>${escapeHtml(x.code)}→${escapeHtml(x.baseCode||f.base)} ${x.rateToBase} • ${escapeHtml(x.rateDate||x.date||"")}</span><span class="badge badge-muted">${x.manual?"manual":"auto"}</span></div>`).join("")||`<div class="p-2 text-xs text-ink-400">No rates</div>`}</div></div></section>`; }

function handleInput(event: Event): void {
  const target = event.target as HTMLElement;

  if (target.hasAttribute("data-fitfat-filter")) {
    const key = (target as HTMLElement).getAttribute("data-fitfat-filter");
    if (key === "exerciseSearch") {
      state.fitfat.exerciseSearch = (target as HTMLInputElement).value;
      return;
    }
    if (key && key in state.fitfat.filters) {
      (state.fitfat.filters as Record<string, string>)[key] = (target as HTMLInputElement).value;
    }
  }


}

window.addEventListener("DOMContentLoaded", () => {
  render();
  document.addEventListener("click", handleClick);
  document.addEventListener("input", handleInput);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && state.ingredients.showAddModal) {
      state.ingredients.showAddModal = false;
      render();
    }
  });

  // Zoom via CSS custom property on :root — scales all rem units proportionally (no distortion)
  // Ctrl+= / Ctrl+- to zoom in/out, Ctrl+0 to reset
  document.addEventListener("keydown", (e) => {
    if (!e.ctrlKey && !e.metaKey) return;
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) return;

    const step = 0.1;
    const minZoom = 0.5;
    const maxZoom = 2.0;

    if (e.key === "=" || e.key === "+") {
      e.preventDefault();
      _currentZoom = Math.min(maxZoom, _currentZoom + step);
      document.documentElement.style.setProperty("--zoom-level", String(_currentZoom));
    } else if (e.key === "-") {
      e.preventDefault();
      _currentZoom = Math.max(minZoom, _currentZoom - step);
      document.documentElement.style.setProperty("--zoom-level", String(_currentZoom));
    } else if (e.key === "0") {
      e.preventDefault();
      _currentZoom = 1.0;
      document.documentElement.style.setProperty("--zoom-level", "1");
    }
  });
});

// Module-level zoom tracking (persists across renders)
let _currentZoom = 1.0;

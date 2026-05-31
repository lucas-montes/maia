import "./styles/tailwind.css";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

type ViewId = "dashboard" | "todos" | "receipts" | "notes" | "settings" | "goals" | "urls";

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

interface GoalsState {
  items: Goal[];
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

interface AppState {
  view: ViewId;
  sidebarCollapsed: boolean;
  settings: {
    configJson: string;
    status: "idle" | "loading" | "saving" | "saved" | "error";
    statusMessage: string;
  };
  notes: NotesState;
  receipts: ReceiptsState;
  todos: TodosState;
  goals: GoalsState;
  dashboard: DashboardState;
  urls: UrlsState;
}

const defaultAppState = (): AppState => ({
  view: "dashboard",
  sidebarCollapsed: false,
  settings: {
    configJson: "",
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
});

let state: AppState = defaultAppState();

function setView(view: ViewId): void {
  state.view = view;
  if (view === "settings") {
    loadConfig();
  } else if (view === "notes") {
    loadNotes();
  } else if (view === "receipts") {
    loadReceipts();
  } else if (view === "todos") {
    loadTodos();
    // Load goals for the goal selector dropdown
    loadGoals();
  } else if (view === "goals") {
    loadGoals();
    loadTodos(); // For task counts per goal
  } else if (view === "urls") {
    loadUrls();
  } else if (view === "dashboard") {
    loadDashboardCounts();
  }
  render();
}

function toggleSidebar(): void {
  state.sidebarCollapsed = !state.sidebarCollapsed;
  render();
}

async function loadConfig(): Promise<void> {
  state.settings.status = "loading";
  state.settings.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: string; error?: string }>("read_config");
    if (result.success && result.data) {
      // Pretty-print the config for the editor
      const parsed = JSON.parse(result.data);
      state.settings.configJson = JSON.stringify(parsed, null, 2);
      state.settings.status = "idle";
    } else {
      state.settings.status = "error";
      state.settings.statusMessage = result.error ?? "Failed to load config";
    }
  } catch (e) {
    state.settings.status = "error";
    state.settings.statusMessage = `Failed to load config: ${e}`;
  }
  render();
}

async function saveConfig(json: string): Promise<void> {
  state.settings.status = "saving";
  state.settings.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; error?: string }>("save_config", { json });
    if (result.success) {
      state.settings.status = "saved";
      state.settings.statusMessage = "Settings saved successfully.";
      // Update local state with the formatted version
      try {
        const parsed = JSON.parse(json);
        state.settings.configJson = JSON.stringify(parsed, null, 2);
      } catch {
        state.settings.configJson = json;
      }
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

async function saveCurrentNote(): Promise<void> {
  if (!state.notes.selectedId) return;

  state.notes.status = "saving";
  state.notes.statusMessage = "";
  render();

  const tagsArr = state.notes.editTags
    .split(",")
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
  const tagsJson = JSON.stringify(tagsArr);

  try {
    const result = await invoke<{ success: boolean; error?: string }>("save_note", {
      id: state.notes.selectedId,
      title: state.notes.editTitle,
      content: state.notes.editContent,
      tags: tagsJson,
    });
    if (result.success) {
      state.notes.status = "idle";
      state.notes.statusMessage = "Note saved.";
      // Reload the notes list to get updated timestamps
      await loadNotes();
    } else {
      state.notes.status = "error";
      state.notes.statusMessage = result.error ?? "Failed to save note";
    }
  } catch (e) {
    state.notes.status = "error";
    state.notes.statusMessage = `Failed to save note: ${e}`;
  }
  render();
}

async function createNote(): Promise<void> {
  state.notes.status = "loading";
  state.notes.statusMessage = "";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Note; error?: string }>("create_note", {
      title: "Untitled",
    });
    if (result.success && result.data) {
      // Reload list and select the new note
      await loadNotes();
      await selectNote(result.data.id);
    } else {
      state.notes.status = "error";
      state.notes.statusMessage = result.error ?? "Failed to create note";
      render();
    }
  } catch (e) {
    state.notes.status = "error";
    state.notes.statusMessage = `Failed to create note: ${e}`;
    render();
  }
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

async function createTodo(): Promise<void> {
  const titleInput = document.getElementById("todo-title") as HTMLInputElement | null;
  const descInput = document.getElementById("todo-description") as HTMLTextAreaElement | null;
  const dueInput = document.getElementById("todo-due") as HTMLInputElement | null;
  const prioSelect = document.getElementById("todo-priority") as HTMLSelectElement | null;
  const tagsInput = document.getElementById("todo-tags") as HTMLInputElement | null;

  const title = titleInput?.value?.trim();
  if (!title) return;

  state.todos.status = "saving";
  render();

  const tagsArr = (tagsInput?.value || "")
    .split(",")
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
  const tagsJson = JSON.stringify(tagsArr);

  try {
    const result = await invoke<{ success: boolean; data?: Todo; error?: string }>("create_task", {
      title,
      description: descInput?.value?.trim() || null,
      due_date: dueInput?.value || null,
      priority: prioSelect?.value || "medium",
      tags: tagsJson,
      goal_id: null,
    });
    if (result.success) {
      // Reset form
      if (titleInput) titleInput.value = "";
      if (descInput) descInput.value = "";
      if (dueInput) dueInput.value = "";
      if (prioSelect) prioSelect.value = "medium";
      if (tagsInput) tagsInput.value = "";
      state.todos.status = "idle";
      await loadTodos();
    } else {
      state.todos.status = "error";
      state.todos.statusMessage = result.error ?? "Failed to create task";
    }
  } catch (e) {
    state.todos.status = "error";
    state.todos.statusMessage = `Failed to create task: ${e}`;
  }
  render();
}

async function toggleTodo(id: number, currentlyCompleted: boolean): Promise<void> {
  const todo = state.todos.items.find((t) => t.id === id);
  if (!todo) return;

  state.todos.status = "saving";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Todo; error?: string }>("update_task", {
      id,
      title: todo.title,
      description: todo.description,
      due_date: todo.due_date,
      priority: todo.priority,
      tags: todo.tags,
      completed: !currentlyCompleted,
      goal_id: todo.goal_id,
    });
    if (result.success) {
      state.todos.status = "idle";
      await loadTodos();
    } else {
      state.todos.status = "error";
      state.todos.statusMessage = result.error ?? "Failed to update task";
    }
  } catch (e) {
    state.todos.status = "error";
    state.todos.statusMessage = `Failed to update task: ${e}`;
  }
  render();
}

async function deleteTodo(id: number): Promise<void> {
  state.todos.status = "saving";
  render();

  try {
    const result = await invoke<{ success: boolean; error?: string }>("delete_task", { id });
    if (result.success) {
      state.todos.status = "idle";
      await loadTodos();
    } else {
      state.todos.status = "error";
      state.todos.statusMessage = result.error ?? "Failed to delete task";
    }
  } catch (e) {
    state.todos.status = "error";
    state.todos.statusMessage = `Failed to delete task: ${e}`;
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

async function createGoal(): Promise<void> {
  const titleInput = document.getElementById("goal-title") as HTMLInputElement | null;
  const descInput = document.getElementById("goal-description") as HTMLTextAreaElement | null;
  const statusSelect = document.getElementById("goal-status") as HTMLSelectElement | null;
  const deadlineInput = document.getElementById("goal-deadline") as HTMLInputElement | null;
  const progressInput = document.getElementById("goal-progress") as HTMLInputElement | null;

  const title = titleInput?.value?.trim();
  if (!title) return;

  state.goals.status = "saving";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Goal; error?: string }>("create_goal", {
      title,
      description: descInput?.value?.trim() || null,
      status: statusSelect?.value || "active",
      deadline: deadlineInput?.value || null,
      progress: parseFloat(progressInput?.value || "0"),
    });
    if (result.success) {
      if (titleInput) titleInput.value = "";
      if (descInput) descInput.value = "";
      if (statusSelect) statusSelect.value = "active";
      if (deadlineInput) deadlineInput.value = "";
      if (progressInput) progressInput.value = "0";
      state.goals.status = "idle";
      await loadGoals();
    } else {
      state.goals.status = "error";
      state.goals.statusMessage = result.error ?? "Failed to create goal";
    }
  } catch (e) {
    state.goals.status = "error";
    state.goals.statusMessage = `Failed to create goal: ${e}`;
  }
  render();
}

async function updateGoalProgress(id: number, progress: number): Promise<void> {
  const goal = state.goals.items.find((g) => g.id === id);
  if (!goal) return;

  state.goals.status = "saving";
  render();

  try {
    const result = await invoke<{ success: boolean; data?: Goal; error?: string }>("update_goal", {
      id,
      title: goal.title,
      description: goal.description,
      status: goal.status,
      deadline: goal.deadline,
      progress,
    });
    if (result.success) {
      state.goals.status = "idle";
      await loadGoals();
    } else {
      state.goals.status = "error";
      state.goals.statusMessage = result.error ?? "Failed to update goal";
    }
  } catch (e) {
    state.goals.status = "error";
    state.goals.statusMessage = `Failed to update goal: ${e}`;
  }
  render();
}

async function deleteGoal(id: number): Promise<void> {
  state.goals.status = "saving";
  render();

  try {
    const result = await invoke<{ success: boolean; error?: string }>("delete_goal", { id });
    if (result.success) {
      state.goals.status = "idle";
      await loadGoals();
    } else {
      state.goals.status = "error";
      state.goals.statusMessage = result.error ?? "Failed to delete goal";
    }
  } catch (e) {
    state.goals.status = "error";
    state.goals.statusMessage = `Failed to delete goal: ${e}`;
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
  const navItems: Array<[ViewId, string, string]> = [
    ["dashboard", "Dashboard", "🎯"],
    ["todos", "Todos", "✓"],
    ["goals", "Goals", "★"],
    ["receipts", "Receipts", "💰"],
    ["notes", "Notes", "📝"],
    ["urls", "URLs", "🔗"],
    ["settings", "Settings", "⚙️"],
  ];

  return `
    <aside class="app-sidebar ${state.sidebarCollapsed ? "app-sidebar-collapsed" : "app-sidebar-expanded"}" aria-label="Primary navigation">
      <button class="app-sidebar-toggle" type="button" data-action="toggle-sidebar" title="Toggle sidebar" aria-label="Toggle sidebar">
        ${state.sidebarCollapsed ? "▶" : "◀"}
      </button>

      <div class="app-sidebar-brand ${state.sidebarCollapsed ? "app-sidebar-brand-collapsed" : ""}">
        <div class="app-sidebar-brand-mark">M</div>
        <div class="min-w-0 ${state.sidebarCollapsed ? "hidden" : "block"}">
          <p class="m-0 text-[0.68rem] uppercase tracking-[0.12em] text-slate-400">Maia</p>
          <h1 class="m-0 text-sm font-semibold text-slate-100">Desktop</h1>
        </div>
      </div>

      <nav class="app-sidebar-nav">
        ${navItems
          .map(
            ([view, label, icon]) => `
              <button class="app-sidebar-nav-button ${state.view === view ? "app-sidebar-nav-button-active" : "app-sidebar-nav-button-inactive"} ${state.sidebarCollapsed ? "app-sidebar-nav-button-collapsed" : ""}" type="button" data-view="${view}" aria-current="${state.view === view ? "page" : "false"}" title="${escapeHtml(label)}" aria-label="${escapeHtml(label)}">
                <span class="app-sidebar-nav-icon">${escapeHtml(icon)}</span>
                <span class="truncate ${state.sidebarCollapsed ? "hidden" : "block"}">${escapeHtml(label)}</span>
              </button>
            `,
          )
          .join("")}
      </nav>

      <div class="app-sidebar-footer ${state.sidebarCollapsed ? "hidden" : "block"}">
        <div class="flex flex-col gap-1">
          <span class="text-xs text-slate-400">${state.todos.items.length} task${state.todos.items.length !== 1 ? "s" : ""}</span>
          <span class="text-xs text-slate-400">${state.receipts.receipts.length} receipt${state.receipts.receipts.length !== 1 ? "s" : ""}</span>
          <span class="text-xs text-slate-500">Local</span>
        </div>
      </div>
    </aside>
  `;
}

function renderContent(): string {
  switch (state.view) {
    case "dashboard":
      return renderDashboard();
    case "todos":
      return renderTodos();
    case "goals":
      return renderGoals();
    case "receipts":
      return renderReceipts();
    case "notes":
      return renderNotes();
    case "urls":
      return renderUrls();
    case "settings":
      return renderSettings();
    default:
      return `<div class="p-6">Loading...</div>`;
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

function handleClick(event: MouseEvent): void {
  const target = event.target as HTMLElement;

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

  // Handle settings save
  const saveButton = target.closest<HTMLElement>("[data-action='save-settings']");
  if (saveButton) {
    const editor = document.getElementById("settings-editor") as HTMLTextAreaElement | null;
    if (editor) {
      saveConfig(editor.value);
    }
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

  // Handle notes create
  const createNoteButton = target.closest<HTMLElement>("[data-action='create-note']");
  if (createNoteButton) {
    createNote();
    return;
  }

  // Handle notes save
  const saveNoteButton = target.closest<HTMLElement>("[data-action='save-note']");
  if (saveNoteButton) {
    saveCurrentNote();
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

  // ---- Task handlers ----

  // Handle create todo
  const createTodoButton = target.closest<HTMLElement>("[data-action='create-todo']");
  if (createTodoButton) {
    createTodo();
    return;
  }

  // Handle toggle todo
  const toggleCheckbox = target.closest<HTMLElement>("[data-action='toggle-todo']");
  if (toggleCheckbox?.dataset.todoId) {
    const id = Number(toggleCheckbox.dataset.todoId);
    const todo = state.todos.items.find((t) => t.id === id);
    if (todo) {
      toggleTodo(id, todo.completed_at !== null);
    }
    return;
  }

  // Handle delete todo
  const deleteTodoButton = target.closest<HTMLElement>("[data-action='delete-todo']");
  if (deleteTodoButton?.dataset.todoId) {
    deleteTodo(Number(deleteTodoButton.dataset.todoId));
    return;
  }

  // Handle todo filter
  const filterButton = target.closest<HTMLElement>("[data-todo-filter]");
  if (filterButton?.dataset.todoFilter) {
    state.todos.filter = filterButton.dataset.todoFilter as "all" | "active" | "completed";
    render();
    return;
  }

  // ---- Goal handlers ----

  // Handle create goal
  const createGoalButton = target.closest<HTMLElement>("[data-action='create-goal']");
  if (createGoalButton) {
    createGoal();
    return;
  }

  // Handle delete goal
  const deleteGoalButton = target.closest<HTMLElement>("[data-action='delete-goal']");
  if (deleteGoalButton?.dataset.goalId) {
    deleteGoal(Number(deleteGoalButton.dataset.goalId));
    return;
  }

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
}

function renderDashboard(): string {
  const c = state.dashboard.counts;

  let loadingHtml = "";
  if (state.dashboard.status === "loading") {
    loadingHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Loading dashboard data...</div>`;
  }

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Dashboard</h2>
        <p class="panel-subtitle text-slate-400">Overview of your Maia workspace</p>
      </div>
      <div class="panel-content space-y-6">
        ${loadingHtml}
        <div class="stats-grid grid grid-cols-1 md:grid-cols-2 gap-4">
          <div class="stat-card bg-slate-900/50 p-4 rounded-lg">
            <h3 class="font-medium text-slate-200 mb-2">Tasks</h3>
            <p class="text-xl font-bold text-slate-100">${c ? c.tasks : "..."}</p>
            <p class="text-sm text-slate-400">items to track</p>
          </div>
          <div class="stat-card bg-slate-900/50 p-4 rounded-lg">
            <h3 class="font-medium text-slate-200 mb-2">Receipts</h3>
            <p class="text-xl font-bold text-slate-100">${c ? c.receipts : "..."}</p>
            <p class="text-sm text-slate-400">captured expenses</p>
          </div>
          <div class="stat-card bg-slate-900/50 p-4 rounded-lg">
            <h3 class="font-medium text-slate-200 mb-2">Notes</h3>
            <p class="text-xl font-bold text-slate-100">${c ? c.notes : "..."}</p>
            <p class="text-sm text-slate-400">knowledge entries</p>
          </div>
          <div class="stat-card bg-slate-900/50 p-4 rounded-lg">
            <h3 class="font-medium text-slate-200 mb-2">Goals</h3>
            <p class="text-xl font-bold text-slate-100">${c ? c.goals : "..."}</p>
            <p class="text-sm text-slate-400">active objectives</p>
          </div>
          <div class="stat-card bg-slate-900/50 p-4 rounded-lg">
            <h3 class="font-medium text-slate-200 mb-2">URLs</h3>
            <p class="text-xl font-bold text-slate-100">${c ? c.urls : "..."}</p>
            <p class="text-sm text-slate-400">saved links</p>
          </div>
        </div>
        
        <div class="quick-actions space-y-3">
          <h3 class="font-medium text-slate-200 mb-2">Quick Actions</h3>
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

  let filtered = s.items;
  if (s.filter === "active") {
    filtered = s.items.filter((t) => t.completed_at === null);
  } else if (s.filter === "completed") {
    filtered = s.items.filter((t) => t.completed_at !== null);
  }

  let statusHtml = "";
  if (s.status === "loading") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Loading...</div>`;
  } else if (s.status === "saving") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Saving...</div>`;
  } else if (s.status === "error" && s.statusMessage) {
    statusHtml = `<div class="rounded border border-red-700 bg-red-900/30 p-3 text-sm text-red-300">${escapeHtml(s.statusMessage)}</div>`;
  }

  const taskItems =
    filtered.length === 0
      ? `<div class="text-center py-8 text-slate-500">No tasks yet. Add your first task above!</div>`
      : filtered
          .map(
            (t) => `
            <div class="flex items-start gap-3 p-3 bg-slate-900/50 rounded-lg hover:bg-slate-900/70 transition-colors ${t.completed_at ? "opacity-60" : ""}">
              <input type="checkbox" class="mt-1 h-4 w-4 accent-sky-500 cursor-pointer" data-action="toggle-todo" data-todo-id="${t.id}" ${t.completed_at ? "checked" : ""} />
              <div class="flex-1 min-w-0">
                <span class="font-medium block ${t.completed_at ? "line-through text-slate-500" : "text-slate-100"}">${escapeHtml(t.title)}</span>
                <div class="flex flex-wrap gap-2 mt-1 text-sm text-slate-400">
                  ${t.priority ? `<span class="px-2 py-0.5 text-xs rounded font-medium ${priorityClass(t.priority)}">${escapeHtml(t.priority)}</span>` : ""}
                  ${t.due_date ? `<span>Due: ${escapeHtml(formatDate(t.due_date))}</span>` : ""}
                  ${renderTodoTags(t.tags)}
                  ${t.goal_id ? `<span class="text-sky-400">Goal #${t.goal_id}</span>` : ""}
                </div>
              </div>
              <button class="text-red-400 hover:text-red-300 text-sm px-2 py-1" type="button" data-action="delete-todo" data-todo-id="${t.id}" title="Delete task">✕</button>
            </div>
          `,
          )
          .join("");

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Tasks</h2>
        <p class="panel-subtitle text-slate-400">Manage your tasks and priorities</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}

        <div class="bg-slate-900/50 p-4 rounded-lg mb-6">
          <h3 class="font-medium text-slate-200 mb-3">Add New Task</h3>
          <div class="space-y-3">
            <div>
              <label class="block text-sm font-medium text-slate-200 mb-1">Title</label>
              <input type="text" id="todo-title" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" placeholder="What needs to be done?" />
            </div>
            <div>
              <label class="block text-sm font-medium text-slate-200 mb-1">Description</label>
              <textarea id="todo-description" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" rows="2" placeholder="Optional description"></textarea>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Due Date</label>
                <input type="date" id="todo-due" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" />
              </div>
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Priority</label>
                <select id="todo-priority" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500">
                  <option value="low">Low</option>
                  <option value="medium" selected>Medium</option>
                  <option value="high">High</option>
                </select>
              </div>
            </div>
            <div>
              <label class="block text-sm font-medium text-slate-200 mb-1">Tags (comma-separated)</label>
              <input type="text" id="todo-tags" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" placeholder="work, personal, urgent" />
            </div>
            <button class="button" type="button" data-action="create-todo">Add Task</button>
          </div>
        </div>

        <div class="flex items-center space-x-3 mb-4">
          <button class="${s.filter === "all" ? "button-outline active" : "button-outline"}" type="button" data-todo-filter="all">All</button>
          <button class="${s.filter === "active" ? "button-outline active" : "button-outline"}" type="button" data-todo-filter="active">Active</button>
          <button class="${s.filter === "completed" ? "button-outline active" : "button-outline"}" type="button" data-todo-filter="completed">Completed</button>
          <span class="text-sm text-slate-400 ml-auto">${s.items.length} task${s.items.length !== 1 ? "s" : ""}</span>
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
    return `<span class="text-sky-400 text-xs">${arr.join(", ")}</span>`;
  } catch {
    return "";
  }
}

function priorityClass(p: string): string {
  switch (p) {
    case "high":
      return "bg-red-500/20 text-red-400";
    case "medium":
      return "bg-yellow-500/20 text-yellow-400";
    case "low":
      return "bg-green-500/20 text-green-400";
    default:
      return "bg-slate-500/20 text-slate-400";
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

// ---- Receipt render functions ----

function renderReceiptList(): string {
  const items = state.receipts.receipts
    .map(
      (r) => `
        <button class="note-list-item ${state.receipts.selectedId === r.id ? "note-list-item-active" : ""}" type="button" data-receipt-id="${r.id}">
          <div class="note-list-item-title">${escapeHtml(r.merchant || r.receipt_id || `Receipt #${r.id}`)}</div>
          <div class="note-list-item-meta">
            ${r.date ? `<span>${escapeHtml(formatDate(r.date))}</span>` : ""}
            ${r.total ? `<span class="font-mono text-sky-400">${escapeHtml(r.total)}</span>` : ""}
            <span class="receipt-status-badge receipt-status-${escapeHtml(r.status || "pending")}">${escapeHtml(r.status || "pending")}</span>
          </div>
          <div class="note-list-item-meta">
            <span class="truncate text-sky-400/70">${escapeHtml(r.current_path)}</span>
          </div>
        </button>
      `,
    )
    .join("");

  if (!items) {
    return `<div class="p-4 text-center text-sm text-slate-500">No receipts captured yet.</div>`;
  }
  return `<div class="note-list-scroll">${items}</div>`;
}

function renderReceiptDetail(): string {
  const r = state.receipts.receipts.find((x) => x.id === state.receipts.selectedId);
  if (!r) {
    return `<div class="flex items-center justify-center h-full text-sm text-slate-500">Receipt not found</div>`;
  }

  const statusColor =
    r.status === "archived" ? "text-amber-400" :
    r.status === "reviewed" ? "text-emerald-400" :
    "text-slate-400";

  return `
    <div class="space-y-4">
      <div class="rounded border border-slate-800 bg-slate-900/30 p-4">
        <h3 class="text-sm font-medium text-slate-200 mb-3">Receipt Info</h3>
        <div class="grid grid-cols-2 gap-3 text-sm">
          <div><span class="text-slate-500">ID:</span> <span class="text-slate-200">${escapeHtml(r.receipt_id || `#${r.id}`)}</span></div>
          <div><span class="text-slate-500">Status:</span> <span class="${statusColor}">${escapeHtml(r.status || "pending")}</span></div>
          ${r.merchant ? `<div class="col-span-2"><span class="text-slate-500">Merchant:</span> <span class="text-slate-200">${escapeHtml(r.merchant)}</span></div>` : ""}
          ${r.total ? `<div><span class="text-slate-500">Total:</span> <span class="text-slate-200 font-mono">${escapeHtml(r.total)}</span></div>` : ""}
          ${r.date ? `<div><span class="text-slate-500">Date:</span> <span class="text-slate-200">${escapeHtml(r.date)}</span></div>` : ""}
          ${r.category ? `<div><span class="text-slate-500">Category:</span> <span class="text-slate-200">${escapeHtml(r.category)}</span></div>` : ""}
        </div>
      </div>

      <div class="rounded border border-slate-800 bg-slate-900/30 p-4">
        <h3 class="text-sm font-medium text-slate-200 mb-3">File Paths</h3>
        <div class="space-y-2 text-sm">
          <div>
            <span class="text-slate-500">Original:</span>
            <div class="font-mono text-xs text-slate-300 mt-0.5 break-all">${escapeHtml(r.original_path)}</div>
          </div>
          <div>
            <span class="text-slate-500">Current:</span>
            <div class="font-mono text-xs text-sky-400 mt-0.5 break-all">${escapeHtml(r.current_path)}</div>
          </div>
          ${r.archived_path
            ? `<div>
                <span class="text-slate-500">Archived:</span>
                <div class="font-mono text-xs text-amber-400 mt-0.5 break-all">${escapeHtml(r.archived_path)}</div>
              </div>`
            : ""}
          ${r.parsed_json_path
            ? `<div>
                <span class="text-slate-500">Parsed JSON:</span>
                <div class="font-mono text-xs text-slate-300 mt-0.5 break-all">${escapeHtml(r.parsed_json_path)}</div>
              </div>`
            : ""}
          ${r.checksum
            ? `<div>
                <span class="text-slate-500">Checksum:</span>
                <div class="font-mono text-xs text-slate-400 mt-0.5">${escapeHtml(r.checksum)}</div>
              </div>`
            : ""}
        </div>
      </div>

      <div class="rounded border border-slate-800 bg-slate-900/30 p-4">
        <h3 class="text-sm font-medium text-slate-200 mb-3">Timestamps</h3>
        <div class="grid grid-cols-2 gap-2 text-sm">
          <div><span class="text-slate-500">Created:</span> <span class="text-slate-300">${escapeHtml(formatDate(r.created_at))}</span></div>
          ${r.processed_at
            ? `<div><span class="text-slate-500">Processed:</span> <span class="text-slate-300">${escapeHtml(formatDate(r.processed_at))}</span></div>`
            : `<div><span class="text-slate-500">Processed:</span> <span class="text-slate-500">not yet</span></div>`}
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
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Loading...</div>`;
  } else if (s.status === "error" && s.statusMessage) {
    statusHtml = `<div class="rounded border border-red-700 bg-red-900/30 p-3 text-sm text-red-300">${escapeHtml(s.statusMessage)}</div>`;
  }

  // If a receipt is selected, show detail
  if (s.selectedId) {
    return `
      <section class="panel">
        <div class="panel-header">
          <h2 class="panel-title">Receipt Details</h2>
          <p class="panel-subtitle text-slate-400">
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
        <p class="panel-subtitle text-slate-400">Track and manage your expenses</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}
        
        <div class="receipt-controls flex justify-between items-center mb-6">
          <div class="flex items-center space-x-3">
            <span class="text-sm text-slate-400">${s.receipts.length} receipt${s.receipts.length !== 1 ? "s" : ""}</span>
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
        <button class="note-list-item ${state.notes.selectedId === note.id ? "note-list-item-active" : ""}" type="button" data-note-id="${note.id}">
          <div class="note-list-item-title">${escapeHtml(note.title || "Untitled")}</div>
          <div class="note-list-item-meta">
            <span>${escapeHtml(formatDate(note.updated_at))}</span>
            ${note.tags && note.tags !== "[]"
              ? `<span class="note-list-item-tags">${escapeHtml(parseTags(note.tags).join(", "))}</span>`
              : ""}
          </div>
        </button>
      `,
    )
    .join("");

  if (!items) {
    return `<div class="p-4 text-center text-sm text-slate-500">No notes yet.</div>`;
  }
  return `<div class="note-list-scroll">${items}</div>`;
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
    return `<div class="flex items-center justify-center h-full text-sm text-slate-500">Select a note to edit</div>`;
  }

  const s = state.notes;
  const status = s.status;

  let statusHtml = "";
  if (status === "loading") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-2 text-xs text-slate-300">Loading...</div>`;
  } else if (status === "saving") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-2 text-xs text-slate-300">Saving...</div>`;
  } else if (status === "error" && s.statusMessage) {
    statusHtml = `<div class="rounded border border-red-700 bg-red-900/30 p-2 text-xs text-red-300">${escapeHtml(s.statusMessage)}</div>`;
  }

  return `
    <div class="note-editor">
      ${statusHtml}
      <div class="mb-3">
        <input
          type="text"
          id="note-title-input"
          class="w-full rounded border border-slate-700 bg-slate-900 px-3 py-2 text-sm font-medium text-slate-100 placeholder-slate-500 focus:border-slate-500 focus:outline-none"
          placeholder="Note title"
          value="${escapeHtml(s.editTitle)}"
          data-bind="note-title"
        />
      </div>
      <div class="note-editor-textarea-wrapper">
        <textarea
          id="note-content-input"
          class="h-64 w-full resize-y rounded border border-slate-700 bg-slate-950 p-3 font-mono text-sm text-slate-100 placeholder-slate-600 focus:border-slate-500 focus:outline-none"
          placeholder="Write your note in Markdown..."
          data-bind="note-content"
        >${escapeHtml(s.editContent)}</textarea>
      </div>
      <div class="mt-3">
        <input
          type="text"
          id="note-tags-input"
          class="w-full rounded border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 placeholder-slate-500 focus:border-slate-500 focus:outline-none"
          placeholder="Tags (comma separated)"
          value="${escapeHtml(s.editTags)}"
          data-bind="note-tags"
        />
      </div>
      <div class="mt-4 flex items-center gap-3">
        <button class="button" type="button" data-action="save-note" ${status === "saving" || status === "loading" ? "disabled" : ""}>
          ${status === "saving" ? "Saving..." : "Save"}
        </button>
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
          <p class="panel-subtitle text-slate-400">
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
        <p class="panel-subtitle text-slate-400">Capture and organize your knowledge</p>
      </div>
      <div class="panel-content space-y-6">
        <div class="note-controls flex justify-between items-center mb-6">
          <div class="flex items-center space-x-3">
            <button class="button secondary" type="button" data-action="create-note">
              <span class="mr-2">+</span> New Note
            </button>
          </div>
          <span class="text-sm text-slate-400">${s.notes.length} note${s.notes.length !== 1 ? "s" : ""}</span>
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
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Loading...</div>`;
  } else if (s.status === "saving") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Saving...</div>`;
  } else if (s.status === "error" && s.statusMessage) {
    statusHtml = `<div class="rounded border border-red-700 bg-red-900/30 p-3 text-sm text-red-300">${escapeHtml(s.statusMessage)}</div>`;
  }

  const goalItems =
    s.items.length === 0
      ? `<div class="text-center py-8 text-slate-500">No goals yet. Create your first goal above!</div>`
      : s.items
          .map(
            (g) => `
            <div class="rounded border border-slate-800 bg-slate-900/30 p-4">
              <div class="flex items-start justify-between gap-3">
                <div class="flex-1 min-w-0">
                  <h4 class="font-medium text-slate-100">${escapeHtml(g.title)}</h4>
                  ${g.description ? `<p class="text-sm text-slate-400 mt-1">${escapeHtml(g.description)}</p>` : ""}
                </div>
                <span class="receipt-status-badge ${goalStatusClass(g.status || "active")}">${escapeHtml(g.status || "active")}</span>
              </div>
              <div class="mt-3 flex items-center gap-3 text-sm text-slate-400">
                ${g.deadline ? `<span>Due: ${escapeHtml(formatDate(g.deadline))}</span>` : ""}
                <span>${taskCountByGoal[g.id] || 0} task${taskCountByGoal[g.id] !== 1 ? "s" : ""}</span>
              </div>
              <div class="mt-3 flex items-center gap-3">
                <div class="flex-1 bg-slate-800 rounded-full h-2 overflow-hidden">
                  <div class="h-full rounded-full transition-all duration-300 ${g.progress >= 100 ? "bg-emerald-500" : "bg-sky-500"}" style="width: ${Math.min(100, Math.max(0, g.progress))}%"></div>
                </div>
                <span class="text-xs text-slate-400 font-mono w-10 text-right">${g.progress}%</span>
                <input type="range" min="0" max="100" value="${g.progress}" class="w-24 accent-sky-500" data-action="goal-progress" data-goal-id="${g.id}" />
              </div>
              <div class="mt-3 flex items-center gap-2">
                <button class="button secondary text-xs px-3 py-1" type="button" data-action="delete-goal" data-goal-id="${g.id}">Delete</button>
              </div>
            </div>
          `,
          )
          .join("");

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Goals</h2>
        <p class="panel-subtitle text-slate-400">Track your objectives and progress</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}

        <div class="bg-slate-900/50 p-4 rounded-lg mb-6">
          <h3 class="font-medium text-slate-200 mb-3">Add New Goal</h3>
          <div class="space-y-3">
            <div>
              <label class="block text-sm font-medium text-slate-200 mb-1">Title</label>
              <input type="text" id="goal-title" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" placeholder="What is your goal?" />
            </div>
            <div>
              <label class="block text-sm font-medium text-slate-200 mb-1">Description</label>
              <textarea id="goal-description" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" rows="2" placeholder="Optional description"></textarea>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Status</label>
                <select id="goal-status" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500">
                  <option value="active">Active</option>
                  <option value="on-hold">On Hold</option>
                  <option value="done">Done</option>
                </select>
              </div>
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Deadline</label>
                <input type="date" id="goal-deadline" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" />
              </div>
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Progress (0-100)</label>
                <input type="number" id="goal-progress" min="0" max="100" value="0" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" />
              </div>
            </div>
            <button class="button" type="button" data-action="create-goal">Create Goal</button>
          </div>
        </div>

        <div class="flex items-center justify-between mb-4">
          <h3 class="font-medium text-slate-200">Active Goals</h3>
          <span class="text-sm text-slate-400">${s.items.length} goal${s.items.length !== 1 ? "s" : ""}</span>
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
      return "bg-sky-900/30 text-sky-400";
    case "on-hold":
      return "bg-amber-900/30 text-amber-400";
    case "done":
      return "bg-emerald-900/30 text-emerald-400";
    default:
      return "bg-slate-700/50 text-slate-400";
  }
}

// ---- URLs screen ----

function renderUrls(): string {
  const s = state.urls;

  let statusHtml = "";
  if (s.status === "loading") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Loading...</div>`;
  } else if (s.status === "saving") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Saving...</div>`;
  } else if (s.status === "error" && s.statusMessage) {
    statusHtml = `<div class="rounded border border-red-700 bg-red-900/30 p-3 text-sm text-red-300">${escapeHtml(s.statusMessage)}</div>`;
  }

  const newCount = s.items.filter((u) => u.is_new === 1).length;

  const urlItems =
    s.items.length === 0
      ? `<div class="text-center py-8 text-slate-500">No saved URLs yet. Add one above!</div>`
      : s.items
          .map(
            (u) => `
            <div class="flex items-start gap-3 p-3 bg-slate-900/50 rounded-lg hover:bg-slate-900/70 transition-colors">
              ${u.is_new === 1 ? `<span class="mt-1 inline-block w-2 h-2 rounded-full bg-sky-400 shrink-0" title="New"></span>` : `<span class="mt-1 inline-block w-2 h-2 rounded-full bg-transparent shrink-0"></span>`}
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-medium text-slate-100 truncate">${escapeHtml(u.title || u.url)}</span>
                  ${u.is_new === 1 ? `<span class="receipt-status-badge bg-sky-900/30 text-sky-400">new</span>` : ""}
                </div>
                <div class="text-xs text-sky-400 truncate mt-0.5">${escapeHtml(u.url)}</div>
                <div class="flex flex-wrap gap-2 mt-1 text-xs text-slate-500">
                  ${u.source ? `<span>${escapeHtml(u.source)}</span>` : ""}
                  ${renderUrlTags(u.tags)}
                  <span>${escapeHtml(formatDate(u.created_at))}</span>
                </div>
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <button class="text-sky-400 hover:text-sky-300 text-sm px-2 py-1" type="button" data-action="open-url" data-url-id="${u.id}" title="Open in browser">↗</button>
                ${u.is_new === 1 ? `<button class="text-slate-400 hover:text-slate-300 text-xs px-2 py-1" type="button" data-action="mark-url-read" data-url-id="${u.id}" title="Mark as read">✓</button>` : ""}
                <button class="text-red-400 hover:text-red-300 text-sm px-2 py-1" type="button" data-action="delete-url" data-url-id="${u.id}" title="Delete URL">✕</button>
              </div>
            </div>
          `,
          )
          .join("");

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Saved URLs</h2>
        <p class="panel-subtitle text-slate-400">Links captured from Chrome or added manually</p>
      </div>
      <div class="panel-content space-y-6">
        ${statusHtml}

        ${newCount > 0 ? `<div class="rounded border border-sky-800 bg-sky-900/20 p-3 text-sm text-sky-300">${newCount} new URL${newCount !== 1 ? "s" : ""} — click ✓ to mark as read, or ↗ to open</div>` : ""}

        <div class="bg-slate-900/50 p-4 rounded-lg mb-6">
          <h3 class="font-medium text-slate-200 mb-3">Add New URL</h3>
          <div class="space-y-3">
            <div>
              <label class="block text-sm font-medium text-slate-200 mb-1">URL</label>
              <input type="url" id="url-url" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" placeholder="https://example.com" />
            </div>
            <div>
              <label class="block text-sm font-medium text-slate-200 mb-1">Title (optional)</label>
              <input type="text" id="url-title" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" placeholder="Page title" />
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Source</label>
                <select id="url-source" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500">
                  <option value="manual">Manual</option>
                  <option value="chrome-extension">Chrome Extension</option>
                  <option value="import">Import</option>
                </select>
              </div>
              <div>
                <label class="block text-sm font-medium text-slate-200 mb-1">Tags (comma-separated)</label>
                <input type="text" id="url-tags" class="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md focus:outline-none focus:border-slate-500" placeholder="dev, article, reference" />
              </div>
            </div>
            <button class="button" type="button" data-action="create-url">Save URL</button>
          </div>
        </div>

        <div class="flex items-center justify-between mb-4">
          <h3 class="font-medium text-slate-200">Saved Links</h3>
          <span class="text-sm text-slate-400">${s.items.length} URL${s.items.length !== 1 ? "s" : ""}</span>
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
    return `<span class="text-sky-400">${arr.join(", ")}</span>`;
  } catch {
    return "";
  }
}

function renderSettings(): string {
  const status = state.settings.status;

  let statusHtml = "";
  if (status === "loading") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Loading configuration...</div>`;
  } else if (status === "saving") {
    statusHtml = `<div class="rounded border border-slate-700 bg-slate-900/70 p-3 text-sm text-slate-300">Saving configuration...</div>`;
  } else if (status === "saved") {
    statusHtml = `<div class="rounded border border-emerald-700 bg-emerald-900/30 p-3 text-sm text-emerald-300">${escapeHtml(state.settings.statusMessage)}</div>`;
  } else if (status === "error") {
    statusHtml = `<div class="rounded border border-red-700 bg-red-900/30 p-3 text-sm text-red-300">${escapeHtml(state.settings.statusMessage)}</div>`;
  }

  const editorContent = state.settings.configJson
    ? state.settings.configJson
    : '{\n  "receipts_path": "",\n  "nutriments_path": "",\n  "bank_statements_path": "",\n  "investments_statements_path": "",\n  "database": "maia.db",\n  "models_api": []\n}';

  return `
    <section class="panel">
      <div class="panel-header">
        <h2 class="panel-title">Settings</h2>
        <p class="panel-subtitle text-slate-400">Configure your Maia workspace</p>
      </div>
      <div class="panel-content space-y-6">
        <div class="rounded border border-slate-800 bg-slate-900/30 p-4">
          <h3 class="mb-3 text-sm font-medium text-slate-200">maia.json Editor</h3>
          <p class="mb-4 text-xs text-slate-400">
            Edit your Maia configuration directly. The file is validated as JSON before saving.
            A backup will be created as <code class="rounded bg-slate-800 px-1 py-0.5 font-mono text-slate-300">maia.json.bak</code>.
          </p>

          ${statusHtml}

          <div class="mt-4">
            <textarea
              id="settings-editor"
              class="h-72 w-full resize-y rounded border border-slate-700 bg-slate-950 p-3 font-mono text-sm text-slate-100 placeholder-slate-600 focus:border-slate-500 focus:outline-none"
              spellcheck="false"
              placeholder="{ ... }"
            >${escapeHtml(editorContent)}</textarea>
          </div>

          <div class="mt-4 flex items-center gap-3">
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
      </div>
    </section>
  `;
}

function handleInput(event: Event): void {
  const target = event.target as HTMLElement;

  if (target.id === "note-title-input") {
    state.notes.editTitle = (target as HTMLInputElement).value;
  } else if (target.id === "note-content-input") {
    state.notes.editContent = (target as HTMLTextAreaElement).value;
  } else if (target.id === "note-tags-input") {
    state.notes.editTags = (target as HTMLInputElement).value;
  }

  // Handle goal progress slider
  const progressSlider = target.closest<HTMLElement>("[data-action='goal-progress']");
  if (progressSlider?.dataset.goalId) {
    const id = Number(progressSlider.dataset.goalId);
    const val = parseFloat((target as HTMLInputElement).value);
    if (!isNaN(val)) {
      updateGoalProgress(id, val);
    }
    return;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  render();
  document.addEventListener("click", handleClick);
  document.addEventListener("input", handleInput);
});

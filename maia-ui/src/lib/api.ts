import { invoke } from "@tauri-apps/api/core";

/**
 * Typed wrappers over every Tauri command the desktop app registers.
 *
 * The contract is frozen (see src-tauri/src/lib.rs `generate_handler!`): these
 * wrappers only rename the payloads, they never change what crosses the bridge.
 * Two response shapes exist on the Rust side — the `{success, data?, error?}`
 * envelope used by the config/notes/receipt/task/url commands, and raw values
 * returned directly by the FitFat-style list commands.
 */

export interface Envelope<T> {
  success: boolean;
  data?: T;
  error?: string;
}

// ---- Shared record shapes ----

export interface Note {
  id: number;
  title: string;
  path: string;
  tags: string;
  created_at: string;
  updated_at: string;
}

export interface NoteWithContent extends Note {
  content: string;
}

/** Matches the `receipts` table in `sync_server::db` (v30 schema). */
export interface Receipt {
  id: string;
  localPath: string;
  remotePath: string | null;
  /** 0 local | 1 uploading | 2 uploaded | 3 error | 4 parsing */
  uploadStatus: number;
  parsed: boolean;
  /** OCR result as a JSON *string*, not a nested object. */
  parsedJson: string | null;
  parseError: string | null;
  transactionId: string | null;
  archivedPath: string | null;
  archivedAt: number | null;
  /** Epoch milliseconds. */
  createdAt: number;
  updatedAt: number;
}

export interface SavedUrl {
  id: string;
  title: string | null;
  url: string;
  source: string | null;
  tags: string;
  is_new: boolean;
  created_at: string;
}

export interface Todo {
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

export interface Goal {
  id: number;
  title: string;
  description: string | null;
  status: string | null;
  deadline: string | null;
  progress: number;
  created_at: string;
  updated_at: string;
}

export interface DashboardCounts {
  tasks: number;
  goals: number;
  receipts: number;
  notes: number;
  urls: number;
}

export interface SyncInfo {
  port: number;
  localUrl: string;
  lanUrl: string;
  lanIp: string;
  apiKey: string;
  isListening: boolean;
}

export interface ConfigResult extends Envelope<string> {}

export interface Exercise {
  id: string;
  name: string;
  body_part?: string | null;
  bodyPart?: string | null;
  equipment?: string | null;
  primary_muscle?: string | null;
  primaryMuscle?: string | null;
  secondary_muscle?: string | null;
  secondaryMuscle?: string | null;
  instructions?: string | null;
  tips?: string | null;
  faqs?: string | null;
  image_path?: string | null;
  hasImage?: boolean;
}

export interface TemplateExerciseBlock {
  id: string;
  exerciseId: string;
  exerciseName?: string;
  notes?: string | null;
  setNumber?: number;
  sets: TemplateSet[];
}

export interface TemplateSet {
  id: string;
  setNumber: number;
  reps: number | null;
  weightKg: number | null;
  restSeconds: number | null;
  durationMinutes: number | null;
  distanceMeters: number | null;
}

export interface TemplateDetail {
  template: { id: string; name: string; notes?: string | null; recurrence?: string | null };
  exercises: TemplateExerciseBlock[];
}

// ---- Envelope-style commands ----

export const api = {
  readConfig: () => invoke<Envelope<string>>("read_config"),

  saveConfig: (json: string) => invoke<Envelope<void>>("save_config", { json }),

  listNotes: () => invoke<Envelope<Note[]>>("list_notes"),

  readNote: (id: string) => invoke<Envelope<NoteWithContent>>("read_note", { id }),

  listReceipts: () => invoke<Envelope<Receipt[]>>("list_receipts"),

  readReceipt: (id: string) => invoke<Envelope<Receipt>>("read_receipt", { id }),

  readReceiptPicture: (id: string) =>
    invoke<Envelope<{ base64: string; contentType: string }>>("read_receipt_picture", { id }),

  archiveReceipt: (id: string) => invoke<Envelope<void>>("archive_receipt", { id }),

  restoreReceipt: (id: string) => invoke<Envelope<void>>("restore_receipt", { id }),

  listTasks: () => invoke<Envelope<Todo[]>>("list_tasks"),

  listGoals: () => invoke<Envelope<Goal[]>>("list_goals"),

  getCounts: () => invoke<Envelope<DashboardCounts>>("get_counts"),

  listUrls: () => invoke<Envelope<SavedUrl[]>>("list_urls"),

  createUrl: (args: { title: string | null; url: string; source: string; tags: string }) =>
    invoke<Envelope<SavedUrl>>("create_url", args),

  deleteUrl: (id: string) => invoke<Envelope<void>>("delete_url", { id }),

  markUrlRead: (id: string) => invoke<Envelope<void>>("mark_url_read", { id }),

  // ---- Raw-value commands (FitFat style) ----

  syncInfo: () => invoke<SyncInfo>("sync_info"),

  listWorkouts: () => invoke<unknown>("list_workouts"),

  listTemplates: () => invoke<unknown>("list_templates"),

  getTemplate: (id: string) => invoke<unknown>("get_template", { id }),

  createTemplate: (args: { id: string; name: string }) => invoke<unknown>("create_template", args),

  updateTemplate: (args: { id: string; name: string; notes: string | null; recurrence: string | null }) =>
    invoke<unknown>("update_template", args),

  deleteTemplate: (id: string) => invoke<unknown>("delete_template", { id }),

  addTemplateExercise: (args: { templateId: string; exerciseId: string }) =>
    invoke<unknown>("add_template_exercise", args),

  updateTemplateExercise: (args: { id: string; notes: string | null }) =>
    invoke<unknown>("update_template_exercise", args),

  deleteTemplateExercise: (id: string) => invoke<unknown>("delete_template_exercise", { id }),

  addTemplateSet: (args: {
    templateExerciseId: string;
    reps: number | null;
    weightKg: number | null;
    restSeconds: number | null;
    durationMinutes: number | null;
    distanceMeters: number | null;
  }) => invoke<unknown>("add_template_set", args),

  updateTemplateSet: (args: {
    id: string;
    reps: number | null;
    weightKg: number | null;
    restSeconds: number | null;
    durationMinutes: number | null;
    distanceMeters: number | null;
  }) => invoke<unknown>("update_template_set", args),

  deleteTemplateSet: (id: string) => invoke<unknown>("delete_template_set", { id }),

  listExercises: () => invoke<unknown>("list_exercises"),

  exerciseMedia: (args: { id: string; video: boolean }) =>
    invoke<unknown>("exercise_media", args),

  listIngredients: () => invoke<unknown>("list_ingredients"),

  createIngredient: (args: {
    name: string;
    caloriesPer100g: number;
    proteinPer100g: number;
    carbsPer100g: number;
    fatPer100g: number;
    fiberPer100g: number | null;
    sugarPer100g: number | null;
    sodiumPer100g: number | null;
    brand: string | null;
    barcode: string | null;
  }) => invoke<unknown>("create_ingredient", args),

  updateIngredient: (args: {
    id: string;
    name: string;
    caloriesPer100g: number;
    proteinPer100g: number;
    carbsPer100g: number;
    fatPer100g: number;
  }) => invoke<unknown>("update_ingredient", args),

  listFoods: () => invoke<unknown>("list_foods"),

  createFood: (args: { name: string; components: { ingredientId: string; amount: number }[] }) =>
    invoke<unknown>("create_food", args),

  updateFood: (args: {
    id: string;
    name: string;
    components: { ingredientId: string; amount: number }[];
  }) => invoke<unknown>("update_food", args),

  archiveFood: (id: string) => invoke<unknown>("archive_food", { id }),

  restoreFood: (id: string) => invoke<unknown>("restore_food", { id }),

  listStores: () => invoke<unknown>("list_stores"),

  createStore: (args: { name: string }) => invoke<unknown>("create_store", args),

  listMeals: () => invoke<unknown>("list_meals"),

  listBodyMetrics: () => invoke<unknown>("list_body_metrics"),

  listExperiments: () => invoke<unknown>("list_experiments"),

  listTags: () => invoke<unknown>("list_tags"),

  listAccounts: () => invoke<unknown>("list_accounts"),

  listTransactions: () => invoke<unknown>("list_transactions"),

  listFxRates: (args: { base: string; date: string }) => invoke<unknown>("list_fx_rates", args),

  createFxRate: (args: { code: string; baseCode: string; rateDate: string; rateToBase: number }) =>
    invoke<unknown>("create_fx_rate", args),
};

/**
 * Unwrap the two response shapes the backend uses. Envelope responses keep
 * their `success` flag so callers can render the error; raw list responses are
 * normalized to an array (they arrive as an array, `{data}`, or `{items}`).
 */
export function unwrapList(res: unknown): any[] {
  const d = (res as { data?: unknown })?.data ?? res;
  if (Array.isArray(d)) return d;
  const items = (d as { items?: unknown; meals?: unknown })?.items ?? (d as { meals?: unknown })?.meals;
  return Array.isArray(items) ? items : [];
}

export function isEnvelopeFailure(res: unknown): string | null {
  const r = res as { success?: boolean; error?: string } | null;
  if (r && r.success === false) return r.error || "unknown error";
  return null;
}

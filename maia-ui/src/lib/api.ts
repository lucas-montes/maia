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

/** One row of the desktop `list_accounts` command. */
export interface FinanceAccount {
  id: string;
  name: string;
  type: string;
  openingBalance: number;
  note: string | null;
  createdAt: number;
  /** Non-null for accounts a statement import created. */
  source: string | null;
  externalId: string | null;
  /** Opening balance plus the net of every transaction touching the account. */
  balance: number;
}

/** One row of the desktop `list_transactions` command. */
export interface FinanceTransaction {
  id: string;
  type: "expense" | "income" | "transfer" | string;
  amount: number;
  currencyCode: string;
  amountBase: number;
  rateUsed: number;
  category: string | null;
  date: number;
  note: string | null;
  receiptId: string | null;
  isDraft: boolean;
  accountId: string | null;
  toAccountId: string | null;
  accountName: string | null;
  toAccountName: string | null;
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

/** A staged statement section or provider bucket, as `GET /statements` returns it. */
export interface StatementSummary {
  id: string;
  source: string;
  institution: string;
  accountName: string;
  accountNumber: string | null;
  accountId: string | null;
  resolvedAccountName: string | null;
  currencyCode: string;
  periodStart: string | null;
  periodEnd: string | null;
  openingBalance: number | null;
  closingBalance: number | null;
  status: string;
  warnings: string[];
  parseError: string | null;
  createdAt: number;
  updatedAt: number;
  lineCount: number;
  pendingCount: number;
  importedCount: number;
  /** The file the statement was staged from; statements of one dropped file share it. */
  sourcePath?: string | null;
}

export interface StatementLine {
  id: string;
  sourceRef: string | null;
  postedAt: number;
  description: string;
  amount: number;
  currencyCode: string;
  balanceAfter: number | null;
  categoryHint: string | null;
  lineKind: string;
  suggestedType: string | null;
  suggestedCategory: string | null;
  metadata: unknown;
  status: string;
  transactionId: string | null;
  importError: string | null;
  duplicateOf: string | null;
}

export interface TransferMatchLine {
  id: string;
  description: string;
  amount: number;
  currencyCode: string;
  postedAt: number;
  status: string;
}

export interface TransferMatch {
  id: string;
  debitLineId: string;
  creditLineId: string;
  transactionId: string | null;
  confidence: number;
  reason: string | null;
  status: string;
  /** The counterpart is included even when it belongs to another statement. */
  debit: TransferMatchLine;
  credit: TransferMatchLine;
  /** Account names of both sides, so a transfer can say what moved where. */
  debitAccountName?: string | null;
  creditAccountName?: string | null;
}

export interface StatementDetail {
  /** The detail endpoint omits the list-only counters; lines carry the counts. */
  statement: Omit<StatementSummary, "lineCount" | "pendingCount" | "importedCount"> & {
    sourcePath: string | null;
    storedPath: string | null;
    parsed: unknown;
  };
  lines: StatementLine[];
  matches: TransferMatch[];
}

export interface StatementImportLine {
  id: string;
  action?: "import" | "skip";
  type?: "expense" | "income" | "transfer";
  category?: string | null;
  /** Transfer lines: the account on the other side. Omitted means "no
   *  counterpart is known" — the server then books the line as a plain
   *  expense/income instead of pointing it at an invented account. */
  counterpartAccountId?: string | null;
}

export interface StatementFeedStatus {
  provider: string;
  enabled: boolean;
  environment: string;
  lookbackDays: number;
  credentialsSet: boolean;
  status: string;
  lastSyncedAt: number | null;
  lastAttemptAt: number | null;
  lastError: string | null;
}

export interface StatementFeedSettings {
  trading212: {
    enabled: boolean;
    environment: string;
    lookbackDays: number;
    apiKeySet: boolean;
    apiSecretSet: boolean;
  };
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
  id: string;
  date: number;
  title: string;
  done: number;
  taskStatus: number | null;
  carryOver: boolean;
  sortOrder: number;
  dueDate: number | null;
  notes: string | null;
  createdAt: number;
  updatedAt: number;
}

/** Row shape of `list_tasks_range` — wide enough to feed `recurrence.ts`'s
 *  `expandRecurrence`/`buildOccurrences` (unlike `Todo`, this carries the
 *  recurrence rule, series id, and all three time-of-day fields). */
export interface CalendarTask {
  id: string;
  date: number;
  title: string;
  done: number;
  taskStatus: number | null;
  dueDate: number | null;
  dueTimeMinutes: number | null;
  startTimeMinutes: number | null;
  endTimeMinutes: number | null;
  notes: string | null;
  /** Raw JSON TEXT column; parse with `parseRecurrence` from `recurrence.ts`. */
  recurrence: string | null;
  seriesId: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface Workout {
  id: string;
  name: string;
  date: number;
  startedAt: number | null;
  completedAt: number | null;
  notes: string | null;
  routineId: string | null;
  templateId: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface ExerciseSet {
  id: string;
  setNumber: number;
  reps: number | null;
  weightKg: number | null;
  restSeconds: number | null;
  actualReps: number | null;
  actualWeightKg: number | null;
  actualRestSeconds: number | null;
  durationMinutes: number | null;
  distanceMeters: number | null;
  actualDurationMinutes: number | null;
  actualDistanceMeters: number | null;
  notes: string | null;
  completedAt: number | null;
}

export interface WorkoutExercise {
  id: string;
  exerciseId: string;
  exerciseName: string;
  sortOrder: number;
  notes: string | null;
  sets: ExerciseSet[];
}

export interface WorkoutDetail {
  id: string;
  name: string;
  date: number;
  startedAt: number | null;
  completedAt: number | null;
  notes: string | null;
  routineId: string | null;
  templateId: string | null;
  createdAt: number;
  updatedAt: number;
  exercises: WorkoutExercise[];
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

// ---- Finance scenarios ----

export interface CategoryOverride {
  category: string;
  monthlyAmount: number;
}

/** Matches `finance_scenarios::Assumptions` on the server, which is also the
    literal shape stored in `finance_scenarios.assumptions_json`. */
export interface ScenarioAssumptions {
  horizonMonths: number;
  incomeDeltaMonthly: number;
  categoryOverrides: CategoryOverride[];
  inflationAnnualPct: number;
  investmentReturnAnnualPct: number;
}

export interface FinanceScenario {
  id: string;
  name: string;
  assumptions: ScenarioAssumptions;
  createdAt: number;
  updatedAt: number;
}

export interface FinanceBaseline {
  windowMonths: number;
  monthlyIncome: number;
  categoryMonthly: CategoryOverride[];
  totalMonthlyExpense: number;
  currentNetWorth: number;
}

export interface ProjectionPoint {
  month: number;
  netWorth: number;
  monthlyExpenseTotal: number;
}

export interface ProjectionSummary {
  endingNetWorth: number;
  totalNetSaved: number;
  finalMonthlyExpenseTotal: number;
}

export interface ScenarioProjection {
  baseline: {
    windowMonths: number;
    monthlyIncome: number;
    totalMonthlyExpense: number;
    currentNetWorth: number;
  };
  projection: {
    series: ProjectionPoint[];
    summary: ProjectionSummary;
  };
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

  /** Tasks for the calendar view: everything due/dated within `[start, end]`
   *  (epoch ms, inclusive), plus every recurring anchor for client-side
   *  expansion — see `lib/recurrence.ts`. */
  listTasksRange: (args: { start: number; end: number }) =>
    invoke<Envelope<CalendarTask[]>>("list_tasks_range", args),

  listGoals: () => invoke<Envelope<Goal[]>>("list_goals"),

  getCounts: () => invoke<Envelope<DashboardCounts>>("get_counts"),

  listUrls: () => invoke<Envelope<SavedUrl[]>>("list_urls"),

  createUrl: (args: { title: string | null; url: string; source: string; tags: string }) =>
    invoke<Envelope<SavedUrl>>("create_url", args),

  deleteUrl: (id: string) => invoke<Envelope<void>>("delete_url", { id }),

  markUrlRead: (id: string) => invoke<Envelope<void>>("mark_url_read", { id }),

  // ---- Raw-value commands (FitFat style) ----

  syncInfo: () => invoke<SyncInfo>("sync_info"),

  listWorkouts: () => invoke<Envelope<Workout>>("list_workouts"),

  getWorkout: (id: string) => invoke<Envelope<WorkoutDetail>>("get_workout", { id }),

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

  // ---- Ingredient price tracking (T02/T03/T05/T06) ----

  addIngredientPrice: (args: {
    ingredientId: string;
    storeId: string;
    price: number;
    currencyCode: string;
    packageGrams: number | null;
    recordedAt: number;
    sourceUrl: string | null;
  }) => invoke<unknown>("add_ingredient_price", args),

  matchReceiptItem: (args: {
    ingredientId: string;
    merchant: string;
    price: number;
    currencyCode: string;
    packageGrams: number | null;
    recordedAt: number;
    sourceUrl: string | null;
  }) => invoke<unknown>("match_receipt_item", args),

  /** Cheapest-per-nutrient-gram comparison across every ingredient for the
   *  period (week/month/year) containing `atMs`. */
  nutrientPriceComparison: (args: { period: "week" | "month" | "year"; nutrient: string; atMs: number }) =>
    invoke<unknown>("nutrient_price_comparison", args),

  /** A logged meal's estimated cost, or `data: null` when nothing in it has
   *  ever been priced. */
  mealCostEstimate: (mealId: string) => invoke<unknown>("meal_cost_estimate", { mealId }),

  /** The day/week/month aggregate "estimated food spend" rollup. */
  foodSpendRollup: (args: { bucket: "day" | "week" | "month"; sinceMs: number; untilMs: number }) =>
    invoke<unknown>("food_spend_rollup", args),

  listFoods: () => invoke<unknown>("list_foods"),

  createFood: (args: { name: string; components: { ingredientId: string; amount: number }[]; cookedWeight: number }) =>
    invoke<unknown>("create_food", args),

  updateFood: (args: {
    id: string;
    name: string;
    components: { ingredientId: string; amount: number }[];
    cookedWeight: number;
  }) => invoke<unknown>("update_food", args),

  archiveFood: (id: string) => invoke<unknown>("archive_food", { id }),

  restoreFood: (id: string) => invoke<unknown>("restore_food", { id }),

  listStores: () => invoke<unknown>("list_stores"),

  createStore: (args: { name: string }) => invoke<unknown>("create_store", args),

  listMeals: () => invoke<unknown>("list_meals"),

  listBodyMetrics: () => invoke<unknown>("list_body_metrics"),


  listTags: () => invoke<unknown>("list_tags"),

  listAccounts: () => invoke<unknown>("list_accounts"),

  listTransactions: () => invoke<unknown>("list_transactions"),

  /** The deployment's base currency, so finance rows do not show
   *  `500 EUR → 500 base (rate 1)` for entries that never needed converting. */
  getBaseCurrency: () => invoke<Envelope<string>>("get_base_currency"),

  listFxRates: (args: { base: string; date: string }) => invoke<unknown>("list_fx_rates", args),

  createFxRate: (args: { code: string; baseCode: string; rateDate: string; rateToBase: number }) =>
    invoke<unknown>("create_fx_rate", args),

  // ---- Insights charts ----

  exerciseProgression: (exerciseId: string) => invoke<unknown>("exercise_progression", { exerciseId }),

  exerciseVolume: (period: string) => invoke<unknown>("exercise_volume", { period }),

  nutritionCalories: (period: string) => invoke<unknown>("nutrition_calories", { period }),

  nutritionMacros: (period: string) => invoke<unknown>("nutrition_macros", { period }),

  financeCategoryTotals: (period: string) => invoke<unknown>("finance_category_totals", { period }),

  financeAccountBalance: (accountId: string, period: string) =>
    invoke<unknown>("finance_account_balance", { accountId, period }),

  // ---- Finance scenarios ----

  listFinanceScenarios: () => invoke<Envelope<{ items: FinanceScenario[] }>>("list_finance_scenarios"),

  createFinanceScenario: (args: { name: string; assumptions: ScenarioAssumptions }) =>
    invoke<Envelope<FinanceScenario>>("create_finance_scenario", args),

  updateFinanceScenario: (args: { id: string; name: string; assumptions: ScenarioAssumptions }) =>
    invoke<Envelope<FinanceScenario>>("update_finance_scenario", args),

  deleteFinanceScenario: (id: string) =>
    invoke<Envelope<{ id: string }>>("delete_finance_scenario", { id }),

  getFinanceBaseline: (windowMonths?: number) =>
    invoke<Envelope<FinanceBaseline>>("get_finance_baseline", { windowMonths }),

  /** Projects a saved scenario (`scenarioId`) or an unsaved draft
      (`assumptions`) — exactly one of the two should be given. */
  projectFinanceScenario: (args: {
    scenarioId?: string | null;
    assumptions?: ScenarioAssumptions | null;
    windowMonths?: number | null;
  }) => invoke<Envelope<ScenarioProjection>>("project_finance_scenario", args),

  // ---- Statements ----

  listStatements: (args: { status?: string; source?: string } = {}) =>
    invoke<Envelope<{ items: StatementSummary[] }>>("list_statements", args),

  getStatement: (id: string) =>
    invoke<Envelope<StatementDetail>>("get_statement", { id }),

  /** Every statement one dropped file produced, with lines and matches. */
  getStatementFile: (path: string) =>
    invoke<Envelope<{ statements: StatementDetail[] }>>("get_statement_file", { path }),

  importStatement: (args: {
    id: string;
    lines: StatementImportLine[];
    autoLink?: boolean;
  }) => invoke<Envelope<unknown>>("import_statement", args),

  /** Imports pending lines of several statements in one server transaction. */
  importStatements: (
    statements: { id: string; lines: StatementImportLine[]; autoLink?: boolean }[],
  ) => invoke<Envelope<unknown>>("import_statements", { statements }),

  /** Soft-deletes several statements in one server transaction. */
  deleteStatements: (ids: string[]) =>
    invoke<Envelope<{ deleted: string[] }>>("delete_statements", { ids }),

  ignoreStatement: (id: string) =>
    invoke<Envelope<unknown>>("ignore_statement", { id }),

  deleteStatement: (id: string) =>
    invoke<Envelope<unknown>>("delete_statement", { id }),

  scanStatements: () => invoke<Envelope<unknown>>("scan_statements"),

  /** Drops picked statement files into the dump folder of one source. The
   *  server's watcher stages them; nothing is parsed here. */
  addStatementFiles: (
    source: string,
    files: { name: string; data: string }[],
  ) => invoke<Envelope<string[]>>("add_statement_files", { source, files }),

  /** Uploads one or many picked receipt pictures to the server's OCR
   *  endpoint, one request per file, and reports per-file: `id` when the
   *  server accepted it (then parse-polling takes over), else `error`. */
  uploadReceiptFiles: (files: { name: string; data: string }[]) =>
    invoke<Envelope<{ name: string; id: string | null; error: string | null }[]>>(
      "upload_receipt_files",
      { files },
    ),

  /** Creates the draft expense a parsed receipt stands for (phone-parity:
   *  upload → OCR → draft) and links receipt → transaction. Idempotent. */
  createReceiptTransaction: (id: string) =>
    invoke<Envelope<{ created: boolean; reason?: string; transactionId?: string }>>(
      "create_receipt_transaction",
      { id },
    ),

  listStatementFeeds: () =>
    invoke<Envelope<{ items: StatementFeedStatus[] }>>("list_statement_feeds"),

  getStatementFeedSettings: () =>
    invoke<Envelope<StatementFeedSettings>>("get_statement_feed_settings"),

  saveStatementFeedSettings: (settings: unknown) =>
    invoke<Envelope<unknown>>("save_statement_feed_settings", { settings }),

  syncStatementFeed: (provider: string) =>
    invoke<Envelope<unknown>>("sync_statement_feed", { provider }),
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

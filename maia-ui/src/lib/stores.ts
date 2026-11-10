import { atom } from "nanostores";
import type {
  CalendarTask,
  DashboardCounts,
  Exercise,
  FinanceBaseline,
  FinanceScenario,
  Goal,
  Note,
  Receipt,
  SavedUrl,
  SyncInfo,
  TemplateDetail,
  Todo,
  WorkoutDetail,
} from "./api";

export type Status = "idle" | "loading" | "saving" | "saved" | "error";

export interface ListState<T> {
  items: T[];
  status: Status;
  message: string;
}

export function emptyList<T>(): ListState<T> {
  return { items: [], status: "idle", message: "" };
}

// ---- Shell / navigation ----

export type ViewId =
  | "dashboard"
  | "exercises"
  | "workouts"
  | "templates"
  | "ingredients"
  | "priceComparison"
  | "foods"
  | "stores"
  | "meals"
  | "bodyMetrics"
  | "todos"
  | "calendar"
  | "receipts"
  | "notes"
  | "settings"
  | "goals"
  | "urls"
  | "sync"
  | "tags"
  | "accounts"
  | "transactions"
  | "statements"
  | "fxRates"
  | "insights"
  | "simulations";

export const routeAtom = atom<ViewId>("dashboard");
export const sidebarCollapsedAtom = atom(false);
export const sidebarGroupsAtom = atom<Record<string, boolean>>({
  Training: true,
  Nutrition: true,
  Health: true,
  Planning: true,
  Finance: true,
  System: true,
});

export const countsAtom = atom<DashboardCounts | null>(null);

// ---- Per-domain stores ----

export const todosAtom = atom<ListState<Todo> & { filter: "all" | "active" | "completed" }>({
  ...emptyList<Todo>(),
  filter: "all",
});

export type CalendarGranularity = "month" | "week" | "day";

/** Calendar view state. `selectedDate` is the anchor day for whichever grid
 *  is showing (epoch ms, local start-of-day) and survives granularity
 *  switches so drilling in/out never loses the user's place. `upNext` is
 *  computed independently of the visible range — it always looks forward
 *  from "now", regardless of which month/week/day is on screen. */
export const calendarAtom = atom<{
  granularity: CalendarGranularity;
  selectedDate: number;
  tasks: CalendarTask[];
  status: Status;
  message: string;
  upNext: CalendarTask[];
  upNextStatus: Status;
}>({
  granularity: "month",
  selectedDate: Date.now(),
  tasks: [],
  status: "idle",
  message: "",
  upNext: [],
  upNextStatus: "idle",
});

export const goalsAtom = atom<ListState<Goal>>(emptyList<Goal>());

export const notesAtom = atom<
  ListState<Note> & {
    selectedId: number | null;
    editTitle: string;
    editContent: string;
    editTags: string;
  }
>({
  ...emptyList<Note>(),
  selectedId: null,
  editTitle: "",
  editContent: "",
  editTags: "",
});

export const receiptsAtom = atom<ListState<Receipt> & { selectedId: string | null }>({
  ...emptyList<Receipt>(),
  selectedId: null,
});

export const urlsAtom = atom<ListState<SavedUrl>>(emptyList<SavedUrl>());

export const syncAtom = atom<
  ListState<any> & {
    info: SyncInfo | null;
    qrDataUrl: string | null;
  }
>({ items: [], status: "idle", message: "", info: null, qrDataUrl: null });

export const workoutsAtom = atom<{
  items: any[];
  status: Status;
  message: string;
  detailId: string | null;
  detail: WorkoutDetail | null;
  detailStatus: Status;
  detailMessage: string;
}>({
  items: [],
  status: "idle",
  message: "",
  detailId: null,
  detail: null,
  detailStatus: "idle",
  detailMessage: "",
});

export const exercisesAtom = atom<
  ListState<Exercise> & {
    filtered: Exercise[];
    search: string;
    bodyPart: string;
    equipment: string;
    hasImage: string;
    selectedId: string | null;
  }
>({
  ...emptyList<Exercise>(),
  filtered: [],
  search: "",
  bodyPart: "",
  equipment: "",
  hasImage: "",
  selectedId: null,
});

export const ingredientsAtom = atom<
  ListState<any> & { search: string; selectedId: string | null; showAddModal: boolean }
>({
  ...emptyList<any>(),
  search: "",
  selectedId: null,
  showAddModal: false,
});

export const priceComparisonAtom = atom<
  ListState<any> & { period: "week" | "month" | "year"; nutrient: string }
>({
  ...emptyList<any>(),
  period: "month",
  nutrient: "protein",
});

export const foodsAtom = atom<
  ListState<any> & {
    editorOpen: boolean;
    editingId: string | null;
    editingName: string;
    editingCookedWeight: string;
    draft: { ingredientId: string; amount: number }[];
    search: string;
  }
>({
  ...emptyList<any>(),
  editorOpen: false,
  editingId: null,
  editingName: "",
  editingCookedWeight: "",
  draft: [],
  search: "",
});

export const storesAtom = atom<ListState<any>>(emptyList<any>());

export const mealsAtom = atom<ListState<any>>(emptyList<any>());

export const bodyMetricsAtom = atom<ListState<any>>(emptyList<any>());

export const tagsAtom = atom<ListState<any>>(emptyList<any>());

export const accountsAtom = atom<ListState<any>>(emptyList<any>());

export const transactionsAtom = atom<ListState<any> & { search: string }>({
  ...emptyList<any>(),
  search: "",
});

export const statementsAtom = atom<
  ListState<any> & {
    selectedId: string | null;
    detail: any | null;
    filter: string;
    source: string;
    selectedLines: string[];
    /** Statements of one dropped file, when the file review page is open. */
    fileReview: { path: string; statements: any[] } | null;
    /** What importing the current selection would do, from the server. `null`
     *  while the first preview is in flight; it is re-fetched whenever the
     *  selection or an account override changes, so the summary on screen is
     *  always the summary the button will produce. */
    preview: any | null;
    /** True while a re-preview is in flight, so the summary can say it is
     *  catching up rather than silently showing stale numbers. */
    previewing: boolean;
    /** Per-statement account override picked in the review, keyed by statement
     *  id — sent as `accountId` with the import and the preview alike. */
    accountOverrides: Record<string, string>;
    /** Statement ids picked on the list page for bulk import/delete. */
    selectedStatements: string[];
    /** Last source the drop form placed files under, so the dropdown stays on it. */
    lastDropSource: string;
  }
>({
  ...emptyList<any>(),
  selectedId: null,
  detail: null,
  filter: "awaiting",
  source: "",
  selectedLines: [],
  fileReview: null,
  preview: null,
  previewing: false,
  accountOverrides: {},
  selectedStatements: [],
  lastDropSource: localStorage.getItem("maia.statements.lastDropSource") ?? "credit_agricole",
});

/** Scenario id used for the always-available "no changes" reference line —
    it never collides with a real scenario id (those are UUIDs). */
export const BASELINE_PSEUDO_ID = "__baseline__";

export const simulationsAtom = atom<
  ListState<FinanceScenario> & {
    baseline: FinanceBaseline | null;
    windowMonths: number;
    /** Scenario ids selected for the overlay chart; `BASELINE_PSEUDO_ID` is
        the always-selectable "no changes" reference line. */
    selectedIds: string[];
    /** Projection results keyed by scenario id (or `BASELINE_PSEUDO_ID`). */
    projections: Record<string, { series: { month: number; netWorth: number; monthlyExpenseTotal: number }[]; summary: { endingNetWorth: number; totalNetSaved: number; finalMonthlyExpenseTotal: number } }>;
    editorOpen: boolean;
    editingId: string | null;
    editingName: string;
    editingHorizonMonths: string;
    editingIncomeDeltaMonthly: string;
    editingInflationAnnualPct: string;
    editingInvestmentReturnAnnualPct: string;
    editingCategoryOverrides: { category: string; monthlyAmount: number }[];
  }
>({
  ...emptyList<FinanceScenario>(),
  baseline: null,
  windowMonths: 12,
  selectedIds: [BASELINE_PSEUDO_ID],
  projections: {},
  editorOpen: false,
  editingId: null,
  editingName: "",
  editingHorizonMonths: "24",
  editingIncomeDeltaMonthly: "0",
  editingInflationAnnualPct: "0",
  editingInvestmentReturnAnnualPct: "0",
  editingCategoryOverrides: [],
});

export const fxRatesAtom = atom<ListState<any> & { base: string; date: string }>({
  ...emptyList<any>(),
  base: "USD",
  date: new Date().toISOString().slice(0, 10),
});

export const settingsAtom = atom<{
  configJson: string;
  configFields: Record<string, string>;
  status: Status;
  message: string;
}>({
  configJson: "",
  configFields: {},
  status: "idle",
  message: "",
});

export const templatesAtom = atom<{
  items: any[];
  status: Status;
  message: string;
  editingId: string | null;
  selectedId: string | null;
  detail: TemplateDetail | null;
  detailLoading: boolean;
  exSearch: string;
}>({
  items: [],
  status: "idle",
  message: "",
  editingId: null,
  selectedId: null,
  detail: null,
  detailLoading: false,
  exSearch: "",
});

/** Media data URLs for exercise images/videos, filled lazily. */
export const exerciseImagesAtom = atom<Record<string, string>>({});
export const exerciseVideosAtom = atom<Record<string, string>>({});

import { atom } from "nanostores";
import type {
  DashboardCounts,
  Exercise,
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
  | "foods"
  | "stores"
  | "meals"
  | "bodyMetrics"
  | "experiments"
  | "todos"
  | "receipts"
  | "notes"
  | "settings"
  | "goals"
  | "urls"
  | "sync"
  | "tags"
  | "accounts"
  | "transactions"
  | "fxRates";

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

export const foodsAtom = atom<
  ListState<any> & {
    editorOpen: boolean;
    editingId: string | null;
    editingName: string;
    draft: { ingredientId: string; amount: number }[];
    search: string;
  }
>({
  ...emptyList<any>(),
  editorOpen: false,
  editingId: null,
  editingName: "",
  draft: [],
  search: "",
});

export const storesAtom = atom<ListState<any>>(emptyList<any>());

export const mealsAtom = atom<ListState<any>>(emptyList<any>());

export const bodyMetricsAtom = atom<ListState<any>>(emptyList<any>());

export const experimentsAtom = atom<ListState<any>>(emptyList<any>());

export const tagsAtom = atom<ListState<any>>(emptyList<any>());

export const accountsAtom = atom<ListState<any>>(emptyList<any>());

export const transactionsAtom = atom<ListState<any> & { search: string }>({
  ...emptyList<any>(),
  search: "",
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

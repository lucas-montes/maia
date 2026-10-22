import { api, type WorkoutDetail } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { workoutsAtom } from "../lib/stores";
import { backToDashboard, panelClose, panelOpen, statusBanner } from "./shared";
import { isEnvelopeFailure, unwrapList } from "../lib/api";

function exerciseBlock(
  x: { id: string; exerciseName: string; notes: string | null; sets: WorkoutDetail["exercises"][number]["sets"] },
): string {
  const setCell = (v: number | null, unit: string): string =>
    v === null ? `<span class="text-ink-600">–</span>` : `<span class="font-mono">${v}</span>${unit ? `<span class="ml-0.5 text-ink-500">${unit}</span>` : ""}`;
  return `
    <div class="rounded border border-surface-800 p-3">
      <div class="mb-2 flex items-center gap-2">
        <div class="flex-1 text-sm font-medium text-ink-100">${escapeHtml(x.exerciseName)}</div>
      </div>
      ${
        x.sets.length
          ? `<div class="space-y-1">
              <div class="flex items-center gap-3 text-xs font-medium uppercase tracking-wide text-ink-500">
                <span class="w-8">Set</span>
                <span class="w-16">Reps</span>
                <span class="w-20">kg</span>
                <span class="w-20">Rest</span>
                <span class="w-20">Min</span>
                <span class="w-20">m</span>
              </div>
              ${x.sets
                .map(
                  (s) => `
                  <div class="flex items-center gap-3 text-sm">
                    <span class="w-8 text-ink-400">${s.setNumber}</span>
                    <span class="w-16">${setCell(s.actualReps ?? s.reps, "")}</span>
                    <span class="w-20">${setCell(s.actualWeightKg ?? s.weightKg, "kg")}</span>
                    <span class="w-20">${setCell(s.actualRestSeconds ?? s.restSeconds, "s")}</span>
                    <span class="w-20">${setCell(s.actualDurationMinutes ?? s.durationMinutes, "min")}</span>
                    <span class="w-20">${setCell(s.actualDistanceMeters ?? s.distanceMeters, "m")}</span>
                    ${s.notes ? `<span class="truncate text-xs text-ink-500">${escapeHtml(s.notes)}</span>` : ""}
                  </div>`,
                )
                .join("")}
            </div>`
          : `<div class="text-xs text-ink-500">No sets recorded</div>`
      }
      ${x.notes ? `<div class="mt-2 text-xs text-ink-400">${escapeHtml(x.notes)}</div>` : ""}
    </div>`;
}

function workoutDetail(d: WorkoutDetail): string {
  const fmt = (ms: number | null): string => (ms ? new Date(ms).toLocaleDateString() : "—");
  return `
    <div class="flex items-center gap-3">
      <button class="btn btn-ghost btn-sm" type="button" data-action="workout-back">← Back to list</button>
      <span class="text-sm text-ink-400">${fmt(d.date)}</span>
      ${d.completedAt ? `<span class="badge badge-sm badge-ok">completed</span>` : ""}
    </div>
    ${
      d.exercises.length
        ? `<div class="space-y-2">${d.exercises.map(exerciseBlock).join("")}</div>`
        : `<div class="text-xs text-ink-400">No exercises recorded</div>`
    }`;
}

/** Workouts — read-only list plus a per-workout detail (managed on mobile). */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    <div data-slot="content"></div>
  `;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;

  const render = (): void => {
    const s = workoutsAtom.get();
    if (s.detailId) {
      setHtml(
        content,
        `
        ${panelOpen(s.detail?.name || "Workout", "Read-only — managed on mobile")}
          ${statusBanner(s)}
          <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
            ${s.detailStatus === "error" ? `<div class="text-sm text-err-400">${escapeHtml(s.detailMessage || "Couldn't load workout.")}</div>` : ""}
            ${s.detail ? workoutDetail(s.detail) : `<div class="text-xs text-ink-500">${s.detailStatus === "loading" ? "Loading workout…" : ""}</div>`}
          </div>
        ${panelClose}`,
      );
    } else {
      const w = s.items;
      setHtml(
        content,
        `
        ${panelOpen("Workouts", "From maia.db workouts • per-tab sync")}
          ${statusBanner(s)}
          <div class="mb-2 text-xs text-ink-500" data-slot="count">${w.length} workouts</div>
          <div class="min-h-0 flex-1 space-y-2 overflow-auto" data-slot="list">
            ${
              w.slice(0, 50)
                .map(
                  (x) => `
                  <button class="list-row w-full text-left rounded border border-surface-800 bg-base-100 p-3" type="button" data-workout-id="${x.id}">
                    <div class="text-sm text-ink-100">${escapeHtml(x.name)} — ${new Date(x.date).toLocaleDateString()}</div>
                    <div class="text-xs text-ink-400">${escapeHtml(x.notes || "")}</div>
                  </button>`,
                )
                .join("") || `<div class="text-xs text-ink-400">No workouts</div>`
            }
          </div>
          <div>${backToDashboard()}</div>
        ${panelClose}`,
      );
    }
    refreshIcons(content);
  };

  const openDetail = async (id: string): Promise<void> => {
    workoutsAtom.set({ ...workoutsAtom.get(), detailId: id, detail: null, detailStatus: "loading", detailMessage: "" });
    try {
      const res = await api.getWorkout(id);
      if (res.success && res.data) {
        workoutsAtom.set({ ...workoutsAtom.get(), detail: res.data, detailStatus: "idle", detailMessage: "" });
      } else {
        workoutsAtom.set({ ...workoutsAtom.get(), detailStatus: "error", detailMessage: res.error ?? "Failed to load workout" });
      }
    } catch (e) {
      workoutsAtom.set({ ...workoutsAtom.get(), detailStatus: "error", detailMessage: `Failed to load workout: ${e}` });
    }
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    if (target.closest<HTMLElement>("[data-action='workout-back']")) {
      workoutsAtom.set({ ...workoutsAtom.get(), detailId: null, detail: null, detailStatus: "idle", detailMessage: "" });
      return;
    }
    const row = target.closest<HTMLElement>("[data-workout-id]");
    if (row?.dataset.workoutId) void openDetail(row.dataset.workoutId);
  });

  const load = async (): Promise<void> => {
    try {
      const res = await api.listWorkouts();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        workoutsAtom.set({ ...workoutsAtom.get(), items: [], status: "error", message: failure });
        return;
      }
      workoutsAtom.set({ ...workoutsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      workoutsAtom.set({ ...workoutsAtom.get(), items: [], status: "error", message: String(e) });
    }
  };

  void load();
  const unsub = workoutsAtom.subscribe(render);
  return unsub;
}

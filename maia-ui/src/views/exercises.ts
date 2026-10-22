import { api, type Exercise } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { exercisesAtom } from "../lib/stores";
import { emptyState, panelClose, panelOpen } from "./shared";

const mediaCache = new Map<string, string | null>();

async function loadExerciseMedia(id: string, video: boolean): Promise<string | null> {
  const key = `${video ? "v" : "i"}:${id}`;
  if (mediaCache.has(key)) return mediaCache.get(key) ?? null;
  try {
    const res = await api.exerciseMedia({ id, video });
    const data = (res as { data?: { dataUrl?: string } })?.data ?? (res as { dataUrl?: string });
    const url = data?.dataUrl ?? null;
    mediaCache.set(key, url);
    return url;
  } catch {
    mediaCache.set(key, null);
    return null;
  }
}

// Media arrives after the list paints, so a slow image never holds up the list.
const exerciseImages: Record<string, string> = {};
const exerciseVideos: Record<string, string> = {};

async function prefetchExerciseMedia(ids: string[]): Promise<void> {
  for (const id of ids) {
    if (exerciseImages[id] !== undefined) continue;
    exerciseImages[id] = (await loadExerciseMedia(id, false)) ?? "";
    if (id === exercisesAtom.get().selectedId) {
      const video = await loadExerciseMedia(id, true);
      exerciseVideos[id] = video ?? "";
      exercisesAtom.set({ ...exercisesAtom.get() });
    } else {
      exercisesAtom.set({ ...exercisesAtom.get() });
    }
  }
}

function applyFilter(s: ReturnType<typeof exercisesAtom.get>): void {
  const q = s.search.toLowerCase();
  s.filtered = s.items.filter((e) => {
    if (q && !`${e.name} ${e.body_part || e.bodyPart || ""} ${e.equipment || ""} ${e.primary_muscle || e.primaryMuscle || ""}`.toLowerCase().includes(q)) return false;
    if (s.bodyPart && (e.body_part || e.bodyPart) !== s.bodyPart) return false;
    if (s.equipment && e.equipment !== s.equipment) return false;
    if (s.hasImage === "yes" && !e.image_path && !e.hasImage) return false;
    if (s.hasImage === "no" && (e.image_path || e.hasImage)) return false;
    return true;
  });
}

function instructionsList(sel: Exercise): string {
  try {
    const a = JSON.parse(sel.instructions ?? "null");
    if (Array.isArray(a)) {
      return `<ol class="ml-4 list-decimal space-y-1 text-xs text-ink-400">${a.map((s: string) => `<li>${escapeHtml(s)}</li>`).join("")}</ol>`;
    }
  } catch {
    // fall through to plain text
  }
  return sel.instructions ? `<p class="text-xs text-ink-400">${escapeHtml(String(sel.instructions))}</p>` : "";
}

function tipsList(sel: Exercise): string {
  try {
    const a = JSON.parse(sel.tips ?? "null");
    if (Array.isArray(a)) {
      return `<ul class="ml-4 list-disc text-xs text-ink-400">${a.map((s: string) => `<li>${escapeHtml(s)}</li>`).join("")}</ul>`;
    }
  } catch {
    // no tips
  }
  return "";
}

/** Exercises — searchable catalog with lazy per-item media and a detail pane. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `<div data-slot="content"></div>`;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;

  const render = (): void => {
    const s = exercisesAtom.get();
    if (s.status === "loading") {
      setHtml(content, `${panelOpen("Exercises")}<div class="p-6 text-sm text-ink-400">Loading exercises...</div>${panelClose}`);
      return;
    }
    const bodyParts = [...new Set(s.items.map((e) => e.body_part || e.bodyPart).filter(Boolean))] as string[];
    const equipments = [...new Set(s.items.map((e) => e.equipment).filter(Boolean))] as string[];
    const list = (s.filtered.length ? s.filtered : s.items).slice(0, 100);
    const sel = s.selectedId ? s.items.find((e) => e.id === s.selectedId) : null;

    setHtml(
      content,
      `
      ${panelOpen(`Exercises — ${s.items.length}`, "Catalog from data/ parsed • editable • lazy media")}
        <div class="flex flex-wrap gap-2">
          <input id="ex-search" placeholder="Search name/body/equipment" value="${escapeHtml(s.search)}" class="input input-bordered min-w-[200px] flex-1" />
          <select id="ex-bodypart" class="select select-bordered">
            <option value="">All body parts</option>
            ${bodyParts.map((b) => `<option ${s.bodyPart === b ? "selected" : ""}>${escapeHtml(b)}</option>`).join("")}
          </select>
          <select id="ex-equip" class="select select-bordered">
            <option value="">All equipment</option>
            ${equipments.map((b) => `<option ${s.equipment === b ? "selected" : ""}>${escapeHtml(b)}</option>`).join("")}
          </select>
          <select id="ex-hasimg" class="select select-bordered">
            <option value="">All</option>
            <option value="yes" ${s.hasImage === "yes" ? "selected" : ""}>Has image</option>
            <option value="no" ${s.hasImage === "no" ? "selected" : ""}>No image</option>
          </select>
          <button class="btn btn-secondary btn-sm" data-action="ex-apply">Apply</button>
        </div>
        <div class="text-xs text-ink-500">maia.db • ${s.items.length} total • ${s.filtered.length || s.items.length} filtered • lazy media</div>
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <div class="min-h-0 flex-1 divide-y divide-surface-800 overflow-auto rounded border border-surface-800">
            ${list.map((e) => `
              <button class="flex w-full gap-3 p-3 text-left hover:bg-surface-800 ${s.selectedId === e.id ? "bg-surface-700/50" : ""}" data-action="ex-select" data-id="${escapeHtml(e.id)}">
                <img src="${exerciseImages[e.id] ?? ""}" loading="lazy" class="h-12 w-12 rounded bg-surface-700 object-cover" onerror="this.style.display='none'" />
                <div class="min-w-0">
                  <div class="truncate text-sm text-ink-100">${escapeHtml(e.name)}</div>
                  <div class="truncate text-xs text-ink-400">${escapeHtml((e.body_part || e.bodyPart || "") as string)} • ${escapeHtml((e.equipment || "") as string)} • ${escapeHtml((e.primary_muscle || e.primaryMuscle || "") as string)}</div>
                </div>
              </button>`).join("") || emptyState("No exercises yet.")}
          </div>
          <div class="rounded border border-surface-800 bg-base-100 p-4">
            ${sel ? `
              <div class="space-y-3">
                <div class="flex gap-3">
                  <img src="${exerciseImages[sel.id] ?? ""}" loading="lazy" class="h-24 w-24 rounded bg-surface-800" onerror="this.style.display='none'" />
                  <video src="${exerciseVideos[sel.id] ?? ""}" controls class="h-24 w-40 rounded bg-surface-800" onerror="this.style.display='none'"></video>
                  <div>
                    <h3 class="font-medium text-ink-100">${escapeHtml(sel.name)}</h3>
                    <p class="text-xs text-ink-400">${escapeHtml((sel.body_part || sel.bodyPart || "") as string)} • ${escapeHtml((sel.equipment || "") as string)}</p>
                    <p class="text-xs text-ink-400">Primary: ${escapeHtml((sel.primary_muscle || sel.primaryMuscle || "") as string)}</p>
                    <p class="text-xs text-ink-400">Secondary: ${escapeHtml((sel.secondary_muscle || sel.secondaryMuscle || "") as string)}</p>
                  </div>
                </div>
                ${instructionsList(sel)}
                ${tipsList(sel)}
                ${sel.faqs ? `<p class="whitespace-pre-wrap text-xs text-ink-400">${escapeHtml(sel.faqs)}</p>` : ""}
              </div>` : `<div class="text-sm text-ink-400">Select an exercise</div>`}
          </div>
        </div>
      ${panelClose}`,
    );
    refreshIcons(content);
  };

  const load = async (): Promise<void> => {
    exercisesAtom.set({ ...exercisesAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listExercises();
      const items = Array.isArray(res) ? res : ((res as { data?: unknown })?.data as Exercise[]) ?? ((res as { items?: unknown })?.items as Exercise[]) ?? [];
      const s = exercisesAtom.get();
      exercisesAtom.set({ ...s, items, status: "idle", message: "" });
      applyFilter(exercisesAtom.get());
      exercisesAtom.set({ ...exercisesAtom.get() });
      void prefetchExerciseMedia(exercisesAtom.get().filtered.slice(0, 60).map((e) => e.id));
    } catch (e) {
      exercisesAtom.set({ ...exercisesAtom.get(), status: "error", message: String(e) });
    }
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const select = target.closest<HTMLElement>("[data-action='ex-select']");
    if (select?.dataset.id) {
      const id = select.dataset.id;
      exercisesAtom.set({ ...exercisesAtom.get(), selectedId: id });
      if (exerciseVideos[id] === undefined) {
        void loadExerciseMedia(id, true).then((url) => {
          exerciseVideos[id] = url ?? "";
          exercisesAtom.set({ ...exercisesAtom.get() });
        });
      }
      return;
    }
    if (target.closest("[data-action='ex-apply']")) {
      const s = exercisesAtom.get();
      exercisesAtom.set({
        ...s,
        search: root.querySelector<HTMLInputElement>("#ex-search")?.value ?? "",
        bodyPart: root.querySelector<HTMLSelectElement>("#ex-bodypart")?.value ?? "",
        equipment: root.querySelector<HTMLSelectElement>("#ex-equip")?.value ?? "",
        hasImage: root.querySelector<HTMLSelectElement>("#ex-hasimg")?.value ?? "",
      });
      applyFilter(exercisesAtom.get());
      exercisesAtom.set({ ...exercisesAtom.get() });
    }
  });

  void load();
  const unsub = exercisesAtom.subscribe(render);
  return unsub;
}

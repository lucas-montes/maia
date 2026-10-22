import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { confirmDialog, toast } from "../lib/overlay";
import { templatesAtom, exercisesAtom } from "../lib/stores";
import { isEnvelopeFailure } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

function num(v: string | null | undefined): number | null {
  const s = (v ?? "").trim();
  if (!s) return null;
  const n = parseFloat(s);
  return isNaN(n) ? null : n;
}

function setNum(v: number | null): string {
  return v === null || v === undefined ? "" : String(v);
}

function val(root: HTMLElement, id: string): string | null {
  return root.querySelector<HTMLInputElement>(`#${CSS.escape(id)}`)?.value ?? null;
}

/** Templates — workout blueprints with full exercise/set CRUD. The list stays
    visible while a template detail is edited in the right pane. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Templates — editable", "Workout blueprints • recurrence • list stays visible while you edit")}
      <div data-slot="status"></div>
      <div class="flex gap-2">
        <input id="tpl-name" placeholder="New template name" class="input input-bordered flex-1" />
        <button class="btn btn-primary btn-sm" data-action="tpl-create">Create</button>
      </div>
      <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <div class="space-y-2" data-slot="rows"></div>
        <div data-slot="detail"></div>
      </div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const rowsEl = root.querySelector("[data-slot='rows']") as HTMLElement;
  const detailEl = root.querySelector("[data-slot='detail']") as HTMLElement;

  const render = (): void => {
    const s = templatesAtom.get();
    setHtml(statusEl, statusBanner(s));

    setHtml(
      rowsEl,
      s.items
        .map((x) => {
          if (s.editingId === x.id) {
            return `<div class="space-y-2 rounded border border-surface-700 p-3">
              <input id="tpl-edit-name" value="${escapeHtml(x.name || "")}" placeholder="Name" class="input input-bordered w-full" />
              <input id="tpl-edit-notes" value="${escapeHtml(x.notes || "")}" placeholder="Notes (optional)" class="input input-bordered w-full" />
              <input id="tpl-edit-recurrence" value="${escapeHtml(x.recurrence || "")}" placeholder="Recurrence rule (optional)" class="input input-bordered w-full" />
              <div class="flex gap-2">
                <button class="btn btn-primary btn-xs" data-action="tpl-save" data-id="${escapeHtml(x.id)}">Save</button>
                <button class="btn btn-ghost btn-xs" data-action="tpl-cancel">Cancel</button>
              </div>
            </div>`;
          }
          const selected = s.selectedId === x.id;
          return `<div class="rounded border ${selected ? "border-surface-600 bg-base-100" : "border-surface-800"} p-3">
            <div class="flex items-center gap-2">
              <div class="min-w-0 flex-1">
                <div class="text-sm text-ink-100">${escapeHtml(x.name)}</div>
                ${x.notes ? `<div class="text-xs text-ink-400">${escapeHtml(x.notes)}</div>` : ""}
                ${x.recurrence ? `<div class="text-xs text-ink-500">${escapeHtml(x.recurrence)}</div>` : ""}
              </div>
              ${selected ? "" : `<button class="btn btn-secondary btn-xs" data-action="tpl-open" data-id="${escapeHtml(x.id)}">Open ›</button>`}
              <button class="btn btn-ghost btn-xs" data-action="tpl-edit" data-id="${escapeHtml(x.id)}">Edit</button>
              <button class="btn btn-ghost btn-xs text-err-400" data-action="tpl-delete" data-id="${escapeHtml(x.id)}">Delete</button>
            </div>
          </div>`;
        })
        .join("") || `<div class="text-xs text-ink-400">No templates</div>`,
    );

    setHtml(detailEl, s.selectedId ? detailHtml(s) : `<div class="rounded border border-surface-800 p-6 text-center text-xs text-ink-500">Select a template on the left to edit its exercises and sets.</div>`);
    refreshIcons(root);
  };

  function detailHtml(s: ReturnType<typeof templatesAtom.get>): string {
    const d = s.detail;
    const head = d
      ? `<div><h2 class="panel-title">${escapeHtml(d.template.name)}</h2><p class="panel-subtitle">${d.template.notes ? escapeHtml(d.template.notes) + " • " : ""}${d.template.recurrence ? escapeHtml(d.template.recurrence) : "blueprint"}</p></div>`
      : `<div><h2 class="panel-title">Template</h2></div>`;

    const exercises = exercisesAtom.get().items;
    const exSearch = (s.exSearch || "").toLowerCase();
    const matches = exSearch ? exercises.filter((e) => (e.name || "").toLowerCase().includes(exSearch)).slice(0, 15) : [];

    const body = !d
      ? `<div class="text-xs text-ink-400">${s.detailLoading ? "Loading template…" : "Couldn't load template."}</div>`
      : `<div class="space-y-4">
          ${(d.exercises || [])
            .map((b) => {
              const sets = (b.sets || [])
                .map(
                  (st) => `
                  <div class="flex items-center gap-1">
                    <span class="w-6 text-xs text-ink-400">${st.setNumber}</span>
                    <input id="ts-${st.id}-reps" type="number" step="any" value="${setNum(st.reps)}" placeholder="–" class="input input-bordered input-xs w-14" />
                    <input id="ts-${st.id}-weight" type="number" step="any" value="${setNum(st.weightKg)}" placeholder="–" class="input input-bordered input-xs w-16" />
                    <input id="ts-${st.id}-rest" type="number" step="any" value="${setNum(st.restSeconds)}" placeholder="–" class="input input-bordered input-xs w-14" />
                    <input id="ts-${st.id}-dur" type="number" step="any" value="${setNum(st.durationMinutes)}" placeholder="–" class="input input-bordered input-xs w-14" />
                    <input id="ts-${st.id}-dist" type="number" step="any" value="${setNum(st.distanceMeters)}" placeholder="–" class="input input-bordered input-xs w-16" />
                    <button class="btn btn-ghost btn-xs px-2" data-action="tpl-delete-set" data-id="${escapeHtml(st.id)}">×</button>
                  </div>`,
                )
                .join("");
              return `<div class="space-y-2 rounded border border-surface-800 p-3">
                <div class="flex items-center gap-2">
                  <div class="flex-1 text-sm font-medium text-ink-100">${escapeHtml(b.exerciseName || b.exerciseId)}</div>
                  <button class="btn btn-ghost btn-xs text-err-400" data-action="tpl-remove-exercise" data-id="${escapeHtml(b.id)}">Remove</button>
                </div>
                <input id="tpl-exnotes-${b.id}" value="${escapeHtml(b.notes || "")}" placeholder="Exercise note (optional)" class="input input-bordered input-sm w-full" />
                <div class="text-xs text-ink-500">Set • reps • kg • rest s • min • m</div>
                ${sets || `<div class="text-xs text-ink-500">No sets yet.</div>`}
                <div class="flex gap-2">
                  <button class="btn btn-primary btn-xs" data-action="tpl-save-block" data-id="${escapeHtml(b.id)}">Save</button>
                  <button class="btn btn-secondary btn-xs" data-action="tpl-add-set" data-id="${escapeHtml(b.id)}">Add set</button>
                </div>
              </div>`;
            })
            .join("") || `<div class="text-xs text-ink-400">No exercises yet — add one below.</div>`}
          <div class="space-y-2 rounded border border-surface-800 p-3">
            <div class="text-xs font-medium text-ink-300">Add exercise</div>
            <div class="flex gap-2">
              <input id="tpl-ex-search" value="${escapeHtml(s.exSearch || "")}" placeholder="Search exercises" class="input input-bordered input-sm flex-1" />
              <button class="btn btn-secondary btn-xs" data-action="tpl-ex-search">Search</button>
            </div>
            <div class="max-h-48 space-y-1 overflow-auto">
              ${matches
                .map(
                  (e) => `
                  <div class="flex items-center gap-2 rounded border border-surface-800 px-2 py-1">
                    <div class="flex-1 truncate text-xs text-ink-200">${escapeHtml(e.name)}</div>
                    <button class="btn btn-primary btn-xs" data-action="tpl-add-exercise" data-id="${escapeHtml(e.id)}">Add</button>
                  </div>`,
                )
                .join("")}
            </div>
          </div>
        </div>`;

    return `<div class="space-y-3">
      <div class="flex items-start gap-2">
        <div class="min-w-0 flex-1">${head}</div>
        <button class="btn btn-ghost btn-xs shrink-0" data-action="tpl-back">Close</button>
      </div>
      ${body}
    </div>`;
  }

  const loadList = async (): Promise<void> => {
    try {
      const res = await api.listTemplates();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        templatesAtom.set({ ...templatesAtom.get(), status: "error", message: failure });
        return;
      }
      const data = (res as { data?: unknown })?.data ?? res;
      const items = Array.isArray(data) ? data : ((data as { items?: unknown })?.items as any[]) ?? [];
      templatesAtom.set({ ...templatesAtom.get(), items, status: "idle", message: "" });
    } catch (e) {
      templatesAtom.set({ ...templatesAtom.get(), status: "error", message: String(e) });
      toast(`Couldn't load templates: ${e}`, "err");
    }
  };

  const loadDetail = async (id: string): Promise<void> => {
    templatesAtom.set({ ...templatesAtom.get(), selectedId: id, detailLoading: true, detail: null, exSearch: "" });
    if (exercisesAtom.get().items.length === 0) {
      try {
        const res = await api.listExercises();
        const data = (res as { data?: unknown })?.data ?? res;
        exercisesAtom.set({
          ...exercisesAtom.get(),
          items: Array.isArray(data) ? data : ((data as { items?: unknown })?.items as any[]) ?? [],
          status: "idle",
          message: "",
        });
      } catch {
        // detail pane search will just be empty
      }
    }
    try {
      const res = await api.getTemplate(id);
      const failure = isEnvelopeFailure(res);
      if (failure) {
        toast(`Couldn't load template: ${failure}`, "err");
        templatesAtom.set({ ...templatesAtom.get(), detailLoading: false });
        return;
      }
      const data = (res as { data?: unknown; template?: unknown }) ?? {};
      const detail =
        (data as any).template && (data as any).exercises
          ? (data as unknown as { template: any; exercises: any[] })
          : ((data as { data?: { template: any; exercises: any[] } })?.data ?? null);
      templatesAtom.set({ ...templatesAtom.get(), detail, detailLoading: false });
    } catch (e) {
      toast(`Couldn't load template: ${e}`, "err");
      templatesAtom.set({ ...templatesAtom.get(), detailLoading: false });
    }
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const action = (name: string): HTMLElement | null => target.closest(`[data-action='${name}']`);

    if (action("tpl-create")) {
      const input = root.querySelector<HTMLInputElement>("#tpl-name");
      const name = input?.value.trim();
      if (name) {
        api
          .createTemplate({ id: crypto.randomUUID(), name })
          .then((res) => {
            if (isEnvelopeFailure(res)) {
              toast(`Couldn't create template: ${(res as { error?: string }).error || "unknown error"}`, "err");
              return;
            }
            if (input) input.value = "";
            void loadList();
          })
          .catch((e) => toast(`Couldn't create template: ${e}`, "err"));
      }
      return;
    }

    const edit = action("tpl-edit");
    if (edit?.dataset.id) {
      templatesAtom.set({ ...templatesAtom.get(), editingId: edit.dataset.id });
      return;
    }
    if (action("tpl-cancel")) {
      templatesAtom.set({ ...templatesAtom.get(), editingId: null });
      return;
    }

    const saveTpl = action("tpl-save");
    if (saveTpl?.dataset.id) {
      const id = saveTpl.dataset.id;
      const name = root.querySelector<HTMLInputElement>("#tpl-edit-name")?.value.trim() ?? "";
      const notes = root.querySelector<HTMLInputElement>("#tpl-edit-notes")?.value.trim() || null;
      const recurrence = root.querySelector<HTMLInputElement>("#tpl-edit-recurrence")?.value.trim() || null;
      if (!name) {
        toast("Template name can't be empty", "err");
        return;
      }
      api
        .updateTemplate({ id, name, notes, recurrence })
        .then((res) => {
          if (isEnvelopeFailure(res)) {
            toast(`Couldn't save template: ${(res as { error?: string }).error || "unknown error"}`, "err");
            return;
          }
          templatesAtom.set({ ...templatesAtom.get(), editingId: null });
          void loadList();
        })
        .catch((e) => toast(`Couldn't save template: ${e}`, "err"));
      return;
    }

    const del = action("tpl-delete");
    if (del?.dataset.id) {
      const id = del.dataset.id;
      const it = templatesAtom.get().items.find((x) => x.id === id);
      void confirmDialog("Delete template?", `Delete "${it?.name || id}" and all its exercises and sets? This can't be undone.`).then((ok) => {
        if (!ok) return;
        api
          .deleteTemplate(id)
          .then((res) => {
            if (isEnvelopeFailure(res)) {
              toast(`Couldn't delete template: ${(res as { error?: string }).error || "unknown error"}`, "err");
              return;
            }
            void loadList();
          })
          .catch((e) => toast(`Couldn't delete template: ${e}`, "err"));
      });
      return;
    }

    const open = action("tpl-open");
    if (open?.dataset.id) {
      templatesAtom.set({ ...templatesAtom.get(), editingId: null });
      void loadDetail(open.dataset.id);
      return;
    }
    if (action("tpl-back")) {
      templatesAtom.set({ ...templatesAtom.get(), selectedId: null, detail: null, exSearch: "" });
      void loadList();
      return;
    }

    if (action("tpl-ex-search")) {
      templatesAtom.set({ ...templatesAtom.get(), exSearch: root.querySelector<HTMLInputElement>("#tpl-ex-search")?.value ?? "" });
      return;
    }

    const addEx = action("tpl-add-exercise");
    if (addEx?.dataset.id) {
      const templateId = templatesAtom.get().selectedId;
      if (!templateId) return;
      api
        .addTemplateExercise({ templateId, exerciseId: addEx.dataset.id })
        .then(async (res) => {
          if (isEnvelopeFailure(res)) {
            toast(`Couldn't add exercise: ${(res as { error?: string }).error || "unknown error"}`, "err");
            return;
          }
          const blockId = (res as { data?: { id?: string }; id?: string })?.data?.id ?? (res as { id?: string })?.id;
          if (blockId) {
            await api
              .addTemplateSet({ templateExerciseId: blockId, reps: null, weightKg: null, restSeconds: null, durationMinutes: null, distanceMeters: null })
              .catch(() => null);
          }
          void loadDetail(templateId);
        })
        .catch((e) => toast(`Couldn't add exercise: ${e}`, "err"));
      return;
    }

    const rmEx = action("tpl-remove-exercise");
    if (rmEx?.dataset.id) {
      const templateId = templatesAtom.get().selectedId;
      if (!templateId) return;
      void confirmDialog("Remove exercise?", "Remove this exercise and all its sets from the template? This can't be undone.").then((ok) => {
        if (!ok) return;
        api
          .deleteTemplateExercise(rmEx.dataset.id as string)
          .then((res) => {
            if (isEnvelopeFailure(res)) {
              toast(`Couldn't remove exercise: ${(res as { error?: string }).error || "unknown error"}`, "err");
              return;
            }
            void loadDetail(templateId);
          })
          .catch((e) => toast(`Couldn't remove exercise: ${e}`, "err"));
      });
      return;
    }

    const addSet = action("tpl-add-set");
    if (addSet?.dataset.id) {
      const templateId = templatesAtom.get().selectedId;
      if (!templateId) return;
      const block = (templatesAtom.get().detail?.exercises || []).find((b) => b.id === addSet.dataset.id);
      const last = (block?.sets || []).slice(-1)[0];
      api
        .addTemplateSet({
          templateExerciseId: addSet.dataset.id,
          reps: last?.reps ?? null,
          weightKg: last?.weightKg ?? null,
          restSeconds: last?.restSeconds ?? null,
          durationMinutes: last?.durationMinutes ?? null,
          distanceMeters: last?.distanceMeters ?? null,
        })
        .then((res) => {
          if (isEnvelopeFailure(res)) {
            toast(`Couldn't add set: ${(res as { error?: string }).error || "unknown error"}`, "err");
            return;
          }
          void loadDetail(templateId);
        })
        .catch((e) => toast(`Couldn't add set: ${e}`, "err"));
      return;
    }

    const delSet = action("tpl-delete-set");
    if (delSet?.dataset.id) {
      const templateId = templatesAtom.get().selectedId;
      if (!templateId) return;
      api
        .deleteTemplateSet(delSet.dataset.id)
        .then((res) => {
          if (isEnvelopeFailure(res)) {
            toast(`Couldn't delete set: ${(res as { error?: string }).error || "unknown error"}`, "err");
            return;
          }
          void loadDetail(templateId);
        })
        .catch((e) => toast(`Couldn't delete set: ${e}`, "err"));
      return;
    }

    const saveBlock = action("tpl-save-block");
    if (saveBlock?.dataset.id) {
      const templateId = templatesAtom.get().selectedId;
      if (!templateId) return;
      const block = (templatesAtom.get().detail?.exercises || []).find((b) => b.id === saveBlock.dataset.id);
      if (!block) return;
      const notes = root.querySelector<HTMLInputElement>(`#tpl-exnotes-${CSS.escape(block.id)}`)?.value.trim() || null;
      void (async () => {
        const r1 = await api.updateTemplateExercise({ id: block.id, notes });
        if (isEnvelopeFailure(r1)) throw new Error((r1 as { error?: string }).error || "unknown error");
        for (const s of block.sets || []) {
          const r = await api.updateTemplateSet({
            id: s.id,
            reps: num(val(root, `ts-${s.id}-reps`)),
            weightKg: num(val(root, `ts-${s.id}-weight`)),
            restSeconds: num(val(root, `ts-${s.id}-rest`)),
            durationMinutes: num(val(root, `ts-${s.id}-dur`)),
            distanceMeters: num(val(root, `ts-${s.id}-dist`)),
          });
          if (isEnvelopeFailure(r)) throw new Error((r as { error?: string }).error || "unknown error");
        }
        void loadDetail(templateId);
      })().catch((e) => toast(`Couldn't save: ${e}`, "err"));
    }
  });

  void loadList();
  const unsub = templatesAtom.subscribe(render);
  return unsub;
}

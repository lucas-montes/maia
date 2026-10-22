import { api, type Note } from "../lib/api";
import { escapeHtml, formatDate, parseTags, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { notesAtom } from "../lib/stores";
import { backToDashboard, panelClose, panelOpen, statusBanner } from "./shared";

function noteRow(note: Note, selected: boolean): string {
  const tags = parseTags(note.tags);
  return `
    <button class="list-row w-full text-left ${selected ? "active" : ""}" type="button" data-note-id="${note.id}">
      <div class="min-w-0 flex-1">
        <div class="truncate text-sm font-medium text-ink-100">${escapeHtml(note.title || "Untitled")}</div>
        <div class="mt-1 flex items-center gap-2 text-xs text-ink-500">
          <span>${escapeHtml(formatDate(note.updated_at))}</span>
          ${tags.length ? `<span class="truncate text-accent-400">${escapeHtml(tags.join(", "))}</span>` : ""}
        </div>
      </div>
    </button>`;
}

/** Notes — read-only list plus a detail pane (notes are file-backed and
    managed on mobile). */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    <div data-slot="content"></div>
  `;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;

  const render = (): void => {
    const s = notesAtom.get();
    if (s.selectedId) {
      const meta = s.items.find((n) => n.id === s.selectedId);
      setHtml(
        content,
        `
        ${panelOpen("Edit Note", meta?.path ? `File: ${meta.path}` : "")}
          ${statusBanner(s)}
          <div class="rounded border border-surface-700 bg-base-100 px-3 py-2 text-sm font-medium text-ink-100">${escapeHtml(s.editTitle)}</div>
          <pre class="min-h-0 w-full flex-1 overflow-auto whitespace-pre-wrap rounded border border-surface-700 bg-base-100 p-3 font-mono text-sm text-ink-100">${escapeHtml(s.editContent)}</pre>
          <div class="rounded border border-surface-700 bg-base-100 px-3 py-2 text-sm text-ink-400">Tags: ${escapeHtml(s.editTags) || "—"}</div>
          <div class="flex items-center gap-3">
            <button class="btn btn-ghost btn-sm" type="button" data-action="notes-back">← Back to list</button>
          </div>
        ${panelClose}`,
      );
    } else {
      const rows = s.items.map((n) => noteRow(n, false)).join("");
      setHtml(
        content,
        `
        ${panelOpen("Notes", "Read-only — managed on mobile")}
          ${statusBanner(s)}
          <div class="flex items-center justify-between">
            <span class="text-sm text-ink-400">${s.items.length} note${s.items.length !== 1 ? "s" : ""} • read-only</span>
          </div>
          <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
            ${rows || `<div class="p-4 text-center text-sm text-ink-500">No notes yet.</div>`}
          </div>
          <div>${backToDashboard()}</div>
        ${panelClose}`,
      );
    }
    refreshIcons(content);
  };

  root.addEventListener("click", async (event) => {
    const target = event.target as HTMLElement;
    const back = target.closest<HTMLElement>("[data-action='notes-back']");
    if (back) {
      notesAtom.set({ ...notesAtom.get(), selectedId: null });
      return;
    }
    const row = target.closest<HTMLElement>("[data-note-id]");
    if (!row?.dataset.noteId) return;
    const id = row.dataset.noteId;
    notesAtom.set({ ...notesAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.readNote(id);
      if (res.success && res.data) {
        let tags = "";
        try {
          const arr = JSON.parse(res.data.tags);
          tags = Array.isArray(arr) ? arr.join(", ") : "";
        } catch {
          tags = "";
        }
        notesAtom.set({
          ...notesAtom.get(),
          selectedId: res.data.id,
          editTitle: res.data.title,
          editContent: res.data.content,
          editTags: tags,
          status: "idle",
          message: "",
        });
      } else {
        notesAtom.set({ ...notesAtom.get(), status: "error", message: res.error ?? "Failed to read note" });
      }
    } catch (e) {
      notesAtom.set({ ...notesAtom.get(), status: "error", message: `Failed to read note: ${e}` });
    }
  });

  api
    .listNotes()
    .then((res) => {
      if (res.success && res.data) {
        const s = notesAtom.get();
        const stillExists = s.selectedId ? res.data.find((n) => n.id === s.selectedId) : null;
        notesAtom.set({ ...s, items: res.data, selectedId: stillExists ? s.selectedId : null, status: "idle", message: "" });
      } else {
        notesAtom.set({ ...notesAtom.get(), status: "error", message: res.error ?? "Failed to load notes" });
      }
    })
    .catch((e) => notesAtom.set({ ...notesAtom.get(), status: "error", message: `Failed to load notes: ${e}` }));

  const unsub = notesAtom.subscribe(render);
  return unsub;
}

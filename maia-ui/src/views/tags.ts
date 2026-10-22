import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { tagsAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/** Tags — read-only usage summary per entity. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Tags", "Usage per entity")}
      <div data-slot="status"></div>
      <div class="list-divided" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const t = tagsAtom.get();
    setHtml(statusEl, statusBanner(t));
    setHtml(
      listEl,
      t.items
        .map(
          (tag) => `
          <div class="flex justify-between">
            <span class="text-sm" style="color:${tag.color ? `#${tag.color.toString(16).padStart(6, "0")}` : "#cbd5e1"}">${escapeHtml(tag.name)}</span>
            <span class="text-xs text-ink-400">tasks:${tag.taskCount ?? 0} exp:${tag.experimentCount ?? 0} goals:${tag.goalCount ?? 0} notes:${tag.noteCount ?? 0}</span>
          </div>`,
        )
        .join("") || `<div class="p-3 text-xs text-ink-400">No tags</div>`,
    );
    refreshIcons(listEl);
  };

  const load = async (): Promise<void> => {
    tagsAtom.set({ ...tagsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listTags();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        tagsAtom.set({ ...tagsAtom.get(), status: "error", message: failure });
        return;
      }
      tagsAtom.set({ ...tagsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      tagsAtom.set({ ...tagsAtom.get(), status: "error", message: String(e) });
    }
  };

  void load();
  const unsub = tagsAtom.subscribe(render);
  return unsub;
}

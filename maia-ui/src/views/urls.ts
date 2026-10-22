import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../lib/api";
import { escapeHtml, formatDate, parseTags, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { toast } from "../lib/overlay";
import { urlsAtom } from "../lib/stores";
import { backToDashboard, panelClose, panelOpen, statusBanner } from "./shared";

function urlRow(u: { id: string; title: string | null; url: string; source: string | null; tags: string; is_new: boolean; created_at: string }): string {
  const tags = parseTags(u.tags);
  return `
    <div class="flex items-start gap-3 rounded-lg bg-base-100 p-3 transition-colors hover:bg-base-100">
      ${u.is_new ? `<span class="mt-1 inline-block h-2 w-2 shrink-0 rounded-full bg-accent-400"></span>` : `<span class="mt-1 inline-block h-2 w-2 shrink-0 rounded-full bg-transparent"></span>`}
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="truncate font-medium text-ink-100">${escapeHtml(u.title || u.url)}</span>
          ${u.is_new ? `<span class="badge badge-sm badge-info">new</span>` : ""}
        </div>
        <div class="mt-0.5 truncate text-xs text-accent-400">${escapeHtml(u.url)}</div>
        <div class="mt-1 flex flex-wrap gap-2 text-xs text-ink-500">
          ${u.source ? `<span>${escapeHtml(u.source)}</span>` : ""}
          ${tags.length ? `<span class="text-accent-400">${escapeHtml(tags.join(", "))}</span>` : ""}
          <span>${escapeHtml(formatDate(u.created_at))}</span>
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-1">
        <button class="btn btn-ghost btn-xs" type="button" data-action="open-url" data-url-id="${u.id}" title="Open in browser"><i data-lucide="external-link" class="h-4 w-4"></i></button>
        ${u.is_new ? `<button class="btn btn-ghost btn-xs" type="button" data-action="mark-url-read" data-url-id="${u.id}" title="Mark as read"><i data-lucide="check" class="h-4 w-4"></i></button>` : ""}
        <button class="btn btn-ghost btn-xs text-err-400" type="button" data-action="delete-url" data-url-id="${u.id}" title="Delete URL"><i data-lucide="x" class="h-4 w-4"></i></button>
      </div>
    </div>`;
}

/** Saved URLs — full CRUD against the sync-server records. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Saved URLs", "Links captured from Chrome or added manually")}
      <div data-slot="status"></div>
      <div data-slot="new-banner"></div>
      <div class="rounded-lg bg-base-100 p-4">
        <h3 class="mb-3 font-medium text-ink-200">Add New URL</h3>
        <div class="space-y-3">
          <div>
            <label class="field-label" for="url-url">URL</label>
            <input type="url" id="url-url" class="input input-bordered w-full" placeholder="https://example.com" />
          </div>
          <div>
            <label class="field-label" for="url-title">Title (optional)</label>
            <input type="text" id="url-title" class="input input-bordered w-full" placeholder="Page title" />
          </div>
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            <div>
              <label class="field-label" for="url-source">Source</label>
              <select id="url-source" class="select select-bordered w-full">
                <option value="manual">Manual</option>
                <option value="chrome-extension">Chrome Extension</option>
                <option value="import">Import</option>
              </select>
            </div>
            <div>
              <label class="field-label" for="url-tags">Tags (comma-separated)</label>
              <input type="text" id="url-tags" class="input input-bordered w-full" placeholder="dev, article, reference" />
            </div>
          </div>
          <button class="btn btn-primary btn-sm" type="button" data-action="create-url">Save URL</button>
        </div>
      </div>
      <div class="flex items-center justify-between">
        <h3 class="font-medium text-ink-200">Saved Links</h3>
        <span class="text-sm text-ink-400" data-slot="count"></span>
      </div>
      <div class="space-y-2" data-slot="list"></div>
      <div>${backToDashboard()}</div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const bannerEl = root.querySelector("[data-slot='new-banner']") as HTMLElement;
  const countEl = root.querySelector("[data-slot='count']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const s = urlsAtom.get();
    setHtml(statusEl, statusBanner(s));
    countEl.textContent = `${s.items.length} URL${s.items.length !== 1 ? "s" : ""}`;
    const newCount = s.items.filter((u) => u.is_new).length;
    setHtml(
      bannerEl,
      newCount > 0
        ? `<div class="rounded border border-accent-800 bg-accent-900/20 p-3 text-sm text-accent-300">${newCount} new URL${newCount !== 1 ? "s" : ""} — click ✓ to mark as read, or ↗ to open</div>`
        : "",
    );
    setHtml(
      listEl,
      s.items.length
        ? s.items.map((u) => urlRow(u)).join("")
        : `<div class="py-8 text-center text-ink-500">No saved URLs yet. Add one above!</div>`,
    );
    refreshIcons(listEl);
  };

  root.addEventListener("click", async (event) => {
    const target = event.target as HTMLElement;

    const createAction = target.closest<HTMLElement>("[data-action='create-url']");
    if (createAction) {
      const urlInput = root.querySelector<HTMLInputElement>("#url-url");
      const titleInput = root.querySelector<HTMLInputElement>("#url-title");
      const sourceSelect = root.querySelector<HTMLSelectElement>("#url-source");
      const tagsInput = root.querySelector<HTMLInputElement>("#url-tags");
      const url = urlInput?.value.trim();
      if (!url) return;
      urlsAtom.set({ ...urlsAtom.get(), status: "saving", message: "" });
      const tagsJson = JSON.stringify(
        (tagsInput?.value || "").split(",").map((t) => t.trim()).filter(Boolean),
      );
      try {
        const res = await api.createUrl({
          title: titleInput?.value.trim() || null,
          url,
          source: sourceSelect?.value || "manual",
          tags: tagsJson,
        });
        if (res.success) {
          if (titleInput) titleInput.value = "";
          if (urlInput) urlInput.value = "";
          if (sourceSelect) sourceSelect.value = "manual";
          if (tagsInput) tagsInput.value = "";
          urlsAtom.set({ ...urlsAtom.get(), status: "idle", message: "" });
          await load();
        } else {
          urlsAtom.set({ ...urlsAtom.get(), status: "error", message: res.error ?? "Failed to create URL" });
        }
      } catch (e) {
        urlsAtom.set({ ...urlsAtom.get(), status: "error", message: `Failed to create URL: ${e}` });
      }
      return;
    }

    const openAction = target.closest<HTMLElement>("[data-action='open-url']");
    if (openAction?.dataset.urlId) {
      const item = urlsAtom.get().items.find((u) => u.id === openAction.dataset.urlId);
      if (item) {
        try {
          await openUrl(item.url);
        } catch {
          toast(`Could not open ${item.url}`, "err");
        }
        void api.markUrlRead(item.id).then(() => load()).catch(() => {});
      }
      return;
    }

    const markAction = target.closest<HTMLElement>("[data-action='mark-url-read']");
    if (markAction?.dataset.urlId) {
      try {
        await api.markUrlRead(markAction.dataset.urlId);
        const items = urlsAtom.get().items.map((u) =>
          u.id === markAction.dataset.urlId ? { ...u, is_new: false } : u,
        );
        urlsAtom.set({ ...urlsAtom.get(), items });
      } catch {
        // silently fail, as the old UI did
      }
      return;
    }

    const deleteAction = target.closest<HTMLElement>("[data-action='delete-url']");
    if (deleteAction?.dataset.urlId) {
      urlsAtom.set({ ...urlsAtom.get(), status: "saving", message: "" });
      try {
        const res = await api.deleteUrl(deleteAction.dataset.urlId);
        if (res.success) {
          urlsAtom.set({ ...urlsAtom.get(), status: "idle", message: "" });
          await load();
        } else {
          urlsAtom.set({ ...urlsAtom.get(), status: "error", message: res.error ?? "Failed to delete URL" });
        }
      } catch (e) {
        urlsAtom.set({ ...urlsAtom.get(), status: "error", message: `Failed to delete URL: ${e}` });
      }
    }
  });

  const load = async (): Promise<void> => {
    try {
      const res = await api.listUrls();
      if (res.success && res.data) {
        urlsAtom.set({ items: res.data, status: "idle", message: "" });
      } else {
        urlsAtom.set({ items: [], status: "error", message: res.error ?? "Failed to load URLs" });
      }
    } catch (e) {
      urlsAtom.set({ items: [], status: "error", message: `Failed to load URLs: ${e}` });
    }
  };

  render();
  void load();
  const unsub = urlsAtom.subscribe(render);
  return unsub;
}

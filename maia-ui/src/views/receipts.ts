import { api, type Receipt } from "../lib/api";
import { escapeHtml, formatEpoch, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { receiptsAtom } from "../lib/stores";
import { backToDashboard, panelClose, panelOpen, statusBanner } from "./shared";

/** Display status derived from the row; there is no status column in the schema. */
function receiptStatus(r: Receipt): string {
  if (r.archivedAt !== null) return "archived";
  if (r.parsed) return "parsed";
  switch (r.uploadStatus) {
    case 1: return "uploading";
    case 2:
    case 4: return "parsing";
    case 3: return "error";
    default: return "local";
  }
}

function badgeClass(r: Receipt): string {
  switch (receiptStatus(r)) {
    case "parsed": return "badge-ok";
    case "archived": return "badge-warn";
    case "error": return "badge-err";
    default: return "badge-muted";
  }
}

/** The OCR payload, or null when unparsed/unparseable. `parsedJson` is a text blob. */
function parsed(r: Receipt): Record<string, unknown> | null {
  if (!r.parsedJson) return null;
  try {
    const v = JSON.parse(r.parsedJson);
    return typeof v === "object" && v !== null ? (v as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

function parsedText(r: Receipt, key: string): string | null {
  const v = parsed(r)?.[key];
  if (v === null || v === undefined) return null;
  return typeof v === "object" ? JSON.stringify(v) : String(v);
}

function listHtml(items: Receipt[], selectedId: string | null): string {
  const rows = items
    .map((r) => {
      const merchant = parsedText(r, "merchant");
      const total = parsedText(r, "total");
      const date = parsedText(r, "date");
      const title = merchant || `Receipt ${r.id.slice(0, 8)}`;
      const image = r.archivedPath || r.remotePath;
      return `
        <button class="list-row w-full text-left ${selectedId === r.id ? "active" : ""}" type="button" data-receipt-id="${escapeHtml(r.id)}">
          <div class="min-w-0 flex-1">
            <div class="truncate text-sm font-medium text-ink-100">${escapeHtml(title)}</div>
            <div class="mt-1 flex items-center gap-2 text-xs text-ink-500">
              ${date ? `<span>${escapeHtml(date)}</span>` : ""}
              ${total ? `<span class="font-mono text-accent-400">${escapeHtml(total)}</span>` : ""}
              <span class="badge ${badgeClass(r)}">${escapeHtml(receiptStatus(r))}</span>
            </div>
            ${image ? `<div class="mt-1 truncate text-xs text-accent-400/70">${escapeHtml(image)}</div>` : ""}
          </div>
        </button>`;
    })
    .join("");
  return rows
    ? `<div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">${rows}</div>`
    : `<div class="p-4 text-center text-sm text-ink-500">No receipts captured yet.</div>`;
}

function detailHtml(r: Receipt): string {
  const status = receiptStatus(r);
  const merchant = parsedText(r, "merchant");
  const total = parsedText(r, "total");
  const currency = parsedText(r, "currency");
  const date = parsedText(r, "date");
  const items = parsed(r)?.items;
  // Archiving requires a parse, and only makes sense once one exists.
  const canArchive = r.parsed && r.archivedAt === null;
  const canRestore = r.archivedAt !== null;

  return `
    <div class="min-h-0 flex-1 space-y-4 overflow-auto">
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-3 text-sm font-medium text-ink-200">Receipt Info</h3>
        <div class="grid grid-cols-2 gap-3 text-sm">
          <div class="col-span-2"><span class="text-ink-500">ID:</span> <span class="break-all font-mono text-xs text-ink-300">${escapeHtml(r.id)}</span></div>
          <div><span class="text-ink-500">Status:</span> ${status === "archived" ? `<span class="text-warn-400">` : status === "parsed" ? `<span class="text-ok-400">` : status === "error" ? `<span class="text-err-400">` : `<span class="text-ink-400">`}${escapeHtml(status)}</span></div>
          <div><span class="text-ink-500">Uploaded:</span> <span class="text-ink-300">${escapeHtml(formatEpoch(r.createdAt))}</span></div>
          ${merchant ? `<div class="col-span-2"><span class="text-ink-500">Merchant:</span> <span class="text-ink-200">${escapeHtml(merchant)}</span></div>` : ""}
          ${total ? `<div><span class="text-ink-500">Total:</span> <span class="font-mono text-ink-200">${escapeHtml(total)}${currency ? ` ${escapeHtml(currency)}` : ""}</span></div>` : ""}
          ${date ? `<div><span class="text-ink-500">Date:</span> <span class="text-ink-200">${escapeHtml(date)}</span></div>` : ""}
          ${Array.isArray(items) ? `<div><span class="text-ink-500">Items:</span> <span class="text-ink-200">${items.length}</span></div>` : ""}
          ${r.transactionId ? `<div class="col-span-2"><span class="text-ink-500">Transaction:</span> <span class="break-all font-mono text-xs text-accent-400">${escapeHtml(r.transactionId)}</span></div>` : ""}
        </div>
        ${r.parseError ? `<div class="mt-3 rounded border border-surface-800 bg-base-100 p-2 text-xs text-err-400">Parse failed: ${escapeHtml(r.parseError)}</div>` : ""}
      </div>

      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-3 text-sm font-medium text-ink-200">Image</h3>
        <img class="max-h-96 w-full rounded object-contain" alt="Receipt ${escapeHtml(r.id)}" data-receipt-image="${escapeHtml(r.id)}" />
        <div class="mt-2 space-y-1 text-xs">
          ${r.archivedPath ? `<div><span class="text-ink-500">Archived:</span> <span class="break-all font-mono text-warn-400">${escapeHtml(r.archivedPath)}</span></div>` : ""}
          ${r.remotePath ? `<div><span class="text-ink-500">Stored:</span> <span class="break-all font-mono text-accent-400">${escapeHtml(r.remotePath)}</span></div>` : ""}
          ${r.localPath ? `<div><span class="text-ink-500">Phone:</span> <span class="break-all font-mono text-ink-400">${escapeHtml(r.localPath)}</span></div>` : ""}
        </div>
      </div>

      ${r.parsedJson ? `<div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-3 text-sm font-medium text-ink-200">Parsed JSON</h3>
        <pre class="max-h-72 overflow-auto whitespace-pre-wrap break-all font-mono text-xs text-ink-300">${escapeHtml(JSON.stringify(parsed(r), null, 2) ?? r.parsedJson)}</pre>
      </div>` : ""}

      <div class="flex flex-wrap items-center gap-3">
        <button class="btn btn-ghost btn-sm" type="button" data-action="receipts-back">← Back to list</button>
        ${canArchive ? `<button class="btn btn-primary btn-sm" type="button" data-action="archive-receipt" data-receipt-id="${escapeHtml(r.id)}">Archive</button>` : ""}
        ${canRestore ? `<button class="btn btn-secondary btn-sm" type="button" data-action="restore-receipt" data-receipt-id="${escapeHtml(r.id)}">Restore</button>` : ""}
      </div>
    </div>`;
}

/** Receipts — list, detail pane with lazily loaded image, archive/restore. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `<div data-slot="content"></div>`;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;

  const render = (): void => {
    const s = receiptsAtom.get();
    if (s.selectedId) {
      const r = s.items.find((x) => x.id === s.selectedId);
      setHtml(
        content,
        `
        ${panelOpen("Receipt Details", r ? `ID: ${r.id}` : "")}
          ${statusBanner(s)}
          ${r ? detailHtml(r) : `<div class="flex h-full items-center justify-center text-sm text-ink-500">Receipt not found</div>`}
        ${panelClose}`,
      );
      void loadImages();
    } else {
      setHtml(
        content,
        `
        ${panelOpen("Receipts", "Track and manage your expenses")}
          ${statusBanner(s)}
          <div class="flex items-center justify-between">
            <span class="text-sm text-ink-400">${s.items.length} receipt${s.items.length !== 1 ? "s" : ""}</span>
          </div>
          ${listHtml(s.items, s.selectedId)}
          <div>${backToDashboard()}</div>
        ${panelClose}`,
      );
    }
    refreshIcons(content);
  };

  /** Fills in any `<img data-receipt-image>` placeholder with bytes from disk. */
  const loadImages = async (): Promise<void> => {
    const placeholders = Array.from(root.querySelectorAll<HTMLImageElement>("img[data-receipt-image]"));
    await Promise.all(
      placeholders.map(async (img) => {
        const id = img.dataset.receiptImage;
        if (!id || img.dataset.receiptLoaded === "1") return;
        try {
          const res = await api.readReceiptPicture(id);
          if (res.success && res.data) {
            img.src = `data:${res.data.contentType};base64,${res.data.base64}`;
            img.dataset.receiptLoaded = "1";
          } else {
            img.replaceWith(
              Object.assign(document.createElement("div"), {
                className: "p-4 text-center text-xs text-ink-500",
                textContent: res.error ?? "Image unavailable",
              }),
            );
          }
        } catch (e) {
          img.replaceWith(
            Object.assign(document.createElement("div"), {
              className: "p-4 text-center text-xs text-ink-500",
              textContent: `Image unavailable: ${e}`,
            }),
          );
        }
      }),
    );
  };

  const list = async (): Promise<void> => {
    receiptsAtom.set({ ...receiptsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listReceipts();
      if (res.success && res.data) {
        const s = receiptsAtom.get();
        const stillExists = s.selectedId ? res.data.find((r) => r.id === s.selectedId) : null;
        receiptsAtom.set({ items: res.data, selectedId: stillExists ? s.selectedId : null, status: "idle", message: "" });
      } else {
        receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: res.error ?? "Failed to load receipts" });
      }
    } catch (e) {
      receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: `Failed to load receipts: ${e}` });
    }
  };

  const select = async (id: string): Promise<void> => {
    receiptsAtom.set({ ...receiptsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.readReceipt(id);
      if (res.success && res.data) {
        receiptsAtom.set({ ...receiptsAtom.get(), selectedId: res.data.id, status: "idle", message: "" });
      } else {
        receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: res.error ?? "Failed to read receipt" });
      }
    } catch (e) {
      receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: `Failed to read receipt: ${e}` });
    }
  };

  const archive = async (id: string): Promise<void> => {
    receiptsAtom.set({ ...receiptsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.archiveReceipt(id);
      if (!res.success) {
        receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: res.error ?? "Failed to archive receipt" });
        return;
      }
    } catch (e) {
      receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: `Failed to archive receipt: ${e}` });
      return;
    }
    await list();
  };

  const restore = async (id: string): Promise<void> => {
    receiptsAtom.set({ ...receiptsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.restoreReceipt(id);
      if (!res.success) {
        receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: res.error ?? "Failed to restore receipt" });
        return;
      }
    } catch (e) {
      receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: `Failed to restore receipt: ${e}` });
      return;
    }
    await list();
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const row = target.closest<HTMLElement>("[data-receipt-id]");
    if (row?.dataset.receiptId && !target.closest("[data-action]")) {
      void select(row.dataset.receiptId);
      return;
    }
    if (target.closest("[data-action='receipts-back']")) {
      receiptsAtom.set({ ...receiptsAtom.get(), selectedId: null });
      return;
    }
    const archiveBtn = target.closest<HTMLElement>("[data-action='archive-receipt']");
    if (archiveBtn?.dataset.receiptId) void archive(archiveBtn.dataset.receiptId);
    const restoreBtn = target.closest<HTMLElement>("[data-action='restore-receipt']");
    if (restoreBtn?.dataset.receiptId) void restore(restoreBtn.dataset.receiptId);
  });

  void list();
  const unsub = receiptsAtom.subscribe(render);
  return unsub;
}

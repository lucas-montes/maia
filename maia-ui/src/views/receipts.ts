import { api, isEnvelopeFailure, unwrapList, type Receipt } from "../lib/api";
import { escapeHtml, formatEpoch, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { toast } from "../lib/overlay";
import { receiptsAtom } from "../lib/stores";
import { backToDashboard, panelClose, panelOpen, statusBanner } from "./shared";

/** One parsed receipt line item, as the OCR payload's `items` array holds it
 *  (`receipt_ocr::ParsedItem`: name/price/currency, no quantity). */
interface ParsedLineItem {
  name: string;
  price: number;
  currency: string;
}

function parsedItems(r: Receipt): ParsedLineItem[] {
  const items = parsed(r)?.items;
  if (!Array.isArray(items)) return [];
  return items.filter(
    (it): it is ParsedLineItem =>
      it && typeof it === "object" && typeof (it as any).name === "string" && typeof (it as any).price === "number",
  );
}

/** The receipt's own date as epoch ms, falling back to when it was uploaded
 *  if the parsed date does not parse -- a match always needs *some* date to
 *  record the price against. */
function receiptRecordedAt(r: Receipt): number {
  const raw = parsedText(r, "date");
  const parsedMs = raw ? Date.parse(raw) : NaN;
  return Number.isFinite(parsedMs) ? parsedMs : r.createdAt;
}

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

      ${lineItemsHtml(r)}

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

/** T02: every parsed line item with a "match to ingredient" action. Matching
 *  is always manual -- there is no suggestion of which ingredient an item
 *  might be, only a picker. */
function lineItemsHtml(r: Receipt): string {
  const items = parsedItems(r);
  if (!items.length) return "";
  const rows = items
    .map(
      (it, i) => `
      <div class="flex items-center justify-between gap-3 border-b border-surface-800 py-2 last:border-0">
        <div class="min-w-0 truncate text-sm text-ink-200">${escapeHtml(it.name)}</div>
        <div class="flex shrink-0 items-center gap-3">
          <span class="font-mono text-sm text-ink-300">${it.price.toFixed(2)} ${escapeHtml(it.currency)}</span>
          <button class="btn btn-secondary btn-xs" type="button" data-action="match-item" data-receipt-id="${escapeHtml(r.id)}" data-item-index="${i}">
            Match to ingredient
          </button>
        </div>
      </div>`,
    )
    .join("");
  return `
    <div class="rounded border border-surface-800 bg-base-100 p-4">
      <h3 class="mb-3 text-sm font-medium text-ink-200">Line items (${items.length})</h3>
      ${rows}
    </div>`;
}

/** Receipts — list, detail pane with lazily loaded image, archive/restore.
 *  The header carries the import form: pick one or many receipt pictures,
 *  they upload to the server's OCR endpoint, parse, and each lands a draft
 *  expense once its parse succeeds. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    <div data-slot="content"></div>
    <input type="file" accept="image/*" multiple hidden data-import-input />
  `;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;
  const importInput = root.querySelector<HTMLInputElement>("[data-import-input]") as HTMLInputElement;

  const render = (): void => {
    const s = receiptsAtom.get();
    if (s.selectedId) {
      const r = s.items.find((x) => x.id === s.selectedId);
      setHtml(
        content,
        `
        ${panelOpen("Receipt Details", r ? `ID: ${r.id}` : "", "panel-fit")}
          ${statusBanner(s)}
          ${r ? detailHtml(r) : `<div class="flex h-full items-center justify-center text-sm text-ink-500">Receipt not found</div>`}
        ${panelClose}`,
      );
      void loadImages();
    } else {
      setHtml(
        content,
        `
        ${panelOpen("Receipts", "Track and manage your expenses", "panel-fit")}
          ${statusBanner(s)}
          <div class="flex items-center justify-between">
            <span class="text-sm text-ink-400">${s.items.length} receipt${s.items.length !== 1 ? "s" : ""}</span>
            <button class="btn btn-secondary btn-sm" type="button" data-action="import-receipts">
              <i data-lucide="image-plus" class="h-4 w-4"></i>
              Import receipts
            </button>
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
    if (target.closest("[data-action='import-receipts']")) {
      importInput.value = "";
      importInput.click();
      return;
    }
    const archiveBtn = target.closest<HTMLElement>("[data-action='archive-receipt']");
    if (archiveBtn?.dataset.receiptId) void archive(archiveBtn.dataset.receiptId);
    const restoreBtn = target.closest<HTMLElement>("[data-action='restore-receipt']");
    if (restoreBtn?.dataset.receiptId) void restore(restoreBtn.dataset.receiptId);
    const matchBtn = target.closest<HTMLElement>("[data-action='match-item']");
    if (matchBtn?.dataset.receiptId && matchBtn.dataset.itemIndex !== undefined) {
      const r = receiptsAtom.get().items.find((x) => x.id === matchBtn.dataset.receiptId);
      const item = r ? parsedItems(r)[Number(matchBtn.dataset.itemIndex)] : undefined;
      if (r && item) void openMatchModal(r, item);
    }
  });
  importInput.addEventListener("change", () => {
    void importFiles(Array.from(importInput.files ?? []));
  });

  void list();
  const unsub = receiptsAtom.subscribe(render);
  return unsub;
}

/** Upload → OCR poll → draft-expense creation, narrated on the view's banner.
 *
 *  One pick can carry many pictures; each is its own HTTP request (the OCR
 *  endpoint takes a single file part), so results are counted per receipt.
 *  The rows land in the shared database the moment the server accepts an
 *  upload, so the list refreshes live while parsing — the poll just watches
 *  for each receipt to reach a terminal parse state (like the phone does
 *  against `/:id/parse`), then creates the draft expense a parsed total
 *  stands for. */
const importFiles = async (picked: File[]): Promise<void> => {
  if (picked.length === 0) return;
  const plural = (n: number): string => `${n} receipt${n !== 1 ? "s" : ""}`;
  receiptsAtom.set({
    ...receiptsAtom.get(),
    status: "saving",
    message: `Uploading ${plural(picked.length)}...`,
  });

  const drops = await Promise.all(
    picked.map(async (f) => ({
      name: f.name,
      data: await readReceiptFileAsDataURL(f),
    })),
  );
  const uploaded = await api.uploadReceiptFiles(drops);
  if (!uploaded.success || !uploaded.data) {
    receiptsAtom.set({
      ...receiptsAtom.get(),
      status: "error",
      message: uploaded.error ?? "Receipt upload failed",
    });
    return;
  }
  const imported = uploaded.data.filter((r) => r.id);
  const uploadFailed = uploaded.data.filter((r) => !r.id).map((r) => `${r.name}: ${r.error}`);
  if (imported.length === 0) {
    receiptsAtom.set({
      ...receiptsAtom.get(),
      status: "error",
      message: `No receipt was imported — ${uploadFailed.join("; ")}`,
    });
    return;
  }
  const ids = imported.map((r) => r.id as string);

  // Refreshes the list without touching the banner (poll loop does the talking).
  const refreshQuiet = async (message: string): Promise<boolean> => {
    try {
      const res = await api.listReceipts();
      if (res.success && res.data) {
        const current = receiptsAtom.get();
        const selected = current.selectedId
          ? res.data.find((r) => r.id === current.selectedId) ?? null
          : null;
        receiptsAtom.set({
          items: res.data,
          selectedId: selected?.id ?? null,
          status: "saving",
          message,
        });
        return true;
      }
    } catch {
      // A poll failure must not kill the import; the next tick retries.
    }
    return false;
  };

  const deadline = Date.now() + 120_000;
  const unresolved = new Set(ids);
  /** Upload responses already account for every receipt; parse result is
   *  tracked here: value = final `parseError` text when resolved, null when
   *  the poll ran out before the OCR did. */
  const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));
  const parseOutcome = new Map<string, string | null>();
  while (Date.now() < deadline && unresolved.size > 0) {
    await sleep(3000);
    const message = `Parsing ${plural(unresolved.size)}...`;
    await refreshQuiet(message);
    const rows = receiptsAtom.get().items;
    for (const id of [...unresolved]) {
      const row = rows.find((r) => r.id === id);
      if (!row) continue;
      if (row.parsed || row.parseError) {
        parseOutcome.set(id, row.parseError);
        unresolved.delete(id);
      }
    }
  }
  for (const id of unresolved) parseOutcome.set(id, null);

  // A parsed receipt with no transaction yet gets its draft expense now.
  let booked = 0;
  const linkFailed: string[] = [];
  const rows = receiptsAtom.get().items;
  for (const id of ids) {
    const row = rows.find((r) => r.id === id);
    if (!row?.parsed || row.transactionId) continue;
    try {
      const res = await api.createReceiptTransaction(id);
      if (res.success) {
        if (res.data?.created) booked++;
      } else {
        linkFailed.push(res.error ?? "unknown error");
      }
    } catch (e) {
      linkFailed.push(String(e));
    }
  }

  await refreshQuiet("");
  const parseFailed = [...parseOutcome.entries()]
    .filter(([, err]) => err !== null)
    .map(([id, err]) => `${idShort(id)}: ${err}`);
  const notes = [
    ...(parseFailed.length ? [`${plural(parseFailed.length)} did not parse (${parseFailed.join("; ")})`] : []),
    ...(unresolvedCount(parseOutcome) > 0
      ? [`${plural(unresolvedCount(parseOutcome))} still parsing`]
      : []),
    ...(uploadFailed.length ? [`uploads failed: ${uploadFailed.join("; ")}`] : []),
    ...(linkFailed.length ? [`could not be booked: ${linkFailed.join("; ")}`] : []),
  ];
  receiptsAtom.set({
    ...receiptsAtom.get(),
    status: notes.length ? "idle" : "saved",
    message:
      booked > 0
        ? `Imported ${plural(booked)} — draft expenses created${notes.length ? ` · ${notes.join(" · ")}` : ""}`
        : notes.length
          ? notes.join(" · ")
          : "Receipts imported",
  });
};

/** Short label for a receipt id in the summary line. */
function idShort(id: string): string {
  return id.slice(0, 8);
}

function unresolvedCount(parseOutcome: Map<string, string | null>): number {
  return [...parseOutcome.values()].filter((v) => v === null).length;
}

/** Reads a picked file as its data URL; the upload command strips the
 *  base64 wrapper. */
function readReceiptFileAsDataURL(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error(`could not read ${file.name}`));
    reader.readAsDataURL(file);
  });
}

// ---------------------------------------------------------------------------
// T02: match-to-ingredient dialog
// ---------------------------------------------------------------------------

/** Ingredients loaded lazily on first match attempt and cached for the rest
 *  of the session -- the picker only needs id/name/archived, not a live feed. */
let ingredientCache: { id: string; name: string; is_archived: boolean }[] | null = null;

async function loadIngredientCache(): Promise<{ id: string; name: string; is_archived: boolean }[]> {
  if (ingredientCache) return ingredientCache;
  const res = await api.listIngredients();
  ingredientCache = isEnvelopeFailure(res) ? [] : unwrapList(res);
  return ingredientCache;
}

function matchResultsHtml(
  ingredients: { id: string; name: string; is_archived: boolean }[],
  query: string,
  selectedId: string | null,
): string {
  const q = query.trim().toLowerCase();
  const matches = ingredients
    .filter((it) => !it.is_archived && (!q || it.name.toLowerCase().includes(q)))
    .slice(0, 30);
  if (!matches.length) {
    return `<div class="p-2 text-xs text-ink-400">No ingredient matches your search.</div>`;
  }
  return matches
    .map(
      (it) => `
      <button type="button" class="block w-full rounded px-2 py-1.5 text-left text-sm ${it.id === selectedId ? "bg-accent-500/20 text-accent-300" : "text-ink-200 hover:bg-surface-800"}" data-action="match-pick" data-id="${escapeHtml(it.id)}">
        ${escapeHtml(it.name)}
      </button>`,
    )
    .join("");
}

/** Opens the "match to ingredient" dialog for one parsed line item: an
 *  ingredient picker plus the package-weight input the receipt never
 *  provides (see `receipt_ocr::ParsedItem`). Submitting calls
 *  `POST /ingredient-prices/match`, which finds-or-creates the store by the
 *  receipt's merchant name and records the price dated the receipt's own
 *  date -- never "today".
 */
async function openMatchModal(r: Receipt, item: ParsedLineItem): Promise<void> {
  const modalRoot = document.getElementById("modal-root");
  if (!modalRoot) return;
  const merchant = parsedText(r, "merchant") || "Unknown store";
  const recordedAt = receiptRecordedAt(r);
  const ingredients = await loadIngredientCache();
  let selectedId: string | null = null;

  modalRoot.innerHTML = `
    <dialog class="modal modal-open" id="match-item-modal">
      <div class="modal-box flex max-h-[85vh] max-w-lg flex-col overflow-hidden">
        <div class="flex items-center justify-between border-b border-surface-800 pb-3">
          <div>
            <h3 class="font-semibold text-ink-100">Match “${escapeHtml(item.name)}”</h3>
            <p class="mt-1 text-xs text-ink-500">${escapeHtml(merchant)} · ${escapeHtml(formatEpoch(recordedAt))} · ${item.price.toFixed(2)} ${escapeHtml(item.currency)}</p>
          </div>
          <button class="btn btn-ghost btn-sm" data-action="match-close" aria-label="Close">✕</button>
        </div>
        <div class="min-h-0 flex-1 space-y-3 overflow-auto py-3">
          <div>
            <label class="field-label">Ingredient</label>
            <input id="match-search" placeholder="Search ingredients..." class="input input-bordered w-full" />
            <div class="mt-2 max-h-48 space-y-0.5 overflow-auto" data-slot="match-results"></div>
          </div>
          <div class="grid grid-cols-2 gap-2">
            <div>
              <label class="field-label text-xs">Package size (g)</label>
              <input id="match-grams" type="number" step="0.1" placeholder="e.g. 500" class="input input-bordered w-full" />
            </div>
            <div>
              <label class="field-label text-xs">Source URL (optional)</label>
              <input id="match-source-url" placeholder="https://..." class="input input-bordered w-full" />
            </div>
          </div>
          <p class="text-xs text-ink-500">
            Re-matching the same ingredient at the same store on the same day overwrites the previous price, it does not add a second one.
          </p>
        </div>
        <div class="modal-action border-t border-surface-800 pt-3">
          <button class="btn btn-ghost btn-sm" data-action="match-close">Cancel</button>
          <button class="btn btn-primary btn-sm" data-action="match-save">Save match</button>
        </div>
      </div>
      <form method="dialog" class="modal-backdrop"><button>close</button></form>
    </dialog>`;

  const resultsEl = modalRoot.querySelector("[data-slot='match-results']") as HTMLElement;
  const searchEl = modalRoot.querySelector("#match-search") as HTMLInputElement;
  const renderResults = (): void => setHtml(resultsEl, matchResultsHtml(ingredients, searchEl.value, selectedId));
  renderResults();
  refreshIcons(modalRoot);
  searchEl.focus();

  const close = (): void => modalRoot.replaceChildren();

  searchEl.addEventListener("input", renderResults);
  modalRoot.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    if (target.closest("[data-action='match-close']") || target.closest(".modal-backdrop")) {
      close();
      return;
    }
    const pick = target.closest<HTMLElement>("[data-action='match-pick']");
    if (pick?.dataset.id) {
      selectedId = pick.dataset.id;
      renderResults();
      return;
    }
    if (target.closest("[data-action='match-save']")) {
      if (!selectedId) {
        toast("Pick an ingredient first", "err");
        return;
      }
      const grams = (modalRoot.querySelector("#match-grams") as HTMLInputElement).value.trim();
      const sourceUrl = (modalRoot.querySelector("#match-source-url") as HTMLInputElement).value.trim();
      void (async () => {
        try {
          const res = (await api.matchReceiptItem({
            ingredientId: selectedId as string,
            merchant,
            price: item.price,
            currencyCode: item.currency,
            packageGrams: grams ? Number(grams) : null,
            recordedAt,
            sourceUrl: sourceUrl || null,
          })) as any;
          if (res?.success === false) {
            toast(res.error || "Could not match item", "err");
            return;
          }
          toast(`Matched to ${ingredients.find((i) => i.id === selectedId)?.name ?? "ingredient"}`, "ok");
          close();
        } catch (e) {
          toast(`Could not match item: ${e}`, "err");
        }
      })();
    }
  });
}

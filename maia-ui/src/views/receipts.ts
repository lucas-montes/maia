import { api, type Receipt } from "../lib/api";
import { escapeHtml, formatEpoch, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { addressLine, isOldReading, readReading } from "../lib/receiptReading";
import { receiptsAtom } from "../lib/stores";
import { mountReceiptPage } from "./receiptPage";
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
        </div>
        ${r.parseError ? `<div class="mt-3 rounded border border-surface-800 bg-base-100 p-2 text-xs text-err-400">Parse failed: ${escapeHtml(r.parseError)}</div>` : ""}
      </div>

      ${readingHtml(r)}

      <div data-slot="matching"></div>

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

function money(value: number): string {
  return value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

/** What the receipt printed about its shop and its sale, with "Read again"
 *  for a reading made by the old reader (or a bad one). */
function readingHtml(r: Receipt): string {
  const reading = readReading(r.parsedJson);
  const id = escapeHtml(r.id);
  const reread = `<button class="btn btn-secondary btn-xs" type="button" data-action="reparse-receipt" data-receipt-id="${id}">Read again</button>`;
  if (!reading) {
    return r.remotePath || r.archivedPath
      ? `<div class="rounded border border-surface-800 bg-base-100 p-4"><div class="flex items-center justify-between"><h3 class="text-sm font-medium text-ink-200">Reading</h3>${reread}</div><div class="mt-2 text-sm text-ink-500">Not read yet.</div></div>`
      : "";
  }
  const fact = (label: string, value: string | null | undefined): string =>
    value ? `<div><span class="text-ink-500">${label}:</span> <span class="text-ink-200">${escapeHtml(value)}</span></div>` : "";
  const amount = (n: number | null): string | null => (n === null ? null : `${money(n)} ${reading.currency ?? ""}`);
  const taxId = reading.taxId ? `${reading.taxIdKind ? `${reading.taxIdKind.toUpperCase()} ` : ""}${reading.taxId}` : null;
  const payment = reading.paymentMethod
    ? `${reading.paymentMethod}${reading.cardLast4 ? ` ····${reading.cardLast4}` : ""}`
    : reading.cardLast4
      ? `card ····${reading.cardLast4}`
      : null;
  return `
    <div class="rounded border border-surface-800 bg-base-100 p-4">
      <div class="mb-3 flex items-center justify-between">
        <h3 class="text-sm font-medium text-ink-200">Reading</h3>
        <div class="flex items-center gap-2">
          ${isOldReading(reading) ? `<span class="text-xs text-warn-400">read by the old reader</span>` : ""}
          ${reread}
        </div>
      </div>
      <div class="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
        ${fact("Brand", reading.brand)}
        ${fact("Issued by", reading.merchant)}
        ${fact("Tax id", taxId)}
        ${fact("VAT number", reading.vatNumber)}
        ${fact("Address", addressLine(reading) || null)}
        ${fact("Branch", reading.branch)}
        ${fact("Website", reading.website)}
        ${fact("Receipt no.", reading.receiptNumber)}
        ${fact("Paid by", payment)}
        ${fact("Loyalty", reading.loyalty)}
        ${fact("Subtotal", amount(reading.subtotal))}
        ${fact("Total", amount(reading.total))}
      </div>
      ${reading.vat.length ? `<div class="mt-2 text-xs text-ink-500">VAT: ${reading.vat.map((v) => `${v.rate}% ${money(v.amount)}${v.base !== null ? ` on ${money(v.base)}` : ""}`).map(escapeHtml).join(" · ")}</div>` : ""}
      ${reading.discounts.length ? `<div class="mt-1 text-xs text-ink-500">Discounts: ${reading.discounts.map((d) => `${d.description} −${money(d.amount)}`).map(escapeHtml).join(" · ")}</div>` : ""}
    </div>`;
}

/** Receipts — list, detail pane with lazily loaded image, archive/restore.
 *  The header carries the import form: pick one or many receipt pictures,
 *  they upload to the server's OCR endpoint and are read and matched there. */
export function mount(root: HTMLElement): () => void {
  let pageEl: HTMLElement | null = null;
  let pageFor: string | null = null;
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
      // The matching page keeps its state (AI proposals, what is folded) while
      // the same receipt stays open, so a refresh re-attaches it.
      const slot = content.querySelector<HTMLElement>("[data-slot='matching']");
      if (slot && r) {
        if (pageFor === r.id && pageEl) {
          slot.replaceWith(pageEl);
        } else {
          pageEl = slot;
          pageFor = r.id;
          mountReceiptPage(slot, r.id, () => void list());
        }
      }
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
    const reparseBtn = target.closest<HTMLElement>("[data-action='reparse-receipt']");
    if (reparseBtn?.dataset.receiptId) {
      void reparse(reparseBtn.dataset.receiptId);
      return;
    }
  });
  importInput.addEventListener("change", () => {
    void importFiles(Array.from(importInput.files ?? []));
  });

  /** Asks the server to read the receipt again, then watches the row until
   *  the new reading (or its error) lands, as the import does. */
  const reparse = async (id: string): Promise<void> => {
    const before = receiptsAtom.get().items.find((x) => x.id === id)?.updatedAt ?? 0;
    receiptsAtom.set({ ...receiptsAtom.get(), status: "saving", message: "Reading the receipt again..." });
    try {
      const res = await api.reparseReceipt(id);
      if (!res.success) {
        receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: res.error ?? "Could not read the receipt again" });
        return;
      }
    } catch (e) {
      receiptsAtom.set({ ...receiptsAtom.get(), status: "error", message: `Could not read the receipt again: ${e}` });
      return;
    }
    const deadline = Date.now() + 120_000;
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 3000));
      const res = await api.listReceipts().catch(() => null);
      const row = res?.success ? res.data?.find((x) => x.id === id) : undefined;
      // upload_status 4 is "parsing"; the job always leaves it, parsed or failed.
      if (row && row.uploadStatus !== 4 && row.updatedAt > before) {
        await list();
        receiptsAtom.set({
          ...receiptsAtom.get(),
          status: row.parseError ? "error" : "saved",
          message: row.parseError ? `Reading failed: ${row.parseError}` : "Read again",
        });
        return;
      }
    }
    await list();
    receiptsAtom.set({ ...receiptsAtom.get(), status: "idle", message: "Still reading; refresh later" });
  };

  void list();
  const unsub = receiptsAtom.subscribe(render);
  return unsub;
}

/** Upload → OCR poll, narrated on the view's banner. Nothing is booked:
 *
 *  One pick can carry many pictures; each is its own HTTP request (the OCR
 *  endpoint takes a single file part), so results are counted per receipt.
 *  The rows land in the shared database the moment the server accepts an
 *  upload, so the list refreshes live while parsing — the poll just watches
 *  for each receipt to reach a terminal parse state (like the phone does
 *  against `/:id/parse`); the server matches each receipt once it is read. */
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

  // Nothing is booked from a receipt: the server has matched each one that
  // parsed (store, transaction, lines) and its page shows what it did.
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
  ];
  receiptsAtom.set({
    ...receiptsAtom.get(),
    status: notes.length ? "idle" : "saved",
    message: notes.length ? notes.join(" · ") : `Imported ${plural(ids.length)}; each one is matched on its page`,
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


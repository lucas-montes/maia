import {
  api,
  isEnvelopeFailure,
  unwrapList,
  type AiProposals,
  type FinanceTransaction,
  type MatchingLine,
  type MatchingPage,
  type ReceiptEffect,
  type StoreDetails,
} from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { openIn } from "../lib/focus";
import { refreshIcons } from "../lib/icons";
import { toast } from "../lib/overlay";
import { rankCandidates } from "../lib/receiptCandidates";
import {
  effectSentence,
  openItems,
  openTarget,
  recommendationSentence,
  sourceBadge,
  splitEffects,
} from "../lib/receiptPage";
import { ingredientsAtom } from "../lib/stores";
import { transactionLabel } from "../lib/transactionLabel";

/** The receipt's matching page: what the receipt did, what is still open, and
 *  "Improve with AI". Everything is decided by the server (`maia::matching`);
 *  this only shows it and sends the user's choices back. Each answer is the
 *  fresh page, so the view re-renders from it. */

function money(n: number): string {
  return n.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

function badge(source: string | null): string {
  const text = sourceBadge(source);
  if (!text) return "";
  const cls = source === "auto" ? "badge-ghost" : source === "ai" ? "badge-info" : "badge-ok";
  return `<span class="badge badge-xs ${cls}">${escapeHtml(text)}</span>`;
}

function card(title: string, body: string, actions = ""): string {
  return `
    <div class="rounded border border-surface-800 bg-base-100 p-4">
      <div class="mb-3 flex items-center justify-between gap-2">
        <h3 class="text-sm font-medium text-ink-200">${title}</h3>
        <div class="flex flex-wrap gap-2">${actions}</div>
      </div>
      ${body}
    </div>`;
}

function btn(action: string, label: string, data: Record<string, string | number> = {}, kind = "btn-secondary"): string {
  const attrs = Object.entries(data)
    .map(([k, v]) => `data-${k}="${escapeHtml(String(v))}"`)
    .join(" ");
  return `<button class="btn ${kind} btn-xs" type="button" data-page-action="${action}" ${attrs}>${label}</button>`;
}

function aiNote(confidence: number, reason: string): string {
  return `<span class="text-xs text-info-400">AI ${Math.round(confidence * 100)}% · ${escapeHtml(reason)}</span>`;
}

/** Mounts the page into `slot` for `receiptId`. `onChanged` runs after any
 *  change, so the surrounding list can refresh. */
export function mountReceiptPage(slot: HTMLElement, receiptId: string, onChanged: () => void): void {
  let page: MatchingPage | null = null;
  let proposals: AiProposals | null = null;
  /** The model the current proposals came from, and any that failed first. */
  let proposalModel: string | null = null;
  let proposalFallbacks = "";
  let showUndone = false;
  let busy = "";
  let stores: StoreDetails[] = [];
  let ledger: FinanceTransaction[] = [];

  const path = (suffix = ""): string => `/receipt-pictures/${encodeURIComponent(receiptId)}${suffix}`;

  const render = (): void => {
    if (!page) {
      setHtml(slot, `<div class="text-sm text-ink-500">${escapeHtml(busy || "Loading matching…")}</div>`);
      return;
    }
    setHtml(
      slot,
      `<div class="space-y-4">
        ${summaryHtml(page)}
        ${storeHtml(page)}
        ${transactionHtml(page)}
        ${linesHtml(page)}
        ${effectsHtml(page)}
      </div>`,
    );
    refreshIcons(slot);
  };

  // --- sections -----------------------------------------------------------

  const summaryHtml = (p: MatchingPage): string => {
    const open = openItems(p);
    return `
      <div class="flex flex-wrap items-center justify-between gap-2 rounded border border-surface-800 bg-base-100 p-3">
        <div class="text-sm ${open.length ? "text-warn-400" : "text-ok-400"}">
          ${open.length ? `Still open: ${escapeHtml(open.join(" · "))}` : "Everything on this receipt is matched."}
          ${p.readBy ? `<div class="mt-0.5 text-xs text-ink-500">Read by <span class="font-mono">${escapeHtml(p.readBy)}</span></div>` : ""}
          ${proposals && proposalModel ? `<div class="mt-0.5 text-xs text-ink-500">AI proposals from <span class="font-mono">${escapeHtml(proposalModel)}</span>${proposalFallbacks ? ` after ${escapeHtml(proposalFallbacks)}` : ""}</div>` : ""}
        </div>
        <div class="flex gap-2">
          ${busy ? `<span class="text-xs text-ink-500">${escapeHtml(busy)}</span>` : ""}
          ${btn("rematch", "Match again", {}, "btn-ghost")}
          ${p.read ? btn("assist", `<i data-lucide="sparkles" class="h-3 w-3"></i> Improve with AI`, {}, "btn-primary") : ""}
        </div>
      </div>`;
  };

  const storeHtml = (p: MatchingPage): string => {
    if (!p.read) return "";
    const s = p.store;
    const ai = proposals?.store;
    const head = s
      ? `<div class="flex items-center gap-2"><span class="text-sm text-ink-100">${escapeHtml(s.name)}</span>${badge(p.storeSource)}</div>
         <div class="text-xs text-ink-500">${escapeHtml([s.legalName, s.country, s.taxId ? `${(s.taxIdKind ?? "tax").toUpperCase()} ${s.taxId}` : null, s.city].filter(Boolean).join(" · "))}</div>`
      : `<div class="text-sm text-ink-500">No store yet.</div>`;
    const recs = p.recommendations
      .map((r) => {
        let action = "";
        if (r.kind === "rename") action = btn("rename-store", `Rename to “${escapeHtml(r.proposed)}”`, { store: r.storeId, name: r.proposed });
        if (r.kind === "sameStore") action = btn("choose-store", "This is it", { store: r.store.id });
        if (r.kind === "newStore")
          action = `<input class="input input-bordered input-xs w-48" data-new-store-name value="${escapeHtml(r.proposed)}" />${btn("create-store", "Create")}`;
        return `<div class="mt-2 flex flex-wrap items-center gap-2 text-xs text-ink-300"><span>${escapeHtml(recommendationSentence(r))}</span>${action}</div>`;
      })
      .join("");
    const aiRow = ai
      ? `<div class="mt-2 flex flex-wrap items-center gap-2">
          ${aiNote(ai.confidence, ai.reason)}
          ${ai.storeId
            ? btn("ai-store", `Accept: ${escapeHtml(stores.find((x) => x.id === ai.storeId)?.name ?? ai.storeId)}`, { store: ai.storeId })
            : ai.newName
              ? btn("ai-new-store", `Accept: new “${escapeHtml(ai.newName)}”`, { name: ai.newName })
              : ""}
        </div>`
      : "";
    return card(
      "Store",
      head + recs + aiRow,
      btn("pick-store", s ? "Change" : "Choose…") + (s ? btn("open-store", "Open", { store: s.id }, "btn-ghost") : ""),
    );
  };

  const transactionHtml = (p: MatchingPage): string => {
    if (!p.read) return "";
    const linkEffect = p.effects.find((e) => e.kind === "transaction_linked" && e.undoneAt === null && e.targetId === p.transactionId);
    const ai = proposals?.transaction;
    const head = p.transactionId
      ? `<div class="flex items-center gap-2"><span class="text-sm text-ink-100">${escapeHtml(linkEffect?.targetLabel ?? p.transactionId)}</span>${badge(p.linkSource)}</div>`
      : `<div class="text-sm text-ink-500">Not linked. A receipt paid in cash, or whose statement is not imported yet, stays unlinked.</div>`;
    const candidates = p.transactionId
      ? ""
      : p.transactionCandidates
          .filter((c) => !c.rejected)
          .slice(0, 4)
          .map(
            (c) => `
            <div class="mt-1 flex items-center justify-between gap-2 text-xs">
              <span class="min-w-0 truncate ${c.exactAmount ? "text-ink-200" : "text-ink-500"}">${escapeHtml(c.label)} · ${escapeHtml(c.accountName ?? "")} · ${new Date(c.date).toLocaleDateString()}</span>
              <span class="flex shrink-0 items-center gap-2">
                <span class="font-mono ${c.exactAmount ? "text-ok-400" : "text-ink-400"}">${money(c.amount)} ${escapeHtml(c.currencyCode)}</span>
                ${c.cardMatch ? `<span class="badge badge-xs badge-ghost">card</span>` : ""}
                ${c.merchantMatch ? `<span class="badge badge-xs badge-ghost">shop</span>` : ""}
                ${btn("link", "Link", { transaction: c.transactionId })}
              </span>
            </div>`,
          )
          .join("");
    const aiRow =
      ai?.transactionId && ai.transactionId !== p.transactionId
        ? `<div class="mt-2 flex flex-wrap items-center gap-2">${aiNote(ai.confidence, ai.reason)}${btn("ai-link", "Accept", { transaction: ai.transactionId })}</div>`
        : "";
    return card(
      "Transaction",
      head + candidates + aiRow,
      btn("pick-transaction", p.transactionId ? "Change" : "Search…") +
        (p.transactionId
          ? btn("open-transaction", "Open", { transaction: p.transactionId }, "btn-ghost") + btn("unlink", "Unlink", {}, "btn-ghost")
          : ""),
    );
  };

  const lineHtml = (l: MatchingLine): string => {
    const qty = l.quantity !== null ? `${l.quantity}${l.unit ? ` ${l.unit}` : ""}` : "";
    const ai = proposals?.lines.find((x) => x.line === l.line);
    let right = "";
    if (l.matched) {
      right = `<span class="text-xs text-ink-200">${escapeHtml(l.matched.ingredientName)}</span>${badge(l.matched.source)}
        ${btn("pick-ingredient", "Change", { line: l.line }, "btn-ghost")}${btn("unmatch", "Unmatch", { line: l.line }, "btn-ghost")}`;
    } else if (l.kind === "product") {
      right =
        l.candidates
          .slice(0, 3)
          .map((c) => btn("match-line", `${c.learned ? "✓ " : ""}${escapeHtml(c.name)}`, { line: l.line, ingredient: c.id }, "btn-ghost"))
          .join("") + btn("pick-ingredient", "Pick…", { line: l.line });
    }
    let aiRow = "";
    if (ai && !(l.matched && l.matched.ingredientId === ai.ingredientId)) {
      const accept = ai.ingredientId
        ? btn("ai-line", "Accept", { line: l.line, ingredient: ai.ingredientId })
        : ai.newIngredientName
          ? btn("new-ingredient", `Create “${escapeHtml(ai.newIngredientName)}”`, { line: l.line, name: ai.newIngredientName })
          : "";
      const what = ai.notAProduct ? "not a product" : ai.ingredientId ? (ingredientName(ai.ingredientId) ?? ai.ingredientId) : "";
      aiRow = `<div class="mt-1 flex flex-wrap items-center gap-2 pl-2">${what ? `<span class="text-xs text-ink-300">${escapeHtml(what)}</span>` : ""}${aiNote(ai.confidence, ai.reason)}${accept}</div>`;
    }
    return `
      <div class="border-b border-surface-800 py-2 last:border-0 ${l.kind !== "product" ? "opacity-60" : ""}">
        <div class="flex items-center justify-between gap-3">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              ${l.kind !== "product" ? `<span class="badge badge-xs badge-ghost">${escapeHtml(l.kind)}</span>` : ""}
              <span class="truncate text-sm text-ink-200">${escapeHtml(l.name)}</span>
            </div>
            <div class="text-xs text-ink-500">${escapeHtml(qty)}${qty ? " · " : ""}${money(l.price)} ${escapeHtml(l.currency)}</div>
          </div>
          <div class="flex shrink-0 flex-wrap items-center justify-end gap-1">${right}</div>
        </div>
        ${aiRow}
      </div>`;
  };

  const linesHtml = (p: MatchingPage): string => {
    if (!p.lines.length) return "";
    const acceptAll = proposals?.lines.some((l) => l.ingredientId) ? btn("ai-accept-all", "Accept all AI lines", {}, "btn-primary") : "";
    return card(`Lines (${p.lines.length})`, p.lines.map(lineHtml).join(""), acceptAll);
  };

  const effectRow = (e: ReceiptEffect): string => {
    const target = openTarget(e);
    const editable = e.undoneAt === null;
    return `
      <div class="flex items-center justify-between gap-3 border-b border-surface-800 py-1.5 last:border-0 ${e.undoneAt ? "opacity-50 line-through" : ""}">
        <div class="flex min-w-0 items-center gap-2">
          ${badge(e.source)}
          <span class="truncate text-sm text-ink-200">${escapeHtml(effectSentence(e))}</span>
          ${e.model ? `<span class="shrink-0 font-mono text-xs text-ink-500" title="Proposed by this model">${escapeHtml(e.model)}</span>` : ""}
          ${e.targetLive || e.targetTable.endsWith("aliases") ? "" : `<span class="text-xs text-ink-500">(gone)</span>`}
        </div>
        <div class="flex shrink-0 gap-1">
          ${target ? btn("open", "Open", { view: target.view, id: target.id }, "btn-ghost") : ""}
          ${editable ? btn("edit-effect", "Edit", { effect: e.id }, "btn-ghost") : ""}
          ${editable ? btn("undo", "Undo", { effect: e.id }, "btn-ghost") : ""}
        </div>
      </div>`;
  };

  const effectsHtml = (p: MatchingPage): string => {
    const { live, undone } = splitEffects(p.effects);
    const body =
      (live.map(effectRow).join("") || `<div class="text-sm text-ink-500">Nothing yet.</div>`) +
      (undone.length
        ? `<div class="mt-2">${btn("toggle-undone", showUndone ? "Hide undone" : `Show undone (${undone.length})`, {}, "btn-ghost")}</div>${showUndone ? undone.map(effectRow).join("") : ""}`
        : "");
    return card("What this receipt did", body);
  };

  const ingredientName = (id: string): string | null =>
    (ingredientsAtom.get().items as { id: string; name: string }[]).find((i) => i.id === id)?.name ?? null;

  // --- data -----------------------------------------------------------------

  const load = async (): Promise<void> => {
    const res = await api.getReceiptMatching(receiptId).catch((e) => ({ success: false, error: String(e) }) as never);
    if (res.success && res.data) {
      page = res.data;
    } else {
      busy = res.error ?? "Could not load the matching";
    }
    render();
  };

  /** Sends one action; an answer that is the page replaces it. */
  const send = async (label: string, method: "POST" | "PUT" | "DELETE", p: string, body?: unknown): Promise<boolean> => {
    busy = label;
    render();
    try {
      const res = await api.matchingRequest<MatchingPage | Record<string, unknown>>(method, p, body);
      busy = "";
      if (!res.success) {
        toast(res.error ?? `${label} failed`, "err");
        render();
        return false;
      }
      const data = res.data as MatchingPage | undefined;
      if (data && "effects" in data && "lines" in data) {
        page = data;
        render();
      } else {
        await load();
      }
      onChanged();
      return true;
    } catch (e) {
      busy = "";
      toast(`${label} failed: ${e}`, "err");
      render();
      return false;
    }
  };

  const loadStores = async (): Promise<void> => {
    const res = await api.listStores().catch(() => null);
    if (res && !isEnvelopeFailure(res)) stores = unwrapList(res) as StoreDetails[];
  };

  const loadLedger = async (): Promise<void> => {
    const res = await api.listTransactions().catch(() => null);
    if (res && !isEnvelopeFailure(res)) ledger = unwrapList(res) as FinanceTransaction[];
  };

  const loadIngredients = async (): Promise<void> => {
    if (ingredientsAtom.get().items.length) return;
    const res = await api.listIngredients().catch(() => null);
    if (res && !isEnvelopeFailure(res)) ingredientsAtom.set({ ...ingredientsAtom.get(), items: unwrapList(res) });
  };

  // --- actions --------------------------------------------------------------

  /** An AI accept carries the model that proposed it, so the change is
   *  filed under that model. */
  const decided = (source: "manual" | "ai") => (source === "ai" ? { source, model: proposalModel } : { source });

  const linkTo = (transactionId: string, source: "manual" | "ai" = "manual"): Promise<boolean> =>
    send("Linking", "PUT", path("/transaction"), { transactionId, ...decided(source) });

  const chooseStore = (storeId: string, source: "manual" | "ai" = "manual"): Promise<boolean> =>
    send("Choosing the store", "PUT", path("/store"), { storeId, ...decided(source) });

  const matchLine = (line: number, ingredientId: string, extra: Record<string, unknown> = {}, source: "manual" | "ai" = "manual"): Promise<boolean> =>
    send("Matching the line", "PUT", path(`/lines/${line}`), { ingredientId, ...decided(source), ...extra });

  const editEffect = (effectId: string): void => {
    const e = page?.effects.find((x) => x.id === effectId);
    if (!e) return;
    if (e.kind === "transaction_linked") void pickTransaction();
    else if (e.kind === "price_recorded" || e.kind === "product_alias_learned") {
      if (e.line !== null) void pickIngredient(e.line);
    } else if (e.kind === "store_renamed") void renameStore(e.targetId, String(e.after.name ?? ""));
    else void pickStore();
  };

  const renameStore = async (storeId: string, current: string): Promise<void> => {
    const name = window.prompt("Store name", current)?.trim();
    if (name && name !== current) await send("Renaming", "PUT", `/stores/${encodeURIComponent(storeId)}/name`, { name, receiptId });
  };

  const acceptAll = async (): Promise<void> => {
    if (!proposals || !page) return;
    const p = proposals;
    if (p.store?.storeId && p.store.storeId !== page.store?.id) await chooseStore(p.store.storeId, "ai");
    else if (!page.store && p.store?.newName)
      await send("Creating the store", "POST", path("/store"), { name: p.store.newName, ...decided("ai") });
    if (p.transaction?.transactionId && p.transaction.transactionId !== page.transactionId)
      await linkTo(p.transaction.transactionId, "ai");
    for (const l of p.lines) {
      const current = page?.lines.find((x) => x.line === l.line);
      if (l.ingredientId && current?.matched?.ingredientId !== l.ingredientId) await matchLine(l.line, l.ingredientId, {}, "ai");
    }
  };

  slot.addEventListener("click", (event) => {
    const el = (event.target as HTMLElement).closest<HTMLElement>("[data-page-action]");
    if (!el || busy) return;
    const d = el.dataset;
    switch (d.pageAction) {
      case "rematch":
        void send("Matching", "POST", path("/match"));
        break;
      case "assist":
        void (async () => {
          busy = "Asking the AI…";
          render();
          await Promise.all([loadStores(), loadIngredients()]);
          const res = await api
            .matchingRequest<{ proposals: AiProposals; model: string; attempts: { model: string; error: string }[] }>("POST", path("/assist"))
            .catch((e) => ({ success: false, error: String(e) }) as never);
          busy = "";
          if (res.success && res.data) {
            proposals = res.data.proposals;
            proposalModel = res.data.model ?? null;
            proposalFallbacks = (res.data.attempts ?? []).map((a) => `${a.model} failed`).join(", ");
            const n = (proposals.store ? 1 : 0) + (proposals.transaction ? 1 : 0) + proposals.lines.length;
            toast(n ? `The AI proposed ${n} match${n === 1 ? "" : "es"}; nothing changes until you accept` : "The AI had nothing to add", "ok");
          } else {
            toast(res.error ?? "The AI could not help", "err");
          }
          render();
        })();
        break;
      case "rename-store":
        void send("Renaming", "PUT", `/stores/${encodeURIComponent(d.store!)}/name`, { name: d.name, receiptId });
        break;
      case "choose-store":
        void chooseStore(d.store!);
        break;
      case "create-store": {
        const name = slot.querySelector<HTMLInputElement>("[data-new-store-name]")?.value.trim();
        void send("Creating the store", "POST", path("/store"), { name: name || null });
        break;
      }
      case "ai-store":
        void chooseStore(d.store!, "ai");
        break;
      case "ai-new-store":
        void send("Creating the store", "POST", path("/store"), { name: d.name, ...decided("ai") });
        break;
      case "pick-store":
        void pickStore();
        break;
      case "open-store":
        openIn("stores", d.store!);
        break;
      case "link":
        void linkTo(d.transaction!);
        break;
      case "ai-link":
        void linkTo(d.transaction!, "ai");
        break;
      case "pick-transaction":
        void pickTransaction();
        break;
      case "open-transaction":
        openIn("transactions", d.transaction!);
        break;
      case "unlink":
        void send("Unlinking", "DELETE", path("/transaction"));
        break;
      case "match-line":
        void matchLine(Number(d.line), d.ingredient!);
        break;
      case "ai-line":
        void matchLine(Number(d.line), d.ingredient!, {}, "ai");
        break;
      case "ai-accept-all":
        void acceptAll();
        break;
      case "pick-ingredient":
        void pickIngredient(Number(d.line));
        break;
      case "new-ingredient":
        void newIngredient(Number(d.line), d.name ?? "");
        break;
      case "unmatch":
        void send("Unmatching", "DELETE", path(`/lines/${d.line}`));
        break;
      case "open":
        if (d.view === "ingredients") {
          ingredientsAtom.set({ ...ingredientsAtom.get(), selectedId: d.id! });
          window.location.hash = "#/ingredients";
        } else {
          openIn(d.view as "stores" | "transactions", d.id!);
        }
        break;
      case "edit-effect":
        editEffect(d.effect!);
        break;
      case "undo":
        void send("Undoing", "POST", path(`/effects/${encodeURIComponent(d.effect!)}/undo`));
        break;
      case "toggle-undone":
        showUndone = !showUndone;
        render();
        break;
    }
  });

  // --- pickers ----------------------------------------------------------------

  /** A searchable list in a modal; `rows` renders the results for a query. */
  const modal = (title: string, subtitle: string, rows: (q: string) => string, extra = ""): { root: HTMLElement; close: () => void } | null => {
    const host = document.getElementById("modal-root");
    if (!host) return null;
    host.innerHTML = `
      <dialog class="modal modal-open">
        <div class="modal-box flex max-h-[85vh] max-w-xl flex-col overflow-hidden">
          <div class="flex items-center justify-between border-b border-surface-800 pb-3">
            <div>
              <h3 class="font-semibold text-ink-100">${title}</h3>
              <p class="mt-1 text-xs text-ink-500">${subtitle}</p>
            </div>
            <button class="btn btn-ghost btn-sm" data-modal="close" aria-label="Close">✕</button>
          </div>
          <div class="min-h-0 flex-1 space-y-2 overflow-auto py-3">
            <input data-modal="search" placeholder="Search…" class="input input-bordered w-full" />
            ${extra}
            <div class="space-y-0.5" data-modal="results"></div>
          </div>
        </div>
        <form method="dialog" class="modal-backdrop"><button>close</button></form>
      </dialog>`;
    const search = host.querySelector<HTMLInputElement>("[data-modal='search']")!;
    const results = host.querySelector<HTMLElement>("[data-modal='results']")!;
    const paint = (): void => setHtml(results, rows(search.value));
    search.addEventListener("input", paint);
    paint();
    search.focus();
    const close = (): void => host.replaceChildren();
    host.addEventListener("click", (event) => {
      const t = event.target as HTMLElement;
      if (t.closest("[data-modal='close']") || t.closest(".modal-backdrop")) close();
    });
    return { root: host, close };
  };

  const pickRow = (id: string, html: string, active = false): string =>
    `<button type="button" class="flex w-full items-center gap-3 rounded px-2 py-1.5 text-left ${active ? "bg-accent-500/20" : "hover:bg-surface-800"}" data-pick="${escapeHtml(id)}">${html}</button>`;

  const pickStore = async (): Promise<void> => {
    await loadStores();
    const m = modal("Which store is this receipt from?", "Choosing one teaches the store this receipt's name.", (q) => {
      const query = q.trim().toLowerCase();
      return (
        stores
          .filter((s) => !query || `${s.name} ${s.brand ?? ""} ${s.city ?? ""} ${s.country ?? ""}`.toLowerCase().includes(query))
          .map((s) =>
            pickRow(
              s.id,
              `<span class="flex-1 text-sm text-ink-200">${escapeHtml(s.name)}</span><span class="text-xs text-ink-500">${escapeHtml([s.country, s.city, s.taxId].filter(Boolean).join(" · "))}</span>`,
              s.id === page?.store?.id,
            ),
          )
          .join("") || `<div class="p-2 text-xs text-ink-400">No store fits. Close this and create one from the recommendation.</div>`
      );
    });
    m?.root.addEventListener("click", (event) => {
      const id = (event.target as HTMLElement).closest<HTMLElement>("[data-pick]")?.dataset.pick;
      if (!id) return;
      m.close();
      void chooseStore(id);
    });
  };

  const pickTransaction = async (): Promise<void> => {
    await loadLedger();
    const server = page?.transactionCandidates ?? [];
    const label = (t: FinanceTransaction): string =>
      `${transactionLabel(t)} ${t.note ?? ""} ${t.category ?? ""} ${t.accountName ?? ""} ${money(t.amount)}`;
    const m = modal("Which transaction did this receipt pay for?", "The server's candidates come first; a search reaches the whole ledger.", (q) => {
      const query = q.trim();
      const list: FinanceTransaction[] = query
        ? rankCandidates(ledger, { id: receiptId, total: null, currency: null, date: Date.now() }, query, label).slice(0, 50)
        : (server
            .filter((c) => !c.rejected)
            .map((c) => ledger.find((t) => t.id === c.transactionId))
            .filter(Boolean) as FinanceTransaction[]);
      return (
        list
          .map((t) =>
            pickRow(
              t.id,
              `<span class="min-w-0 flex-1 truncate text-sm text-ink-200">${escapeHtml(transactionLabel(t))}</span>
               <span class="text-xs text-ink-500">${escapeHtml(t.accountName ?? "")} · ${new Date(t.date).toLocaleDateString()}</span>
               <span class="font-mono text-sm text-ink-300">${money(t.amount)} ${escapeHtml(t.currencyCode)}</span>`,
              t.id === page?.transactionId,
            ),
          )
          .join("") || `<div class="p-2 text-xs text-ink-400">No candidate. Search the ledger.</div>`
      );
    });
    m?.root.addEventListener("click", (event) => {
      const id = (event.target as HTMLElement).closest<HTMLElement>("[data-pick]")?.dataset.pick;
      if (!id) return;
      m.close();
      void linkTo(id);
    });
  };

  const pickIngredient = async (line: number): Promise<void> => {
    await loadIngredients();
    const l = page?.lines.find((x) => x.line === line);
    if (!l) return;
    const weighed = l.unit === "kg" || l.unit === "g" || l.unit === "l" || l.unit === "ml";
    const all = ingredientsAtom.get().items as { id: string; name: string; brand?: string | null; is_archived?: boolean }[];
    const m = modal(
      `What is “${escapeHtml(l.name)}”?`,
      `${l.quantity ?? ""} ${l.unit ?? ""} · ${money(l.price)} ${escapeHtml(l.currency)} · the store learns this name`,
      (q) => {
        const query = q.trim().toLowerCase();
        const suggested = new Set(l.candidates.map((c) => c.id));
        return all
          .filter((i) => !i.is_archived && (!query || `${i.name} ${i.brand ?? ""}`.toLowerCase().includes(query)))
          .sort((a, b) => Number(suggested.has(b.id)) - Number(suggested.has(a.id)))
          .slice(0, 40)
          .map((i) => pickRow(i.id, `<span class="flex-1 text-sm text-ink-200">${escapeHtml(i.name)}</span><span class="text-xs text-ink-500">${escapeHtml(i.brand ?? "")}</span>`, i.id === l.matched?.ingredientId))
          .join("") || `<div class="p-2 text-xs text-ink-400">No ingredient matches.</div>`;
      },
      weighed
        ? `<p class="text-xs text-ink-500">Weighed line: priced by its weight.</p>`
        : `<label class="field-label text-xs">Package size (g), if you know it</label><input data-modal="grams" type="number" step="0.1" class="input input-bordered input-sm w-40" />`,
    );
    m?.root.addEventListener("click", (event) => {
      const id = (event.target as HTMLElement).closest<HTMLElement>("[data-pick]")?.dataset.pick;
      if (!id) return;
      const grams = m.root.querySelector<HTMLInputElement>("[data-modal='grams']")?.value.trim();
      m.close();
      void matchLine(line, id, grams ? { packageGrams: Number(grams) } : {});
    });
  };

  /** An ingredient the AI named that does not exist yet: a small form, then the match. */
  const newIngredient = async (line: number, name: string): Promise<void> => {
    const host = document.getElementById("modal-root");
    if (!host) return;
    const field = (key: string, label: string): string =>
      `<div><label class="field-label text-xs">${label}</label><input data-ing="${key}" type="number" step="0.1" class="input input-bordered input-sm w-full" /></div>`;
    host.innerHTML = `
      <dialog class="modal modal-open">
        <div class="modal-box max-w-md space-y-3">
          <h3 class="font-semibold text-ink-100">New ingredient</h3>
          <div><label class="field-label text-xs">Name</label><input data-ing="name" class="input input-bordered input-sm w-full" value="${escapeHtml(name)}" /></div>
          <div class="grid grid-cols-2 gap-2">
            ${field("kcal", "kcal / 100 g")}${field("protein", "Protein / 100 g")}${field("carbs", "Carbs / 100 g")}${field("fat", "Fat / 100 g")}
          </div>
          <div class="modal-action">
            <button class="btn btn-ghost btn-sm" data-ing="cancel">Cancel</button>
            <button class="btn btn-primary btn-sm" data-ing="save">Create and match</button>
          </div>
        </div>
      </dialog>`;
    const value = (k: string): string => host.querySelector<HTMLInputElement>(`[data-ing='${k}']`)?.value.trim() ?? "";
    host.addEventListener("click", (event) => {
      const t = (event.target as HTMLElement).closest<HTMLElement>("[data-ing]")?.dataset.ing;
      if (t === "cancel") host.replaceChildren();
      if (t !== "save") return;
      const keys = ["kcal", "protein", "carbs", "fat"];
      const nums = keys.map((k) => Number(value(k)));
      if (!value("name") || keys.some((k) => value(k) === "" || !Number.isFinite(Number(value(k))))) {
        toast("Give the name and the four values per 100 g", "err");
        return;
      }
      void (async () => {
        const res = (await api.createIngredient({
          name: value("name"),
          caloriesPer100g: nums[0],
          proteinPer100g: nums[1],
          carbsPer100g: nums[2],
          fatPer100g: nums[3],
          fiberPer100g: null,
          sugarPer100g: null,
          sodiumPer100g: null,
          brand: null,
          barcode: null,
        })) as { success?: boolean; data?: { id?: string }; error?: string };
        const id = res?.data?.id;
        if (res?.success === false || !id) {
          toast(res?.error ?? "Could not create the ingredient", "err");
          return;
        }
        host.replaceChildren();
        ingredientsAtom.set({ ...ingredientsAtom.get(), items: [] });
        await loadIngredients();
        await matchLine(line, id, {}, "ai");
      })();
    });
  };

  render();
  void load();
  void loadIngredients();
}

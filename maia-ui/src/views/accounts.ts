import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { accountsAtom } from "../lib/stores";
import { isEnvelopeFailure } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/** Accounts — read-only list with balances. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Accounts", "Balances across your accounts")}
      <div data-slot="status"></div>
      <div class="list-divided" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const a = accountsAtom.get();
    setHtml(statusEl, statusBanner(a));
    setHtml(
      listEl,
      a.items
        .map(
          (ac) => `
          <div class="flex justify-between">
            <span class="text-sm text-ink-100">${escapeHtml(ac.name)} (${escapeHtml(ac.type)})</span>
            <span class="text-xs text-ink-400">${ac.openingBalance}</span>
          </div>`,
        )
        .join("") || `<div class="p-3 text-xs text-ink-400">No accounts</div>`,
    );
    refreshIcons(listEl);
  };

  const load = async (): Promise<void> => {
    accountsAtom.set({ ...accountsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listAccounts();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        accountsAtom.set({ ...accountsAtom.get(), status: "error", message: failure });
        return;
      }
      const data = (res as { data?: unknown })?.data ?? res;
      accountsAtom.set({ ...accountsAtom.get(), items: Array.isArray(data) ? data : [], status: "idle", message: "" });
    } catch (e) {
      accountsAtom.set({ ...accountsAtom.get(), status: "error", message: String(e) });
    }
  };

  void load();
  const unsub = accountsAtom.subscribe(render);
  return unsub;
}

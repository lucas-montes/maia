import QRCode from "qrcode";
import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { syncAtom } from "../lib/stores";
import { panelClose, panelOpen } from "./shared";

/** The JSON a phone scans to pair with this server. */
function pairingPayload(url: string, apiKey: string): string {
  return JSON.stringify({ url, apiKey, version: 1 });
}

/** Sync — server status plus the QR a phone scans to pair. Refreshes the
    status every 10 seconds while mounted. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `<div data-slot="content"></div>`;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;

  const render = (): void => {
    const s = syncAtom.get();
    const info = s.info;
    const isListening = info?.isListening ?? false;
    const lanUrl = info?.lanUrl || info?.localUrl || "";
    const apiKey = info?.apiKey || "";
    const payload = pairingPayload(lanUrl, apiKey);
    const lanIp = info?.lanIp || "";

    setHtml(
      content,
      `
      ${panelOpen("Sync", "Pair a phone with this machine")}
        <div class="flex items-start justify-between gap-4">
          <div></div>
          <button class="btn btn-secondary btn-sm shrink-0" data-action="sync-refresh"><i data-lucide="refresh-cw" class="h-4 w-4"></i> Refresh</button>
        </div>
        <div class="panel p-4">
          <div class="flex items-center gap-2">
            <span class="${isListening ? "text-ok-400" : "text-err-400"} text-xl">●</span>
            <span class="text-sm text-ink-100">${isListening ? "Server running" : "Server not reachable"}</span>
            <span class="text-xs text-ink-500">port ${info?.port || "?"}</span>
          </div>
          ${isListening ? "" : `<p class="mt-2 text-xs text-ink-400">Reading still works — it comes from the database. Saving is not possible until the server is back, because it is the only thing that writes.</p>`}
        </div>

        <div class="panel p-4">
          <h3 class="mb-3 text-sm font-medium text-ink-200">Pairing</h3>
          ${
            s.qrDataUrl
              ? `<div class="inline-block rounded-lg bg-white p-2"><img src="${s.qrDataUrl}" alt="Pairing QR" class="h-44 w-44" /></div>`
              : `<div class="text-xs text-ink-400">No pairing code — the server has not reported an API key yet.</div>`
          }
          <div class="mt-3 space-y-1 text-xs text-ink-500 break-all">
            <div>Address: <code class="rounded bg-surface-800 px-1">${escapeHtml(lanUrl || "—")}</code>
              <button class="btn btn-ghost btn-xs ml-2" data-action="sync-copy-url">Copy</button></div>
            <div>API key: <code class="select-all rounded bg-surface-800 px-1">${escapeHtml(apiKey || "—")}</code>
              <button class="btn btn-ghost btn-xs ml-2" data-action="sync-copy-key">Copy</button></div>
            <div>Payload: <code class="rounded bg-surface-800 px-1">${escapeHtml(payload)}</code>
              <button class="btn btn-ghost btn-xs ml-2" data-action="sync-copy-json">Copy JSON</button></div>
          </div>
          ${lanIp && lanIp !== "127.0.0.1" ? "" : `<p class="mt-2 text-xs text-warn-400">Showing loopback — connect to Wi-Fi and refresh to get an address the phone can reach.</p>`}
        </div>
      ${panelClose}`,
    );
    refreshIcons(content);
  };

  const load = async (): Promise<void> => {
    syncAtom.set({ ...syncAtom.get(), status: "loading", message: "" });
    try {
      const info = await api.syncInfo();
      syncAtom.set({
        ...syncAtom.get(),
        status: "idle",
        message: "",
        info: {
          port: info?.port ?? 0,
          localUrl: info?.localUrl ?? "",
          lanUrl: info?.lanUrl ?? "",
          lanIp: info?.lanIp ?? "",
          apiKey: info?.apiKey ?? "",
          isListening: !!info?.isListening,
        },
      });
      const key = syncAtom.get().info?.apiKey ?? "";
      if (key) {
        try {
          const lanUrl = syncAtom.get().info?.lanUrl || syncAtom.get().info?.localUrl || "";
          const qr = await QRCode.toDataURL(pairingPayload(lanUrl, key), { width: 180, margin: 1 });
          syncAtom.set({ ...syncAtom.get(), qrDataUrl: qr });
        } catch {
          syncAtom.set({ ...syncAtom.get(), qrDataUrl: null });
        }
      }
    } catch (e) {
      syncAtom.set({ ...syncAtom.get(), status: "error", message: String(e) });
    }
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const info = syncAtom.get().info;
    const lanUrl = (info?.lanUrl || info?.localUrl) ?? "";
    const apiKey = info?.apiKey ?? "";
    const copy = (text: string): void => {
      void navigator.clipboard.writeText(text).catch(() => {});
    };
    if (target.closest("[data-action='sync-refresh']")) void load();
    else if (target.closest("[data-action='sync-copy-json']")) copy(pairingPayload(lanUrl, apiKey));
    else if (target.closest("[data-action='sync-copy-url']")) copy(lanUrl);
    else if (target.closest("[data-action='sync-copy-key']")) copy(apiKey);
    else if (target.closest("[data-action='sync-copy-curl']")) {
      copy(`curl -s -H "Authorization: Bearer ${apiKey}" ${lanUrl}/health`);
    }
  });

  render();
  void load();
  // Only the status needs refreshing; the pairing details do not change under
  // the user, so this is cheap and stops the panel looking stale.
  const interval = window.setInterval(() => void load(), 10000);
  const unsub = syncAtom.subscribe(render);
  return () => {
    window.clearInterval(interval);
    unsub();
  };
}

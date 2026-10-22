import "./app.css";
import { invoke } from "@tauri-apps/api/core";
import { startRouter } from "./router";
import { routes } from "./views";
import { mountShell } from "./shell/shell";
import { countsAtom, syncAtom } from "./lib/stores";
import { initOverlays } from "./lib/overlay";
import { refreshIcons } from "./lib/icons";

/**
 * Bootstrap. The shell is mounted once; the router swaps only the main outlet
 * from here on. Sync status and dashboard counts are warmed eagerly so the
 * sidebar badges and the topbar dot are live from the first paint.
 */
window.addEventListener("DOMContentLoaded", () => {
  const appRoot = document.getElementById("app");
  if (!appRoot) throw new Error("Maia app root is missing");

  initOverlays();
  mountShell(appRoot);
  startRouter(appRoot.querySelector("[data-slot='outlet']") as HTMLElement, routes);

  // Warm the shell data. These are the only invokes outside lib/api.ts — they
  // exist so the shell is alive before any view loads.
  invoke<unknown>("sync_info")
    .then((info) => syncAtom.set({ ...syncAtom.get(), info: info as never }))
    .catch(() => {});
  invoke<unknown>("get_counts")
    .then((res) => {
      const data = (res as { success?: boolean; data?: never }).data;
      if (data) countsAtom.set(data as never);
    })
    .catch(() => {});
  refreshIcons();
});

// Zoom via CSS custom property on :root — scales all rem units proportionally.
// Ctrl+= / Ctrl+- to zoom in/out, Ctrl+0 to reset.
let currentZoom = 1.0;
document.addEventListener("keydown", (e) => {
  if (!e.ctrlKey && !e.metaKey) return;
  const target = e.target as HTMLElement;
  if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) return;

  const step = 0.1;
  const minZoom = 0.5;
  const maxZoom = 2.0;

  if (e.key === "=" || e.key === "+") {
    e.preventDefault();
    currentZoom = Math.min(maxZoom, currentZoom + step);
    document.documentElement.style.setProperty("--zoom-level", String(currentZoom));
  } else if (e.key === "-") {
    e.preventDefault();
    currentZoom = Math.max(minZoom, currentZoom - step);
    document.documentElement.style.setProperty("--zoom-level", String(currentZoom));
  } else if (e.key === "0") {
    e.preventDefault();
    currentZoom = 1.0;
    document.documentElement.style.setProperty("--zoom-level", "1");
  }
});

// Global nav shortcut: any element with a data-nav attribute navigates via the
// hash router without a full re-render.
document.addEventListener("click", (event) => {
  const target = (event.target as HTMLElement).closest<HTMLElement>("[data-nav]");
  if (target?.dataset.nav) {
    window.location.hash = `#/${target.dataset.nav}`;
  }
});

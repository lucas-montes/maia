import { mountSidebar } from "./sidebar";
import { mountTopbar } from "./topbar";

export interface Shell {
  /** Permanently mounted main outlet views render into. */
  outlet: HTMLElement;
  dispose(): void;
}

/**
 * Build the app shell exactly once: sidebar on the page bg, headings inside
 * the rounded content pane like the daisyUI dashboard template. The sidebar
 * and topbar keep their DOM identity for the whole session (AC1) — only the
 * outlet's content changes when the route changes.
 */
export function mountShell(appRoot: HTMLElement): Shell {
  appRoot.innerHTML = `
    <div class="app-body">
      <div data-slot="sidebar"></div>
      <main class="app-main">
        <div data-slot="topbar"></div>
        <div class="app-main-scroll flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-4" data-slot="outlet"></div>
      </main>
    </div>
  `;

  const disposeTopbar = mountTopbar(appRoot.querySelector("[data-slot='topbar']") as HTMLElement);
  const disposeSidebar = mountSidebar(appRoot.querySelector("[data-slot='sidebar']") as HTMLElement);

  return {
    outlet: appRoot.querySelector("[data-slot='outlet']") as HTMLElement,
    dispose() {
      disposeTopbar();
      disposeSidebar();
    },
  };
}

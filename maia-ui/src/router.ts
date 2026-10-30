import { routeAtom, type ViewId } from "./lib/stores";

export interface ViewModule {
  /** Build the view inside `root`; return a dispose function. */
  mount(root: HTMLElement): () => void;
}

const KNOWN: ViewId[] = [
  "dashboard",
  "exercises",
  "workouts",
  "templates",
  "ingredients",
  "foods",
  "stores",
  "meals",
  "bodyMetrics",
  "todos",
  "receipts",
  "notes",
  "settings",
  "goals",
  "urls",
  "sync",
  "tags",
  "accounts",
  "transactions",
  "statements",
  "fxRates",
];

function viewFromHash(): ViewId {
  const raw = window.location.hash.replace(/^#\/?/, "");
  return (KNOWN as string[]).includes(raw) ? (raw as ViewId) : "dashboard";
}

/**
 * Hash router: swaps only the outlet's content on navigation. The shell
 * (sidebar/topbar) is mounted once and never re-created.
 */
export function startRouter(outlet: HTMLElement, routes: Record<ViewId, ViewModule>): void {
  let dispose: (() => void) | null = null;

  const apply = (): void => {
    const view = viewFromHash();
    routeAtom.set(view);
    dispose?.();
    outlet.innerHTML = "";
    dispose = routes[view].mount(outlet) ?? null;
  };

  window.addEventListener("hashchange", apply);
  apply();
}

import { setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { routeAtom, syncAtom } from "../lib/stores";

const VIEW_TITLES: Record<string, string> = {
  dashboard: "Dashboard",
  exercises: "Exercises",
  workouts: "Workouts",
  templates: "Templates",
  ingredients: "Ingredients",
  foods: "Recipes",
  stores: "Stores",
  meals: "Meals",
  bodyMetrics: "Body Metrics",
  todos: "Tasks",
  goals: "Goals",
  notes: "Notes",
  urls: "Saved URLs",
  receipts: "Receipts",
  accounts: "Accounts",
  transactions: "Transactions",
  tags: "Tags",
  fxRates: "FxRates",
  sync: "Sync",
  settings: "Settings",
};

/** Topbar like the dashboard template: big page title with a trailing
    status chip on the right. */
export function mountTopbar(container: HTMLElement): () => void {
  const render = (): void => {
    const sync = syncAtom.get();
    const listening = sync.info?.isListening ?? false;
    const title = VIEW_TITLES[routeAtom.get()] ?? "Maia";
    setHtml(
      container,
      `
      <header class="app-topbar">
        <h1 class="page-title m-0 text-lg font-semibold">${title}</h1>
        <div class="grow"></div>
        <span class="badge badge-soft badge-sm badge-${listening ? "success" : "error"}" title="${
          listening ? "Server running" : "Server not reachable"
        }">
          <span class="status status-${listening ? "success" : "error"}"></span>
          ${listening ? "Server online" : "Server offline"}
        </span>
      </header>`,
    );
    refreshIcons(container);
  };

  render();
  const unsubs = [routeAtom.subscribe(render), syncAtom.subscribe(render)];
  return () => unsubs.forEach((u) => u());
}

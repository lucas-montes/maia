import { setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { countsAtom, routeAtom, type ViewId } from "../lib/stores";

interface NavItem {
  view: ViewId;
  label: string;
  icon: string;
  /** Which dashboard count to badge, when one applies. */
  countKey?: "tasks" | "goals" | "receipts" | "notes" | "urls";
}

const TOP_LEVEL: NavItem[] = [{ view: "dashboard", label: "Dashboard", icon: "gauge" }];

const GROUPS: { name: string; icon: string; items: NavItem[] }[] = [
  {
    name: "Planning",
    icon: "clipboard-list",
    items: [
      { view: "todos", label: "Tasks", icon: "list-checks", countKey: "tasks" },
      { view: "goals", label: "Goals", icon: "target", countKey: "goals" },
      { view: "notes", label: "Notes", icon: "file-text", countKey: "notes" },
      { view: "urls", label: "URLs", icon: "link-2", countKey: "urls" },
    ],
  },
  {
    name: "Training",
    icon: "dumbbell",
    items: [
      { view: "exercises", label: "Exercises", icon: "dumbbell" },
      { view: "workouts", label: "Workouts", icon: "activity" },
      { view: "templates", label: "Templates", icon: "clipboard-list" },
    ],
  },
  {
    name: "Nutrition",
    icon: "utensils",
    items: [
      { view: "ingredients", label: "Ingredients", icon: "shopping-basket" },
      { view: "foods", label: "Recipes", icon: "chef-hat" },
      { view: "stores", label: "Stores", icon: "store" },
      { view: "meals", label: "Meals", icon: "utensils" },
    ],
  },
  {
    name: "Health",
    icon: "scale",
    items: [
      { view: "bodyMetrics", label: "Body", icon: "scale" },
      { view: "experiments", label: "Experiments", icon: "flask-conical" },
    ],
  },
  {
    name: "Finance",
    icon: "wallet",
    items: [
      { view: "accounts", label: "Accounts", icon: "wallet" },
      { view: "transactions", label: "Transactions", icon: "credit-card" },
      { view: "receipts", label: "Receipts", icon: "receipt", countKey: "receipts" },
    ],
  },
  {
    name: "System",
    icon: "smartphone",
    items: [
      { view: "tags", label: "Tags", icon: "tag" },
      { view: "fxRates", label: "FxRates", icon: "coins" },
      { view: "sync", label: "Sync", icon: "smartphone" },
      { view: "settings", label: "Settings", icon: "settings" },
    ],
  },
];

function navLink(item: NavItem, active: boolean): string {
  const count = item.countKey ? (countsAtom.get()?.[item.countKey] ?? 0) : 0;
  const badge = item.countKey && count > 0 ? `<span class="badge badge-xs">${count}</span>` : "";
  return `<a class="${active ? "menu-active" : ""}" data-view="${item.view}" href="#/${item.view}">
      <i data-lucide="${item.icon}" class="h-4 w-4 opacity-70"></i>
      <span class="grow">${item.label}</span>
      ${badge}
    </a>`;
}

/**
 * Sidebar shaped like the daisyUI dashboard template: brand row, a
 * `menu` of top-level links, `<details name="sidebar-group">` collapsible
 * groups, and a sticky account-style footer.
 */
export function mountSidebar(container: HTMLElement): () => void {
  // Expansion state survives re-renders (counts/route subscribe triggers
  // them); groups start closed.
  const openGroups = new Set<string>();

  const render = (): void => {
    const route = routeAtom.get();
    const topItems = TOP_LEVEL.map((item) => `<li>${navLink(item, route === item.view)}</li>`).join("");
    const groups = GROUPS.map(
      (group) => `
      <li>
        <details name="sidebar-group" data-group="${group.name}" ${openGroups.has(group.name) ? "open" : ""}>
          <summary>
            <i data-lucide="${group.icon}" class="h-4 w-4 opacity-70"></i>
            <span class="grow font-medium">${group.name}</span>
            <span class="badge badge-ghost badge-xs">${group.items.length}</span>
          </summary>
          <ul>
            ${group.items.map((item) => `<li>${navLink(item, route === item.view)}</li>`).join("")}
          </ul>
        </details>
      </li>`,
    ).join("");

    setHtml(
      container,
      `
      <aside class="app-sidebar" aria-label="Primary navigation">
        <div class="app-sidebar-brand">
          <div class="app-sidebar-brand-mark">M</div>
          <p class="m-0 truncate text-lg font-semibold">Maia</p>
        </div>
        <nav class="app-sidebar-nav">
          <ul class="menu menu-sm w-full gap-1">${topItems}${groups}</ul>
        </nav>
        <div class="app-sidebar-footer">
          <button class="btn btn-ghost w-full justify-start gap-3" type="button" data-view="sync" title="Open sync">
            <div class="avatar avatar-placeholder">
              <div class="w-8 rounded-full bg-neutral text-neutral-content">
                <span class="text-sm">DB</span>
              </div>
            </div>
            <span class="grid grow text-left">
              <span class="truncate text-sm font-medium">maia.db</span>
              <span class="truncate text-xs text-base-content/50">local database</span>
            </span>
            <i data-lucide="gauge" class="h-4 w-4 opacity-50"></i>
          </button>
        </div>
      </aside>`,
    );
    refreshIcons(container);
  };

  render();

  const unsubs = [routeAtom.subscribe(render), countsAtom.subscribe(render)];

  const onClick = (event: MouseEvent): void => {
    const viewButton = (event.target as HTMLElement).closest<HTMLElement>("[data-view]");
    if (viewButton?.dataset.view) {
      window.location.hash = `#/${viewButton.dataset.view}`;
    }
  };

  // `toggle` doesn't bubble but reaches this capture-phase listener; keep the
  // expanded-set in sync (the exclusive name="" group fires toggles for the
  // details it auto-closes too).
  const onToggle = (event: Event): void => {
    const details = event.target as HTMLDetailsElement;
    const name = details.dataset.group;
    if (!name) return;
    if (details.open) openGroups.add(name);
    else openGroups.delete(name);
  };

  container.addEventListener("click", onClick);
  container.addEventListener("toggle", onToggle, true);
  return () => {
    container.removeEventListener("click", onClick);
    container.removeEventListener("toggle", onToggle, true);
    unsubs.forEach((unsub) => unsub());
  };
}

import { api } from "../lib/api";
import { countsAtom } from "../lib/stores";
import { setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";

/** Overview: daisyUI `stats` block like the dashboard template, plus quick
    action cards using `list list-row`. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    <div class="flex min-h-0 flex-1 flex-col gap-4" data-slot="content"></div>
  `;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;

  const stat = (title: string, value: number | string, desc: string) => `
    <div class="stat">
      <div class="stat-title">${title}</div>
      <div class="stat-value text-primary">${value}</div>
      <div class="stat-desc">${desc}</div>
    </div>`;

  const render = (): void => {
    const c = countsAtom.get();
    setHtml(
      content,
      `
      <section class="stats stats-vertical bg-base-200 border border-base-300 xl:stats-horizontal w-full">
        ${stat("Tasks", c ? c.tasks : "–", "items to track")}
        ${stat("Receipts", c ? c.receipts : "–", "captured expenses")}
        ${stat("Notes", c ? c.notes : "–", "knowledge entries")}
        ${stat("Goals", c ? c.goals : "–", "active objectives")}
        ${stat("URLs", c ? c.urls : "–", "saved links")}
      </section>

      <section class="card bg-base-200 border border-base-300">
        <div class="card-body">
          <div class="flex justify-between">
            <h2 class="card-title grow">Quick actions</h2>
            <button class="btn btn-sm" type="button" data-nav="sync">Sync status</button>
          </div>
          <p class="text-base-content/60">Jump straight into the modules you use most.</p>
          <ul class="list bg-base-100 rounded-box">
            ${[
              ["todos", "list-checks", "Manage tasks", "Plan and complete your next steps"],
              ["goals", "target", "View goals", "Track progress toward active objectives"],
              ["receipts", "receipt", "Review receipts", "Capture and parse expense receipts"],
              ["notes", "file-text", "Browse notes", "Read-only note archive synced from mobile"],
              ["urls", "link-2", "Saved URLs", "Open freshly captured links"],
              ["settings", "settings", "Configure settings", "Workspace configuration and options"],
            ]
              .map(
                ([view, icon, label, desc]) => `
                <li><a class="list-row" href="#/${view}">
                  <div class="rounded-box grid h-10 w-10 shrink-0 place-items-center bg-base-200 border border-base-300">
                    <i data-lucide="${icon}" class="h-5 w-5"></i>
                  </div>
                  <div>
                    <div class="font-medium">${label}</div>
                    <div class="text-sm text-base-content/60">${desc}</div>
                  </div>
                  <i data-lucide="chevron-right" class="h-4 w-4 self-center opacity-50"></i>
                </a></li>`,
              )
              .join("")}
          </ul>
        </div>
      </section>`,
    );
    refreshIcons(content);
  };

  render();
  const unsub = countsAtom.subscribe(render);
  api
    .getCounts()
    .then((res) => {
      if (res.success && res.data) countsAtom.set(res.data);
    })
    .catch(() => {});
  return unsub;
}

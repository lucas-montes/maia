/** A hand-off between views: "Open" on a receipt's page names the row to show
 *  and navigates; the target view takes it once when it mounts. The router
 *  only knows view names, so the row travels here. */

import type { ViewId } from "./stores";

const pending = new Map<ViewId, string>();

export function openIn(view: ViewId, id: string): void {
  pending.set(view, id);
  window.location.hash = `#/${view}`;
}

/** The row another view asked this one to show, once. */
export function takeFocus(view: ViewId): string | null {
  const id = pending.get(view) ?? null;
  pending.delete(view);
  return id;
}

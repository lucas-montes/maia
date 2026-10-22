/** Small DOM + formatting helpers shared by every view. */

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** Build a single element from an HTML string. */
export function el(html: string): HTMLElement {
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  return template.content.firstElementChild as HTMLElement;
}

/** Replace the children of a container with freshly rendered HTML. */
export function setHtml(container: HTMLElement, html: string): void {
  container.innerHTML = html;
}

/** ISO date-time (DB `created_at` style) to a short local rendering. */
export function formatDate(iso: string): string {
  if (!iso) return "";
  try {
    const d = new Date(iso + "Z");
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

/** Epoch milliseconds to a short local date+time. */
export function formatEpoch(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "";
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function parseTags(tagsJson: string): string[] {
  try {
    const arr = JSON.parse(tagsJson);
    return Array.isArray(arr) ? arr : [];
  } catch {
    return [];
  }
}

export function todayStr(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

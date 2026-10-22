import { escapeHtml } from "../lib/dom";

/** Loading/saving/error banner above view content. */
export function statusBanner(state: { status: string; message: string }): string {
  if (state.status === "loading") return `<div class="banner">Loading...</div>`;
  if (state.status === "saving") return `<div class="banner">Saving...</div>`;
  if (state.status === "saved" && state.message) {
    return `<div class="banner banner-ok">${escapeHtml(state.message)}</div>`;
  }
  if (state.status === "error" && state.message) {
    return `<div class="banner banner-err">${escapeHtml(state.message)}</div>`;
  }
  return "";
}

export function panelOpen(title: string, subtitle = ""): string {
  return `<section class="panel">
    <div class="panel-header">
      <h2 class="panel-title">${escapeHtml(title)}</h2>
      ${subtitle ? `<p class="panel-subtitle">${escapeHtml(subtitle)}</p>` : ""}
    </div>
    <div class="panel-content space-y-4">`;
}

export const panelClose = `</div></section>`;

export function emptyState(text: string): string {
  return `<div class="empty-state">${escapeHtml(text)}</div>`;
}

export function backToDashboard(): string {
  return `<button class="btn btn-ghost btn-sm" type="button" data-nav="dashboard">← Back to Dashboard</button>`;
}

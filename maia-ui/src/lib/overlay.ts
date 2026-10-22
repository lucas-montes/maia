import { el } from "./dom";
import { refreshIcons } from "./icons";

/* Toasts and modals live outside the view outlet so they survive route
   changes; both use DaisyUI components. */

function ensureToastStack(): HTMLElement {
  let stack = document.getElementById("toast-stack");
  if (!stack) {
    stack = el(`<div class="toast toast-end toast-bottom z-[60]" id="toast-stack" aria-live="polite"></div>`);
    document.body.appendChild(stack);
  }
  return stack;
}

export function toast(message: string, kind: "ok" | "err" | "info" = "info"): void {
  const stack = ensureToastStack();
  const alertClass =
    kind === "ok" ? "alert-success" : kind === "err" ? "alert-error" : "alert-info";
  const node = el(`<div class="alert ${alertClass} py-2 text-sm shadow-lg"><span></span></div>`);
  (node.firstElementChild as HTMLElement).textContent = message;
  stack.appendChild(node);
  window.setTimeout(() => node.remove(), 4000);
}

export function confirmDialog(
  title: string,
  body: string,
  confirmLabel = "Delete",
): Promise<boolean> {
  let root = document.getElementById("modal-root") as HTMLElement | null;
  if (!root) {
    root = el(`<div id="modal-root"></div>`);
    document.body.appendChild(root);
  }
  root.innerHTML = "";
  const dialog = el(`
    <dialog class="modal modal-open">
      <div class="modal-box max-w-sm">
        <h3 class="text-base font-semibold text-ink-100"></h3>
        <p class="py-4 text-sm text-ink-400"></p>
        <div class="modal-action">
          <button class="btn btn-ghost btn-sm" data-x="cancel">Cancel</button>
          <button class="btn btn-error btn-sm" data-x="confirm"></button>
        </div>
      </div>
      <form method="dialog" class="modal-backdrop"><button>close</button></form>
    </dialog>`);
  (dialog.querySelector("h3") as HTMLElement).textContent = title;
  (dialog.querySelector("p") as HTMLElement).textContent = body;
  (dialog.querySelector("[data-x='confirm']") as HTMLElement).textContent = confirmLabel;
  root.appendChild(dialog);

  return new Promise((resolve) => {
    const done = (v: boolean) => {
      dialog.remove();
      document.removeEventListener("keydown", onKey, true);
      resolve(v);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        done(false);
      }
    };
    document.addEventListener("keydown", onKey, true);
    dialog.querySelector("[data-x='cancel']")?.addEventListener("click", () => done(false));
    dialog.querySelector("[data-x='confirm']")?.addEventListener("click", () => done(true));
    dialog.querySelector(".modal-backdrop")?.addEventListener("click", () => done(false));
    (dialog.querySelector("[data-x='cancel']") as HTMLElement)?.focus();
  });
}

/** Generic error surface for actions that have no inline status row. */
export function reportError(context: string, e: unknown): void {
  toast(`${context}: ${e}`, "err");
}

export function initOverlays(): void {
  ensureToastStack();
  if (!document.getElementById("modal-root")) {
    document.body.appendChild(el(`<div id="modal-root"></div>`));
  }
  refreshIcons();
}

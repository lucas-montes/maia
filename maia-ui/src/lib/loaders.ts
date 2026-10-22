import { unwrapList } from "./api";
import { toast } from "./overlay";
import type { ListState, Status } from "./stores";

type ListAtom = { get(): ListState<any>; set(state: ListState<any>): void };

/**
 * Load a list command into a list atom. Handles both response shapes and the
 * "keep the old items visible while loading" behaviour the old UI had.
 */
export async function loadList(
  atom: ListAtom,
  cmd: () => Promise<unknown>,
  opts: { silent?: boolean; errorMessage?: string } = {},
): Promise<void> {
  const current = atom.get();
  atom.set({ ...current, status: "loading", message: "" });
  if (!opts.silent) {
    // First paint with the loading state happens by the caller re-rendering.
  }
  try {
    const res = await cmd();
    const failure = (res as { success?: boolean; error?: string })?.success === false
      ? (res as { error?: string }).error
      : null;
    if (failure) {
      atom.set({ ...atom.get(), status: "error", message: failure });
      return;
    }
    atom.set({ ...atom.get(), items: unwrapList(res), status: "idle", message: "" });
  } catch (e) {
    atom.set({ ...atom.get(), status: "error", message: String(e) });
    if (opts.errorMessage) toast(`${opts.errorMessage}: ${e}`, "err");
  }
}

export function withStatus(state: ListState<any>, status: Status): ListState<any> {
  return { ...state, status };
}

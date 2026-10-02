// Talks to the search backend: Tauri commands/events in the desktop app,
// HTTP + server-sent events in the web app.

import type { StoreResult } from "./results";

export type SiteCheck = { ok: boolean; status: number | null; detail: string };

export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function checkSites(): Promise<Record<string, SiteCheck>> {
  if (isDesktop) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<Record<string, SiteCheck>>("check_sites");
  }
  const response = await fetch("/api/check");
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.json();
}

/** Ends the web search in progress, if any (a newer search replaces it). */
let cancelActive: (() => void) | null = null;

/** Runs a search, calling `onStore` per store as results arrive. Resolves when every store is done. */
export async function searchStores(searchId: string, term: string, stores: string[], onStore: (result: StoreResult) => void) {
  if (isDesktop) {
    const [{ invoke }, { listen }] = await Promise.all([import("@tauri-apps/api/core"), import("@tauri-apps/api/event")]);
    const stop = await listen<StoreResult>("search-store", (event) => {
      if (event.payload.searchId === searchId) onStore(event.payload);
    });
    try {
      await invoke("search_stores", { searchId, term, stores });
    } finally {
      stop();
    }
    return;
  }

  cancelActive?.();
  const params = new URLSearchParams({ id: searchId, term, stores: stores.join(",") });
  const stream = new EventSource(`/api/search?${params}`);
  await new Promise<void>((resolve, reject) => {
    const finish = (error?: Error) => {
      stream.close();
      if (cancelActive === finish) cancelActive = null;
      if (error) reject(error); else resolve();
    };
    cancelActive = finish;
    stream.addEventListener("store", (event) => onStore(JSON.parse((event as MessageEvent).data)));
    stream.addEventListener("done", () => finish());
    stream.addEventListener("error", (event) => {
      const data = (event as MessageEvent).data;
      // A server "error" event carries a message; a bare one means the connection dropped.
      finish(new Error(data ? JSON.parse(data) : "서버와 연결이 끊겼어요. 잠시 후 다시 시도해 주세요."));
    });
  });
}

export async function openExternal(url: string) {
  if (isDesktop) {
    const { open } = await import("@tauri-apps/plugin-shell");
    await open(url);
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

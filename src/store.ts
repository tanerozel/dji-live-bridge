import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { getSnapshot, normalizeError } from "./lib/backend";
import type { BridgeSnapshot, ErrorPayload } from "./types";

interface BridgeStore {
  snapshot: BridgeSnapshot | null;
  uiError: ErrorPayload | null;
  initialized: boolean;
  setUiError: (error: unknown) => void;
  clearUiError: () => void;
  initialize: () => Promise<() => void>;
}

export const useBridgeStore = create<BridgeStore>((set) => ({
  snapshot: null,
  uiError: null,
  initialized: false,
  setUiError: (error) => set({ uiError: normalizeError(error) }),
  clearUiError: () => set({ uiError: null }),
  initialize: async () => {
    const unlisten = await listen<BridgeSnapshot>("bridge://state", (event) => {
      set({ snapshot: event.payload, initialized: true });
    });
    try {
      const snapshot = await getSnapshot();
      set({ snapshot, initialized: true });
    } catch (error) {
      set({ uiError: normalizeError(error), initialized: true });
    }
    return unlisten;
  },
}));

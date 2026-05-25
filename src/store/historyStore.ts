import { create } from "zustand";
import { SessionEntry } from "../types";
import { invoke } from "@tauri-apps/api/core";

interface HistoryState {
  sessions: SessionEntry[];
  isOpen: boolean;
  load: () => Promise<void>;
  toggle: () => void;
  deleteSession: (id: string) => Promise<void>;
  clearAll: () => Promise<void>;
}

export const useHistoryStore = create<HistoryState>((set) => ({
  sessions: [],
  isOpen: false,

  load: async () => {
    try {
      const sessions = await invoke<SessionEntry[]>("get_history");
      set({ sessions });
    } catch {
      // ignore
    }
  },

  toggle: () => set((s) => ({ isOpen: !s.isOpen })),

  deleteSession: async (id) => {
    await invoke("delete_session", { id });
    set((s) => ({ sessions: s.sessions.filter((e) => e.id !== id) }));
  },

  clearAll: async () => {
    await invoke("clear_history");
    set({ sessions: [] });
  },
}));

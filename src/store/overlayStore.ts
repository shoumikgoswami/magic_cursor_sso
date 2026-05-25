import { create } from "zustand";
import {
  CapturedContext,
  ConversationMessage,
  OverlayMode,
  OverlayStatus,
} from "../types";

interface OverlayState {
  status: OverlayStatus;
  mode: OverlayMode;
  // Current streaming buffer (cleared once finalized into conversationMessages)
  response: string;
  // Full conversation history for the current session
  conversationMessages: ConversationMessage[];
  context: CapturedContext | null;
  model: string;

  reset: () => void;
  appendChunk: (chunk: string) => void;
  setStatus: (status: OverlayStatus) => void;
  setMode: (mode: OverlayMode) => void;
  setContext: (ctx: CapturedContext) => void;
  setModel: (model: string) => void;
  /** Add a user turn to the conversation display immediately */
  addUserMessage: (content: string) => void;
  /** Move current streaming response into conversation history as an assistant turn */
  finalizeResponse: () => void;
  loadSession: (response: string, query: string) => void;
}

export const useOverlayStore = create<OverlayState>((set) => ({
  status: "idle",
  mode: "chip",
  response: "",
  conversationMessages: [],
  context: null,
  model: "",

  reset: () =>
    set({
      status: "idle",
      mode: "chip",
      response: "",
      conversationMessages: [],
      context: null,
    }),

  appendChunk: (chunk) =>
    set((state) => ({ response: state.response + chunk })),

  setStatus: (status) => set({ status }),

  setMode: (mode) => set({ mode }),

  setContext: (ctx) => set({ context: ctx }),

  setModel: (model) => set({ model }),

  addUserMessage: (content) =>
    set((state) => ({
      conversationMessages: [
        ...state.conversationMessages,
        { role: "user", content },
      ],
    })),

  finalizeResponse: () =>
    set((state) => {
      if (!state.response) return {};
      return {
        conversationMessages: [
          ...state.conversationMessages,
          { role: "assistant", content: state.response },
        ],
        response: "",
      };
    }),

  loadSession: (response, _query) =>
    set({
      conversationMessages: [{ role: "assistant", content: response }],
      response: "",
      status: "done",
      mode: "ask",
    }),
}));

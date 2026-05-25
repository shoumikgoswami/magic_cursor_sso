import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useOverlayStore } from "../store/overlayStore";
import { CapturedContext } from "../types";


export function useShakeOverlay() {
  const { appendChunk, setStatus, setContext, setMode, reset, finalizeResponse } = useOverlayStore();

  useEffect(() => {
    let mounted = true;
    const unlisteners: Array<() => void> = [];

    Promise.all([
      listen<{ x: number; y: number }>("shake-detected", () => {
        console.log("[useShakeOverlay] shake-detected");
        reset();
        setStatus("idle");
        setMode("chip");
      }),
      listen<CapturedContext>("context-ready", (e) => {
        console.log("[useShakeOverlay] context-ready:", e.payload.content_type);
        setContext(e.payload);
        // Determine initial mode from context
        if (e.payload.quick_actions && e.payload.quick_actions.length > 0) {
          setMode("chip");
        } else {
          setMode("ask");
        }
      }),
      listen<string>("ollama-chunk", (e) => {
        console.log("[useShakeOverlay] ollama-chunk, len:", e.payload.length);
        setStatus("streaming");
        appendChunk(e.payload);
      }),
      listen("ollama-done", () => {
        console.log("[useShakeOverlay] ollama-done");
        // Move the streamed response into conversation history
        finalizeResponse();
        setStatus("done");
      }),
      listen<string>("ollama-error", (e) => {
        console.log("[useShakeOverlay] ollama-error:", e.payload);
        setStatus("error");
        appendChunk(`\n\n**Error:** ${e.payload}`);
      }),
    ]).then((fns) => {
      if (!mounted) {
        fns.forEach((fn) => fn());
      } else {
        unlisteners.push(...fns);
      }
    });

    return () => {
      mounted = false;
      unlisteners.forEach((fn) => fn());
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const store = useOverlayStore();
  return {
    status: store.status,
    mode: store.mode,
    response: store.response,
    conversationMessages: store.conversationMessages,
    context: store.context,
    model: store.model,
  };
}

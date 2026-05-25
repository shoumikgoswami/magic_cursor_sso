import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useShakeOverlay } from "../hooks/useShakeOverlay";
import { useOverlayStore } from "../store/overlayStore";
import { ContextBanner } from "./ContextBanner";
import { ResponseStream } from "./ResponseStream";
import { InputBar } from "./InputBar";
import { QuickActions } from "./QuickActions";
import { HistoryPanel } from "./HistoryPanel";
import { BubbleResponse } from "./BubbleResponse";
import { AppConfig } from "../types";
import styles from "../styles/overlay.module.css";

const BAR_HEIGHT = 64;
const EXPANDED_HEIGHT = 400;
const IDLE_TIMEOUT_MS = 1000;
const ACTIVE_TIMEOUT_MS = 3000;

function doHide() {
  invoke("hide_overlay").catch(console.error);
}

export function Overlay() {
  const { status, response, conversationMessages, context, model, mode } = useShakeOverlay();
  const store = useOverlayStore();
  const { setStatus, appendChunk, setModel, setMode, addUserMessage } = store;
  const [query, setQuery] = useState("");
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [accessibilityBanner, setAccessibilityBanner] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const accessibilityShownRef = useRef(false);

  // Load config on mount, then verify the saved model is actually installed.
  // On a fresh system "llama3.2" may not exist — auto-select the first
  // available model so the user doesn't have to open Settings first.
  useEffect(() => {
    invoke<AppConfig>("get_config").then((cfg) => {
      setConfig(cfg);
      const saved = cfg.default_model.trim();
      if (saved) setModel(saved);

      invoke<string[]>("list_models").then((models) => {
        if (models.length === 0) return; // no models yet; query will error helpfully
        const available = models.map((m) => m.toLowerCase());
        if (saved && available.includes(saved.toLowerCase())) return; // all good
        // Configured model not found — silently use first available this session
        setModel(models[0]);
      }).catch(() => {}); // Ollama not running yet is fine; surfaces as query error
    });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // macOS Accessibility permission banner
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen("macos-accessibility-needed", () => {
      if (!accessibilityShownRef.current) {
        accessibilityShownRef.current = true;
        setAccessibilityBanner(true);
      }
    }).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Keep model in sync when settings saved
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ default_model: string }>("config-updated", (e) => {
      const saved = e.payload.default_model.trim();
      if (saved) setModel(saved);
      invoke<AppConfig>("get_config").then(setConfig);
    }).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Auto-hide timer ──
  const cancelHide = useCallback(() => {
    if (timerRef.current) { clearTimeout(timerRef.current); timerRef.current = null; }
  }, []);

  const scheduleHide = useCallback((ms: number) => {
    cancelHide();
    timerRef.current = setTimeout(() => {
      console.log("[Overlay] Auto-hide timer fired");
      doHide();
    }, ms);
    console.log(`[Overlay] Scheduled hide in ${ms}ms`);
  }, [cancelHide]);

  useEffect(() => {
    scheduleHide(IDLE_TIMEOUT_MS);
    return () => cancelHide();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // When status changes, adjust the hide timer
  useEffect(() => {
    console.log("[Overlay] Status changed:", status, "mode:", mode, "response length:", response.length);
    if (status === "thinking" || status === "streaming") {
      scheduleHide(30_000);
    } else if (status === "done") {
      cancelHide();
    } else if (status === "error") {
      scheduleHide(ACTIVE_TIMEOUT_MS);
    }
  }, [status]); // eslint-disable-line react-hooks/exhaustive-deps

  // Activity listener — resets timer on mouse/keyboard interaction
  useEffect(() => {
    const onActivity = () => {
      const liveStatus = useOverlayStore.getState().status;
      if (liveStatus !== "done") {
        const isStreaming = liveStatus === "thinking" || liveStatus === "streaming";
        scheduleHide(isStreaming ? 30_000 : ACTIVE_TIMEOUT_MS);
      }
    };
    window.addEventListener("mousemove", onActivity);
    window.addEventListener("keydown", onActivity);
    window.addEventListener("click", onActivity);
    return () => {
      window.removeEventListener("mousemove", onActivity);
      window.removeEventListener("keydown", onActivity);
      window.removeEventListener("click", onActivity);
    };
  }, [scheduleHide]);

  // Focus-blur hide
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        console.log("[Overlay] Focus changed:", focused);
        if (!focused) doHide();
      })
      .then((fn) => (unlisten = fn))
      .catch(console.error);
    return () => unlisten?.();
  }, []);

  // Keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (historyOpen) {
          setHistoryOpen(false);
        } else {
          doHide();
        }
        return;
      }
      // ⌘H / Ctrl+H — toggle history
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "h") {
        e.preventDefault();
        setHistoryOpen((v) => !v);
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [historyOpen]);

  // ── Dynamic resize ──
  const hasConversation = conversationMessages.length > 0;
  const isExpanded = historyOpen || (mode !== "bubble" && (status !== "idle" || !!response || hasConversation));
  useEffect(() => {
    console.log("[Overlay] isExpanded:", isExpanded, "→ resizing to", isExpanded ? EXPANDED_HEIGHT : BAR_HEIGHT);
    invoke("resize_overlay", { height: isExpanded ? EXPANDED_HEIGHT : BAR_HEIGHT }).catch(console.error);
  }, [isExpanded]);

  // ── Handlers ──
  const handleSubmit = useCallback(
    async (q: string) => {
      console.log("[Overlay] Submitting query:", q);
      setQuery("");
      setStatus("thinking");
      setHistoryOpen(false);
      scheduleHide(30_000);

      // Snapshot history BEFORE adding the new user message
      const historySnapshot = useOverlayStore.getState().conversationMessages;
      addUserMessage(q);

      const ct = context?.content_type ?? "unknown";
      const preview = context?.selected_text?.slice(0, 80) ?? "";

      try {
        console.log("[Overlay] Stream query invoke, model:", model, "history:", historySnapshot.length);
        setMode("ask");
        const images = context?.screenshot_b64 ? [context.screenshot_b64] : undefined;
        await invoke("stream_query", {
          prompt: q,
          model,
          system: undefined,
          images,
          conversationHistory: historySnapshot,
          query: q,
          contentType: ct,
          contextPreview: preview,
        });
      } catch (err) {
        console.error("[Overlay] Query failed:", err);
        setStatus("error");
        appendChunk(`\n\n**Error:** ${String(err)}`);
      }
    },
    [context, model, setStatus, appendChunk, scheduleHide, setMode, addUserMessage]
  );

  const handleQuickAction = useCallback((prompt: string) => {
    setQuery(prompt);
    handleSubmit(prompt);
  }, [handleSubmit]);

  const handleBubbleExpand = useCallback(() => {
    setMode("ask");
  }, [setMode]);

  const showQuickActions =
    !historyOpen &&
    mode === "chip" &&
    status === "idle" &&
    !response &&
    config?.quick_actions_enabled &&
    (context?.quick_actions?.length ?? 0) > 0;

  const showBubble = !historyOpen && mode === "bubble" && (status === "streaming" || status === "done");

  // Glow the border when actively thinking or streaming
  const isActive = status === "thinking" || status === "streaming";
  const cardClass = isExpanded
    ? `${styles.overlayCard} ${isActive ? styles.overlayCardGlow : ""}`
    : "";

  return (
    <div className={styles.overlayRoot}>
      {/* macOS Accessibility permission banner — shown once if rdev can't start */}
      {accessibilityBanner && (
        <div style={{
          background: "#1a1a2e", border: "1px solid #f59e0b", borderRadius: 8,
          color: "#fbbf24", fontSize: 12, padding: "10px 14px", marginBottom: 6,
          lineHeight: 1.5, position: "relative",
        }}>
          <strong>Accessibility permission needed</strong>
          <button
            onClick={() => setAccessibilityBanner(false)}
            style={{ position: "absolute", top: 6, right: 10, background: "none",
              border: "none", color: "#fbbf24", cursor: "pointer", fontSize: 14 }}
          >✕</button>
          <div style={{ marginTop: 4 }}>
            Magic Cursor needs Accessibility access to detect mouse shakes.
            Go to <strong>System Settings → Privacy &amp; Security → Accessibility</strong> and add Magic Cursor, then relaunch.
          </div>
        </div>
      )}
      <div className={cardClass} style={{ position: "relative" }}>
        {showBubble && (
          <BubbleResponse
            text={response}
            status={status}
            delayMs={config?.auto_dismiss_delay_ms ?? 5000}
            onExpand={handleBubbleExpand}
          />
        )}

        {/* Always render expandedArea — use CSS to show/hide to prevent mount/unmount issues */}
        <div
          className={styles.expandedArea}
          style={{
            display: isExpanded ? "flex" : "none",
            flexDirection: "column",
          }}
        >
          {historyOpen ? (
            <HistoryPanel onClose={() => setHistoryOpen(false)} />
          ) : (
            <>
              {showQuickActions && (
                <QuickActions
                  actions={context!.quick_actions!}
                  onSelect={handleQuickAction}
                  visible={true}
                />
              )}
              <ContextBanner context={context} />
              {!showBubble && (
                <ResponseStream
                  messages={conversationMessages}
                  response={response}
                  status={status}
                  cleanResponses={config?.clean_responses ?? false}
                />
              )}
            </>
          )}
        </div>

        <InputBar
          onSubmit={handleSubmit}
          status={status}
          model={model}
          value={query}
          onChange={setQuery}
          docked={isExpanded}
          onHistoryToggle={() => setHistoryOpen((v) => !v)}
        />
      </div>
    </div>
  );
}

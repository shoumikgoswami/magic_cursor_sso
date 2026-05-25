import { memo, useRef, useEffect, KeyboardEvent } from "react";
import { OverlayStatus } from "../types";
import styles from "../styles/overlay.module.css";

interface Props {
  onSubmit: (query: string) => void;
  status: OverlayStatus;
  model: string;
  value: string;
  onChange: (v: string) => void;
  docked?: boolean;
  onHistoryToggle?: () => void;
}

export const InputBar = memo(function InputBar({
  onSubmit,
  status,
  model,
  value,
  onChange,
  docked = false,
  onHistoryToggle,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null);

  const isStreaming = status === "thinking" || status === "streaming";

  useEffect(() => {
    const id = setTimeout(() => inputRef.current?.focus(), 50);
    return () => clearTimeout(id);
  }, []);

  const handleKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && !isStreaming && value.trim()) {
      onSubmit(value.trim());
    }
  };

  const barClass = [
    styles.inputBar,
    isStreaming ? styles.barStreaming : "",
    docked ? styles.inputBarDocked : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={barClass}>
      <input
        ref={inputRef}
        className={styles.input}
        type="text"
        placeholder={isStreaming ? "Responding…" : "Ask anything…"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKey}
        disabled={isStreaming}
        autoComplete="off"
        spellCheck={false}
      />

      <button
        className={styles.sendBtn}
        onClick={() => !isStreaming && value.trim() && onSubmit(value.trim())}
        disabled={isStreaming || !value.trim()}
        title="Send  (Enter)"
        aria-label="Send"
      >
        {isStreaming ? (
          <span style={{ fontSize: 18, lineHeight: 1 }}>·</span>
        ) : (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M5 12h14M13 6l6 6-6 6"/>
          </svg>
        )}
      </button>

      <span className={styles.modelBadge} title={model}>
        {model.replace(/:latest$/, "") || "no model"}
      </span>

      {onHistoryToggle && (
        <button
          className={styles.historyToggleBtn}
          onClick={onHistoryToggle}
          title="Session history"
          aria-label="Toggle history"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10"/>
            <polyline points="12 6 12 12 16 14"/>
          </svg>
        </button>
      )}
    </div>
  );
});

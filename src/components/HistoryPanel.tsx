import { memo, useEffect, useState, useRef, useCallback } from "react";
import { useHistoryStore } from "../store/historyStore";
import { useOverlayStore } from "../store/overlayStore";
import { SessionEntry } from "../types";
import styles from "../styles/overlay.module.css";

interface Props {
  onClose: () => void;
}

const TYPE_ICON: Record<string, string> = {
  code: "{ }",
  text: "¶",
  image: "◨",
  url: "⬡",
};

export const HistoryPanel = memo(function HistoryPanel({ onClose }: Props) {
  const { sessions, load, deleteSession, clearAll } = useHistoryStore();
  const loadSession = useOverlayStore((s) => s.loadSession);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void load();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Focus the list on mount so keyboard events work
  useEffect(() => {
    listRef.current?.focus();
  }, []);

  // Clamp selected index when sessions change (e.g. after deletion)
  useEffect(() => {
    if (sessions.length === 0) {
      setSelectedIdx(0);
    } else if (selectedIdx >= sessions.length) {
      setSelectedIdx(sessions.length - 1);
    }
  }, [sessions.length]); // eslint-disable-line react-hooks/exhaustive-deps

  // Keyboard navigation
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (sessions.length === 0) return;

      switch (e.key) {
        case "ArrowUp":
          e.preventDefault();
          setSelectedIdx((i) => (i > 0 ? i - 1 : sessions.length - 1));
          break;
        case "ArrowDown":
          e.preventDefault();
          setSelectedIdx((i) => (i < sessions.length - 1 ? i + 1 : 0));
          break;
        case "Enter":
          e.preventDefault();
          if (sessions[selectedIdx]) {
            handleSelect(sessions[selectedIdx]);
          }
          break;
        case "Delete":
        case "Backspace":
          e.preventDefault();
          if (sessions[selectedIdx]) {
            void deleteSession(sessions[selectedIdx].id);
            setSelectedIdx((i) => Math.min(i, sessions.length - 2));
          }
          break;
        case "Escape":
          e.preventDefault();
          onClose();
          break;
      }
    },
    [sessions, selectedIdx, onClose, deleteSession]
  );

  const handleSelect = (s: SessionEntry) => {
    loadSession(s.response, s.query);
    onClose();
  };

  const typeClass = (t: string) => {
    if (t === "code") return styles.historyTypeCode;
    if (t === "image") return styles.historyTypeImage;
    if (t === "url") return styles.historyTypeUrl;
    return "";
  };

  const thumbClass = (t: string) => {
    if (t === "code") return styles.historyThumbCode;
    if (t === "text") return styles.historyThumbText;
    if (t === "image") return styles.historyThumbImage;
    if (t === "url") return styles.historyThumbUrl;
    return styles.historyThumb;
  };

  return (
    <div className={styles.historyFull}>
      <div className={styles.historyHeader}>
        <span>Session History</span>
        <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
          {sessions.length > 0 && (
            <button className={styles.historyClearBtn} onClick={() => void clearAll()}>
              Clear all
            </button>
          )}
          <button className={styles.historyCloseBtn} onClick={onClose}>✕</button>
        </div>
      </div>

      <div
        ref={listRef}
        className={styles.historyList}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        style={{ outline: "none" }}
      >
        {sessions.length === 0 ? (
          <div className={styles.historyEmpty}>No sessions yet</div>
        ) : (
          sessions.map((s, idx) => (
            <div
              key={s.id}
              className={`${styles.historyEntry} ${idx === selectedIdx ? styles.historyEntryActive : ""}`}
              onClick={() => handleSelect(s)}
              onMouseEnter={() => setSelectedIdx(idx)}
              role="button"
              tabIndex={-1}
            >
              <div className={thumbClass(s.content_type)}>
                {TYPE_ICON[s.content_type] ?? "◈"}
              </div>

              <div className={styles.historyBody}>
                <div className={styles.historyMeta}>
                  <span className={`${styles.historyType} ${typeClass(s.content_type)}`}>
                    {s.content_type}
                  </span>
                  <span className={styles.historyTime}>
                    {new Date(s.timestamp).toLocaleString([], {
                      month: "short", day: "numeric",
                      hour: "2-digit", minute: "2-digit",
                    })}
                  </span>
                </div>
                <div className={styles.historyQuery}>{s.query}</div>
                {s.context_preview && (
                  <div className={styles.historyPreview}>{s.context_preview}</div>
                )}
              </div>

              <div className={styles.historyToolsMini}>
                <span className={styles.dots}>
                  {Array.from({ length: Math.min(s.tool_calls.length || 1, 3) }).map((_, i) => (
                    <i key={i} />
                  ))}
                </span>
                <span>{s.tool_calls.length || 1} TOOL{s.tool_calls.length !== 1 ? "S" : ""}</span>
              </div>

              <button
                className={styles.historyDeleteBtn}
                onClick={(e) => { e.stopPropagation(); void deleteSession(s.id); }}
                title="Delete"
              >
                ✕
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
});

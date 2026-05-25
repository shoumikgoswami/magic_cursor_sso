import { memo, useEffect, useState, useCallback } from "react";
import { useOverlayStore } from "../store/overlayStore";
import styles from "../styles/overlay.module.css";

interface Props {
  text: string;
  status: "thinking" | "streaming" | "done";
  delayMs?: number;
  onExpand: () => void;
}

export const BubbleResponse = memo(function BubbleResponse({
  text,
  status,
  delayMs = 5000,
  onExpand,
}: Props) {
  const [visible, setVisible] = useState(false);
  const [dismissStyle, setDismissStyle] = useState<React.CSSProperties>({});
  const setMode = useOverlayStore((s) => s.setMode);

  useEffect(() => {
    // Small entrance delay for smoothness
    const t = setTimeout(() => setVisible(true), 50);
    return () => clearTimeout(t);
  }, []);

  useEffect(() => {
    if (status === "done" && delayMs > 0) {
      setDismissStyle({
        animationDuration: `${delayMs}ms`,
      });
      const t = setTimeout(() => {
        setVisible(false);
        // After fade out, switch back to chip mode
        setTimeout(() => setMode("chip"), 400);
      }, delayMs + 400);
      return () => clearTimeout(t);
    }
  }, [status, delayMs, setMode]);

  const handleClick = useCallback(() => {
    setVisible(false);
    setTimeout(() => {
      setMode("ask");
      onExpand();
    }, 200);
  }, [onExpand, setMode]);

  const isStreaming = status === "thinking" || status === "streaming";

  return (
    <div
      className={`${styles.bubble} ${visible ? styles.bubbleVisible : ""}`}
      onClick={handleClick}
      title="Tap to expand"
    >
      {isStreaming && <span className={styles.bubblePulse} />}
      <span className={styles.bubbleText}>{text || "Thinking…"}</span>
      {status === "done" && (
        <div className={styles.bubbleDismissBar}>
          <div className={styles.bubbleDismissProgress} style={dismissStyle} />
        </div>
      )}
    </div>
  );
});

import { memo, useState } from "react";
import { CapturedContext } from "../types";
import { EntityCard } from "./EntityCard";
import styles from "../styles/overlay.module.css";

interface Props {
  context: CapturedContext | null;
}

export const ContextBanner = memo(function ContextBanner({ context }: Props) {
  const [expanded, setExpanded] = useState(false);

  if (!context) return null;

  // Derive the "location" line from structured app context
  const ac = context.app_context;
  const locationLine = ac?.file_path
    ? `📁 ${ac.file_path}`
    : ac?.browser_url
    ? `🌐 ${ac.browser_url}`
    : ac?.app_name
    ? `⬡ ${ac.app_name}`
    : null;

  const hasText = !!context.selected_text;
  const text = context.selected_text ?? "";
  const preview = text.length > 90 ? text.slice(0, 90) + "…" : text;

  if (!hasText && !locationLine) return null;

  return (
    <>
      {locationLine && (
        <div className={styles.contextBanner} style={{ opacity: 0.75 }}>
          <span className={styles.contextText} style={{ fontSize: 11, fontFamily: "monospace" }}>
            {locationLine}
          </span>
        </div>
      )}
      {hasText && (
        <div
          className={styles.contextBanner}
          onClick={() => setExpanded((v) => !v)}
          title={expanded ? "Click to collapse" : "Click to expand"}
        >
          <span className={styles.contextIcon}>//</span>
          <span className={styles.contextText}>{expanded ? text : preview}</span>
        </div>
      )}
      {context.entity && <EntityCard entity={context.entity} />}
    </>
  );
});

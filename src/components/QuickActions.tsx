import { memo } from "react";
import { QuickAction } from "../types";
import styles from "../styles/overlay.module.css";

interface Props {
  actions: QuickAction[];
  onSelect: (prompt: string) => void;
  visible: boolean;
}

export const QuickActions = memo(function QuickActions({ actions, onSelect, visible }: Props) {
  if (!visible || actions.length === 0) return null;
  return (
    <div className={styles.quickActions}>
      {actions.map((a) => (
        <button key={a.label} className={styles.chip} onClick={() => onSelect(a.prompt)}>
          {a.label}
        </button>
      ))}
    </div>
  );
});

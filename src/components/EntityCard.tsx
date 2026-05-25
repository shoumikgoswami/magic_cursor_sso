import { memo } from "react";
import { DetectedEntity } from "../types";
import { invoke } from "@tauri-apps/api/core";
import styles from "../styles/overlay.module.css";

const ENTITY_ICONS: Record<string, string> = {
  url: "⬡",
  email: "@",
  date: "◈",
  file_path: "▹",
  price: "$",
  code_snippet: "{ }",
  unknown: "◆",
};

interface Props {
  entity: DetectedEntity;
}

export const EntityCard = memo(function EntityCard({ entity }: Props) {
  const icon = ENTITY_ICONS[entity.type] ?? "◆";

  const handleAction = async (tool: string, args: Record<string, unknown>) => {
    try {
      const baseArgs = {
        selected_text: null as string | null,
        screenshot_b64: null as string | null,
        content_type: "unknown",
      };
      if (tool === "open_url") {
        await invoke("agent_query", {
          prompt: `Open this URL: ${args.url}`,
          ...baseArgs,
        });
      } else if (tool === "copy_to_clipboard") {
        await invoke("agent_query", {
          prompt: `Copy to clipboard: ${args.text}`,
          ...baseArgs,
        });
      } else if (tool === "read_file") {
        await invoke("agent_query", {
          prompt: `Read the file at path: ${args.path}`,
          ...baseArgs,
        });
      } else {
        await invoke("agent_query", {
          prompt: `Execute ${tool} with args: ${JSON.stringify(args)}`,
          ...baseArgs,
        });
      }
    } catch {
      // ignore
    }
  };

  return (
    <div className={styles.entityCard}>
      <span className={styles.entityIcon}>{icon}</span>
      <span className={styles.entityValue} title={entity.value}>
        {entity.value.length > 50 ? entity.value.slice(0, 50) + "…" : entity.value}
      </span>
      <div className={styles.entityActions}>
        {entity.actions.map((action) => (
          <button
            key={action.label}
            className={styles.entityBtn}
            onClick={() => handleAction(action.tool, action.args as Record<string, unknown>)}
          >
            {action.label}
          </button>
        ))}
      </div>
    </div>
  );
});

import { memo, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";
import { ConversationMessage, OverlayStatus } from "../types";
import styles from "../styles/overlay.module.css";

interface Props {
  messages: ConversationMessage[];
  response: string;
  status: OverlayStatus;
  /** When true, strip explanatory preamble before display and insert. */
  cleanResponses?: boolean;
}

marked.setOptions({ breaks: true, gfm: true });

function parseMarkdown(text: string): string {
  return marked.parse(text) as string;
}

// ── Output extraction ──────────────────────────────────────────────────────────
//
// Models often prefix their actual answer with an explanatory paragraph:
//   "It seems like the selected text is... Here's a potential response:"
//   "Based on the context, here is a draft reply:"
//
// extractCleanOutput() strips that preamble and returns only the direct answer.
// It's ALWAYS applied when inserting text; optionally applied for display.

function stripWrappingQuotes(text: string): string {
  // Remove surrounding typographic or straight quote pairs
  return text.replace(/^[""«]|[""»]$/g, "").trim();
}

export function extractCleanOutput(text: string): string {
  const paragraphs = text.split(/\n\n+/).map((p) => p.trim()).filter(Boolean);

  // ── Strategy 1: "Here's/Here is [something]:" at end of any paragraph ──
  // Captures everything after the intro line that ends with a colon.
  const herePattern = /(?:here(?:'s| is)\b[^:\n]*:)\s*$/im;
  for (let i = 0; i < paragraphs.length - 1; i++) {
    if (herePattern.test(paragraphs[i])) {
      return stripWrappingQuotes(paragraphs.slice(i + 1).join("\n\n"));
    }
  }

  // ── Strategy 2: Labelled block — "Response:", "Answer:", "Output:", etc. ──
  const labelPattern = /^(?:response|answer|output|draft|email|message|reply|result|text):\s*\n([\s\S]+)/im;
  const labelMatch = text.match(labelPattern);
  if (labelMatch) return stripWrappingQuotes(labelMatch[1].trim());

  // ── Strategy 3: First paragraph is pure explanation (no actual content) ──
  // Heuristics: starts with a known explanation opener AND the paragraph ends
  // with either a colon or a full stop (not the answer itself).
  const explanationOpener =
    /^(?:it seems(?: like)?|i (?:can see|notice|see that)|the (?:selected|copied|highlighted|provided|given)?\s*text|based on|looking at|this (?:appears|looks|is|text)|from the|the (?:context|message|content|email|selection)|i(?:'ve)? (?:analyzed|reviewed|looked at))/i;

  if (paragraphs.length >= 2 && explanationOpener.test(paragraphs[0])) {
    return stripWrappingQuotes(paragraphs.slice(1).join("\n\n"));
  }

  // ── Strategy 4: First paragraph ends with ":" (common intro pattern) ──
  if (paragraphs.length >= 2 && paragraphs[0].endsWith(":")) {
    return stripWrappingQuotes(paragraphs.slice(1).join("\n\n"));
  }

  // ── Strategy 5: Inline preamble + colon on the same line ──
  // Handles "A response has been generated: 'Hi, ...'" where preamble and
  // content are in a single paragraph separated only by ": ".
  // Match: preamble (10–150 chars, no newline) + ": " + quoted-or-plain content.
  const inlineQuoted = text.trim().match(
    /^[^:\n]{10,150}:\s*(["""''‘’“”])([\s\S]+)\1\s*$/
  );
  if (inlineQuoted) return inlineQuoted[2].trim();

  // Same but without surrounding quotes — only fire when the preamble contains
  // a generation/draft verb so we don't strip colons from normal sentences.
  const inlinePlain = text.trim().match(
    /^[^:\n]{10,150}(?:generated|drafted|written|created|composed|prepared|response|reply|message|email|decline|rejection)(?:[^:\n]{0,60}):\s+([\s\S]+)$/i
  );
  if (inlinePlain) return inlinePlain[1].trim();

  return text.trim();
}

// ── Markdown → plain text for insert ──────────────────────────────────────────
function toPlainText(md: string): string {
  return md
    .replace(/```[\s\S]*?```/g, (m) => m.replace(/```\w*\n?/g, "").trim())
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/#{1,6}\s+/g, "")
    .replace(/>\s+/g, "")
    .trim();
}

// ── Component ──────────────────────────────────────────────────────────────────

export const ResponseStream = memo(function ResponseStream({
  messages,
  response,
  status,
  cleanResponses = false,
}: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);
  const [inserting, setInserting] = useState(false);

  const deferredResponse = useDeferredValue(response);

  // Apply clean extraction to the streaming response if setting is on
  const displayResponse = useMemo(
    () => (cleanResponses && deferredResponse ? extractCleanOutput(deferredResponse) : deferredResponse),
    [deferredResponse, cleanResponses]
  );

  const streamingHtml = useMemo(
    () => (displayResponse ? parseMarkdown(displayResponse) : ""),
    [displayResponse]
  );

  // Auto-scroll on every new chunk or new message
  useEffect(() => {
    const raf = requestAnimationFrame(() => {
      bottomRef.current?.scrollIntoView({ behavior: "instant" });
    });
    return () => cancelAnimationFrame(raf);
  }, [deferredResponse, messages.length]);

  const isStreaming = status === "streaming";
  const isThinking = status === "thinking";
  const isError = status === "error";

  // Last assistant message for action buttons
  const lastAssistant = [...messages].reverse().find((m) => m.role === "assistant");
  const showActions =
    (status === "done" || (isError && !!lastAssistant)) && !!lastAssistant;

  const handleCopy = () => {
    const raw = lastAssistant?.content ?? "";
    if (!raw) return;
    // Copy the clean version if the setting is on, otherwise full response
    const text = cleanResponses ? extractCleanOutput(raw) : raw;
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const handleInsert = async () => {
    const raw = lastAssistant?.content ?? "";
    if (!raw || inserting) return;
    setInserting(true);
    try {
      // Always extract clean output for insert — we only want the actual answer.
      const clean = extractCleanOutput(raw);
      const plain = toPlainText(clean);
      await invoke("insert_text", { text: plain });
    } catch (e) {
      console.error("Insert failed:", e);
    } finally {
      setInserting(false);
    }
  };

  const hasContent = messages.length > 0 || !!response;

  return (
    <div className={`${styles.responseArea} ${hasContent ? styles.fadeIn : ""}`}>

      {/* ── Conversation history ── */}
      {messages.map((msg, i) => {
        if (msg.role === "user") {
          return (
            <div key={i} className={styles.userBubbleWrap}>
              <div className={styles.userBubble}>{msg.content}</div>
            </div>
          );
        }
        const displayContent = cleanResponses
          ? extractCleanOutput(msg.content)
          : msg.content;
        return (
          <div key={i} className={styles.assistantTurn}>
            <div
              className={styles.markdown}
              dangerouslySetInnerHTML={{ __html: parseMarkdown(displayContent) }}
            />
          </div>
        );
      })}

      {/* ── Currently streaming response ── */}
      {isThinking && !response && (
        <div className={styles.thinking}>
          <span className={styles.dots}>
            <span className={styles.dot} />
            <span className={styles.dot} />
            <span className={styles.dot} />
          </span>
          <span>Thinking</span>
        </div>
      )}

      {response && (
        <div className={styles.assistantTurn}>
          <div
            className={styles.markdown}
            dangerouslySetInnerHTML={{ __html: streamingHtml }}
          />
          {isStreaming && <span className={styles.cursor} aria-hidden />}
        </div>
      )}

      {/* ── Action buttons on last assistant message ── */}
      {showActions && (
        <div className={styles.responseActions}>
          <button
            className={styles.insertBtn}
            onClick={handleInsert}
            disabled={inserting}
            title="Insert direct response at cursor position"
          >
            {inserting ? "Inserting…" : "Insert"}
          </button>
          <button className={styles.copyBtn} onClick={handleCopy} title="Copy response">
            {copied ? "Copied" : "Copy"}
          </button>
        </div>
      )}

      <div ref={bottomRef} />
    </div>
  );
});

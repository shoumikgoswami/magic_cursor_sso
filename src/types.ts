export type OverlayMode = "ask" | "act" | "bubble" | "chip";

export interface ConversationMessage {
  role: "user" | "assistant";
  content: string;
}

export type OverlayStatus =
  | "idle"
  | "thinking"
  | "streaming"
  | "done"
  | "error";

export type Provider = "ollama" | "openai" | "groq" | "anthropic";

export interface AppConfig {
  // provider
  provider: Provider;
  ollama_base_url: string;
  openai_api_key: string;
  groq_api_key: string;
  anthropic_api_key: string;
  // models
  default_model: string;
  vision_model: string;
  stt_model: string;        // speech-to-text model; "" = provider default
  system_prompt: string;
  reversal_threshold: number;
  window_ms: number;
  min_displacement: number;
  cooldown_ms: number;
  capture_radius: number;
  overlay_width: number;
  overlay_height: number;
  // v2
  enable_tools: boolean;
  allowed_tool_categories: string[];
  always_ask_before_ui_control: boolean;
  max_tool_iterations: number;
  auto_dismiss_short_responses: boolean;
  auto_dismiss_word_threshold: number;
  auto_dismiss_delay_ms: number;
  quick_actions_enabled: boolean;
  history_max_entries: number;
  read_allowed_roots: string[];
  clean_responses: boolean;  // strip explanatory preamble before display/insert
}

export interface EntityAction {
  label: string;
  tool: string;
  args: Record<string, unknown>;
}

export interface DetectedEntity {
  type: string;
  value: string;
  actions: EntityAction[];
}

export interface QuickAction {
  label: string;
  prompt: string;
}

export interface AppContext {
  app_name?: string;
  browser_url?: string;
  file_path?: string;
}

export interface CapturedContext {
  selected_text?: string;
  has_image: boolean;
  screenshot_b64?: string;
  window_title?: string;
  app_context?: AppContext;
  content_type?: string;
  quick_actions?: QuickAction[];
  entity?: DetectedEntity;
}


export interface SessionEntry {
  id: string;
  timestamp: string;
  content_type: string;
  context_preview: string;
  query: string;
  response: string;
  tool_calls: AuditEntry[];
}

export interface AuditEntry {
  tool: string;
  args: Record<string, unknown>;
  result: string;
  duration_ms: number;
  approved_by: string;
}

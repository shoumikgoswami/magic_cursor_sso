import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppConfig, Provider } from "../types";
import s from "../styles/settings.module.css";

// Static STT model lists — these don't need to be fetched from the API
const STT_MODELS: Partial<Record<Provider, { value: string; label: string }[]>> = {
  openai: [
    { value: "whisper-1", label: "whisper-1" },
  ],
  groq: [
    { value: "whisper-large-v3-turbo", label: "whisper-large-v3-turbo  (fastest)" },
    { value: "whisper-large-v3",       label: "whisper-large-v3  (most accurate)" },
    { value: "distil-whisper-large-v3-en", label: "distil-whisper-large-v3-en  (English only)" },
  ],
};

const PROVIDERS: { id: Provider; label: string }[] = [
  { id: "ollama",    label: "Ollama" },
  { id: "openai",    label: "OpenAI" },
  { id: "groq",      label: "Groq" },
  { id: "anthropic", label: "Anthropic" },
];

const PROVIDER_HINTS: Record<Provider, string> = {
  ollama:    "Local — no key needed",
  openai:    "platform.openai.com → API Keys",
  groq:      "console.groq.com → API Keys",
  anthropic: "console.anthropic.com → API Keys",
};

const DEFAULT_CONFIG: AppConfig = {
  provider: "ollama",
  ollama_base_url: "http://localhost:11434",
  openai_api_key: "",
  groq_api_key: "",
  anthropic_api_key: "",
  default_model: "",
  vision_model: "",
  stt_model: "",
  system_prompt: "You are a helpful AI assistant. Be concise and direct.",
  reversal_threshold: 3,
  window_ms: 600,
  min_displacement: 30,
  cooldown_ms: 2000,
  capture_radius: 350,
  overlay_width: 440,
  overlay_height: 380,
  enable_tools: true,
  allowed_tool_categories: ["Screen", "Clipboard", "FileRead", "Browser"],
  always_ask_before_ui_control: true,
  max_tool_iterations: 6,
  auto_dismiss_short_responses: true,
  auto_dismiss_word_threshold: 40,
  auto_dismiss_delay_ms: 5000,
  quick_actions_enabled: true,
  history_max_entries: 50,
  read_allowed_roots: [],
  clean_responses: false,
};

function close() {
  invoke("hide_settings").catch(() => getCurrentWindow().hide());
}

function apiKeyField(config: AppConfig): keyof AppConfig {
  const map: Record<Provider, keyof AppConfig> = {
    openai:    "openai_api_key",
    groq:      "groq_api_key",
    anthropic: "anthropic_api_key",
    ollama:    "ollama_base_url",
  };
  return map[config.provider];
}

export function Settings() {
  const [config, setConfig] = useState<AppConfig>(DEFAULT_CONFIG);
  const [models, setModels] = useState<string[]>([]);
  const [visionModels, setVisionModels] = useState<string[]>([]);
  const [saved, setSaved] = useState(false);
  const [loading, setLoading] = useState(true);
  const [keyVisible, setKeyVisible] = useState(false);
  const [fetchState, setFetchState] = useState<"idle" | "fetching" | "ok" | "error">("idle");
  const [fetchError, setFetchError] = useState("");
  const fetchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    Promise.all([
      invoke<AppConfig>("get_config"),
      invoke<string[]>("list_models").catch(() => [] as string[]),
    ]).then(([cfg, ms]) => {
      setConfig({ ...DEFAULT_CONFIG, ...cfg });
      setModels(ms);
      setLoading(false);
    });

    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const set = <K extends keyof AppConfig>(key: K, value: AppConfig[K]) =>
    setConfig((c) => ({ ...c, [key]: value }));

  const handleSave = async () => {
    await invoke("save_config_cmd", { config });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  // Fetch model list for the currently selected provider + entered key.
  // Also fetches the vision-only subset for the vision model dropdown.
  const fetchModels = useCallback(async (cfg: AppConfig) => {
    const isOllamaP = cfg.provider === "ollama";
    const key = isOllamaP ? "" : (cfg[apiKeyField(cfg)] as string);
    if (!isOllamaP && !key.trim()) return;

    setFetchState("fetching");
    setFetchError("");
    try {
      const [ms, vms] = await Promise.all([
        invoke<string[]>("list_provider_models", {
          provider: cfg.provider,
          apiKey: key,
          baseUrl: cfg.ollama_base_url,
        }),
        invoke<string[]>("list_provider_vision_models", {
          provider: cfg.provider,
          apiKey: key,
          baseUrl: cfg.ollama_base_url,
        }),
      ]);
      setModels(ms);
      setVisionModels(vms);
      // Auto-pick first model if current default is not in list
      if (ms.length > 0 && !ms.includes(cfg.default_model)) {
        setConfig((c) => ({ ...c, default_model: ms[0] }));
      }
      setFetchState("ok");
      if (fetchTimer.current) clearTimeout(fetchTimer.current);
      fetchTimer.current = setTimeout(() => setFetchState("idle"), 3000);
    } catch (e) {
      setFetchState("error");
      setFetchError(String(e));
    }
  }, []);

  if (loading) {
    return <div className={s.root}><div className={s.loading}>Loading…</div></div>;
  }

  const isOllama = config.provider === "ollama";
  const currentKey = isOllama ? config.ollama_base_url : (config[apiKeyField(config)] as string);
  const modelOptions = models.length > 0 ? models : [config.default_model, config.vision_model].filter(Boolean);
  // Vision dropdown: use the filtered vision list if we have one, else fall back to saved value only
  const visionOptions = visionModels.length > 0
    ? visionModels
    : config.vision_model ? [config.vision_model] : [];

  return (
    <div className={s.root}>
      {/* Title bar — drag region */}
      <div className={s.titleBar} data-tauri-drag-region>
        <span className={s.titleIcon}>⬡</span>
        <span className={s.title}>AI Cursor — Settings</span>
        <button className={s.closeBtn} onClick={close} title="Close (Esc)">✕</button>
      </div>

      <div className={s.body}>

        {/* ── Models ── */}
        <section className={s.section}>
          <h3 className={s.sectionTitle}>Provider & Models</h3>

          {/* Provider tabs */}
          <div className={s.providerTabs}>
            {PROVIDERS.map(({ id, label }) => (
              <button
                key={id}
                className={`${s.providerTab} ${config.provider === id ? s.providerTabActive : ""}`}
                onClick={() => {
                  setConfig((c) => ({ ...c, provider: id }));
                  setModels([]);
                  setVisionModels([]);
                  setFetchState("idle");
                }}
              >
                {label}
              </button>
            ))}
          </div>

          {/* API key / base URL */}
          <div className={s.field}>
            <span className={s.label}>
              {isOllama ? "Ollama URL" : "API Key"}
            </span>
            <div className={s.apiKeyRow}>
              <input
                className={s.comboInput}
                type={!isOllama && !keyVisible ? "password" : "text"}
                value={currentKey}
                onChange={(e) => set(apiKeyField(config), e.target.value as never)}
                placeholder={isOllama ? "http://localhost:11434" : `Enter ${config.provider} API key…`}
                autoComplete="off"
                spellCheck={false}
              />
              {!isOllama && (
                <button className={s.eyeBtn} onClick={() => setKeyVisible((v) => !v)} title="Show / hide">
                  {keyVisible ? "🙈" : "👁"}
                </button>
              )}
              <button
                className={s.testBtn}
                onClick={() => fetchModels(config)}
                disabled={fetchState === "fetching" || (!isOllama && !currentKey.trim())}
              >
                {fetchState === "fetching" ? "…" : fetchState === "ok" ? "✓" : fetchState === "error" ? "✗" : "Load"}
              </button>
            </div>
            <span className={s.fieldHint}>{PROVIDER_HINTS[config.provider]}</span>
            {fetchState === "error" && <span className={s.testError}>{fetchError}</span>}
            {fetchState === "ok" && <span className={s.testOk}>{models.length} model{models.length !== 1 ? "s" : ""} loaded</span>}
          </div>

          {/* Default model */}
          <div className={s.field}>
            <span className={s.label}>Default model</span>
            {isOllama ? (
              <>
                <input
                  className={s.comboInput}
                  type="text"
                  list="default-model-list"
                  value={config.default_model}
                  onChange={(e) => set("default_model", e.target.value)}
                  placeholder="e.g. llama3.2, mistral, gemma3…"
                  autoComplete="off"
                  spellCheck={false}
                />
                <datalist id="default-model-list">
                  {modelOptions.map((m) => <option key={m} value={m} />)}
                </datalist>
              </>
            ) : (
              <select
                className={s.select}
                value={config.default_model}
                onChange={(e) => set("default_model", e.target.value)}
              >
                {modelOptions.length === 0 && (
                  <option value="">— load models first —</option>
                )}
                {modelOptions.map((m) => <option key={m} value={m}>{m}</option>)}
              </select>
            )}
          </div>

          {/* Vision model */}
          <div className={s.field}>
            <span className={s.label}>
              Vision model
              <span className={s.hint}> — screenshot queries only</span>
            </span>
            {isOllama ? (
              <>
                <input
                  className={s.comboInput}
                  type="text"
                  list="vision-model-list"
                  value={config.vision_model}
                  onChange={(e) => set("vision_model", e.target.value)}
                  placeholder="e.g. llava, minicpm-v, moondream…"
                  autoComplete="off"
                  spellCheck={false}
                />
                <datalist id="vision-model-list">
                  <option value="" />
                  {visionOptions.map((m) => <option key={m} value={m} />)}
                </datalist>
              </>
            ) : (
              <select
                className={s.select}
                value={config.vision_model}
                onChange={(e) => set("vision_model", e.target.value)}
              >
                {visionOptions.length === 0 ? (
                  <option value="">— load models first —</option>
                ) : (
                  <>
                    <option value="">— none (screenshots ignored) —</option>
                    {visionOptions.map((m) => <option key={m} value={m}>{m}</option>)}
                  </>
                )}
              </select>
            )}
            {!isOllama && models.length > 0 && visionOptions.length === 0 && (
              <span className={s.fieldHint}>No vision-capable models found for this provider.</span>
            )}
          </div>

          {/* STT model — only shown for providers that support it */}
          {STT_MODELS[config.provider] && (
            <div className={s.field}>
              <span className={s.label}>
                Speech-to-text model
                <span className={s.hint}> — used for mic voice input</span>
              </span>
              <select
                className={s.select}
                value={config.stt_model || STT_MODELS[config.provider]![0].value}
                onChange={(e) => set("stt_model", e.target.value)}
              >
                {STT_MODELS[config.provider]!.map(({ value, label }) => (
                  <option key={value} value={value}>{label}</option>
                ))}
              </select>
            </div>
          )}

          <label className={s.field}>
            <span className={s.label}>System prompt</span>
            <textarea
              className={s.textarea}
              rows={3}
              value={config.system_prompt}
              onChange={(e) => set("system_prompt", e.target.value)}
            />
          </label>
        </section>

        {/* ── Responses ── */}
        <section className={s.section}>
          <h3 className={s.sectionTitle}>Responses</h3>

          <label className={s.field} style={{ flexDirection: "row", alignItems: "center", gap: 10 }}>
            <input
              type="checkbox"
              checked={config.clean_responses}
              onChange={(e) => set("clean_responses", e.target.checked)}
              style={{ width: 14, height: 14, accentColor: "var(--accent)", flexShrink: 0 }}
            />
            <span>
              <span className={s.label} style={{ display: "inline" }}>Extract direct output</span>
              <span className={s.fieldHint} style={{ display: "block", marginTop: 2 }}>
                Strip explanatory preamble from responses. Insert always uses the clean output.
              </span>
            </span>
          </label>
        </section>

        {/* ── Shake detection ── */}
        <section className={s.section}>
          <h3 className={s.sectionTitle}>Shake detection</h3>

          <label className={s.field}>
            <span className={s.label}>
              Sensitivity
              <span className={s.value}>{config.reversal_threshold} reversals</span>
            </span>
            <input
              type="range" className={s.range}
              min={2} max={6} step={1}
              value={config.reversal_threshold}
              onChange={(e) => set("reversal_threshold", Number(e.target.value))}
            />
            <div className={s.rangeLabels}><span>Loose</span><span>Tight</span></div>
          </label>

          <label className={s.field}>
            <span className={s.label}>
              Detection window
              <span className={s.value}>{config.window_ms} ms</span>
            </span>
            <input
              type="range" className={s.range}
              min={300} max={900} step={50}
              value={config.window_ms}
              onChange={(e) => set("window_ms", Number(e.target.value))}
            />
          </label>

          <label className={s.field}>
            <span className={s.label}>
              Min displacement
              <span className={s.value}>{config.min_displacement} px</span>
            </span>
            <input
              type="range" className={s.range}
              min={15} max={80} step={5}
              value={config.min_displacement}
              onChange={(e) => set("min_displacement", Number(e.target.value))}
            />
          </label>

          <label className={s.field}>
            <span className={s.label}>
              Cooldown
              <span className={s.value}>{(config.cooldown_ms / 1000).toFixed(1)} s</span>
            </span>
            <input
              type="range" className={s.range}
              min={500} max={5000} step={500}
              value={config.cooldown_ms}
              onChange={(e) => set("cooldown_ms", Number(e.target.value))}
            />
          </label>
        </section>
      </div>

      {/* ── Footer ── */}
      <div className={s.footer}>
        <button className={s.cancelBtn} onClick={close}>Cancel</button>
        <button className={s.saveBtn} onClick={handleSave}>
          {saved ? "✓ Saved" : "Save"}
        </button>
      </div>
    </div>
  );
}

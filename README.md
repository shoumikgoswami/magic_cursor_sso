# Magic Cursor — Open Source

> Shake your mouse → instant AI overlay. Bring your own keys.

Magic Cursor is a lightweight desktop utility for Windows and macOS that detects a mouse shake gesture and instantly opens an AI assistant overlay — no shortcut to remember, no click, just a shake.

This is the **open-source BYOK edition**. Connect your own Ollama instance, OpenAI key, Groq key, or any OpenAI-compatible API. Full source code, no subscriptions.

---

## Features

- **Mouse shake trigger** — wiggle your cursor to summon the overlay (sensitivity adjustable)
- **Ask mode** — type a question, get a streaming response with the content under your cursor as context
- **Bubble mode** — compact floating response that stays out of the way
- **Chip mode** — minimal chip-style answer for quick lookups
- **Screen context** — automatically captures selected text, window title, and an optional screenshot
- **Session history** — browse past queries in a side panel
- **Insert / Copy** — paste the AI response directly into the focused window
- **Clean response extraction** — strip explanatory preamble before inserting
- **Windows + macOS** — native builds for both platforms

---

## Setup

### Option A — Download installer

1. Go to the [Releases](https://github.com/magic-cursor-oss/magic-cursor/releases) page
2. Download the installer for your platform:
   - **Windows** → `.msi` installer
   - **macOS** → `.dmg` disk image
3. Install and launch **Magic Cursor**
4. The tray icon appears — click it and open **Settings**
5. Choose your provider and paste your API key (or set Ollama URL)
6. Shake your mouse to start

### Option B — Build from source

**Prerequisites:**
- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 18+
- [Tauri CLI](https://tauri.app/start/create-project/#using-cargo-create-tauri-app): `cargo install tauri-cli`

```bash
git clone https://github.com/magic-cursor-oss/magic-cursor.git
cd magic-cursor
npm install
cargo tauri build
```

The compiled installer will be in `src-tauri/target/release/bundle/`.

For development (hot reload):
```bash
cargo tauri dev
```

---

## Providers

| Provider | What you need |
|---|---|
| **Ollama** | Run [Ollama](https://ollama.com) locally; no key needed |
| **OpenAI** | API key from [platform.openai.com](https://platform.openai.com/api-keys) |
| **Groq** | API key from [console.groq.com](https://console.groq.com) |

In Settings, pick a provider, enter your key (or Ollama URL), and click **Load** to fetch available models. Select a default model and optionally a vision model for screenshot queries.

### Ollama quick start

```bash
ollama pull llama3.2      # default model
ollama pull llava         # vision model (optional, for screenshots)
ollama serve              # starts on http://localhost:11434
```

---

## Configuration

Settings are stored at:
- **Windows**: `%APPDATA%\ai-cursor\config.json`
- **macOS**: `~/Library/Application Support/ai-cursor/config.json`

Key settings:

| Setting | Default | Description |
|---|---|---|
| `reversal_threshold` | 3 | Number of direction reversals to trigger |
| `window_ms` | 600 | Time window for shake detection (ms) |
| `min_displacement` | 30 | Minimum cursor travel per reversal (px) |
| `cooldown_ms` | 2000 | Minimum time between triggers (ms) |
| `capture_radius` | 350 | Screenshot capture area around cursor (px) |
| `system_prompt` | (see defaults) | System prompt sent with every query |
| `clean_responses` | false | Strip preamble before display/insert |

---

## Want zero-setup?

**Magic Cursor Managed** — download, paste a license key, done. Includes:
- ✓ Best available models via OpenRouter (no API key juggling)
- ✓ Voice input — speak your question, Whisper transcribes it
- ✓ Act mode — agentic tools: run shell commands, control the UI, read/write files
- ✓ One subscription, no API billing surprises

→ **[magiccursor.app](https://magiccursor.app)** — $9/month

---

## Architecture

```
src-tauri/src/
  main.rs           — Tauri commands, tray icon setup
  config.rs         — AppConfig serde, load/save JSON
  providers.rs      — Multi-provider LLM client (Ollama, OpenAI, Groq)
  context.rs        — Screen context capture (screenshot, clipboard, window title)
  mouse_hook.rs     — Global mouse hook via rdev
  shake_detector.rs — Direction-reversal shake algorithm
  window_manager.rs — Overlay show/hide/resize helpers
  entity.rs         — Smart entity detection in selected text
  history.rs        — Session history persistence

src/
  components/Overlay.tsx       — Main overlay UI (Ask / Bubble / Chip modes)
  components/Settings.tsx      — Settings window
  components/ResponseStream.tsx — Streaming token renderer
  store/overlayStore.ts        — Zustand state for overlay
```

---

## Contributing

Pull requests welcome. For significant changes please open an issue first to discuss the approach.

- Keep the core feature set minimal — this is the BYOK edition
- Rust code: `cargo fmt` + `cargo clippy` before submitting
- TypeScript: `npx tsc --noEmit` must pass

---

## License

MIT

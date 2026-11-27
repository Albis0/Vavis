# VAVIS

> Pronounced **VAY-vis**. A personal AI assistant for Windows.

VAVIS replaces the TypeScript and Electron version of AEGIS/RealJarvis. The current app uses Rust, Tauri, and a React and TypeScript interface.

## Getting started

Download the portable Windows executable and its checksum from [Releases](https://github.com/Albis0/Vavis/releases/latest). It requires WebView2. See [Security](SECURITY.md) for download verification.

On first launch, open **Settings → Model & keys** (`Ctrl+,`), and select a provider.

| Option | What you need |
| --- | --- |
| **Claude Code** | A Claude subscription with Claude Code access. [Install the CLI](https://claude.com/code) and sign in by running `claude` in a terminal. VAVIS can detect the installation. |
| **Gemini** | An API key from [Google AI Studio](https://aistudio.google.com). The same key can be used for live voice and semantic memory. |
| **Groq** | An API key from [Groq Console](https://console.groq.com). Groq also handles Whisper speech recognition. |
| **Cerebras, OpenRouter, GitHub Models, Mistral, NVIDIA** | A provider account and an API key. Check each provider's model access and usage limits. OpenRouter's default is `openrouter/free`. |
| **Local** | A local server such as Ollama or LM Studio. Enter its address if it uses a non-default port. |

Use **Fallback** to set a provider order for requests that fail before an answer or a tool action starts. Limits, availability, and model access depend on each provider.

## Features

- **Conversations and memory.** The clock icon opens past conversations. Search, open, rename, or delete them. VAVIS can extract lasting facts from what you say and retrieve relevant ones by meaning. Facts are saved locally and can be viewed or removed in **Settings → Memory**.
- **Voice.** Train your wake word by saying it three times in **Settings → Voice → Wake word**. Wake-word recognition runs on the device. In wake-word mode, speech is sent for transcription after a match. The conversation microphone starts Gemini Live, which supports interruptions and tool calls. Regular transcription uses Groq Whisper or Gemini.
- **Windows controls and integrations.** Use Windows UI Automation to address controls by name, scroll, or drag. Other tools cover files, apps, sound and brightness, the clipboard, PowerShell, Spotify, Steam, Obsidian, VirusTotal, web search, image generation, and MCP servers.
- **Automations.** Schedule tasks or trigger them by CPU or battery usage, a new file, an app starting or stopping, VAVIS starting, or returning after an absence.
- **Phone access.** Connect a Telegram bot in **Settings → Phone**. It replies in private to the paired account and asks for approval on the phone before destructive actions.
- **Code.** Open a workspace through `Ctrl+K` → **Code**. The assistant can search, read, make exact-text edits, and run tests. Changes pass through the approval system, and the open file refreshes after edits.
- **Council.** Ask several models the same question and compare their answers side by side.

## Security

- Destructive actions ask for approval. After three destructive actions in one turn, the app can ask again even if you chose **Always allow**.
- If outside content tries to give the model instructions, every state-changing action in that turn asks again, even with full authority enabled. This includes content read directly by Claude Code and filenames from watched folders.
- Destructive requests from the phone ask for approval, and the Telegram bot only replies in private chats.
- API keys are encrypted with Windows DPAPI and kept separately from the settings file.
- Claude Code's own shell and write tools are disabled. VAVIS tools use the approval system. During a code turn, Claude Code's file reads are confined to the open project. Project-level hooks and `CLAUDE.md` are excluded.
- Text sent to PowerShell passes through a shared quoting routine. The app launcher rejects command interpreters and script files.

See [SECURITY.md](SECURITY.md) for the security policy and how to report problems privately.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| `Ctrl+K` | Open the command palette |
| `Ctrl+,` | Open settings |
| `Ctrl+L` | Start a new conversation and keep the old one in the list |
| `Ctrl+B` | Show or hide the conversation panel |
| `Ctrl+M` | Cycle voice modes |
| `Ctrl+S` | Save the open file in Code |
| `Esc` | Stop speech or deny a pending approval |
| `F11` | Cycle window modes |

## Architecture

| Component | Responsibility |
| --- | --- |
| `ui/` | React and TypeScript interface built with Vite |
| `vavis-shell` | Tauri window, commands, conversations, memory, event monitoring, and Telegram |
| `vavis-tools` | Tools, approvals, agent loop, MCP client, and Claude Code bridge |
| `vavis-brain` | Providers, Claude Code CLI, embeddings, context budgets, and keys |
| `vavis-audio` | Microphone, voice activity detection, wake word, transcription, synthesis, and Gemini Live |
| `vavis-core` | Settings, SQLite storage, conversations, memory, automations, gallery, and search |

Dependencies point downward. Tool selection uses the request and the model's budget. Ordinary small talk can run without tools. The model can request additional tools with `request_tools`. Claude Code and code turns have their own selection paths.

The app starts Claude Code with `claude -p` and reads stream JSON. The tool bridge uses an authenticated loopback MCP endpoint that only accepts tool calls while a turn is active. A shared audio lock keeps queued playback and interruptions consistent. Context budgeting includes tool schemas and images.

## Build and test

Build the interface once before running the app or checking Rust on a fresh checkout.

```bash
cd ui && bun install --frozen-lockfile && bun run build && cd ..
cargo run --release --locked -p vavis-shell
```

```bash
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p vavis-tools --test selection_eval -- --nocapture

# Requires a local Claude Code installation and a signed-in account.
cargo test -p vavis-tools --test claude_code_agent_e2e -- --ignored --nocapture
```

Run `bun run check` and `bun run test` from `ui/`. The application targets Windows. The core, provider, tool, and audio crates also have platform-independent logic and tests. See the [manual checklist](MANUEL_TEST.md) and [provider checklist](PROVIDER_TEST.md).

### Data directory

The default is `%APPDATA%\vavis\data\`. Use the `VAVIS_DATA_DIR` environment variable with an absolute path to run a separate profile.

| File or directory | Contents |
| --- | --- |
| `vavis.toml` | Settings |
| `vavis.db` | Conversations, memory, automations, and the gallery index |
| `keys.dat` | Windows DPAPI-encrypted API keys |
| `wake.json` | Trained wake-word data |
| `media/` | Generated images and videos |
| `logs/` | Daily logs and crash log |

## Known limits

- Edge TTS may return HTTP 403. The app can fall back to Windows SAPI.
- Live voice has no acoustic echo cancellation. The app holds back quiet input during playback, and clear speech can interrupt it. Headphones reduce speaker feedback.
- A trained wake word is tuned to the voice that trained it.
- The brightness control targets built-in laptop displays.

*Apache-2.0*

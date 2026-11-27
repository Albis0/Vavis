# Changelog

VAVIS follows [Semantic Versioning](https://semver.org/). Use `bun scripts/version.mjs` to update the version in all three manifests. The changelog records user-visible changes. Git records the implementation details.

## 0.8.0 — 2026-11-27

### Added

- **Claude Code.** Use a Claude subscription without a separate API key. VAVIS exposes its tools through the same approval system as the other providers. A detected CLI can become the initial provider when the selected provider is unconfigured.
- **Provider options.** OpenRouter, Cerebras, GitHub Models, a custom endpoint, and a configurable local server address. A fallback chain can retry with the next provider when the first one fails before producing output or running tools.
- **Conversations and semantic memory.** Keep multiple searchable conversations and start a new one without removing the old one. Relevant saved facts can be retrieved by meaning.
- **Voice.** On-device wake-word training, Gemini transcription as an alternative to Groq, and interruptible Gemini Live conversations with tool support.
- **Windows UI Automation.** Address controls by name, scroll, and drag.
- **Event-driven automations.** Trigger on files arriving, apps starting or stopping, VAVIS launching, and returning after an absence.
- **Telegram.** Reach VAVIS through your own bot and approve destructive actions on the phone.
- **Code workspace.** The assistant can read, edit, and run tests in the open project, with approvals for changes.

### Fixed

- Conversations restore their history and keep it aligned with the model after switching or deleting. The active conversation and its replacement are handled together. Rename now has a dedicated action. The new-conversation button keeps the previous conversation.
- Voice and automation requests wait their turn without clearing a draft. A reply finishing on the phone frees the same queue. A failed history load leaves event listeners active, and closing during startup cleans up late subscriptions.
- OpenRouter uses its current free router in place of the retired default. Provider streams preserve split UTF-8 characters and report interrupted or empty answers. Anthropic model discovery uses the proper authentication and version headers. The status bar recognizes keyless providers.
- Tool selection offers core tools and `request_tools` for substantive requests, prioritizes relevant tools for larger models, and carries the previous task's tools into follow-up requests. Incorrect tool names return up to three suggestions. Missing arguments identify the accepted fields. VirusTotal tools use `virus_scan_file`, `virus_scan_hash`, and `virus_scan_url`, with aliases for older names. A request to turn off sound mutes it.
- The window stays responsive during approval and network checks. Hung commands have a timeout. Console windows no longer flash during common actions, and stopping speech targets the app's own process. Turn limits are 15 for regular tasks and 40 for code tasks.
- OpenAI GPT-5 and o-series requests use their required parameters. German, French, and Spanish language selections are respected. The live voice command updates its label when the session changes.
- The approval system checks content read directly by Claude Code and filenames supplied by folder automations. After suspicious outside instructions, all state-changing actions require approval, including scheduling an automation.
- Claude Code ignores project-level hooks and instructions files. Direct reads during a code turn are confined to the project. Workspace writes reject links that escape the workspace, and search skips loops.
- PowerShell text and its output preserve Turkish characters and curly quotes. App launching uses Windows directly and rejects command interpreters and script files. Windows MCP servers started through `.cmd` shims such as `npx`, `npm`, and `pnpm` are found correctly.
- The shortcuts list includes `Ctrl+K` and `Ctrl+B`, and describes `Ctrl+L` as starting a new conversation. Window controls with square brackets are found by name. Multiline typing avoids accidental message submission. Speech works with apostrophes in Windows user names. Telegram replies and approvals are restricted to private chats.

## 0.7.4 — 2026-09-07

Versions 0.7.0 through 0.7.3 were already used by AEGIS. VAVIS continued from 0.7.4 so GitHub could correctly mark the newest release.

### Added

- Model selection, API key entry, and connection testing appear on the same provider card. Settings controls have larger hit areas.
- Full authority was introduced as an opt-in setting with a persistent status indicator.
- Models can request additional tools when needed. Per-model budgets replaced a fixed 12-tool cap. An optional smaller routing model can suggest tools, with keyword routing as a fallback.
- Outside text is framed and checked for prompt injection. Suspicious content causes approvals to be requested again in the current turn. The original content remains available to the model.
- Up and down arrows navigate the last 20 messages, preserve an unfinished draft, and respect multiline caret movement. Recent messages are restored on restart.
- Added NVIDIA as a provider and frontend tests for Markdown escaping, message history, and reply streaming. A version script keeps the three manifests in sync.

### Fixed

- Clock and calculator tools are offered for relevant questions, while ordinary greetings remain conversational.
- Keyless web search can return general results, use calculated answers, and distinguish throttling from no results.
- The borderless window fills its work area, and the chat panel stays inside wide windows.
- Short, explicit rate-limit waits are honored up to twice. The outside-content framing is shown only to the model.

## 0.6.1 — 2026-08-30

- Rate limits are distinguished from context-length refusals, even when both use HTTP 413.
- Oversized conversations offer a recovery action. Tool definitions that exceed the budget are dropped before sending the request.
- Fixed the playback panel, window permissions, and interface language settings.

## 0.6.0 — 2026-08-30

- Added modals, notifications, and approval dialogs.
- Rebuilt the Code, Canvas, and Council views. The playback panel can be placed anywhere on the screen.
- Replaced the reactor with a 2D instrument face. Added one-click Spotify connection on Windows.

## 0.3.0

- Rebuilt the interface with Tauri and Svelte.
- Added Canvas, Council, and Code views and redesigned settings.
- Addressed a Windows Defender false positive affecting the release executable.

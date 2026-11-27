# VAVIS development notes

September 4, 2026 · Version 0.6.3

This document records the priorities and test results at the time. See the [current backlog](../../TODO.md), [changelog](../../CHANGELOG.md), and [security policy](../../SECURITY.md) for the current app.

## Project direction

The goal is a capable desktop assistant in the spirit of Jarvis. Every useful capability matters more than executable size or a fixed tool count. Windows was the first platform. Protection is the default, and authority settings should be visible and explicit.

## A. Tool architecture

The original `select_tools(registry, message)` had no model or provider information. It sent the same fixed list of up to 12 tools to very different models.

### A1. Per-model budgets — completed

- Added `tool_budget` to `ModelCaps` and passed it through selection.
- At the time, the table used 48 for Claude and GPT-5, 32 for Gemini and GPT-4o, 24 for Grok, 16 for Qwen, Kimi, and DeepSeek, 12–14 for Llama, and 8 for Gemma.
- Unknown models used a cautious default of 12, with a floor of 6. A test verifies that increasing the cap does not fill the list with irrelevant tools.
- The budget is a ceiling. Unused schemas consume tokens and make selection harder.

### A2. Optional two-stage routing — completed

1. A smaller routing model sees the request and a catalog of tool names and one-line descriptions.
2. The answering model receives the request and only the selected schemas.

`Router`, `KeywordRouter`, and `LlmRouter` provide the two implementations. Keyword routing is the default and the fallback for a failed, malformed, or disabled routing request. Configuration uses `llm.router_model`. Logging records the router model and catalog and selection counts. Small talk bypasses the paid routing request.

### A3. Tool discovery during a turn — completed

The model can request missing tools using the meta-tool originally named `arac_iste`. Newly requested tools are added to the current list. The initial limit was two requests per turn, followed by an instruction to continue with the available tools.

### A4. More capabilities — proposed

The original list included calendar events, email, browser operation, a coding assistant, documents and spreadsheets, OCR and audio analysis, Windows notifications, local network diagnostics, Git, and downloads. Each new tool should declare a domain and risk level and have a short catalog description.

## B. Security

### B1. Prompt injection defenses — completed

External text is framed consistently and scanned for suspicious English and Turkish phrases. A match forces new approvals during the turn even if a tool normally has a standing approval. The app explains why it asks. Source text remains available to the model to avoid discarding legitimate contents on a false positive.

### B2. Full authority — completed

An explicit setting under Tools was introduced with a confirmation when enabling it and a persistent status indicator. The original design disabled approval budgets and injection checks in this mode. Subsequent versions reinstated protection against suspicious outside instructions even when full authority is enabled. The setting defaults to off.

### B3. Secret scanning in CI — completed

Gitleaks checks the repository in CI. Full history is fetched so a key added and later deleted can still be detected.

## C. Quality and architecture

- **C1, frontend tests.** Added Vitest and jsdom for Markdown escaping, incomplete streamed input, message history and drafts, assembly of replies, and empty-bubble removal. Store instances are isolated for tests. The thin API wrapper remains covered through behavior at its call sites. The historical snapshot had 710 Rust and 38 frontend tests.
- **C2, operator abstraction.** Proposed a small screenshot and action interface. The planned order was a Windows operator, a browser operator, and eventually Android or a remote machine.
- **C3, folder-based skills.** Proposed loading a manifest, prompt template, and optional script from `%APPDATA%\vavis\skills\<name>\` at launch. Skills would appear in the tool catalog and pass through the existing approval system.
- **C4, memory.** Proposed local embeddings and a hybrid BM25 and vector score to match related wording. Later work could consolidate old facts during idle time.
- **C5, version automation.** Added the changelog and `scripts/version.mjs`. It synchronizes the Cargo workspace, UI package, and Tauri manifest. CI verifies they agree.

## D. Possible later platforms

Explore phone and messaging access, token-protected remote access, and Linux or macOS support. Non-Windows platforms would require replacements for DPAPI and Win32 operations.

## Priorities at the time

The next priority was useful new tools, especially calendar and notifications, browser control, and the code assistant. Folder-based skills would make future tools independent of the Rust build. Operator abstraction and hybrid memory followed. Additional channels and platforms were longer-term work.

The comparison report was revised to describe those priorities. It removed executable size and a fixed tool cap as success criteria while keeping the practical benefit of a portable executable. It also distinguished completed security, testing, and release automation work from the remaining architectural proposals.

## E. Findings from the September 4 Windows test

The app ran using a tool budget of 16. File operations and the outside-content approval check were verified. The test also found the following issues.

| Finding | Resolution |
| --- | --- |
| **E1. Clock and calculator tools were missing from ordinary questions.** The Core domain was absent and selection exited before the core tools could be offered. | Added Core selection, treated general time words as weak signals, detected arithmetic operators independently of wording, and added selection and small-talk regressions. |
| **E2. Outside-content framing appeared in chat.** | Stripped framing only from the displayed tool result. The model still receives the framed source. |
| **E3. Keyless web search returned no useful results.** DuckDuckGo's Instant Answer endpoint mostly recognizes encyclopedic entities. | Kept structured answers and added the Lite fallback for general queries. Distinguished throttling pages from empty results using captured fixtures and a live check. |
| **E4. Borderless mode and wide layouts overflowed.** Startup and the shortcut used different window logic. | Unified the window-mode command, sized the borderless window to the monitor, and removed the flex row's minimum-width overflow. |
| **E5. The rate-limit refusal ended the turn.** | Honor explicit waits up to twice and cap automatic waits at 60 seconds. Show a notice while waiting. Fixed delays between all tool calls were unnecessary. |
| **E6. Input history lost unfinished text.** | Added navigation through 20 recent messages, kept multiline caret behavior, restored drafts, and persisted history across restarts. |
| **E7. NVIDIA needed a provider entry.** | Added NVIDIA's OpenAI-compatible endpoint, default model, and the entry generated by `Provider::ALL`. |

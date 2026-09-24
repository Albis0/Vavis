# Contributing to VAVIS

How to set up the project, the conventions it follows, and how to submit changes.

## Development setup

Requirements: **Windows 10/11**, **Rust 1.85+** and **[Bun](https://bun.sh)**
(for the interface). The WebView2 runtime ships with Windows 11 and current
Windows 10.

```bash
git clone https://github.com/Albis0/Vavis.git
cd Vavis
cd ui && bun install && bun run build && cd ..
cargo run --release
```

The interface is built once into `ui/dist/` and embedded in the executable;
rebuild it after changing anything under `ui/`. There is no `.env` file: API
keys are entered in the app (**Settings → Model & keys**, `Ctrl+,`) and stored
encrypted with Windows DPAPI.

The core, brain, tools and audio crates also build and test on Linux, where the
Windows-only code is stubbed out. To check the Windows target from Linux:

```bash
rustup target add x86_64-pc-windows-gnu
cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings
```

## Project layout

Five crates and the interface, each a layer. **Dependencies point one way
only** — a lower layer never knows about the one above it.

| Part | Layer | Contents |
|---|---|---|
| `ui/` | Interface | React + TypeScript (Vite), rendered in the Tauri window |
| `vavis-shell` | Shell | Tauri window and commands, the chat turn, memory recall, event watcher, Telegram |
| `vavis-tools` | Hands | Built-in tools, permission gate, tool selection, agent loop, MCP client and Claude Code bridge |
| `vavis-brain` | Mind | Provider clients, the Claude Code CLI, embeddings, context budget, key storage |
| `vavis-audio` | Senses | Microphone, VAD, wake word, speech-to-text, text-to-speech, Gemini Live |
| `vavis-core` | Foundation | Config, SQLite store (conversations, memory, automations, gallery), search, scheduler, logging |

## Conventions

**Language.** Code, commit messages and issues are in English; many older
comments are in Turkish and stay as they are. The interface text is English;
the **Language** setting (Settings → General) sets the language the assistant
answers and speaks in.

**Comments explain *why*, not *what*.** The code already says what it does.
A comment earns its place by recording a decision, a constraint, or a trap
someone would otherwise fall into again.

**Tests are part of the change.** A bug fix without a test that fails before
it is not finished. Run everything before opening a PR:

```bash
cargo test --all                              # 1000+ tests
cargo clippy --all-targets -- -D warnings     # must be clean
cargo fmt --all -- --check
cd ui && bun run check && bun run test        # type check + interface tests
```

**Commits** follow [Conventional Commits](https://www.conventionalcommits.org/):
`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`. The body explains
the reasoning, not just the diff.

**Versions** live in three places (`Cargo.toml`, `ui/package.json`,
`crates/vavis-shell/tauri.conf.json`); `bun scripts/version.mjs --check` fails
if they disagree.

## Invariants a change must not break

These are load-bearing. Breaking one is a regression even if the tests pass.

**1. A request offers at most 12 tools** (`DEFAULT_TOOL_BUDGET`).

The predecessor project defined 353 tools and sent 64 on every request; no
model chooses reliably from 64 options. Selection narrows by domain first, and
a large domain is ranked by the tools the message names and cut at six.
`crates/vavis-tools/tests/selection_eval.rs` measures this — it must stay at
100% and the average must stay at or under 8. (Claude Code and code turns are
the exception: they get every tool the job needs.)

**2. Conversational messages get no tools at all.**

"hello", "write me a poem", "good night" must offer zero tools. Offering a
tool list to a chat message provokes needless tool calls.

**3. Barge-in must not start the next utterance.**

Stopping speech clears the queue *before* stopping playback, under one lock,
and no callback runs in between. See `crates/vavis-audio/src/queue.rs`.

**4. Everything counts against the context budget.**

System prompt, history, tool schemas *and* images. Images cost a flat 1,100
tokens — counting their base64 length would blow the budget on a single
screenshot. See `crates/vavis-brain/src/budget.rs`.

**5. Anything that changes something asks, and the guards outrank grants.**

`Destructive` tools ask every time unless granted; after three destructive
actions in one turn, "always allow" stops applying. Once a turn has read
outside content that tries to give orders, every non-`Safe` tool asks again —
even with full authority on. See `crates/vavis-tools/src/permission.rs`.

**6. Outside text never reaches a PowerShell script unquoted.**

Every value goes through `vavis_core::process::ps_quote`, which escapes all
five single-quote characters PowerShell accepts, not only `'`.

## Adding a tool

1. Implement `Tool` in a file under `crates/vavis-tools/src/builtin/`.
2. Pick the right `Domain` — this decides when the tool is offered.
3. Set `Risk` honestly. Anything irreversible is `Destructive`.
4. Register it in `builtin/mod.rs`.
5. Add a case to `selection_eval.rs` proving it gets offered for a realistic
   sentence, and check the eval still scores 100%.

Watch the domain size: when one domain grows past a handful of tools it
starts crowding core tools out of the budget. That is what forced the
System/Control split.

## Reporting bugs

Use the issue templates. If it is a **security vulnerability**, do not open a
public issue — see [SECURITY.md](SECURITY.md).

## Pull requests

- One logical change per PR.
- Tests pass, clippy is clean, the interface type-checks.
- The description says *why*, and names any invariant the change touches.

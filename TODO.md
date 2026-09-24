# Vavis — work queue

Tracks the Obsidian notes in `Obsidian Vault/Vavis/`. Kept up to date as
things land, so what is done and what is not is never a guess.

Status legend: `[x]` done and tested · `[~]` partly done · `[ ]` not started

## Integrations

- [x] **VirusTotal** — dosya ve adres itibarı, 3 tool. Dosya **hiçbir zaman
      yüklenmiyor**: SHA-256 yerel hesaplanıp yalnızca o soruluyor, bilinmeyen
      hash "bilinmiyor" dönüyor. Eşik oran üzerinden — sabit sayı google.com'u
      "ciddi" diye işaretliyordu. Dakika sınırı istemcide de sayılıyor, anahtar
      kaydedilirken gerçek bir sorguyla doğrulanıyor.
      `crates/vavis-tools/src/virustotal/`

- [x] **Web search chain** — Tavily → Brave → custom → DuckDuckGo, sequential
      failover, 15-min cooldown after a rate limit, order editable in settings.
      `crates/vavis-tools/src/websearch/`
- [x] **Obsidian** — 9 tools, filesystem-based, frontmatter/wikilink/tag
      parsing, trash-not-delete, atomic writes that keep line endings.
      `crates/vavis-tools/src/obsidian/`
- [x] **Steam** — library, achievements, store price, wishlist, friends,
      launch (asks first). Running-game detection is local, so it works on a
      private profile. `crates/vavis-tools/src/steam/`
- [x] **Spotify** — OAuth PKCE, 7 tools, now-playing panel with cached art.
      `crates/vavis-tools/src/spotify/`
- [x] **Custom MCP** — stdio + HTTP transports, per-server tool-selection
      domain, per-tool on/off, destructive by default.
      `crates/vavis-tools/src/mcp/`

## Interfaces

- [~] **Main interface** — an empty stage with the reactor at its centre and a
      resizable chat panel docked right. Everything the old layout kept on
      screen permanently — two telemetry rails, meters, a shortcut list, a view
      switcher — is now in the command palette on `Ctrl+K`, and what is left is
      one status line along the bottom.
  - [x] the reactor is an instrument face drawn on a 2D canvas: a turned
        housing, ten wound coils in a machined track, a graduated bezel and a
        layered core, with bloom taken from an emissive layer of its own. Its
        hue and speed say what the assistant is doing. `ui/src/lib/reactor.ts`
  - [x] the reactor is turned by hand — grab it anywhere and it follows the
        bearing of the pointer, wheel to zoom, double click to reset. A flick
        carries and settles. There is no limit on the rotation: it turns in
        its own plane, so it cannot foreshorten.
  - [x] the coils are wound copper seated in a recess, each turn a dark stroke
        for the gap with a lit one beside it for the crown of the wire, and
        the core's light caught along the inner face of every one.
  - [x] command palette — every action in one searchable list, which is what
        lets the stage stay empty. `ui/src/react/CommandPalette.tsx`
  - [x] chat panel resizable by its left edge, width persisted, `Ctrl+B` to
        hide. `ui/src/react/ChatPanel.tsx`
  - [x] light and dark themes, accent derived from one hue in `styles.css`.
        The reactor keeps a full palette for each and swaps it on the
        attribute change.
  - [x] approvals inline in the message flow. They are messages now, answered
        where they appear and left in the feed marked with what was decided.
        The modal is gone; it stole the keyboard mid-sentence every time.
  - [x] microphone in the composer, with the live level as a ring around it.
        The level is its own command polled at 10 Hz — `get_status` is far
        too heavy for that — and only while something is listening.
  - [x] tool calls collapsed to one line, expandable to show what the tool was
        called with and what it returned.
  - [x] conversation list — a drawer from the clock in the chat panel's
        header: search by title and content, open, rename, delete. Schema v5
        gives messages a conversation; Ctrl+L starts a new one and keeps the
        old. `crates/vavis-core/src/conversations.rs`,
        `ui/src/react/ConversationList.tsx`
- [x] **Code interface** — workspace backend done and tested (tree, read,
      write, search, path-escape refusal). View written.
      `crates/vavis-tools/src/workspace.rs`, `ui/src/react/CodeView.tsx`
  - [x] code work can go to its own provider and model, separate from chat.
        Optional and empty by default. A code provider with no key, a model
        the provider filter would not offer, or a name we no longer
        recognise all fall back to the chat model rather than failing the
        turn. The interface marks the handed-over turn, because only it
        knows which pane the message came from.
  - [x] the harness itself — ws_list/read/search/edit/write/run, a prompt
        that knows the project, 40 steps. Exact-text edits that must match
        once; commands time-limited with the process tree killed. With Claude
        Code, the CLI runs in the project with read-only Read/Glob/Grep;
        changes still go through the gate. Verified against the real CLI:
        failing test → fix → re-run → pass. `crates/vavis-tools/src/builtin/code.rs`
- [x] **Canvas interface** — image and video. Provider chain (OpenAI,
      Stability, Replicate, custom OpenAI-compatible endpoint) with the same
      failover shape as search. Every result keeps the seed the provider
      actually used, so it can be reproduced. Variation, animate and a real
      upscale endpoint. Files on disk, index in SQLite, usage and clear in
      settings. One chat tool (`gorsel_uret`) writes into the same gallery.
      `crates/vavis-tools/src/canvas/`, `crates/vavis-core/src/gallery.rs`,
      `ui/src/react/CanvasView.tsx`
- [x] **Council interface** — several models on one question, genuinely
      parallel. Independent seats run together; seats marked "reads the
      others" run in a second wave with the first wave's answers. A failing
      seat is one failing panel. Cost is forecast before the run and totalled
      after. Nothing spawns itself.
      `crates/vavis-shell/src/council.rs`, `ui/src/react/CouncilView.tsx`
- [x] **Settings layout** — categories left, content right, its own window
      rather than a rail panel. Search box, instant apply, masked keys, and a
      "test" on every provider and integration that makes a real request.
      Five languages. `ui/src/react/settings/Settings.tsx`

- [x] **Claude Code provider** — the user's Claude plan through the CLI;
      Vavis's tools over a token-guarded localhost MCP bridge.
      `crates/vavis-brain/src/claude_code.rs`, `crates/vavis-tools/src/mcp/bridge.rs`
- [x] **Free providers** — OpenRouter, Cerebras, GitHub Models, custom
      endpoint, configurable local URL; failover chain.
- [x] **Semantic memory** — facts injected per turn by relevance (embeddings
      + BM25), learned from conversation, embeddings backfilled.
      `crates/vavis-core/src/memory.rs`, `crates/vavis-shell/src/recall.rs`
- [x] **On-device wake word** — MFCC + DTW against the user's own
      recordings. `crates/vavis-audio/src/wake.rs`
- [x] **Event automations** — file appears, app starts/stops, startup,
      back after an absence. `crates/vavis-shell/src/watch.rs`
- [x] **Live conversation** — Gemini Live over one WebSocket, barge-in,
      tools. `crates/vavis-audio/src/live.rs`
- [x] **Phone** — Telegram bot, one-time pairing, approvals as buttons.
      `crates/vavis-shell/src/remote.rs`

## Heavy works

- [x] **Computer use** — the loop is closed and the actions are human-like:
  - [x] the cursor travels to its target on an eased path instead of
        teleporting, so hover states fire and slow software sees it arrive
  - [x] typing is paced in chunks rather than dumped in one burst, which
        several applications drop half of
  - [x] `wait_for_screen` is the check step: it waits for the screen to settle
        and answers in one sentence, so the model does not send a full
        screenshot after every click. A blinking caret does not count as
        movement — the signature is a coarse 32×18 brightness map.
  - [x] drag and scroll, plus controls by name through UI Automation
        (list_ui_elements, click_element, set_element_text). Large domains now
        rank by the tools the message names and cap at six; the eval stays at
        100%. `crates/vavis-tools/src/builtin/uia.rs`

## Known constraints

- Tool selection is capped per model; an eval test holds the average offered
  at or below 8.0. It is currently **7.7**, with large domains capped at six
  (`DOMAIN_CAP`). Check with
  `cargo test -p vavis-tools --test selection_eval -- --nocapture` first.
- Domain keywords are matched as substrings, so short ones are dangerous:
  `"md"` once matched "durumda" and "hakkımda" and pulled 9 unrelated tools
  into ordinary requests. The canvas keywords are all four characters or
  more for the same reason, with a test that says so.
- Generated media is served to the webview through Tauri's asset protocol,
  whose scope is opened at startup to the media directory alone
  (`allow_media_in_webview` in `main.rs`). Widening it would let the webview
  read anything on disk.
- Model prices in `vavis-brain/src/budget.rs` go stale. Everything derived
  from them is labelled an estimate, and an unknown model reports nothing
  rather than a confident wrong number.
- The `custom-protocol` feature on the tauri dependency is what embeds the
  built frontend. `tauri::is_dev()` is `!cfg!(feature = "custom-protocol")`
  and never looks at `debug_assertions`, so without it a `--release` build
  still points the webview at the dev server and the window opens to
  ERR_CONNECTION_REFUSED. `tauri build` sets it; this project ships with
  `cargo build --release`, which does not, so it is on by default in
  `crates/vavis-shell/Cargo.toml`. `release_builds_embed_the_frontend` in
  `main.rs` fails if it is removed, and the release workflow runs that test
  in release mode because `cargo test --all` runs in debug and cannot see it.
- Tools are synchronous but the agent loop that calls them is async, so any
  network tool that builds its own runtime and `block_on`s it panics with
  "Cannot start a runtime from within a runtime" — killing the request thread,
  so no reply ever arrives. Every tool goes through `vavis_tools::run_async`
  instead (`crates/vavis-tools/src/blocking.rs`), which reuses the ambient
  runtime when there is one. `tools_run_from_inside_a_runtime` in `agent.rs`
  drives a real tool through the agent from inside a runtime, and
  `the_old_pattern_panics_inside_a_runtime` pins down why the helper exists.
- `max_output` was 1024 for every model, so any long answer stopped
  mid-sentence and a code block stopped mid-line, with nothing to say it had
  been truncated — providers just stop at the number they are given. The reply
  limit is per family now, and held to a quarter of the window by
  `ModelCaps::new` so it cannot starve the input instead.
  `every_model_can_write_a_long_answer` guards it.
- A Spotify client id that is really the redirect URI produces a blank
  `client_id: Not present` page — Spotify does not say the id was malformed,
  so it is unreportable from the response. The settings screen prints the
  redirect URI directly above the input, which is what makes the paste easy,
  so `spotify::auth::check_client_id` rejects it before the browser opens.
- The reactor is drawn in 2D, and was a WebGL object before that. The three
  things that forced the change were all lighting problems, and a face with no
  lighting has none of them:
  - A disc foreshortens by the cosine of the angle it is turned through, so
    every drag that made the object feel handled also flattened it. At the 66
    degrees first allowed it collapsed to a third of its width; clamped to the
    half radian that looked right, the perspective was never really seen.
  - Bloom is screen-space and cannot tell metal from plasma, so the threshold
    had to be tuned against the housing's own luminance, and the answer was
    different in each theme. Below it the whole assembly came back as one
    milky wash.
  - Metal is entirely reflection. Keeping a metal housing dark on a white page
    meant de-metalling it to 0.15 metalness, which is the definition of
    plastic and is exactly what it looked like.
- Bloom is now an explicit blur of a separate emissive canvas, so it is
  thresholded by construction: the housing is not in that buffer and cannot
  bloom whatever the palette does. It runs in both themes for that reason,
  gently on a light page rather than switched off.
- The two palettes are not inversions of each other. A dark instrument on a
  dark page and a pale one on a white page are both real objects; a dark
  instrument on a white page is a hole. What does not change is the bore,
  which is dark in both, because it is the only thing guaranteeing the core
  reads as hot rather than as a coloured circle. The emissive layer is
  composited additively on the dark face and normally on the pale one, for the
  same reason the wave blending is switched rather than merely dimmed.
- The reactor canvas takes pointer events, but the stage around it does not:
  the canvas is full-bleed, so `pointer-events: none` on `.stage` with `auto`
  on `.host` is what keeps empty space from swallowing clicks meant for the
  page. The wheel listener is registered non-passive on purpose — Chrome
  defaults wheel listeners to passive, which silently voids `preventDefault`
  and lets the page scroll behind the zoom.
- The reactor's radius is derived every frame from the host size and the zoom
  together, never written from both. The 3D version kept a base distance owned
  by `resize` and a zoom owned by the wheel, and writing the camera from each
  meant whichever fired last discarded the other: resizing the window threw
  away the zoom.
- The reactor's core is built only from additive layers; there is no opaque
  disc at its centre. A solid bright circle is the obvious way and hides the
  very glow layers meant to give it depth, so it renders as a flat pale coin
  no matter how bright it is driven.
- The theme is applied to `<html>` in `main.ts`, before the app mounts, not
  from an effect inside it. Components read `data-theme` as they initialise —
  the reactor builds a whole environment map from it — and a child's `onMount`
  runs before the parent's `$effect`.
- The release profile deliberately keeps the symbol table and the unwind
  tables. `strip = "symbols"` plus `panic = "abort"` produced a binary with
  neither, which is the shape of a packed executable: Defender's ML model
  flagged 0.4.0 as `Trojan:Win32/Sabsik.FL.A!ml` — a heuristic guess, not a
  signature match. Restoring them costs about 6 MB and clears the scan, with
  and without Mark of the Web. The binary is unsigned, so SmartScreen may
  still warn on first run until the download builds reputation; a code
  signing certificate is the only real fix and has not been bought.

## Checks

```
cargo test --all                # 1000+ passing
cargo clippy --all-targets -- -D warnings
cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings
cd ui && bun run check && bun run test && bun run build
```

Environment-dependent tests are `#[ignore]`d and run explicitly:

```
VAVIS_TEST_VAULT="C:/path/to/vault" cargo test -p vavis-tools real_vault -- --ignored
cargo test -p vavis-tools local_steam -- --ignored --nocapture
cargo test -p vavis-tools --test mcp_e2e -- --ignored   # needs node
cargo test -p vavis-tools live_screen -- --ignored --nocapture  # needs a desktop
```

## Next

- [ ] Echo cancellation for the live conversation. Today quiet input is held
      back while the assistant speaks; real AEC (WebRTC's) would allow
      interrupting it at normal volume on speakers.
- [ ] A second live backend (OpenAI Realtime) for users with that key.
- [ ] Code turns with Claude Code could stream the CLI's own tool use into
      the feed (today only Vavis's ws_* calls appear there).
- [ ] Wake word: enrolment that adapts over time from confirmed detections.

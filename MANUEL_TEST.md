# VAVIS manual test checklist

The automated suite covers logic and simulated services. Use this checklist on Windows for the cases that need a real window, microphone, speaker, or provider account. Build the app using the [README](README.md) and use a separate profile and test accounts where possible. The whole checklist takes about 90 minutes.

## 0. Launch and window controls

- [ ] The window opens with the reactor in the middle and chat on the right.
- [ ] `Ctrl+K` opens the command palette, typing filters the actions, and `Esc` dismisses it.
- [ ] `Ctrl+B` hides and restores chat. Drag its left edge and check that the width is remembered after restart.
- [ ] `F11` cycles windowed, borderless, and fullscreen modes.
- [ ] The latest conversation reappears after restart. **Settings → Updates** shows the correct version.

**Notes** _______________________________________________

## 1. Providers and chat

- [ ] Open **Settings → Model & keys**, select a provider, add its key, and use **Test**. Repeat with an invalid test key and check the error.
- [ ] If Claude Code is installed and signed in, select it without an API key.
- [ ] A short reply streams into the chat as the provider sends it.
- [ ] Ask a question in Turkish, then change **Settings → General → Language** and check the language of the next reply.
- [ ] Fetch the model list, select a different model, and verify that it handles the next reply.
- [ ] Configure a fallback, make the primary test provider fail before replying, and confirm that the app tries the next provider and explains the switch.
- [ ] Markdown code blocks, lists, and tables render correctly.

**Notes** _______________________________________________

## 2. Conversations and memory

- [ ] `Ctrl+L` starts a new conversation and keeps the old one. Search, open, rename, and delete a conversation from the clock menu.
- [ ] State a fictional name and city. Check for a **Remembered** notice. Start another conversation and ask where you live.
- [ ] With a Gemini key, say you like espresso and ask about your coffee preference in another conversation. Check that the related fact is found by meaning.
- [ ] List and remove facts in **Settings → Memory**.
- [ ] Summarize a test page containing a claim about the user's name. Its claim should stay out of memory.

**Notes** _______________________________________________

## 3. Voice

- [ ] The microphone indicator responds to speech. Groq transcribes it, or Gemini when Groq is unconfigured.
- [ ] The app speaks a reply. `Esc` stops it immediately and no queued sentence starts afterward.
- [ ] `Ctrl+M` cycles voice modes, and the status line shows the current mode.
- [ ] Train a wake word with three samples. In wake-word mode, unrelated speech produces no transcription request. The trained speaker can wake it. A different voice may not match.
- [ ] With a Gemini key, start a live conversation from the command palette. Check continuous speech, interruptions, and a read-only tool question such as asking for the time.
- [ ] End live voice and check that the conversation was saved.
- [ ] If Edge TTS returns HTTP 403, check that VAVIS reports and uses the SAPI fallback.

**Notes** _______________________________________________

## 4. Approvals and untrusted content

- [ ] In a disposable directory, ask to write a test file. **Deny** leaves the directory unchanged. **Allow** creates the file.
- [ ] **Always allow** remembers an approval. More than three destructive actions in one turn trigger another prompt.
- [ ] With full authority enabled, routine approvals are skipped. After a test page or watched filename containing untrusted instructions, state-changing actions ask again.
- [ ] The ordinary launcher rejects PowerShell and other command interpreters. Switching to a shell action still uses its approval rules.
- [ ] Copy `it’s a test` to the clipboard and check that the curly apostrophe survives.
- [ ] Type multiline text into Notepad and confirm the lines stay separate. In messaging apps, line breaks should not submit the draft.

**Notes** _______________________________________________

## 5. Windows tools

- [ ] List open windows and check that the names match.
- [ ] Open Notepad, list its controls by name, open the File menu, and set text in an input using UI Automation.
- [ ] Find a control with square brackets in its name, scroll a page, and drag an item in a test window.
- [ ] Describe a screenshot of a test window. Verify volume, laptop brightness where supported, media keys, and clipboard reads.
- [ ] Open a browser, a test PDF, and `ms-settings:`. An unknown application should produce a clear error.

**Notes** _______________________________________________

## 6. Automations

- [ ] Create a test reminder, and confirm it fires at the scheduled time.
- [ ] Watch the Downloads folder with a benign test file. The completed file triggers once, and an incomplete `.crdownload` does not trigger.
- [ ] Test app-start, app-stop, and VAVIS startup events.
- [ ] List, disable, and delete an automation. A task that fires during a reply waits until the reply ends.

**Notes** _______________________________________________

## 7. Phone access

- [ ] Pair a test Telegram bot and send a message from the paired account.
- [ ] Verify that an unpaired account and a group chat receive no reply.
- [ ] Ask for a test state change and check for approval buttons on the phone, even with full authority enabled.
- [ ] Send a test notification to the phone. Use `/yeni` to start a new conversation.

**Notes** _______________________________________________

## 8. Code workspace

- [ ] Open a fixture project from **Code**. Check the tree, read an existing file, make a small edit, and save with `Ctrl+S`.
- [ ] Ask the assistant to run a failing test and fix it. Check the approvals, the changed file, and the rerun result.
- [ ] With Claude Code, confirm workspace reads deny paths outside the project. Verify that project-local `.claude/settings.json` hooks do not run.

**Notes** _______________________________________________

## 9. Integrations

- [ ] **Web search.** Ask for an answer with sources. Change the provider order and check that search follows it.
- [ ] **Canvas.** Generate a test image and check the gallery. Test video with a configured video provider and a budget that permits it.
- [ ] **Council.** Send a question to two configured models and compare their answers.
- [ ] **Spotify.** Connect a test account, play a track, queue another, and check the now-playing panel.
- [ ] **Steam.** List a library and detect a running game. Starting a game should ask for approval.
- [ ] **Obsidian.** Search, read, and create a note in a test vault. A removed note moves to `.trash`.
- [ ] **VirusTotal.** Check a harmless fixture file. Only its hash should be queried.
- [ ] **MCP.** Add a trusted test server through `npx`. Check that it starts, lists tools, and runs a benign test tool.

**Notes** _______________________________________________

## 10. Shutdown and data

- [ ] The configured profile contains `vavis.toml`, `vavis.db`, `keys.dat`, `media/`, and `logs/` after those features have been used.
- [ ] `keys.dat` is unreadable as plain text, and daily logs omit keys.
- [ ] Close the app normally. Check that no new `logs/crash.log` was written.

**Notes** _______________________________________________

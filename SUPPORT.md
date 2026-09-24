# Support

## Getting started

The [README](README.md) covers installation, the first provider and the
shortcuts. Most first-run problems are one of these:

**The assistant does not answer** — no provider is set up yet. Open
**Settings → Model & keys** (`Ctrl+,`) and pick one. Free options: Claude Code
on a Claude subscription (no key), or a free key from Gemini
([aistudio.google.com](https://aistudio.google.com)) or Groq
([console.groq.com](https://console.groq.com)). The **test** button next to
each provider makes a real request to check it.

**Windows SmartScreen warning** — expected and permanent. The binary is
deliberately unsigned; [SECURITY.md](SECURITY.md) explains how to verify a
download instead.

**Voice does nothing** — speech recognition needs a Groq or Gemini key, even
if you chat through another provider. Press `Ctrl+M` to cycle voice modes. For
the wake word, train it once in **Settings → Voice → Wake word**.

**The assistant answers in the wrong language** — **Settings → General →
Language** (English, Türkçe, Deutsch, Français, Español).

## Something is broken

1. Note the version (**Settings → Updates**) and the provider and model
   (**Settings → Model & keys**).
2. Check `%APPDATA%\vavis\data\logs\` — the daily `vavis.log` usually names
   the cause. A `crash.log` file only exists if the app crashed.
3. Open a [bug report](https://github.com/Albis0/Vavis/issues/new/choose).

Include the log output. Keys never appear in logs, but glance before pasting.

## Asking a question

Use [Discussions](https://github.com/Albis0/Vavis/discussions) rather than an
issue. Issues are for defects and concrete feature requests.

## Reporting a security problem

By email, not publicly — see [SECURITY.md](SECURITY.md).

## Response times

This is a one-person hobby project. Expect a reply within a few days;
occasional delays happen. Security reports get looked at first.

## What is out of scope

- **Platforms other than Windows.** The app links Win32 APIs directly
  (DPAPI, UI Automation, screen capture, media keys) with no cross-platform
  stand-in.
- **Provider account problems.** Billing, rate limits and key issues belong
  to Groq, Anthropic, OpenAI and the rest.
- **"Make it do X" without detail.** Describe the problem you are hitting;
  that usually leads somewhere better than a proposed solution.

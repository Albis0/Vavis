# Security Policy

## Reporting a vulnerability

Found a security issue? Thank you for taking the time to report it.

One firm request: report security problems by **email, not a public issue** —
a public issue hands the exploit to everyone before a fix exists. Write to
**abdurrahman.aksakal09@gmail.com**. Any format is fine, even two sentences.
Expect a reply within a few days; this is a one-person project, so occasional
delays happen.

Reproduction steps, the affected version, or impact details all speed up the
fix — include what you can, none of it is required. "Something feels off in X"
is already a useful report.

If you opened a public issue by accident, don't worry about it; the details
will be moved out of view and followed up privately.

## Supported versions

Only the latest release receives fixes. This is a young project moving fast;
backporting to older tags is not practical yet.

| Version | Supported |
|---|---|
| Latest release | ✅ |
| Anything older | ❌ |
| AEGIS 0.7.x (TypeScript) | ❌ superseded — see `feat/claude-code-parity` |

## Unsigned releases — how to verify a download

VAVIS is a free, hobby open-source project. Its executable is **deliberately
not code-signed**: a signing certificate is a recurring cost that isn't
justified here, so the Windows SmartScreen warning is expected and
**permanent**. Instead of trusting a signature, verify the release directly:

1. **Check the hash.** Every release asset lists a SHA-256 digest on the
   release page. Compare it with `Get-FileHash .\vavis-<version>.exe`.
2. **Scan before running.** Upload the executable to
   [VirusTotal](https://www.virustotal.com) and confirm the hash matches.
3. **Build from source.** Build the interface (`cd ui && bun install && bun
   run build`), then `cargo build --release`. No bundled installers; the only
   network access at build time is fetching crates and npm packages.

The warning is louder than for a typical app because VAVIS legitimately uses
screen capture and simulated mouse/keyboard input — the same APIs malware
uses. The difference is that every line doing it is public and readable.

## Threat model

The realistic threat for an LLM agent with 70+ tools is **prompt injection
through external content**, not memory safety. Rust removes the memory-safety
class outright; the interesting risks are elsewhere.

### Mitigations in this repository

**Approval gate.** Every tool declares a risk level. Anything irreversible —
writing files, closing applications, running shell commands, clicking, typing,
deleting memories — is `Destructive` and requires an explicit click.
See `crates/vavis-tools/src/permission.rs`.

**Destructive budget.** After three destructive actions in a single turn, a
standing "always allow" grant stops applying and approval is requested again.
A loop-guard catches repetition; the budget catches *variety* — deleting eight
different files one after another.

**Injection guard.** Every tool result — files, MCP servers, and the pages
Claude Code reads with its own tools included — is scanned for text that tries
to give orders; web pages, project files and window contents are also framed
as data before the model sees them. Once a turn has seen such text, every tool that changes
something asks again — **even with full authority on**. File names that land
in a watched folder are treated the same way. Background memory takes facts
only from the user's own words, never from quoted pages.

**Narrow tool exposure.** The model is offered at most 12 tools per request,
selected by domain, averaging at or under 8. Fewer options means fewer opportunities
for an injected instruction to reach a dangerous tool.

**Command injection guards.** Every value that reaches a PowerShell script
goes through one function that escapes all five single-quote characters
PowerShell accepts (the typographic `‘ ’ ‚ ‛` close a string too). Apps are
launched with `ShellExecuteW`, not a command line, and `launch_app` refuses
shells, script hosts and script files — running code is `run_command`'s job,
which always asks. Closing apps refuses protected processes (`vavis`,
`system`, `csrss`, `winlogon`, `services`, `svchost`).

**Claude Code.** When Claude Code is the provider, its own write and shell
tools are switched off; everything it does on the computer goes through
Vavis's tools and this gate, over a token-protected MCP server bound to
127.0.0.1 that exists only during the turn. In a code turn it can read only
the open project, and the project's own `.claude` settings (hooks) and
CLAUDE.md are not loaded.

**Remote access.** The Telegram bot answers only the paired account, only in a
private chat, and every destructive action is asked on the phone with buttons
— full authority never applies to a remote request.

**Bounded inputs.** File reads cap at 256 KB, screen coordinates are validated
against the actual screen, typed text is length-limited, and web page content
is truncated before it reaches the model.

### Known limits

- A user who approves a `run_command` call approves arbitrary code execution.
  The gate makes it visible; it does not make it safe.
- The injection scan matches known phrasings. A page that gives orders in
  words it does not recognise is still framed as data, but does not trigger
  the extra approvals.
- Screen capture sends an image of the whole screen to the configured LLM
  provider. Close what you don't want transmitted.

## Secret management

**No secret is ever bundled into the binary or committed to the repository.**

- **API keys** (Groq, Anthropic, OpenAI, …) are entered at runtime and stored
  in `%APPDATA%\vavis\data\keys.dat`, encrypted with **Windows DPAPI** under
  the current user account. Copying the file to another machine or user makes
  it undecryptable.
- Keys are **never printed**: the settings screen shows only whether a key
  is set, and a test asserts the plaintext key does not appear in the stored
  file.
- Keys leave the device only in requests to the provider you configured.

## Data handling

Everything stays local:

| Data | Location |
|---|---|
| Conversation history | `%APPDATA%\vavis\data\vavis.db` (SQLite) |
| Remembered facts | same database |
| Automations, generated-media index | same database |
| Generated images and video | `%APPDATA%\vavis\data\media\` |
| Trained wake word | `%APPDATA%\vavis\data\wake.json` |
| API keys | `keys.dat`, DPAPI-encrypted |
| Logs | `%APPDATA%\vavis\data\logs\` |

There is no telemetry, no analytics, no crash reporting to any server, and no
account. Outbound traffic goes only to services you configured or asked for:
the LLM provider (and its speech and embedding endpoints), the sites
`web_search` / `fetch_page` reach, integrations you connected (Spotify, Steam,
VirusTotal — which receives file hashes, never files — Telegram, MCP
servers), and GitHub's releases API, asked once at startup and whenever you check for
updates. The wake word
is recognised on the device; nothing is sent until it is heard.

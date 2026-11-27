# Groq built-in tools investigation

September 16, 2026

The investigation asked when Groq's built-in tools can be used alongside VAVIS tools. In the live API, supplying even one function schema to `groq/compound` caused HTTP 400. Omitting the field returned HTTP 200. The fix omits custom schemas for Compound and preserves them for compatible models.

## The two tool systems

| Model family | Tool behavior observed |
| --- | --- |
| `groq/compound`, `groq/compound-mini` | Groq runs its own tools on the server within one API request. Custom functions and MCP tools are excluded. |
| `openai/gpt-oss-120b`, `openai/gpt-oss-20b` | Provider tool types can appear in the `tools` array alongside custom function schemas. |

Compound exposed `web_search`, `visit_website`, `code_interpreter`, and `wolfram_alpha` through `compound_custom.tools.enabled_tools`. These were enabled by default at the time of the test. The legacy `browser_automation` type was accepted without effect, so VAVIS did not add it.

## Live observations

| Request | Status and result |
| --- | --- |
| Compound or Compound Mini with one custom schema | 400, tool calling unsupported |
| GPT-OSS 120B with one custom schema | 200 |
| Qwen 3.8 27B with one custom schema | 200, returned the requested time-tool call |
| GPT-OSS with `browser_search` and one custom schema | 200 |
| GPT-OSS with `code_interpreter` and one custom schema | 200 |
| Qwen with `browser_search` | 400, only function and MCP types accepted |
| Compound with the field omitted or with an empty array | 200 |

The results make the model family the right place for the rule. Appending built-ins to all Groq requests would have broken Qwen.

## Implementation

- `builtin.rs` exposes `tool_support`, `extra_tools`, and `covers_web_search`. Compound matching uses a family prefix so newly named variants are not sent incompatible schemas.
- The client applies filtering when building the body, before budget fitting. With Compound it omits the field entirely. It appends supported built-in tool types centrally.
- When GPT-OSS's built-in search is active, VAVIS suppresses its overlapping `web_search` tool in both initial selection and later `request_tools` calls.
- Anthropic uses a separate payload and was inspected without requiring a change for this issue.

## Verification

The original change passed workspace tests and Clippy. Six unit tests verified the policy. Four socket tests verified the actual payload, including omission of `tools` for both Compound variants, custom schemas for Qwen, and built-in search alongside custom schemas for GPT-OSS. A live retest confirmed Compound could answer a time question using its own tools. GPT-OSS and Qwen continued to return custom time-tool calls.

## Open items at the time of the investigation

- VAVIS did not add the provider-side code interpreter because it already has workspace and shell tools.
- Compound supplies its own chat and web capabilities. Local VAVIS tools such as files, memory, Spotify, and Obsidian are unavailable in that mode. A visible setting hint was proposed.
- The local price table did not include Groq's catalog prices.

## References

- [Groq built-in tools](https://console.groq.com/docs/tool-use/built-in-tools)
- [Compound built-in tools](https://console.groq.com/docs/compound/built-in-tools)
- [Compound systems](https://console.groq.com/docs/compound/systems/compound)
- [Retired browser automation](https://console.groq.com/docs/tool-use/built-in-tools/browser-automation)

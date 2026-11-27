# Provider test checklist

VAVIS exposes 14 chat provider options. Check both the advertised capabilities and what the service sends on a real request. Availability, quotas, and model IDs change independently of the app.

## Verification snapshot

The November 27, 2026 audit used one or more short synthetic prompts per available connection. It did not test every listed model or every account tier. The local socket suite exercised all 13 HTTP paths. Claude Code uses a separate CLI path.

| Provider | Live result | Local coverage |
| --- | --- | --- |
| Groq | Model list, default reply, and tool round trip passed | OpenAI-compatible stream and Groq built-in tools |
| Gemini | Model list, default reply, and tool round trip passed | Native payload, stream, and tool state |
| OpenRouter | Model list, `openrouter/free` reply, and tool round trip passed. The previous default returned HTTP 404. | Stream errors and retired saved default |
| Claude Code | Signed-in Windows CLI produced a reply and called a test tool through the bridge | CLI stream events and MCP bridge |
| Local | Model list and reply with a local llama.cpp/Qwen server | OpenAI-compatible stream and desktop flow |
| Custom | Model list and reply from the same real local runtime configured as a custom endpoint | Configured endpoint, stream, and failover |
| Anthropic API | Test account unavailable | Native stream, tool call, and model-list headers |
| Cerebras, DeepSeek, GitHub Models, Mistral, NVIDIA, OpenAI, xAI | Test accounts unavailable | HTTP stream path using local responses |

## How to test a provider

Use a short-lived probe with a test account and synthetic prompts. Keep keys and probe output with account details out of source control. Start with one short request per model in the scope of the test. Avoid quota or load tests on shared accounts.

### 1. Model discovery

- [ ] Check the live catalog against `budget.rs`, including context windows, output limits, published tool capabilities, and pricing when available.
- [ ] Check that speech, embedding, image, and classification models are excluded from the chat picker.
- [ ] Verify the default answers and send a request to each model in the intended test set. A retired model can remain in the catalog.

### 2. Context and output budgets

- [ ] Compare `ModelCaps::for_model` with provider metadata. Check broad family-name matches for smaller models.
- [ ] Verify that a request fits the combined context and output budget. Verify that the output limit is accepted.
- [ ] Check the per-minute input token limit separately from the context window. Waiting cannot help when a single request exceeds an entire minute's allowance.

### 3. Tools

- [ ] Send a harmless function schema, receive a tool call, and return a synthetic result. Verify that the provider binds the result to the call.
- [ ] For providers with built-in tools, verify whether built-in and custom tools can coexist. Check that other model families do not receive unsupported built-in types.
- [ ] Inspect the actual request at a local socket. A routing unit test alone cannot show whether the request builder applies its decision.

### 4. Error shapes

- [ ] Record the status and sanitized response for an invalid test key, nonexistent model, oversized request, invalid output limit, and known rate-limit refusal.
- [ ] Verify error classification and recovery. Context length, per-minute quota, and a retired model require different responses.
- [ ] Confirm that error output and logging do not reflect keys.

### 5. Rate limits

- [ ] Check `retry-after`, `x-ratelimit-*`, and whether reset values are durations or timestamps. Check the scope of the quota and time window.
- [ ] Check that short delays the provider explicitly supplies can be honored. Large requests that cannot fit within the quota should offer guidance to shorten the input.

### 6. Input throughput

- [ ] Compare advertised input tokens per minute with `input_budget()`.
- [ ] Confirm that the app distinguishes rate-limit exhaustion from a permanent input-size problem.
- The September Groq probe saw a Qwen context window of 131,042 tokens and a free-tier input limit of 7,000 tokens per minute. A request with 60,012 input tokens fit the context and still could not pass the throughput limit.

### 7. Request body

- [ ] Check `max_tokens` versus `max_completion_tokens`, supported fields, the temperature range, system message placement, and repeated roles.
- [ ] Check empty content and the complete tool-call and tool-result exchange.
- The historical Groq probe accepted `max_tokens`, ignored unknown fields, accepted repeated roles and system messages in different positions, and rejected temperature 2.5.

### 8. Streaming

- [ ] Check completion markers, UTF-8 text split across network packets, malformed and empty frames, and tool-call arguments split across frames.
- [ ] Simulate an HTTP 200 followed by a stream error. Check that incomplete tool calls are discarded and a partial answer is not treated as successful.
- [ ] Test a primary failure before output, then test one after output. The first can use a configured fallback. The second should surface the failed stream.

### 9. Credential storage

- [ ] Check that the client sends the key only in the expected authentication header and redacts it from error output.
- [ ] Check that the Windows credential file is DPAPI-encrypted and that secrets are absent from tracked files and Git history.
- [ ] Use only scoped test credentials where the provider supports them. Revoke a test key if it was exposed.

## Groq findings from September 2026

| Finding | Resolution |
| --- | --- |
| Compound rejected custom tool schemas with HTTP 400 | VAVIS skips custom schemas for that family |
| GPT-OSS supported a built-in web search that was not used | The compatible models receive `browser_search` |
| Prompt Guard had a 512-token context window and matched a 32,000-token family rule | Corrected the budget table and filtered classifier models |
| GPT-OSS and Compound had larger context windows than the table stated | Updated the measured entries |
| TTS and classifier models appeared in the chat picker | Filtered non-chat models while retaining `gpt-oss-safeguard-20b` |
| Context-length errors did not match the recovery rule | Added the observed error shape |
| The app advised waiting when the request exceeded the per-minute limit | Advises shortening the request |

The probe recorded a limit at the organization level, duration strings such as `2m59.56s`, and HTTP 413 for some throughput-limit errors. A catalog-driven price and budget refresh and a visible note about Compound's custom-tool restriction were left open. See the [Groq built-in tools investigation](example/docs/rapor-groq-yerlesik-araclar.md) for historical request details.

## Gemini native API

The native path uses `generateContent` and `streamGenerateContent`. It exposes system instructions, multiple content parts, generation settings, and function calls directly.

| Field | OpenAI-compatible shape | Gemini shape |
| --- | --- | --- |
| Message list | `messages` | `contents` |
| Model role | `assistant` | `model` |
| System prompt | A message | `systemInstruction` |
| Text | `content` | `parts[].text` |
| Tools | `tools[].function` | `tools[].functionDeclarations[]` |
| Tool call | `tool_calls[]` | `parts[].functionCall` |
| Tool result | `role: "tool"` | `role: "user"` with `functionResponse` |
| Output limit | `max_tokens` | `generationConfig.maxOutputTokens` |
| Model name | Request body | URL path |
| Catalog ID | `data[].id` | `models[].name` |

- The key uses `x-goog-api-key`. Streaming uses `alt=sse`. Model names accept a single optional `models/` prefix.
- Function schemas strip unsupported fields such as `$schema` and `additionalProperties`. Arguments use JSON objects. A tool result is linked by function name, and opaque thought signatures are returned with the original call.
- Gemini may close an SSE stream without a stop event. The client retains completed tool calls through EOF. Model discovery uses `supportedGenerationMethods` when available and falls back to name filtering when absent.
- The September probe found that `gemini-2.5-flash`, `gemini-2.5-pro`, and `gemini-2.5-flash-lite` remained listed after returning HTTP 404 to requests. Native audio and live models supported `bidiGenerateContent` only. Filtering for the literal name `streamGenerateContent` would have excluded all 58 entries observed at that time.
- Its separate TTS model returned raw mono PCM at 24 kHz. VAVIS adds a WAV header for playback. Provider-matched voice can be disabled and preserves a configured keyless engine.

## Recording results

Add the date, model, and status to the table. Add a local-socket regression test before changing request or stream handling. Record live account access separately from simulated API behavior, and remove temporary probes from the checkout after use.

## References

- [Groq rate limits](https://console.groq.com/docs/rate-limits)
- [Groq built-in tools](https://console.groq.com/docs/tool-use/built-in-tools)
- [Groq security onboarding](https://console.groq.com/docs/production-readiness/security-onboarding)
- [OpenRouter streaming errors](https://learn.openrouter.ai/courses/first-request/stream-it)
- [Anthropic model listing](https://platform.claude.com/docs/en/api/models/list)

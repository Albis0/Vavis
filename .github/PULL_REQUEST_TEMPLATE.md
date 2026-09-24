## What this changes

<!-- One or two sentences. What behaviour is different after this PR? -->

## Why

<!-- The reasoning, not the diff. What problem does this solve? -->

## Checks

- [ ] `cargo test --all` passes
- [ ] `cargo clippy --all-targets -- -D warnings` is clean
- [ ] `cd ui && bun run check && bun run test` pass (if `ui/` changed)
- [ ] New behaviour has a test that fails without the change

## Invariants

Tick any this PR touches, and say how it stays intact
(see [CONTRIBUTING.md](../blob/main/CONTRIBUTING.md)):

- [ ] A request offers at most 12 tools
- [ ] Conversational messages get no tools
- [ ] Barge-in does not start the next utterance
- [ ] Everything counts against the context budget
- [ ] Anything that changes something asks; the guards outrank grants
- [ ] Outside text never reaches a PowerShell script unquoted

<!-- If this adds a tool: did the selection eval stay at 100%? -->

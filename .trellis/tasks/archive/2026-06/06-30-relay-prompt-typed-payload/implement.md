# Execution Plan

## Steps

- [x] Replace relay app port prompt request `prompt_blocks` with typed `input`.
- [x] Replace relay protocol `CommandPromptPayload.prompt_blocks` with typed `input`.
- [x] Update cloud relay connector to pass canonical typed input directly.
- [x] Update local prompt handler to use `payload.input` directly.
- [x] Delete relay-only ACP conversion helpers/tests.
- [x] Add or update typed serialization and cloud/local handler tests.
- [x] Run targeted checks.

## Validation

Preferred targeted commands:

- `cargo fmt --check`
- `cargo test -p agentdash-relay prompt --lib`
- `cargo test -p agentdash-local prompt --lib`
- `cargo test -p agentdash-application relay_connector --lib`
- `rg -n "prompt_blocks|relay_prompt_blocks_to_user_input|user_input_blocks_to_relay_content_blocks" crates packages`

If package test names differ, run the nearest targeted package tests and report exact commands.

## Subagent Instructions

Use `trellis-implement` for code changes and `trellis-check` for verification.

Every dispatch prompt must start with:

```text
Active task: .trellis/tasks/06-30-relay-prompt-typed-payload
```

Workers must prioritize deleting the old raw JSON relay prompt path over adding compatibility.

## Completion Result

- `RelayPromptRequest` and `CommandPromptPayload` now carry typed `input: Vec<UserInputBlock>`.
- Cloud relay connector passes `PromptPayload::Input` through directly and converts text prompts once to canonical text blocks.
- Local prompt handler constructs `UserPromptInput` directly from typed relay payload input.
- Relay-only ACP ContentBlock JSON conversion helpers and tests were removed.
- Typed relay prompt tests cover text, image, local image, skill, and mention payload roundtrip / local preservation.

## Checks Run

- `cargo fmt --check`
- `git diff --check`
- `cargo test -p agentdash-relay prompt --lib`
- `cargo test -p agentdash-local prompt --lib`
- `cargo test -p agentdash-application relay_connector --lib`
- `cargo test -p agentdash-api workspace_resolution --lib`
- `rg -n "prompt_blocks|relay_prompt_blocks_to_user_input|user_input_blocks_to_relay_content_blocks" crates packages`

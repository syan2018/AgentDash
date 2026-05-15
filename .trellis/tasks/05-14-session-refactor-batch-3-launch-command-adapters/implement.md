# Implementation Plan：Batch 3 Gate

## Current Decision

Batch 2b 已补完 launch fallback summary，Batch 3 可以开始受控迁移。第一段先删除独立 `SessionLaunchIntent` 层，将 source / strictness / preparation / follow-up policy 吸收到 `LaunchCommand`，随后再逐入口迁移 production adapters。

## Progress

- [x] 删除 `SessionLaunchIntent` / `SessionLaunchPreparation` / `SessionLaunchStrictness` / `SessionLaunchSource`。
- [x] 新增 `LaunchCommand` source policy，并迁移 hub facade 到 `launch_command`。
- [x] 覆盖 strict augmenter 与 LaunchCommand source policy focused tests。
- [x] HTTP prompt 与 Local relay prompt 改为在入口构造 `LaunchCommand`。
- [x] Task / Workflow / Routine 入口改为通过 `LaunchCommand::*_prepared` 消化 `PreparedSessionInputs`。
- [x] Companion dispatch / parent resume 与 Hook auto-resume 改为通过 `LaunchCommand` source adapter 启动。
- [x] 删除 `launch_*prompt` 迁移 wrapper，hub facade 只保留 `launch_command` 与低层 `start_prompt`。
- [x] `PromptRequestAugmenter` 输入改为 `PromptAugmentInput`，切断 request 作为跨层 augment 入参。
- [x] 删除旧 `PromptSessionRequest` 类型名；pipeline 内部输入投影收口为 `PreparedLaunchPrompt`。

## Exit Criteria For This Gate

- `PromptSessionRequest` 在生产源码中零命中。
- `SessionLaunchIntent` 删除。
- 所有外围入口通过 `LaunchCommand` source adapter 启动。

## Verification

```powershell
cargo test -p agentdash-application session::launch
cargo test -p agentdash-application launch_prompt_strict_requires_prompt_augmenter
cargo test -p agentdash-application schedule_hook_auto_resume_strict_mode_requires_augmenter
cargo test -p agentdash-api acp_sessions
cargo test -p agentdash-local
cargo check -p agentdash-application
cargo test -p agentdash-application
cargo test -p agentdash-api acp_sessions
cargo test -p agentdash-application session::hub::tests::schedule_hook_auto_resume_routes_through_augmenter
cargo test -p agentdash-application companion_parent_resume_routes_through_augmenter
cargo fmt --check
rg -n "PromptSessionRequest" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
rg -n "SessionLaunchIntent|SessionLaunchPreparation|SessionLaunchStrictness|SessionLaunchSource|launch_intent|launch_prompt_with_intent" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

上述测试和 fmt 均通过；`PromptSessionRequest` 与 `SessionLaunchIntent` 相关 `rg` 均无命中。`cargo check -p agentdash-application` / `cargo test -p agentdash-application` 仍有既有 `CANVAS_SYSTEM_RUNTIME_BRIDGE_REFERENCE_PATH` unused import warning，非本批次引入。

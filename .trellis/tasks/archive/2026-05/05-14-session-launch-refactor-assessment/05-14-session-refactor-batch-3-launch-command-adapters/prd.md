# Session Refactor Batch 3：LaunchCommand Adapters 与 PromptSessionRequest 删除

## Goal

将所有生产入口从旧 `PromptSessionRequest` 迁移到 `LaunchCommand`，并删除该生产主链路。Batch 3 的最终形态是：外围入口只构造 `LaunchCommand`；需要补齐 owner/context/capability 的路径使用 `PromptAugmentInput`；prompt pipeline 内部消费 `PreparedLaunchPrompt`，该类型只表示进入 `LaunchExecution` 前的已补齐输入投影。

## Current Fact

旧 `PromptSessionRequest` 已从生产源码删除；原入口现状如下：

- HTTP prompt：已迁移到 `LaunchCommand::http_prompt_input`
- 本机 relay：已迁移到 `LaunchCommand::local_relay_prompt_input`
- task：已迁移到 `LaunchCommand::task_service_prepared`
- workflow：已迁移到 `LaunchCommand::workflow_orchestrator_prepared`
- routine：已迁移到 `LaunchCommand::routine_executor_prepared`
- companion dispatch / parent resume：已迁移到 `LaunchCommand` adapter
- hook auto-resume：已迁移到 `LaunchCommand::hook_auto_resume_input`
- hub facade wrappers：已删除

## Requirements

- 新增 `LaunchCommand` source variants：HTTP、Task、Workflow、Routine、CompanionDispatch、CompanionParentResume、HookAutoResume、LocalRelay。
- 每个 adapter 只表达来源意图和 source contract，不携带 composition / execution / terminal effect 语义。
- `SessionLaunchIntent` 应被吸收进 `LaunchCommand` source policy 或删除。
- `PromptRequestAugmenter` 不再以旧 `PromptSessionRequest` 作为跨层输入；augment 入参只表达原始 prompt、identity、VFS/MCP request overlay 等启动原料。
- 旧 `PromptSessionRequest` 从生产主链路删除；pipeline 内部类型命名为 `PreparedLaunchPrompt`。
- 删除前必须先完成 Batch 2 follow-up：pipeline 中剩余 lifecycle、VFS/MCP/capability、hook/follow-up fallback 必须进入 `LaunchExecution` builder。

## Acceptance Criteria

- [x] `LaunchCommand` variants 覆盖全部生产入口。
- [x] HTTP / Local relay / Task / Workflow / Routine / Companion / Hook auto-resume 不再直接构造 `PromptSessionRequest`。
- [x] `SessionLaunchIntent` 不再作为主链路分发层。
- [x] `rg "PromptSessionRequest" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src` 零命中。
- [x] `launch_*prompt` wrappers 不再承载业务分发。

## Progress

- [x] Batch 2b 已补完 fallback summary，Batch 3 gate 解除。
- [x] `SessionLaunchIntent` 已吸收到 `LaunchCommand` source policy。
- [x] HTTP / Local relay / Task / Workflow / Routine / Companion / Hook auto-resume 逐入口迁移到 `LaunchCommand` source adapter。
  - [x] HTTP prompt 入口已不再直接构造 `PromptSessionRequest`。
  - [x] Local relay prompt 入口已不再直接构造 `PromptSessionRequest`。
  - [x] Task / Workflow / Routine / Companion / Hook auto-resume 已迁移。
- [x] `PromptRequestAugmenter` 已改为接收 `PromptAugmentInput`，不再把 `PromptSessionRequest` 暴露为 augment 输入协议。
- [x] 删除 `PromptSessionRequest` 生产主链路；原内部 pipeline 输入更名为 `PreparedLaunchPrompt`，作为 `LaunchExecution` 前的已补齐输入投影。

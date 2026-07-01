# Implementation Plan

## Step 1: 重写 default_system_prompt.md

- [ ] 1.1 重写 `crates/agentdash-executor/src/connectors/pi_agent/prompts/default_system_prompt.md`
- [ ] 1.2 检查 `crates/agentdash-application-shared-library/src/seed.rs` 中 builtin preset 的 system_prompt 是否依赖旧 base prompt 中的 coding 措辞
- [ ] 1.3 搜索项目中引用 DEFAULT_SYSTEM_PROMPT 的地方，确认无硬依赖

验证: `cargo check -p agentdash-executor`

## Step 2: Compaction Handoff 提示

- [ ] 2.1 修改 `crates/agentdash-application-runtime-session/src/session/compaction_context_frame.rs` 的 `rendered_text()` 方法
- [ ] 2.2 更新该文件中的单元测试

验证: `cargo test -p agentdash-application-runtime-session compaction`

## Step 3: Environment Context Frame

- [ ] 3.1 在 `agentdash-spi/src/hooks/mod.rs` 的 `delivery_order_for_kind` / `delivery_phase_for_kind` / `model_channel_for_kind` 中注册 `"environment"` kind
- [ ] 3.2 新建 `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs`
  - struct `EnvironmentFrameInput` (date, platform, arch, model_id, executor, working_directory)
  - struct `EnvironmentContextFrame`
  - impl `ContextFramePayload`
  - pub fn `build_environment_context_frame`
- [ ] 3.3 在 `crates/agentdash-application-runtime-session/src/session/mod.rs` 新增 mod 声明
- [ ] 3.4 在 `preparation.rs` 中 identity_frames 和 guidelines_frame 之间构建 environment frame 并加入 turn_context_frames
- [ ] 3.5 新增单元测试

验证: `cargo test -p agentdash-application-runtime-session environment`

## Step 4: Companion 子代理上下文继承

- [ ] 4.1 在 `crates/agentdash-application/src/companion/` 下新增 `inherited_summary.rs`
  - struct `CompanionInheritedSummary`
  - fn `render_inherited_summary` → ContextFragment
- [ ] 4.2 在 `request_assembler.rs` 新增 `resolve_companion_inherited_summary` 方法（从父级 session 获取 user_preferences + environment facts）
- [ ] 4.3 在 `companion/tools.rs` 的 `dispatch_child` 流程中调用 resolve + render，将 fragment 加入 compose_fragments
- [ ] 4.4 确认 `build_assignment_context_frame` 能拾取 slot=`inherited_parent_context` 的 fragment

验证: `cargo check -p agentdash-application`

## Step 5: 全局验证

- [ ] 5.1 `cargo check` 全项目
- [ ] 5.2 `cargo test` 涉及的 crate
- [ ] 5.3 Review rendered output 格式是否符合预期

## Commit 策略

一次提交包含所有四个改动（逻辑上是一个 feature），commit message:
`feat(prompt): 补全 Agent System Prompt 体系 — 重写默认 prompt、环境 frame、companion 继承、compaction 提示`

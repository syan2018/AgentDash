# 项目指引注入重构：guidelines 独立帧 + identity 单一真相源

## Goal

把 AGENTS.md / MEMORY.md 等项目指引（以及用户偏好）从 identity 帧中剥离，迁到独立的系统级 guidelines 帧；消除 identity 帧 `effective_prompt` 与 `rendered_text` 对同一份"系统提示词"的双写；并为 AGENTS.md 引入确定性的层级就近合并语义。最终让连接器对系统提示词的组装有单一、可预测的真相源，去掉今天的 `extract_identity_prompt` fallback 顺序止血补丁。

## Background / 问题陈述

当前实现（截至 commit `4971bf46`）：

1. `discover_mount_files` 发现 AGENTS.md/MEMORY.md → `DiscoveredGuideline`。
2. `build_identity_context_frame` 把 guidelines + user_preferences 拼进 **identity 帧的 `rendered_text`**（`## Project Guidelines` / `## User Preferences` 段），但结构化 `sections[Identity].effective_prompt` **只含身份**（base + agent prompt）。
3. 同一逻辑"系统提示词"被双写到两处且不一致；guidelines/preferences 只活在 `rendered_text`。
4. `pi_agent::extract_identity_prompt` 此前优先读 `effective_prompt`，导致 guidelines/preferences 丢失。今天的 fix 把优先级翻转为 `rendered_text` 优先来止血。

问题：
- **根因未除**：两份拷贝可漂移；fallback 翻转只是换一份赢，下一个读 `effective_prompt` 的消费者会再翻车。
- **语义层级错配**：项目指引被焊进"身份"，与"我是谁"耦合；代码里本有 `project_guidelines` 槽（`ASSIGNMENT_CONTEXT_SLOTS`）却是死的。
- **行为变更未被有意识决策**：fallback 翻转后模型实际收到的 system prompt 多了 `## Identity` 等 markdown 脚手架。
- **合并语义偏离惯例**：AGENTS.md 惯例是层级就近（最近文件优先/逐级合并），现实现把根 + 一级子目录所有命中文件无序 concat、无去重、深度仅 1 层。

## Scope

完整重构（用户已确认）。包含：identity 帧净化、独立系统级 guidelines 帧、连接器系统提示词组装统一、AGENTS.md 合并语义。

### In scope
- identity 帧只承载系统身份（base + agent prompt），`rendered_text` 由结构化数据派生，杜绝双写。
- 新增独立的系统级 guidelines 帧，承载 project guidelines 与 user preferences，`rendered_text` 由结构化 section 单一派生。
- 连接器（pi_agent）系统提示词组装：身份帧 + guidelines 帧合并为最终 system prompt，并修正 `last_identity_prompt` 变更检测；去掉 fallback 止血补丁。
- 帧通道路由：新 guidelines 帧与 identity 一样走系统通道、从 turn-notice 通道排除，避免重复投递。
- AGENTS.md 合并：确定性排序 + 去重 + 明确就近优先语义。
- 兼容 codex_bridge（`compose_prompt_text` 渲染全部帧）路径不回归。

### Out of scope
- 重做整套 ContextFrame / hook 投递协议。
- skill 发现路径（`BUILTIN_SKILL_RULES`）改动。
- 前端展示层重构（除非帧结构变更导致必须跟随）。
- MEMORY.md 的语义扩展（沿用与 AGENTS.md 相同的发现/合并规则即可）。

## Requirements

- R1：identity 帧不再包含 user_preferences / discovered_guidelines；其 `rendered_text` 与结构化 section 表达同一份内容，不存在第二份可漂移的拷贝。
- R2：项目指引 + 用户偏好通过独立的系统级帧投递，帧的 `rendered_text` 由结构化 section 经单一渲染函数派生。
- R3：连接器对"系统提示词"有单一组装路径（身份 + guidelines），其变更检测覆盖 guidelines 变化；删除 `extract_identity_prompt` 的 fallback 顺序补丁。
- R4：新帧不被重复投递（既进系统提示词又进 turn 上下文），不破坏 dedupe / 审计 / 摘要 / bridge replay。
- R5：AGENTS.md/MEMORY.md 合并具备确定性：稳定排序、按规范化路径去重、就近优先语义有明确定义并被测试覆盖。
- R6：codex_bridge 路径行为不回归（项目指引仍出现在其 prompt 文本中）。
- R7：现有系统提示词刷新优化（identity 不变则不重置 system prompt）语义保持等价或更正确。

## Acceptance Criteria

- [x] AC1：guidelines 帧在偏好/指引非空时构建并经 connector_context 通道进系统提示词，`assemble_system_prompt` 合并身份+指引；空时 `build_guidelines_context_frame` 返回 None（删除 AGENTS.md → 不含）。单测 `assemble_system_prompt_combines_identity_and_guidelines` / `empty_inputs_produce_no_frame`。（R1/R2/R3）
- [x] AC2：identity 帧 `rendered_text` 为原样身份（`identity_frame_rendered_text_only_carries_identity`）；guidelines 帧 `rendered_text == render_sections(sections)`（`rendered_text_is_derived_from_sections`），无第二份手写拷贝。（R1）
- [x] AC3：`extract_identity_prompt` 重构为单一 `assemble_system_prompt`，删除 effective_prompt/rendered_text fallback 分支；单测覆盖合并结果。（R3）
- [x] AC4：`preparation.rs::enqueue_context_frames_for_transform_context` 排除名单加入 `system_guidelines`，不重复作为 turn-notice 投递。（R4）
- [x] AC5：user_preferences 迁入 guidelines 帧并进系统提示词（`preferences_only_omits_guidelines_section` + assemble 合并）。（R1/R2）
- [x] AC6：`merge_discovered_guideline_files` 稳定排序 (mount_id, 深度, 规范化路径) + 去重；单测 `merge_guidelines_sorts_by_mount_depth_path_and_dedupes`。（R5）
- [x] AC7：codex_bridge 走 `compose_prompt_text` 渲染全部帧（含新 guidelines 帧、identity 帧内容不变），路径未改动，指引仍在 prompt 文本中。（R6）
- [x] AC8：`cargo build`（全 workspace）+ application/executor/spi `cargo test --lib` 通过；新增/调整单测全绿。唯一失败 `hooks::script_engine::tests::script_reads_ctx_params` 经 `git stash` 验证为 HEAD 既有、与本任务无关（script_engine.rs 与 rhai 依赖均未改动）。

## Constraints

- 遵循仓库 ContextFrame / ContextFramePayload 既有抽象，新帧复用 `build_context_frame` 机制。
- 不引入对本机路径的依赖（沿用现有 VFS 抽象）。
- 直接提交 main，仅 stage 相关文件，默认不 push（见用户惯例）。
- 单测以验证行为为主，不为旧的低价值测试束缚设计。

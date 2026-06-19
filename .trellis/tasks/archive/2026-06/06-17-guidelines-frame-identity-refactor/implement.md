# 执行计划：guidelines 独立帧 + identity 单一真相源

按"先下层契约、再组装、最后连接器消费"的顺序推进。每步结束应可独立 `cargo build`。连接器消费（Step 4）前，application 侧通过临时兼容保持系统提示词不丢，便于分步回滚。

## Step 0 · 基线确认 `[review gate]`
- [ ] 复读 design.md §1 链路，确认以下事实仍成立：
  - identity 帧在 `preparation.rs` 排除名单内（不走 turn-notice）。
  - pi_agent 仅 `extract_identity_prompt` 读 identity 帧；codex_bridge 用 `compose_prompt_text` 渲染全部帧。
  - guidelines 仅在 `include_connector_startup_context` 时构建。
- 验证：`grep` 复核上述三处；无需编译。

## Step 1 · SPI 契约：新增结构化 section 变体
- [ ] 在 `crates/agentdash-spi/src/hooks/mod.rs` 的 `ContextFrameSection` 增加**两个独立变体**（已定：拆两段，不合并）：
  - `ProjectGuidelines { title, summary, entries: Vec<GuidelineEntry{ path, content }> }`
  - `UserPreferences { title, summary, items: Vec<String> }`
- [ ] 若需要新结构体（`GuidelineEntry`），定义并补 `Serialize/Deserialize/PartialEq/Debug/Clone`。
- 验证：`cargo build -p agentdash-spi`。
- 回滚点：仅新增类型，未接线，可安全保留或还原。

## Step 2 · 新建 guidelines 帧 payload（单一真相源）
- [ ] 新增 `crates/agentdash-application/src/session/guidelines_context_frame.rs`：
  - `GuidelinesFrameInput<'a> { user_preferences: &'a [String], discovered_guidelines: &'a [DiscoveredGuideline] }`
  - `build_guidelines_context_frame(&GuidelinesFrameInput) -> Option<ContextFrame>`（两者皆空返回 None）。
  - `ContextFramePayload` 实现：`kind="system_guidelines"`、`delivery_channel="connector_context"`、`message_role="system"`。
  - **单一渲染**：`sections()` 产出结构化数据；`rendered_text()` 调用同一 `render_guidelines(&sections)`，杜绝手写第二份。
- [ ] `session/mod.rs` 导出 `build_guidelines_context_frame`。
- 验证：`cargo build -p agentdash-application` + 新增渲染单测（section ↔ rendered_text 一致）。

## Step 3 · 净化 identity 帧 + 接线 preparation
- [ ] `identity_context_frame.rs`：从 `IdentityFrameInput` / `IdentityContextFrame` 移除 `user_preferences`、`discovered_guidelines`；`rendered_text` 改为仅 `## Identity\n\n{effective_prompt}`；同步删除对应单测分支。
- [ ] `preparation.rs`：
  - 在构建 identity 帧之后，用相同的 `user_preferences` + `discovered_guidelines` 构建 guidelines 帧；`include_connector_startup_context` 为真且非空时 push 进 `turn_context_frames` 与 `accepted_context_frames_to_emit`。
  - `enqueue_context_frames_for_transform_context` 排除名单加入 `"system_guidelines"`。
- 验证：`cargo build -p agentdash-application`；单测断言 identity 帧 `rendered_text` 不含 guidelines/preferences，guidelines 帧含。
- 注意：此步后但 Step 4 前，pi_agent 仍只读 identity → 会暂时丢指引。**Step 3、4 在同一提交内完成**，或 Step 4 紧随，避免中间态进主干。

## Step 4 · 连接器消费：统一系统提示词组装（去补丁）
- [ ] `pi_agent/connector.rs`：
  - 将 `extract_identity_prompt` 重构为 `assemble_system_prompt(frames) -> Option<String>`：取 `kind=="identity"` 帧 `rendered_text` + `kind=="system_guidelines"` 帧 `rendered_text`，按序拼接；空则 None。
  - 删除原 fallback（effective_prompt vs rendered_text 顺序）分支——identity 帧现已单一派生，直接用 `rendered_text`。
  - `incoming_identity_prompt` / `last_identity_prompt` 缓存语义改为"组合系统提示词"，确保 guidelines 变化能触发 `set_system_prompt` 重置，guidelines 不变不误触发。
- [ ] 调整/新增 `connector_tests.rs`：用"身份帧 + system_guidelines 帧"断言组合结果含两者；移除/改写 `extract_identity_prompt_preserves_rendered_guidelines`（其前提已变）。
- 验证：`cargo build -p agentdash-executor` + 该 crate 相关单测。

## Step 5 · AGENTS.md 合并语义
- [ ] 在 `derive_session_guidelines`（或 `discover_mount_files` 结果处理）加入：
  - 稳定排序：(mount_id, 路径深度, 路径字典序)。
  - 按规范化路径去重。
  - 逐级追加 + 明确顺序（靠后更具体/优先），渲染保留 `### {path}` 来源标注。
- [ ] 在代码注释/`mount_file_discovery.rs` 文档化"仅扫描根 + 一级子目录"的深度限制。
- [ ] **不实现"深层覆盖"（段级替换）**：当前平铺发现无工作文件锚点、无真实触发 case（详见 design §3.4），仅文档化为未来"按编辑文件逐级解析"时的独立增强。
- 验证：单测覆盖多文件排序 + 去重 + 顺序断言。

## Step 6 · 回归与跨层验证
- [ ] codex_bridge：确认 `compose_prompt_text` 仍含项目指引、且不重复（既有行为基线）。补/查单测。
- [ ] 核对 `dedupe_context_frames` 不会吞掉 `system_guidelines` 帧。
- [ ] 核对 title-gen / summarizer / audit / bridge-replay 对新 kind 的 scope 处理无副作用（按需读取相关消费点）。
- 验证：`cargo build`（全 workspace）+ `cargo test -p agentdash-application -p agentdash-executor -p agentdash-spi`。

## Step 7 · 收尾
- [ ] 跑 `cargo fmt` / clippy（按仓库惯例）。
- [ ] 对照 prd.md AC1–AC8 自检逐条勾选。
- [ ] spec 更新（若本次澄清了帧通道/系统提示词组装约定，沉淀到 `.trellis/spec`）。
- [ ] 提交：直接 main，仅 stage 相关文件，默认不 push。

## 验证命令汇总
```bash
cargo build -p agentdash-spi
cargo build -p agentdash-application
cargo build -p agentdash-executor
cargo build
cargo test -p agentdash-application -p agentdash-executor -p agentdash-spi
cargo fmt
```

## 审查门（review gates）
- Step 0 后：事实基线确认无误再动手。
- Step 3↔4 之间：必须连续/同提交，避免主干出现"指引被丢"的中间态。
- Step 6 后：跨层回归全绿才进入收尾提交。

## 回滚点
- Step 1/2 纯新增，可独立保留。
- Step 3+4 为一组原子改动；如连接器侧出问题，可临时让 `assemble_system_prompt` 退回"identity 帧 rendered_text 含全部"（等价今天的止血态）作为过渡，再排查。
- Step 5 合并语义独立，可单独回退而不影响帧重构。

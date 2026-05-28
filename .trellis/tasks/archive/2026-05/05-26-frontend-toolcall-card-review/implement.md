# 实施计划：工具调用卡片信息架构重构

## 阶段拆分

按照"先共享基础 → 后端协议基线确认 → 前端 shell+注册表 → 各 renderer → 清理"的
顺序推进。每个阶段结束都应能独立编译/通过测试，便于阶段性提交。

## 当前后端基线（2026-05-26 修订）

本任务的后续 P3 从 `.trellis/tasks/05-26-backend-tool-event-source-convergence`
之后继续：前端只消费 Backbone `AgentDashThreadItem`，不再把 vibe-kanban
`ActionType` 当成平台工具语义来源。

- Codex Protocol 已有的 `ThreadItem` / status enum / 输出片段直接使用。
- `fs_apply_patch` 使用 Codex `fileChange`，前端复用 FileChange renderer。
- AgentDash 自有工具执行事实从 `agentdash-agent-types::AgentDashNativeThreadItem`
  扩展，当前覆盖 `fsRead` / `fsGrep` / `fsGlob`。
- vibe-kanban `ActionType` 只属于 legacy executor adapter；前端 P3 不围绕
  `ActionType` 设计 renderer，也不要求新增 `ActionType -> UI` 映射。

---

### Phase 0 — 验证 & 起点（不改代码）

- [x] P0.1 ActionType 全 variant 已确认：`FileRead/FileEdit/CommandRun/Search/`
      `WebFetch/Tool/TaskCreate/PlanPresentation/TodoManagement/AskUserQuestion/Other`
      （`vibe-kanban/crates/executors/src/logs/mod.rs:170`）
- [x] P0.2 FileUpdateChange schema 已确认：`{ path, kind: PatchChangeKind, diff }`，
      `PatchChangeKind = add | delete | update{move_path?}`。
      OQ2 决议：`executors::FileChange::Write` → `kind:add`、
      `Delete` → `kind:delete`、`Rename{new_path}` → `kind:update{move_path:Some}`、
      `Edit{unified_diff}` → `kind:update{move_path:None} + diff:unified_diff`。
      add/delete 不合成 unified_diff，`diff` 填空字符串，前端 FileChangeCardBody
      按 `kind` 分支渲染（"新建文件" / "删除文件" / "重命名 → X" / unified diff）
- [x] P0.3 基线：
      - `cargo test -p agentdash-executor --lib`：58 passed ✅
      - `pnpm -C packages/app-web test --run`：202 passed ✅
      - `pnpm -C packages/app-web typecheck`：**预存红**（`workflowStore.ts` /
        `types/workflow.ts`，与本任务无关）。验证标准校准为"不引入新 ts error"
- [x] P0.4 OQ3 决议：**不引入** feature flag，靠渐进 commit 分隔回滚

---

### Phase 1 — 前端 kind 注册表与孤儿清理（前端独立）

目标：把"分散的 kind 字面量"先收口；把已确认的孤儿/死代码删掉。这一阶段**不改
渲染分发**，仅做基础清理，风险最小。

- [x] P1.1 新增 `packages/app-web/src/features/session/model/threadItemKind.ts`
- [x] P1.2 改 `model/types.ts::getThreadItemKind` 调用 `resolveKind`
- [x] P1.3 改 `ui/SessionToolCallCard.tsx::getKindConfig` 改为读 KIND_REGISTRY
- [x] P1.4 改 `ui/SessionEntry.tsx::buildKindSummary` 改为按 KIND_REGISTRY 聚合
- [x] P1.5 `acp/tool-call.tsx` 已不存在（先前已删除）
- [x] P1.6 `SessionToolCallCard.compact` 先前已不存在
- [x] P1.7 `extractDetailContent` 内无 commandExecution 死分支
- [x] P1.8 验证通过

**Review gate**：P1 提交一次，pause 等用户确认后再进 P2。

---

### Phase 2 — 后端 Backbone/Codex 事实源归位（后端独立）

目标：前端 P3 开始前，后端主链路已经以 Backbone/Codex `ThreadItem` 为事实源；
vibe-kanban 的 `ActionType` 映射仅保留在 legacy adapter 边界。

**P2 原实现已被后续任务吸收并修正 ✅**：见
`.trellis/tasks/05-26-backend-tool-event-source-convergence`。该后续任务完成后，
本任务 P3 直接面向 Backbone `AgentDashThreadItem` 渲染。

- [x] P2.1 ✅ 实现位置由 `executor/adapters/threaditem_mapping.rs` 改为
      `agentdash-agent-protocol/src/backbone/thread_item.rs`（用户反馈"位置
      不合适，糊屎"后归位）。270 行 + 9 单测。提供类型安全 builder API：
      `command_execution` / `file_change` / `web_search` / `dynamic_tool_call` /
      `context_compaction`；`FileChangeSpec` 表达 Add/Delete/Edit/Rename；
      内部 cwd 自动 `current_dir().join` 绝对化（解决 AbsolutePathBuf 反序列化
      对绝对路径的强校验）
- [x] P2.2 ✅ `tool_use_envelopes` 内部抽 `build_thread_item` 函数按 ActionType
      分发；CommandRun → CommandExecution、FileEdit → FileChange、Search →
      WebSearch；FileRead/WebFetch/AskUserQuestion/TaskCreate/Other 走
      DynamicToolCall 但 tool 名规范化（Read/WebFetch/AskUserQuestion/Task/Other）
- [x] P2.3 ✅ cwd 由 builder 自动绝对化（OQ1 解决）
- [x] P2.4 ✅ 映射单测覆盖 legacy mapper 的主要分支。
- [x] P2.5 ✅ `pi_agent/stream_mapper.rs::make_command_execution_item` 改用
      shared builder，删除本地 JSON 拼接 hack 与 `status_to_source_str`；
      `extract_shell_args` 不再做 cwd 绝对化（统一交给 builder）。
      原计划"加 Bash/Edit/Search 白名单"未做——因为 builder 集中后，pi_agent
      已通过 builder 拿到正确 CommandExecution 构造，不再需要在 pi_agent 内
      重复白名单分发（连同 design.md §2.4 的方案也作废）
- [x] P2.6 ✅ 验证：
      - `cargo test -p agentdash-agent-protocol --lib`：9 passed
      - `cargo test -p agentdash-executor --lib`：70 passed (58 现有 + 12 新增)
      - 改动文件 clippy 0 warning（剩余 11 个 collapsible_if 是预存红，与本次无关）

**后续任务追加的收口结果**：
- `NormalizedToBackboneConverter` 已改名为 `VibeKanbanLogToBackboneConverter`。
- legacy dynamic fallback 保留 `entry.content` 到 `content_items`。
- 状态直接使用 Codex Protocol enum。
- `fs_apply_patch` 进入 Codex `fileChange`，前端后续直接复用 FileChange renderer。
- `AgentDashThreadItem` 从 `agentdash-agent-types` 导出，作为 Codex `ThreadItem` 与
  AgentDash native item 的统一 item lifecycle 输入。

**Review gate**：P3 以 Backbone `AgentDashThreadItem` 作为输入，不再等待
`ActionType` 路线继续展开。

---

### Phase 3 — 前端 ToolCallCardShell 与一级分发（前端核心重构）

目标：抽 shell + 注册表，逐步替换 `SessionToolCallCard`。P3 只基于
`AgentDashThreadItem.type` 做一级分发，`dynamicToolCall.tool` 做二级摘要；其输入
来自 Backbone stream，不关心 connector 原始语义来自 pi-agent、Codex bridge 还是
vibe-kanban legacy adapter。

- [x] P3.1 新增 `ui/ToolCallCardShell.tsx`（shared shell with header/fold/approval/elapsed timer）
- [x] P3.2 新增 `ui/toolCardRegistry.ts`（一级分发 + dynamicToolCall 二级摘要）
- [x] P3.3 改 `SessionEntry.tsx`：统一走 ToolCallCardShell + renderToolCallCard
- [x] P3.4 验证通过：lint 0 errors, 202 tests passed
- [x] P3.5 前端以 `AgentDashThreadItem` 为唯一输入契约

**Review gate**：P3 提交一次，pause；此时视觉应与改造前一致，是平移基础。

---

### Phase 4 — 各 renderer body 实现

每个 renderer 一个小 commit，可以并行也可以串行。

- [x] P4.1 `bodies/JsonTree.tsx` + `GenericJsonBody.tsx`（递归折叠树 + 入参/出参双分区 + 复制按钮）
- [x] P4.2 `bodies/FileChangeCardBody.tsx`（按文件 kind / +N -M / diff 展示）
- [x] P4.3 `bodies/McpCardBody.tsx`（入参/出参分区 + 错误提示，复用 GenericJsonBody）
- [x] P4.4 `bodies/WebSearchCardBody.tsx`（query + action 摘要）
- [x] P4.5 `bodies/ImageCardBody.tsx`（imageView / imageGeneration）
- [x] P4.6 `bodies/CollabAgentCardBody.tsx`（tool / status / prompt / model / threads）
- [x] P4.7 `bodies/DynamicToolCallCardBody.tsx`（复用 GenericJsonBody）
      二级摘要 10 种工具在 toolCardRegistry::getDynamicToolTitle 实现
- [x] P4.8 `bodies/CommandExecutionCardBody.tsx`（cwd / streaming output / footer / promote）
      header 移交 ToolCallCardShell（title: `$ {command}`）
- [x] P4.9 toolCardRegistry 所有分支直接使用专用 renderer（无 LegacyDetailView）
- [x] P4.10 旧 `SessionToolCallCard.tsx` + `CommandExecutionCard.tsx` 已删除
- [x] P4.11 验证：lint 0 errors, 202 tests passed

**Review gate**：P4 完成后用户验收一次视觉效果。

---

### Phase 5 — 最终验证与收口

- [x] P5.1 质量门通过：`pnpm lint` 0 errors, `pnpm test` 202 passed
      （typecheck 仅预存红，无新引入错误）
- [x] P5.2 孤儿验证：ToolCallView=0, acp/tool-call=0, extractDetailContent=0,
      getKindConfig 仅 threadItemKind.ts 注释
- [ ] P5.3 手动 dev server 验证（待用户确认）
- [x] P5.4 更新 `.trellis/spec/cross-layer/backbone-protocol.md` Tool Card Rendering 段落
- [x] P5.5 已在 spec 中记录"Codex 已有类型直接使用，AgentDash 仅在 Codex 不足时
      通过 AgentDashNativeThreadItem 加法扩展"约束

---

## 验证命令汇总

```bash
# 后端
cargo test -p agentdash-executor
cargo test -p agentdash-application
cargo test -p agentdash-infrastructure
cargo clippy --workspace -- -D warnings

# 前端
pnpm -C packages/app-web typecheck
pnpm -C packages/app-web lint
pnpm -C packages/app-web test

# 手动
pnpm -C packages/app-web dev
# → 触发 Bash/Read/Edit/Grep/Glob/WebSearch/TodoWrite/未知工具，依次检查折叠态 header
```

## 提交节奏

每个 Phase 一个 commit（共 5 个），每个都是可发布、可回滚的最小单元。

| Phase | commit 主题 |
|-------|------------|
| P1    | feat(frontend): 收口 kind 注册表 + 清理 acp/tool-call 与 compact 死代码 |
| P2    | feat(executor): Backbone/Codex 工具事件事实源收束 |
| P3    | refactor(frontend): 抽 ToolCallCardShell + 一级分发注册表 |
| P4    | feat(frontend): 各工具卡 body renderer + dynamicToolCall 二级摘要 |
| P5    | chore: 收口 spec 与最终验证 |

## Rollback 路径

- P1 / P3 视觉变化：单 commit revert 即可
- P2 后端语义收束：由 `05-26-backend-tool-event-source-convergence` 单独承载；
  P3 只依赖 Backbone/Codex `ThreadItem` 输入契约
- P4 各 renderer：toolCardRegistry 内分支独立，单个 renderer 出问题
  可以临时 fallback 回 GenericJsonBody / LegacyDetailView，无需整体回滚

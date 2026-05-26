# 实施计划：工具调用卡片信息架构重构

## 阶段拆分

按照"先共享基础 → 后端语义还原 → 前端 shell+注册表 → 各 renderer → 清理"的
顺序推进。每个阶段结束都应能独立编译/通过测试，便于阶段性提交。

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

- [ ] P1.1 新增 `packages/app-web/src/features/session/model/threadItemKind.ts`
      （见 design.md §3.9）
- [ ] P1.2 改 `model/types.ts::getThreadItemKind` 调用 `resolveKind`，保留
      函数签名向后兼容
- [ ] P1.3 改 `ui/SessionToolCallCard.tsx::getKindConfig` 改为读 KIND_REGISTRY
- [ ] P1.4 改 `ui/SessionEntry.tsx::buildKindSummary` 改为按 `KIND_REGISTRY[meta.kind].label`
      聚合，不再硬编码 `"运行 N 条命令"` 字面量
- [ ] P1.5 删除 `packages/app-web/src/components/acp/tool-call.tsx`
      （先全仓 grep `ToolCallView` 与 `acp/tool-call` 路径，确认零外部引用）
- [ ] P1.6 删除 `SessionToolCallCard.compact` 模式分支与 `compact` prop
      （先全仓 grep `compact={true}` / `compact:` 在 SessionToolCallCardProps
      位置，确认零外部传值）
- [ ] P1.7 删除 `SessionToolCallCard::extractDetailContent` 内 `commandExecution`
      死分支
- [ ] P1.8 验证：`pnpm typecheck` + `pnpm lint` + `pnpm test` 全绿

**Review gate**：P1 提交一次，pause 等用户确认后再进 P2。

---

### Phase 2 — 后端 ActionType → ThreadItem 分发（后端独立）

目标：把 `normalized_to_backbone.rs` 的工具语义还原走通；前端无需任何变化即可
"自然"看到分化（旧 SessionToolCallCard 通用路径会接住）。

- [ ] P2.1 新增 `crates/agentdash-executor/src/adapters/threaditem_mapping.rs`，
      实现 `action_type_to_thread_item`（design.md §2.1–§2.2）
      - 包含 `command_status_from_tool_status`、`patch_apply_status_from_tool_status`、
        `dynamic_status_from_tool_status` 三个状态转换辅助
      - 包含 `convert_file_change_to_codex` 把 `executors::FileChange` 子枚举
        映射到 `codex::FileUpdateChange`（含为 Write/Delete/Rename 合成
        unified_diff 字符串的逻辑）
- [ ] P2.2 改 `tool_use_envelopes` 调用 `action_type_to_thread_item` 替代原地
      `ThreadItem::DynamicToolCall { ... }` 直接构造
- [ ] P2.3 cwd 缺失策略：从 `NormalizedEntry` / converter 字段推导，缺省 "."
      （OQ1）
- [ ] P2.4 新增单测覆盖（`normalized_to_backbone.rs` 测试模块或 `threaditem_mapping.rs::tests`）：
      - `CommandRun` → `CommandExecution`
      - `FileEdit{Edit}` / `FileEdit{Write}` / `FileEdit{Rename}` / `FileEdit{Delete}`
        → `FileChange`
      - `Search` → `WebSearch`
      - `Tool { tool_name="Read" }` → `DynamicToolCall(tool="Read")`
      - `TaskCreate` → `CollabAgentToolCall`
      - `Other` → `DynamicToolCall(tool="Other")`
- [ ] P2.5 改 `pi_agent/stream_mapper.rs` 加 Bash/Edit/Search 白名单
      （design.md §2.4），新增 1-2 个 connector_tests
- [ ] P2.6 验证：
      - `cargo test -p agentdash-executor` 全绿
      - `cargo clippy --workspace -- -D warnings` 无新增告警
      - `cargo test -p agentdash-application` / `-p agentdash-infrastructure`
        若已有 ThreadItem variant 测试，全部仍绿（验证 application/persistence
        层 match arms 仍然正确）

**Review gate**：P2 提交一次，pause 等用户验证 ThreadItem 流量是否正确。

---

### Phase 3 — 前端 ToolCallCardShell 与一级分发（前端核心重构）

目标：抽 shell + 注册表，逐步替换 `SessionToolCallCard`。每个 renderer 单独
PR-able。

- [ ] P3.1 新增 `ui/ToolCallCardShell.tsx`（design.md §3.2）
      - props: kind / title / status / isPendingApproval / sessionId / itemId
      - 内部完整接管 header / 折叠 / 审批按钮 / declined 提示 / approvalError 容器
      - 不再有 detail 渲染——children 透传
- [ ] P3.2 新增 `ui/toolCardRegistry.ts`（design.md §3.3）
      - 暴露 `renderToolCallCard(item, ctx) → { title, body }` 主入口
      - 一开始所有分支都直接 return `{ title: getThreadItemTitle(item), body: <LegacyDetailView /> }`，
        然后逐个分支替换为专用 renderer
- [ ] P3.3 改 `SessionEntry.tsx`：
      - 删除 `commandExecution → CommandExecutionCard` / `contextCompaction → SessionToolCallCard{kindOverride}`
        的特例分支
      - `item_started` / `item_completed` 一律走 `<ToolCallCardShell>{ renderToolCallCard(...).body }`
      - 短暂保留旧 `SessionToolCallCard` 作为 LegacyDetailView 内部实现，逐步替换
- [ ] P3.4 验证：所有现有 e2e / unit test 至少视觉等价（typecheck + test 全绿）

**Review gate**：P3 提交一次，pause；此时视觉应与改造前一致，是平移基础。

---

### Phase 4 — 各 renderer body 实现

每个 renderer 一个小 commit，可以并行也可以串行。

- [ ] P4.1 `bodies/jsonTree/JsonTree.tsx` + `GenericJsonBody.tsx`（design.md §3.5）
      - 单测覆盖：标量 / 对象 / 数组 / 嵌套 / 复制按钮
- [ ] P4.2 `bodies/FileChangeCardBody.tsx` + `countDiffLines` 工具函数（§3.6）
      - 单测：unified diff 字符串 → `{ added, removed }`
- [ ] P4.3 `bodies/McpCardBody.tsx`（入参/出参分区，复用 GenericJsonBody）
- [ ] P4.4 `bodies/WebSearchCardBody.tsx`（query + action）
- [ ] P4.5 `bodies/ImageCardBody.tsx`（imageView / imageGeneration）
- [ ] P4.6 `bodies/CollabAgentCardBody.tsx`
- [ ] P4.7 `bodies/DynamicToolCallCardBody.tsx` + `dynamicToolRenderers.ts`（§3.4, §3.7）
      - 实现 Read/Write/Grep/Glob/WebFetch/WebSearch/TodoWrite/AskUserQuestion 8 个 summarizer
      - body 默认走 GenericJsonBody；TodoWrite/AskUserQuestion 视具体长得不长得需要
        专用 body，可以延后
- [ ] P4.8 `CommandExecutionCard.tsx` 重构：
      - 把 header 移交 ToolCallCardShell（title 用 `$ {command}`）
      - body 保留 cwd / output / footer / promote-to-terminal 完整逻辑
      - 整体改为返回 `{ title, body }` 给 toolCardRegistry，对外不再是顶层组件
- [ ] P4.9 在 toolCardRegistry 内把所有分支从 LegacyDetailView 切换到对应 renderer
- [ ] P4.10 删除旧 `SessionToolCallCard.tsx`（如果还有引用）和
      `extractDetailContent` 工具函数
- [ ] P4.11 验证：
      - `pnpm typecheck` + `pnpm lint` + `pnpm test` 全绿
      - 手动跑 dev server，至少触发 bash / read / edit / grep / glob 五种调用，
        肉眼检查折叠态 header 摘要符合 design.md §3.7 表格

**Review gate**：P4 完成后用户验收一次视觉效果。

---

### Phase 5 — 最终验证与收口

- [ ] P5.1 跑全套质量门：
      - `cargo test --workspace`
      - `cargo clippy --workspace -- -D warnings`
      - `pnpm -C packages/app-web typecheck`
      - `pnpm -C packages/app-web lint`
      - `pnpm -C packages/app-web test`
      - 现有 playwright e2e（如有）
- [ ] P5.2 全仓 grep 验证孤儿已清：
      - `ToolCallView` → 0 命中
      - `acp/tool-call` → 0 命中
      - `SessionToolCallCard.compact` / `compact: true` 在 ToolCallCard 入参 → 0 命中
      - `extractDetailContent` → 0 命中
      - `getKindConfig` 字面量重复 → 仅 `threadItemKind.ts` 一处
- [ ] P5.3 手动构造一个未注册 tool 名（如 `tool="MyCustom"`）的 dynamicToolCall，
      验证 GenericJsonBody 兜底（AC4）
- [ ] P5.4 更新相关 spec/index：
      - `.trellis/spec/frontend/...` 如果有"工具调用卡片"专题文档，更新到新架构
      - `.trellis/spec/cross-layer/...` 如果有 ActionType ↔ ThreadItem 映射文档，
        加上新映射表
- [ ] P5.5 调用 `trellis-update-spec` 把"connector 必须把 ActionType 还原到对应
      ThreadItem variant"作为执行性约束写入 spec，避免未来新增 connector 时退化

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
| P2    | feat(executor): ActionType → ThreadItem variant 分发还原工具语义 |
| P3    | refactor(frontend): 抽 ToolCallCardShell + 一级分发注册表 |
| P4    | feat(frontend): 各工具卡 body renderer + dynamicToolCall 二级摘要 |
| P5    | chore: 收口 spec 与最终验证 |

## Rollback 路径

- P1 / P3 视觉变化：单 commit revert 即可
- P2 后端语义还原：单 commit revert（P3 之后视觉稳定，互不依赖）
- P4 各 renderer：toolCardRegistry 内分支独立，单个 renderer 出问题
  可以临时 fallback 回 GenericJsonBody / LegacyDetailView，无需整体回滚

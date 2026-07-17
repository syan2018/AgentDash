# 实施计划

> 本文只定义实施顺序、变更边界与验证门槛。任务在用户评审通过前保持 `planning`，
> 不运行 `task.py start`，也不修改生产代码。

## Phase 0：建立保护性基线

- [ ] 重新读取任务 PRD、design、research 与 context manifests。
- [ ] 运行 `git status --short`，记录实施开始前已有 dirty paths。
- [ ] 确认当前 migration 最大编号；若仍为 `0081`，使用
  `0082_runtime_thread_name_projection.sql`，否则顺延到下一个空闲编号。
- [ ] 确认 `WorkspaceTitlePort` 没有显式用户重命名调用者；只删除自动复制遗留，不碰
  仍有效的用户命名入口。
- [ ] 为协议、Runtime reducer、Agent title resolver 先补失败测试，锁定 direct cutover
  目标。

完成门槛：

- 受影响文件清单与既有 dirty paths 无重叠；若有重叠，保留并适配现有修改。
- 不创建兼容 flag、旧事件 reader 或双写方案。

## Phase 1：把标准 `thread/name/updated` 提升为 Backbone canonical event

### 1.1 Protocol

- [ ] 在 `agentdash-agent-protocol::codex_app_server_protocol` re-export
  `ThreadNameUpdatedNotification`。
- [ ] 在 `BackboneEvent` 资源/状态分组加入
  `ThreadNameUpdated(ThreadNameUpdatedNotification)`。
- [ ] 删除 `PlatformEvent::SourceSessionTitleUpdated`。
- [ ] 更新 protocol serialization/schema tests，覆盖 `Some` 和 `None` wire shape。

### 1.2 Codex adapter

- [ ] `mapping.rs` 对 vendor `ThreadNameUpdatedNotification` 做 strict transcode 后直接生成
  durable `BackboneEvent::ThreadNameUpdated`。
- [ ] 不再 trim/filter/drop `None`，不再合成 `preview/source/executor_session_id`。
- [ ] `bind_presentations` 从 `/thread/name` 合成同一个标准事件：
  - string → `Some`；
  - explicit null → `None`；
  - absent → no event。
- [ ] 增加 live/bind tests，断言两条路径的 immutable event body 完全相同。

### 1.3 Contract generation

- [ ] 运行 protocol codegen 更新 generated Rust/TS/schema。
- [ ] 全仓搜索确认不存在 `SourceSessionTitleUpdated` /
  `source_session_title_updated`。

完成门槛：

- Codex 标题结果只有标准 Backbone variant。
- `threadName=None` 未丢失。
- `pnpm run contracts:check` 对协议部分通过。

## Phase 2：建立 Runtime 当前名称投影与 migration

### 2.1 State model

- [ ] 给 `RuntimeThreadState` 增加非默认的 `thread_name: Option<String>`，新 thread 初始化
  为 `None`。
- [ ] 给 `RuntimeSnapshot` 增加 `thread_name` 并更新 snapshot builder/equality fixtures。
- [ ] 给 `DriverTranscript` 增加 `current_thread_name`，由 context broker 从 Runtime
  projection 读取。
- [ ] 更新 memory/Postgres repository roundtrip fixtures 和所有结构字面量。

### 2.2 Reducer/admission

- [ ] `apply_journal_fact` 识别标准名称事件，校验 source identity，然后 set/replace/clear
  `thread_name`。
- [ ] 保持 terminal item transcript reducer 独立，名称事件不进入 item transcript。
- [ ] driver ingress 校验 event `threadId == envelope.source_thread_id`。
- [ ] 重复相同值时状态保持相等；不同值和 `None` 按 journal sequence last-write-wins。
- [ ] 用 replay test 从完整 journal 重建同一 projection。

### 2.3 Persistence migration

- [ ] 新增 migration，为既有 `agent_runtime_thread.projection` 增加
  `"thread_name": null`。
- [ ] 同一 migration 清除 `workspace_title_source IN ('auto', 'codex')` 的旧自动
  Lifecycle 标题；保留 `user/source` 显式或继承标题。
- [ ] 更新 migration registry/guard 期望。
- [ ] 增加真实 Postgres roundtrip：迁移前形态可被迁移，新 projection 严格反序列化，
  set/clear/reload 一致。
- [ ] 不扫描旧标题事件、不从 Lifecycle workspace title 回填。

完成门槛：

- journal record 与 projection 由同一 `RuntimeCommit` 提交。
- Runtime inspect、cold driver context 与 replay 三种读取均返回同一 name。
- migration guard 和 Runtime repository targeted tests 通过。

## Phase 3：在 Agent 层实现 Managed Agent conversation naming

### 3.1 Agent deep module

- [ ] 新增 `agentdash-agent::conversation_naming`。
- [ ] 定义 `ConversationNamingInput`、validated `ConversationName` 与 typed error。
- [ ] 使用 `LlmBridge::complete` 构造无工具独立请求。
- [ ] 固定输出规则：单行、trim、移除包裹引号/Markdown title marker、非空、最多 22
  Unicode 字符。
- [ ] 单元测试覆盖中英文、空白、多行、引号、超长、空响应和 bridge failure。

### 3.2 Native adapter orchestration

- [ ] `NativeThread` 从 `DriverTranscript.current_thread_name` 初始化
  `Present/Idle/InFlight` gate。
- [ ] event pump 只在成功 completed turn 且取得 canonical user + final assistant messages
  后 claim naming。
- [ ] active turn fences 清理并发送 terminal，不等待 naming completion。
- [ ] naming job 使用独立 bridge 请求，不锁住 `Agent` 跨越网络 await，不改正文 history。
- [ ] 生成成功后创建：

  ```rust
  BackboneEvent::ThreadNameUpdated(ThreadNameUpdatedNotification {
      thread_id: binding.source_thread_id.to_string(),
      thread_name: Some(name),
  })
  ```

- [ ] 通过 binding-level envelope 发送：
  `operation_id/source_turn_id/source_item_id=None`，保留 binding/generation/source thread。
- [ ] sink 成功后 gate 进入 `Present`；LLM/普通 sink failure 回 `Idle`；stale/terminalized
  generation 结束旧作业。
- [ ] 不新增 `AgentEvent::TitleGenerated`，不让 Agent Core 依赖 protocol crate。

### 3.3 Native tests

- [ ] 主 terminal 先于 title event。
- [ ] 同一 thread 并发成功 turns 只调用一次 naming bridge。
- [ ] 已有 Runtime name 的 cold bind 不调用 naming bridge。
- [ ] naming failure 不改变 turn success，未来成功 turn 能重新尝试。
- [ ] rebind 后旧 generation 结果被 Runtime 拒绝。
- [ ] standard notification 是 durable、binding-level 且 payload thread ID 正确。

完成门槛：

- Managed Agent 主链能产生与 Codex 完全相同的标准结果事件。
- 命名能力停留在 Agent 层，AgentRun/Runtime 无模型依赖。

## Phase 4：统一 AgentRun list/workspace 展示

### 4.1 Shared resolver

- [ ] 在 `agentdash-application-agentrun` 增加纯 `AgentRunDisplayTitle` resolver。
- [ ] 测试固定：
  `workspace_title > runtime_thread_name > 新会话`。
- [ ] 空白输入不被当成有效 title。
- [ ] source 固定为显式 source/`workspace`、`runtime_thread`、`pending`。
- [ ] Project Agent label 不进入 resolver 参数。

### 4.2 List

- [ ] `AgentRunListRuntimeSummaryModel`/API contract 增加 `thread_name`。
- [ ] `agent_facts` 先 inspect Runtime，再用 shared resolver 计算 `title`。
- [ ] root/child list entry 使用同一个 resolved fact。
- [ ] 更新 API mapping 与 list query tests，覆盖 explicit/runtime/pending 和 identity label
  分离。

### 4.3 Workspace

- [ ] workspace current-delivery/runtime selection 携带 snapshot name。
- [ ] shell builder 调用 shared resolver，删除 Project Agent name title fallback。
- [ ] lineage/child display title 继续消费 resolved title。
- [ ] 更新 workspace facade/API tests 与 generated workflow contracts（若 wire shape变化）。

完成门槛：

- 列表、workspace shell、child reference 对同一 AgentRun 得到一致标题。
- `project_agent_label` 仍可显示身份，但不会替代 conversation title。

## Phase 5：接通 commit 后失效通知与前端刷新

### 5.1 Durable presentation observer

- [ ] 在 Runtime ports 增加通用 committed durable-presentation observer SPI；输入使用
  `RuntimeJournalRecord + projection_changed`，不增加 title payload。
- [ ] Gateway 在 reducer apply 前后比较 current name，把语义变化标记随 durable
  publication 保留。
- [ ] Gateway 只在 commit 成功和 durable presentation publish 后调用 observer。
- [ ] observer 错误只记录诊断，不逆转已成功 commit、不诱发 driver 重派。
- [ ] production Runtime composition 强制注册 AgentRun notifier；tests 可用显式空 observer
  集合。

### 5.2 AgentRun notifier

- [ ] 过滤 `BackboneEvent::ThreadNameUpdated`。
- [ ] 相同 before/after name 不发通知。
- [ ] 根据 runtime thread anchor/current binding 解析 run/agent，再读取 run.project_id。
- [ ] 发布现有：

  ```text
  ProjectProjectionInvalidation::agent_run_list(
      reason = TitleChanged,
      runtime_thread_id = Some(thread_id),
  )
  ```

- [ ] 非 AgentRun runtime thread 不发布项目事件；AgentRun anchor 缺失记录结构化诊断。
- [ ] 不把 name 写入 Lifecycle repository。

### 5.3 Frontend

- [ ] list store test 覆盖 `agent_run_list/title_changed` 触发一次最新第一页 refresh。
- [ ] workspace control-plane model 识别标准 `thread_name_updated`，计划
  workspace-state + AgentRun-list refresh。
- [ ] 不直接用 event payload patch shell title；刷新后读取后端 resolved title。
- [ ] 更新 session event classification tests，确保标准名称通知不是错误/未知平台事件。

完成门槛：

- 无需手动刷新即可在 list 和已打开 workspace 看到标题 set/replace/clear。
- 标题值仍只有 Runtime projection 一个事实源。

## Phase 6：删除遗留并更新规范

- [ ] 删除 `SourceSessionTitleUpdated` 的 Rust/TS/generated 残留。
- [ ] 删除确认无调用者的 `WorkspaceTitlePort` 自动写入遗留和 module export。
- [ ] 删除 Lifecycle 中只服务旧自动写入优先级的 `update_workspace_title` /
  `title_source_priority`；保留 fork 的 `user/source` 显式标题语义。
- [ ] 搜索确认没有 `AgentRunWorkspaceTitleAdapter`、RuntimeSession title generator/service、
  title `SessionMetaUpdate` 恢复。
- [ ] 更新以下可执行规范，记录最终“为什么”与 producer/admission/consumer matrix：
  - `backend/agent-runtime-kernel.md`
  - `backend/agent-runtime-native-adapter.md`
  - `backend/agent-runtime-codex-adapter.md`
  - `backend/agent-runtime-agentrun-facade.md`
  - `cross-layer/backbone-protocol.md`
  - `cross-layer/frontend-backend-contracts.md`
- [ ] 不在规范中记录旧实现历史或一次性任务说明。

完成门槛：

- `rg` 只剩标准事件与正式规范术语。
- 每个名称生命周期只有一个 producer、一个 Runtime admission/reducer 和明确 consumers。

## Phase 7：定向验证与完整交付检查

### Rust 定向测试

```powershell
cargo test -p agentdash-agent conversation_naming
cargo test -p agentdash-agent-protocol
cargo test -p agentdash-integration-codex
cargo test -p agentdash-integration-native-agent --test native_driver
cargo test -p agentdash-agent-runtime --test runtime_interface
cargo test -p agentdash-infrastructure runtime_thread_name
cargo test -p agentdash-application-agentrun
cargo test -p agentdash-application agent_run_list
cargo test -p agentdash-api agent_run
```

### Contracts / migration / frontend

```powershell
pnpm run migration:guard
pnpm run contracts:check
pnpm --filter app-web test -- src/features/agent/agent-run-list-state-store.test.ts src/features/agent-run-workspace/model/controlPlaneModel.test.ts src/features/session/model/platformEvent.test.ts
pnpm --filter app-web run typecheck
```

### Targeted check

```powershell
cargo check -p agentdash-agent -p agentdash-agent-protocol -p agentdash-agent-runtime -p agentdash-agent-runtime-contract -p agentdash-integration-api -p agentdash-integration-codex -p agentdash-integration-native-agent -p agentdash-application-agentrun -p agentdash-application -p agentdash-infrastructure -p agentdash-api
```

### Source gates

```powershell
rg -n "SourceSessionTitleUpdated|source_session_title_updated|AgentRunWorkspaceTitleAdapter" crates packages
rg -n "workspace_title.*runtime|update_workspace_title.*auto" crates
rg -n "ThreadNameUpdated|thread_name_updated" crates packages/app-web/src
```

### Formatting

- 先观察 Cargo/rust-analyzer 是否占用 build directory lock。
- 不盲跑 `cargo fmt --all`；若 reference checkout 缺失导致 workspace 解析失败，使用同一
  toolchain 对本任务 Rust 文件执行 `rustfmt --edition 2024 <files>`。
- TypeScript 使用项目既有 formatter/lint 对受影响文件定向执行。

## 最终验收场景

1. 新建 Managed AgentRun，发送首条消息：
   - 主回答正常结束；
   - 稍后列表与 workspace 从“新会话”变成总结名称；
   - Agent 身份标签仍独立显示。
2. 重启服务后重新打开：
   - 名称仍存在；
   - Native naming bridge 不再调用。
3. 设置显式 workspace title：
   - 展示显式 title；
   - 后续 Runtime name update 不覆盖它。
4. 清除标准 thread name：
   - 有显式 title 时展示不变；
   - 无显式 title 时回到“新会话”。
5. Codex AgentRun：
   - bind 初始 name 和 live rename 都走同一标准事件/reducer。
6. 故障注入：
   - naming LLM 失败不把成功 turn 标成失败；
   - stale generation 结果不污染新 binding；
   - project invalidation 失败不回滚 Runtime commit。

## 交付边界

- 完成实现、验证、规范更新后再进入 Trellis check。
- 用户未明确要求前不创建 commit。
- Commit 格式：

  ```text
  feat(agent-runtime): 统一Managed Agent会话名称事件与投影

  - 将Managed Agent与Codex名称结果统一为thread/name/updated
  - 建立Runtime可重放名称投影并接通AgentRun展示刷新
  - 删除Lifecycle自动标题复制与自有标题事件契约
  ```

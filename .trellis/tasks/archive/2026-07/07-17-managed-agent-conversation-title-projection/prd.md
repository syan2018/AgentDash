# 恢复 Managed Agent 会话标题投影链路

## Goal

恢复 Managed Agent 会话自动总结标题从 Agent 生成到 AgentRun 列表与工作台展示的完整链路，并把各层职责收敛为：

- Agent 拥有会话命名业务；
- Agent adapter 只把结果映射成标准协议通知；
- Managed Runtime 持久化标准通知并维护当前会话名投影；
- AgentRun application 只组合显式 workspace 标题与 Runtime 会话名；
- 前端只消费查询模型和通用投影失效通知。

唯一结果契约使用当前依赖版本 `codex-app-server-protocol` 已有的
`thread/name/updated` 通知：

```rust
ThreadNameUpdatedNotification {
    thread_id: String,
    thread_name: Option<String>,
}
```

不再维护 AgentDashboard 自有的会话标题事件 payload。

## Background

- 当前 Codex adapter 已接到 `thread/name/updated`，却把它改写成
  `PlatformEvent::SourceSessionTitleUpdated { title, preview, source, ... }`。
- 该自有事件没有 Managed Runtime reducer；标题虽进入 journal/live presentation，
  却没有进入 `RuntimeSnapshot` 当前态。
- `ProjectAgentRunListQuery` 和 workspace shell 最终从
  `LifecycleAgent.workspace_title` 回退到 Project Agent 名，因此界面展示的是 Agent
  身份名，而不是会话自动总结标题。
- `main` 曾通过 adapter 把 Runtime 标题复制写回
  `LifecycleAgent.workspace_title`。这会让自动标题和显式 workspace 命名共享两个可写
  事实源，本任务不恢复该设计。
- Managed Agent 当前尚未提供与 Codex 对齐的会话命名能力；这项业务应进入 Agent
  层，而不是 AgentRun application。

## Product Rules

1. `LifecycleAgent.workspace_title` 只表达用户或上层产品显式设置的 workspace 标题。
2. `RuntimeSnapshot.thread_name` 只表达 Agent conversation 的当前名字。
3. Project Agent 名是身份标签，只进入 `project_agent_label` 等身份字段，不是会话标题。
4. 展示标题优先级固定为：
   1. 非空显式 workspace 标题；
   2. 非空 Runtime `thread_name`；
   3. 中性的待命名文案“新会话”。
5. `ThreadNameUpdatedNotification.thread_name = None` 是清除当前会话名的标准语义，必须
   原样进入 journal 并把 Runtime 投影清为 `None`。
6. Agent 自动标题不得覆盖显式 workspace 标题；它只更新自己的 Runtime 会话名事实。

## Functional Requirements

### Standard protocol

1. `BackboneEvent` 直接承载 owned
   `codex_app_server_protocol::ThreadNameUpdatedNotification`。
2. Codex 实时通知与 bind/read 返回的 `thread.name` 都映射到同一个标准事件。
3. 删除 `PlatformEvent::SourceSessionTitleUpdated` 及其 `preview`、`source`、
   `executor_session_id` 标题语义，不提供兼容分支。
4. Driver ingress 校验通知中的 `threadId` 等于同一 envelope 的
   `source_thread_id`；不匹配的事件按协议违规处理。AgentRun 的 canonical runtime /
   presentation thread 坐标继续由 carrier 提供，不改写标准 payload。

### Managed Agent naming

5. 会话命名逻辑位于 `agentdash-agent`，通过已经解析好的 `LlmBridge` 发起隔离的无工具
   completion；AgentRun application、Runtime 与前端不得调用标题模型。
6. Native adapter 在第一个成功完成且具有有效用户/助手语义的 turn 后异步触发一次
   命名，不阻塞主 turn terminal。
7. 命名请求使用该 turn 的 canonical user input 与最终 assistant message，不修改 Agent
   正文消息历史，不携带工具定义。
8. 命名模块只返回规范化字符串，不定义第二套事件；Native adapter 将字符串包装成
   标准 `ThreadNameUpdatedNotification` 并作为 binding-level durable presentation
   发送。
9. 同一 Native thread 同时最多一个命名作业。Runtime 已有非空 `thread_name` 时不再
   生成；失败时保留 `None`，后续成功 turn 可以重新尝试。
10. 命名作业受 binding generation fence 约束。旧 generation 的迟到结果必须被 Runtime
    拒绝，不得覆盖新 binding 的当前态。

### Runtime projection

11. Managed Runtime 在提交标准事件时，把 immutable presentation record 与
    `RuntimeThreadState.thread_name` 在同一个 Runtime commit 中持久化。
12. `RuntimeSnapshot` 与 driver 恢复读取模型暴露当前 `thread_name`，不增加
    `source`/`preview` 等非标准派生字段。
13. Runtime replay 严格按 durable journal sequence 归约；最后一个已接受的
    `thread/name/updated` 决定当前值。
14. 相同值的重复通知在状态上幂等，不触发重复语义更新；`None` 清除也遵循相同规则。
15. 为现有 JSONB projection 增加显式数据库 migration，将既有 projection 补齐为
    `"thread_name": null`；同时清除 Lifecycle 中由旧自动链路写入且 source 为
    `auto/codex` 的 workspace title。该迁移不从旧自有标题事件回填开发数据。

### AgentRun projection and refresh

16. AgentRun list runtime summary 和 workspace runtime selection 从
    `RuntimeSnapshot.thread_name` 取得 conversation name，并通过同一个共享 resolver
    计算展示标题。
17. 标准名称事件 durable commit 后，AgentRun 投影通知器解析
    `runtime_thread_id -> run_id/agent_id/project_id`，发布现有
    `ControlPlaneProjectionChanged`：
    `projection=agent_run_list`、`reason=title_changed`。
18. 项目 AgentRun list store 收到该通用投影失效通知后重新查询第一页。
19. 已打开 workspace 收到标准 `thread_name_updated` presentation 后重新查询 workspace
    shell 和 AgentRun list；不得引入前端私有标题事件。
20. 标题清除与标题更新使用完全相同的刷新链路。

## Acceptance Criteria

- [ ] Managed Agent 的首个成功会话名通过标准 `thread/name/updated` 进入 Runtime，
  `RuntimeSnapshot.thread_name` 可见，主 turn terminal 不等待命名 completion。
- [ ] Codex 原生实时名称通知和 bind/read 初始名称通过同一个 Backbone variant、同一个
  Runtime reducer 与同一个 AgentRun resolver 生效。
- [ ] `threadName: null/None` 可清除 Runtime 当前会话名，并让展示回到显式 workspace
  标题或“新会话”。
- [ ] Runtime 重启、journal replay 和 driver rebind 后得到相同当前会话名；已有名字的
  Native thread 不会再次调用命名模型。
- [ ] stale binding generation 的迟到命名结果被拒绝；同一 thread 不并发生成多个名字。
- [ ] 相同名称重复事件不改变最终投影，不造成重复业务写入或可见列表抖动。
- [ ] 列表与 workspace 的标题优先级均为
  `workspace_title > runtime.thread_name > 新会话`，且由同一个 resolver 测试锁定。
- [ ] Project Agent 名只显示为身份信息，不再作为 conversation title fallback。
- [ ] 标题提交后，侧栏/AgentRun 列表和已打开 workspace 无需手动刷新即可看到新标题。
- [ ] 代码库中不存在 `SourceSessionTitleUpdated`，也不存在从 Runtime 自动标题写回
  `LifecycleAgent.workspace_title` 的 adapter。
- [ ] AgentRun application、Runtime 和前端中不存在标题 LLM 调用。
- [ ] Rust 定向测试、Runtime repository migration/roundtrip 测试、schema/TS 生成检查、
  前端定向测试与 type-check 通过。

## Out of Scope

- 改变 Codex 自身的标题生成算法或调用时机。
- 让 AgentRun application 再调用一次标题总结模型。
- 恢复旧 RuntimeSession、SessionMeta 或 workspace-title 自动复制链路。
- 为没有标准名称事件的历史本地开发会话补造标题。
- 在本任务中设计用户手动重命名 UI；既有显式 workspace 标题仍按原所有权工作。

## Confirmed Decisions

- canonical 通知固定为 `thread/name/updated` /
  `ThreadNameUpdatedNotification`。
- Runtime 投影只保存 `Option<String>`，不保存标题来源；来源由所有权边界决定，无需成为
  vendor payload。
- Agent 层提供命名能力，Native adapter 负责触发和标准协议映射；不新增
  `AgentEvent::TitleGenerated`。
- 直接删除自有标题事件并迁移 projection JSON，不提供旧事件解释器或回退链路。

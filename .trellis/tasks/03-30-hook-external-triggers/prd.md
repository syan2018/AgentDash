# Hook 引擎外部触发入口扩展

> 状态：待讨论（parking）
> 前置：`03-30-workflow-hook-simplification` 已完成后端+前端精简

## 背景

当前 Hook 引擎的 10 种 `HookTrigger` 全部是 **Agent 生命周期事件**（SessionStart → BeforeStop → SessionTerminal 等）。但实际场景中存在不由 Agent 自身生命周期触发的外部输入需求。

## 动机场景（待确认）

### 1. ExternalMessage — 跨 Agent/Session 消息回流
- 其它 Agent Session 完成后主动向当前 session 推送结果
- 人工在前端手动向 session 注入指令/上下文
- 外部系统（CI、webhook）向活跃 session 发送通知

### 2. StateChange — 业务实体状态变更
- Task 状态转移（如从 implementing → reviewing）触发注入
- Story 状态更新时通知关联的活跃 session
- WorkspaceBinding 状态变化（online/offline）影响执行策略
- Workflow LifecycleRun step 推进时触发注入

### 3. ConditionMet — 内置条件触发
- 定时器到期（如 session 运行超过 N 分钟）
- Artifact 数量达到阈值
- 检查通过/失败条件满足
- 资源消耗（token 用量）达到预警线

## 设计要点

### 共享规则引擎
外部触发**必须**与 Agent 生命周期触发共享同一套 `NormalizedHookRule` 引擎：
- 同样的 `trigger + matches + apply` 三元组
- 同样的 `HookEvaluationQuery` 入口
- 同样的 `HookResolution` 输出
- 区别仅在于 trigger 枚举值和 payload 结构

### 触发入口
外部系统通过 `HookSessionRuntimeAccess.evaluate()` 提交 query：
```rust
let query = HookEvaluationQuery {
    session_id: "sess-123".to_string(),
    trigger: HookTrigger::StateChange,  // 新增枚举值
    payload: Some(json!({
        "entity_type": "task",
        "entity_id": "...",
        "old_status": "implementing",
        "new_status": "reviewing",
    })),
    ..Default::default()
};
runtime.evaluate(query).await?;
```

### 精简原则
- 不为每种外部场景创建独立的 API/通道
- payload 用 `serde_json::Value` 保持灵活
- 规则引擎通过 `matches` 函数判断是否处理特定 payload

## 待讨论

- [ ] 三种外部触发类型的粒度是否合适？是否需要合并/拆分？
- [ ] 每种触发的 payload schema 具体是什么？
- [ ] 触发时机：推模型（外部主动 push）还是拉模型（runtime 轮询）？
- [ ] ExternalMessage 是否需要和现有的 SubagentResult 合并？
- [ ] ConditionMet 的条件注册和评估机制如何设计？
- [ ] 前端需要展示外部触发事件吗？trace 中如何呈现？

## 参考

- 本任务 PRD 原始讨论见 `03-30-workflow-hook-simplification/prd.md` 的 P7 章节
- Claude Code hook 系统参考见 `03-30-workflow-hook-simplification/ref-claude-code-hooks.md`
  - CC 的 `TeammateIdle`、`TaskCreated`、`TaskCompleted` 可作为跨 Agent 触发的参考
  - CC 的 `FileChanged`、`ConfigChange` 可作为状态变更触发的参考

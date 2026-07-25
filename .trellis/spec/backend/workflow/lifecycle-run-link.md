# Lifecycle Run / Runtime Link

`LifecycleRun` 与 `LifecycleAgent` 拥有产品身份；`AgentFrame` 拥有某 revision 的期望 surface；`AgentRunRuntimeBinding` 按 `run_id + agent_id` 指向 canonical Runtime thread 与 Host binding。

Runtime link 的原因是业务归属、权限与编排坐标不能进入 Driver protocol，同时 Runtime lifecycle 也不能由产品状态反推。

```text
LifecycleRun + LifecycleAgent + current AgentFrame
  -> AgentRunRuntimeTarget
  -> AgentRunRuntimeBinding(thread_id, binding_id)
  -> canonical Runtime snapshot/events/operation
```

- Workflow/Task/Story evidence 保存 thread/operation typed refs。
- Frame revision 变化通过 Business Surface 重新编译/绑定，不改写历史 Runtime operation。
- 删除 LifecycleRun 依赖产品 FK/cascade 清理 binding；Runtime terminal/read side不依赖已删除的产品 pointer。
- authorization 从 run/project/agent ownership开始，随后才允许 facade 读取 Runtime。

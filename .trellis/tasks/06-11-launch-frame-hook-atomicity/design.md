# 设计

## Launch Boundary

当前 `TurnCommitter` 已在 connector accepted 后提交 user input 和 turn started；需要移除 orchestrator 中额外的早期 frame 写入。

目标：

- LaunchPlanner 输出 pending capability/execution/context projection。
- TurnPreparer 使用 pending projection 构造 connector context。
- ConnectorStarter accepted 后进入 TurnCommitter。
- TurnCommitter 或相邻 commit service 持久化 AgentFrame revision/current frame。

如果现有 frame construction pipeline 已有 accepted frame compose 能力，优先复用它。

## Failure Path

在 plan/prepare/start 任一阶段失败：

- clear claimed turn/hook runtime。
- persist terminal failure where appropriate。
- leave current frame unchanged。
- leave command receipt pending/failed according to command child contract。

## HookRuntime Cache

新增 registry operation：

```text
set_or_replace_hook_runtime(session_id, runtime)
```

`ensure_hook_runtime_for_delivery_session` 流程：

1. resolve current HookControlTarget。
2. read cached runtime。
3. if cached target matches, refresh provenance and return。
4. if cached target differs, load frame snapshot and replace runtime。
5. return rebuilt runtime。

`SessionHookService::ensure_hook_runtime_for_target` 继续做最终校验。

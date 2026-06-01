# 执行计划

## 顺序

1. 盘点当前 frame 输入：
   - `StepActivationInput` / `StepActivation`
   - `SessionConstructionPlan`
   - `LaunchPlan`
   - `HookSessionRuntime`
   - `CapabilityState`
   - context / VFS / MCP projection
2. 引入 `AgentFrameBuilder` 与内部 `AgentFrameConstructionPlan`。
3. 将 `StepActivation` 输出改为 frame delta / revision source。
4. 将 `SessionConstructionPlan` 降为 builder 内部结构，不再作为业务 command input。
5. 将 connector launch 改为 `AgentFrame -> RuntimeLaunchRequest -> ExecutionContext`。
6. 将 pending capability/context transitions 写入 frame revision 或 frame event。
7. 将 Hook runtime API 改为 agent/frame scoped；session scoped API 只保留 trace adapter。
8. 更新调用方，保证 business modules 不 import construction / launch / connector internals。

## 质量门

- 一个 frame revision 能回答 procedure、tools、MCP、VFS、context、runtime refs。
- RuntimeSession launch 可以从 frame data 重建，不需要从 session 反查 business owner。
- Hook advance/resolution 接收 agent/frame/assignment refs，不把 session 当 owner。
- connector execution 仍能拿到 `ExecutionContext`，但只能由 frame projection 生成。
- `AgentFrame` 满足父任务 `concept-boundaries.md` 的 ownership 与 corruption checks。

## 断裂点

迁移中 connector launch、hook runtime panel、capability transition 可能短期丢字段。允许中间不可用，但不能恢复 `SessionMeta`、live maps 或 hook runtime 作为 parallel truth。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-agent-frame-construction-migration`
- frame builder 单元测试。
- connector launch adapter 测试。
- hook runtime resolution 测试。
- `rg -n "SessionConstructionPlan|LaunchPlan|HookSessionRuntime|CapabilityState" crates/agentdash-application crates/agentdash-domain crates/agentdash-contracts`
- `git diff --check -- .trellis/tasks`

## 后续交接

- `workflow-agent-assignment-migration` 使用 `AgentFrame` refs 完成 scheduler/terminal routing。
- `companion-gate-lineage-migration` 使用 frame context slice / capability surface 表达 companion inheritance。
- `session-first-api-demotion` 删除旧 construction / hook / session owner DTO 的 public 暴露。

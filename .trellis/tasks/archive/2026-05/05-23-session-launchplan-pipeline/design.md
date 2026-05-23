# Session LaunchPlan 阶段化 Design

## Boundary

本任务收敛 session launch 主链路。它需要兼容当前 session runtime、Backbone event、VFS、capability、hook 与 relay 入口，但第一阶段不要求拆 crate。

## Target Pipeline

```text
LaunchCommand
  -> LaunchPlanner
  -> LaunchPlan
  -> LaunchExecutor
  -> TurnSupervisor
  -> EventIngestion
```

阶段职责：

| 阶段 | 职责 |
| --- | --- |
| `LaunchCommand` | 表达入口意图与用户输入，不携带半成品 runtime 状态 |
| `LaunchPlanner` | 读取 owner/project/story/task/session facts，生成不可变 plan |
| `LaunchPlan` | 固化 owner lifecycle、context、VFS、MCP、capability、hook、commit policy |
| `LaunchExecutor` | claim turn、构造 ExecutionContext、调用 connector |
| `TurnSupervisor` | 管理 adapter task、processor task、cancel、terminal |
| `EventIngestion` | 将 ExecutionStream 转换为 BackboneEnvelope/persistence/eventing |

## Commit Policy

bootstrap 成功、turn started、connector accepted 等必须有显式 policy。默认建议 connector prompt accepted 后再提交 bootstrap 成功，避免失败时 meta 已提前进入 bootstrapped/running。

## Migration Strategy

先建立新阶段结构和 facade，然后让 HTTP prompt 与 local relay prompt 进入新路径；Task、Workflow、Routine、Companion 逐步改为薄适配器。

## Spec Update

更新 `session-startup-pipeline.md`，把 startup 主线从函数名描述改为阶段契约描述。

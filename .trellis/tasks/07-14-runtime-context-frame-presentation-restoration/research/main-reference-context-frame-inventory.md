# main-reference ContextFrame 搬运清单

## 固定参考源

```text
repository: D:/Projects/AgentDash-main-reference
branch:     main
commit:     957fa9d60ea3d67efa1bb278fe5b376cf0c34598
policy:     read-only behavior oracle
```

所有实现与 review 必须用 `git -C D:/Projects/AgentDash-main-reference` 确认该 commit；若参考目录 HEAD 改变，先记录并重新审核 golden 来源，不能静默跟随。

## 核心类型与公共规则

| 规则 | 参考文件 |
| --- | --- |
| ContextFrame/delivery/section 类型 | `crates/agentdash-spi/src/hooks/mod.rs` |
| 公共 builder、timestamp/ID、notice enqueue | `crates/agentdash-application-runtime-session/src/session/context_frame.rs` |
| turn 内排序、去重、delivery plan/record | `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs` |
| durable event wrapper/persistence | `crates/agentdash-application-runtime-session/src/session/eventing.rs` |

## Frame family builders

| Family | 参考文件 | 新归属 |
| --- | --- | --- |
| identity | `session/identity_context_frame.rs` | Runtime context_projection + Application identity facts |
| user_context | `session/user_context_frame.rs` | Runtime context_projection + Application user facts |
| environment | `session/environment_context_frame.rs` | Runtime context_projection + workspace facts |
| system_guidelines | `session/guidelines_context_frame.rs` | Runtime context_projection + guideline facts |
| memory_context | `session/memory_context_frame.rs` | Runtime context_projection + memory facts |
| assignment_context | `session/assignment_context_frame.rs` | Runtime context_projection + workflow/hook injection facts |
| capability/tool/skill/VFS/MCP/companion delta | `session/hub/runtime_context_transition.rs` 与 `session/dimension/*` | Runtime normalized surface delta |
| pending_action | `session/pending_action_context_frame.rs` | Runtime HookRun/mailbox projection |
| auto_resume/system notice | hook notice builders + launch preparation | Runtime HookRun/mailbox projection |
| compaction_summary | `session/compaction_context_frame.rs` 与 `session/eventing.rs` | Runtime context activation projection |

## 触发路径

| 场景 | 参考路径 | 新 canonical operation |
| --- | --- | --- |
| 初始启动 | `session/launch/preparation.rs` → `launch/commit.rs` | ThreadStart |
| live surface transition | `session/hub/runtime_context_transition.rs` | SurfaceAdopt |
| queued hook/pending frame | `session/launch/preparation.rs` | HookRun effect / next TurnStart |
| compaction | `session/eventing.rs` | Context activation/head commit |

## 搬运规则

1. 先为参考 builder 建立 fixture/golden，再写新 projector。
2. payload 字段、section 内容与顺序保持一致；wrapper 可改为 typed PlatformEvent。
3. 不搬运 `SessionRuntimeInner`、live connector、进程内 queue 或 repository side effects。
4. builder 内 `Utc::now()` 改为显式 operation clock；magic string 改为 typed enum，序列化值保持一致。
5. 多点 delta 计算收敛为一次 normalized `ContextSurfaceDelta`。
6. 每个 golden 记录参考文件、输入 fixture 与允许 normalization 的动态字段。

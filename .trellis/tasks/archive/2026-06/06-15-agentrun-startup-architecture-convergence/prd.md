# AgentRun 启动主线架构收束

## Goal

重新收束 ProjectAgent 启动、AgentRun Mailbox、Session Launch、Runtime Tool Composer 与 AgentRuntimeDelegate 的主线模型，让会话启动只有一条明确的状态机、一组 accepted 边界和一份可恢复的 durable 事实。

本任务的目标不是围绕 `tokio-rt-worker has overflowed its stack` 追加局部补丁，而是在修正模型的过程中自然消除该类启动瞬间崩溃、半成品 frame、pending receipt 与 consuming mailbox 残留问题。

## User Value

- 新会话启动、首条用户消息、后续 composer 输入、hook/system follow-up 走同一套 AgentRun intake 和恢复模型。
- 启动链路出现失败时，数据库状态能被系统解释和恢复，而不是留下需要人工判断的半成品。
- 后续扩展 workspace module、extension action、multi-agent、hook follow-up 时，不再继续叠加旧 Session prompt 流程的包装层。
- 清理完成标准可被 review，不再因为残留旧路径而继续支付长期维护成本。

## Confirmed Facts

- `2026-06-14T16:01:13Z` 对应本地 `2026-06-15 00:01:13 +0800`；事故现场是后端 Rust 进程栈溢出退出，前端 WebSocket `10054` 是后果。
- 失败现场中 `project_agent_start` receipt 和首条 `agent_run_message` receipt 均保持 `pending`，mailbox message 已进入 `consuming`，LifecycleRun/LifecycleAgent/RuntimeSession 已创建。
- 失败现场只写入一个空 AgentFrame，未写入最终 capability/context/vfs/module surface frame，说明崩溃发生在控制面初始物化之后、最终 launch surface/turn commit 之前。
- `d7a11421 refactor(agentrun): 收敛 mailbox-first 消息投递` 把 ProjectAgent start 改为外层 start 同步等待内层 mailbox/session launch 完成，是当前半成品状态的主要结构原因。
- `4d5ab4d2 refactor(runtime): 收束本机运行时装配与扩展契约` 改动了 runtime tool composer 和 runtime tool provider 注入面，是 connector accepted 前工具/schema/context 组装阶段的高风险触发点。
- 数据库当前无 `mcp_presets` 和 `project_extension_installations`，因此这次事故不应优先假设为外部 MCP preset 或 extension action schema 直接触发。
- 现有 `.trellis/spec/backend/session/session-startup-pipeline.md` 已定义目标主线：`LaunchCommand -> FrameLaunchEnvelope -> LaunchPlan -> PreparedTurn -> ConnectorAcceptedTurn -> CommittedTurn -> AttachedTurn`。当前实现的问题是 ProjectAgent/mailbox 启动在该主线外部又套了一层同步编排。

## Requirements

- 本任务作为一个大任务执行，不拆 child task；内部按 phase 切分，并按 phase 产出阶段性提交。
- 每个 phase 必须有清晰的代码边界、可验证结果和 commit 边界，避免一次性混改到无法 review。
- 任务文档必须明确旧模型与新模型的对比，后续实现与 review 以删除旧模型路径为目标，不允许只在旧模型上再包一层。
- ProjectAgent start 必须降级为 AgentRun workspace 的 thread/envelope 创建入口，不再作为同步包住内层 SessionLaunch 的第二套 launch use case。
- AgentRun Mailbox 必须成为用户输入、hook/system delivery、follow-up、queued work 的唯一 durable intake 和 scheduler 事实源。
- Session Launch 必须回到“给定明确 LaunchCommand 和 launch-ready FrameLaunchEnvelope，启动一个 runtime turn”的职责，不再承担 ProjectAgent 业务控制面补齐。
- Frame construction 必须在进入 launch 前产出 launch-ready `FrameLaunchEnvelope`，包含 working directory、VFS、MCP、capability、context、identity、resolution trace 和 executor profile。
- AgentFrame / FrameLaunchEnvelope / CapabilityState / runtime tools 的事实源必须单向投影，不允许在 `TurnPreparer` 或 tool composer 中再补齐核心控制面事实。
- Runtime Tool Composer 必须拆分工具声明与工具调用。provider-visible tool schema 构建阶段不得触发复杂 runtime 查询、session launch、gateway invoke 或控制面递归。
- AgentRuntimeDelegate 的 mailbox、hook、context transform、stop policy、audit/injection 职责需要拆出明确阶段或窄接口，避免组合器因为缺少 inner delegate 改写 provider-visible input。
- accepted 边界必须分层定义：command receipt accepted、mailbox delivery accepted、session turn accepted、frame/bootstrap accepted 各自表达不同事实，不再互相隐式替代。
- 崩溃/重启恢复必须覆盖 `pending receipt + consuming mailbox + empty bootstrap frame` 等启动中断状态，恢复结果要明确进入 blocked/failed/retryable/unknown delivery 中的一种。
- 必须设置一个专门的旧模型清理 phase，清理旧 ProjectAgent 同步启动、route-local launch 推断、frame/preparer 补事实、tool declaration side effect、delegate 黑盒组合等残留路径。
- 旧模型清理 phase 完成后必须触发 `trellis-check` 子代理做专项 review gate；该 gate 通过前不得进入最终规格更新和归档。
- 任务讨论和设计应优先描述目标模型为什么成立，不围绕旧错误状态设计兼容层或长期 fallback。

## Out of Scope

- 不为当前错误状态设计长期兼容 API。
- 不保留旧 ProjectAgent start 同步 launch 语义作为备用路径。
- 不把本任务拆成“先补栈溢出，再做架构”的两段长期并行路线；必要的止血只能服务于模型收束。
- 不在第一轮同时重写所有 VFS mount、Tauri profile/claim 或完整 Extension Host 运行模型，除非它们直接阻塞启动主线收束。
- 不在设计文档里记录“不要做什么”的历史清单；只记录目标职责、原因和验收边界。

## Acceptance Criteria

- [ ] `ProjectAgent start` 的返回语义不再依赖内层 `SessionLaunchService` 完整执行成功；它返回 AgentRun thread/envelope/scheduler 的 durable accepted 或 blocked 投影。
- [ ] 首条用户消息和后续 composer submit 使用同一套 mailbox command、scheduler outcome、accepted refs 和恢复逻辑。
- [ ] 启动新 AgentRun 时不会留下无法解释的 `pending project_agent_start + consuming mailbox + empty frame` 状态；恢复器或 scheduler 能给出确定投影。
- [ ] `FrameLaunchEnvelope` 在 launch 前完成核心 surface 校验，`TurnPreparer` 不再负责补齐 VFS/MCP/capability/executor 等事实。
- [ ] Runtime tool provider 的 tool schema 构建可被单测覆盖为无 runtime side effect，复杂 invocation 只发生在 tool call 阶段。
- [ ] Agent loop 的 context transform / stop continuation / mailbox boundary / hook injection 有明确阶段语义，缺失 inner delegate 不会清空用户输入。
- [ ] 旧模型清理 phase 完成，代码中不再保留 ProjectAgent start 同步等待 initial mailbox/session launch 的业务路径，也不再保留用于推断旧语义的 route-local 分支。
- [ ] `trellis-check` 以“旧模型残留专项审计”为目标完成 review，审计至少覆盖 source search、关键启动链路、tests/contracts、frontend projection 和 specs。
- [ ] 栈溢出事故路径有 focused regression 覆盖：ProjectAgent 首轮启动能走完整 accepted/commit/attach，或在构造失败时进入可恢复状态。
- [ ] 相关 `.trellis/spec/backend/session/*`、`backend/capability/*` 或 `backend/runtime-gateway.md` 被更新为新的稳定契约。
- [ ] 后端 targeted tests、contract check、必要 frontend type/check 通过；测试范围按大改影响面选择，不做无意义全量拖延。
- [ ] 最终验收必须使用 `pnpm dev` 启动完整项目链路，并通过前端开启一个新会话，连续发送两轮用户消息，确认会话启动、首轮响应、第二轮响应和 WebSocket/后端进程均保持可用。

## Planning Decision

- 本任务不拆 child task。采用单个 Trellis 任务内分 phase、分阶段提交、阶段性 check gate 的方式推进。
- 最优路径是先把 AgentRun intake/accepted 边界改正确，再收束 launch-ready frame closure，再拆 runtime tools/delegate，最后用独立 cleanup phase 删除旧模型残留。

# Hook runtime ownership 模型收敛

## Goal

将 Hook runtime 的业务归属收敛到 AgentRun / AgentFrame control target，让 RuntimeSession 只承担执行器连接通道、事件流归属和 trace/debug 记录职责。

## Requirements

- Hook runtime 的权威身份使用 `HookControlTarget { run_id, agent_id, frame_id }` 表达，并以 AgentFrame revision 作为 hook policy、capability、context、VFS、MCP 的生效边界。
- RuntimeSession 相关字段只表达执行器会话、turn、事件流、connector continuation 和运行记录来源；session 维度缓存需要能够从 AgentRun / AgentFrame control target 校验或重建。
- AgentFrame revision 推进、runtime capability transition、workspace module / canvas 动态授权、companion/gate 回流等路径共享同一套 hook target 解析和同步机制。
- AgentRun Workspace action projection、hook runtime diagnostics、runtime trace/detail 三类视图保持分层：工作台控制面读取 AgentRun projection，trace/debug 读取 RuntimeSession projection，hook 卡片读取 AgentFrame hook snapshot。
- AgentRun Workspace 输入区在 projection refresh / command in-flight / turn terminal 重拉期间不得执行上一帧 actions；上一帧 workspace 只能作为展示连续性使用，不能作为新的 command authority。
- runtime capability context frame 的增量展示以 `CapabilityState before/after` 为权威基线；Hook runtime 内部 capability cache 只负责同步执行期视图，不能反向决定用户可见 diff。
- `context_frame` 属于会改变 Agent 可见上下文的运行期事件，展示上必须截断工具调用聚合单元；连续 context frame 仍可彼此聚合，但不能让前后工具调用跨过 CTX 合成一张工具 burst。
- Workspace Panel 的 Canvas 链接必须来自后端 canonical workspace module presentation / frame projection；`canvas://` 只能作为未绑定状态，不得在已有 Canvas 关联时成为实际 tab URI。
- Workspace Panel 必须支持多个 Canvas tab，每个 tab 绑定一个具体 `canvas://{mount_id}`；用户可以不经过 Agent 工具调用，直接从可选 Canvas 列表中选择并拉起对应 Canvas。
- 用户主动拉起 Canvas 与 Agent 通过 `workspace_module_create` / `workspace_module_present` 拉起 Canvas 共享同一套 session exposure、presentation URI 和 capability synchronization 语义。
- 后续实现需要保留现有用户可见语义：AgentRun 可继续下一轮、运行中可 steer / enqueue / cancel、Canvas 创建和展示不会因 frame revision 切换产生 target mismatch。
- 迁移期间允许调整数据库 schema、DTO 和内部 API，目标是把模型收敛到当前最正确形态。

## Acceptance Criteria

- [ ] 代码中 hook runtime 的业务 owner 可以从接口、类型和缓存 key 上明确看出是 AgentRun / AgentFrame control target。
- [ ] RuntimeSession 到 hook runtime 的映射只作为执行器会话到当前 HookControlTarget 的绑定查找，并在 frame revision 变化时通过统一路径同步或失效。
- [ ] 创建 Canvas、workspace module present、runtime capability hot update、后续消息、steer、companion/gate 回流都不会出现 Hook runtime target mismatch。
- [ ] AgentRun Workspace 输入区 action state 在 turn start / terminal / command accepted 后以 `AgentRunWorkspaceView.actions` 为准刷新。
- [ ] `refreshing` / `error` / stale projection 状态下，Enter 和 Ctrl/Cmd+Enter 不会执行 enqueue、steer 或 send_next；刷新完成后才按最新 `AgentRunWorkspaceView.actions` 执行动作。
- [ ] Canvas create / present 触发 capability update 时，Capability Keys 只显示真实增量；新增 Canvas VFS mount / Skill / workspace module 可见性按各自维度展示，不把已有基础能力全量标为新增。
- [ ] AgentRun start 时如果当前 AgentFrame 已有关联 Canvas，Workspace Panel 可以从 workspace projection 打开真实 `canvas://{mount_id}`，不会停留在空 `canvas://` tab。
- [ ] `workspace_module_create(kind=canvas)` 成功后会同步出同一份 Canvas presentation 事实，右栏可立即打开或激活真实 Canvas tab；`workspace_module_present` 仍使用同一 canonical presentation。
- [ ] Workspace Panel 的 Canvas 入口可以列出当前 Project / 当前 session 可打开的多个 Canvas，用户选择后打开对应 `canvas://{mount_id}` tab。
- [ ] 用户主动打开一个尚未暴露到当前 session 的 Canvas 时，后端会先执行同源的 Canvas exposure / workspace module grant / capability transition，再返回 canonical presentation；不会只在前端打开一个无法使用的 tab。
- [ ] 多个 Canvas 可并存为多个 tab；重复选择同一 Canvas 激活已有 tab，不制造重复空壳 tab。
- [ ] Session feed 中的 capability / assignment context frame 会截断工具调用聚合；context frame 不再浮在跨工具 burst 的外侧。
- [ ] 后端有回归测试覆盖 frame revision 切换后的 hook target 同步、Canvas 创建、后续消息、steer 和 companion/gate 关键路径。
- [ ] 前端有 focused test 或 typecheck 覆盖 turn terminal 后 action projection 刷新、非 running 态 Ctrl/Cmd+Enter 不触发 steer、context frame 截断工具聚合、Canvas tab 只接受真实 `canvas://{mount_id}` presentation、用户主动选择多个 Canvas。
- [ ] 相关 spec 更新，记录 Hook runtime ownership、RuntimeSession 运行记录归属、AgentRun Workspace projection 的正向模型。

## Architecture Cutover Gate

最终验收必须明确回答：旧的 RuntimeSession-first 状态模型是否已经被釜底抽薪，系统是否已经完整迁移到目标架构。该 Gate 不以“复现 case 不报错”为充分条件。

- [ ] Hook runtime 创建、读取、刷新、失效入口不再以裸 `RuntimeSession` 作为业务 owner；所有业务路径都能追溯到 `HookControlTarget { run_id, agent_id, frame_id }`。
- [ ] `runtime_session_id -> HookControlTarget` 只作为执行器会话绑定；绑定失效、frame revision 推进、lazy rebuild 时不会形成第二套事实源。
- [ ] AgentFrame revision 是 capability/context/VFS/MCP/workspace module visibility 的唯一业务事实源；runtime cache、connector tool surface、hook snapshot 都能从当前 frame 校验或重建。
- [ ] Runtime capability transition 的用户可见 diff、context frame、workspace presentation 都从同一份 `CapabilityState before/after` 与 frame transition 派生。
- [ ] AgentRun Workspace 输入控制只读取当前 ready projection；代码中不存在 refreshing/stale/error 时继续执行上一帧 actions 的路径。
- [ ] Workspace Panel 的 Canvas presentation 不依赖前端默认 URI 或本地猜测；Agent create/present、用户 open、start restore 共享同一套后端 exposure + presentation contract。
- [ ] 最终 review 需要列出已删除/替换的旧路径、保留的 RuntimeSession debug/trace 路径、以及每个保留路径为什么不是业务 owner。

## Constraints

- 项目处于预研期，可以进行 schema、DTO、内部 API 和持久化结构调整，以模型正确性优先。
- 文档描述以目标模型和设计原因表达，避免记录一次性错误实现细节。
- 验证至少包含 `cargo test -p agentdash-application` 的相关 focused tests、`pnpm --filter app-web run typecheck`，涉及 contract 或 migration 时补充 `pnpm run contracts:check` / `pnpm run migration:guard`。

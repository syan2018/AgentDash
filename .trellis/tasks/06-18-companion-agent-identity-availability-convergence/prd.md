# Companion Agent 身份与可用模型收束

## Goal

把 `companion_request(payload.agent_key)` 从“选择某个 ProjectAgent 的 executor config 后裸启动 child subagent”收束为“选择一个可用 companion ProjectAgent，并按该 ProjectAgent 的身份、能力和上下文契约启动 child companion session”。

同时优化可用协作 Agent 模型：ProjectAgentConfig 增加“是否默认可作为协作 Agent”的目标侧字段；调用侧手动添加协作 Agent 时，只展示并保存那些没有默认开放、但希望被当前 Agent 额外调用的 Agents。

## User Value

- Agent 看到的 `## Companion Agents` 列表、`companion_request` 的工具校验和实际 child session 身份保持同源。
- 协作 Agent 配置从“每个 caller 手工维护白名单，空集代表全部”变成“目标 Agent 声明默认可协作，caller 只补充非默认例外”，配置成本更低。
- child companion 的行为更符合用户直觉：调用 reviewer 就得到 reviewer 的 preset / capability / skill / VFS / executor 语义，而不是只有 reviewer 的 executor config。

## Confirmed Facts

- 当前 roster 事实源是 `CapabilityState.companion.agents`，owner bootstrap 会把它渲染为 `companion_agents` context fragment。
- `CompanionRequestTool` 构造时已经接收 `flow.companion.agents.clone()`，`payload.agent_key` 会先在当前 frame roster 中匹配，查不到会拒绝调用。
- 当前 `agent_key` 匹配成功后只读取目标 ProjectAgent preset 并转换为 `companion_executor_config`。
- 当前 child 派发使用 `AgentPolicy::SpawnChild` + `ExecutionSource::ParentAgent`，创建出的 `LifecycleAgent.project_agent_id` 为空。
- 当前 companion frame construction 通过 `LaunchCommand::CompanionDispatch` 强制走 `composer_companion`，child capability 来自 `CapabilityResolver::resolve_companion_caps(slice_mode)`，不是目标 ProjectAgent 的完整 resolver 输出。
- 当前 `AgentPresetConfig.allowed_companions` 是 caller 侧白名单：非空表示仅允许这些 companion，空/缺省表示全部可用。

## Requirements

- `ProjectAgentConfig` / `AgentPresetConfig` 增加目标侧布尔字段，用于声明该 Agent 是否默认进入其它 Agent 的 companion roster。
- caller 侧 companion 手动配置改为“额外允许的非默认 companion agents”，不再使用“空集 = 全部、非空 = 白名单”的旧语义。
- companion roster 生成时应包含：
  - 所有默认可作为协作 Agent 的同项目 Agents；
  - 当前 caller 手动额外添加的非默认 Agents；
  - 不包含 caller 自身。
- ProjectAgent 配置界面应把默认可协作状态作为目标 Agent 自身属性展示/编辑。
- caller 的 companion 配置界面只列出“非默认可协作”的其它 Agents，用于额外加入当前 caller 的 roster。
- 前端需要同步适配 ProjectAgent config 类型、编辑表单、Agent 卡片提示、companion picker 文案和状态展示；UI 不再表达“白名单模式：清空即全部可用”。
- 前端/会话展示应能体现 selected companion child 的 ProjectAgent 身份；用户不应只看到匿名 spawned subagent。
- `companion_request(payload.agent_key)` 匹配成功后必须解析出 selected ProjectAgent identity，而不只是 executor config。
- child companion runtime 应保留 parent-child lineage 和 companion result 回流语义。
- child `LifecycleAgent` / frame construction 应能表达 selected ProjectAgent 身份，使后续查询、UI 展示和 frame bootstrap 可以把 child 识别为对应 companion ProjectAgent。
- selected companion 的 executor config、capability directives、skills、VFS grants、workspace module visibility 等 ProjectAgent preset facts 应参与 child launch 组装。
- 现有 roster context fragment、runtime delta section、tool error message 继续使用 canonical `agent_key`，并与新的 roster 生成规则同源。
- 不设计兼容双轨；本项目未上线，允许直接迁移旧字段语义和必要数据库/seed/mock 数据。
- 不提供 caller 侧排除默认 companion 的 denylist；需要禁止协作时应从不可提权的内嵌工具护栏控制是否允许发起 subagent/companion 调用。
- companion 发起操作面与 companion 回流通道需要分离建模；禁止发起 subagent/companion 调用不应导致已经启动的 child companion 无法调用 `companion_respond` 返回结果。
- companion agent run 默认禁用 human 路由，避免 child companion 在后台协作时直接打扰用户；只有用户主动向该 companion agent run 发送消息/进入该 run 的交互上下文后，Authority 才可打开 human route。
- 区分工具模型、上下文投影与 runtime operation surface：工具是否存在、启动上下文是否提示某类操作、某个动作是否被当前 runtime role/topology 允许，是三层不同问题；本任务不应仅通过新增/拆分工具 capability key 或工具执行时拦截来表达所有护栏语义。
- 内嵌工具护栏不是 PermissionGrant 体系，不通过用户审批或 capability grant 提权；例如 dynamic workflow 这类能力应可声明“仅主 Agent 可用”，child/subagent 即便看见相关工具也不能绕过该 runtime guard。
- 对不具备 subagent/companion dispatch 操作面的 Agent，owner bootstrap 不应注入 `## Companion Agents` roster；模型不应在启动信息里看到不可用的 companion candidates。
- 规划一套通用 Authority Model，用同一组概念解释 tool exposure、context affordance、runtime operation、requestable grant 和 non-escalatable platform invariant；companion 收束应适配该模型，而不是继续按局部工具名补丁。
- `CapabilityState` 是 `AuthorityState` 的下游投影；Authority 可以直接裁剪 capability / context / UI affordance，例如 workspace module 展示能力原则上只对用户唤起的主线程 ProjectAgent 生效，subagent 身份应在 Authority 层被卡掉。

## Acceptance Criteria

- [x] 新增 ProjectAgentConfig 字段能在 Rust domain、API contract、前端类型、编辑表单中完整往返。
- [x] roster 生成规则改为“默认开放 + caller 额外添加非默认”，并有单元测试覆盖默认开放、额外添加、自身排除、重复去重。
- [x] caller 手动添加 companion 的 UI 不再展示默认开放 Agents，只展示非默认 Agents。
- [x] 前端 ProjectAgent 配置 UI、Agent 卡片提示、companion picker 文案不再使用旧白名单语义。
- [x] 前端 AgentRun / companion 派发展示能呈现 selected ProjectAgent 身份，避免把 companion child 展示成无来源的匿名 subagent。
- [x] `companion_request` 使用 `agent_key` 解析 selected ProjectAgent identity，并在 launch source / dispatch result / frame construction 中保留该身份。
- [x] child companion 的 `LifecycleAgent` 或等价 runtime identity surface 能关联 selected ProjectAgent，而不是仅作为匿名 spawned child。
- [x] child companion 的 frame construction 消费 selected ProjectAgent preset facts，不再只消费 selected executor config。
- [x] 模型上下文中的 `## Companion Agents`、工具参数校验和实际 child launch 使用同一 roster 数据源。
- [x] companion 操作面设计明确区分 tool exposure、context projection 与 non-escalatable runtime guard；禁止发起协作时不注入 companion roster，且不影响 child result return channel。
- [ ] companion child 默认没有 human route；用户主动向 companion run 发送消息后，human route 可按 Authority 状态打开。
- [x] `design.md` 给出通用 Authority Model，并说明 companion dispatch、companion respond、dynamic workflow authoring 如何落入该模型。
- [x] `design.md` 明确 `AuthorityState -> CapabilityState` 的数据方向，并说明 workspace module 展示这类身份约束如何由 Authority 裁剪下游 capability。
- [x] 更新相关 spec，记录 companion roster 与 identity launch 的新事实源和数据流原因。
- [x] 通过聚焦测试验证 Rust companion/tool/capability 链路与前端 ProjectAgent 配置链路。

## Implementation Status Notes

- 已落地：companion child 默认拒绝 `target=human`，并要求通过 `companion_respond` 回流父会话。
- 尚未落地：用户主动向 companion run 发送消息后打开 human route。当前 `ExecutionContext` 未携带 `LaunchSource::LifecycleAgentUserMessage` 或等价 turn provenance，后续应先把该 provenance 投到 authority 输入，再在 `human.ask` guard 中打开该 route。
- 已落地：`AuthorityState::companion_child()` 裁剪 `workspace_module.present`、`dynamic_workflow.author`、`companion.dispatch`，同时保留 `companion.respond` return channel。

## Out Of Scope

- 不改变 `companion_respond` 的结果 payload 协议。
- 不改变 durable gate / wait 机制的业务语义。
- 不引入跨 Project companion 调用。
- 不保留旧 `allowed_companions` 白名单语义作为运行时 fallback。
- 不提供 caller 侧 denylist 来局部排除默认 companion。

## Route Policy Decisions

- subagent/companion dispatch 被禁用时，保留 `companion_request` 的非 sub 路由；`target=sub` 由 `companion.dispatch` 控制。
- companion agent run 默认禁用 `target=human`；用户主动向 companion run 发送消息/进入该 run 的交互上下文后，Authority 可打开 human route。

## Original Need Checklist

- 修复原始 companion 派发语义：`agent_key` 不再只是 executor config selector，而是 selected ProjectAgent identity selector。
- 修复模型上下文语义：不可 dispatch companion 的 Agent 不看到 companion roster。
- 修复协作 Agent 可用模型：目标 Agent 声明默认可协作，caller 只额外添加非默认 Agents。
- 修复 child 回流语义：child 可以 `companion_respond`，但不默认继承 dispatch 新 child 的能力。
- 建立通用 Authority 上游模型：CapabilityState、context affordance、tool exposure、UI 展示都是下游投影。
- 前端同步迁移：类型、表单、文案、卡片、companion child 身份展示都要改到新语义。

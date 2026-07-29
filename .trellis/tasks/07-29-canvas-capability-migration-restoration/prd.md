# 完整迁回 Canvas 产品能力

## Goal

在不恢复旧 Canvas aggregate、旧 RuntimeSession ownership 或兼容 API 的前提下，以
2026-07-09 reference worktree 为完整能力基线，把 Canvas 创作、运行、交互、资源、诊断与
Agent 回流能力原样迁移到
`InteractionDefinition + InteractionInstance + Attachment + OperationGateway + AgentRun`
新底层，使 Canvas 同时成为用户可脱离 Agent 会话独立使用、Agent 可创建编辑并向用户交付的
核心产品能力。

## Background

- 2026-07-10 的 Canvas→Interaction hard cut 删除了旧 Canvas domain/application/repository、
  `canvas_fs`、runtime snapshot/state、Workspace Module Canvas bridge、前端 Canvas runtime SDK
  与 `canvas-system` Skill。
- 当前新底层已经具备 Interaction definition revision、immutable SourceBundle、instance、
  attachment、canonical state revision、command/event、component binding、OperationScript 和
  Workspace Module definition/runtime projection。
- 当前普通 Canvas iframe 仅使用 `srcDoc + sandbox="allow-scripts"`，没有注入 Canvas host SDK。
  旧 `window.agentdash.invoke/assets/interaction/agent.submit` 均不存在。
- 当前 Agent Workspace Module 工具只有 list/describe/invoke/present；没有 Canvas definition
  create/copy/source edit、resource binding、render inspect 或 submit-to-Agent 能力。
- 当前仍声明 AgentRun runtime-surface update port，但曾负责 immutable AgentFrame revision 与
  Product surface 收敛的生产 updater 已被删除；接口存在不代表 Canvas 已能进入 AgentFrame。
- 当前 `workspace_module_present` 会建立 Canvas presentation 所需的 Interaction instance/attachment，
  但 presentation/attachment 与 AgentFrame 是不同事实；present 不负责把 Canvas module/VFS mount
  纳入 AgentFrame。
- 当前 `OperationGateway` 已具备 authenticated user、AgentRun agent、Project/Interaction scope、
  Canvas/Interaction origin、attachment 与逐次 current-authority admission，适合作为旧执行层的
  唯一新落点。
- 完整对照使用只读工作树 `D:/ABCTools_Dev/AgentDash-main-reference`，commit
  `957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。
- 当前工作区已有一组未提交改动，开始恢复 `canvas-system` embedded Skill、builtin catalog 与
  lifecycle 默认注入；该改动必须在最终能力合同明确后继续收束，不能描述尚不存在的 bridge。

## Requirements

- R1：恢复独立 `canvas-system` embedded Skill，并让 Project provisioning、lifecycle mount、
  Agent skill baseline 与 Workspace Module 引导一致暴露它。
- R2：完整恢复旧 Agent-facing Workspace 工具合同：
  `workspace_module_list/describe/operate/invoke/present` 的名称、参数、返回、诊断和使用顺序保持不变；
  `workspace_module_operate` 继续支持 `canvas.create`、`canvas.attach`、`canvas.copy`，不新增工具名或
  标记。Agent 能借此创建、挂接、复制、读取和修改 Canvas InteractionDefinition source；写入遵守
  immutable SourceBundle revision 与 expected-base CAS。
- R3：恢复 Canvas authoring source 的 VFS 使用体验，但底层写入必须生成新 definition revision，
  不复活旧 mutable Canvas aggregate 或 `canvas_fs` 事实源。
- R4：为 definition preview 与 attached Interaction runtime 建立统一、版本化、最小权限的 Canvas
  host SDK；SDK 通过受控 MessageChannel/envelope 连接宿主，不向 iframe 暴露 credential、
  backend ID、placement、authority revision、session ID 或本机路径。
- R5：恢复 Canvas 内的 capability 调用能力，并迁移为 describe/discovery 返回的 exact
  OperationRef；每次调用重新经过 OperationGateway admission、schema、authority、cancellation、
  result 与 audit。
- R6：恢复 Canvas 对当前资源面的图片/二进制展示能力；资源解析沿 applied VFS/resource surface
  与 attachment authority，不让浏览器直接加载 mount URI。
- R7：用 canonical Interaction state/command/event 完整承接旧 interaction snapshot 的产品价值：
  用户选择、表单、过滤器和最近交互可被 Agent 观察，但 state projection 只读，修改必须经过
  schema、expected revision 与声明 command/event。
- R8：恢复 Canvas submit-to-Agent，使显式用户动作可以向当前 AgentRun mailbox 提交 canonical
  user input，并明确 queue/steer、幂等、running/terminal 状态与 attachment currentness。
- R9：恢复 renderer observation 与 diagnostics，使 Agent 能检查 ready/error、viewport、受限 DOM
  summary、构建/运行错误和当前 presentation generation；诊断事实不成为 Interaction state。
- R10：把旧 `canvas.bind_data` 迁移到 definition/shared-instance/attachment-local resource slot
  binding，保持 source、shared state 与 actor-local resource authority 分离。
- R11：Workspace Module 同源投影 `canvas:{definition_id}` 与
  `interaction:{instance_id}`，并暴露准确的 authoring、runtime、resource、diagnostic 与
  presentation Operations；不建立第二套 catalog。Canvas 只是一个系统内嵌 Workspace Module
  provider；通用 Workspace Module 层只组合 provider descriptor、路由 operation 和处理
  presentation，不查询或修改 Canvas definition、mount、Interaction instance 或 AgentFrame。
- R12：Standalone UserWorkshop preview 与 AgentRun-attached runtime 使用同一 SDK contract，
  但按当前 actor/attachment 能力裁切；没有 AgentRun 时 submit-to-Agent 明确 unavailable。
- R13：前端 Canvas、Workspace tab、backend contracts、generated TypeScript、Skill、Trellis specs
  和测试必须同步，不保留旧接口或回退分支。
- R14：实施严格分三段：先把旧 Canvas backend、Workspace 工具、VFS、API/contracts、前端、
  runtime bridge、Skill/references 与测试原文件完整硬搬回当前工作区，允许编译直接失败；再逐个把
  旧 aggregate/Gateway/Session 接缝接到新底层；最后才删除旧路径、当前错误替代实现和重复投影。
  禁止在完整搬运前按当前残缺实现零散补洞。
- R15：Canvas capability 的用户调用以 authenticated user + Project/Interaction authority 为主，
  不以 AgentRun 存活为前提；AgentRun 只提供 Agent caller、attachment-local resource、
  mailbox 与 workspace presentation 上下文。
- R16：Canvas 内的用户 capability 调用与 Agent 工具调用都必须经过同一个 OperationGateway；
  用户点击不得冒充 Agent principal，iframe 不得提供或选择 authority。
- R17：旧 `agentdash.extensions.invoke` 的 Extension channel/backend capability 也必须保留，
  迁移为当前 Extension Operation provider 的 exact Operations，不因统一 Gateway 而丢失。
- R18：Canvas authoring mount 物化只有 create 与 attach 两条产品语义：
  `canvas.create` 创建 definition 后进入 create 链，`canvas.copy` 创建 personal copy 后复用同一
  create 链，`canvas.attach` 对既有 definition 进入 attach 链。两条链都让 authoring VFS mount
  在当前 Product runtime resource surface 生效；若当前底层以 immutable AgentFrame 承载该
  applied resource surface，它只是 mount 收敛的内部持久化机制，不进入 Canvas 或 Workspace
  Module 的公开语义和结果合同。
- R19：`workspace_module_present`、list、describe、invoke、`canvas.bind_data`、inspect、
  interaction-state、source edit、publish/unpublish/archive/promote、instance/attachment 操作均不得
  修改 AgentFrame。present 只处理 presentation command/effect/outbox；ResourceSlot 与
  Interaction attachment 各自沿自己的事实链收敛。
- R20：以 `research/legacy-tool-behavior-matrix.md` 为逐工具验收账本。旧 Workspace 工具、
  Canvas module operations、VFS 工具、iframe bridge、用户资产接口和运行时接口的输入、输出、
  权限、错误及副作用必须各有最终实现与测试落点。

## Acceptance Criteria

- [ ] Project 启动 reconciliation 后，所有 Project 都拥有受管 `canvas-system` Skill；Project Agent
      首帧 lifecycle surface 同时包含 Canvas、Companion 与 Workspace Module 三项默认 Skill。
- [ ] Agent 能从空 Project 创建 Canvas、读取 source、通过 VFS 或 typed changeset 修改 source、
      得到新 revision、describe 并 present；全过程不调用旧 Canvas route/aggregate。
- [ ] Agent-facing 仍完整暴露
      `workspace_module_list/describe/operate/invoke/present`；operate 的
      `canvas.create/attach/copy` 继续保持旧参数和结果，不出现 `canvas.request` 或额外 surface 标记。
- [ ] `canvas.create` 成功后只创建一次 definition，并产生使 Canvas module ref 与 authoring VFS
      mount 生效的内部收敛；幂等重放不重复创建 definition 或 mount，公开结果不暴露 AgentFrame。
- [ ] `canvas.attach` 不复制或修改 definition，只将有权访问的既有 Canvas 纳入 AgentFrame；
      重复 attach 不产生重复 module ref、mount 或无意义 Frame revision。
- [ ] `canvas.copy` 保持旧的一步行为：复制完整 source/config/lineage，并复用 create 链将新 personal
      Canvas 纳入 AgentFrame，不建立第三套 Frame mutation。
- [ ] `workspace_module_present` 在成功、重放、失败和 Canvas/Extension 两类 renderer 场景均不改变
      AgentFrame revision、VFS mount 或 capability surface；它只产生 presentation 事实和回执。
- [ ] Workspace Module core 不包含 Canvas repository、mount、Interaction instance 或 AgentFrame
      专用分支；Canvas create/attach/copy、descriptor projection 与 presentation adapter 均由
      系统内嵌 Canvas provider 通过通用 provider/router contract 接入。
- [ ] `canvas.bind_data`、source edit、inspect/state read 与 instance/attachment 变化不会产生
      AgentFrame revision；binding、source、state、presentation 各自只改变所属事实。
- [ ] Canvas SDK 的 capability discovery/invoke 使用 exact OperationRef，并覆盖成功、schema 拒绝、
      capability 撤销、stale authority、timeout/cancel 与结果错误。
- [ ] Canvas 能安全显示当前授权 VFS 中的图片/二进制资源，撤销或 attachment 失效后旧 URL 不再可用。
- [ ] Canvas UI 能通过声明 command/event 更新 canonical Interaction state；Agent 只能看到
      allowlisted projection，并能基于 current state revision 调用允许的 command。
- [ ] 用户可从 attached Canvas 显式 queue 或 steer AgentRun；无 attachment、terminal run、
      stale generation、重复 client command 与非法 input 都产生准确结果。
- [ ] Canvas 构建/运行失败、ready 状态和受限 renderer observation 可由 Agent 通过当前
      Workspace Module descriptor 中的 diagnostic Operation 查询。
- [ ] resource slot binding 覆盖 definition、shared instance 与 attachment-local 三种 authority，
      Canvas source 与 Interaction state 不被隐式修改。
- [ ] standalone preview、AgentRun Workspace presentation 和刷新/重连使用同一 versioned SDK
      contract，并按 actor surface 正确降级。
- [ ] 用户在没有 AgentRun 的情况下可以创建、编辑、预览并使用 Canvas，调用其当前 Project/
      Interaction authority 允许的 exact Operations；打开或关闭 Agent 会话不改变该基础权限。
- [ ] reference worktree 中的资产管理、多文件创作、VFS、运行能力、资源、交互状态、诊断、
      submit、Extension channel、发布/复制/取消发布/晋升能力均在迁移矩阵中有实现与测试落点。
- [ ] 第一阶段硬搬完成时，legacy tool/file/test ledger 无缺项；该阶段只要求文件完整和
      `git diff --check`，明确允许 Rust/TypeScript 编译失败，且未把旧执行路径注册进生产启动链。
- [ ] `rg` 对旧 Canvas route/DTO/repository/runtime snapshot 与旧
      bridge contract 的生产引用为空；不存在兼容或 fallback 实现。
- [ ] Rust focused tests、前端 focused Vitest、contract generation/check、migration guard（如涉及
      schema）与 `git diff --check` 全部通过。

## Out of Scope

- 恢复旧 Canvas aggregate、旧 mutable Canvas table、旧 RuntimeSession bridge ownership 或旧
  `window.agentdash.invoke(actionKey, input)` 字符串协议。
- 为历史发布物或旧数据库记录提供兼容读取、双写、数据搬运或回退路径。
- 把 OperationScript 变成 durable Workflow，或让 iframe 持有执行 authority。

## Technical Notes

无阻塞产品问题。VFS 采用 provider-level atomic mutation：每次 `fs_apply_patch` 在 current
SourceBundle 上形成 changeset，并以 current revision 做 CAS，成功即产生一个新 revision；冲突
直接返回，不引入 working draft aggregate。

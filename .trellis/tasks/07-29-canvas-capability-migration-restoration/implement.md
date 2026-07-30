# 实施计划

## 准备门槛

- [ ] 用户确认 `prd.md`、`design.md` 与本计划。
- [ ] 保持 reference worktree `D:/ABCTools_Dev/AgentDash-main-reference` 只读。
- [ ] 对当前 dirty worktree 做 ownership 清点；不改写其它会话文件。
- [ ] `task.py start` 后再进入实现。

## WP0：锁定回迁清单

- [ ] 将 reference Canvas backend/frontend/Skill/test 文件逐一登记到 transplant ledger。
- [ ] 为 `legacy-tool-behavior-matrix.md` 中每个 Workspace 工具、Canvas operation、VFS 工具、
      iframe bridge 和用户接口绑定原文件、current target owner 与验收测试。
- [ ] 标记所有旧 RuntimeGateway、Canvas aggregate、RuntimeSession 接缝。
- [ ] 单独核对 AgentFrame 副作用：create/copy 复用 create 链、attach 走 attach 链、present/bind/edit
      均不写 Frame。
- [ ] Gate：能力矩阵无“删除/不迁移”产品项，只有内部替换项；Agent-facing 工具合同无改名或加标记。

## WP1：原文件完整硬搬

- [ ] 原样恢复 `agentdash-workspace-module` crate 全部 Canvas/workspace_module 文件。
- [ ] 原样恢复 `workspace_module_list/describe/operate/invoke/present` 注册、surface、runtime context、
      runtime bridge、结果投影和测试，包含 `canvas.create/attach/copy`。
- [ ] 原样恢复旧 Canvas domain/application/API/contracts/repository/runtime-state 文件。
- [ ] 原样恢复 Canvas VFS mount/provider、runtime builder/resource/visibility 文件。
- [ ] 原样恢复前端 Canvas manager、files/bindings editor、runtime preview/SDK、service/types/generated
      contract 位置、Extension Canvas bridge 与测试。
- [ ] 原样恢复 `canvas-system` Skill、全部 references、workspace-module-system 中的 Canvas 内容和 fixtures。
- [ ] 本阶段不改旧 import、不删当前残缺实现、不注册旧 bootstrap/route/provider，不为恢复编译而漏搬。
- [ ] Gate：transplant ledger 与工具矩阵逐文件签核，`git diff --check` 通过；明确允许
      Rust/TypeScript 编译直接失败。

## WP2：接入 Interaction 资产事实与 Workspace provider 深模块

- [ ] 把 Canvas entity/repository 调用替换为 InteractionDefinition/Revision/SourceBundle service。
- [ ] 恢复 `agentdash-workspace-module` 深模块职责：core 只维护通用 provider registry、surface、
      operation/presentation routing 与五个 Agent-facing tools。
- [ ] 建立通用 provider/router contract，让系统内嵌模块和未来用户提供的 Workspace Module 使用
      同一接入面；core 不依赖 Canvas repository、mount、Interaction instance 或 AgentFrame。
- [ ] 将 Canvas definition/runtime projection、create/attach/copy 和 presentation adapter 放入
      系统内嵌 Canvas provider，不在 Workspace Module core 添加 Canvas 分支。
- [ ] 保持 Agent-facing list/describe/operate/invoke/present 名称、参数、返回和诊断。
- [ ] 实现 Project/Definition Canvas authoring providers。
- [ ] 补齐 create/list/read/commit/publish/copy/unpublish/archive/promote/create-instance。
- [ ] 恢复个人/项目共用 Canvas manager、files editor 与权限投影。
- [ ] 同步 Rust contracts、generated TypeScript 与 service。
- [ ] Gate：用户无 AgentRun 可完成完整资产生命周期；definition/module projection 单源。

## WP3：接通 create/attach authoring mount

- [ ] 从硬搬代码恢复 `canvas.create/attach/copy` 编排和完整结果投影。
- [ ] create 创建 personal definition 后调用唯一 create Frame command。
- [ ] copy 创建 personal definition 后复用同一 create Frame command，不建立 copy 专属 surface updater。
- [ ] attach 校验既有 definition authority 后调用唯一 attach Frame command，不改资产。
- [ ] 让 Canvas authoring VFS mount 进入当前 Product applied resource surface；底层如需基于
      committed frame 构造 immutable AgentFrame revision，仅由 Product resource adapter 负责，
      不进入 Workspace Module/Canvas 公开合同；desired surface 已满足时幂等返回。
- [ ] 用当前 AppliedResourceSurface CAS 与 Product runtime rebind/convergence 生效，不恢复旧 direct
      runtime adopter。
- [ ] create/copy 的资产写入与 Frame 收敛共享 invocation/idempotency receipt；失败重放不得重复资产。
- [ ] 证明 present、bind、source edit、instance/attachment 不调用 Frame updater。
- [ ] Gate：create/copy/attach 的资产、Frame、applied surface 与 runtime generation 纵向测试通过；
      present 前后 AgentFrame revision 完全不变。

## WP4：恢复 SourceBundle VFS

- [ ] 建立 Canvas authoring mount identity 与 visibility。
- [ ] 实现 read/list/search/version token。
- [ ] 实现 provider-level atomic apply_patch/write/delete/rename → revision CAS。
- [ ] shared source 只读、bindings generated file 只读、无 exec。
- [ ] 将 mount 注入 AgentRun VFS surface，并让 `canvas-system` 给出准确用法。
- [ ] Gate：Agent 从空项目创建、多文件修改、冲突后重读、present 全链路通过。

## WP5：回迁 preview builder 与 Canvas Host SDK

- [ ] 回迁多文件 build/runtime、React/import map、CSS/JSON/local import。
- [ ] 基于 MessageChannel 建立 Canvas host protocol v1。
- [ ] 实现 initialize/projection/ready/dispose、request/result、generation fencing。
- [ ] 加入 CSP、schema、rate/size/timeout/cancel/outstanding request 限制。
- [ ] definition preview 与 instance runtime 共用 builder/SDK。
- [ ] Gate：老 runtime test 场景迁为新 contract test，reload 不串代。

## WP6：Gateway 能力迁到 Operation 权限层

- [ ] 回迁 old Gateway capability surface 行为，但 descriptor 改为 exact OperationRef。
- [ ] standalone user、interaction user、attachment user 与 Agent principal 使用明确 host。
- [ ] iframe 只提交 OperationRef/input/idempotency key。
- [ ] Agent-facing `workspace_module_invoke(module_id, operation_key, input)` 保持旧合同，由可信 host
      将 current descriptor 的 operation_key 解析为 exact OperationRef。
- [ ] 将 old Extension channel/backend service bridge 一并迁入 Extension exact Operations。
- [ ] 每次调用经过 current surface、schema、placement、audit、timeout/cancel/result。
- [ ] 移除全部 string action-key 与 RuntimeGateway Canvas path。
- [ ] Gate：无 AgentRun 的用户 Canvas 可调用授权 Operation；权限撤销立即生效。

## WP7：资源、状态与 binding

- [ ] 实现 VFS/resource asset URL broker 与 generation cleanup。
- [ ] 补齐 ResourceSlot definition/instance/attachment binding mutation surface。
- [ ] 将文本绑定 materialize 为只读 `bindings/*`。
- [ ] Canvas SDK interaction dispatch/emit 接入 canonical command/event。
- [ ] Agent projection 仅返回 allowlisted state。
- [ ] Gate：图片、文本绑定、shared/local binding、state CAS、Agent inspect 全覆盖。

## WP8：diagnostics 与 submit-to-Agent

- [ ] 建立 renderer observation domain/repository/service/API/Operation。
- [ ] 如需持久化，新增目标 schema migration 与 repository tests。
- [ ] 回迁 ready/error/viewport/DOM summary/build/runtime diagnostic 上报。
- [ ] 实现 attached Canvas mailbox Operations：queue/steer、幂等、currentness、terminal 状态。
- [ ] 支持附带 interaction projection/observation ref。
- [ ] Gate：standalone 明确 unavailable；attached queue/steer 复用 Product receipt。

## WP9：Agent surface、Skill 与产品集成

- [ ] `builtin:canvas`、`canvas:{definition_id}`、`interaction:{instance_id}` descriptor 完整。
- [ ] list/describe/operate/invoke/present 覆盖 authoring/runtime/resource/diagnostic/mailbox，且公开合同与
      legacy tool matrix 一致。
- [ ] 收束已恢复的 `canvas-system` Skill 与 references，确保只描述已实现合同。
- [ ] 同步 lifecycle default skill、Workspace Module Skill 与 Trellis specs。
- [ ] 恢复/改写 E2E：用户 standalone 与 Agent attached 两条路径。
- [ ] Gate：legacy 能力矩阵逐项签核。

## WP10：删除错误路径并验证

- [ ] 只有对应新底层行为测试通过后，才删除 transplant 中不再需要的旧
      aggregate/repository/route/DTO/runtime-state/adapter。
- [ ] 删除当前 `agentdash-application` 内残缺 Workspace Module 替代实现和重复 projection。
- [ ] `rg` 检查旧 aggregate/repository/route/DTO/runtime state/action-key 引用为空。
- [ ] Rust focused fmt/check/test。
- [ ] frontend lint/type-check/focused Vitest。
- [ ] contract generation/check。
- [ ] migration/repository focused tests。
- [ ] `git diff --check`。
- [ ] 运行 `pnpm dev` 做用户 standalone、Agent authoring、attached runtime 手工验收；
      Rust 改动后按项目要求重启调试进程。

## Review gates

1. WP0 后审查能力清单，禁止漏项。
2. WP1 后只审查硬搬完整性，不以编译结果删减文件。
3. WP3 后审查 create/attach AgentFrame 边界，确认 present/bind/edit 没有 Frame 副作用。
4. WP4 后审查 source 单一事实与 VFS CAS。
5. WP6 后审查权限图，确认 Agent-facing 合同未改变且用户态不依赖 AgentRun。
6. WP8 后审查 state/diagnostic/mailbox 三类事实没有混存。
7. WP10 后按 legacy tool/capability matrix 做最终 parity review。

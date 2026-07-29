# Canvas 工具行为对照矩阵

## 对照范围

完整行为基线固定为只读工作树
`D:/ABCTools_Dev/AgentDash-main-reference` 的
`957fa9d60ea3d67efa1bb278fe5b376cf0c34598`。工具清单以实际注册代码、Skill 和测试共同为准，
不能只根据当前 Workspace Module descriptor 反推。

第一阶段原文件硬搬回时保留旧名称和旧调用关系，允许工作区暂时无法编译；第二阶段只替换底层，
Agent-facing 工具名称、参数、返回和使用顺序继续按旧合同，不另加标记或新工具。

## Agent / Workspace 工具

| 旧入口 | 旧行为 | 最终行为 | AgentFrame |
| --- | --- | --- | --- |
| `workspace_module_list` | 读取当前 Agent surface 可见 module summary | 读取 current Product/Agent surface；不物化、不请求、不呈现 | 不写 |
| `workspace_module_describe` | 返回指定可见 module 的 UI entry、operation schema、readiness、权限摘要 | 返回同一 canonical module projection；Operation 使用 exact `OperationRef` | 不写 |
| `workspace_module_operate(operation="canvas.create")` | 创建 personal editable Canvas，返回 descriptor，并把 module/VFS mount 暴露给当前会话 | 保持同名同参同结果；底层创建 personal InteractionDefinition 首 revision，并通过内部 create surface command 纳入当前 AgentFrame | **通过 create 写入** |
| `workspace_module_operate(operation="canvas.attach")` | 加载已存在且可查看 Canvas，并把 module/VFS mount 暴露给当前会话 | 保持同名同参同结果；底层不复制、不修改 definition，通过 Canvas attach 链路纳入当前 AgentFrame | **通过 attach 写入** |
| `workspace_module_operate(operation="canvas.copy")` | 把 shared Canvas 复制为 personal Canvas，并隐式暴露到当前会话 | 保持一步完成：复制完整 source/config/distribution lineage，并把新 personal definition 交给同一内部 create surface command | **通过 create 写入** |
| `workspace_module_invoke(..., "canvas.bind_data")` | 写 AgentRun-scoped runtime binding，生成 `bindings/*`，旧实现同时更新 runtime surface | 写 ResourceSlot binding（definition/shared instance/attachment-local 之一），不改 source/state；资源面按自身 current authority 收敛 | 不写 |
| `workspace_module_invoke(..., "canvas.inspect")` | 读取 active run/agent 对应的 latest render observation | 读取 current presentation renderer lease/generation 的 ready/error/viewport/DOM summary/diagnostics | 不写 |
| `workspace_module_invoke(..., "canvas.get_interaction_state")` | 读取 Canvas 显式发布的 latest interaction snapshot，不进入对话历史 | 读取 canonical Interaction state 的 Agent allowlisted projection | 不写 |
| `workspace_module_invoke` 的 runtime action / protocol channel / backend service 分支 | 按 descriptor dispatch 到 RuntimeGateway、Extension channel 或 backend service | 所有分支成为 provider-qualified exact Operations，统一经过 OperationGateway；Extension channel 能力不得丢失 | 不写 |
| `workspace_module_present` | 校验 module/view 后通知前端打开 UI；旧实现还错误地顺带请求 Canvas visibility | 只提交/重放 Workspace Module presentation intent/change/outbox，并返回 presentation receipt；可建立呈现所需 Interaction attachment，但不得改变 AgentFrame、VFS mount 或 capability surface | **不写** |

### AgentFrame 唯一规则

最终只有两种命令可以改变 AgentFrame：

1. 内部 create surface command：由 Agent-facing `canvas.create` 和 `canvas.copy` 在成功创建新定义后
   使用，把新 Canvas module ref 与 authoring VFS mount 写入新的 immutable AgentFrame revision。
2. Canvas attach command：由 Agent-facing `canvas.attach` 使用，对已存在且当前有权使用的
   Canvas，把 module ref 与 authoring VFS mount 写入新的 immutable AgentFrame revision。

`list`、`describe`、`bind_data`、`inspect`、`get_interaction_state`、source edit、
publish/unpublish/archive/promote、instance/attachment 操作和 `workspace_module_present` 都不得直接写
AgentFrame。`copy` 不拥有第三套 Frame mutation，只复用内部 create command。重复 attach 在
desired surface 已满足时返回既有收敛证据，不生成重复 mount/module ref。

## Canvas VFS 工具

旧 Canvas mount 通过通用 VFS 工具使用，工具行为也属于 Canvas parity：

| 工具 | 旧 Canvas 行为 | 最终行为 |
| --- | --- | --- |
| `fs_read` | 读取 Canvas source 或 materialized binding，并返回 version token | 读取 pinned/current SourceBundle；version token 使用 definition revision/digest |
| `fs_glob` | 在 Canvas mount 内 list/glob | 对 SourceBundle 与只读 generated bindings 做同等投影 |
| `fs_grep` | 在 Canvas 文本文件内搜索 | 对允许读取的 source/generated text 搜索 |
| `fs_apply_patch` | 通过 provider 完成 write/delete/rename；read-only mount 拒绝写入 | 单次 patch 在 current SourceBundle 上形成 changeset，并用 expected revision CAS 生成一个新 revision |

Canvas mount 的能力合同为：

- personal editable：read、write、list、search、delete、rename；
- project shared：read、list、search；
- `bindings/*`：read-only generated files；
- 永远不支持 exec；
- URI 保持 mount-relative authoring identity，不暴露 backend id 或绝对路径。

Source edit 只产生 definition revision，不改变 AgentFrame。AgentFrame 中的 mount identity 保持稳定，
后续读取解析 current authorized definition revision。

## iframe Canvas Host API

| 旧 API | 必须保留的行为 | 最终落点 |
| --- | --- | --- |
| `window.agentdash.invoke(actionKey, input)` | 显式用户动作调用运行时能力，返回 trace/result，支持拒绝、失败、超时、取消 | `operations.invoke(exactOperationRef, input)` → trusted host → OperationGateway |
| `assets.url(uri)` | 仅解析 current Canvas/runtime VFS 可见的图片资源，返回浏览器可用 URL | applied resource surface asset broker |
| `assets.revoke(url)` | 主动释放 URL；reload/unmount 自动清理 | host-managed revocable URL registry |
| `interaction.setState/clearState` | 发布紧凑、Agent 可见的当前 UI 语义状态 | declared Interaction command + expected state revision |
| `interaction.emit` | 发布近期用户事件 | declared Interaction event |
| `interaction.getState` | 读取当前 iframe projection | canonical state browser-safe projection |
| `agent.submit` | 显式用户动作向当前 AgentRun mailbox queue/steer，支持幂等与可选 state/observation | exact mailbox Operation；standalone 时明确 unavailable |

旧 `mcp.call_tool`、`mcp.list_tools` 以及 Extension protocol channel/backend service 不是可删除的旁支；
它们需要由 current provider surface 投影为 exact Operations。浏览器只提交 OperationRef/input/
idempotency key，不得提交 principal、scope、placement、credential 或 backend identity。

## 用户 Canvas 资产与运行时行为

旧 API/service/test 覆盖以下行为，迁移时必须逐项映射到 Interaction，而不是只恢复 Agent 工具：

| 行为组 | 旧入口 | 最终事实源 |
| --- | --- | --- |
| 资产发现 | list by project/scope、get by id、get by mount | InteractionDefinition + current revision |
| 资产创作 | create、update files/entry/import map/sandbox/title/description、delete | immutable SourceBundle revision + definition metadata |
| 分发 | publish-to-project、copy-to-personal、unpublish | definition ownership/distribution |
| Extension | promote-extension | current Extension projection/install contracts |
| runtime projection | runtime-snapshot | definition revision + optional instance/attachment + resource surface |
| renderer facts | runtime-observation get/upsert | renderer observation latest fact |
| UI state | interaction-snapshot get/upsert | canonical Interaction state/command/event |
| binding | runtime-bindings upsert | ResourceSlot binding |
| capability execution | runtime-invoke | OperationGateway |
| Agent feedback | agent-input-submit | AgentRun mailbox |

这些用户入口不依赖 AgentRun；只有 attachment-local resource、attached diagnostics 和
submit-to-Agent 需要 AgentRun/attachment。

## 搬运与适配顺序

1. **完整硬搬**：恢复 `agentdash-workspace-module`、旧 Canvas domain/application/API/contracts、
   Workspace 工具注册与执行文件、VFS provider、前端 Canvas feature、service、runtime bridge、
   Skill/references 和全部相关测试。此时允许编译直接失败。
2. **清点签核**：按本矩阵确认每个旧工具、输入、输出、错误、权限和副作用都有原文件与测试落点。
3. **新底层接线**：依次替换 Canvas aggregate、RuntimeGateway、RuntimeSession、旧 state/binding 为
   Interaction、OperationGateway、Product surface、ResourceSlot 和 mailbox。
4. **AgentFrame 收束**：只给 Canvas create/attach command 接 immutable frame revision +
   Product runtime surface rebind/convergence；Agent-facing create/copy/attach 分别复用这两条语义，
   从 present/bind/source edit 等其它路径中移除 Frame 更新。
5. **删除错误路径**：新链路行为测试通过后，再删除旧 repository/route/DTO/runtime snapshot、
   当前残缺 Workspace Module 实现和重复 projection。

## 关键证据

- `agentdash-workspace-module/src/workspace_module/tools.rs`：五个 Workspace Agent 工具注册、参数和结果。
- `agentdash-workspace-module/src/workspace_module/surface.rs`：create/attach/copy、invoke、present 的真实副作用。
- `agentdash-workspace-module/src/workspace_module/mod.rs`：bind/inspect/get-state descriptors。
- `agentdash-workspace-module/src/canvas/vfs_provider.rs`：Canvas mount read/write/delete/rename/list/search/no-exec。
- `agentdash-domain/src/canvas/skills/canvas-system/`：Agent 工具与 iframe bridge 的公开使用合同。
- `agentdash-api/src/routes/canvases.rs` 与 `packages/app-web/src/services/canvas.ts`：用户资产和运行时行为面。
- 当前 `agentdash-application/src/runtime_tools/workspace_module_product.rs`：仅有
  list/describe/invoke/present，且 present 内含 Interaction attachment 逻辑；这不等于 AgentFrame 更新。
- 当前 `agentdash-application-ports/src/agent_frame_materialization.rs`：仍有 surface update port
  声明，但生产 updater 已删除，不能把接口存在误判为 create/attach 已生效。

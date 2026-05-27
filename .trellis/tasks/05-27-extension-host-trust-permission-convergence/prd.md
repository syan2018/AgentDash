# Extension Host 信任模型与权限裁决收口

## Goal

先处理 extension runtime 后续优化中风险最高的一段：让 TS Extension Host 的信任模型与实际执行能力一致，并让 RuntimeGateway、runtime projection、本机 host 对 extension action 权限得出同一裁决结果。

## Requirements

- 首版执行模型按真实状态表达为 trusted local extension。
  - 当前 Node `vm` 不作为安全 sandbox 宣称；它只能作为插件 bundle 加载和全局对象约束的执行机制。
  - Host/API/contract/spec 中对本机 TS extension 的描述必须避免“受限 VM 安全隔离”语义，改为“本机可信插件通过 host facade 获得产品权限与审计入口”。
  - 后续 isolated execution 可以继续由 parent task 追踪，但不阻塞本任务交付。
- 权限裁决语义收敛。
  - `api.local.getProfile()` 等 host API 的使用必须同时被 Gateway admission 与本机 host enforcement 约束。
  - 顶层 extension capability 和 action-level permission 的关系必须明确；本任务采用更严格语义：使用 host API 时需要 extension 顶层 capability 与当前 action permission 同时满足。
  - runtime projection 与 action surface 不应让 action permission 为空的 action 看起来拥有 `local.profile.read` 能力。
- 本机 host 权限实现需要可测试。
  - 增加“顶层声明 local profile、action 未声明 local.profile.read 时拒绝”的本机 host 测试。
  - 增加 Gateway 或 permission evaluator 测试，覆盖同一 manifest/action 组合的拒绝结果。
- `agentdash-local` extension host 代码在本任务中只做必要目录化。
  - 若实现权限收口时需要拆分文件，优先把 extension host 相关逻辑移动到 `extensions/host` 边界。
  - 不在本任务中重排 artifact storage、workspace preparation、terminal、MCP 等无关 local 模块。

## Acceptance Criteria

- [ ] 文档/spec/contract 中不再把当前 Node `vm` 执行模型描述为安全 sandbox；若保留 `vm`，其语义是 trusted local extension runner。
- [ ] `api.local.getProfile()` 在 extension 顶层声明 `local_profile` 但 action 未声明 `local.profile.read` 时被拒绝。
- [ ] RuntimeGateway extension action admission 拒绝“action 声明 `local.profile.read` 但 extension 顶层未授权”的 manifest，本机 host enforcement 拒绝“实际调用 `api.local.getProfile()` 但当前 action 未声明 `local.profile.read`”的调用。
- [ ] 权限规则有清晰的共享 helper、fixture 或等价测试，后续新增 workspace/runtime_action permission 时有可延展入口。
- [ ] 若移动 local extension host 文件，`agentdash-local` crate 对外 re-export 仍保持稳定。
- [ ] 聚焦测试通过：`cargo test -p agentdash-local extension_host`、相关 `agentdash-application` runtime gateway 测试，以及必要的 `cargo check`。

## Out Of Scope

- 不在本任务实现真正 OS/Node 级 untrusted sandbox。
- 不处理 extension artifact storage 从 API route 抽离。
- 不处理 Vite dev proxy。
- 不重写 extension SDK 或 WorkspacePanel UI；除非 contract 文案变化需要最小同步。

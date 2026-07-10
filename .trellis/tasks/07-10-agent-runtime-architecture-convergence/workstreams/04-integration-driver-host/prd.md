# Integration Runtime Driver Host

## Goal

将 Agent 服务改造成受信 Integration contribution，建立 service definition/instance/offer/factory/binding/placement/router，让 Native、Codex 与企业 Agent 不再通过 application/executor 硬编码进入系统。

## Depends On

- `01-runtime-contract`

## Parent Design

- `../../design.md` 第 6、7、10 节
- `../../research/integration-runtime-baseline.md`

## Requirements

- Integration API 改为贡献 `AgentRuntimeDriverContribution { definition, factory }`。
- 实现 AgentServiceDefinition、AgentServiceInstance、RuntimeOffer、RuntimeBinding、DriverLease 与 SourceIdMap。
- 支持 config schema、credential slots/refs、health、driver generation 与 activation。
- Router 只按 durable sticky binding路由；source event验证generation与mapping。
- effective profile 由 service guarantee、placement transport与host policy求交。
- Driver descriptor声明逐trigger HookProfile与delivery mechanism；Host只负责校验/绑定，不拥有Hook policy。
- Binding持久化BoundHookPlan、plan/artifact digest、configuration boundary与per-point apply status；required point未ack时不dispatch Turn。
- First-party/enterprise service通过同一contribution机制装配。
- 删除 connector enum、Composite OR、broadcast cancel/approval 与 first-live-session probe。

## Acceptance Criteria

- [ ] 新企业 Agent service只新增Integration contribution/config，不修改application或router分支。
- [ ] 同一Integration可贡献多个definition，同一definition可创建多个instance。
- [ ] Thread binding sticky且durable，旧generation event无法推进新状态。
- [ ] credential/health/config failure在side effect前typed reject。
- [ ] Relay只作为placement descriptor/transport，不成为service identity。
- [ ] HookProfile由behavior tests支撑，不能由配置文件存在、hooks/list或driver自报直接提升。
- [ ] 生产composition中不再硬编码Pi/Codex/Relay connector构建。

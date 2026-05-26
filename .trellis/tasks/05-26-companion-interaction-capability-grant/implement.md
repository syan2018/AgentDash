# Companion 通用交互信道与能力扩展治理设计实施计划

## Phase A：规划收敛

- [ ] 和用户确认 MVP 范围：能力 grant 链路 + companion 基础契约轻量收束 + `payload: object`。
- [ ] 明确 `target=platform` 的产品语义与 broker 命名。
- [ ] 明确 grant scope 首版支持集合：`turn`、`session`、`workflow_step` 的取舍。
- [ ] 明确 live apply 与 next-turn apply 的 connector 支持矩阵。
- [ ] 审核是否需要父子任务拆分。

## Phase B：契约与领域模型

- [x] 扩展 companion target enum，增加 `platform`。
- [x] 扩展 companion payload registry，补齐 typed schema / expected response / ui hint 的集中契约。
- [x] 将 `CompanionRequestParams.payload` 与 `CompanionRespondParams.payload` 从 JSON string 改为结构化 object。
- [x] 新增 `capability_grant_request` 与 `capability_grant_result` payload contract。
- [ ] 设计 grant request domain entity、状态机、repository 与 migration。
- [x] 接入 platform broker 骨架，以 human approval interaction 承载 capability grant request。
- [ ] 设计 platform broker service，把 payload 转成 grant request。
- [ ] 设计 approved grant 到 `RuntimeCapabilityTransition` 的 compiler。

## Phase C：Runtime 接入

- [x] 在 companion target routing 中接入 `platform` broker。
- [ ] 在 grant approved 后调用 capability transition apply 入口。
- [ ] 补齐 live apply 成功、失败、next-turn apply 的事件与 context frame。
- [ ] 确认 `tool_schema_delta` 只展示真正新增给 Agent 的工具。
- [ ] 补齐 TTL / revoke 对 runtime tool visibility 的影响。

## Phase D：Embedded Skill

- [x] 新增 `companion-system` embedded skill bundle。
- [x] 编写 `SKILL.md` 与 references。
- [x] 将 bundle 纳入源码声明与 validation。
- [x] 在 lifecycle mount / session skill projection 中默认注入 `companion-system`。
- [x] 补测试覆盖 lifecycle mount projection 与 runtime capability update 相关路径。

## Phase E：Frontend

- [x] 扩展 Companion request card，按 `payload_type` / `ui_hint` 做 renderer 分发。
- [x] 新增 capability grant approval / result 展示。
- [x] 展示 requested paths、reason、TTL、审批状态、失败原因。
- [ ] 保持 ContextFrame 的 capability delta / tool schema delta 展示为最终生效反馈。

## Phase F：验证

建议命令：

```powershell
cargo test -p agentdash-spi
cargo test -p agentdash-application companion
cargo test -p agentdash-application capability
cargo test -p agentdash-api
pnpm --filter app-web test
pnpm --filter app-web typecheck
```

最终联调：

```powershell
pnpm dev
```

验证场景：

- Agent 通过 companion 请求一个未授权平台 MCP 工具。
- 用户批准后，当前 session 收到 capability delta 与 tool schema delta。
- Agent 能调用新批准的具体工具。
- 用户拒绝后，工具不进入 provider tools。
- grant 过期或撤销后，后续 turn 不再暴露对应工具。

## 风险文件

- `crates/agentdash-application/src/companion/tools.rs`
- `crates/agentdash-application/src/companion/payload_types.rs`
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
- `crates/agentdash-application/src/session/capability_state.rs`
- `crates/agentdash-spi/src/session_persistence.rs`
- `crates/agentdash-spi/src/platform/tool_capability.rs`
- `crates/agentdash-domain/src/embedded_skill.rs`
- `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx`
- `packages/app-web/src/features/session/model/contextFrame.ts`

## 回滚点

- Payload registry 扩展应保持 request / response 校验入口集中，便于单独回退。
- Grant request 持久化与 runtime apply 分阶段合入，避免数据模型和 live tool update 同时漂移。
- Embedded skill bundle 可先作为只读 skill projection 引入，再接入自动注入策略。

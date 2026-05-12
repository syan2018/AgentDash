# 主线 5：Governance And Migration

## Goal

建立 Runtime Gateway 的治理、审计、错误、长任务和迁移标准，确保新协议不会变成另一个隐形旁路。该主线贯穿其它主线，用来约束“统一入口”是否真的成立。

## Scope

- Policy / permission 模型。
- Invocation trace / audit。
- Runtime Operation 长任务模型。
- 错误码、超时、取消、重试语义。
- 旧 API / relay 直连路径迁移准则。
- 文档与测试策略。

## Dependencies

- 依赖主线 1 的核心类型。
- 影响主线 2、3、4 的实现检查。

## Execution Plan

1. Policy 模型：
   - Session Runtime Action 使用 CapabilityState / tool policy。
   - Setup Action 使用项目权限、backend 权限、配置流程权限。
   - 明确 actor 可调用 action 的矩阵。
2. Trace / Audit：
   - 每次 invocation 生成 invocation_id / trace_id。
   - 记录 actor、session_id、setup context、action_key、target、result status。
   - 支持投影到 session event 或平台审计视图。
3. Runtime Operation：
   - 长任务不挂死 HTTP。
   - 支持 status query、cancel、timeout、result retrieval。
   - 环境准备、terminal、长 MCP call 可逐步接入。
4. 错误模型：
   - `invalid_request`
   - `capability_denied`
   - `provider_unavailable`
   - `provider_failed`
   - `timeout`
   - `cancelled`
5. 迁移准则：
   - 新增能力不得绕过 Gateway。
   - 旧 route 迁移为 thin route。
   - relay command 保持 transport，不成为上层业务 API。
6. 文档与测试：
   - 更新跨层契约 spec。
   - 为 setup/session action 分别写测试清单。
   - 为每次迁移保留行为等价测试。

## Acceptance Criteria

- Gateway 有统一 actor/action 权限矩阵。
- Invocation trace 覆盖 session runtime 与 setup action。
- 长任务有明确 Operation 方案，不要求第一阶段全部实现。
- API thin route 迁移有检查标准。
- 文档明确 Relay / MCP / VFS / Gateway 的边界。

## Risks

- 如果审计仅做日志而不是结构化 trace，后续很难排查跨端调用。
- 如果 Setup Action 权限过宽，会变成本机探测后门。
- 如果迁移期允许新旧路径长期并存，维护成本会反弹。

## First PR Shape

- 先定义错误码和 trace 结构。
- 在第一批 provider 中强制产出 trace。
- 将迁移准则写入 spec，后续主线按此检查。

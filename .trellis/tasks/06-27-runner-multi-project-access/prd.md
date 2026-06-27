# Runner 多 Project 复用授权管理

> 独立任务（不挂父任务）。依赖前置任务 `06-27-local-backend-enrollment-convergence` 已落地的「runner backend 身份去 project 化 + ProjectBackendAccess 权威授权 + 最小 auth 放行规则」。本任务在此模型之上构建完整的多 project 复用授权产品能力。

## Goal

让一台已部署的 Local Runner 能被多个 Project 安全复用：在前置任务把 runner backend 改为机器级身份、`ProjectBackendAccess` 成为唯一权威 project→backend 授权层之后，补齐围绕该模型的**完整授权管理能力与产品入口**，包括把已有 runner 授权给新 project、撤销、优先级与访问策略，以及配套的可见性/审计语义。

## Background / 前置事实

- 前置任务后：runner backend `share_scope=User(owner=token 创建者)`，`stable_local_backend_id` 不含 project_id；同机器 + 同 capability_slot 只有唯一 backend。
- project 对 runner 的可见/可用完全由 active `ProjectBackendAccess(project, backend)` 决定。
- `authorization.rs` 已有最小放行规则：User-scoped backend + active grant → 该 project 成员按 permission 放行；无 grant 维持 owner-only。
- claim 时会为 token 所属 project 自动写 active grant。
- 当前只有 User/Project/System 三种 scope，无 Org/Team。

## Requirements

- 把已有 runner backend 授权给新 Project 的产品流程（不需要重新部署 runner，也不必为每个 project 单独发 token 再 claim）：
  - 在 Project 设置内浏览/选择「同 owner 名下、或可被本 project 使用的」已有 runner backend 并建立 active grant。
  - 撤销 / 暂停 / 恢复 project 对某 runner 的 grant。
- 多 grant 下的访问策略：
  - priority 排序语义（已存在字段）在多 project / 多 backend 场景下的明确定义与 UI 暴露。
  - root_policy / capability_policy 在 per-project grant 维度的可配置性与默认值。
- 审计与可解释性：
  - 谁在何时把哪个 runner 授权给了哪个 project；撤销同理。
  - 一个 runner 当前服务于哪些 project 的反向视图。
- 安全边界：
  - 非 owner 的 project 成员能否发起授权、需要何种权限（project Edit / 平台权限）需明确。
  - 跨 owner 复用（A 用户部署的 runner 被 B 用户的 project 使用）是否允许、走什么审批，需明确。

## Scope Boundaries

- 不重复实现前置任务已完成的 enrollment 收束、身份去 project 化、最小 auth 放行规则。
- 不在本任务做 Workspace 设置整体心智 IA 重构（归 `06-27-local-backend-enrollment-convergence` 内处理或其后续）。

## Acceptance Criteria

- [ ] Project 设置内可把一台已有 runner 授权进来（建立 active grant）并可撤销，无需重新部署或重新 claim。
- [ ] 一台 runner 可同时服务 N 个 project，各 project 独立 grant、独立撤销，互不影响。
- [ ] priority / root_policy / capability_policy 在 per-project grant 维度可解释、有合理默认，并在 UI 呈现。
- [ ] 授权 / 撤销有审计记录与反向（runner→projects）视图。
- [ ] 安全边界明确：发起授权所需权限、跨 owner 复用策略有定义并被测试覆盖。

## Open Questions / 后续追踪

- **（原 C）引入 Org/Team scope**：当前 runner owner 绑定到 token 创建者（个人 User），创建者离开/换岗会带来所有权问题。是否引入 Org/Team scope 让 runner 所有权脱离个人、以组织为单位管理复用与审计——在本任务内评估，必要时再拆独立任务落地。
- 跨 owner 复用是否需要 owner 侧审批流。
- priority 冲突（多 runner 同 project）时的解析与 UI 表达细节。

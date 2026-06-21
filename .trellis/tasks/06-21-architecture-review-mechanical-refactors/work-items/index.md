# Work Items Index

| ID | Title | Group | Status | Source |
| --- | --- | --- | --- | --- |
| M01 | Project event NDJSON contract 化 | Contract Surface | pending | `research/10-contract-boundary-deep-dive.md` |
| M02 | ProjectBackendAccess / BackendWorkspaceInventory contract 化 | Contract Surface | pending | `research/10-contract-boundary-deep-dive.md` |
| M03 | Canvas CRUD contract 化 | Contract Surface | pending | `research/10-contract-boundary-deep-dive.md` |
| M04 | SkillAsset HTTP DTO contract 化 | Contract Surface | pending | `research/10-contract-boundary-deep-dive.md` |
| M05 | ExtensionManagement service 回到 generated DTO | Contract Surface | pending | `research/10-contract-boundary-deep-dive.md` |
| M06 | `workspace_module_presented` stream payload contract 化 | Contract Surface | pending | `research/10-contract-boundary-deep-dive.md`, `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| M07 | Auth/current-user/identity-directory DTO contract 化或明确 wrapper | Contract Surface | pending | `research/10-contract-boundary-deep-dive.md` |
| M08 | 拆分 `types/index.ts` | Residual Surface Cleanup | pending | `research/10-contract-boundary-deep-dive.md` |
| M09 | 确认 SessionExecutionState 消费面 | Residual Surface Cleanup | pending | `research/10-contract-boundary-deep-dive.md` |
| M10 | 移除或封装 `AgentRunSteeringService` | Residual Surface Cleanup | pending | `research/11-agentrun-control-deep-dive.md` |
| M11 | 清理 AppState 中未公开消费的 `StoryActivityActivationService` | Residual Surface Cleanup | pending | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| M12 | raw anchor repository API 与 application selection API 分层命名 | Residual Surface Cleanup | pending | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| M13 | RuntimeGateway `surface_for` debug 入口守卫 | Residual Surface Cleanup | pending | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| M14 | 固化 runtime status aggregation owner tests | Tests / Diagnostics / UI | pending | `research/12-lifecycle-runtime-facts-deep-dive.md` |
| M15 | top-level `AgentRunWorkspaceView.control_plane` display-only 验证 | Tests / Diagnostics / UI | pending | `research/11-agentrun-control-deep-dive.md` |
| M16 | WorkspaceModule runtime deps 缺失可观测诊断 | Tests / Diagnostics / UI | pending | `research/13-permission-frame-vfs-gateway-deep-dive.md` |
| M17 | workspace routing 文案区分 binding 与 execution | Tests / Diagnostics / UI | pending | `research/14-local-placement-relay-deep-dive.md` |
| M18 | Profile UI 将 machine id 表达为只读事实 | Tests / Diagnostics / UI | pending | `research/14-local-placement-relay-deep-dive.md` |
| M19 | extension relay payload 不携带 backend_id regression test | Tests / Diagnostics / UI | pending | `research/14-local-placement-relay-deep-dive.md` |

## Excluded Design Items

以下内容不进入本机械任务池，留在父任务 `design-coupling-tracker.md`：

- application/contracts 分层 owner 的最终架构取舍。
- AgentRun delivery runtime resolver 与 RuntimeSessionExecutionAnchor selection policy。
- Lifecycle start/drain public command 合同。
- PermissionGrant / AgentFrame / Canvas expose 的运行态事实源。
- Extension backend target resolver 与 relay command target taxonomy。


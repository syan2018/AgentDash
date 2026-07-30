# Canvas 回迁文件台账

基线差异来自 `3028f4456` 与其后的前端清理提交。实施时先把本台账文件全部硬搬回工作区，
允许编译失败；“适配/不恢复”描述的是第二阶段最终处理，不是第一阶段可以漏搬的理由。状态含义：

- **直接回迁并适配**：恢复文件职责和行为，内部依赖改到新底层。
- **以当前实现为主合并**：当前已有部分新实现，把 legacy 行为补回同一文件。
- **仅作行为参考**：不恢复旧类型/仓储/执行层。

## Workspace Module 深模块

| Legacy 文件 | 处理 |
| --- | --- |
| `agentdash-workspace-module/src/canvas/identity.rs` | 直接回迁并适配 definition/instance identity |
| `canvas/management.rs` | 直接回迁并适配 Interaction definition use cases |
| `canvas/runtime.rs` | 直接回迁并适配 SourceBundle + host SDK projection |
| `canvas/runtime_resource.rs` | 直接回迁并适配 ResourceSlot/attachment surface |
| `canvas/vfs_mount.rs` | 直接回迁并适配 definition revision mount |
| `canvas/vfs_provider.rs` | 直接回迁并适配 atomic revision CAS |
| `canvas/visibility.rs` | 直接回迁并适配 user/Agent authority |
| `workspace_module/runtime_bridge.rs` | 先原样硬搬；再把执行接 Operation host、Frame 接 create/attach |
| `workspace_module/runtime_context.rs` | 先原样硬搬；再去掉 RuntimeSession ownership |
| `workspace_module/runtime_tool_provider.rs` | 先原样硬搬五个 Workspace 工具；再合并当前 Product tool host |
| `workspace_module/surface.rs` | 先原样硬搬 create/attach/copy/invoke/present；再改 canonical Operation catalog |
| `workspace_module/tools.rs` | 先原样硬搬名称、参数、返回、诊断和测试；Agent-facing 合同保持旧版 |
| `workspace_module/visibility.rs` | 直接回迁并使用 current capability dimension |
| `workspace_module/mod.rs` | 以当前实现为主合并 legacy 完整职责 |

恢复 crate manifest/workspace membership。Agent-facing 必须继续暴露
`workspace_module_list/describe/operate/invoke/present`，operate 必须继续支持
`canvas.create/attach/copy`。当前
`agentdash-application/src/workspace_module.rs` 与
`runtime_tools/workspace_module_product.rs` 的逻辑最终迁入该 crate，不能保留两份 projection。

## Domain / Application / Persistence

| Legacy 文件 | 处理 |
| --- | --- |
| `agentdash-domain/src/canvas/entity.rs` | 仅作行为参考；映射 InteractionDefinition/Revision |
| `canvas/value_objects.rs` | 仅作行为参考；需要的合同进入 SourceBundle/ResourceSlot |
| `canvas/access.rs` | 仅作行为参考；映射 Project/Interaction/Operation authority |
| `canvas/repository.rs` | 先硬搬；完成 Interaction repository 适配后删除 |
| `canvas/runtime_state.rs` | 先硬搬；拆到 canonical state 与 renderer observation 后删除 |
| `agentdash-application/src/canvas/diagnostics.rs` | 直接回迁职责，改为 renderer observation service |
| `canvas/promotion.rs` | 以当前 Interaction extension promotion 为主合并 legacy 测试 |
| `canvas/mod.rs` | 恢复产品 facade，不恢复 aggregate |
| `canvas_repository.rs` | 先硬搬；Interaction persistence 接通后删除 |
| `canvas_runtime_state_repository.rs` | 先硬搬；必要时新建 renderer observation repository/migration 后删除 |

第一阶段仍硬搬上述标为“不恢复”的旧文件，用于保证类型、错误和测试场景没有漏失；第二阶段完成
Interaction 映射后才删除。

## API / Contracts

| Legacy 文件 | 处理 |
| --- | --- |
| `routes/canvases.rs` | 仅作 endpoint/错误/验收参考；能力进入 Interaction/API/Operation hosts |
| `dto/canvas.rs` | 先硬搬；新合同进入 Interaction/Canvas host contract 后删除旧 DTO |
| `surface/canvas.rs` | 回迁产品 runtime/diagnostic contract，使用新 identity |
| `routes/workspace_module.rs` | 以当前实现为主合并 user/Agent Canvas host |
| generated `canvas-contracts.ts` | 先硬搬作类型/行为基线；再由新 Rust contract 重新生成覆盖 |

第一阶段原样搬回 route/DTO/generated contract 位置，但不注册旧 router 或生成双合同；适配完成后按
最终合同生成并删除旧类型。

## Frontend

| Legacy 文件 | 处理 |
| --- | --- |
| `CanvasFilesEditor.tsx` | 直接回迁并适配 SourceBundle revision |
| `CanvasRuntimeBindingsEditor.tsx` | 直接回迁并适配 ResourceSlot binding |
| `CanvasRuntimePreview.observation.ts` | 直接回迁并适配 renderer lease/generation |
| `CanvasRuntimePreview.runtime.ts` | 直接回迁并替换为 MessageChannel SDK v1 |
| `CanvasRuntimePreview.tsx` | 直接回迁并适配 user/attachment Operation host |
| `CanvasRuntimePreview.test.ts` | 直接回迁全部行为场景并更新合同 |
| `CanvasRuntimePanel.tsx` | 以 legacy 完整 UI 为主合并 current Interaction component |
| `ProjectCanvasManager.tsx` | 以 legacy 资产管理能力为基线合并 current routes |
| `extension-runtime/model/canvasBridge.ts` | 回迁 channel 行为，执行统一映射 Extension Operations |
| `ExtensionCanvasPanel.tsx` | 回迁 Canvas Extension renderer 能力并复用新 SDK |
| `canvasModuleOpen.ts` | 以当前 interaction URI 为主补齐 legacy open 行为 |
| `canvas-tab.tsx` | 以当前 tab 为主接入完整 runtime host |
| `services/canvas.ts` / test | 以当前 Interaction service 为主补齐完整能力与错误测试 |
| `types/canvas.ts` | 删除手写旧模型，消费生成合同 |

## Skill / Tests

- `canvas-system/SKILL.md` 与六个 legacy references 全部作为能力清单回迁，并按新 API 重写。
- `agentdash-test-support/src/workspace_module.rs` 恢复等价 fixtures，identity 改为 definition/instance。
- `tests/e2e/canvas-promote-extension.spec.ts` 保留并更新 revision identity。
- 新增 standalone user、Agent authoring、attached interaction、Operation revoke、asset revoke、
  renderer diagnostics、queue/steer parity tests。

## RuntimeGateway

`agentdash-application-runtime-gateway`、bootstrap、setup/session action、MCP surface 不直接复制。
其以下机制逐项进入 Operation/Canvas host 测试：

- actor-bound surface discovery；
- provider readiness/schema；
- request/result/trace/timeout/cancel；
- MCP、Extension channel/backend service 调用；
- current authority revoke；
- no credential/placement leakage。

## AgentFrame / Product surface

| 历史文件或提交 | 处理 |
| --- | --- |
| reference `agent_run/frame/surface_service.rs` | 硬搬作 create/attach/binding 副作用与测试基线 |
| reference `agent_run/runtime_surface_update.rs` | 硬搬作 module ref、VFS mount、immutable frame 行为基线；不恢复 direct adopter |
| `ef4cc2499:agentdash-application/src/product_runtime_surface_update.rs` | 作为 current Product CAS/rebind 接线参考 |
| `708f234c9:workspace_module_product.rs` | 作为 Workspace Product tool → frame convergence 纵向参考 |
| current `agent_frame_materialization.rs` ports | 评估后收束为 create/attach 所需稳定 port，不能保留无实现声明 |

最终 `canvas.create` 与 `canvas.copy` 复用 create Frame command，`canvas.attach` 使用 attach Frame
command。`workspace_module_present`、`canvas.bind_data`、source edit 与 Interaction attachment
不写 AgentFrame。

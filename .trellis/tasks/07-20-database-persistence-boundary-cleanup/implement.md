# 数据库持久化边界清理实施计划

## Phase 1 — 固定 canonical JSONB 回归

- [x] 将 operation Accepted → Running 回归测试改为只通过
  `ManagedRuntimeStateRepository`验证revision与重启重放。
- [x] 增加 Runtime Create change 早于 Product binding 的回归场景。
- [x] 增加 Product observer独立cursor场景，固定per-consumer持久化语义。
- [x] Host/Callback repository roundtrip只依赖canonical revision facts。

## Phase 2 — Runtime Repository 去镜像

- [x] 删除 `replace_runtime_projection` 的normalized写入。
- [x] 删除 Runtime projection/prefix/drift verification。
- [x] `load`/`commit`只读写 `agent_runtime_state_revision`。
- [x] change/outbox读取改用canonical facts。
- [x] 删除测试中直接seed Runtime normalized rows的fixture。

## Phase 3 — Product Delivery 回归 Product owner

- [x] Product Runtime binding schema/model增加独立`change_delivery_state` JSONB，不进入binding digest。
- [x] 用Product binding row上的per-consumer claim/cursor替换
  `PostgresManagedRuntimeProductChangeDelivery`。
- [x] worker按已绑定Product target与单consumer读取Runtime facts outbox。
- [x] 删除全局delivery表、共同ack和无binding warning路径。

## Phase 4 — Host / Callback 去镜像

- [x] `PostgresCompleteAgentHostRepository`只读写`agent_runtime_host_revision`。
- [x] `PostgresCompleteAgentCallbackRepository`只读写`agent_runtime_callback_revision`。
- [x] 删除Host/Callback normalized写入、prefix和drift verification。
- [x] Product activation 不读取 Host persistence；Host-owned流程通过typed Host snapshot访问
  coordination facts。
- [x] 删除依赖normalized Host rows的test fixture与readiness检查。

## Phase 5 — Migration Hard Cut

- [x] 新增下一序号migration，增加Product delivery JSONB并删除镜像FK/表。
- [x] 保留canonical owner facts并为既有Product binding初始化空delivery state。
- [x] 更新schema readiness与migration guard。
- [x] 通过fresh embedded PostgreSQL验证完整migration序列与最终readiness。

## Phase 6 — 规范与领域语言

- [x] 更新`CONTEXT.md`为Product、Managed Runtime、Host coordination与Complete Agent四owner语言。
- [x] 更新Runtime persistence、Driver Host与AgentRun facade specs，固定JSONB canonical authority及
  Product-owned consumer cursor。
- [x] 文档只记录最终owner、事务与恢复理由。

## Phase 7 — Product Binding Canonical Document

- [x] Product binding digest改用带schema identity的递归canonical JSON。
- [x] binding commit与frame replacement返回committed receipt，launch replay 保持 Product
  intent 幂等。
- [x] PostgreSQL repository只从`binding JSONB`解码并复验digest/coordinate。
- [x] migration清理旧digest attestation并删除execution profile/source revision镜像列。
- [x] 多键JSONB roundtrip、binding replay与repository restart定向回归通过。

## Phase 8 — Command Revision Boundary

- [x] 删除 `ManagedRuntimeCommandEnvelope` 与 Product/API command request 的通用
  `expected_revision`。
- [x] Runtime command admission 使用 current facts/availability；repository revision 只留在
  `ManagedRuntimeStateCommit` 内部 CAS。
- [x] Product launch、recovery、surface update、workflow 与 mailbox delivery 只冻结稳定
  operation/idempotency/binding identity。
- [x] 更新前端 command service、生成契约与定向回归，证明无关 projection revision 不再拒绝命令。

## Phase 9 — Product / Host Surface Evidence Boundary

- [x] Product activation 不读取 Host aggregate，Host evidence 保持 Host-owned。
- [x] 删除 Product binding row 与 tool-binding DTO 中复制的 Host binding id/generation。
- [x] Product tool authorization 不比较 Product surface-facts digest 与 Host compiled-surface
  digest，只校验共享 RuntimeThread/source/surface revision coordinate。
- [x] 新增 `0092_product_host_pin_boundary.sql` forward migration，保证已执行 `0091` 的开发库
  自动升级。

## Phase 10 — AgentFrame-derived Product Runtime Authority

- [x] Product authority resolver 只读取 binding 精确引用的 AgentFrame，不读取 latest frame。
- [x] VFS、workspace module、API surface 与 runtime tool authorization 改为即时编译 Product
  authority。
- [x] launch、recovery、surface update 删除 resource materialize/query phase，Product
  activation 只提交 canonical binding。
- [x] 删除 applied-resource-surface repository/materializer 和工具证据中的 snapshot/vfs
  revision。
- [x] 新增 `0093_agent_frame_runtime_authority.sql` 删除两张全局 surface 表、Product snapshot
  pin，并清理旧 recovery saga phase。
- [x] 完成 migration guard、定向测试与最终格式化检查。

## Phase 11 — Runtime evidence single writer

- [x] Product Runtime binding 删除 source/applied/activation evidence，digest 只覆盖 Product intent。
- [x] launch/activation 与 recovery saga 删除 Runtime evidence 回写和复制阶段。
- [x] command/currentness 校验改为 AgentFrame revision 对 Runtime applied surface 的单向派生校验。
- [x] AgentRun list 使用弱 presentation read；Runtime summary 不可用不影响 Product list。
- [x] Workspace presentation provenance 收窄为 source coordinate + surface revision。
- [x] 新增 `0094_runtime_evidence_single_writer.sql`，清理旧 binding/saga/presentation 文档并
  删除 source revision 拆分列。

## Phase 12 — AgentFrame owner-local JSONB

- [ ] 将 AgentFrame repository seam 收口为 agent-scoped exact/latest/history lookup。
- [ ] 将 frame revision history 与 canonical surface document 迁入 LifecycleAgent owner JSONB。
- [ ] 删除无独立生命周期的 frame transition/split storage，并以 owner document 删除替代跨表级联。

## Focused Validation

```powershell
cargo test -p agentdash-infrastructure runtime_operation_status_advancement_reloads_from_canonical_document -- --nocapture
cargo test -p agentdash-infrastructure runtime_outbox_without_product_binding_is_not_delivery_work -- --nocapture
cargo test -p agentdash-infrastructure postgres_activation_persists_canonical_binding_across_repository_restart -- --nocapture
cargo check -p agentdash-agent-runtime -p agentdash-agent-runtime-host -p agentdash-infrastructure -p agentdash-api
node scripts/check-migration-history.js
git diff --check
```

测试名称会随删除旧projection/delivery术语同步改名。只运行受影响定向测试和必要check，不重复运行
无关全量套件。

## Review Gates

1. Runtime repository生产路径只访问`agent_runtime_state_revision`。
2. Product change delivery没有无binding错误，且每consumer独立推进。
3. Host/Callback repository生产路径只访问各自canonical revision表。
4. Product/Infrastructure无直接JOIN Host normalized镜像。
5. 最终schema和readiness不包含被删除表。
6. migration、restart replay、CAS/idempotency/stale claim定向证据通过。
7. Product binding只从canonical JSONB解码；digest经过JSONB roundtrip稳定，AgentFrame 是
   Product runtime authority 的精确surface identity。
8. Runtime/Product/API command contract 不暴露 aggregate revision；并发仍由内部CAS和具体业务
   coordinate保证。
9. Product runtime authority 没有 repository/materialize phase；Task association 撤销可在下次
   工具授权时立即生效。

## Rollback Point

项目未上线，不建立运行时回退。若migration或repository切换未通过，修正同一forward migration和
代码后在隔离数据库重跑；不恢复normalized双写或兼容reader。

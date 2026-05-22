# Capability 维度管线标准化执行计划

## Phase Goals

### Phase 1: Spec And Registry Contract

目标：先定义“稳定主干 + 维度模块”的协议。

- [ ] 新增或更新 capability spec，定义 declaration / contribution / effect / projection / dimension module。
- [ ] 写入现有维度矩阵，并标注 built-in module / projection-only module / future module。
- [ ] 更新 session startup/runtime specs，说明 runtime command payload 保存 records，不保存 per-dimension 顶层字段。
- [ ] 写入 extension/plugin 接入边界：extension 新能力产出 declaration/effect records 或注册 dimension module。
- [ ] 写入 ordering 规则：registry 集中维护维度顺序。

完成标准：

- future agent 能判断新增能力应该注册 runtime module，还是 projection-only module。
- spec 以正向契约说明：新增能力通过 record envelope 与 dimension module 注册接入，主干只维护 envelope、ordering、dispatch 与 projection 汇聚。

### Phase 2: Built-in Dimension Modules

目标：先把现有维度核心解析与 replay 逻辑拆到模块，避免 envelope 只变成新外壳。

- [ ] 新增 dimension module 目录或文件边界。
- [ ] 拆出 Tool dimension module：decode tool declaration/effect payload，replay tool access。
- [ ] 拆出 MCP dimension module：decode server-set effect payload，replay MCP servers。
- [ ] 拆出 Companion dimension module：decode roster effect payload，replay companion agents。
- [ ] 拆出 VFS dimension module：decode VFS overlay / mount operation effect payload，replay VFS changes。
- [ ] 将现有 overlay merge、mount operation application、MCP set、companion roster set、tool access set 从主 replay helper 迁入 modules。
- [ ] 为 built-in module 增加 typed payload decode / validation 测试，覆盖缺字段、未知 effect type、payload 类型不匹配等失败路径。

完成标准：

- 核心解析/replay 逻辑已经模块化。
- 旧主 replay helper 内不再直接持有各维度业务分支。
- envelope 的 `serde_json::Value` 在 module 边界被转换为强类型 payload，replay 内部不直接操作裸 JSON。

### Phase 3: Envelope Payload Types And Registry

目标：引入 record/envelope payload，并用 registry 串起 modules。

- [ ] 新增 `CapabilityDimensionKey`。
- [ ] 新增 `CapabilityArtifactSource`。
- [ ] 新增 `CapabilityDeclarationRecord`。
- [ ] 新增 `RuntimeCapabilityEffectRecord`。
- [ ] 新增 `RuntimeCapabilityTransition { declarations, effects }`。
- [ ] 新增 `CapabilityDimensionModule` trait 或等价内部接口。
- [ ] 新增 `CapabilityDimensionRegistry`，集中维护 module map 与 ordering。
- [ ] projection-only module 先记录 spec / scaffold，避免本轮过度重写 Skill/guideline。
- [ ] 将 `replay_runtime_context_patch` 改为 registry dispatch。
- [ ] 新增统一 fold replay 入口，按 runtime command store 返回顺序应用所有 requested transitions。
- [ ] 将 pending transition payload 从 `patch: RuntimeContextPatch` 迁移为 `transition: RuntimeCapabilityTransition` 或等价命名。

完成标准：

- replay 主干只遍历 effect records。
- 新增维度无需修改 transition struct。
- construction / context query / next-turn launch / pending apply event 能复用同一个 replay 结果类型。

### Phase 4: Replace Production Chain

目标：删除旧 runtime context patch 生产链路，直接用新 transition records 重写回去。

- [ ] `StepActivation` / workflow pending path 生成 declaration records。
- [ ] `StepActivation` / resolver output 生成 runtime effect records。
- [ ] 更新 construction/prompt pipeline 对 pending MCP/VFS 的读取方式，通过 registry context 获取顺序 fold 后的 effect replay 结果。
- [ ] 移除 production callsite 对最后一个 pending command 的专用读取逻辑，改为统一 replay 所有 requested transitions。
- [ ] 更新 hub pending transition input/output，持久化 `RuntimeCapabilityTransition`。
- [ ] 移除或重命名 `RuntimeContextPatch`、`RuntimeToolIntent`、`RuntimeMcpIntent`、`RuntimeCompanionIntent`、`RuntimeVfsIntent` 生产类型。
- [ ] 移除旧 `apply_runtime_context_patch` / `replay_runtime_context_patch` 生产入口。
- [ ] 更新 hub / repository / launch / assembler tests fixtures。
- [ ] 保持 live `after_state` 只用于 event diff / connector hot update。

完成标准：

- production 代码不存在 full projection -> runtime payload 反推路径。
- pending payload 可追溯 declarations，也能稳定 replay effects。
- 生产代码唯一 replay 入口是 registry-dispatched `RuntimeCapabilityTransition`。
- 多个 pending transitions 的 VFS/mount operation 不会因为只读取最后一个 command 而丢失。

### Phase 5: Tests And Review Gates

目标：用测试和搜索门禁证明边界真的收住。

- [ ] serialization test 断言 payload 有 `declarations` / `effects` records，且没有 final projection cache。
- [ ] replay test 断言 registry dispatch 生成等价 final projection。
- [ ] replay fold test 断言多个 pending transitions 按顺序叠加 VFS/mount effects。
- [ ] repository/runtime/context 聚焦测试更新到新 JSON shape。
- [ ] 增加 search gate，覆盖旧字段与 per-dimension 顶层 payload 反模式。
- [ ] 运行 Rust 聚焦验证。

完成标准：

- 新旧行为等价。
- payload 结构支持新增维度模块，不引入数据库 schema churn。

## Validation Commands

```bash
cargo test -p agentdash-application runtime_capability_transition --lib
cargo test -p agentdash-application runtime_command_store --lib
cargo test -p agentdash-application pending_capability_state_transition --lib
cargo test -p agentdash-application pending_runtime_context_transition --lib
cargo test -p agentdash-application session::construction --lib
cargo test -p agentdash-application prompt_pipeline --lib
cargo test -p agentdash-application session::launch --lib
cargo test -p agentdash-api session_context
cargo check -p agentdash-application
cargo check -p agentdash-api
python ./.trellis/scripts/task.py validate .trellis/tasks/05-22-capability-intent-pipeline-standardization
git diff --check
```

如果前端 DTO 或 generated types 受影响，再补：

```bash
pnpm --filter app-web typecheck
pnpm --filter app-web lint
```

## Review Gates

- 新能力维度必须通过 dimension module 接入，不允许在主干 payload struct 增加专用字段。
- runtime command payload 不允许保存 final `CapabilityState`、runtime surface、skill baseline、guideline projection。
- replay 入口只能遍历 effect records，并由 registry 分发。
- construction / context query / next-turn launch / pending apply event 必须共用同一个 transition fold replay 入口。
- built-in module 必须在 module 边界 decode typed payload 并 validation，业务 replay 内不直接消费裸 `serde_json::Value`。
- declarations 可以用于审计和后续迁移，但不能绕过 resolver/normalizer 直接拼 final projection。
- dimension ordering 必须在 registry/spec 集中声明。
- plugin/extension 新能力必须产出 records 或注册 module，不能要求主干 DTO 扩字段。

## Risky Files

- `crates/agentdash-application/src/session/types.rs`
- `crates/agentdash-application/src/session/capability_state.rs`
- `crates/agentdash-application/src/session/prompt_pipeline.rs`
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
- `crates/agentdash-application/src/session/hub/tests.rs`
- `crates/agentdash-application/src/session/launch.rs`
- `crates/agentdash-application/src/session/memory_persistence.rs`
- `crates/agentdash-application/src/workflow/agent_executor.rs`
- `crates/agentdash-application/src/workflow/step_activation.rs`
- `crates/agentdash-api/src/bootstrap/session_construction_bootstrap.rs`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/tasks/04-12-plugin-extension-api/dynamic-installation-discussion.md`

## Rollback Points

- Phase 1 后只更新 spec，不触碰代码。
- Phase 2 后若 envelope serde 影响面超出预期，保留新 records 但暂停 callsite wiring。
- Phase 3 后若 registry dispatch 不等价，优先修对应 module，不恢复 per-dimension payload 字段。

# Implement · Workspace Module 读路径与单一契约

> 依据 `design.md`。有序执行，每步后跑对应验证。运行代码已 `in_progress`。

## 步骤

1. **契约 DTO**（`crates/agentdash-contracts/src/workspace_module.rs`）
   - 定义 §2 全部类型，`#[derive(Serialize, Deserialize, TS)]`，Uuid→String。
   - 在 contracts `lib.rs`/mod 注册模块。
   - `generate_ts.rs` 加 `export_all::<...>`（全部新类型）。
   - 验证：`cargo build -p agentdash-contracts && pnpm contracts:check`

2. **聚合层**（`crates/agentdash-application/src/workspace_module/mod.rs`）
   - `build_workspace_modules(ext, canvases)`：extension→module、canvas→module、channel-as-operation（§3/§4）。
   - module_id 约定 §4。bundles 缺失→status unavailable。
   - 单测：构造 fixture projection + canvas，断言 module 数、operation 含 channel method、module_id 格式。
   - 验证：`cargo test -p agentdash-application workspace_module`

3. **Capability 维度**（spi）
   - `WorkspaceModuleVisibility{ mode, allowed_module_ids }` 加入 `CapabilityState`（模板 SkillDimension）。默认 `all`。
   - launch 组装填充（默认 all；读 AgentFrame 字段若非空→allowlist）。
   - 验证：`cargo test -p agentdash-application capability`

4. **AgentFrame 预留字段**（`crates/agentdash-domain/src/workflow/agent_frame.rs`）
   - `visible_workspace_module_refs_json` + getter + append，镜像 `visible_canvas_mount_ids_json`；不随 revision 复制。
   - 单测镜像现有 canvas 字段测试。
   - 验证：`cargo test -p agentdash-domain agent_frame`

5. **Agent 工具**（`crates/agentdash-application/src/workspace_module/tools.rs`）
   - `WorkspaceModuleListTool` / `WorkspaceModuleDescribeTool`（样板 ListCanvasesTool）。
   - `RelayRuntimeToolProvider::build_tools` 加新 capability tool flag 门控、注册二工具。
   - 工具内 `project_id_from_context` → repos 取 enabled installations + visible canvases → `build_workspace_modules` → §6 可见性过滤。
   - describe 未知 module_id → 结构化错误。
   - 验证：`cargo test -p agentdash-application workspace_module`

6. **D6 删除死字段**
   - 删 `FrameLaunchIntent.extension_runtime`（runtime_launch.rs）、`ConstructionProjections.extension_runtime`（construction.rs）及 frame_construction/mod.rs:381 的 `extension_runtime: None`、plan.rs 测试 fixture 引用。
   - 验证：`cargo build -p agentdash-application`（无残留引用）

7. **全量验证**
   ```powershell
   cargo build -p agentdash-contracts
   cargo test -p agentdash-domain agent_frame
   cargo test -p agentdash-application workspace_module
   cargo test -p agentdash-application capability
   pnpm contracts:check
   pnpm --filter @agentdash/app-web typecheck
   ```

## 回滚点

- 每步独立可回退；步 6（删字段）若发现仍有生产消费则改为"接管填充"而非删除（design §7 备选）。

## Review gate

- 步 1/2 完成后先确认 DTO shape（影响 Child 2/3）再继续。

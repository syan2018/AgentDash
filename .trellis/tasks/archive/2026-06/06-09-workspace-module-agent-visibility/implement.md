# Implement · ProjectAgent Workspace Module 可见性配置

> 依据 design.md。改动并入分支 feat/workspace-module-registry（PR #45）。

## 步骤

1. **domain 字段**（`crates/agentdash-domain/src/common/agent_config.rs`）
   - `AgentPresetConfig` 加 `visible_workspace_module_refs: Option<Vec<String>>`（serde skip_if none）。
   - `merge_field!` 调用加该字段。补任何因新字段报错的构造点。
   - 验证：`cargo build -p agentdash-domain`

2. **frame construction 填充**（`crates/agentdash-application/src/workflow/frame_construction/` + construction_planner / assembler）
   - 定位 `visible_canvas_mount_ids` 从 agent preset 写入 frame 的路径，镜像之，从 `preset_config.visible_workspace_module_refs` 写 `visible_workspace_module_refs_json`。
   - None/空→不写；非空→写 refs。不改 mod.rs:364 下游解析。
   - 单测：给定 preset 含 refs → 构造的 frame.visible_workspace_module_refs() == refs；空 preset → 空。
   - 验证：`cargo test -p agentdash-application workspace_module && cargo test -p agentdash-application -- frame_construction`

3. **contracts**（project-agent preset DTO + ts）
   - create/update preset DTO 加字段；`generate_ts` 同步。
   - 验证：`cargo build -p agentdash-contracts && pnpm contracts:check`

4. **前端 form-state + picker**（`packages/app-web/src/features/project/agent-preset-editor/`）
   - `PresetFormState` 加 `visible_workspace_module_refs: string[]`；form↔config 转换补字段。
   - preset-form-fields.tsx 加 Workspace Module Visibility 区块，用 `useProjectWorkspaceModules`/`fetchProjectWorkspaceModules`（Child 3）列 module 多选；空=全部可见提示。
   - 验证：`pnpm --filter app-web typecheck`

5. **全量验证**
   ```powershell
   cargo build --workspace
   cargo test -p agentdash-domain agent_config
   cargo test -p agentdash-application workspace_module
   pnpm contracts:check
   pnpm --filter app-web typecheck
   pnpm --filter app-web test
   ```

## 回滚点

- 步 1/2 后端先通再做前端。
- 默认行为必须保持"未配置=全集"，回归现有 agent 不受影响。

## Review gate

- 步 1-2（config→frame 闭环）后端验证通过后确认再做前端。

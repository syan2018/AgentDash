# Research: AgentFrame DB 列（visible_workspace_module_refs_json 加列样板）

- **Query**: migrations 目录 / 命名规范 / lifecycle_anchor 表列 / FrameRow 读写 / migration guard 规则
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `crates/agentdash-infrastructure/migrations/` | migration 目录（已编号 0001–0007） |
| `crates/agentdash-infrastructure/migrations/0001_init.sql:40-56` | `CREATE TABLE agent_frames`（含 `visible_canvas_mount_ids_json text`） |
| `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` | FrameRow + SELECT/INSERT/UPDATE 全部读写点 |
| `crates/agentdash-domain/src/workflow/agent_frame.rs` | `AgentFrame` 域结构 + `visible_workspace_module_refs()` / `append_visible_workspace_module_ref()`（已实现） |
| `scripts/check-migration-history.js` | migration 历史守卫 |

### migration 目录 + 命名规范

现有文件（升序，4 位编号 + 下划线 + snake 描述）：
```
0001_init.sql
0002_runtime_session_anchor_fks.sql
0003_lifecycle_orchestration_contract.sql
0004_orchestration_runtime_convergence.sql
0005_remove_builtin_mcp_server_templates.sql
0006_library_asset_source_integration_embedded.sql
0007_library_asset_integration_source_ref.sql
```
**新增列 → 新建 `0008_<desc>.sql`**，例如 `0008_agent_frame_visible_workspace_module_refs.sql`，内容：
```sql
ALTER TABLE agent_frames ADD COLUMN visible_workspace_module_refs_json text;
```
（与 `visible_canvas_mount_ids_json` 同类型 `text`，nullable，无默认。0001_init.sql:55 即此列。）

### agent_frames 表当前列（0001_init.sql:40-56）

```sql
CREATE TABLE agent_frames (
    id text NOT NULL,
    agent_id text NOT NULL,
    revision integer DEFAULT 1 NOT NULL,
    procedure_id text,
    -- effective_capability_json / context_slice_json / vfs_surface_json / mcp_surface_json text
    created_by_kind text NOT NULL,
    created_by_id text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    execution_profile_json text,
    visible_canvas_mount_ids_json text   -- ← 样板列
);
```
PK：`agent_frames_pkey (id)`（行 825）。索引 `idx_agent_frames_agent_id`、唯一 `idx_agent_frames_agent_revision (agent_id, revision)`（行 1034-1036）。FK `agent_id → lifecycle_agents(id)`（行 1212）。

### FrameRow 读写点（lifecycle_anchor_repository.rs）

**struct FrameRow（行 163-187）** —— `visible_canvas_mount_ids_json: Option<String>`（行 173）。
新增需加字段 `visible_workspace_module_refs_json: Option<String>`。

**TryFrom<FrameRow> for AgentFrame（行 188-216）** —— 当前硬编：
```rust
// 行 206-209: visible_canvas 正常 parse
visible_canvas_mount_ids_json: parse_opt_json(row.visible_canvas_mount_ids_json, "visible_canvas_mount_ids_json")?,
// 行 210-212: workspace module 硬编 None（Child 1 占位）
visible_workspace_module_refs_json: None,
```
**改法**：把 None 换成
```rust
visible_workspace_module_refs_json: parse_opt_json(row.visible_workspace_module_refs_json, "visible_workspace_module_refs_json")?,
```

**INSERT（行 233-249）** —— 列清单 + `VALUES ($1..$12)` + `.bind(...)`：
当前 12 列：`id, agent_id, revision, effective_capability_json, context_slice_json, vfs_surface_json, mcp_surface_json, visible_canvas_mount_ids_json, execution_profile_json, created_by_kind, created_by_id, created_at`。
加列后改成 13 列（加 `visible_workspace_module_refs_json`）+ `$13` + 新增一行 `.bind(opt_json_str(&frame.visible_workspace_module_refs_json)?)`（紧跟 canvas 那行，行 248 之后）。

**SELECT × 3** —— `get`（行 260-266）、`get_current`（行 277-283）、`list_by_agent`（行 294-300）三处的 `SELECT ... visible_canvas_mount_ids_json, ...` 列清单都要加 `visible_workspace_module_refs_json`，否则 `FromRow` 解不出新字段会 panic。

**UPDATE 写入样板（append_visible_canvas_mount，行 311-331）**：
```rust
async fn append_visible_canvas_mount(&self, frame_id, mount_id) -> Result<(), DomainError> {
    let mut frame = self.get(frame_id).await?.ok_or(NotFound{...})?;
    frame.append_visible_canvas_mount(mount_id);
    sqlx::query("UPDATE agent_frames SET visible_canvas_mount_ids_json=$1 WHERE id=$2")
        .bind(opt_json_str(&frame.visible_canvas_mount_ids_json)?)
        .bind(frame_id.to_string()).execute(&self.pool).await.map_err(db_err)?;
    Ok(())
}
```
若 R2 要在 frame 上写 module refs（裁切落地），可仿此加 `set_visible_workspace_module_refs` / `append_visible_workspace_module_ref` 方法（域层 `agent_frame.rs:112` 的 `append_visible_workspace_module_ref` 已存在），需先在对应 trait（domain 侧 `LifecycleAnchorRepository`-类）补方法签名。

### 域层已就绪（agent_frame.rs）

- 字段 `visible_workspace_module_refs_json: Option<Value>`（行 31）。
- `visible_workspace_module_refs() -> Vec<String>`（行 103）、`append_visible_workspace_module_ref(&str)`（行 112，去重 + skip blank）。
- `new_initial` / `new_revision` 均初始化为 None（行 51、69）；revision 不携带（测试 156）。

### capability 通道已接（R2 上游已就绪）

`crates/agentdash-application/src/workflow/frame_construction/mod.rs:363-370`：
```rust
let visible_module_refs = frame.visible_workspace_module_refs();
capability_state.workspace_module = if visible_module_refs.is_empty() {
    WorkspaceModuleDimension::default()   // = All
} else {
    WorkspaceModuleDimension { mode: Allowlist, allowed_module_ids: visible_module_refs ... }
};
```
SPI 维度 `crates/agentdash-spi/src/connector/mod.rs:286-337`：`WorkspaceModuleDimension { mode, allowed_module_ids }` + `.allows(module_id)`。
即**只要 DB 列能读出非空数组，裁切就自动经 capability 生效**——R2 的下游链路已通，本 child 只需补 (a) DB 列读写、(b) UI 编辑写入 frame refs 的入口。

### migration guard 规则（scripts/check-migration-history.js）

- 守卫只检查 staged diff 中 `crates/agentdash-infrastructure/migrations/` 下文件。
- 禁止：**修改/删除/重命名已提交（tracked）的 migration**（status 非 `A`，且文件存在于 `merge-base(HEAD, origin/main)`）→ exit 1。
- 允许：**新增（status `A`）migration**（行 88 `if (status.startsWith("A")) continue;`）。
- 允许：把已改文件**还原**到 base ref 内容（行 90-97）。
- 逃生阀：`ALLOW_MIGRATION_BASELINE_REWRITE=1`（仅授权 baseline squash 任务）。
- **结论**：本 child 加列必须**新建 0008**，绝不能改 0001_init.sql。

## Caveats / Not Found

- `parse_opt_json` / `opt_json_str` 是文件内辅助函数（lifecycle_anchor_repository.rs 内已用，未单独贴），照搬 canvas 列写法即可。
- domain 侧 Repository trait（声明 `append_visible_canvas_mount`）位置未在本轮定位；如需新增 module-refs 写方法，需同步 trait + 任何 in-memory/mock 实现。

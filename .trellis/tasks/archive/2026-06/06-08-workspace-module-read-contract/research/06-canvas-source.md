# Research: Canvas 源 (entity + application 层按 project 列出)

- **Query**: Canvas 字段；application 层如何按 project 列 visible canvas
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### Canvas 实体

`crates/agentdash-domain/src/canvas/entity.rs` L8-21：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub id: Uuid,
    pub project_id: Uuid,
    pub mount_id: String,
    pub title: String,
    pub description: String,
    pub entry_file: String,
    pub sandbox_config: CanvasSandboxConfig,
    pub files: Vec<CanvasFile>,
    pub bindings: Vec<CanvasDataBinding>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

`Canvas::new` (L24-43) 种入默认 entry/skill 文件；`touch()` (L45-47)。
值对象（`CanvasDataBinding / CanvasFile / CanvasSandboxConfig`）来自
`super::value_objects`。

### 按 project 列出 canvas

- **application 用例**：`crates/agentdash-application/src/canvas/management.rs`
  `list_project_canvases(repos, project_id) -> Result<Vec<Canvas>, ApplicationError>` (L32-41)
  = `repos.canvas_repo.list_by_project(project_id)`。re-export 经 `canvas/mod.rs` L9。
- **visibility 裁切**：`crates/agentdash-application/src/canvas/visibility.rs`
  `append_visible_canvas_mounts(canvas_repo, project_id, vfs, visible_mount_ids)` (L14-37)：
  先 `canvas_repo.list_by_project(project_id)` (L30)，再按 `visible_mount_ids` 过滤
  `canvas.mount_id`，最后 `append_canvas_mounts(vfs, &visible)` (L35)。
  注释 L11-13：「默认不注入任何 canvas，只有会话里记录过的 mount_id 才会被追加」——
  这是 visible 裁切语义的现成范本。
- **工具内列出**：`ListCanvasesTool::execute`（canvas/tools.rs L171-213）同样调
  `list_by_project` 并排序输出（见 02 文档样板）。

`CanvasRepository::list_by_project` 是仓储 trait 方法（`agentdash_domain::canvas`）；
repos.canvas_repo 是 `Arc<dyn CanvasRepository>`。

## Caveats / Not Found

- 没有“project 级 visible canvas”单独 API；visible 概念由
  `visibility.rs::append_visible_canvas_mounts` + AgentFrame `visible_canvas_mount_ids_json`
  共同表达（见 05 文档）。

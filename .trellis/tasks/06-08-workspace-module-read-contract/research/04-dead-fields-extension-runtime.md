# Research: 死字段收口点 (extension_runtime on FrameLaunchIntent / ConstructionProjections)

- **Query**: FrameLaunchIntent.extension_runtime 与 ConstructionProjections.extension_runtime 的全部读/写位置；frame_construction 填 None 处；construction 能否拿 enabled installations
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### 字段定义（两处声明）

1. `FrameLaunchIntent.extension_runtime: Option<ExtensionRuntimeProjection>`
   `crates/agentdash-application/src/workflow/runtime_launch.rs` L79（struct L72-80，`#[derive(Default)]`）。
2. `ConstructionProjections.extension_runtime: Option<ExtensionRuntimeProjection>`
   `crates/agentdash-application/src/session/construction.rs` L145（struct L139-146，import L18）。

### 全部写/读位置

| 位置 | 行为 | file:line |
|---|---|---|
| `frame_construction/mod.rs` `build_envelope_from_frame` | 写 `extension_runtime: None`（FrameLaunchIntent 唯一真实构造点） | `workflow/frame_construction/mod.rs:381` (struct L375-382) |
| `session/launch/plan.rs` test `envelope_from_construction` | 写 `extension_runtime: construction.projections.extension_runtime`（**测试 fixture**） | `session/launch/plan.rs:456` |
| `construction.rs` `RuntimeContextInspectionPlan::new` | `ConstructionProjections { session_capabilities: ..., ..Default::default() }` → extension_runtime 默认 None（从不显式赋值） | `session/construction.rs:203-206` |

`construction.rs` 的 `ConstructionProjections` 只在 `RuntimeContextInspectionPlan::new`
里以 `..Default::default()` 间接产出（extension_runtime 永远 None）；该 plan 类型注释
标为「测试 fixture」(L66-67)。

**结论**：
- 生产链路里 `FrameLaunchIntent.extension_runtime` **只被写 None**（mod.rs:381），无任何读取消费点。
- `ConstructionProjections.extension_runtime` 仅在 plan.rs 测试 fixture 中被读一次，生产无写入。
- 两个字段当前都是死字段（write-None / 仅测试读）。

### construction 阶段能否拿到 enabled installations

能。`FrameConstructionService` 持有 `repos: RepositorySet`
(`workflow/frame_construction/mod.rs:47`)。`RepositorySet` 含
`project_extension_installation_repo`（见 extension_runtime.rs L339 用法），其
`list_enabled_by_project(project_id)` 返回 enabled installations
（trait `ProjectExtensionInstallationRepository`，
`agentdash_domain::shared_library`）。

construct_launch_envelope 内已解析出 `agent` / `run`，`run.project_id` /
`agent.project_id` 可用（`mod.rs` L94-122），因此可在此查 enabled installations 并经
`extension_runtime_projection_from_installations(...)` 生成 projection。

### build_envelope_from_frame 关键结构（填 None 点上下文）

`workflow/frame_construction/mod.rs:301-393`：

```rust
Ok(FrameLaunchEnvelope {
    surface,
    intent: FrameLaunchIntent {
        input,
        environment_variables,
        identity: command.identity(),
        terminal_hook_effect_binding: hook_binding,
        discovered_guidelines: Vec::new(),
        extension_runtime: None,   // L381
    },
    ...
})
```

注意：这是 free-standing fn，不持有 `&self`/repos，所以若要在此填充需要把数据从
`construct_launch_envelope`（持 repos）一路传入，或改为在 service 方法内填充。

## Caveats / Not Found

- `discovered_guidelines` 在同一处也被写 `Vec::new()`（L380）——同样是“此处无数据可填”的占位，
  与 extension_runtime 同构。如要收口可参考其在 plan.rs envelope 中如何流转。

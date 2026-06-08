# Research: AgentFrame visible_canvas_mount_ids 模板 + CapabilityState 链路

- **Query**: visible_canvas_mount_ids_json 字段/getter/append 写法；CapabilityState / effective_capability_json 定义与 launch 解析
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### AgentFrame visible_canvas_mount_ids_json（新增 visible module 裁切字段同构模板）

`crates/agentdash-domain/src/workflow/agent_frame.rs`。

字段声明 (L24-26)：

```rust
/// 当前可见的 Canvas mount ids（运行时追加，不随 revision 复制）。
#[serde(default, skip_serializing_if = "Option::is_none")]
pub visible_canvas_mount_ids_json: Option<serde_json::Value>,
```

- struct `AgentFrame` (L9-31) 全部 surface 字段都是 `Option<serde_json::Value>`，
  带 `#[serde(default, skip_serializing_if = "Option::is_none")]`。
- 构造器 `new_initial` (L34-49) / `new_revision` (L51-66) 都把该字段初始化为 `None`。

getter + append（L68-93，**这是新增字段最直接的模板**）：

```rust
pub fn visible_canvas_mount_ids(&self) -> Vec<String> {
    let Some(Value::Array(ids)) = &self.visible_canvas_mount_ids_json else {
        return Vec::new();
    };
    ids.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
}

pub fn append_visible_canvas_mount(&mut self, mount_id: &str) {
    if mount_id.trim().is_empty() { return; }
    let already = self.visible_canvas_mount_ids().iter().any(|e| e == mount_id);
    if already { return; }
    let next = Value::String(mount_id.to_string());
    match &mut self.visible_canvas_mount_ids_json {
        Some(Value::Array(ids)) => ids.push(next),
        _ => self.visible_canvas_mount_ids_json = Some(Value::Array(vec![next])),
    }
}
```

注意注释「运行时追加，不随 revision 复制」——`new_revision` 不携带它（每个新 revision 重置为 None）。

### CapabilityState 定义

`crates/agentdash-spi/src/connector/mod.rs` L281-292（doc L269-280 说明数据流：写经
`AgentFrameBuilder::with_capability_state` → AgentFrame revision；读经内存缓存；
投影 `project_capability_state_from_frame`）：

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityState {
    pub tool: ToolDimension,           // L284
    pub companion: CompanionDimension, // L286
    pub vfs: VfsDimension,             // L288
    #[serde(default)]
    pub skill: SkillDimension,         // L291
}
```

维度子结构（同文件）：
- `ToolDimension` (L236-246)：`capabilities: BTreeSet<ToolCapability>`,
  `enabled_clusters: BTreeSet<ToolCluster>`, `tool_policy: BTreeMap<String, ToolCapabilityFilter>`,
  `mcp_servers: Vec<SessionMcpServer>`。
- `CompanionDimension` (L249-253)：`agents: Vec<CompanionAgentEntry>`。
- `VfsDimension` (L256-260)：`active: Option<Vfs>`。
- `SkillDimension` (L262-267)：`skills: Vec<SkillEntry>`。 ← **新增 module 维度的同构模板**（一个 Vec<Entry> 的新维度）。

方法：`all()` (L296-311), `has(cluster)` (L314-316),
`is_capability_tool_enabled(capability_key, tool_name, cluster)` (L328+)。

### effective_capability_json 与 launch 解析

- `AgentFrame.effective_capability_json: Option<serde_json::Value>`（agent_frame.rs L14-15）。
- typed 读取：`crates/agentdash-application/src/workflow/frame_surface.rs`
  `AgentFrameSurfaceExt::typed_capability_state` (L43-47) =
  `effective_capability_json.and_then(serde_json::from_value)`。
- launch 时解析进 turn 的链路：
  - `build_envelope_from_frame`（frame_construction/mod.rs L312）
    `let mut capability_state = frame.typed_capability_state();`，可被 extras 覆盖 (L336-338)，
    最终 envelope.capability_state non-optional (L358-362)。
  - `preparation.rs` L74 `let capability_state = launch_plan.context.turn.capability_state.clone();`
    并 L99 把 `capability_state.tool.mcp_servers` 喂给工具装配。
  - `context.turn.capability_state` 即 provider 门控读取源（见 02/03 文档）。

## Caveats / Not Found

- 新增 “visible module” 裁切字段若放 AgentFrame，建议与 `visible_canvas_mount_ids_json`
  同构（Option<Value> + getter + append + new_revision 不复制）。
- 若放 CapabilityState 则与 `SkillDimension` 同构（新维度 + serde default）。两条路并存，
  具体取决于 design：「运行时追加」语义走 AgentFrame，「revision 内能力」语义走 CapabilityState。

# VFS Architecture

## Role

VFS 子系统给 Agent、前端和业务用例提供统一地址模型，屏蔽 `backend_id`、绝对路径、数据库主键和 inline storage 坐标。它负责 surface resolution、provider dispatch、runtime mount、mutation 和本机物化边界。

## Invariants

- 外部访问地址统一为 `surface_ref + mount_id + mount_relative_path`。
- `mount_relative_path` 进入 application 层前必须 normalize；绝对路径和 `..` escape 必须失败。
- 云端 provider 不直接访问本机文件系统；本机 provider 不直接读写业务数据库。
- runtime mount 是 provider 分发单位，至少包含 id、provider、root_ref、capabilities 和 metadata。
- `Vfs` 构建后必须 hard validate：mount id 唯一、default mount 存在、provider/root_ref 合法、capability 与 provider 支持范围一致、link 无环。
- Inline storage 坐标只能由 application resolver 从 runtime mount metadata 生成。
- binary bytes 不内联进 JSON DTO；通过 `read_binary` / blob 通道读取。
- Agent-facing VFS tools 按职责拆分：共享 runtime VFS handle 与 URI resolution 在 `vfs/tools/common.rs`，mount discovery 在 `vfs/tools/mounts.rs`，`vfs/tools/fs.rs` 只保留 file/search/patch/shell tool facade，具体 handler 位于 `vfs/tools/fs/`。共享 session state 和具体工具分离，原因是工具集合会继续扩展，但 runtime VFS address 语义必须集中。
- Session runtime tool surface 由 `SessionRuntimeToolComposer` 组合多个 domain provider；VFS bootstrap 只构建 VFS service/materialization/registry，session bootstrap 负责注入 runtime tool composer，原因是 Agent-facing tool surface 同时消费 VFS、workflow、collaboration 和 workspace module runtime facts，不能归属于单一 VFS service。
- Workspace module runtime surface 以 `WorkspaceModuleAgentSurface` 的 domain outcome / surface error 作为业务边界，`AgentToolResult`、`AgentToolError` 和 `ContentPart` 只出现在 `workspace_module/tools.rs` 等 AgentTool adapter 层。Canvas runtime bridge helpers 使用 workspace-module bridge error，再由 surface 显式映射为 surface error，原因是 surface 需要表达 Canvas binding、runtime action、protocol channel、diagnostic 和 presentation 语义，而工具协议投影只是 Agent-facing adapter 的输出格式。

## Current Baseline

Provider baseline：

| Provider | 职责 |
| --- | --- |
| `relay_fs` | 通过 relay 访问本机 workspace 文件 |
| `inline_fs` | 暴露 Project / Story / Agent Knowledge 等内联文件 |
| `skill_asset_fs` | 暴露 Skill asset 文件视图 |
| `lifecycle_vfs` | 暴露 AgentRun delivery session 证据面与 runtime node artifact / record 投影 |
| `routine_vfs` | 暴露 Routine 当前触发投影、Routine 级 memory 与当前 entity memory |
| `canvas_fs` | 暴露 Canvas 虚拟内容 |

Tool module baseline：

| Module | 职责 |
| --- | --- |
| `vfs/tools/common.rs` | `SharedRuntimeVfs`、tool path resolution、text result helper |
| `vfs/tools/mounts.rs` | `mounts_list` discovery tool |
| `vfs/tools/fs.rs` | FS tool facade 与既有 public import 路径 |
| `vfs/tools/fs/read.rs` | `fs.read` text/binary/image read handler |
| `vfs/tools/fs/apply_patch.rs` | `fs.apply_patch` handler 与 mutation key locking |
| `vfs/tools/fs/glob.rs` | `fs.glob` list/pattern handler |
| `vfs/tools/fs/grep.rs` | `fs.grep` text search handler |
| `vfs/tools/fs/shell.rs` | `shell.exec`、VFS URI materialization notice、stream output projection |
| `vfs/tools/provider.rs` | `SessionRuntimeToolComposer` 与共享 runtime session/project/VFS 解析 helper |
| `vfs/tools/vfs_provider.rs` | mounts/fs/shell VFS 工具装配 |
| `vfs/tools/workflow_provider.rs` | lifecycle workflow 工具装配 |
| `vfs/tools/collaboration_provider.rs` | companion request/respond 工具装配 |
| `vfs/tools/workspace_module_provider.rs` | workspace module list/describe/create/invoke/present 工具装配 |

## Local Decisions

- Project VFS Mount 使用外部 `mount_id` 作为路径身份，数据库 UUID 只服务持久化和 inline owner，原因是 runtime address 必须稳定可读。
- VFS 物化默认使用公共稳定路径，只有物化副本语义明确绑定 runtime session 的动态投影进入 session cache scope，原因是公共资源需要跨 session 复用，而 session trace 派生内容需要随 runtime 生命周期收口。
- Routine memory 使用 session-scoped `routine` runtime mount 承载当前触发投影和长期工作记忆，原因是 Routine 的跨轮次上下文应脱离 prompt template 与 session history，并通过 VFS 的路径级能力边界管理读写。
- AgentRun workspace 的 resource browser 使用 conversation snapshot 中的 `resource_surface`，该 surface 从当前 `AgentFrame` typed VFS surface 摘要生成，并由 AgentRun surface resolver 通过 `AgentRunLifecycleSurfaceProjector` 叠加唯一 `lifecycle` aggregate mount。这样做的原因是 workspace panel 需要浏览当前 AgentRun delivery session 的执行证据和同一 surface 上的 SkillAsset projection，而不是浏览数据库层的跨会话 run inventory。
- AgentRun resource surface来自当前AgentFrame/Business Surface typed VFS；Runtime binding只提供thread/binding coordinate。运行日志通过canonical Runtime events/context读取，不伪装成VFS mount。
- ProjectAgent explicit lifecycle 和 Workflow AgentCall 通过 frame construction / lifecycle activation 把 `scope = "node_runtime"` lifecycle mount 写入 runtime frame VFS；该 mount 以 `orchestration_id + node_path + attempt` 作为执行节点身份，提供当前 node 的可写 `artifacts` / `records` 和只读 `session` 视角。这样做的原因是写入边界属于正在执行的 runtime node，而 workspace browser 的只读证据面属于 AgentRun delivery session。
- AgentRun surface resolver 在应用层输出已闭包的 resource surface，原因是 resource browser、Agent connector launch 和 conversation snapshot 都需要消费同一份包含 lifecycle mount 的 AgentRun resource surface。
- AgentRun lifecycle mount 的运行时 contract 是 `lifecycle_vfs` provider 必需的扁平 metadata：`scope`、run/session/node identity、`writable_port_keys`、`skill_asset_project_id` 和 `skill_asset_keys`。Projector 的 typed facts 留在 application 内存投影中，原因是 provider dispatch、SkillAsset 文件解析和 artifact 写入只需要稳定 provider metadata，不需要额外的调试 envelope 成为第二事实源。
- VFS overlay / mount directive 保持整 mount replace 语义；AgentRun lifecycle projection refresh 在 projector 内对同一 Project 的 SkillAsset、message stream 和 node facts 重新闭包。这样做的原因是 overlay 只表达 mount 集合替换，而 lifecycle refresh 要保证单个 `lifecycle` aggregate mount 上的 skills、session evidence 与 node runtime metadata 来自同一次 projection facts。

## Scenario: Session Runtime Tool Composition

### 1. Scope / Trigger

- Trigger: Runtime tool surface 同时服务 Agent connector、capability policy、workspace module 和 extension invocation；组合边界必须让每个 domain provider 只持有自身依赖。
- Scope: `SessionRuntimeToolComposer`、`VfsRuntimeToolProvider`、`WorkflowRuntimeToolProvider`、`CollaborationRuntimeToolProvider`、`WorkspaceModuleRuntimeToolProvider`、`SharedRuntimeGatewayHandle`。

### 2. Signatures

```rust
#[async_trait]
pub trait RuntimeToolProvider {
    async fn build_tools(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DynAgentTool>, ConnectorError>;
}
```

### 3. Contracts

- `SessionRuntimeToolComposer` 是唯一对外实现 `RuntimeToolProvider` 的 session tool composition root。
- Domain provider 必须通过 `ExecutionContext` 和显式注入依赖构建工具，不回读 API bootstrap 或 route state。
- VFS domain provider 只负责 `mounts_list`、`fs.*` 与 `shell.exec`；workflow/collaboration/workspace module 工具由各自 provider 装配。
- `WorkspaceModuleRuntimeToolProvider` 只有在 `RuntimeGateway` 与 extension channel transport 延迟注入完成后才装配 `workspace_module_invoke`。
- 所有 domain provider 必须继续经过 `capability_state.is_capability_tool_enabled()` 判断具体工具可见性。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| `ExecutionContext.session.vfs` 缺失 | VFS 工具装配返回 `ConnectorError::InvalidConfig` |
| capability 关闭某个 tool | provider 不注入该 tool |
| context 无法解析 Project id | workspace module provider 不注入 Project-scoped module tools |
| `RuntimeGateway` 或 channel transport 未注入 | 跳过 `workspace_module_invoke` 并记录 warning |
| domain provider 构建失败 | composer 返回该错误，不吞掉失败 |

### 5. Good/Base/Bad Cases

- Good: VFS 工具只依赖 `VfsService`、runtime VFS 和 materialization transport；workspace module invoke 只在 workspace module provider 中读取 Gateway handle。
- Base: 一个 session 只启用 VFS capability 时，composer 返回 mounts/fs/shell 工具集合。
- Boundary mismatch: VFS bootstrap 同时构建 workflow/collaboration/workspace module 工具会把 session runtime surface 绑回 VFS service 生命周期。
- Canonical flow: session bootstrap 构造 domain providers，交给 composer；app state 在 RuntimeGateway 建好后回填 `SharedRuntimeGatewayHandle`。

### 6. Tests Required

- `agentdash-application vfs::tools` 测试 mounts/fs/shell 工具 schema、capability gating 与 VFS 缺失错误。
- `agentdash-application workspace_module::tools` 测试 module list/describe/create/invoke/present 装配和 Gateway 延迟注入。
- `agentdash-workspace-module workspace_module::surface` 测试 typed outcome，例如 Canvas binding 返回 `CanvasBindingApplied`、Canvas present 返回 `Presented` 并注入 notification；adapter 测试再覆盖 outcome 到 `AgentToolResult` 的投影。
- `agentdash-api bootstrap::tests::bootstrap_modules_do_not_depend_on_routes` 断言 bootstrap 不反向依赖 routes。
- `agentdash-api vfs_access::tests::runtime_tool_schemas_are_openai_compatible` 覆盖最终 tool schema 兼容性。

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```rust
// VFS provider 同时组装 workflow、collaboration 和 workspace module 工具
let tools = build_every_runtime_tool_from_vfs_bootstrap(context);
```

#### Canonical

```rust
let composer = SessionRuntimeToolComposer::new(vec![
    Arc::new(VfsRuntimeToolProvider::new(...)),
    Arc::new(WorkflowRuntimeToolProvider::new(...)),
    Arc::new(CollaborationRuntimeToolProvider::new(...)),
    Arc::new(WorkspaceModuleRuntimeToolProvider::new(...)),
]);
```

## Contract Appendices

- [VFS Access](./vfs-access.md)
- [VFS Materialization](./vfs-materialization.md)

# AgentDash 项目分模块 Review 与重构汇总报告

> 审查对象：`/mnt/data/AgentDash.zip` 解压后的 AgentDash 仓库。  
> 审查方式：静态源码 / 文档 / 路由 / 路径设计 review。当前容器缺少 `cargo`，因此没有执行 `cargo check/test/clippy`、`pnpm check` 或端到端测试；本报告不声称项目能否通过编译或测试。  
> 重点范围：模块边界、数据流、VFS/文件路径、API 路径、前端接口路径、重复实现、职责漂移、文档漂移与重构优先级。

---

## 0. 总体结论

AgentDash 已经形成了较清晰的“大方向”：Domain / Application / Infrastructure / API / Local Runtime / Frontend 多层结构基本存在，VFS、SessionHub、Lifecycle、Routine、Canvas、ACP/MCP、Relay 等能力也已纳入同一工程。但从代码形态看，项目处在高速演进后的“架构收敛期”：核心概念很多，部分概念已经落地，部分仍处于概念文档或过渡实现，导致多个层面出现重复、路径歧义、职责混合和意图漂移。

最需要优先治理的是 **数据路径与资源寻址体系**。当前 VFS 使用 `mount_id://relative/path`、`Mount.root_ref`、HTTP route、前端字符串 endpoint、local runtime physical path、lifecycle virtual path、materialization path 多套路径表达共同工作，但它们多数仍是 `String`，缺少统一的类型约束和策略对象。结果是：

1. 同一资源存在多种访问路径，例如 lifecycle 的 `active/*`、`session/*`、`tool-calls/*` 与 `nodes/{step_key}/*` 并存。
2. 同一类路径校验逻辑重复分布在 `vfs/path.rs`、`session/path_policy.rs`、`agentdash-local/tool_executor.rs`、materialization 等位置。
3. 有些路径策略明显不一致：文件读写拒绝绝对路径，但 shell cwd 允许 workspace 内绝对路径；session working_dir 直接 `Path::join`，保留了绝对路径覆盖与 `..` 语义。
4. `root_ref` 同时承担物理路径、虚拟 URI、provider root 等多种含义，`join_root_ref` 是 scheme-blind 字符串拼接，未来很容易在 `lifecycle://`、`skill-assets://`、`canvas://`、本机路径之间产生歧义。
5. API 与前端 endpoint 仍以字符串手写为主，REST 路由数量多、动作路径多，前后端 drift 风险较高。

建议的重构方向不是推倒重来，而是围绕“地址类型化 + 路径策略集中 + VFS 服务拆分 + API/前端契约生成”分阶段收敛。

---

## 1. 项目结构与规模观察

### 1.1 代码规模

静态扫描结果：

| 指标 | 结果 |
|---|---:|
| 总文件数（排除 `.git/node_modules/target/dist/build/.pnpm-store`） | 1906 |
| Rust 文件 | 470 |
| TS/TSX 文件 | 246 |
| Markdown 文档 | 506 |
| JSON/JSONL | 556 |
| SQL migration | 33 |

较大的源码/文档文件集中在 session、VFS、agent loop、API route、前端大页面：

| 文件 | 大小 | 观察 |
|---|---:|---|
| `crates/agentdash-application/src/session/assembler.rs` | 110 KB | 多启动路径组合器，职责仍偏重 |
| `crates/agentdash-application/src/companion/tools.rs` | 103 KB | companion 工具逻辑过大 |
| `packages/app-web/src/pages/SettingsPage.tsx` | 86 KB | 页面/状态/API/表单可能混杂 |
| `crates/agentdash-api/src/routes/acp_sessions.rs` | 72 KB | ACP Session route handler 过大 |
| `crates/agentdash-application/src/session/hook_delegate.rs` | 62 KB | Hook runtime 委托逻辑复杂 |
| `crates/agentdash-agent/src/agent_loop.rs` | 62 KB | Agent loop 主逻辑可拆分 |
| `crates/agentdash-domain/src/workflow/value_objects.rs` | 62 KB | workflow 值对象聚集过多 |
| `crates/agentdash-application/src/vfs/provider_lifecycle.rs` | 55 KB | Lifecycle VFS 路径目录和行为混合 |
| `packages/app-web/src/features/project/agent-preset-editor.tsx` | 51 KB | 配置编辑器复杂度高 |

这些大文件多数不是单纯“行数多”，而是体现出多个概念在同一个模块内继续扩展，后续维护会越来越依赖个别文件的上下文记忆。

### 1.2 Workspace 与文档漂移

实际 Cargo workspace 包含：

- `agentdash-domain`
- `agentdash-application`
- `agentdash-infrastructure`
- `agentdash-spi`
- `agentdash-executor`
- `agentdash-first-party-integrations`
- `agentdash-integration-api`
- `agentdash-api`
- `agentdash-mcp`
- `agentdash-agent-types`
- `agentdash-agent`
- `agentdash-relay`
- `agentdash-local`
- `agentdash-local-tauri`
- `agentdash-agent-protocol`

README 的项目结构仍提到 `frontend/` 和 `agentdash-acp-meta`。实际前端目录是 `packages/app-web`、`packages/app-tauri`、`packages/core`、`packages/ui`、`packages/views`；Cargo workspace 中也没有 `agentdash-acp-meta`。这说明 README 的结构说明落后于代码。

文档层面，`docs/modules/*.md` 多处标记为“概念定义阶段 / 待讨论 / 暂不定义”，`.trellis/tasks/archive` 中保留了大量历史设计与任务。这些文档对理解演进很有价值，但现在与实际代码混在同一仓库中，容易让新贡献者误判“哪份是当前权威设计”。

建议将文档分为三类：

1. **current/**：当前权威架构与接口。
2. **decision-records/**：已决策 ADR。
3. **archive/**：历史任务、草案、废弃方案。

---

## 2. 当前数据流与路径流总览

### 2.1 前端到后端的数据流

典型路径：

```text
React 页面 / Store / Service
  -> api.get/post/put/delete(字符串 endpoint)
  -> Axum routes.rs / 子 route handler
  -> AppState 中的 repository / service / registry
  -> Application service / Domain aggregate / Infrastructure repository
  -> Postgres / SQLite / Relay / SessionHub / Connector
```

现状问题：

- 前端 service 中大量 endpoint 是手写字符串。静态扫描 `packages/app-web/src` 约有 243 处 endpoint literal/template，去重后约 169 个。
- 后端 `routes.rs` 中静态 route/nest 约 136 条，且集中写在单个 `create_router` 中。
- 前端 DTO 与 Rust DTO 多数手动维护；虽然已有 `agentdash-agent-protocol` 生成的 `backbone-protocol.ts`，但 REST API 的类型没有形成统一生成链路。

### 2.2 Agent 工具到 VFS/本地文件的数据流

典型路径：

```text
Agent tool call: fs_read/fs_write/search/apply_patch/shell_exec
  -> agentdash-application VFS tool layer
  -> RelayVfsService
  -> parse_mount_uri / normalize_mount_relative_path / resolve_mount
  -> MountProviderRegistry
  -> provider:
       relay_fs       -> WebSocket Relay -> agentdash-local ToolExecutor -> physical FS/shell
       inline_fs      -> DB inline overlay
       lifecycle_vfs  -> workflow/session/record virtual projection
       canvas_fs      -> canvas assets
       skill_asset_fs -> project skill assets
```

核心风险在于“每一层都能解释路径”，而不是只有一个地址解析中心解释路径。

### 2.3 Session 启动与上下文数据流

`session/assembler.rs` 注释中明确写到当前需要统一的启动路径包括：

- ACP Story / Project session
- Story step activation
- Routine
- Workflow AgentNode
- Companion

这是正确方向：多入口不应各自手写 bootstrap。问题是组合器文件已经达到 110 KB，说明“组合器 + 平坦末端”的重构还没有完成。建议进一步拆成：

```text
SessionLaunchIntent
  -> OwnerResolver
  -> WorkspaceResolver
  -> AgentPresetResolver
  -> ContextBundleBuilder
  -> VfsPlanBuilder
  -> HookRuntimePlanBuilder
  -> SessionLaunchPlan
  -> SessionHub.start(plan)
```

这样每个阶段都可以单元测试，并且可记录“最终会话启动计划”。

---

## 3. 路径设计专项审计

### 3.1 VFS URI：解析与规范化分离，容易产生隐性差异

位置：`crates/agentdash-application/src/vfs/path.rs`

当前：

- `parse_mount_uri(input, vfs)` 支持：
  - `mount_id://relative/path`
  - `relative/path` 使用默认 mount
- 解析时只做 trim、去掉 URI 后 leading `/`，不立即规范化 path。
- 后续 read/write/list/search 等再分别调用 `normalize_mount_relative_path`。

风险：

- 同一输入在不同调用链上可能被不同方式接受或拒绝。
- `parse_mount_uri` 解析出的 `ResourceRef.path` 仍可能包含 `.`、重复斜杠、`a/../b` 等，需要每个消费者记得再 normalize。
- VFS link 解析基于字符串 prefix，规范化前后可能影响 link 命中。

建议：

- 引入 `VfsUri` 与 `MountRelativePath` 两个类型。
- `VfsUri::parse` 阶段完成规范化与安全检查，除非显式选择 `RawVfsUri`。
- `ResourceRef` 内部不再持有任意 `String path`，而是持有 `MountRelativePath`。
- Link 匹配前统一规范化 from/to path。

### 3.2 `Mount.root_ref` 同时表示物理路径和虚拟 URI

位置：

- `crates/agentdash-domain/src/common/mount.rs`
- `crates/agentdash-application/src/vfs/mount.rs`
- `crates/agentdash-application/src/vfs/path.rs`

当前：

- Workspace mount 的 `root_ref` 是本机物理目录，例如 `/repo` 或 Windows 路径。
- Lifecycle mount 的 `root_ref` 是 `lifecycle://run/{run_id}`。
- Skill asset mount 的 `root_ref` 是 `skill-assets://project/{project_id}`。
- Canvas mount 的 `root_ref` 是 `canvas://{canvas_id}`。
- `join_root_ref(root_ref, relative_path)` 通过是否包含反斜杠判断用 `/` 还是 `\` 拼接。

风险：

- `root_ref` 的含义依赖 provider，Domain 层无法表达不变量。
- `join_root_ref` 对 URI scheme 不敏感；对 `context://`、`lifecycle://`、`canvas://` 等虚拟 URI 做普通字符串拼接，很难保证语义一致。
- 后续如果 `root_ref` 包含 query、fragment、Windows UNC path、provider-specific encoded path，普通拼接会出错。

建议：

```rust
pub enum RootRef {
    LocalPath(LocalRootPath),
    ProviderUri(ProviderRootUri),
}

pub struct ProviderRootUri {
    scheme: ProviderScheme,
    authority_or_scope: String,
    path: MountRelativePath,
}
```

Provider trait 接口不要接收裸 `root_ref: String`，而应接收已解析的 provider root + relative path。

### 3.3 session working_dir 当前显式保留不安全语义

位置：

- `crates/agentdash-application/src/session/path_policy.rs`
- `crates/agentdash-application/src/session/prompt_pipeline.rs`

当前代码注释直接说明：

> 非空输入直接按 `Path::join` 处理，因此绝对路径与 `..` 的策略仍保持现状，后续由独立任务收紧。

`prompt_pipeline.rs` 又将 default mount 的 `root_ref` 转成 `PathBuf`，再调用 `resolve_working_dir`。

风险：

- `Path::join` 在遇到绝对路径时会产生平台相关的覆盖语义。
- `..` 不被拒绝，可能离开 mount root。
- 如果 default mount 是虚拟 URI root，例如 `lifecycle://run/...` 或 `skill-assets://project/...`，转成 `PathBuf` 本身语义就不稳定。
- working_dir 是 session prompt/connector 的关键输入，一旦绕开 root，后续 shell/tool 操作的边界会变得难以审计。

建议列为 P0：

1. `SessionUserInput.working_dir` 类型改为 `Option<MountRelativePath>` 或 `Option<VfsUri>`。
2. 不允许绝对路径、不允许越界 `..`。
3. 对 shell cwd 如需支持“workspace 内绝对路径”，必须显式通过 `PathPolicy::ShellCwd { allow_absolute_inside_root: true }` 表达，并在审计日志中保留原始输入与规范化结果。
4. 对虚拟 mount 不允许解析为 OS `PathBuf`，除非 provider 声明可 materialize。

### 3.4 local runtime 的文件路径与 shell cwd 策略不一致

位置：`crates/agentdash-local/src/tool_executor.rs`

当前：

- 文件读写通过 `normalize_relative_path` 拒绝绝对路径，拒绝越界 `..`。
- `resolve_shell_cwd` 对 shell cwd 允许绝对路径，只要 canonical 后仍在 workspace root 内。
- `validate_workspace_root` 对 workspace root 做 canonical，并在存在显式 `workspace_roots` 时确认 mount root 来源。

风险：

- 用户/Agent 对“路径是否合法”的预期在 fs tool 和 shell tool 之间不同。
- 如果云端 VFS/relay 层已经规范化 cwd 为相对路径，local 仍保留绝对 cwd 分支会成为隐性入口。
- 搜索结果中 `.strip_prefix(workspace_root).unwrap_or(abs_path)` 在 fallback 场景可能暴露绝对路径。

建议：

- 将 local path 解析统一为 `LocalPathPolicy`：`FileReadWrite`、`ShellCwd`、`SearchBase`、`PatchTarget`。
- 是否允许绝对 cwd 必须成为显式配置，并默认关闭。
- 搜索结果如果无法 strip workspace prefix，应返回错误或跳过，不要 fallback 到 absolute path。

### 3.5 relay_fs 的 search 将 base path 拼进 root_ref，语义与 list/read 不一致

位置：`crates/agentdash-api/src/mount_providers/relay_fs.rs`

当前：

- read/list/write 通常向 relay 传 `mount_root_ref` + `path`。
- search 中将 `query.path` 先 normalize 为 `base_path`，然后传：
  - `mount_root_ref: join_root_ref(&mount.root_ref, &base_path)`
  - `path: None`

风险：

- 同一请求中的“root”和“relative path”边界被改变。
- local side 看到的是新的 root，而不是原 mount root + path，审计/权限/日志粒度不一致。
- 依赖 `join_root_ref` 的 scheme-blind 拼接。

建议：

- relay protocol 对 search 也保持 `{ mount_root_ref, path }` 分离。
- local ToolExecutor 在同一处解析 root + relative path。
- 禁止 provider 在中途改写 mount root。

### 3.6 Lifecycle VFS 路径别名过多，目录 catalog 重复

位置：

- `crates/agentdash-application/src/vfs/mount.rs`
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`

当前 lifecycle mount 暴露的路径包含：

- `active`
- `active/steps/{step_key}`
- `state`
- `session/meta`
- `session/summary`
- `session/turns`
- `session/turns/{turn_id}/events.json`
- `session/events.json`
- `session/terminal`
- `tool-calls`
- `tool-calls/{tool_call_id}/raw.json`
- `tool-calls/{tool_call_id}/request.json`
- `tool-calls/{tool_call_id}/result.json`
- `tool-calls/{tool_call_id}/stdout.txt`
- `writes`
- `records/{name}`
- `nodes/{step_key}/state`
- `nodes/{step_key}/session/meta`
- `nodes/{step_key}/session/summary`
- `nodes/{step_key}/session/turns/{turn_id}/events.json`
- `nodes/{step_key}/session/tool-calls`
- `nodes/{step_key}/session/writes`
- `nodes/{step_key}/records/{name}`
- `active/log`
- `runs`

风险：

- `active/*`、`session/*`、`tool-calls/*`、`records/*` 是当前 node 的快捷路径；`nodes/{step_key}/*` 是显式 node 路径。二者是强别名关系，但没有通过 `Vfs.links` 或统一 catalog 建模。
- `mount.rs` 的 metadata `directory_hint.index` 与 `provider_lifecycle.rs` 的 read/list/write match arm 需要手动同步。
- 路径很长，且命名层级不统一：有的以 `active` 开头，有的直接以 `session`/`tool-calls` 开头。

建议：

1. 定义单一 canonical schema，例如：

```text
runs/{run_id}/nodes/{step_key}/state.json
runs/{run_id}/nodes/{step_key}/session/meta.json
runs/{run_id}/nodes/{step_key}/session/events.json
runs/{run_id}/nodes/{step_key}/records/{name}
runs/{run_id}/artifacts/{port_key}
```

2. 当前 node 快捷方式用 `Vfs.links` 或 provider-level alias table 表达：

```text
current/state.json -> runs/{run_id}/nodes/{active_step}/state.json
current/session/*  -> runs/{run_id}/nodes/{active_step}/session/*
```

3. `directory_hint`、read/list/write 都从同一个 `LifecyclePathCatalog` 生成，禁止重复手写。

### 3.7 `apply_patch_multi` 的 move_path 没有一起规范化

位置：

- `crates/agentdash-application/src/vfs/relay_service.rs`
- `crates/agentdash-application/src/vfs/apply_patch.rs`

当前：

- `apply_patch_multi` 对每个 patch entry 的 primary path 调用 `split_mount_prefix`，按 mount 分组。
- `PatchEntry::set_path` 只修改 primary `path`，没有修改 `UpdateFile { move_path }`。

风险：

- 如果 patch 是 `UpdateFile main://a` 并 `Move to: main://b`，primary path 会被剥掉 mount prefix，但 `move_path` 可能仍保留 `main://b`。
- 如果 move target 是 `other://b`，当前逻辑没有明确拒绝跨 mount move，也没有跨 mount 事务。
- 这类 bug 很隐蔽，因为普通 add/delete/update 不触发，只有 move patch 触发。

建议列为 P0：

- 在 `PatchEntry` 中增加 `normalize_paths_with_mount(fallback_mount_id)`，同时处理 primary path 与 move_path。
- 明确策略：跨 mount move 要么禁止，要么拆成 copy+delete 并实现事务/回滚。
- 增加表格测试：
  - `main://a -> main://b`
  - `main://a -> other://b`
  - `a -> b`
  - `a -> ../b`
  - `a -> /abs/b`

### 3.8 workspace mount 固定为 `main`，不利于多 workspace 与 mount 去重

位置：`crates/agentdash-application/src/vfs/mount.rs`

当前：

- `workspace_mount` 固定 `id: "main"`。
- `build_derived_vfs` 如果存在 id 为 `main` 的 mount，就把 default mount 设置为 `main`。
- Context container、canvas、skill asset、lifecycle 也有自己的 mount id，但没有全局唯一性验证。

风险：

- 一个 session 难以表达多个 workspace mount。
- 用户/Story context container 如果配置 mount_id 为 `main`，可能和 workspace mount 冲突。
- `main` 既是默认 mount，又是硬编码 ID，语义过重。

建议：

- workspace mount ID 使用稳定、可区分的规则：`ws-{workspace_id短码}` 或 `workspace-{slug}`。
- `main` 只作为 alias/default pointer，不作为真实 mount id。
- 引入 reserved mount ids：`main`、`lifecycle`、`skill-assets`、`agent-knowledge`、`current` 等，并校验用户自定义 mount id 不得冲突。
- `Vfs::validate()` 检查 mount id 唯一、provider 存在、root_ref 合法、default_mount_id 存在、links 无环。

### 3.9 file-picker 与 VFS API 重叠

位置：

- `crates/agentdash-api/src/routes.rs`
- `packages/app-web/src/services/*`

当前：

- VFS surface API：`/vfs-surfaces/read-file`、`write-file`、`create-file`、`delete-file`、`rename-file`、`stat-file`、`apply-patch`。
- File picker API：`/file-picker`、`/file-picker/read`、`/file-picker/batch-read`。
- 注释写着 file-picker “走 VFS 统一访问层”，但 HTTP 表面仍是另一套入口。

风险：

- UI 文件选择器和 VFS 浏览器可能出现不同权限、分页、排序、过滤、错误处理。
- 前端维护两套 service 概念。
- 后端 route handler 随着功能增强会重复。

建议：

- 保留 file-picker 作为前端组件/adapter，不保留独立资源 API。
- 统一为 VFS 查询接口，例如：

```text
GET  /vfs-surfaces/{surface}/mounts/{mount}/entries?path=&kind=&q=
POST /vfs-surfaces/{surface}/files:read-batch
```

或将 batch/read 行为作为 VFS action，但命名保持一致。

---

## 4. 分模块 Review 与重构意见

### 4.1 Domain：模型完整，但缺少关键不变量和类型化地址

涉及目录：`crates/agentdash-domain`

优点：

- Project / Story / Task / Workspace / Session / Workflow / Routine / Canvas 等聚合边界基本清晰。
- `AgentPresetConfig` 已声明为 Agent 配置存储层权威类型，尝试收敛 `Agent.base_config`、`ProjectAgentLink.config_override`、`AgentPreset.config`。
- Workspace 模型区分了逻辑 workspace 与 physical binding，这是正确方向。

问题：

1. **Mount 仍是 stringly typed。**  
   `Mount.id`、`provider`、`backend_id`、`root_ref` 都是 `String`，无法在 Domain 层表达 mount id 合法性、provider scheme、root 类型。

2. **WorkspaceResolutionPolicy 没有被充分执行。**  
   Domain 中有 `PreferDefaultBinding`、`PreferOnline`，`WorkspaceBinding` 也有 `status`、`priority`、`last_verified_at`，但 `selected_workspace_binding` 只看 default binding 或 exactly one binding，忽略 status/priority/policy。

3. **Workspace 默认能力过宽。**  
   默认 mount capabilities 包含 Read / Write / List / Search / Exec。对 Agent session 来说，Exec 是高风险能力，建议不作为默认能力，改为 capability profile 或项目策略显式开启。

4. **Agent 配置权威仍有重叠来源。**  
   `AgentPresetConfig` 是权威类型，但 Task 的 `AgentBinding` 仍包含 `agent_type`、`agent_pid`、`preset_name`、`prompt_template`、`initial_context`、`context_sources` 等运行相关信息。ProjectAgentLink、Task AgentBinding、Session request、AgentPresetConfig 之间的覆盖顺序需要形成一张明确的 merge matrix。

重构建议：

- 引入 newtype：`MountId`、`ProviderId`、`BackendId`、`RootRef`、`VfsUri`、`MountRelativePath`、`ExecutorId`。
- `Workspace::select_binding(policy, backend_status_provider)` 放到 domain/application 边界，严格使用 status/priority/default。
- Workspace 默认能力改为 Read/List/Search，Write/Exec 由 project/agent policy 授权。
- 写一份 `AgentRuntimeConfigResolution` 规范，明确 base/override/task/session/routine/lifecycle 的优先级。

### 4.2 SPI / Protocol：抽象方向正确，但路径仍应类型化

涉及目录：

- `crates/agentdash-spi`
- `crates/agentdash-relay`
- `crates/agentdash-agent-protocol`

优点：

- MountProvider、Connector、Hook、Relay Protocol 等跨层边界已独立为 crate。
- Relay 协议把 local runtime 和 cloud backend 解耦，是适合 NAT/多机的方向。

问题：

- Relay payload 中仍传 `mount_root_ref` 和 `path` 字符串，协议层不能强制二者语义。
- search 和 exec 对 root/path/cwd 的表达不完全统一。
- `agentdash-relay/src/protocol.rs` 约 46 KB，协议枚举和 payload 可能继续膨胀。

重构建议：

- 在 protocol 中区分：`WorkspaceRootRef`、`MountRelativePath`、`ShellCwd`、`SearchBasePath`。
- 保持所有 Relay FS 操作都传 root + relative path，不允许 provider 改写 root。
- 对 protocol payload 做版本化，减少新增字段引起的兼容歧义。

### 4.3 Application / VFS：当前是最大重构收益区

涉及目录：`crates/agentdash-application/src/vfs`

优点：

- 已经有 `MountProviderRegistry`、`RelayVfsService`、provider 分层。
- inline / relay / lifecycle / canvas / skill asset 能力统一暴露为 VFS mount。
- `normalize_mount_relative_path` 对绝对路径和越界 `..` 有明确防护。

问题：

1. **RelayVfsService 职责过多。**  
   它同时负责 address parsing、capability check、inline overlay、provider dispatch、create/delete/rename/stat fallback、patch grouping、多 mount patch、fallback search。

2. **Provider 之间存在职责混合。**  
   `append_skill_asset_projection` 在存在 lifecycle mount 时，把 skill asset metadata 注入 lifecycle mount；`provider_lifecycle.rs` 又负责读取 `skills/...`。这让 lifecycle provider 同时承载 journey projection 与 skill asset projection。

3. **create/rename 通过 read 判断存在性。**  
   `create_text` 中任意 read error 都被当作“不存在或不可读”，再走 write；rename 也先 read source/read destination。对 provider 来说，NotFound、PermissionDenied、BackendOffline 应区分。

4. **Lifecycle path catalog 重复。**  
   `mount.rs` metadata、`provider_lifecycle.rs` read/list/write 都手写路径。

建议拆分：

```text
VfsAddressResolver      解析 VfsUri / default mount / links
VfsPolicyValidator      path/capability/write target/security policy
VfsMountDispatcher      找 provider 并执行 operation
InlineOverlayAdapter    inline_fs overlay 的特殊写入
VfsPatchCoordinator     patch parse/group/apply/transaction
LifecyclePathCatalog    lifecycle canonical path + alias + directory hints
VfsError                NotFound/PermissionDenied/Offline/InvalidPath/Unsupported
```

### 4.4 Session / Context / Hook / Workflow：方向对，但启动计划需要显性化

涉及目录：

- `crates/agentdash-application/src/session`
- `crates/agentdash-application/src/context*`
- `crates/agentdash-application/src/workflow`
- `crates/agentdash-application/src/hook*`

优点：

- 代码已经意识到多入口启动路径重复，`session/assembler.rs` 试图统一 ACP、Story、Routine、Workflow、Companion。
- Hook runtime 与 workflow/lifecycle 都有较完整的注入点。

问题：

- `assembler.rs` 过大，说明“启动计划”还没有成为足够稳定的数据结构。
- `prompt_pipeline.rs` 中 VFS fallback 顺序是请求级 -> cached continuation -> hub 默认，再 merge pending VFS overlay。这是重要策略，但现在散落在执行逻辑中。
- SessionOwnerCtx 与 route/response 中的 flat owner fields 仍处于过渡态，owner 语义在 API/DB/Application 之间可能不一致。

重构建议：

- 定义 `SessionLaunchPlan`，作为 assembler 的唯一产物，包含：owner、agent_config、workspace_binding、vfs_plan、context_bundle、hook_runtime_plan、capability_state、initial_user_input。
- 把 VFS fallback/overlay merge 抽成 `VfsPlanResolver`，记录每个 mount 的来源。
- 对每种启动入口写 golden test：Project session、Story task、Routine、Workflow node、Companion。
- Hook 注入结果形成可审计 manifest，避免“注入顺序靠代码阅读”。

### 4.5 API：route 集中、动作路径多、前端契约缺生成

涉及目录：`crates/agentdash-api`

优点：

- route 模块已经按业务拆了多个 handler 文件。
- 认证中间件与公开 webhook/auth/health 有区分。
- REST + SSE + WS 的入口都能在 `create_router` 中看到，便于初期理解。

问题：

1. **`routes.rs` 过度集中。**  
   单个 `create_router` 中约 136 条 route/nest，变更时不容易按 bounded context review。

2. **资源路径和动作路径混合。**  
   例子：`/routines/{id}/enable`、`/tasks/{id}/start`、`/tasks/{id}/continue`、`/vfs-surfaces/read-file`、`/vfs-surfaces/apply-patch`、`/workflow-templates/{builtin_key}/bootstrap`。

3. **查询语义用 path 表达。**  
   `/lifecycle-runs/by-session/{session_id}` 可以考虑放到 `/sessions/{id}/lifecycle-runs` 或 query。

4. **类似能力入口重复。**  
   `/workspaces/detect-git` 与 `/projects/{project_id}/workspaces/detect`；`/file-picker/*` 与 `/vfs-surfaces/*`；`/backends/{backend_id}/browse` 与 VFS list/search。

重构建议：

- 每个 bounded context 暴露 `router(state)`，最终合并：`project_routes()`, `session_routes()`, `vfs_routes()`, `workflow_routes()`。
- 为 REST DTO 和前端 client 生成契约：OpenAPI/utoipa、ts-rs、或者自定义 route manifest。
- 将 VFS action 统一命名，减少 file-picker 特例。
- 给 route 增加稳定版本：`/api/v1/...`。

### 4.6 Relay / Local Runtime：安全边界已有雏形，但多 root 和配置语义偏弱

涉及目录：

- `crates/agentdash-local`
- `crates/agentdash-api/src/mount_providers/relay_fs.rs`
- `scripts/dev-joint.js`

优点：

- `ToolExecutor::validate_workspace_root` 会 canonicalize workspace root，并在存在显式 workspace roots 时检查归属。
- file read/write 路径有相对路径规范化和越界检查。
- local runtime 支持 WebSocket 主动连云端，符合 README 的双后端方向。

问题：

1. **多 workspace roots 下配置只取第一个。**
   `load_local_backend_config(workspace_roots)` 使用 `workspace_roots.first()`；`runtime.rs` 初始化 SessionHub、SQLite db、connector workspace_root 也取第一个 root。多 root 对 ToolExecutor 是安全边界，对 runtime config 却不是完整模型。

2. **搜索结果可能 fallback 绝对路径。**  
   `strip_prefix(...).unwrap_or(abs_path)` 不利于隐藏本机绝对路径。

3. **materialization store 忽略 backend_id。**  
   `MaterializationStore::new(_backend_id)` 当前参数未用于路径命名。若多个 local identity 共享数据目录，缓存隔离需要重新审视。

4. **shell cwd 策略和 VFS 路径策略不完全一致。**

重构建议：

- Local backend 配置按 root/backend identity 分层：global config + per-root config。
- local runtime 的 workspace root 不应默认取 first root；应由 cloud 下发 workspace binding，或由用户选择。
- 搜索/list 增加分页、大小限制、ignore 规则；不返回绝对路径 fallback。
- materialization cache 使用 backend_id/machine_id/session_id 参与 namespace。

### 4.7 Agent / Executor：默认执行器与长期方向有漂移

涉及目录：

- `crates/agentdash-agent`
- `crates/agentdash-executor`
- `crates/agentdash-domain/src/common/agent_config.rs`

观察：

- README 声称 Pi Agent 是云端原生标准执行器。
- `AgentConfig::is_cloud_native` 也把 `PI_AGENT` 作为 cloud native executor。
- 但 `AgentConfig::default()` 返回 `CLAUDE_CODE`。

风险：

- 文档、默认值、实际 cloud-native 判断不一致。
- 新建 Agent 或 fallback executor 可能不符合产品长期方向。

建议：

- 明确当前默认执行器：如果 Pi Agent 是标准实现，默认值应转为 `PI_AGENT`；如果当前仍以 Claude Code 为默认，应在 README 中说明“Pi Agent 是未来/参考实现，当前默认仍是 Claude Code”。
- 引入 `ExecutorId` enum/newtype，所有 executor id 集中定义。
- `CompositeConnector` 的路由规则与 cloud/local/native 判断应由一个 registry 管理。

### 4.8 Infrastructure：仓储完整，但组合根过重

涉及目录：`crates/agentdash-infrastructure`、`crates/agentdash-api/src/app_state.rs`

优点：

- Postgres/SQLite repository 较完整。
- migration 数量多，说明模型在持续落地。

问题：

- `AppState` 构造可能承担过多 repository/service/registry 组装逻辑。
- 多个 migration 和注释体现历史字段迁移，例如 `tool_clusters -> capability_directives`，需要保证旧字段不会继续在前端/文档出现。
- workflow/lifecycle transition 如果涉及多 repo 写入，应明确事务边界。

重构建议：

- 拆出 composition builders：`build_repositories`、`build_vfs_registry`、`build_connectors`、`build_session_hub`、`build_routine_services`。
- 对关键业务写 transaction script，不要让 route handler/application service 隐式依赖多次 repo 调用成功。
- migration 文档加“当前 schema 权威字段”说明。

### 4.9 Frontend：功能覆盖广，但 API/状态/组件边界需要拆分

涉及目录：`packages/app-web`、`packages/app-tauri`、`packages/core`、`packages/ui`、`packages/views`

优点：

- 已抽出 `@agentdash/core`、`@agentdash/ui`、`@agentdash/views`，说明共享层方向正确。
- app-tauri 复用 app-web，有利于桌面壳统一。

问题：

1. **大页面/大组件过多。**  
   `SettingsPage.tsx`、`ProjectSettingsPage.tsx`、`workspace-layout.tsx`、`agent-preset-editor.tsx`、`project-agent-view.tsx` 都偏大。

2. **API endpoint 手写字符串多。**  
   约 169 个去重 endpoint/template，容易与后端 route drift。

3. **VFS/file-picker/service 重复。**  
   UI 上可能同时存在 VFS browser、file picker、assets panel、canvas editor 等文件入口，但底层 path 解析、错误处理、batch read 不统一。

4. **DTO 类型手写镜像。**  
   后端 route 变更需要手工同步 TypeScript 类型。

重构建议：

- 建立生成式 API client：Rust DTO -> schema -> TS client。
- 把 endpoint 字符串集中到 route manifest，禁止页面/组件直接拼 API path。
- 大页面拆成：data hook、container、presentational component、form state、mutation action。
- VFS UI 统一使用 `VfsAddress` 类型和 `useVfsSurface` hook。
- app-tauri 与 app-web 的区别放在 shell adapter，不要让 app-web 知道太多 Tauri/local runtime 细节。

### 4.10 Docs / Trellis / Scripts：保留历史有价值，但需要权威化

涉及目录：`docs`、`.trellis`、`scripts`

优点：

- 文档量很大，能看出大量架构思考和任务记录。
- `scripts/dev-joint.js` 已承担一键启动、多端口管理、local runtime 启动等实际价值。

问题：

- README 与实际 workspace/package 结构不一致。
- `docs/modules` 多为概念阶段，容易被误读成当前实现。
- `.trellis/tasks/archive` 中路径很长，最长 repo path 达 127 字符；多数不是源码问题，但会增加搜索噪音、压缩包体积、review 复杂度。
- `package.json` 中 clippy 显式 suppress 了多个复杂度相关 lint：`too_many_arguments`、`type_complexity`、`collapsible_if` 等。这是短期可接受的演进债，但需要逐步还。

建议：

- README 改成当前权威结构。
- 每份 docs 标记：`Current / Draft / Archived / Deprecated`。
- `.trellis/archive` 可保留，但建议移动到单独 history repo 或在默认检索/打包时排除。
- clippy allow 项逐步缩小范围，从 workspace 级 allow 改为局部 allow，并记录原因。

---

## 5. 重复路径、冗余路径、过长路径清单

### 5.1 重复路径 / 平行路径

| 类型 | 示例 | 问题 | 建议 |
|---|---|---|---|
| Lifecycle 当前节点别名 | `session/*` vs `nodes/{step_key}/session/*` | 同一数据两套路径 | canonical + alias table |
| Lifecycle tool call | `tool-calls/*` vs `nodes/{step_key}/session/tool-calls/*` | 目录语义重复 | 统一到 node/session 下 |
| Lifecycle records | `records/{name}` vs `nodes/{step_key}/records/{name}` | 当前节点快捷路径未显式建模 | `current/records/*` alias |
| Lifecycle artifacts | `artifacts/*` vs `active/artifacts/*` 或 metadata 中 current 表达 | 产物路径层级不稳定 | canonical `artifacts/{port}` |
| VFS read/list | `/vfs-surfaces/*` vs `/file-picker/*` | 两套 HTTP 入口 | file-picker 降级为 UI adapter |
| Workspace detect | `/workspaces/detect-git` vs `/projects/{project_id}/workspaces/detect` | 探测入口重叠 | 一个通用 detect API + project binding action |
| Backend browse / VFS list | `/backends/{backend_id}/browse` vs VFS entries | 物理浏览与 VFS 浏览可能不一致 | browse 也返回 VFS-compatible entries |
| Session stream | `/sessions/{id}` 与 `/acp/sessions/{id}/stream` | ACP stream path 脱离 session resource | 可接受但建议归并命名 |

### 5.2 冗余实现

| 功能 | 分散位置 | 风险 |
|---|---|---|
| 相对路径规范化 | `vfs/path.rs`、`tool_executor.rs`、materialization、embedded skill path 等 | 不同层策略逐渐分叉 |
| root/path 拼接 | `join_root_ref`、local root join、materialization join | scheme/OS 差异隐蔽 |
| 文件 read/write/create/delete/rename | VFS service、provider、file-picker、frontend service | 错误处理和权限判断重复 |
| Lifecycle path catalog | `mount.rs` metadata、`provider_lifecycle.rs` read/list/write | 目录说明和实际行为漂移 |
| API route 字符串 | Rust routes 与 TS service | 前后端 drift |
| Agent 配置 merge | AgentPresetConfig、ProjectAgentLink、Task AgentBinding、Session request | 覆盖顺序不透明 |

### 5.3 过长路径

| 路径类型 | 示例 | 评价 |
|---|---|---|
| 仓库文件路径 | `.trellis/tasks/archive/2026-05/04-30-session-pipeline-architecture-refactor/research/pipeline-review/03-connector-hook-layer.md` | 主要是 archive 噪音，可排除或移仓 |
| Migration 文件 | `crates/agentdash-infrastructure/migrations/0028_agent_config_tool_clusters_to_capability_directives.sql` | 可接受，但反映字段迁移历史较长 |
| API route | `/projects/{project_id}/skill-assets/{id}/reset-from-builtin` | 语义明确但动作路径偏长 |
| API route | `/sessions/{id}/companion-requests/{request_id}/respond` | 可以接受，但建议统一 action 命名 |
| API route | `/vfs-surfaces/{surface_ref}/mounts/{mount_id}/entries` | 层级清晰，但与 action endpoints 混用 |
| Lifecycle VFS | `nodes/{step_key}/session/turns/{turn_id}/events.json` | 可读但路径链条深；应只作为 canonical schema 的一部分 |

---

## 6. 意图漂移清单

| 漂移点 | 当前表现 | 风险 | 修复建议 |
|---|---|---|---|
| README 项目结构 | README 写 `frontend/`、`agentdash-acp-meta`；实际是 `packages/*` 且无该 crate | 新人误导 | 更新 README |
| Pi Agent 标准实现 | README 与 `CLOUD_NATIVE_EXECUTORS` 指向 Pi Agent；`AgentConfig::default()` 是 `CLAUDE_CODE` | 默认行为与长期方向不一致 | 明确当前默认/未来默认 |
| Workspace policy | Domain 有 `PreferOnline`/priority/status；实际选 binding 基本只看 default/单 binding | 领域模型未落地 | 实现 selector |
| Skill asset VFS | 有独立 `skill_asset_fs`，但 lifecycle mount 可注入 skill metadata 并服务 `skills/*` | provider 职责混合 | 独立 mount 或显式 projection provider |
| Session owner | 有 SessionOwnerCtx，但 API/DB response 仍有 flat owner fields | owner 语义可能分叉 | 统一 owner model 和 DTO |
| Docs modules | 多为概念定义阶段，而代码已经演进 | 文档不是权威 | docs 分层标记 |
| clippy 策略 | workspace 级 allow 多个复杂度 lint | 复杂度债务隐性化 | 局部 allow + 还债计划 |

---

## 7. 重构优先级路线图

### P0：安全与路径一致性，建议立即处理

1. **修复 session working_dir 策略。**  
   禁止绝对路径覆盖和越界 `..`，虚拟 mount 不转 OS PathBuf。

2. **修复 `apply_patch_multi` 的 move_path。**  
   move_path 必须和 primary path 一起解析、规范化、分 mount；跨 mount move 明确禁止或事务化。

3. **引入核心路径 newtype。**  
   最少先落地 `MountId`、`MountRelativePath`、`VfsUri`、`RootRef`，并在 VFS/Relay/local 关键入口使用。

4. **VFS mount validation。**  
   检查 mount id 唯一、reserved id、default mount 存在、link 无环、provider 支持。

5. **统一 local 文件/shell/search 路径策略。**  
   搜索结果不得 fallback 返回绝对路径；shell cwd 绝对路径策略显式化。

6. **补路径测试矩阵。**  
   至少覆盖 Unix/Windows-like、absolute、UNC、`..`、重复 slash、URI prefix、link loop、cross-mount patch。

### P1：架构收敛，降低重复和漂移

1. **拆分 RelayVfsService。**  
   AddressResolver / Dispatcher / OverlayAdapter / PatchCoordinator / Error model。

2. **LifecyclePathCatalog 单一来源。**  
   由 catalog 生成 directory hint、read/list/write route、aliases。

3. **Workspace binding selector 落地。**  
   实现 `PreferDefaultBinding`、`PreferOnline`、priority/status，支持多 workspace mount。

4. **REST/TS 契约生成。**  
   生成 API client 或 route manifest，减少前后端 endpoint string drift。

5. **统一 file-picker 与 VFS UI。**  
   file-picker 只做 UI adapter，底层使用同一 VFS service/hook。

6. **SessionLaunchPlan 显性化。**  
   统一多入口启动，并输出可审计 plan。

### P2：维护性与产品一致性

1. **拆分大文件和大组件。**  
   `assembler.rs`、`companion/tools.rs`、`SettingsPage.tsx`、`agent-preset-editor.tsx` 等按职责拆分。

2. **README 和 docs 权威化。**  
   current/draft/archive 分层，移除或标注过期模块。

3. **AppState composition root 拆分。**  
   构造 repositories、registries、services、connectors 的代码分模块。

4. **clippy allow 收敛。**  
   从 workspace 级 allow 改成局部 allow，逐个消除 `too_many_arguments`、`type_complexity` 等债务。

5. **local runtime 多 root 模型完善。**  
   配置、SQLite、materialization、connector workspace root 不再默认取 first root。

---

## 8. 建议的目标路径模型

建议建立统一的地址模型，作为后续所有 VFS、Relay、Frontend route 的基础。

```rust
pub struct MountId(String);
pub struct ProviderId(String);
pub struct BackendId(String);

pub struct MountRelativePath(String); // 已规范化，不含 absolute / escaping ..

pub enum RootRef {
    Local(LocalRootPath),
    Provider(ProviderRootUri),
}

pub struct VfsUri {
    pub mount_id: MountId,
    pub path: MountRelativePath,
}

pub enum PathPolicy {
    VfsRead,
    VfsWrite,
    VfsList,
    VfsSearchBase,
    ShellCwd { allow_absolute_inside_root: bool },
    SessionWorkingDir,
    MaterializationTarget,
}
```

Frontend 对应：

```ts
type MountId = string & { readonly __brand: 'MountId' }
type MountRelativePath = string & { readonly __brand: 'MountRelativePath' }

type VfsAddress = {
  surfaceRef: string
  mountId: MountId
  path: MountRelativePath
}
```

关键原则：

- 原始字符串只允许出现在 UI input/API boundary。
- 一进入 application 层就 parse/normalize 成类型。
- Provider 不再自己猜路径语义，只消费已验证的 root + relative path。
- 所有错误使用结构化 error：`InvalidPath`、`NotFound`、`PermissionDenied`、`BackendOffline`、`UnsupportedCapability`。

---

## 9. 建议的 API 路径收敛方向

### 9.1 VFS

当前：

```text
POST /vfs-surfaces/read-file
POST /vfs-surfaces/write-file
POST /vfs-surfaces/create-file
POST /vfs-surfaces/delete-file
POST /vfs-surfaces/rename-file
POST /vfs-surfaces/apply-patch
GET  /vfs-surfaces/{surface_ref}/mounts/{mount_id}/entries
GET  /file-picker
POST /file-picker/read
POST /file-picker/batch-read
```

建议：

```text
GET    /api/v1/vfs-surfaces/{surface}/mounts/{mount}/entries?path=
GET    /api/v1/vfs-surfaces/{surface}/mounts/{mount}/files?path=
PUT    /api/v1/vfs-surfaces/{surface}/mounts/{mount}/files
POST   /api/v1/vfs-surfaces/{surface}/mounts/{mount}/files:create
DELETE /api/v1/vfs-surfaces/{surface}/mounts/{mount}/files
POST   /api/v1/vfs-surfaces/{surface}/mounts/{mount}/files:rename
POST   /api/v1/vfs-surfaces/{surface}/patches
POST   /api/v1/vfs-surfaces/{surface}/files:read-batch
```

也可以继续使用 JSON body action，但命名应统一在 VFS resource 下。

### 9.2 Workspace detect

当前：

```text
POST /workspaces/detect-git
POST /projects/{project_id}/workspaces/detect
```

建议：

```text
POST /api/v1/workspace-detections
POST /api/v1/projects/{project_id}/workspace-bindings:detect
```

### 9.3 Lifecycle runs by session

当前：

```text
GET /lifecycle-runs/by-session/{session_id}
```

建议：

```text
GET /api/v1/sessions/{session_id}/lifecycle-runs
```

---

## 10. 测试建议

### 10.1 路径安全测试

必须覆盖：

- `""`, `"."`, `"./a"`, `"a//b"`, `"a/../b"`
- `"../a"`, `"a/../../b"`
- `"/abs"`, `"C:\\repo\\a"`, `"C:/repo/a"`, `"\\\\server\\share"`
- `"main://a/b"`, `"main:///a/b"`, `"main://a/../b"`
- VFS link loop 和超过 `MAX_LINK_DEPTH`
- lifecycle aliases 与 canonical path 等价性
- relay search base path 不改变 root
- local search 不返回 absolute fallback

### 10.2 Patch 测试

必须覆盖：

- add/update/delete 普通路径
- `mount://path` patch
- move patch 同 mount
- move patch 跨 mount
- move target absolute / escaping
- partial success 是否被 UI 正确展示

### 10.3 Session 启动 golden tests

每个入口输出 `SessionLaunchPlan` snapshot：

- Project session
- Story execution task
- Routine trigger
- Workflow node activation
- Companion request
- Continue/recover session

### 10.4 API contract tests

- route manifest 与 frontend generated client 一致。
- 每个 route 的 DTO schema snapshot。
- 公开 route 与 authenticated route 白名单测试。

---

## 11. 最小可执行重构顺序

建议按以下顺序推进，避免影响过大：

1. **补测试，不改行为。**  
   先把当前路径行为用 table tests 固化，包括已知不合理语义。

2. **修 P0 安全问题。**  
   working_dir、patch move_path、local search absolute fallback。

3. **引入 newtype，但先做兼容转换。**  
   对外 DTO 保持 string，application 内部转类型。

4. **新增 `Vfs::validate()` 并在构建 VFS 时调用。**  
   先 warn，再逐步变成 hard error。

5. **拆 RelayVfsService。**  
   先按内部私有模块拆，不改变 public API。

6. **LifecyclePathCatalog。**  
   先让 metadata 和 list 从 catalog 生成，再迁移 read/write match。

7. **API route manifest / generated client。**  
   先覆盖 VFS/session/project 高频接口，再逐步扩展。

8. **前端大组件拆分。**  
   优先拆 Settings、ProjectSettings、VFS/file picker、AgentPresetEditor。

---

## 12. 一页式优先事项

| 优先级 | 工作项 | 预期收益 |
|---|---|---|
| P0 | working_dir 安全收口 | 避免 session 执行路径越界 |
| P0 | patch move_path 规范化 | 避免跨 mount/URI move 隐性错误 |
| P0 | VFS mount id 唯一和 reserved 校验 | 消除 `main/lifecycle/skill-assets` 冲突 |
| P0 | local search 不泄露 absolute path | 收紧本机路径暴露 |
| P1 | 路径 newtype + PathPolicy | 从根上减少路径重复实现 |
| P1 | RelayVfsService 拆分 | 降低 VFS 核心复杂度 |
| P1 | LifecyclePathCatalog | 消除 lifecycle 目录重复和别名漂移 |
| P1 | Workspace binding policy 落地 | 让逻辑 workspace 模型真正生效 |
| P1 | API/TS client 生成 | 降低前后端 drift |
| P2 | 大文件/大组件拆分 | 提升维护效率 |
| P2 | README/docs 权威化 | 降低新人理解成本 |

---

## 13. 结语

AgentDash 的架构方向是清楚的，尤其是 VFS + Relay + SessionHub + Lifecycle 的组合很有潜力。但当前最大的系统性风险不是单点 bug，而是“同一个资源地址被多处、多种方式解释”。这会持续放大重复实现、权限边界不一致、文档漂移和前后端契约漂移。

因此，建议把下一轮重构的主题定为：

> **统一资源寻址与会话启动计划。**

先把路径和 VFS 这条主链路收紧，再逐步治理 API/前端契约与大模块拆分，整体风险最低、收益最高。

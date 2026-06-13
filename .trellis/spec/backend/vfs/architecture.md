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
| `vfs/tools/fs.rs` | FS tool facade 与旧 public import 路径 |
| `vfs/tools/fs/read.rs` | `fs.read` text/binary/image read handler |
| `vfs/tools/fs/apply_patch.rs` | `fs.apply_patch` handler 与 mutation key locking |
| `vfs/tools/fs/glob.rs` | `fs.glob` list/pattern handler |
| `vfs/tools/fs/grep.rs` | `fs.grep` text search handler |
| `vfs/tools/fs/shell.rs` | `shell.exec`、VFS URI materialization notice、stream output projection |

## Local Decisions

- Project VFS Mount 使用外部 `mount_id` 作为路径身份，数据库 UUID 只服务持久化和 inline owner，原因是 runtime address 必须稳定可读。
- VFS 物化默认使用公共稳定路径，只有物化副本语义明确绑定 runtime session 的动态投影进入 session cache scope，原因是公共资源需要跨 session 复用，而 session trace 派生内容需要随 runtime 生命周期收口。
- Routine memory 使用 session-scoped `routine` runtime mount 承载当前触发投影和长期工作记忆，原因是 Routine 的跨轮次上下文应脱离 prompt template 与 session history，并通过 VFS 的路径级能力边界管理读写。
- AgentRun workspace 的 resource browser 使用 conversation snapshot 中的 `resource_surface`，该 surface 从当前 `AgentFrame` typed VFS surface 摘要生成，并由 AgentRun surface resolver 叠加 `RuntimeSessionExecutionAnchor` 锚定的 `agent_run_session` lifecycle mount。这样做的原因是 workspace panel 需要浏览当前 AgentRun delivery session 的执行证据，而不是浏览数据库层的跨会话 run inventory。
- `lifecycle_vfs` 在 AgentRun resource surface 中是只读 session log surface。resolver 读取 latest delivery anchor、current frame / anchor frame 和 typed VFS 后安装 `scope = "agent_run_session"` mount；graphless AgentRun 只要存在 `RuntimeSessionExecutionAnchor`，就必须能看到 `session/*` 日志投影。可选 orchestration node anchor 只附带当前 node 的执行证据，不从 graph 或 active workflow 猜测节点。
- ProjectAgent explicit lifecycle 和 Workflow AgentCall 通过 frame construction / lifecycle activation 把 `scope = "node_runtime"` lifecycle mount 写入 runtime frame VFS；该 mount 以 `orchestration_id + node_path + attempt` 作为执行节点身份，提供当前 node 的可写 `artifacts` / `records` 和只读 `session` 视角。这样做的原因是写入边界属于正在执行的 runtime node，而 workspace browser 的只读证据面属于 AgentRun delivery session。
- AgentRun surface resolver 在应用层输出已闭包的 resource surface，原因是 resource browser、Agent connector launch 和 conversation snapshot 都需要消费同一份包含 lifecycle mount 的 AgentRun resource surface。

## Contract Appendices

- [VFS Access](./vfs-access.md)
- [VFS Materialization](./vfs-materialization.md)

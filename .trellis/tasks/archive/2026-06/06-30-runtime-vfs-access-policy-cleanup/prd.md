# Runtime VFS access policy 收束

## Goal

实现 design backlog Slice 7 / D9：把当前 Project VFS mount grant / mount capability 语义收窄为 provider support，把运行期 VFS 授权收束到一个 `RuntimeVfsAccessPolicy`。VFS 工具在 path normalize 后必须同时满足 tool capability、mount provider capability 和 runtime policy，避免把 Project VFS 预设 grant、tool-level PermissionGrant 或 mount capability 错当成通用路径授权。

## Requirements

- Project VFS mount grant 必须被重命名或收窄为 Project VFS mount exposure / preset grant；它不再表达 generic VFS authorization。
- 新增 runtime VFS access policy 模型，至少表达：
  - mount id / surface ref
  - path pattern 或 normalized path scope
  - operation set：read/list/search/write/exec/apply_patch
  - source：Project preset、PermissionGrant、system/runtime projection 等
- VFS tool admission 必须在 mount-relative path normalize 后执行，绝对路径和 `..` escape 不得进入 policy matching。
- Effective VFS access 必须是三者交集：
  - tool capability / tool policy 允许该工具可见；
  - runtime mount supports 该 operation；
  - `RuntimeVfsAccessPolicy` admits normalized mount/path/operation。
- PermissionGrant 对 VFS 的贡献必须被投影为 path-level policy，不得仅通过 tool-level grant 扩大 mount/path 访问。
- Shell/materialization 路径必须同样经过 runtime policy，不得只依赖 `MountCapability::Exec` 或 workspace root guard。
- 清理旧问题优先于添加 feature：不得新增一个旁路 “VfsAccessService” 而不删除旧 grant/mount capability 误用；若发现范围超过可接受修改，必须在 design note 中明确剩余旧路径和原因。
- Subagent 执行约束：研究 worker 只读；实现 worker 不跑大规模 Rust 编译或 broad suites。允许 scoped `rg`、format、小型定向 Rust tests。最终编译/集成校验由 check 阶段统一决定。

## Acceptance Criteria

- [x] 代码中 Project VFS mount grant 的命名/接口不再暗示 generic VFS authorization。
- [x] 存在一个 runtime VFS access policy model/compiler，当前从 runtime `Vfs` 编译 whole-mount policy。
- [x] VFS read/list/search/write/apply_patch/shell tool resolution 在 normalize 后检查 runtime policy。
- [x] Tests 覆盖 path normalize 后 allow/deny、mount supports operation 但 policy deny、tool capability grant 不扩大 mount/path access。
- [x] PermissionGrant VFS path rules 能进入 runtime policy，且不写入 provider mount capability。
- [x] Shell/materialization 对 mount/path/exec 的拒绝语义清晰。
- [x] Specs 更新记录 provider capability、tool visibility、runtime VFS policy 三者分工。

## Acceptance Review

PermissionGrant 的 VFS path-level 投影已通过独立 typed contract 收束：`requested_paths`
继续表达 tool capability path，`requested_vfs_access` 表达 mount/path/operation runtime
policy contribution。Active Grant 在 AgentRun runtime surface 查询时投影为
`RuntimeVfsAccessSource::PermissionGrant` 规则，provider mount capability 仍只表达 provider
support。

## Notes

- Source: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d9-vfs-per-mount--path-authorization`.
- This is Slice 7 from `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md`.

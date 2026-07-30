# Runtime VFS 工具失败链证据

## 运行态结论

- 故障会话的 AgentFrame revision 1/2 中，`main` 都由 `relay_fs` 提供，能力为 `read / write / list / search / exec`。
- revision 2 新增 Canvas mount，但没有改变 `main`。因此此次问题不是 Canvas，也不是 durable mount capability 漂移。
- 服务端历史中的相关 tool item 均已完成；当前没有 active turn。

## `fs_apply_patch`

调用使用规范路径：

```text
*** Update File: main://crates/.../SKILL.md
```

返回：

```text
not found: 路径不存在: main:/crates/.../SKILL.md
```

代码链：

1. `runtime_tool_authorization.rs` 的 `patch_paths` 正确解析 mount URI 并授予 Write。
2. `VfsService` 只在 `prefers_native_apply_patch()` 分支调用 `normalize_native_patch_paths`。
3. `relay_fs` 进入 composite `ProviderPatchTarget` 分支，原始 patch 被传给 `apply_patch_to_target`。
4. `apply_entries_to_target` 把 `main://...` 当作 mount 内路径归一化；Windows 上形成 `main:/...`，最终读文件失败。

现有定向测试：

```powershell
cargo test -p agentdash-application-vfs apply_patch -- --nocapture
```

结果为 18 passed，但缺少“显式 mount URI + non-native/relay provider”执行测试。

## `shell_exec`

失败调用使用：

```json
{
  "cwd": "main://",
  "command": "<PowerShell here-string，其中包含 main://... 与 lifecycle://... 文本>"
}
```

返回：

```text
mount main 不支持该能力
```

该调用没有 relay dispatch 日志，说明在本机执行前被拒绝。

代码链：

1. `shell_vfs_grant` 对显式 cwd 只生成 `Exec` grant。
2. `rewrite_shell_command_inner` 扫描整个 command，再对每个 URI 要求 `Read`。
3. 执行期 VFS surface 是本次调用的 grant 切片，故 `resolve_mount(... Read)` 失败。
4. `find_mount_uri_candidates` 不理解 PowerShell here-string 数据边界；即使补足 Read，也可能改写原本要写进文档的 URI 字面量。

## 终端与前端

- 服务端 tool event 已记录失败终态，active turn 为空。
- 当前 `sessionStreamReducer` 对 `item_completed` 固定生成 terminal lifecycle、`isStreaming = false` 并清除 pending approval。
- 失败发生在 relay dispatch 与 shell terminal registration 之前，因此不会创建 `useTerminalStore` 条目；交互式 terminal store 不参与这次工具卡终态。
- 没有前端残留运行态的代码证据，本任务不增加 reducer/store 补偿逻辑。

## 实施验证

- `cargo test -p agentdash-application-vfs single_mount_patch -- --nocapture`：2 passed。
- `cargo test -p agentdash-infrastructure shell_mount_exec -- --nocapture`：2 passed。
- `rewrite.rs` 独立单测 `powershell_here_string`：1 passed。
- 后续 workspace Cargo 命令被并行会话尚未完成的 `runtime_view.rs -> wire_u64::option` 引用阻断；该文件不属于本任务，未修改。

## 设计约束

- patch 的 caller-facing mount URI 与 provider-facing relative path 必须有唯一转换边界。
- shell 授权和物化必须共用资源候选语义。
- 命令代码与脚本数据需要明确词法边界；数据中的 URI 是普通文本。
- 同 backend 直连路径同样需要在 rewrite 前完成 VFS policy 校验。

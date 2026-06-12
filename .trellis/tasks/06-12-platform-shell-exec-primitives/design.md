# 平台 shell exec 原语设计

## Architecture

该任务扩展现有 `shell_exec` 工具，而不是新增 Agent-facing 工具。

```text
shell_exec
  cwd omitted          -> platform shell backend
  cwd = platform://... -> platform shell backend
  cwd = <exec mount>   -> existing provider exec / relay-local OS shell
```

平台 shell 是 application 层解释器，不启动 OS process。它消费当前 session runtime VFS snapshot、InlineContentOverlay、AuthIdentity 和 CapabilityState，按命令语义调用 VFS service。

单行命令的词法拆分复用 `shell-words` crate。该依赖只负责 Unix shell words 层面的 quoting / escape / splitting；平台 shell 不消费完整 shell AST，也不承诺 bash/POSIX 兼容。

## Execution Boundary

当前 `ShellExecTool` 在执行时先用 `resolve_uri_path(&vfs, params.cwd.unwrap_or("."))` 定位 exec mount，再调用 `VfsService.exec`。本任务需要把分派改为：

1. 解析 shell mode。
2. `cwd` 缺失或显式 `platform://...` 时进入 platform shell。
3. 其他 `cwd` 仍解析为 exec-capable mount，并走现有 materialization + provider exec。

平台 shell 不适合作为普通 mount provider 的孤立 `exec` 实现，因为 `cp source dest` 可能跨 mount，需要整份 runtime VFS 与 VFS service，而 provider `exec(mount, request, ctx)` 只拥有单 mount 视角。

## Platform Shell Context

```text
PlatformShellContext
  vfs: runtime VFS snapshot
  service: VfsService
  cwd: optional platform cwd ResourceRef or root
  overlay: optional InlineContentOverlay
  identity: optional AuthIdentity
  capability_state: CapabilityState or precomputed read/write grants
```

`VfsToolFactoryInput` 当前已经传入 `CapabilityState` 给工具构建。`ShellExecTool` 需要保存足够信息，让 platform shell 在命令内部校验 `file_read` / `file_write`。

## Path Rules

- VFS URI 使用现有 `mount_id://relative/path` 语法。
- 相对路径基于 platform shell cwd 解析。
- `cwd` 缺失时 platform shell cwd 为一个平台逻辑根；要求跨 mount 路径使用显式 URI。
- `cwd = platform://lifecycle/session` 可作为后续增强；MVP 可先接受 `platform://` 和显式 VFS URI。

## Commands

MVP 命令：

| Command | Behavior |
| --- | --- |
| `pwd` | 输出 platform shell cwd |
| `ls [path]` | VFS list |
| `cat <path>` | VFS read text |
| `cp <source> <dest>` | read text source + write text dest |
| `mv <source> <dest>` | same-mount rename when available; cross-mount copy + delete |
| `rm <path>` | delete text |
| `echo ...` | stdout |
| `echo ... > path` | write text |
| `cat source > dest` | read text source + write text dest |

Unsupported syntax returns a shell-style error instead of trying to run OS shell.

## Parser Boundary

第一版 parser 分两层：

1. 轻量预检：拒绝 newline、pipe、subshell、glob token、后台任务等不支持语法。
2. `shell-words::split`：解析单行命令为 argv，复用公开库处理 quote 与 escape。

重定向只支持 `>` 作为独立 token，例如 `echo "done" > lifecycle://records/summary.md` 与 `cat source > dest`。其他重定向形式返回 unsupported syntax。

## Permission Model

平台 shell 不能只依赖 `shell_execute`。

| Operation | Required grants |
| --- | --- |
| enter `shell_exec` | `shell_execute` tool enabled |
| `ls` / `cat` | `file_read` tool capability + source mount Read |
| `cp` | `file_read` + source Read + `file_write` + destination Write |
| `mv` | `file_read` + source Read + `file_write` + destination Write/Delete |
| `rm` | `file_write` + target Write |
| `echo >` | `file_write` + destination Write |

Tool-level policy should remain enforced: if `file_read::fs_read` is blocked, platform shell read primitives must be blocked; if `file_write::fs_apply_patch` is blocked, write primitives must be blocked unless the project introduces a dedicated write primitive policy.

## Result Shape

返回仍使用 `AgentToolResult`，文本输出保持 shell-like：

```text
command: cp lifecycle://session/tools/a.json lifecycle://artifacts/a.json
cwd: platform://
exit_code: 0
copied lifecycle://session/tools/a.json -> lifecycle://artifacts/a.json
```

`details` 可标记：

```json
{
  "type": "platform_shell_exec",
  "command": "cp ...",
  "operations": [
    { "kind": "copy", "source": "...", "destination": "..." }
  ]
}
```

## Trade-Offs

- 默认 `cwd` 缺失进入平台 shell 会改变当前默认行为。项目处于预研期，该取舍换来低门槛平台原语入口。
- 不做完整 shell parser 可以控制范围，但必须给清晰错误，避免 Agent 误以为支持 pipes/globs。
- 复用 `shell-words` 可以避免自写 quote/split；命令语义仍保留在 VFS application 层，避免引入完整 shell 语义。
- 不新增 `fs_copy` 等工具减少工具面，但 platform shell 内部需要维护一组命令 handler 和权限矩阵。

## Rollback

实现应集中在 `shell_exec` 分派、platform shell executor 和工具描述。若出现问题，可以回退默认分派到现有 `"."` 行为，并保留显式 `platform://` 入口作为可控能力。

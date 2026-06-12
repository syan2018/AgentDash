# 平台 shell exec 原语

## Goal

让现有 `shell_exec` 获得一个平台内建的受限执行后端，用 Unix-ish 原语操作当前 session runtime VFS。Agent 仍然只学习和调用 `shell_exec`，但在未提供 `cwd` 时默认进入平台 shell；显式提供普通 exec-capable mount 的 `cwd` 时继续走现有本机 / relay OS shell。

这个能力服务低门槛的跨 mount 操作，例如把 lifecycle/session/tool 投影实例化为 lifecycle artifact、record 或其他可写 mount 文件：

```sh
cp lifecycle://session/tools/shell_exec-7.json lifecycle://artifacts/debug_trace.json
cat lifecycle://session/tools/shell_exec-7.json
ls lifecycle://session/tools
echo "done" > lifecycle://records/summary.md
```

用户价值是减少低频边角文件操作的工具面膨胀。Agent 不需要学习 `fs_copy` / `fs_move` / `artifact_capture` 等专用工具，而是复用 `shell_exec` 和常见 Unix 原语完成平台地址空间内的操作。

## Requirements

- `shell_exec` 保持唯一 Agent-facing 命令执行入口；不新增独立的 `vfs_shell` / `fs_copy` 等 Agent 工具。
- `shell_exec.cwd` 未提供时默认走平台 shell 后端，并在工具描述中明确说明此时可用的是受限 Unix-ish 原语集合。
- `shell_exec.cwd` 显式指向普通 exec-capable mount 时，继续走现有本机 / relay OS shell 行为。
- 平台 shell 不启动 OS 进程，不依赖本机 terminal relay；它解释命令并通过 runtime VFS / provider 执行读写。
- 平台 shell 复用公开 shell words 词法库处理基础 quoting / escape / splitting；平台只实现命令分派、VFS 语义和权限裁决，避免重复造 shell 词法轮子。
- 平台 shell 必须能访问当前 session 的完整 runtime VFS snapshot，支持跨 mount 操作。
- MVP 命令集至少覆盖 `pwd`、`ls`、`cat`、`cp`、`mv`、`rm`、`echo`。
- MVP 支持单行命令、基础 quoted string 参数、VFS URI、以及基于平台 shell cwd 的相对路径。
- MVP 支持窄重定向：`echo "text" > path` 与 `cat source > path`。
- `cp` 第一版只要求支持单文件 text copy；目录递归、glob、pipe 和完整 POSIX shell 语法不进入第一版。
- 权限不能通过平台 shell 绕过：调用入口仍需 `shell_execute`，命令内部读写还需消费 `file_read` / `file_write` 与 mount capability。
- 写入 lifecycle artifact 继续受 `lifecycle_vfs` 的 `writable_port_keys` 裁决。
- session feed / UI 中仍表现为一次 `shell_exec` command execution，方便复用现有展示、审计和 hook 语义。

## Acceptance Criteria

- [ ] 不传 `cwd` 调用 `shell_exec` 时，命令由平台 shell 后端执行，而不是默认落到 workspace OS shell。
- [ ] 显式传入普通 exec mount `cwd` 时，现有 OS shell 执行路径保持可用。
- [ ] `shell_exec` 工具描述说明未提供 `cwd` 时会使用平台 shell，并列出 MVP 受限原语。
- [ ] `cat lifecycle://...` 能读取当前 session runtime VFS 中的 text 投影。
- [ ] `cp lifecycle://session/... lifecycle://artifacts/{port_key}` 能将 text 投影写入允许的 lifecycle artifact port。
- [ ] `echo "..." > lifecycle://records/summary.md` 能写入 journey record。
- [ ] 缺少 `file_read` 时，`cat` 与 `cp` source 读取被拒绝。
- [ ] 缺少 `file_write` 时，`cp` destination、`rm`、`mv`、`echo >` 写入被拒绝。
- [ ] 写入未授权 lifecycle artifact port 时仍返回 provider 层拒绝。
- [ ] 普通 OS shell 的 VFS URI materialization 行为不被平台 shell 改动破坏。
- [ ] 第一版明确拒绝 pipe、glob、subshell、多行 script、目录递归等未支持语法，并给出可理解错误。
- [ ] 平台 shell 的单行解析复用轻量 shell words 库，不手写完整 quote/split 逻辑。

## Notes

- 当前代码中 `shell_exec.cwd` 未提供时会解析 `"."` 并通常落到 VFS default mount。该任务计划将默认行为调整为平台 shell，以便无门槛提供通用平台原语。
- 项目处于预研期，不需要兼容旧默认行为；但实现必须保持显式 OS shell cwd 的行为清晰可用。

## Out of Scope

- 完整 bash / POSIX shell 兼容。
- 管道、变量展开、subshell、环境变量、后台任务。
- `cp -r`、目录复制、glob 展开。
- binary copy 与大型文件 streaming。
- 新增独立 Agent-facing 文件操作工具。

## Open Questions

- 已定：未提供 `cwd` 时无条件进入平台 shell；真实 OS shell 必须显式提供普通 exec mount `cwd`。

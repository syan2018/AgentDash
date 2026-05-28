# VFS 搜索工具默认忽略策略收束

## Goal

让 Agent-facing VFS 搜索工具在默认工作区扫描时自动避开依赖、构建产物和 ignore 规则命中的噪音内容，同时保留用户显式指定 ignored subtree 时进入搜索的能力。

该任务面向 `fs_glob` / `fs_grep` 及其 relay/local backend 执行路径。目标是让工具结果更接近开发者使用 `rg` / glob 搜索源码时的直觉：默认结果干净，显式路径有明确意图时可检查生成物、依赖包或其它被普通 ignore 规则覆盖的目录。

## Confirmed Facts

- `fs_glob` 当前通过 `RelayVfsService::list` 转发到 provider，工具层只显式过滤 VCS 元数据目录。
- `fs_grep` 当前通过 `RelayVfsService::grep_text_extended` 转发到 provider；relay_fs 会把 grep 请求下发给本机后端。
- 本机 `ToolExecutor::file_list` 使用目录遍历和可选 glob matcher，当前没有统一应用 `.gitignore` / `.ignore` / 内置噪音目录策略。
- 本机 `ToolExecutor::search` 有 ripgrep 路径和 fallback 路径；fallback 有独立硬编码跳过目录，和 `file_list` 行为不统一。
- Claude Code `GrepTool` 使用 ripgrep 默认 ignore 规则并显式排 VCS 元数据目录；Claude Code `GlobTool` 参考实现默认使用 `--no-ignore`，但本项目的 VFS 工具更适合默认返回工作区有意义内容。
- VFS 规范要求 mount-relative path 在 application 层前 normalize，绝对路径与 `..` escape 必须失败；云端后端不能直接访问本机文件系统。

## Requirements

- 默认从 mount root / cwd 进行 glob 或 grep 时，搜索应应用工作区 ignore 规则与内置噪音目录策略。
- 普通 ignore 规则包括工作区内 `.gitignore` / `.ignore` 可表达的忽略语义；实现可优先使用系统已有 `rg` / ignore crate 能力，但行为必须可测试。
- 内置噪音目录至少覆盖常见依赖、构建与缓存目录：`node_modules`、`target`、`dist`、`build`、`.next`、`.venv`、`__pycache__`。
- 显式 `path` 指向普通 ignored subtree 时，该 subtree 是用户搜索目标，工具应允许进入并返回结果。
- VCS 元数据目录继续作为 hard exclude：`.git`、`.svn`、`.hg`、`.bzr`、`.jj`、`.sl` 不参与默认 glob / grep 结果。
- workspace root 边界、mount capability、path normalize 与路径逃逸保护继续优先于 ignore 策略。
- `fs_glob` 与 `fs_grep` 的工具描述应说明默认忽略策略和显式 `path` 的进入语义，避免 Agent 把 ignored subtree 的默认缺席误解为文件不存在。
- relay/local backend、ripgrep 路径和 fallback 路径应共享同一产品语义，避免同一次搜索因为本机是否安装 `rg` 得出相互矛盾的结果。

## Acceptance Criteria

- [x] 默认 `fs_glob` 递归扫描不返回 `.gitignore` / `.ignore` 命中的普通 ignored subtree 内容。
- [x] 默认 `fs_glob` 递归扫描不返回内置噪音目录中的内容。
- [x] `fs_glob` 显式 `path` 指向普通 ignored subtree 时可以列出该 subtree 中匹配内容。
- [x] 默认 `fs_grep` 不搜索 `.gitignore` / `.ignore` 命中的普通 ignored subtree 内容。
- [x] `fs_grep` 显式 `path` 指向普通 ignored subtree 时可以搜索该 subtree 中匹配内容。
- [x] ripgrep 路径与 fallback search 路径在上述 ignore / explicit path 语义上保持一致。
- [x] VCS 元数据目录不会出现在默认 glob / grep 结果中。
- [x] 新增或更新的测试覆盖 local backend 文件列表、grep 搜索和显式 ignored subtree 进入行为。
- [x] `fs_glob` / `fs_grep` 工具描述更新后能准确表达默认忽略策略。

## Scope Boundaries

- 本任务不新增用户可见开关；默认语义直接收束到正确状态。
- 本任务不改变 `fs_read` 对显式文件路径的读取能力，读取仍以路径边界与文件类型语义为准。
- 本任务不修改数据库 schema。

## Open Questions

- 已确认：显式 `path` 指向 VCS 元数据目录时仍保持 hard exclude。

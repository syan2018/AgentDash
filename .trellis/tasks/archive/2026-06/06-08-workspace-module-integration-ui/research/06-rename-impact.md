# Research: 改名影响面（slug rename + 文档 + 任务目录安全做法）

- **Query**: grep "surface-registry" / "agent-runtime-surface-registry" 在 .trellis 内引用；docs 是否补章节；任务目录 rename 安全做法
- **Scope**: internal
- **Date**: 2026-06-08

## Findings

### slug 引用全清单（.trellis 内）

目标改名：`agent-runtime-surface-registry` → `workspace-module-registry`（parent slug；`surface` 仅保留为底层 runtime projection 命名）。

**parent 自身**：
- 目录：`.trellis/tasks/06-08-agent-runtime-surface-registry/`
- `task.json:2` `"id": "agent-runtime-surface-registry"`
- `task.json:3` `"name": "agent-runtime-surface-registry"`
- `task.json` `children: ["06-08-workspace-module-read-contract", "06-08-workspace-module-operate", "06-08-workspace-module-integration-ui"]`（已是 dir 名形式，含 `06-08-` 前缀）
- `design.md:216` slug 改名待办行（自引用，文案）

**三个 child 的 `parent` 字段**（均指向带前缀的 dir 名 `06-08-agent-runtime-surface-registry`）：
- `.trellis/tasks/06-08-workspace-module-read-contract/task.json:22`
- `.trellis/tasks/06-08-workspace-module-operate/task.json:22`
- `.trellis/tasks/06-08-workspace-module-integration-ui/task.json:22`

**child prd/design 的文案引用**（非结构字段，正文里的 `> Parent: ...`）：
- read-contract `prd.md:3`、`design.md:3`
- operate `prd.md:3`、`design.md:3`
- integration-ui `prd.md:3`、`prd.md:13`

**check.jsonl**（parent）：
- `.trellis/tasks/06-08-agent-runtime-surface-registry/check.jsonl:4,5` 含 parent 路径字符串（若目录改名，这些 file 路径也需同步）。

> 注意命名层级两种形态：
> - `task.json.id`/`.name` = 不带日期前缀（`agent-runtime-surface-registry`）。
> - 目录名 + child `parent` 字段 + `children` 数组 = 带前缀（`06-08-agent-runtime-surface-registry`）。
> 改名要两种都覆盖，保持一致。

### 任务目录 rename 的安全做法

- **task.py 无 rename/move 命令**（CLI 子命令：create/add-context/validate/start/current/finish/set-branch/set-base-branch/set-scope/archive/list/add-subtask/remove-subtask/list-archive）。改名需手工。
- **git 安全性**：四个 `06-08-*` 任务目录均**未被 git 跟踪**（初始 git status 显示为 `??` 未跟踪；唯一已修改的是本 child 的 task.json）。因此 `git mv` 不适用，直接文件系统重命名即可，无历史包袱。
- **rename checklist**（建议在 implement.md 固化）：
  1. 重命名 parent 目录 `06-08-agent-runtime-surface-registry` → `06-08-workspace-module-registry`。
  2. parent `task.json`：`id` + `name` 改为 `workspace-module-registry`（不带前缀，与现有约定一致）。
  3. parent `task.json.children`：三个值若按前缀约定保持（child 目录未改名，无需动）。
  4. 三个 child `task.json.parent`：`06-08-agent-runtime-surface-registry` → `06-08-workspace-module-registry`。
  5. parent `check.jsonl` 内的路径字符串同步新目录。
  6. 文案引用（child prd/design 的 `> Parent:` 行、parent design.md:216）按需更新。
  7. `task.py current --source` 当前活动是 integration-ui，未指向 parent，rename parent 不影响 active 指针；但若有任何 source pointer 指向旧 parent slug，需核对 `.trellis` 下 session/active 记录。
  8. 改名后跑 `python ./.trellis/scripts/task.py validate`（如支持）或 `list` 确认父子链完整。

### docs/extension-system.md

- 文件存在，章节结构：`# AgentDash 插件系统开发与使用` → `## 插件项目结构 / Manifest / Host APIs / Runtime Actions / Protocol Channels / Panel Bridge / 本地开发流程 / 打包安装和试用 / 示例`。
- **当前无 "Workspace Module" 章节**。"surface" 仅作描述性词出现 3 处（行 76/100/137：runtime action surface / extension host surface / API surface），均指底层 runtime projection 语义，与改名后"surface 保留为底层命名"一致，无需改。
- R4 需新增一节（如 `## Workspace Module`）说明：Workspace Module 是 Extension + Canvas + Builtin 聚合的统一协作模块；与 Runtime Surface（runtime_actions / protocol_channels 等底层 projection）的术语边界。

## Caveats / Not Found

- 代码侧（crates/packages）无 `surface-registry` slug 引用——改名纯属 .trellis 任务元数据 + 文档范畴，不触碰代码（代码里的 `Surface`/`surface` 是 runtime projection 命名，保留）。
- 是否需要保留旧目录别名/软链以防外部引用：未发现外部引用，无需。

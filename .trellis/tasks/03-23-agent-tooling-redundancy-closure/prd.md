# Agent 工具体系冗余收口

## 背景

本次 review 发现，项目中提供给 agent 使用的工具体系已经出现几类可感知的冗余：

- 同一类文件/命令访问能力同时存在旧 builtin 工具名、当前 address space runtime 工具名，以及规范目标命名三套语言
- 部分流程控制工具被无条件注入到 address space session，但并非所有 session 都真正可用
- 还有一批通过 per-session MCP server 暴露给 agent 的工具，当前在 tool visibility 中只被折叠成 `mcp_tools`
- relay 侧仍保留 `command.workspace_files.*` 遗留协议链，与当前统一 address space 访问链形成并行维护

这些问题已经开始带来：

- prompt/tool visibility 与真实工具面不一致
- hook 规则需要兼容多套工具别名
- 工具收口目标和现状实现持续偏离

## 目标

为 Agent 工具体系建立一份明确的收口任务，推动后续实现满足以下方向：

1. agent 可见工具面只保留一套正式命名与参数模型
2. session 暴露的工具集合与 `tool_visibility`/runtime policy 保持一致
3. 遗留协议或旧工具链要么迁移到统一实现，要么明确冻结/删除
4. MCP 暴露的工具面、scope 和写能力边界需要被显式纳入同一套 review 与收口范围

## 问题范围

### 1. 工具命名与契约分叉

当前至少存在以下几套工具语言：

- 旧 builtin：`read_file` / `write_file` / `list_directory` / `search` / `shell`
- 当前 runtime：`mounts_list` / `fs_read` / `fs_write` / `fs_list` / `fs_search` / `shell_exec`
- 规范目标：`mounts.list` / `fs.read` / `fs.write` / `fs.list` / `fs.search` / `shell.exec`

需要明确：

- 最终保留哪一套名字
- 兼容层是否短期保留
- hook / prompt / tool visibility / 文档如何同步收敛

### 2. 流程工具暴露面过宽

当前 runtime provider 除了访问工具，还会注入：

- `report_workflow_artifact`
- `companion_dispatch`
- `companion_complete`
- `resolve_hook_action`

这些工具并非所有 session 都适用。需要明确：

- 哪些工具属于“基础运行时工具”
- 哪些工具属于“仅特定 workflow / companion / hook session 可见的条件工具”
- 工具暴露是否应由 session 能力和上下文状态共同裁剪

### 3. 遗留 workspace_files 协议链

当前 API 的 workspace files 路由已经主要走统一 address space 访问链，但 relay 协议与本机 handler 中仍保留：

- `command.workspace_files.list`
- `command.workspace_files.read`

需要确认：

- 这条协议链是否仍有真实生产调用方
- 是否应迁移、冻结或删除
- 是否还有重复的路径校验、错误语义和返回结构维护成本

### 4. MCP 暴露工具面未纳入同一套收口

当前项目会按 session 注入三类 MCP server：

- `relay` scope：面向项目/全局看板操作
- `story` scope：面向 Story 上下文管理与 Task 编排
- `task` scope：面向 Task 状态更新、产物上报与同 Story 协调

目前已识别的 MCP tool 范围包括但不限于：

- relay: `list_projects`、`get_project`、`create_story`、`list_stories`、`get_story_detail`、`update_story_status`、`update_project_context_config`
- story: `get_story_context`、`update_story_context`、`update_story_details`、`create_task`、`batch_create_tasks`、`advance_story_status`
- task: `get_task_info`、`update_task_status`、`report_artifact`、`get_sibling_tasks`、`get_story_context`、`append_task_description`

这些工具当前存在几个需要纳入 review 的问题：

- session plan / tool visibility 只把它们折叠为 `mcp_tools`，没有反映真实工具名、scope 和写操作风险
- 实际注入到 PiAgent 时会被转成带 server 名前缀的动态名字，如 `mcp_agentdash_task_tools_xxx_update_task_status`
- hook / policy 层已经开始按“名字后缀”兼容 MCP tool，例如 `update_task_status`
- MCP tool 与本地 runtime tool / 流程工具之间的职责边界尚未统一描述

需要明确：

- MCP tool 是否属于与 runtime tool 并列的一等工具面
- tool visibility 是否需要展开到 server scope 或具体 tool 名级别
- 哪些写操作应纳入统一审批/策略控制
- task/story/relay 三层 MCP 是否存在能力重叠或职责漂移

## 非目标

- 当前任务不直接改造所有工具实现
- 当前任务不处理数据库兼容或历史迁移兼容方案
- 当前任务不扩展新的 agent 工具能力
- 当前任务不替代对单个 MCP server 的详细产品设计，只负责工具体系层面的收口

## 产出要求

- 输出一份工具体系收口设计，明确正式工具面、条件工具面和遗留链路处理策略
- 输出一份 MCP 工具面清单与分层边界说明，明确 relay/story/task 各层职责
- 给出分阶段实施顺序，避免一次性大改
- 标注受影响模块：agent、executor、api、local、relay、hook runtime、session plan

## 初步收口顺序

1. 统一正式工具命名与参数契约
2. 让 `tool_visibility` 与实际注入工具集合对齐
3. 将流程工具改为按 session 条件注入
4. 审查 MCP 工具面，明确 scope、命名、可见性和审批边界
5. 审查并处理 `command.workspace_files.*` 遗留链
6. 清理 hook 里的别名兼容逻辑与相关文档漂移

## 验收标准

- [ ] 明确一套正式 agent 工具命名，不再允许并行扩张多套语言
- [ ] `tool_visibility` 能准确表达当前 session 真实可见工具
- [ ] 流程工具具备明确的注入条件，不再对无关 session 无条件暴露
- [ ] MCP 工具面被纳入同一份工具清单，而不是仅以 `mcp_tools` 概括
- [ ] relay/story/task 三层 MCP 的职责边界和写能力风险被明确记录
- [ ] `workspace_files` 遗留链的命运被明确记录为保留、冻结或删除
- [ ] 后续实现任务能够基于此 PRD 继续拆分推进

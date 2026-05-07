# Workflow 动态上下文与模板化 Locator

## Goal
把 workflow 运行时所需的动态上下文统一收敛到 lifecycle VFS 下的真实资源路径中，避免在代码里硬编码 `execution_context` / `review_checklist` 这类语义型 key；同时为 `WorkflowContextBinding.locator` 增加受限模板变量能力，使 workflow 可以声明式引用当前 run / node 生成的上下文资源。

## Background
当前系统已经确认两件事：

1. 项目不希望继续保留“语义型 context key”：
   - `context://execution_context`
   - `context://review_checklist`
   - `context://workspace_journal`
   - `context://workflow_archive_action`
   - `context://story_context_snapshot`
   - `context://project_session_context`

2. workflow 后续仍然需要引用动态上下文，但这些上下文应表现为 **真实的 VFS 资源**，而不是代码拼出来的说明文本。

因此，本次迭代的目标不是恢复 `context_vfs` 这类 provider，而是建立一套基于 lifecycle VFS 的统一资源模型。

## Problem Statement
当前 `WorkflowContextBinding.locator` 已经具备统一的 `parse_mount_uri -> read_text` 读取链路，但缺少两个能力：

- 没有一个正式的“动态上下文目录”约定，workflow 无法稳定引用运行时生成的上下文资源。
- locator 还不能根据 run / node / binding 等运行时变量动态定位资源路径。

结果就是：

- 一旦 workflow 需要动态上下文，容易退回“代码硬编码文本 key”的老路。
- hook / lifecycle / workflow 三层之间没有稳定的上下文资源契约。

## Scope
本 task 关注以下内容：

- 在 lifecycle VFS 中定义统一的动态上下文目录约定。
- 为 `WorkflowContextBinding.locator` 增加受限模板变量展开能力。
- 明确 hook / lifecycle / artifact 如何把上下文物化到 lifecycle VFS。
- 给出 builtin workflow 的迁移方式和约束。

本 task **不** 直接实现完整业务逻辑，也不要求一次性打通所有 workflow。

## Design Principles
- locator 必须引用 **真实可读的 VFS 资源**，不能再引用“语义型 key”。
- resolver 保持通用，只做：
  - 模板展开
  - URI 解析
  - `read_text`
- resolver 不理解业务语义，不应出现：
  - checklist 是什么
  - archive_action 是什么
  - journal_target 是什么
- 动态上下文由 lifecycle / hook / workflow runtime 写入 VFS，workflow 只声明式读取。

## Proposed Path Model

### Run 级共享上下文
- `lifecycle://active/context/<name>`
- `lifecycle://runs/<run_id>/context/<name>`

适用场景：
- 当前 run 的执行摘要
- 当前 run 的共享说明
- 对所有 node 可见的全局上下文

### Node 级上下文
- `lifecycle://nodes/<node_key>/context/<name>`

适用场景：
- 当前 node 的 review 输入
- 当前 node 的补充说明
- 某个 node 的动态生成中间结果

### 约束
- `context/` 目录下的资源应表现为普通 VFS 文件，例如 `.md` / `.txt` / `.json`
- workflow 不依赖文件扩展名语义，但建议首轮统一用 Markdown 文本

## Locator Template Proposal

### First-class syntax
建议首轮采用最简单的 Mustache 风格变量替换：

```text
lifecycle://nodes/{{active_step_key}}/context/review.md
lifecycle://runs/{{run_id}}/context/summary.md
lifecycle://active/context/{{binding_kind}}-overview.md
```

### First version allowed variables
- `run_id`
- `lifecycle_key`
- `active_step_key`
- `binding_kind`
- `binding_id`
- `project_id`
- `story_id`
- `task_id`

### Explicitly unsupported in v1
- 条件分支
- 循环
- 任意表达式
- 函数调用
- 跨 provider 引用拼接
- 从任意 JSON 字段动态取值

### Missing variable behavior
- 模板变量不存在或为空时，locator 解析失败
- `required=true` 的 binding 直接报错
- `required=false` 的 binding 记录 warning 并跳过

## Runtime Materialization Model
动态上下文的写入者可以是：

- lifecycle orchestrator
- hook runtime
- workflow 相关内置工具

但写入结果必须统一表现为 lifecycle VFS 下的文件资源，例如：

- hook 在某个节点结束时写入：
  - `lifecycle://nodes/check/context/review.md`
- lifecycle 在 run 开始时写入：
  - `lifecycle://active/context/session-overview.md`

workflow 不关心“谁写的”，只关心 locator 能否读取。

## Migration Strategy

### Phase 1
- 删除语义型 `context://...` locator
- 只保留真实 VFS 路径绑定
- 让 builtin workflow 先回退到 instruction / hook / artifact 驱动

### Phase 2
- lifecycle VFS 支持 `context/` 目录族
- 可以由 runtime 在执行时动态写入上下文文件

### Phase 3
- locator 支持模板变量
- builtin workflow 改为引用 `lifecycle://.../context/...`

## Acceptance Criteria
- [ ] 设计一套统一的 lifecycle context 路径约定，并明确 run 级 / node 级上下文边界。
- [ ] 明确第一版模板变量集合、缺失变量行为和禁用能力范围。
- [ ] 明确动态上下文的物化责任边界：谁写、写到哪、workflow 如何读。
- [ ] 给出 builtin workflow 的迁移示例，说明如何从语义型 key 迁移到真实 VFS 路径。
- [ ] 明确 resolver 保持通用，不引入新的业务语义分支。

## Non-Goals
- 不要求本 task 内直接实现完整的 lifecycle `context/` 目录读写。
- 不要求本 task 内直接完成所有 builtin workflow 的生产级迁移。
- 不要求把模板能力扩展成脚本语言。

## Open Questions
- node 级和 run 级上下文是否都需要历史可追溯路径，还是首轮只支持 `active/`？
- 动态 context 文件是否需要持久化进入 `record_artifacts` 或 event log？
- 模板变量是否需要支持 `current_node_key` 与 `active_step_key` 的兼容命名？

## Notes
- 本 task 用于跟踪后续迭代方向。
- 当前已经完成的前置收敛是：删除语义型 `context://...` 硬编码路径。

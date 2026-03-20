# Trellis Workflow 平台映射

## 文档目标

这份文档把当前 `.trellis/` 工作流中的现实元素，映射到 AgentDash 平台里的目标对象、phase、artifact 和运行时落点。

它是 `Trellis Workflow 平台化映射` 任务的第一份正式产出，后续将直接作为：

- `workflow-definition-and-assignment-model` 的建模输入
- `trellis-dev-workflow-golden-path` 的实现输入

## 当前 Trellis Workflow 的现实结构

从现有仓库看，Trellis workflow 已经由以下元素组成：

- 会话启动入口：`$start` + `.trellis/workflow.md`
- 当前任务容器：`.trellis/tasks/<date>-<slug>/`
- 需求定义：`prd.md`
- 任务元信息：`task.json`
- phase-specific context：`implement.jsonl / check.jsonl / debug.jsonl`
- 当前任务切换：`.current-task`
- 执行记录：`.trellis/workspace/<developer>/journal-*.md`
- 任务收尾：`finish-work`
- 归档动作：`task.py archive`

这些元素已经天然构成了一条阶段化 workflow，只是目前仍以目录结构、脚本和约定存在。

## 平台对象映射

### 1. Task Directory -> `WorkflowTargetContext`

`.trellis/tasks/<task>/` 目录本身不是 WorkflowDefinition，而是某个 workflow run 的目标上下文容器。

它当前承载：

- 需求定义
- 执行范围
- phase-specific context
- 当前状态元信息

平台侧建议映射为：

- `WorkflowTarget`
- `WorkflowTargetContext`

其中：

- `WorkflowTarget` 表示 workflow 作用的业务目标
- `WorkflowTargetContext` 表示该目标附带的执行上下文资料集合

### 2. `prd.md` -> `WorkflowGoalSpec`

`prd.md` 当前承担：

- 目标定义
- 需求边界
- 验收条件
- out of scope

平台侧建议映射为：

- `WorkflowGoalSpec`

它不应被降级成普通附件，因为它直接决定 workflow 的目标、约束和完成判据。

### 3. `task.json` -> `WorkflowRunMeta`

`task.json` 当前承载：

- 标题、描述、优先级、assignee
- 当前状态、phase、next_action
- 相关文件、备注

平台侧建议拆成两部分：

- `WorkflowRunMeta`
- `WorkflowExecutionBacklogMeta`

其中：

- 业务上稳定的运行信息属于 `WorkflowRunMeta`
- Trellis 特有的 CLI / PR / branch 元信息属于 backlog 或 task 管理附属信息

### 4. `implement/check/debug.jsonl` -> `PhaseContextBinding`

这是 Trellis 最接近平台 phase 机制的部分。

它们当前已经表达：

- 不同 phase 注入不同上下文
- context 不是固定写死，而是按任务配置选择

平台侧建议映射为：

- `PhaseContextBinding`

它至少应表达：

- phase 名称
- 上下文来源列表
- 每个来源的 reason
- 是否必须注入

### 5. workspace journal / archive -> `WorkflowRecordArtifact`

journal 与 archive 当前承担 workflow 的“收尾输出”职责。

平台侧建议映射为：

- `WorkflowRecordArtifact`

细分可包括：

- `SessionSummaryArtifact`
- `JournalUpdateArtifact`
- `ArchiveSuggestionArtifact`

## Phase 映射

### Phase 1: Start

当前 Trellis 现实动作：

- 读取 workflow / context
- 获取当前 developer / git / active tasks
- 识别当前任务
- 读取 spec 和 guideline

平台侧建议职责：

- 识别 workflow target
- 绑定 required reading
- 生成 start phase context
- 输出当前 run 的初始 phase state

当前代码可复用落点：

- `.trellis/workflow.md`
- `get_context.py`
- `SessionPlan` 的 persona / workflow / runtime policy 结构

### Phase 2: Implement

当前 Trellis 现实动作：

- 使用 implement context
- 推进开发
- 执行必要验证

平台侧建议职责：

- 绑定 implement context
- 启动或复用执行 session
- 记录实现阶段的 session output

当前代码可复用落点：

- `TaskExecutionGateway`
- `build_task_agent_context`
- `Project / Story / Task Session`
- `address_space`

### Phase 3: Check

当前 Trellis 现实动作：

- 使用 check context
- 运行 `finish-work`
- 对照规范做 review / 收尾检查

平台侧建议职责：

- 绑定 check context
- 执行 checklist / review policy
- 输出 check result

当前代码可复用落点：

- `check.jsonl`
- Session context snapshot
- 前端 Session 解释面

### Phase 4: Record

当前 Trellis 现实动作：

- 记录 session
- 更新 journal
- 归档已完成 task

平台侧建议职责：

- 生成结构化 summary
- 输出 journal suggestion
- 输出 archive suggestion

当前代码可复用落点：

- `SessionBinding`
- workspace journal 目录
- `task.py archive`

## 当前代码与目标平台对象的对应关系

| Trellis 元素 | 平台对象 | 当前代码落点 |
|---|---|---|
| `prd.md` | `WorkflowGoalSpec` | 仍在 task 目录，尚未平台化 |
| `task.json` | `WorkflowRunMeta` | 仍在 task 目录，尚未平台化 |
| `implement/check/debug.jsonl` | `PhaseContextBinding` | 任务目录 + hook 注入，尚未平台化 |
| `SessionPage` 上下文解释 | `WorkflowPhaseView` 的一部分 | `frontend/src/pages/SessionPage.tsx` |
| `session_plan` | `PhaseContextRenderer` 的一部分 | `crates/agentdash-application/src/session_plan.rs` |
| `SessionBinding` | `RunSessionLink` | `crates/agentdash-domain/src/session_binding/entity.rs` |
| journal / archive | `WorkflowRecordArtifact` | `.trellis/workspace/*` + `task.py archive` |

## 当前最关键的缺口

### 1. 缺正式 `WorkflowDefinition`

当前 phase 是散落在文档和 task 目录中的，没有正式对象描述。

### 2. 缺正式 `WorkflowRun`

现在 task 目录和 session binding 共同承担了 run 的一部分语义，但没有统一运行对象。

### 3. 缺 phase state

当前只有 task.json 里的粗粒度状态，没有正式的 phase progression 结构。

### 4. 缺 record artifact 结构化输出

当前 journal / archive 依然偏脚本和文档驱动，尚未成为平台内正式 artifact。

## 推荐建模边界

### 本任务内先明确，但不实现的对象

- `WorkflowDefinition`
- `WorkflowAssignment`
- `WorkflowRun`
- `WorkflowPhaseState`
- `WorkflowRecordArtifact`

### 本任务内明确不做的内容

- 不直接落 runtime 表结构
- 不直接落 API
- 不直接做 UI
- 不直接做 control plane

## 对后续任务的直接输入

### 给 `workflow-definition-and-assignment-model`

应直接消费：

- phase 列表
- phase 绑定的上下文语义
- Trellis 元素与平台对象映射

### 给 `trellis-dev-workflow-golden-path`

应直接消费：

- `Start / Implement / Check / Record` 四阶段边界
- 每个 phase 当前可复用的代码落点
- record / archive 应作为 workflow 输出，而不是额外手工动作

## 当前结论

Trellis workflow 已经不是“可以参考的流程”，而是当前项目最接近正式 workflow 产品形态的一条现实主航道。

因此它的正确推进方式不是继续把 `.trellis/` 当特殊脚手架，而是：

- 先把它映射成平台对象
- 再让平台显式承接这条 workflow

这就是本任务后续继续推进的正确方向。

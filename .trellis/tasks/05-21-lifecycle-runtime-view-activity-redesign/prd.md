# Lifecycle Runtime 观察视图基于 Activity 模型重设计 — PRD

## 背景

`lifecycle-session-view.tsx` 当前是"按 attempt 列表平铺"，不能充分表达 Activity 模型的运行时事实：

- `ActivityAttemptStatus` 4 个核心态（ready / claiming / running / terminal）在 UI 上没区分（claiming 直接 alias 成 running，"调度中"暂态不可见）
- `iteration_policy.max_attempts > 1` 时多 attempt 没有切换 UI
- function activity 没有 input/output JSON 渲染区
- human approval 的提交按钮硬编码 approved/rejected，未读取 form_schema 候选项
- artifact 面板是 latest+history 两栏对照，未表达"输入←transition←输出"的数据流

依赖 `05-21-lifecycle-step-fallback-cleanup` 先落地。

## 目标

把 Lifecycle Run Viewer 重塑成 Activity 模型原生的"调度+执行+产出"可观察界面。

## 范围

### 顶部 DAG 缩略图

- 用 lifecycle definition 渲染只读 DAG，active activity 高亮
- 节点点击 → 切换右侧详情聚焦
- 上方进度条改为按 attempts 状态加权（claiming/running/terminal 不同颜色段）

### Activity 卡片按 executor 分类渲染

- **Agent**：保持 SessionList，但 attempt 切换器在卡片头
- **Function**：新增 input/output JSON 渲染区（input artifacts → 实参，output artifacts → 返回），run_id 展示在头部
- **Human approval**：根据 `form_schema_key` 拉取 schema 渲染表单（MVP 可仍硬编码 approved/rejected，但 decision_port 显式可见，提交时写入 `decision_port + value`）

### Attempt 切换器

- iteration_policy.max_attempts > 1 时显示 `#1 #2 #3...`
- 默认显示 latest，可切回历史 attempt 查看其 input/output/session
- 不同 attempt 状态用不同 badge 表示

### Claiming 暂态可见

- 状态映射不再把 claiming 折叠为 running，独立 badge "调度中"

### Artifact 流面板

- 升级为按 transition 串联的"输入→产出"流图
- 节点点击同步高亮顶部 DAG 缩略图

## 非目标

- 后端 schema 变更
- form_schema 的 schema-store / 注册机制（MVP 仍按 form_schema_key 字符串）
- 编辑器重设计（独立任务）

## 验收标准

1. 单/多 Activity 运行视图在浏览器走通；3 种 executor kind 各自渲染正确
2. 4 个 attempt 状态都有视觉区分
3. iteration_policy.max_attempts > 1 时 attempt 切换可用
4. Human decision 流程完整（含 decision_port 可见、提交后状态推进）
5. artifact 流图能展示 input artifact 来源 transition
6. typecheck/lint/test 通过；spec 更新 `frontend/workflow-activity-lifecycle.md`

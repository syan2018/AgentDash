# Trellis 核心流程分析

**项目：** https://github.com/mindfold-ai/Trellis  
**分析日期：** 2026-02-21  
**分析重点：** Hook机制、多Agent流水线、Spec注入、验证循环

---

## 1. 初始化流程 (trellis init)

### 1.1 执行步骤

```
trellis init
    │
    ├─ 1. 显示Banner
    │
    ├─ 2. 检测开发者身份
    │   ├─ 优先：--user参数
    │   ├─ 其次：git config user.name
    │   └─ 交互：提示用户输入
    │
    ├─ 3. 检测项目类型 (frontend/backend/fullstack)
    │
    ├─ 4. 选择AI工具平台 (--cursor/--claude/--opencode/--iflow)
    │
    ├─ 5. 模板选择 (--template，可选)
    │
    ├─ 6. 创建工作流结构 (createWorkflowStructure)
    │
    ├─ 7. 配置各平台 (configurePlatform)
    │   └─ 复制agents/, commands/, hooks/到项目
    │
    ├─ 8. 创建根文件 (AGENTS.md)
    │
    ├─ 9. 初始化开发者 (init_developer.py)
    │   ├─ 创建.trellis/.developer (gitignored)
    │   ├─ 创建workspace/{name}/目录
    │   └─ 创建bootstrap任务
    │
    └─ 10. 初始化模板哈希 (用于更新检测)
```

### 1.2 创建的目录结构

```
.trellis/
├── .developer                     # 开发者身份 (gitignored)
├── .version                       # Trellis版本号
├── .template-hashes.json          # 模板文件哈希
├── .current-task                  # 当前任务指针
├── .ralph-state.json              # Ralph Loop状态
├── workflow.md                    # 开发流程文档
├── worktree.yaml                  # 多代理配置
├── scripts/                       # Python脚本工具
│   ├── __init__.py
│   ├── common/                    # 共享模块
│   │   ├── paths.py
│   │   ├── developer.py
│   │   ├── git_context.py
│   │   ├── cli_adapter.py
│   │   └── registry.py
│   ├── multi_agent/               # 多代理管道
│   │   ├── start.py
│   │   ├── status.py
│   │   ├── plan.py
│   │   ├── create_pr.py
│   │   └── cleanup.py
│   ├── task.py                    # 任务管理(核心)
│   ├── add_session.py
│   └── create_bootstrap.py
├── workspace/                     # 开发者工作区
│   ├── index.md                   # 工作区总索引
│   └── {developer}/
│       ├── index.md               # 个人索引
│       └── journal-{N}.md         # 日志文件
├── tasks/                         # 任务目录
│   └── {MM-DD-slug}/
│       ├── task.json
│       ├── prd.md
│       ├── info.md
│       ├── implement.jsonl
│       ├── check.jsonl
│       ├── debug.jsonl
│       └── codex-review-output.txt
└── spec/                          # 规范文档
    ├── shared/
    ├── frontend/
    ├── backend/
    └── guides/

.claude/                           # Claude Code配置
├── settings.json                  # Hook配置
├── agents/                        # Agent定义
│   ├── implement.md
│   ├── check.md
│   ├── debug.md
│   ├── research.md
│   ├── plan.md
│   └── dispatch.md
├── commands/trellis/              # 斜杠命令
└── hooks/                         # Hook脚本
    ├── session-start.py
    ├── inject-subagent-context.py
    └── ralph-loop.py

AGENTS.md                          # 项目级AI指令
```

---

## 2. 会话启动流程 (/trellis:start)

### 2.1 Hook注入机制

**settings.json 配置：**
```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup",
        "hooks": [{
          "type": "command",
          "command": "python3 .claude/hooks/session-start.py",
          "timeout": 10
        }]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Task",
        "hooks": [{
          "type": "command",
          "command": "python3 .claude/hooks/inject-subagent-context.py",
          "timeout": 30
        }]
      }
    ],
    "SubagentStop": [
      {
        "matcher": "check",
        "hooks": [{
          "type": "command",
          "command": "python3 .claude/hooks/ralph-loop.py",
          "timeout": 10
        }]
      }
    ]
  }
}
```

### 2.2 SessionStart Hook 工作流程

**输入：** 无（在Claude Code启动时触发）

**输出：** JSON格式
```json
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "<注入的上下文>"
  }
}
```

**注入内容结构：**
```
<session-context>
  You are starting a new session in a Trellis-managed project...
</session-context>

<current-state>
  Developer: {name}
  Current Task: {task}
  Git Status: branch, modified files
  Active Tasks: 列表
</current-state>

<workflow>
  cat .trellis/workflow.md
</workflow>

<guidelines>
  Frontend: cat .trellis/spec/frontend/index.md
  Backend: cat .trellis/spec/backend/index.md
  Guides: cat .trellis/spec/guides/index.md
</guidelines>

<instructions>
  cat .claude/commands/trellis/start.md
</instructions>
```

### 2.3 会话启动后的任务分类处理

```
用户输入
    │
    ▼
任务分类判断
    │
    ├─ Question (问答) ───────────────> 直接回答
    ├─ Trivial Fix (单字修改) ────────> 直接修改
    ├─ Simple Task (明确小任务) ──────> Quick Confirm
    └─ Complex Task (模糊大任务) ─────> Brainstorm
                                              │
                                              ▼
                                    Task Workflow (任务工作流)
                                              │
                        ┌─────────────────────┼─────────────────────┐
                        ▼                     ▼                     ▼
                   Research            Create Task Dir         Configure
                   (研究)               (创建任务目录)         Context
                        │                     │               (配置上下文)
                        └─────────────────────┼─────────────────────┘
                                                  │
                                                  ▼
                                          Write PRD (写需求)
                                                  │
                                                  ▼
                                          Activate Task (激活)
                                                  │
                        ┌─────────────────────┼─────────────────────┐
                        ▼                     ▼                     ▼
                  Implement              Check               Complete
                  Agent (编码)            Agent (验证)              │
                        │                     │                     │
                        └─────────────────────┴─────────────────────┘
                                                  │
                                                  ▼
                                      /trellis:record-session (记录)
```

---

## 3. 任务执行流程 (/trellis:parallel)

### 3.1 Multi-Agent Pipeline 架构

```
主仓库 (Main Repo)
│
├─ User <──> Orchestrator Agent (在main repo中运行)
│            │
│            ├─ Plan Agent (需求分析/任务拆分)
│            │
│            ├─ Research Agent (代码分析)
│            │
│            └─ Create Task Directory
│                │
│                ▼
│           .trellis/tasks/{MM-DD-task}/
│                │
│                ▼
│           start.py (创建worktree + agent)
│                │
│                ▼
│           Git Worktree (隔离环境)
│                │
│                ▼
│           ┌─────────────────────────────┐
│           │ ../trellis-worktrees/{branch}/
│           │     │
│           │     ▼
│           │  Dispatch Agent <──> Implement Agent <──> Check Agent
│           │     │                      │                    │
│           │     └──────────────────────┴────────────────────┘
│           │              (Hook自动注入上下文, Ralph Loop控制)
│           │                           │
│           │                           ▼
│           │  ┌──────────┐   ┌──────────┐   ┌──────────┐
│           │  │ Finish   │──>│ Create PR│──>│ Cleanup  │
│           │  │ (最终验证)│   │ (创建PR) │   │ (清理)   │
│           │  └──────────┘   └──────────┘   └──────────┘
│           └─────────────────────────────┘
```

### 3.2 启动Worktree Agent详细流程

```python
# 步骤1: 创建Worktree
if not worktree_path:
    # 1.1 计算worktree路径
    worktree_path = worktree_base / branch
    
    # 1.2 Git worktree add
    git worktree add [-b {branch}] {worktree_path} [{branch}]
    
    # 1.3 复制环境文件
    for file in worktree.yaml['copy']:
        cp {project}/{file} {worktree}/{file}
    
    # 1.4 复制任务目录
    cp -r {task_dir} {worktree}/{task_dir}
    
    # 1.5 执行post_create hooks
    for cmd in worktree.yaml['post_create']:
        run(cmd, cwd=worktree)

# 步骤2: 在worktree中设置当前任务
echo "{task_dir}" > {worktree}/.trellis/.current-task

# 步骤3: 启动Claude Code Agent
claude \
  --agent dispatch \
  --prompt "Follow your agent instructions..." \
  --session-id {uuid} \
  > {worktree}/.agent-log 2>&1 &

# 步骤4: 注册到Agent Registry
{
  "agents": [
    {
      "id": "{task-id}",
      "pid": 12345,
      "worktree": "/path/to/worktree",
      "task_dir": ".trellis/tasks/01-21-task",
      "platform": "claude",
      "session_id": "uuid",
      "status": "running"
    }
  ]
}
```

---

## 4. Spec注入机制

### 4.1 PreToolUse Hook工作流程

**触发条件：** Claude Code执行Task工具前

**输入：**
```json
{
  "tool_name": "Task",
  "tool_input": {
    "subagent_type": "implement|check|debug|research|plan",
    "prompt": "原始提示..."
  },
  "cwd": "/current/working/directory"
}
```

**Agent类型与上下文文件映射：**

| Agent Type | 读取文件 |
|-----------|---------|
| implement | implement.jsonl → prd.md → info.md |
| check | check.jsonl → prd.md |
| debug | debug.jsonl → codex-review-output.txt |
| research | research.jsonl + 项目结构 |
| finish | finish.jsonl → prd.md |

### 4.2 JSONL上下文文件格式

```jsonl
{"file": ".trellis/spec/backend/index.md", "reason": "Backend guide"}
{"file": ".trellis/spec/frontend/", "type": "directory", "reason": "Frontend specs"}
```

### 4.3 构建新Prompt

```
# {Agent} Agent Task

## Your Context

=== {file_path} ===
{file_content}

=== {file_path} ===
{file_content}

...

---

## Your Task

{original_prompt}

---

## Workflow / Constraints
(Agent特定的工作流程和约束)
```

### 4.4 默认上下文内容

**implement.jsonl（编码规范）：**
- .trellis/workflow.md
- .trellis/spec/shared/index.md
- .trellis/spec/backend/index.md (如后端)
- .trellis/spec/frontend/index.md (如前端)

**check.jsonl（验证规范）：**
- .claude/commands/trellis/finish-work.md
- .trellis/spec/shared/index.md
- .claude/commands/trellis/check-backend.md
- .claude/commands/trellis/check-frontend.md

---

## 5. 状态持久化

### 5.1 工作空间目录结构

```
.trellis/workspace/
├── index.md                    # 工作区总索引
└── {developer}/
    ├── index.md               # 个人索引
    │   ├─ @@@auto:developer-header (自动更新区域)
    │   ├─ 开发者信息
    │   ├─ 会话统计
    │   ├─ @@@auto:session-list (会话列表)
    │   ├─ 按月份分组的会话历史
    │   └─ 日志文件列表
    │
    └── journal-{N}.md         # 日志文件
        ├─ 会话1: {标题}
        ├─ 会话2: {标题}
        └─ ... (每文件最多2000行)
```

### 5.2 会话记录流程

```
用户执行: /trellis:record-session
              │
              ▼
    ┌─────────────────────┐
    │  收集会话信息        │
    │  - 标题 (--title)   │
    │  - 提交 (--commit)  │
    │  - 摘要 (--summary) │
    │  - 详细内容 (stdin) │
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │  确定日志文件        │
    │  1. 查找最新journal-N.md
    │  2. 如行数>2000, 创建N+1
    │  3. 返回(file_path, line_count)
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │  追加到日志文件      │
    │  echo "$content"    │
    │    >> journal-N.md │
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │  更新个人索引        │
    │  1. 解析@@@auto标记  │
    │  2. 更新会话统计     │
    │  3. 添加新会话       │
    └─────────────────────┘
               │
               ▼
         完成记录
```

---

## 6. 验证机制 (Ralph Loop)

### 6.1 Ralph Loop工作流程

**触发条件：** Check Agent尝试停止时

**验证方式（优先级）：**

**1. 程序验证 (worktree.yaml配置)：**
```yaml
verify:
  - pnpm lint
  - pnpm typecheck
```
- 全部通过 → 允许停止
- 任一失败 → 阻止停止 + 错误信息

**2. 标记验证 (Fallback)：**
- 读取 {task_dir}/check.jsonl
- 提取所有reason字段 → 生成标记列表
- 检查agent_output是否包含所有标记

### 6.2 状态管理

```json
{
  "task": ".trellis/tasks/01-21-feature",
  "iteration": 3,
  "started_at": "2026-01-21T10:30:00"
}
```

**安全限制：**
- MAX_ITERATIONS = 5 (最大迭代次数)
- STATE_TIMEOUT_MINUTES = 30 (状态超时)

### 6.3 Check Agent定义

**核心职责：**
1. 获取代码变更 (git diff)
2. 检查是否符合规范
3. 自修复问题
4. 运行验证 (typecheck和lint)

**完成标记：**
- 每个check.jsonl中的reason字段成为标记
- 示例：{"reason": "TypeCheck"} → "TYPECHECK_FINISH"
- 必须输出所有标记才能完成

---

## 7. 关键设计特点

1. **Hook驱动架构**：通过Claude Code的Hook机制自动注入上下文
2. **Multi-Agent Pipeline**：主仓库Orchestrator + Worktree执行Agent
3. **Spec驱动开发**：规范文件定义开发标准，自动注入到Agent
4. **验证循环**：Ralph Loop确保代码质量，支持程序验证和标记验证
5. **会话持久化**：完整记录会话历史，支持上下文恢复

---

## 8. 局限性分析

| 局限性 | 说明 |
|--------|------|
| **命令行门槛高** | 需要熟悉大量斜杠命令和配置 |
| **交互性较弱** | 主要面向工程师，产品/PM使用困难 |
| **Hook依赖严重** | 强依赖Claude Code的Hook机制 |
| **验证方式固定** | Ralph Loop虽有配置，但整体框架固定 |
| **单用户导向** | 虽有workspace隔离，但本质是个人工具 |
| **git依赖** | 所有工作基于git，无法处理非版本控制内容 |

---

## 9. 与AgentDashboard的对比

| 维度 | Trellis | AgentDashboard方向 |
|------|---------|-------------------|
| 使用门槛 | 工程师向 | 产品/PM友好 |
| 任务结构 | 扁平 | 树状父子 |
| 验证机制 | Ralph Loop固定 | 可插拔验证层 |
| 连接架构 | 单仓库 | 多对多后端 |
| 场景覆盖 | 代码开发 | 任意数字生产 |

---

*文档基于代码分析生成，反映Trellis的实际实现*

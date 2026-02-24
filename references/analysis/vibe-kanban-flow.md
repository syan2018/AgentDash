# Vibe-Kanban 核心流程分析

**项目：** https://github.com/BloopAI/vibe-kanban  
**分析日期：** 2026-02-21  
**分析重点：** 工作流程、状态流转、并行机制

---

## 1. 核心工作流程

### 1.1 任务创建流程

```
用户输入
    │
    ▼
选择仓库 ──> 选择目标分支 ──> 输入任务描述 ──> 选择AI Agent
                                                    │
                                                    ▼
                                        提交创建请求
                                                    │
                                                    ▼
                              ┌─────────────────────────────────┐
                              │ 后端处理                        │
                              │ 1. 创建工作区目录                │
                              │ 2. 创建Git Worktree             │
                              │ 3. 创建Session数据库记录         │
                              │ 4. 执行Setup Script（可选）      │
                              │ 5. 启动AI Agent                 │
                              └─────────────────────────────────┘
```

**关键数据结构：**
```typescript
CreateAndStartWorkspaceRequest {
  name: string | null,              // 工作区名称
  repos: Array<{
    repo_id: string,
    target_branch: string
  }>,
  executor_config: ExecutorConfig,  // Agent配置
  prompt: string                    // 用户提示
}
```

### 1.2 任务执行流程

**支持的Agent类型：**
- ClaudeCode, Amp, Gemini, Codex, Opencode, CursorAgent, QwenCode, Copilot, Droid

**执行步骤：**
1. 创建 ExecutionProcess 数据库记录
2. 捕获 before_head_commit（记录当前commit）
3. 启动 Agent 进程（spawn/spawn_follow_up）
4. 流式处理输出（stdout/stderr → MsgStore + 数据库）
5. 归一化日志（转换为结构化数据 NormalizedEntry）

---

## 2. 状态流转

### 2.1 ExecutionProcess 状态定义

```typescript
enum ExecutionProcessStatus {
  running = "running",
  completed = "completed",
  failed = "failed",
  killed = "killed"
}

type ExecutionProcessRunReason = 
  | "setupscript" 
  | "cleanupscript" 
  | "archivescript" 
  | "codingagent" 
  | "devserver";
```

### 2.2 状态流转图

```
创建 ExecutionProcess
       │
       ▼
[running] ← 开始执行
       │
  ┌────┴────┐
  ▼         ▼
completed  failed   ← 自然结束或错误
  │         │
           killed   ← 用户主动停止
```

### 2.3 执行链式控制

**结束条件判断：**
- DevServer 永不结束
- 并行 setup 没有 next_action 时不结束
- 失败或被杀时强制结束
- 没有下一个动作时结束

---

## 3. 并行机制

### 3.1 多Agent并行

**两种模式：**

**并行模式（parallel）：**
```rust
// 所有setup同时启动
for repo in repos_with_setup {
    start_execution(repo, SetupScript);
}
// 然后启动coding agent
start_execution(coding_action, CodingAgent);
```

**顺序模式（sequential）：**
```rust
// 通过next_action链式执行
let main_action = build_sequential_setup_chain(repos, coding_action);
start_execution(main_action, SetupScript);
```

### 3.2 Worktree管理

**关键机制：**
1. **全局锁**：每个worktree路径有独立异步锁，防止并发冲突
2. **自动清理**：孤儿worktree检测和清理
3. **重试逻辑**：创建失败时自动清理并重试

```rust
static WORKTREE_CREATION_LOCKS: 
    LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

pub async fn ensure_worktree_exists(
    repo_path: &Path, 
    branch_name: &str, 
    worktree_path: &Path
) {
    let lock = get_or_create_lock(worktree_path);
    let _guard = lock.lock().await;  // 获取路径级锁
    
    if is_worktree_properly_set_up(repo_path, worktree_path)? {
        return Ok(());
    }
    
    recreate_worktree_internal(repo_path, branch_name, worktree_path).await
}
```

---

## 4. 上下文管理

### 4.1 任务上下文传递

**执行前捕获：**
```rust
for repo in repositories {
    let before_head_commit = git().get_head_info(repo_path).ok().map(|h| h.oid);
    repo_states.push(CreateExecutionProcessRepoState {
        repo_id: repo.id,
        before_head_commit,
        after_head_commit: None,  // 执行后更新
        merge_commit: None,
    });
}
```

### 4.2 核心数据结构

- **Workspace**：任务工作区，包含分支名、容器引用
- **WorkspaceRepo**：工作区和仓库的多对多关系
- **ExecutionProcessRepoState**：记录执行前后的commit状态

---

## 5. 验证/审查机制

### 5.1 工具调用审批

**流程：**
1. Agent请求工具调用 → 创建 PendingApproval
2. 在 MsgStore 中标记状态为 PendingApproval
3. 等待用户响应（批准/拒绝/超时）

**状态流转：**
```
Created → PendingApproval → Approved/Denied/TimedOut
```

### 5.2 人工介入点

1. **工具调用审批**：敏感操作前需用户确认
2. **会话重置**：可重置到任意历史执行点
3. **合并确认**：代码合并前显示diff统计
4. **冲突解决**：rebase/merge冲突时暂停等待解决

---

## 6. 核心架构总结

```
┌─────────────────────────────────────────────┐
│  Frontend (React)                           │
│  CreateChatBox → useCreateWorkspace → API   │
└───────────────────┬─────────────────────────┘
                    ▼
┌─────────────────────────────────────────────┐
│  Server (Axum/Rust)                         │
│  task_attempts.rs → container.rs            │
└───────────────────┬─────────────────────────┘
                    ▼
┌─────────────────────────────────────────────┐
│  Services Layer                             │
│  WorkspaceManager → WorktreeManager         │
│  ContainerService → ExecutionProcess        │
└───────────────────┬─────────────────────────┘
                    ▼
┌─────────────────────────────────────────────┐
│  External Agents                            │
│  Claude Code / Codex / Gemini / ...         │
└─────────────────────────────────────────────┘
```

---

## 7. 关键设计特点

1. **Workspace-centric**：每个任务对应一个工作区，包含完整环境
2. **Git-native**：深度集成git worktree，每个任务独立分支
3. **Multi-repo**：原生支持跨仓库任务
4. **Streaming**：所有输出实时流式传输到前端
5. **Resumable**：Session机制支持长时间任务和断点续传

---

## 8. 局限性分析

| 局限性 | 说明 |
|--------|------|
| **绑定git仓库** | 所有任务必须关联git仓库，无法处理非代码场景 |
| **扁平任务结构** | 任务之间没有父子关系，无法表达复杂依赖 |
| **验证机制简单** | 主要依靠人工审批，缺乏可配置的验证规则 |
| **上下文传递有限** | 通过git commit传递状态，无法注入复杂设计信息 |
| **使用门槛较高** | 需要理解git worktree、分支管理等概念 |

---

*文档基于代码分析生成，反映vibe-kanban的实际实现*

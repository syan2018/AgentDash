# 模块：执行调度（Execution）

## 定位

管理Task的实际执行，负责与Agent的交互和生命周期管理。

## 职责

- 为Task分配Agent资源
- 启动、监控、停止Agent进程
- 管理Agent的执行环境（工作空间/隔离环境）
- 捕获Agent的输出和产物
- 向编排层报告执行结果

## 核心概念

### Agent（执行代理）
- 实际执行Task的实体
- 类型：Claude Code、Codex、Gemini、自定义Agent等
- 一对一绑定Task（一个Task同一时间只能由一个Agent执行）

### 执行环境（Execution Environment）
- Agent运行的隔离空间
- 包含必要的工作目录、文件、环境变量
- 支持多种隔离机制（worktree、container、vm等）

### Agent池（Agent Pool）
- 管理可用的Agent资源
- 根据Task需求匹配合适的Agent
- 支持Agent的复用和回收

## 生命周期

```
待分配（Pending）
    ↓
已分配（Assigned）- 选定Agent，准备环境
    ↓
执行中（Running）- Agent正在执行
    ↓
执行完成（Completed）- 正常结束
或
执行失败（Failed）- 异常结束
```

## 接口定义（概念层面）

```
ExecutionManager {
  assignAgent(taskId, agentType): Agent
  startExecution(taskId, agentId): void
  stopExecution(taskId): void
  getExecutionStatus(taskId): ExecutionStatus
  getExecutionOutput(taskId): Output
}

Agent {
  id: string
  type: string
  status: "idle" | "busy" | "error"
  environment: Environment
}

Environment {
  workDir: string
  isolated: boolean
  resources: ResourceConfig
}
```

## 关键设计决策（待讨论）

- [ ] Agent类型的抽象接口
- [ ] 环境隔离的实现方式
- [ ] Agent资源的分配策略
- [ ] 执行超时和熔断机制

## 暂不定义

- 具体Agent的集成细节
- 容器/VM技术选型
- 资源限制和配额管理
- 执行日志的存储策略

---

*状态：概念定义阶段*  
*更新：2026-02-21*

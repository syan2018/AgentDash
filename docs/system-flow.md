# AgentDashboard - 系统流程图

基于 vibe-kanban、Trellis 和胡渊鸣实践的分析，绘制的核心系统流程。

---

## 1. 整体架构流程

```mermaid
flowchart TB
    subgraph UserLayer["用户层"]
        User["用户"]
        Client["前端Client"]
    end
    
    subgraph ConnectionLayer["连接层"]
        LocalBackend["本地后端"]
        RemoteBackend["远程后端"]
    end
    
    subgraph StateLayer["状态层"]
        WorkspaceManager["工作空间管理器"]
        StateContainer["状态容器"]
    end
    
    subgraph OrchestrationLayer["编排层"]
        StrategyEngine["策略引擎"]
        Injector["信息注入器"]
        Validator["验证器"]
    end
    
    subgraph ExecutionLayer["执行层"]
        AgentPool["Agent池"]
        Worktree["隔离环境"]
    end
    
    User --> Client
    Client --> LocalBackend
    Client --> RemoteBackend
    
    LocalBackend --> WorkspaceManager
    RemoteBackend --> WorkspaceManager
    
    WorkspaceManager --> StateContainer
    StateContainer --> StrategyEngine
    StrategyEngine --> Injector
    StrategyEngine --> Validator
    
    StrategyEngine --> AgentPool
    AgentPool --> Worktree
    
    Validator -.->|反馈| StrategyEngine
    Worktree -.->|状态更新| StateContainer
```

---

## 2. 工作项生命周期流程

### 2.1 故事（Story）生命周期

```mermaid
stateDiagram-v2
    [*] --> Created: 创建故事
    
    Created --> ContextInjected: 注入设计信息
    
    ContextInjected --> Decomposed: 拆解为任务
    
    Decomposed --> Orchestrating: 编排执行中
    
    Orchestrating --> AllCompleted: 所有任务完成
    
    AllCompleted --> Validating: 聚合验证
    
    Validating --> Orchestrating: 需要补充任务
    Validating --> Completed: 故事完成
    Validating --> Failed: 无法完成
    
    Completed --> [*]
    Failed --> [*]
```

**故事职责：**
- 从用户角度描述需求
- 维护完整设计上下文
- 拆解和编排任务
- 聚合任务结果并验收
- 不直接执行，不绑定Agent

### 2.2 任务（Task）生命周期

```mermaid
stateDiagram-v2
    [*] --> Pending: 创建（待分配）
    
    Pending --> Assigned: 分配Agent
    
    Assigned --> Running: Agent开始执行
    
    Running --> Validating: 执行完成
    
    Validating --> Running: 验证失败（重新执行）
    Validating --> Completed: 验证通过
    Validating --> Failed: 无法修复
    
    Completed --> [*]
    Failed --> [*]
```

**任务职责：**
- 一对一绑定Agent进程
- 在隔离环境中执行
- 捕获执行状态和产物
- 向所属故事报告结果

---

## 3. 多对多连接模型

```mermaid
flowchart LR
    subgraph Users["用户群"]
        U1["用户A"]
        U2["用户B"]
        U3["用户C"]
    end
    
    subgraph Clients["客户端"]
        C1["Client实例1"]
        C2["Client实例2"]
        C3["Client实例3"]
    end
    
    subgraph Backends["后端群"]
        B1["本地后端"]
        B2["远程后端-P4项目"]
        B3["远程后端-项目2"]
    end
    
    subgraph Workspaces["工作空间"]
        W1["工作空间A"]
        W2["工作空间B"]
        W3["工作空间C"]
    end
    
    U1 --> C1
    U1 --> C2
    U2 --> C2
    U3 --> C3
    
    C1 --> B1
    C1 --> B2
    C2 --> B2
    C3 --> B3
    
    B1 --> W1
    B2 --> W2
    B3 --> W3
```

---

## 4. 状态迁移控制流程

```mermaid
sequenceDiagram
    participant User as 用户/策略
    participant SC as 状态容器
    participant ST as 迁移控制器
    participant VL as 验证层
    participant AG as Agent执行
    
    User->>SC: 请求状态变更
    SC->>ST: 检查迁移规则
    
    alt 满足前置条件
        ST->>AG: 执行迁移动作
        AG-->>ST: 执行结果
        
        ST->>VL: 验证后置状态
        VL-->>ST: 验证结果
        
        alt 验证通过
            ST->>SC: 更新状态
            SC-->>User: 迁移成功
        else 验证失败
            ST->>SC: 回滚/标记失败
            SC-->>User: 迁移失败
        end
    else 不满足条件
        ST-->>User: 拒绝迁移
    end
```

---

## 5. 标准化信息注入流程

```mermaid
flowchart TD
    subgraph Sources["信息源"]
        DesignDoc["设计文档"]
        SpecFile["规范文件"]
        History["历史记录"]
        Context["项目上下文"]
    end
    
    subgraph Injection["注入机制"]
        Parser["解析器"]
        Merger["合并器"]
        Injector["注入器"]
    end
    
    subgraph Target["目标"]
        Task["任务"]
        Agent["Agent"]
        Workspace["工作空间"]
    end
    
    DesignDoc --> Parser
    SpecFile --> Parser
    History --> Parser
    Context --> Parser
    
    Parser --> Merger
    Merger --> Injector
    
    Injector --> Task
    Injector --> Agent
    Injector --> Workspace
```

---

## 6. 与参考项目的对比映射

```mermaid
flowchart TB
    subgraph Reference["参考项目特性"]
        VK1["vibe-kanban:<br/>Workspace-centric"]
        VK2["vibe-kanban:<br/>Git worktree隔离"]
        VK3["vibe-kanban:<br/>流式输出"]
        
        TR1["Trellis:<br/>Hook注入机制"]
        TR2["Trellis:<br/>Multi-Agent Pipeline"]
        TR3["Trellis:<br/>Ralph Loop验证"]
        
        HY1["胡渊鸣:<br/>任务队列循环"]
        HY2["胡渊鸣:<br/>CLAUDE.md/PROGRESS.md"]
        HY3["胡渊鸣:<br/>闭环反馈"]
    end
    
    subgraph Craft["AgentDashboard抽象"]
        C1["工作空间管理"]
        C2["状态容器"]
        C3["实时状态流"]
        C4["信息注入器"]
        C5["编排层策略"]
        C6["验证层"]
        C7["任务队列"]
        C8["上下文管理"]
        C9["反馈机制"]
    end
    
    VK1 --> C1
    VK2 --> C2
    VK3 --> C3
    
    TR1 --> C4
    TR2 --> C5
    TR3 --> C6
    
    HY1 --> C7
    HY2 --> C8
    HY3 --> C9
```

---

## 7. 典型使用场景流程（游戏项目故事产出）

```mermaid
sequenceDiagram
    participant PM as 产品经理
    participant Client as 客户端
    participant Backend as 后端服务
    participant Workspace as 工作空间
    participant Agents as Agent组
    
    PM->>Client: 创建故事（Story）：设计新关卡
    Client->>Backend: 请求创建故事
    Backend->>Workspace: 初始化故事状态容器（注入设计文档、关卡规范、美术资源）
    
    PM->>Client: 拆解为任务（Task）
    Client->>Backend: 批量创建任务（绑定Agent）
    Backend->>Workspace: 建立Story与Task的包含关系
    
    loop 每个任务（Task）
        Backend->>Agents: 派发任务到Agent
        Agents->>Workspace: 任务执行状态变更
        
        alt 任务需要验证
            Workspace->>Backend: 触发验证流程
            Backend->>Agents: 验证Agent审查任务结果
            Agents-->>Backend: 验证结果
        end
        
        Workspace-->>Backend: 任务状态更新
        Backend-->>Client: 推送任务状态变化
        Client-->>PM: 显示任务进度
    end
    
    PM->>Client: 验收故事（Story）
    Client->>Backend: 标记故事完成
    Backend->>Workspace: 归档故事状态
```

---

## 关键洞察总结

1. **状态容器** 是核心抽象，超越vibe-kanban的Workspace和Trellis的Task目录
2. **Story-Task双层模型** Story负责编排和验收，Task负责执行，职责清晰分离
3. **Story组织用户自定义** Story间关系是视图层概念，用户可按需编组，不影响执行
4. **验证层可插拔** 比Trellis的Ralph Loop更灵活，支持多种形式
5. **连接层透明化** 实现真正的多对多架构
6. **注入机制通用化** 不限于代码场景，支持任意数字生产

---

*版本：v0.1*  
*更新：2026-02-21 - 基于参考项目分析绘制*

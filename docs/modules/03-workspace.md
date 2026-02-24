# 模块：工作空间管理（Workspace）

## 核心思想

隔离是**策略选择**，不是固定实现。

我们不预设"必须用容器"或"必须用worktree"，因为：
- 代码项目（MB级）vs 游戏项目（GB级）需要完全不同的隔离方式
- 信任Agent vs 不受信Agent需要不同安全级别
- 性能 vs 安全需要权衡

系统只保证：**Task执行需要隔离**，但"如何隔离"可自由选择。

## 定位

管理Agent执行任务所需的隔离环境，处理环境创建、权限控制和资源隔离。

## 职责

- 为Task创建隔离的执行环境（worktree、container、vm等）
- 管理环境的资源配置（文件、网络、权限）
- 处理Agent在环境中的权限（文件访问、命令执行等）
- 环境的生命周期管理（创建、暂停、销毁）
- 支持多种隔离策略（按项目需求选择）

## 核心概念

### 工作空间（Workspace）
- Task执行的物理/逻辑隔离空间
- 包含工作目录、配置、缓存等
- 独立于其他Task的执行环境

### 隔离策略（Isolation Strategy）
- **Git Worktree**：轻量级，适合代码项目
- **文件快照**：适合大文件项目（如游戏资源）
- **容器**：强隔离，适合不受信Agent
- **VM**：最强隔离，但性能开销大

### 环境模板（Environment Template）
- 预定义的环境配置
- 包含基础工具、依赖、初始化脚本
- 可复用，加速环境创建

### Agent权限（Agent Permission）
- 定义Agent在环境中的操作权限
- 文件读写范围
- 可执行的命令类型
- 网络访问权限

## 隔离策略对比

| 策略 | 隔离强度 | 性能 | 适用场景 | 资源占用 |
|------|---------|------|----------|----------|
| Git Worktree | 低 | 极高 | 代码项目 | 极低 |
| 文件快照 | 中 | 高 | 大文件项目 | 中 |
| 容器 | 高 | 中 | 不受信Agent | 中 |
| VM | 最高 | 低 | 强安全要求 | 高 |

## 权限模型

```
Agent权限层级：

Level 0 - 只读（Read-Only）
- 只能读取指定文件
- 不能修改任何内容
- 适合审查类Agent

Level 1 - 受限写（Restricted Write）
- 可在指定目录写文件
- 禁止执行危险命令
- 适合内容生成类Agent

Level 2 - 标准（Standard）
- 完整工作目录访问
- 可执行常规命令
- 禁止系统级操作
- 适合一般开发Agent

Level 3 - 特权（Privileged）
- 几乎完全访问权限
- 需谨慎使用
- 适合初始化/清理Agent
```

## 环境生命周期

```
待创建（Pending）
    ↓
准备中（Preparing）- 分配资源、复制模板
    ↓
就绪（Ready）- 环境可用
    ↓
使用中（Active）- 绑定Task和Agent
    ↓
暂停（Paused）- 临时释放资源（可选）
    ↓
销毁中（Destroying）- 清理资源
    ↓
已销毁（Destroyed）
```

## 接口定义（概念层面）

```
WorkspaceManager {
  createWorkspace(config): Workspace
  prepareWorkspace(workspaceId): void
  bindTask(workspaceId, taskId): void
  unbindTask(workspaceId, taskId): void
  setAgentPermission(workspaceId, agentId, level): void
  destroyWorkspace(workspaceId): void
  getWorkspaceStatus(workspaceId): WorkspaceStatus
}

Workspace {
  id: string
  type: "worktree" | "snapshot" | "container" | "vm"
  path: string
  template: string | null
  resources: ResourceConfig
  permissions: PermissionConfig
  status: WorkspaceStatus
}

IsolationConfig {
  strategy: "worktree" | "snapshot" | "container" | "vm"
  baseDir: string
  copyFiles: string[]
  postCreateHook: string[]
  resourceLimits: ResourceLimits
}
```

## 关键设计决策（待讨论）

- [ ] 默认隔离策略的选择逻辑
- [ ] 大文件项目的快照机制（上百GB的游戏项目）
- [ ] 环境模板的定义和管理方式
- [ ] 权限模型的细化（是否需要更细粒度？）
- [ ] 多Task共享环境的安全边界
- [ ] 环境预热和缓存策略

## 与参考项目的对比

| 特性 | vibe-kanban | Trellis | AgentDashboard方向 |
|------|-------------|---------|-------------------|
| 隔离方式 | Git Worktree | Git Worktree | 多策略可选 |
| 权限控制 | 无（完全信任） | 无（--dangerously-skip-permissions） | 分级权限 |
| 环境模板 | 无 | worktree.yaml配置 | 可复用模板 |
| 适用场景 | 代码开发 | 代码开发 | 任意数字生产 |

## 暂不定义

- 具体容器技术选型（Docker/Podman/其他）
- VM虚拟化方案
- 文件快照的实现细节
- 跨平台兼容性处理
- 资源监控和限制的具体实现

---

*状态：概念定义阶段*  
*更新：2026-02-21*

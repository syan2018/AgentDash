# Directory Structure

> How backend code is organized in this project.

---

## Overview

<!--
Document your project's backend directory structure here.

Questions to answer:
- How are modules/packages organized?
- Where does business logic live?
- Where are API endpoints defined?
- How are utilities and helpers organized?
-->

<!-- PROJECT-SPECIFIC-START: AgentDashboard Backend Structure -->
> **AgentDashboard 后端代码的组织方式。**
> **注意：当前为概念阶段，技术栈未定，目录结构仅为参考设计。**

### 设计原则

按照项目的**策略可插拔**原则，目录结构应体现模块边界：
- 每个模块独立目录，模块间通过接口交互
- 接口定义与实现分离
- 策略（Strategy）作为可替换组件
<!-- PROJECT-SPECIFIC-END -->

---

## Directory Layout

```
<!-- Replace with your actual structure -->
src/
├── ...
└── ...
```

<!-- PROJECT-SPECIFIC-START: Directory Tree -->
### 建议目录布局（参考设计）

```
backend/
├── modules/               # 核心模块（对应 docs/modules/）
│   ├── state/             # 模块02：状态管理
│   │   ├── interfaces/    # 接口定义（StateManager等）
│   │   ├── entities/      # 数据实体（Story, Task, StateChange）
│   │   └── strategies/    # 存储策略（文件/数据库等）
│   ├── connection/        # 模块01：连接管理
│   │   ├── interfaces/    # ConnectionManager接口
│   │   └── strategies/    # 连接策略
│   ├── workspace/         # 模块03：工作空间管理
│   │   ├── interfaces/    # WorkspaceManager接口
│   │   └── strategies/    # 隔离策略（worktree/container/vm）
│   ├── execution/         # 模块05：执行调度
│   │   ├── interfaces/    # ExecutionManager接口
│   │   └── agents/        # Agent适配器（Claude/Codex/Gemini）
│   ├── orchestration/     # 模块04：编排引擎
│   │   ├── interfaces/    # OrchestrationStrategy接口
│   │   └── strategies/    # 编排策略（规则/AgentPM/手动/混合）
│   ├── injection/         # 模块06：信息注入
│   │   ├── interfaces/    # Injector接口
│   │   └── sources/       # 注入源处理器
│   └── validation/        # 模块07：验证机制
│       ├── interfaces/    # Validator接口
│       └── rules/         # 验证规则实现
├── api/                   # API层（对外接口）
│   ├── routes/            # 路由定义
│   └── handlers/          # 请求处理器
├── shared/                # 共享工具
│   ├── types/             # 共享类型定义
│   ├── errors/            # 错误类型
│   └── utils/             # 工具函数
└── config/                # 配置
    └── index              # 应用配置
```
<!-- PROJECT-SPECIFIC-END -->

---

## Module Organization

<!-- How should new features/modules be organized? -->

<!-- PROJECT-SPECIFIC-START: Module Guidelines -->
### 每个模块的标准结构

```
modules/<module-name>/
├── interfaces/         # 接口/类型定义（稳定，不轻易改变）
│   └── index          # 导出接口
├── strategies/         # 可替换策略实现
│   ├── <strategy-a>/
│   └── <strategy-b>/
└── index               # 模块入口，注册策略
```

### 模块依赖方向

```
api → orchestration → state
                   ↓       ↓
              injection  execution → workspace
                              ↓
                          validation
```

> **禁止跨层依赖：** api层不能直接访问 state 层内部实现
<!-- PROJECT-SPECIFIC-END -->

---

## Naming Conventions

<!-- File and folder naming rules -->

<!-- PROJECT-SPECIFIC-START: Naming Rules -->
> **注意：技术栈确定后，根据所选语言的约定调整命名规范。**

- **模块目录**：小写短横线（kebab-case），如 `state-manager/`
- **接口文件**：描述性名称，如 `StateManager`, `ConnectionManager`
- **策略实现**：`<技术>-<功能>`，如 `sqlite-state-store`, `worktree-workspace`
- **实体类型**：PascalCase，如 `Story`, `Task`, `StateChange`
<!-- PROJECT-SPECIFIC-END -->

---

## Examples

<!-- Link to well-organized modules as examples -->

<!-- PROJECT-SPECIFIC-START: Current Status -->
### 当前状态

> 技术栈未确定，上述为概念性目录设计。
> 确定技术栈后，在此文件更新实际目录结构。

**需要讨论决定：**
- [ ] 后端语言选择（Node.js / Python / Go / Rust / ...）
- [ ] 框架选择
- [ ] 存储方案（影响 state 模块目录结构）
- [ ] 构建工具和项目结构约定
<!-- PROJECT-SPECIFIC-END -->

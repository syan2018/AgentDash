# 兼容层与回退逻辑清理

## Goal

围绕本次批量 review 暴露出的历史兼容层、回退逻辑、补丁式实现，组织一轮跨层清理。
目标不是“继续包一层兼容”，而是把项目收敛到单一正确路径，减少静默兜底、协议双轨、
运行时迁移和开发期清场脚本带来的长期复杂度。

## Background

当前项目处于预研阶段，尚未上线。
根据项目约束：

- 不需要保留旧 API / 旧 schema / 旧字段兼容
- 不需要为历史数据或外部用户做迁移友好性设计
- 应优先追求当前代码和协议的最正确状态

本次 review 发现的主要问题包括：

- 持久化层通过默认值吞掉坏数据和解析失败
- address space / inline persistence 在 owner 或 workspace 不明确时继续猜测
- Hook / 执行器通过 fallback 文本或伪造 steering message 绕开状态机和结构化输入问题
- 前后端同时维护旧路由、旧 payload、旧状态映射
- workflow contract / schema 初始化仍保留运行时 legacy 迁移
- dev 启动链路依赖杀进程、杀端口、清理残留实例来维持可运行

## Scope

### 重点清理范围

- 后端 repository 中的静默默认值、`unwrap_or_default`、未知枚举回退
- address space / session context / inline persistence 中的跨 scope 猜测性回退
- PiAgent / 执行器中的 provider / model / wire API 猜测与默认 bridge 回退
- structured prompt 到 text fallback 的双轨路径
- 前端旧 API 主路径、旧状态映射、协议兼容补丁
- workflow / schema 的 runtime legacy 迁移
- dev scripts 与 embedded PostgreSQL 的补丁式生命周期管理

### 不在本任务内

- 为上线兼容性准备迁移方案
- 保留旧版本协议的长期支持
- 为历史数据读写兼容补充更多兜底逻辑

## Cleanup Principles

- 解析失败应暴露错误，不应伪装成默认业务值
- owner / workspace / provider / model 必须显式解析成功，不能靠“选第一个”或“退默认”
- 结构化输入必须保持结构化，不再长期维护 text shadow path
- 协议与 schema 只能保留单一路径，legacy 迁移改为一次性数据/代码清理
- 开发期生命周期管理应显式建模，不再依赖“先杀再起”

## Workstreams

### 1. 数据与持久化收口

- 清点 repository 中所有静默默认值和坏数据吞没点
- 将“默认值兜底”改为显式错误或显式空值
- 删除预研阶段不再需要的运行时补列 / legacy schema 兼容

### 2. Address Space 与上下文归属收口

- 移除 inline persistence 从 story 回退到 project 的写入/删除逻辑
- 移除 workspace 解析中的“第一个 workspace”兜底
- 区分“确实没有上下文”和“上下文构建失败”

### 3. 执行器与 Prompt 收口

- 收敛 provider / model / wire API 解析路径
- 删除默认 bridge / 伪造 discovery provider/model
- 推进 structured prompt 成为唯一真相，逐步移除 text fallback 主路径
- 修正 Hook Runtime 对空 Continue 的补丁式 steering 注入

### 4. 前端协议与状态收口

- 切换到新 project-agent / session API 主路径
- 删除旧 task status / execution mode 映射
- 删除裸 `SessionNotification`、NDJSON -> SSE 等过渡兼容逻辑

### 5. 开发基础设施收口

- 评估并移除按端口/进程名暴力清场的启动逻辑
- 收敛 embedded PostgreSQL / external PostgreSQL 的解析和生命周期策略
- 统一 ready check / retry 逻辑，避免多份脚本重复维护

## Acceptance Criteria

- [ ] 后端不再通过默认值吞掉持久化层坏数据、坏枚举或坏 JSON
- [ ] address space / inline persistence / workspace 解析不再跨 scope 猜测目标对象
- [ ] PiAgent / 执行器不再对未知 provider/model 静默回退到默认 bridge
- [ ] structured prompt 不再依赖长期保留的 text fallback 主路径
- [ ] 前端主路径不再调用旧 project-agent / session 兼容 API
- [ ] 前端不再维护旧状态映射与协议补丁式兼容
- [ ] workflow / schema 不再依赖 runtime legacy migration
- [ ] dev 启动链路不再依赖无差别杀进程/杀端口清场

## Suggested Execution Order

1. 先清“会掩盖真实错误”的静默兜底
2. 再清协议和 schema 的双路径兼容
3. 最后收执行器输入模型与 dev 生命周期管理

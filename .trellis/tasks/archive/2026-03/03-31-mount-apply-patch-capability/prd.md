# 统一 Mount Patch 能力方案

## 背景

当前项目已经具备一条可用的 `fs_apply_patch` 链路：

- `relay_fs` 可通过本机 relay 执行 patch
- `inline_fs` 已通过共享 patch 引擎 + overlay 支持 patch
- Agent 运行时工具、HTTP API、relay 协议、SPI 契约已经打通

但从架构上看，这一版仍然是“共享算法 + 特定 provider 接入”的折中态，还没有完全演进到“基于 provider primitive/capability 的组合式 apply_patch”。

现在的主要问题是：

1. `apply_patch` 的可用性仍然主要挂在 provider 显式实现上，而不是由共享层根据底层 primitive 自动组合。
2. mount capability 仍然比较粗，只能表达 `read/write/list/search/exec`，无法精确表达 patch 所需的 `create/delete/rename` 能力。
3. 上层目前只能大致认为“有 write 就可能能 patch”，但无法按 patch 实际内容做精确判定。
4. 后续新增 provider 时，仍然容易复制一遍 patch 支持逻辑，而不是天然复用共享引擎。

## 目标

把 `apply_patch` 演进为统一 Address Space 里的默认编辑能力：

1. patch 解析与匹配算法只有一份共享实现。
2. provider 主要暴露基础文件编辑 primitive，而不是重复实现 patch 引擎。
3. patch 是否可用由“patch 实际需要的操作集合”和“mount/provider 暴露的精细能力”共同决定。
4. 只有在 provider 存在更高效、更原子或远端原生 patch 通道时，才覆盖默认实现。

## 非目标

1. 不做兼容性保底方案。
2. 不为旧接口保留双轨逻辑。
3. 不考虑数据库字段历史兼容。
4. 不把 patch 语法改成 unified diff；继续使用 Codex 风格 apply_patch 语法。

## 当前状态

### 已完成

1. `MountProvider::apply_patch` 已加入 SPI，默认返回 `NotSupported`。
2. `agentdash-application::address_space::apply_patch` 已承载共享 patch 引擎。
3. `relay_fs` 与 `inline_fs` 都已接入共享 patch 能力。
4. `fs_apply_patch` 工具、API route、relay 协议、Pi Agent 提示词、文档都已补齐基本支持。

### 仍待演进

1. provider primitive 还不够完整。
2. capability 语义不够细。
3. patch 执行前缺少“按 patch 内容推导所需能力”的统一判定。
4. 默认组合式实现还没有完全成为 provider 的主路径。

## 目标架构

```text
fs_apply_patch / HTTP apply-patch / 其它上层入口
                ↓
        Shared Apply Patch Engine
                ↓
      Parse Patch + Analyze Required Ops
                ↓
      Capability Check + Primitive Planner
                ↓
  MountProvider Primitive Layer / Optional Native Override
                ↓
      relay_fs / inline_fs / future external providers
```

## Primitive 与能力模型

### 建议新增或显式化的 primitive

1. `read_text(path)`
2. `write_text(path, content)`
3. `delete(path)`
4. `rename(from, to)`
5. 可选 `stat(path)`
6. 可选 `ensure_parent_dirs(path)`

### 建议能力模型

当前粗能力：

- `read`
- `write`
- `list`
- `search`
- `exec`

目标细能力：

- `read`
- `write_existing`
- `create`
- `delete`
- `rename`
- `list`
- `search`
- `exec`

如果暂时不想一次性改大 capability 枚举，也可以先在 provider 元信息层引入“编辑 primitive 能力声明”，再逐步提升到 mount capability 正式模型。

## Patch 需求推导规则

共享层在 parse patch 后，先分析本次 patch 需要的 primitive：

1. 仅含 `*** Update File`
   - 需要：`read_text + write_text`
2. 含 `*** Add File`
   - 需要：`create` 或“允许对不存在路径执行 write_text(create)”
3. 含 `*** Delete File`
   - 需要：`delete`
4. 含 `*** Move to`
   - 优先需要：`rename`
   - 可退化为：`read_text + write_text + delete`

然后统一做 capability check：

1. 能力完整：直接执行
2. 缺失可退化 primitive：走 fallback plan
3. 缺失不可退化 primitive：拒绝 patch，并返回明确错误

## 分阶段落地

### Phase 1：收束当前 patch 支撑为正式基线

目标：

1. 保持现有 `relay_fs` / `inline_fs` patch 能力稳定。
2. 补全测试与文档，确保现状是可维护基线。

交付：

1. 共享 patch 引擎稳定 API
2. inline / relay 测试覆盖
3. 工具说明、文档、system prompt 一致

### Phase 2：补齐 provider primitive

目标：

1. 在 SPI / application 层补出 `delete` / `rename` 等编辑 primitive。
2. 明确各 provider 的 primitive 支持声明。

交付：

1. `MountProvider` primitive 扩展
2. `relay_fs` primitive 实现
3. `inline_fs` primitive 实现
4. 错误语义与 capability matrix 文档

### Phase 3：默认组合式 apply_patch

目标：

1. 让共享层成为 `apply_patch` 的默认实现主路径。
2. provider 只有在需要原生优化时才覆盖。

交付：

1. patch -> required ops analyzer
2. primitive planner / fallback plan
3. 默认 `apply_patch` 组合执行器

### Phase 4：能力精细化与开放扩展

目标：

1. 让 patch 可用性由精细能力控制，而不是粗粒度 `write`。
2. 为未来外部 mount provider 提供清晰接入模型。

交付：

1. capability 模型升级
2. provider 接入指南
3. patch capability contract

## 子任务拆分

1. 盘点 `apply_patch` 当前调用面与 provider 实现边界。
2. 设计 primitive/capability 模型，并决定落在 SPI 还是 application 元信息层。
3. 为 `relay_fs` 与 `inline_fs` 补齐 `delete/rename` primitive。
4. 实现 patch 所需操作分析器。
5. 实现共享层默认组合式 `apply_patch`。
6. 补齐 API / runtime tool / prompt / 文档同步。
7. 增加多 provider / fallback / capability denied 测试。

## 验收标准

1. 新增一个可写 provider 时，在不重写 patch 算法的前提下，可以通过声明 primitive 获得 `apply_patch` 支持。
2. patch 执行前能明确报出缺失的是哪类能力，而不是笼统报“write 不支持”。
3. `relay_fs` 与 `inline_fs` 继续通过相同共享引擎工作。
4. `fs_apply_patch` 的说明、system prompt、后端规范文档三处描述保持一致。
5. 至少覆盖：
   - update-only
   - add
   - delete
   - move
   - move fallback
   - capability denied
   - path escape rejected

## 风险与注意点

1. `rename` 能力是否真实原子，可能因 provider 不同而不同，错误语义要区分“原生 rename”与“fallback rename”。
2. `write_text` 是否允许“写不存在文件即创建”，必须显式定义，不能靠隐式约定。
3. `inline_fs` 目前通过 overlay + persister 工作，primitive 设计时不能绕开持久化层。
4. `relay_fs` 若未来支持原生远端 patch，应允许 provider 覆盖默认组合实现。

## 相关文件

1. `.trellis/spec/backend/address-space-access.md`
2. `crates/agentdash-spi/src/mount.rs`
3. `crates/agentdash-application/src/address_space/apply_patch.rs`
4. `crates/agentdash-application/src/address_space/relay_service.rs`
5. `crates/agentdash-application/src/address_space/inline_persistence.rs`
6. `crates/agentdash-application/src/address_space/tools/fs.rs`
7. `crates/agentdash-api/src/routes/address_spaces.rs`
8. `crates/agentdash-executor/src/connectors/pi_agent.rs`

# 清除 workspace_root 冗余概念

> 状态：planning
> 优先级：P2（技术债，不阻塞功能但持续增加维护成本）
> 规模：M-L

## 问题

`workspace_root: PathBuf` 是 address space 体系出现前的遗留概念。当前系统同时维护两套路径体系：
- `workspace_root`：裸 PathBuf，直接指向本机物理路径
- `address_space.mounts[].root_ref`：统一寻址模型，mount + 相对路径

两者信息重复，新功能（如 skill 扫描）必须二选一，导致实现混乱。

## 当前使用分布（245 处 / 40 文件）

| 类别 | 占比 | 核心文件 | 替换难度 |
|------|------|---------|---------|
| **Relay 协议 + 本机安全边界** | ~65 处 | tool_executor.rs(49), protocol.rs(12), command_handler, apply_patch | 高：安全模型重设计 |
| **可直接换成 mount root_ref** | ~16 处 | connector.rs, bootstrap.rs, resolver.rs, prompt_pipeline | 低：纯路径替换 |
| **进程级基础设施状态** | ~5 处 | SessionHub, hook_runtime | 中：需明确 hub 获取 root 的方式 |
| **address_space 为 None 时兜底** | 1 处 | injection/resolver.rs | 低：已 Optional |

## 迁移方案

### Phase 1：云端层（低风险，先做）

`ExecutionContext`、`connector.rs`、`bootstrap.rs`、`resolver.rs`、`prompt_pipeline.rs` 中的 workspace_root → 从 `address_space.default_mount().root_ref` 获取。

`SessionHub.workspace_root` → 改为按需从 address space 或 workspace binding 解析。

### Phase 2：Relay 协议（中风险）

`protocol.rs` 的 `CommandPromptPayload.workspace_root` 等字段 → 重命名为 `mount_root_ref` 或从协议消息中移除（relay backend 已经知道自己的 mount 路径）。

需要 relay 协议版本号管理，确保向后兼容。

### Phase 3：本机安全边界（高风险）

`tool_executor.rs` 的 49 处 workspace_root → 重新设计为基于 mount capabilities + path normalization 的安全围栏。

当前安全模型：
```
validate_workspace_root(path, accessible_roots)
  → 确保所有文件操作不逃逸 workspace_root
```

目标安全模型：
```
validate_mount_access(mount_id, path, capabilities)
  → 确保操作在 mount 授权范围内（Read/Write/Exec 分别控制）
```

这实际上是更强的安全模型（per-mount 细粒度权限 vs 单一根目录平铺），但改动面大。

### Phase 4：清理

- 从 `ExecutionContext` 移除 `workspace_root` 字段
- 从 `PromptSessionRequest` 移除 `workspace_root` 字段
- 从 `SessionHub` 移除 `workspace_root` 字段
- 确保所有路径都走 `address_space` 唯一通道

## 前置条件

- `agentdash-local` 的 `PromptSessionRequest` 构造处（目前 `address_space: None`）需要补齐 address space 构建逻辑
- relay 协议需要支持 mount 级别的消息路由（当前已部分实现）

## 风险

- tool_executor 的安全边界是最后一道防线，改动必须有充分测试覆盖
- relay 协议变更影响已部署的 local backend 实例
- 部分 legacy 调用方（本机后端、companion tools）address_space 为 None，需要逐个补齐

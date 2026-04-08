# 清除业务编排层的 workspace_root 冗余概念

> 状态：planning
> 优先级：P2
> 规模：S-M

## 问题

`workspace_root: PathBuf` 在业务编排层（spi / application / executor）与 `address_space.mounts` 语义重复。
新功能（skill 扫描、context 注入等）必须在两套体系间二选一，导致实现混乱。

## 边界定义

**要清理的**（业务编排层，不该感知裸物理路径）：
- `ExecutionContext.workspace_root` → 从 `address_space.default_mount().root_ref` 派生
- `SessionHub.workspace_root` → 改为按需从 address space 解析
- `PromptSessionRequest.workspace_root` → 移除，统一走 `address_space`
- `connector.rs` 里的 `workspace_root` 显示/路径逻辑 → 走 mount
- `bootstrap.rs`、`prompt_pipeline.rs` 中的 workspace_root 解析 → 走 address space
- `injection/resolver.rs` 的 `workspace_root` 参数 → 走 mount root_ref

**不动的**（基础设施层，物理路径是正当职责）：
- `tool_executor.rs`：本机安全边界，49 处保持原样
- `relay/protocol.rs`：relay 协议消息的物理路径字段
- `command_handler.rs`：relay 命令解析
- `apply_patch.rs`：本机 patch 安全边界
- `agentdash-local/main.rs`：本机后端启动参数

这些文件里 `workspace_root` 就是"本机物理根目录"的意思，概念正确。

## 迁移步骤

1. **确保所有进入业务编排层的入口都构建了 address_space**
   - `agentdash-local/command_handler.rs` 目前构造 `PromptSessionRequest` 时 `address_space: None` → 补齐，从 `workspace_root` 构建一个单 mount 的 address space
   - `companion/tools.rs` 同理

2. **从 PromptSessionRequest 移除 workspace_root**
   - 所有调用方改为传 address_space

3. **从 ExecutionContext 移除 workspace_root**
   - `prompt_pipeline.rs` 改为从 `address_space.default_mount()` 取路径
   - `connector.rs` 的系统 prompt 显示逻辑已有 address_space 分支，删除 else 分支

4. **从 SessionHub 移除 workspace_root**
   - 如果还需要默认路径（没有 address_space 时），用构建时传入的 default address space 代替

5. **injection/resolver.rs 的 workspace_root 参数**
   - 改为从调用方传入 mount root_ref（已 Optional，改动小）

## 前置条件

- `agentdash-local` 构造 `PromptSessionRequest` 时必须补齐 address space 构建（目前 `address_space: None`）
- 这是唯一的 blocker，做完后其余改动是纯机械删除

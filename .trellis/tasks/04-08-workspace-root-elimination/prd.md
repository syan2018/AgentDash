# 清除业务编排层的 `workspace_root` 冗余概念（边界收敛版）

> 状态：done  
> 优先级：P1  
> 规模：M  
> 最近更新：2026-04-08

## 背景与目标

`workspace_root` 在项目里应是**本机执行边界概念**，不是业务编排概念。  
云端/业务层应优先依赖 `workspace` / `mount` 元数据（`backend_id`、`mount_id`、`root_ref`），而不是把本机路径字符串一路向上传播并参与路由决策。

本任务目标从“删字段”升级为“**收敛职责边界**”：
- 云端编排层不再兜底注入本机路径；
- 路由优先使用 mount/workspace 元信息；
- 仅在 relay 本机执行边界保留 `workspace_root`。

## 分层边界（强约束）

### 必须保留 `workspace_root` 的层（基础设施边界）

- `agentdash-local/src/tool_executor.rs`（本机路径安全沙箱）
- `agentdash-relay/src/protocol.rs`（relay 协议当前路径字段）
- `agentdash-local/src/command_handler.rs`（协议入站路径校验）
- `agentdash-application/src/address_space/apply_patch.rs`（本机 patch 边界）
- `agentdash-local/src/main.rs`（本机后端启动根路径）

### 必须清理的层（业务编排层）

- 云端 `SessionHub` 默认地址空间不再使用 `current_dir` 本机路径兜底
- workspace 默认注入不再伪造 `local_fs` 路径 mount，应走 workspace binding 语义构造
- companion 回流续跑不再丢失 `address_space`（避免回落到错误默认路径链路）
- relay backend 解析不再按路径前缀匹配，改为 mount/workspace 元数据优先

## 落地计划与执行状态

### A. 路由语义收敛（relay 解析链）

- [x] `RelayPromptTransport::resolve_backend` 从 `mount_root_ref` 匹配改为 `preferred_backend_id` 优先 + executor 唯一匹配兜底
- [x] `RelayAgentConnector` 新增 `preferred_backend_id_from_context`，不再用路径参与后端路由
- [x] `RelayPromptRequest.workspace_root` 语义改名为 `mount_root_ref`（应用层）

### B. 云端默认路径兜底清理（本轮重点）

- [x] `agentdash-api/src/app_state.rs`：`SessionHub::new_with_hooks_and_persistence` 改为 `default_address_space = None`
- [x] 移除 cloud 侧 `current_dir` + `local_workspace_address_space` 默认注入
- [x] `session/context.rs::apply_workspace_defaults`：改为 `build_workspace_address_space(workspace)`，不再从 `root_ref -> Path -> local_fs` 伪造
- [x] `companion/tools.rs`：parent resume prompt 透传 `address_space` 与 `mcp_servers`，避免回落无上下文路径兜底
- [x] `relay_connector.rs`：改为直接读取 default mount 元数据（`backend_id/root_ref`），不再依赖 SPI 通用路径 helper 参与路由链路

### C. 协议迁移收口（兼容完成）

- [x] `command.prompt` 协议新增 canonical 字段 `mount_root_ref`
- [x] 迁移期双字段兼容：下发 `mount_root_ref + workspace_root`，接收端优先新字段、回退旧字段
- [x] `agentdash-local` 入站 prompt 校验切到 `effective_mount_root_ref()`，错误与日志语义改为 mount_root_ref
- [x] 新增 `agentdash-relay` 协议迁移单测，覆盖“旧字段兼容 / 新字段优先 / 双字段序列化”

## 验收与回归

已执行：
- `cargo check`（通过）
- `cargo test -p agentdash-application session::bootstrap::tests::build_plan_applies_workspace_defaults`（通过）
- `cargo test -p agentdash-application companion::tools::companion_tests::compact_execution_slice_drops_write_and_mcp_servers`（通过）
- `cargo test -p agentdash-application session::hub::tests::start_prompt_uses_request_address_space_override`（通过）
- `cargo test -p agentdash-relay`（通过，3 项协议迁移测试）

已知非本任务阻塞：
- `agentdash-api` 测试模块存在既有重复导入问题（`address_space_access/mod.rs`），不属于本次 `workspace_root` 清理引入。

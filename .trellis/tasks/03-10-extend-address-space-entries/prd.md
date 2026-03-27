# Address Space 条目检索与地址解析补全

## 背景

项目的 Address Space 能力发现、mount 级浏览器、`workspace_snapshot` 上下文注入和前端选择器骨架已经落地，但“按 space_id 做候选条目检索 + 把候选地址统一解析回上下文来源”的链路还没有补齐。

当前现状：

- `GET /api/address-spaces` 已能返回 `workspace_file` / `workspace_snapshot` / `mcp_resource` 等空间描述
- `GET /api/address-spaces/{space_id}/entries` 目前仍只支持 `workspace_file`
- 前端已有 `AddressEntryPickerPopup` / `useAddressSpacePicker`，但不同 space 的候选项形状还不统一
- 现有 mount browser 已支持直接浏览 mount，但未形成“从 picker 选地址 -> 回写统一 source ref”的闭环

## Goal

补齐 Address Space 的“候选条目检索 + 地址解析”闭环，使前端可以基于不同 `space_id` 获取可选项，并统一落回可消费的上下文来源表达。

## 当前真实缺口

### 1. `workspace_snapshot` 缺少 entries 端点支持

- 需要能返回至少一个可选条目，用于代表“当前工作区快照”
- 不要求做全文搜索，只需要让 picker 能选中该空间

### 2. `mcp_resource` 缺少 entries 端点支持

- 需要列出当前已注册/可见 MCP resource 候选
- 允许首轮只支持列资源，不支持复杂筛选

### 3. 缺少统一地址解析端点

- 需要把 picker 选中的 address 统一解析为可写入 `ContextSourceRef` 或等价结构
- 避免前端针对不同 space 手写转换规则

### 4. 前端 picker 仍停留在 `workspace_file` 视角

- 不同 space 需要有各自的 label / icon / entry_type 展示
- picker 返回值需要稳定表达 `space_id + address + label`

## 非目标

- 不重做当前 mount browser
- 不在本任务内实现新的 SourceResolver
- 不在本任务内引入新的 AddressSpaceProvider 类型

## Requirements

- `GET /api/address-spaces/{space_id}/entries` 至少支持：
  - `workspace_file`
  - `workspace_snapshot`
  - `mcp_resource`
- 新增统一地址解析接口，前端不再为各 space 手写回填逻辑
- 前端 picker 能根据 `space_id` 渲染不同候选项
- Story / Session 侧复用同一套 picker 返回模型

## Acceptance Criteria

- [ ] `workspace_snapshot` 在 entries API 下可返回至少一个候选条目
- [ ] `mcp_resource` 在 entries API 下可返回候选列表
- [ ] 存在统一 address resolve API，返回稳定的 source 表达
- [ ] 前端 picker 对非 `workspace_file` 空间可正常展示与选择
- [ ] 选中的地址可被 Session / Story 场景复用，而不是只在单页内部消费

## Related Files

- `crates/agentdash-api/src/routes/address_spaces.rs`
- `crates/agentdash-injection/src/address_space.rs`
- `frontend/src/services/addressSpaces.ts`
- `frontend/src/features/context-source/model/useAddressSpacePicker.ts`
- `frontend/src/features/context-source/ui/AddressEntryPickerPopup.tsx`

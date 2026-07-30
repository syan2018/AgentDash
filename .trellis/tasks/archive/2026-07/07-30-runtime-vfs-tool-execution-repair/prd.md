# 收束 Runtime VFS 工具执行链路

## 背景

当前 AgentFrame 中 `main` 挂载仍由 `relay_fs` 提供，并持续声明 `read / write / list / search / exec`。故障来自工具执行链内部的路径规范化与授权切片不一致：

- `fs_apply_patch` 已按 `main://...` 完成授权，但非 native provider 执行分支仍把完整 mount URI 当作 mount 内相对路径。
- `shell_exec` 使用显式 VFS cwd 时只得到 `Exec` grant，而命令物化阶段还会读取命令中的 VFS URI，因此在真正下发 relay 之前被拒绝。
- 当前命令扫描会把脚本数据区中的 URI 示例也当作待物化资源，存在把文档内容改写成本机路径的风险。
- 服务端已为失败调用写入终态；前端是否残留运行态需要用同类失败事件复现后判定。

Canvas 不是故障域，本任务不修改 Canvas 能力、挂载或文档。

## Goal

建立一条一致的 Runtime VFS 工具执行契约：规范的 `mount_id://relative/path` 在授权、路径规范化、provider 执行和 shell 物化各阶段表达同一资源；失败调用也能稳定收敛为可解释的终态。

## Requirements

### R1. Apply patch 路径归一化

- `fs_apply_patch` 在单 mount 执行边界把所有 patch header 路径归一化为 mount 内相对路径。
- inline、composite provider 与 native provider 消费同一份已归一化 patch，不再各自解释 mount URI。
- Add、Update、Delete、Move 的源路径与目标路径遵循同一规则。

### R2. Patch 授权与执行一致

- 授权仍以调用方提交的规范 mount URI 判定 mount、Write operation 和 path scope。
- 归一化发生在授权完成后的 provider 边界，并保留单 mount 校验、跨 mount 分组、move 约束和既有原子性。
- provider 收到的路径不得再次携带 mount 前缀。

### R3. Shell URI 解析统一

- `shell_exec.start` 的授权至少包含 cwd 的 `Exec` 需求与命令中真实路径引用的 `Read` 需求。
- 授权、执行物化和未解析 URI 拦截共用同一个候选提取函数，不能出现三套扫描规则。
- Read grant 按实际 mount/path scope 合并，不扩大为整 mount 权限。
- 同 backend 直接路径重写与跨 backend 物化都必须经过相同的 Read/Exec policy 校验。

### R4. Shell 脚本数据边界

- 只有 shell 实际消费的 VFS 路径引用进入资源计划。
- PowerShell here-string 数据区中的 URI 文本保持原值，不能被物化成本机绝对路径。
- 现有直接参数形式（含带引号参数）的 VFS URI 仍可被正确识别和改写。
- 词法规则必须由测试固定；不引入无法验证的通用 shell AST。

### R5. 错误语义

- 授权拒绝应指出 operation、mount 与受限 path；provider 能力错误只用于 provider 本身缺少编辑能力。
- 前置失败不得被表述为 durable mount 丢失或 provider 不支持整项能力。
- 调用在 relay 下发前失败时也必须产生完整工具终态。

### R6. 终态投影验证

- 用前置授权/物化失败事件验证 server history 到 session reducer、工具卡和 terminal store 的终态收敛。
- 仅在可复现前端残留 `in_progress` 时修复对应 reducer/store；若无法复现，则以回归测试和证据记录结论。

### R7. 测试与规范

- Rust 测试覆盖 relay/non-native provider 的显式 mount URI patch、Move、受限 path scope、显式 cwd + 跨 mount Read、同 backend 直连和 PowerShell here-string。
- 前端测试覆盖 `item_completed`/失败终态，确保没有悬挂工具卡或终端。
- 将最终形成的 provider 边界与 shell 资源计划契约更新到 VFS 规范。

## Acceptance Criteria

- [ ] 对 `relay_fs` 的 `fs_apply_patch` 使用 `main://path` 可完成 Add/Update/Delete/Move，provider 只接收 mount 内相对路径。
- [ ] patch 越权、跨 mount move 和 provider 能力不足分别返回准确、可区分的错误。
- [ ] `shell_exec` 以 `cwd: main://...` 执行且命令引用其他可读 mount 时，授权 grant 精确包含 cwd Exec 与引用路径 Read，并能完成直连或物化。
- [ ] 命令引用无 Read 权限路径时在副作用前被拒绝，错误包含 mount/operation/path 上下文。
- [ ] PowerShell here-string 中作为数据的 `main://...`、`lifecycle://...` 保持原文。
- [ ] 直接或带引号的 VFS 路径参数仍能被正确改写并执行。
- [ ] 前置失败调用在服务端和前端都收敛为终态，不残留运行中工具卡或 terminal 状态。
- [ ] 定向 Rust、前端测试和格式检查通过；未修改并行会话拥有的文件。
- [ ] VFS access/materialization 规范与最终实现一致。

## Scope

涉及：

- `agentdash-application-vfs` 的 patch service、URI rewrite、materialization 与 shell tool。
- `agentdash-infrastructure` 的 Runtime tool authorization。
- 必要时涉及 session stream reducer/terminal store 的最小终态修复与测试。
- VFS 规范更新。

不涉及：

- Canvas 挂载、Canvas skill、Canvas 文档或 Canvas 工作区模块。
- VFS URI 语法更换、新增顶层工具、通用 shell 解析器。
- Agent Runtime live observation 的整体重构。
- 数据库 schema；本任务预计无 migration。

## Constraints

- 项目尚未上线，直接收敛到唯一正确契约，不保留旧行为分支或静默回退。
- 优先删除重复解析、分支特例和隐式权限扩张；只有现有类型无法表达唯一契约时才增加新结构。
- 不触碰当前工作区中其他会话的修改。
- 若前端问题无法复现，不做猜测性改动。

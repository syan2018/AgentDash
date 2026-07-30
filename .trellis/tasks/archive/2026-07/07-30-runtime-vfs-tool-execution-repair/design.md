# Runtime VFS 工具执行链技术设计

## 1. 收束原则

本次修复优先做减法：

- 删除 native/non-native provider 各自解释 patch path 的分叉。
- 删除 authorization、materialization 与 unresolved guard 各自解释 command URI 的分叉。
- 不为不可复现的前端状态增加补偿逻辑。

只有在现有调用参数无法承载共享结果时，才引入最小资源计划类型；该类型替代重复扫描，不形成第三套解释层。

## 2. 目标架构

### 2.1 Patch

```text
tool patch
  -> 解析并按 mount 授权
  -> 按 mount 分组
  -> 归一化为 provider-relative patch
  -> inline / composite / native provider
```

`mount_id://relative/path` 是调用与授权身份；进入单 mount provider 后，路径身份已经由 mount 参数承载，patch header 只保留相对路径。

### 2.2 Shell

```text
command + cwd
  -> 共享 shell URI 候选扫描
       - 跳过 PowerShell here-string 数据
       - 保留真实路径引用及 replacement spans
  -> 精确 RuntimeVfsExecutionGrant
  -> 使用同一候选语义校验并物化
  -> relay/local shell
```

授权、物化和未解析 URI 拦截统一调用同一个 scanner，不新增并列资源模型。

## 3. Patch 设计

### 3.1 单一归一化边界

- 将当前仅服务 native provider 的路径 header 归一化提升为单 mount patch 的公共步骤。
- 归一化同时处理 Add/Delete/Update/Move，并校验显式 mount 与当前分组 mount 一致。
- patch body 不参与 URI 替换，代码或文档中的 URI 字面量保持原值。
- inline、composite 与 native 分支都消费归一化结果。

### 3.2 解析与执行

- 优先让已解析的 `PatchEntry` 在分组后直接进入 `apply_entries_to_target`，避免 parse → string rewrite → parse 的语义漂移。
- native provider 若协议仍接收 patch string，则只在 provider adapter 边界从已校验条目生成规范 patch，或使用同一 header-only normalizer；两种分支必须共享测试向量。
- 单 mount入口与 multi-mount 入口共用归一化 helper。

### 3.3 权限与错误

- policy admission 使用归一化前的 mount 身份和归一化后的 relative path。
- Move 同时校验源、目标 path scope。
- provider 能力检查发生在路径/授权通过之后，错误类型不混淆。

## 4. Shell URI 扫描设计

### 4.1 共享候选

- 保留通用 `find_mount_uri_candidates` 服务 JSON leaf rewrite。
- 增加 shell 专用薄封装，只负责排除 PowerShell here-string 的数据范围。
- authorizer、materializer 和 unresolved guard 都调用该薄封装。
- authorizer 对候选 mount/path 去重后追加 Read grant；provider 与物理路径仍只在执行期解析。

### 4.2 脚本数据词法边界

不引入通用 shell AST，仅固定本次 Windows 故障需要的最小边界：

- 普通命令区：识别裸参数与单/双引号参数中的已知 mount URI。
- PowerShell here-string：从合法起始标记到行首终止标记均视为数据，不产生候选。
- URI delimiter、引号与 replacement span 必须基于原始 command 字节位置。

### 4.3 授权合并

- start + 显式 VFS cwd：授予 cwd 精确路径的 Exec，再为 plan 中每个 URI 请求精确 Read。
- start + platform shell：保持现有虚拟 shell 授权语义，本任务只复用 grant merge，避免扩大修复面。
- continuation：只需要既有终端 owner/operation 所需的执行授权，不重新解释 command。
- 合并 grant 时保留 operation 对应 path scope，不能因为同 mount 上另一个 operation 是 All 而扩大 Read。

### 4.4 执行物化

- 同 backend：policy 通过后将 source mount root 与 relative path 组合为本机路径。
- 跨 backend：policy 通过后构造 materialization payload。
- replacement 使用共享 scanner 返回的 span。
- 数据区未进入候选，故不会被替换，也不会产生无关 Read grant。

## 5. 终态投影

先构造“授权或物化阶段失败、没有 relay dispatch”的回归场景：

1. 确认服务端仍产生 tool item terminal event。
2. 将相同事件序列送入 `sessionStreamReducer`。
3. 检查工具卡状态及 `useTerminalStore` 是否清除/终止对应状态。

只有第 2/3 步可复现残留运行态时才修改前端。否则仅补回归测试，结论记录在任务研究中。

## 6. 兼容与迁移

项目未上线，直接采用唯一的新契约：

- provider 只接收 mount-relative patch path。
- shell URI rewrite 只针对共享 scanner 识别出的命令路径。
- 不保留旧的全 command 字符串扫描或额外权限回退。
- 无数据库结构变化，无 migration。

## 7. 主要风险

- PowerShell/POSIX 词法边界遗漏导致真实参数漏物化或数据被误改写。
- grant 合并仍受现有 invocation VFS surface 表达能力限制。
- multi-mount Move 与 native patch adapter 若各自序列化，可能出现路径语义漂移。
- 当前工作区存在并行修改；实现必须限制在本任务文件及确认无冲突的代码范围。

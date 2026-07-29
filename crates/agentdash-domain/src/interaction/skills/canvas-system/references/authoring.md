# Canvas 创作

## Source 模型

Canvas 是 `InteractionDefinitionRevision(kind=canvas)`，包含：

- immutable `SourceBundle`；
- entry file 与规范化 source files；
- sandbox libraries 与 import-map declarations；
- initial state 与 JSON state schema；
- 可选的 Agent state projection、commands、component bindings 与 resource slots。

使用 `canvas:{definition_id}` 作为 module identity，使用 `canvas://{definition_id}` 作为 preview
identity。不要从 revision、VFS mount、Interaction instance 或 browser tab 派生这两个身份。

## Revision 流程

1. 读取当前 definition 与 revision。
2. 以准确的 current revision ID 作为修改基线。
3. 用规范化 source changeset 表达文件修改。
4. 提交一个新的 immutable revision。
5. 继续修改前重新读取或 describe definition。

不要原地修改旧 `SourceBundle`。revision conflict 表示 authoring base 已过期，应重新加载并显式重放修改。

## Agent capability 边界

使用 `workspace_module_operate(operation="canvas.create"|"canvas.attach"|"canvas.copy")`
建立准确的 Canvas authoring module 与 mount。create/copy 创建 personal definition；attach
只挂接已有 definition。返回结果中的 `canvas_mount_id` 是 VFS mount identity。

通过通用 `fs_read`、`fs_glob`、`fs_grep` 与 `fs_apply_patch` 使用
`{canvas_mount_id}://...`。personal mount 可写，project shared mount 只读；都不支持 exec。
source mutation 生成新 immutable revision，不改变 module 或 mount identity。

Project Canvas manager 的 UserWorkshop HTTP 用例不等于 Agent authority；Agent 使用 Workspace
Module 与 VFS 工具。

## Source 编写原则

- 保证 entry file 有效且存在于 bundle。
- 使用较小的 source tree 与明确的本地 import。
- 把有状态业务事实保存在 Interaction state，而不是仅存在于 DOM 变量中。
- 用 resource slot 声明外部数据，不嵌入 credential 或 host path。
- 把 Extension component 作为固定 ABI 的显式 binding，不进行任意 same-realm import。

# Workspace Module Surface 诊断

## 解释 list/describe

先读取 `surface_readiness`，再解释 module 数量：

- `ready` 且零 modules：当前 execution authority 的权威空 surface。
- `degraded`：读取每条 `surface_diagnostics` 的 provider、code 与 message，说明哪个 catalog
  facet 不可用。
- typed tool failure：当前 execution authority 或 Product surface 无法解析，不等于空 module
  surface。

Dynamic Operation provider 的 descriptor 集合可能被 Gateway 整组隔离，因此其它 provider 仍可
显示。不要因为 `builtin:*` 仍存在就推断 Canvas、Extension 或 Interaction provider 正常。

## Builtin modules

- `builtin:vfs`：workspace mount、读取、搜索与 patch Operations。
- `builtin:process`：`shell_exec` 等 workspace process Operations。
- `builtin:task`：Project Task 读取与写入 Operations。

Builtin modules 没有 UI view。它们把显式暴露的原生工具投影为 canonical Operations，供
`operation_script` 与其它 providers 组合；实际执行仍重新进入 platform tool broker 与当前资源
授权。

## 恢复动作

authority、permission、attachment、revision 或 provider readiness 改变后重新 list/describe。
不要用猜测的 OperationRef 绕过 unavailable provider，也不要把 degraded surface 当成稳定的完整
catalog。

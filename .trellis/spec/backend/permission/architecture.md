# Permission Architecture

PermissionGrant是AgentRun-scoped产品授权事实。Business Surface把active grants编译为immutable permission/tool requirements；Managed Runtime/Tool Broker在canonical binding与operation坐标上执行。

- grant source audit引用`source_runtime_operation_id`。
- approve/reject/revoke先写授权事实，再触发surface revision/admission；应用失败返回可见diagnostic。
- Driver不能直接读取grant repository或扩大permission profile。
- approval interaction使用canonical Runtime Interaction；UI通过facade resolve。
- credential与permission分离：credential broker只解析声明slot，permission仍由Tool Broker判定。

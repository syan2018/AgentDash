# WI-02 UserWorkshop And Canvas Standalone

Status: planned

Depends On: WI-01

## Scope

- Authenticated User + Project/Interaction access adapter。
- standalone discovery/invoke host bridge。
- Canvas 提交完整 Rhai source 到服务端 OperationScript executor；iframe 不解释执行脚本。
- Project Canvas asset preview 与 attached runtime preview 分离。
- 冻结 `canvas:{definition_id}` / `canvas://{definition_id}` 与 `interaction:{instance_id}` / `interaction://{instance_id}`。
- standalone preview、instance 与 attachment resource binding 进入同一 authorization resolver。
- 删除前端已失效 `/canvases/{id}/runtime-snapshot` 请求。
- 修正 agent submit interaction/render refs 合同。

## Exit Criteria

- Canvas/Extension panel 在没有 AgentRun/AgentFrame/RuntimeSession 时可发现并调用授权 Operation。
- endpoint 断口消失且没有恢复 legacy route。
- 每次 invoke 重新 admission，surface handle 不充当 capability token。
- Canvas 保存的 script source 不是独立 OperationScript asset 或 execution identity。
- attachment-local resource binding 只服务当前 actor，不成为 shared instance authority。

## Validation

- API/application access tests。
- Canvas service/panel focused tests。
- authority injection negative tests。

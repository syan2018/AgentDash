# WI-02 UserWorkshop And Canvas Standalone

Status: done

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

## Implementation Evidence

- `7b1c1c49` 建立 UserWorkshop、Canvas、Interaction、AgentRun、Extension panel/service 的 trusted host bridge；caller contract 只包含 Operation identity/input/idempotency，trace、deadline、principal、scope、origin、placement 与 authority revision 均由服务端绑定或解析。
- discovery 通过 `surface_current` 接收稳定 `ScopeRef`，resolved authority revision 只在 Gateway/Core 内部流转；direct/nested invocation 继续共享每次调用的 TOCTOU re-admission。
- authoring identity、runtime identity 与 attachment identity 已分离：`canvas:{definition_id}`、`interaction:{instance_id}` 不作为 attachment ref，只有显式 Interaction attachment id 能进入 attachment-local binding。
- `4fe22ffb` 将 Interaction definition/instance repository 装入 application composition；User/Project ownership、Interaction state revision、Canvas definition revision、AgentRun current AgentFrame 与 enabled Extension installation 共同生成 current authority revision。
- focused validation：domain Operation refs `3/3`、runtime-gateway Operation/host authority `14/14`、`cargo check -p agentdash-api --lib` 通过。

## Remaining Integration

- canonical catalog 仍需接入 exact MCP tool 与 Extension Operation providers，随后 Canvas/Extension panel route 才能完全移除 legacy `RuntimeGateway` invocation。
- Canvas inline Rhai caller 依赖 WI-03 executor composition；agent submit 的 interaction/render ref 由 WI-05 基于最终 InteractionInstance/event/presentation contract 落地，不继续读取即将删除的 Canvas runtime snapshot tables。
- preview/instance/attachment 的公开 URI 与 frontend bridge 在 WI-05 migration 中接入当前 resolver；本项不把 legacy Canvas repository 重新提升为 authority。

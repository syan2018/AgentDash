# WI-07 Workspace Module Convergence

Status: planned

Depends On: WI-01、WI-03、WI-04、WI-05、WI-06

## Scope

- Workspace Module 只保留 Agent-facing lifecycle/discovery/presentation projection。
- list/describe/invoke/present 消费 canonical Operation/Interaction refs。
- authoring `canvas:*` 与 runtime `interaction:*` module/presentation identity 分离并同源投影。
- 删除重复 provider resolution、schema/admission、Canvas state 和 Extension dispatch。
- 收束 generated contracts、frontend consumers 与 runtime tool provider。

## Exit Criteria

- Workspace Module 不拥有第二套 operation catalog 或 interaction state。
- describe/invoke schema、visibility、readiness、provenance 同源。
- 旧 weak parser/manual DTO/bypass resolver 静态扫描为空。

## Validation

- workspace-module crate tests。
- Agent runtime tool/catalog tests。
- contracts/frontend focused checks。

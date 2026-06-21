# Contract Boundary Design

## Boundary Rules To Evaluate

- application read model 与 browser-facing wire DTO 是否分离。
- API adapter 是否是 read model -> wire DTO 的默认映射边界。
- `agentdash-contracts` 是否允许持有 domain/SPI/protocol conversion。
- incoming command DTO -> domain command conversion 是否应离开 contracts crate。

## Audit Output Shape

| Import / Conversion | Current Owner | Proposed Owner | Action |
| --- | --- | --- | --- |
| application -> contracts import | TBD | TBD | keep / migrate / document |
| contract From<domain> | TBD | TBD | keep outbound / move incoming |

## Migration Principle

先审计，后迁移。只对高风险入口创建实现任务，避免为分层纯度进行一次性大规模移动。


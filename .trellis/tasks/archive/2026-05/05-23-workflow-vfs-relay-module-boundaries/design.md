# Workflow/VFS/Relay 模块边界拆分 Design

## Boundary

本任务是目录级/文件级模块拆分。目标是让语义边界可见，并为后续 crate 级拆分打底。

## Batch Strategy

每次只选一个 area：

1. Workflow value objects；
2. VFS tools/providers/mutation；
3. Relay protocol payload；
4. Agent loop internals。

本任务已恢复为实现任务。先前归档只完成每个区域的最小抽取，未达到 review 中对巨型文件语义拆分的预期。后续继续按批次推进，每个批次仍保持单一 bounded context。

## Principles

- public re-export 保持调用方改动最小。
- wire format、database schema、serialized enum 名称不变。
- 拆分时优先移动类型和纯 helper，再移动带副作用逻辑。
- 每个 batch 后运行针对性 check。

## Candidate First Batch

Workflow value objects 适合作为第一批，因为它主要是领域类型与 validation 的文件级拆分，行为风险低，收益明显。

## Phase Commit Plan

| 阶段 | 边界 |
| --- | --- |
| Workflow value objects | Workflow value objects / validation / activity state |
| VFS boundary | VFS core / providers / tools / mutation / materialization / surface |
| Relay protocol payload | Relay payload 子协议 |
| Agent loop internals | Agent loop turn/tool/event/cancel/prompt/output |

## Spec Update

每完成一个 area，更新对应 architecture appendix 的模块边界说明。

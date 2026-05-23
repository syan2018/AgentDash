# Workflow/VFS/Relay 模块边界拆分 Design

## Boundary

本任务是目录级/文件级模块拆分。目标是让语义边界可见，并为后续 crate 级拆分打底。

## Batch Strategy

每次只选一个 area：

1. Workflow value objects；
2. VFS tools/providers/mutation；
3. Relay protocol payload；
4. Agent loop internals。

本任务只负责建立批次边界；具体代码移动按批次提交，避免一个提交同时改动 domain、application、relay 和 agent runtime。

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

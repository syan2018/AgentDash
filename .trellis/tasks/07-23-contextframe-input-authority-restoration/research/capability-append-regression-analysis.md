# Bug Analysis: Dash Capability Context 被拆帧并伪装成全量 Delta

## 1. Root Cause Category

- **Category**: B / D / E — Cross-Layer Contract、Test Coverage Gap、Implicit Assumption
- **Specific Cause**:
  - Native Adapter分别物化`CapabilityManifest` instruction与ToolSchema，导致同一个accepted
    surface transition产生两个可独立消费的CAP frame。
  - Tool renderer先正确计算added/removed/changed，随后又遍历current tools输出全量schema；
    代码隐含假设“每轮只读取最新surface”，因此必须靠全量重放维持模型上下文。
  - Dash provider materializer确实只读取最新`state.surface.context_frames`，没有从native history
    恢复active surface的append序列。
  - 测试只证明provider prompt包含工具字段说明、结构化delta正确，没有同时断言CAP frame数量、
    `system_append` mode、更新frame不含未变化schema以及上一revision append仍在下一round上下文。

## 2. Why The Previous Fix Failed

1. **Surface Fix**：上一轮消除了canonical/provider的第二套renderer，却保留了
   capability-instruction frame与tool-schema frame两个物化入口，单一文本owner并不等于单一投递事实。
2. **Incomplete Scope**：ToolSchema section已经是added/removed/changed，但`rendered_text`仍从
   current tools全量生成；只验证section无法发现Agent原文仍是snapshot。
3. **Mental Model**：把surface视为当前snapshot，没有把`SystemAppend`理解为需要由native history
   恢复的序列语义，于是实现自然退化为“最新surface重放全部内容”。
4. **Test Gap**：集成测试曾明确期待可读delta不出现完整schema关键字，也没有检查delivery mode；
   因而简化文本和错误消费策略都能通过。

## 3. Bayesian Update

| Hypothesis | Prior | Discriminating evidence | Posterior |
| --- | ---: | --- | ---: |
| 前端自行简化或错误同步 | 25% | `ContextFrameBody`直接展示`frame.rendered_text`，没有重渲染 | 2% |
| 后端把capability与schema拆成两个frame | 35% | `materialize_surface_frames`先遍历manifest instruction，随后额外push tool frame | 49% |
| renderer把真delta重新扩成全量snapshot | 25% | `render_tool_schema`接收all current tools并逐项输出 | 31% |
| provider materializer没有append历史 | 15% | `render_accepted_context`只读取latest surface frames | 18% |

三项后端假设并非互斥；红测分别在“两个CAP frame”“未变化tool被重放”“下一round丢失上一append”
处失败，组合解释置信度超过99%。

## 4. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
| --- | --- | --- | --- |
| P0 | Architecture | capability manifest sections与ToolSchemaDelta只从一个materializer生成一个CAP frame | DONE |
| P0 | Architecture | Dash按native history恢复active surface的SystemAppend ledger，revoke清空 | DONE |
| P0 | Test Coverage | unit断言单frame、`context/system_append`、真实section delta、no-op无frame | DONE |
| P0 | Test Coverage | active-turn integration断言下一round同时含initial append与新delta | DONE |
| P0 | Test Coverage | revoke integration断言失效schema不再进入provider prompt | DONE |
| P1 | Documentation | Native Adapter、Capability Pipeline、Backbone与cross-layer guide固化同一合同 | DONE |
| P1 | Frontend Contract | Dash CAP fixture使用真实`system_append` metadata | DONE |

## 5. Systematic Expansion

- **Similar Issues**：identity、initial context与compaction summary同样需要检查“accepted frame =
  provider-visible text = canonical presentation”；本任务已有对应纵向测试，未发现新的独立renderer。
- **Design Improvement**：区分stable current-frame物化与append ledger恢复。前者可从folded state读取，
  后者必须保留提交顺序和revoke边界。
- **Process Improvement**：ContextFrame相关修复必须同时验证四个维度：frame数量、delivery mode、
  structured sections、最终provider原文；单独的contains断言不能证明单一权威。
- **Knowledge Capture**：已更新三份领域spec与cross-layer thinking guide；仓库不存在
  `src/templates/markdown/spec/`，无需模板同步。

## 6. Follow-up Regression: Stable Snapshot 被当作 Canonical Change

真实页面复验发现tool-only surface update除CAP外又发布了identity、environment、guidelines和
assignment。最小Complete Agent回归证明revision 2只新增一个tool时实际产生8条ContextFrameChanged，
预期为1条CAP。

- **Root category**：B / D — snapshot与transition的跨层合同未分离，缺少第二次surface projection
  的纵向测试。
- **Why previous checks missed it**：测试分别证明provider prompt拥有完整stable context、CAP自身是真
  delta，却没有证明canonical projector不会把current stable snapshot全量当成delta。
- **Fix**：`entry_records`显式接收history fold中的previous surface与current entry，而不复制完整
  turn/item state。非SystemAppend frame按instruction source identity比较，忽略surface级id/cache
  identity，真实delivery/section/text变化仍发布；CAP继续作为append transition直接发布。
- **Shared-path proof**：history read、changes replay与durable live callback都在replay apply前捕获
  previous state，并调用同一个projector。
- **Prevention**：新增tool-only revision回归，并在Native Adapter、Backbone与cross-layer guide中
  固化“current snapshot供恢复，previous/current diff供事件”的合同。

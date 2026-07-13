# Frontend Architecture

Frontend以产品路由与generated contracts组织：Project/Story/Task/Lifecycle是产品read models；AgentRun workspace通过`run_id + agent_id`消费canonical Runtime snapshot/events/context。

## Invariants

- API client只使用generated Rust contracts；不手写Runtime/vendor DTO。
- AgentRun command availability只来自Runtime snapshot，不从产品status、Backbone或executor kind推导。
- Runtime feed按snapshot transcript baseline + durable/live双cursor消费canonical events；Runtime adapter只输出feature-local `SessionPresentationEvent`，再进入`useSessionFeed -> SessionChatStream -> SessionEntry -> toolCardRegistry`。target切换隔离旧state。
- `AgentRuntimeFeed`等平行renderer不存在；AgentRun workspace只提供Runtime target、inspect、command availability与product projection，不拥有第二套会话UI。
- Workspace Module/Canvas tab以concrete presentation URI为identity；layout按AgentRun product key持久化。
- VFS/resource surface来自current AgentFrame/Business Surface；Runtime binding只提供typed execution coordinate。
- UI intent必须对应真实API/facade command；无canonical endpoint的按钮、service与contract必须一起删除。
- errors保持typed code/diagnostic；stale command触发inspect refresh，不静默retry不同语义命令。

## Data Flow

```text
React intent
  -> typed service
  -> AgentRun API/facade
  -> Runtime operation receipt
  -> snapshot/events refresh
  -> view model
```

## Tests Required

- generated contract check与TypeScript typecheck。
- command-state availability、target isolation与stream cursor tests。
- session presentation parity覆盖message/reasoning/plan/tool/context/Companion/usage/error/interaction、item terminal与transient generation切换。
- service URL/encoding、Draft create/composer/cancel/context/approval tests。
- Workspace presentation、Canvas/VFS surface与Runtime Lost UI tests。

可以，我把它画成“事件事实层 + durable projection 层 + 模型读取层 + UI 审计层”四层来看。当前这套架构的核心思想是：**`session_events` 永远是事实源，compaction 不改历史；压缩结果作为 durable projection 持久化，模型上下文读取时用 projection summary + suffix events 重建当前可见上下文。**

```mermaid
flowchart TB
  subgraph Runtime["运行时事件流"]
    A["用户 / Agent / Tool 事件"] --> B["SessionEventingService.persist_notification"]
    B --> C["session_events<br/>append-only fact log"]
  end

  subgraph Trigger["压缩触发与结果"]
    D["Hook / token pressure<br/>触发上下文压缩"] --> E["Agent 生成 CompactionSummary"]
    E --> F["context_compacted notification<br/>包含 summary / tokens_before / messages_compacted"]
    F --> G["maybe_enrich_compaction_notification<br/>补 compacted_until_ref"]
    G --> H["maybe_commit_compaction_projection"]
  end

  C --> D
  G --> C

  subgraph Durable["Durable Projection 持久化层"]
    H --> I["commit_compaction_projection<br/>一个原子提交"]
    I --> J["session_events<br/>写 completed event"]
    I --> K["session_compactions<br/>写 compaction checkpoint"]
    I --> L["session_projection_segments<br/>写 summary segment"]
    I --> M["session_projection_heads<br/>更新 active head"]
  end

  subgraph Read["模型上下文读取"]
    N["ContextProjector.build_model_context"] --> O["读取 projection head"]
    O --> P["读取 active compaction"]
    P --> Q["读取 projection segments"]
    Q --> R["读取 session_events suffix"]
    R --> S["AgentContextEnvelope<br/>summary segment + suffix transcript"]
  end

  M --> N
  K --> N
  L --> N
  C --> N

  subgraph UI["前端投影视图 / 审计"]
    T["GET /sessions/{id}/context/projection"] --> N
    S --> U["SessionProjectionViewResponse"]
    U --> V["SessionProjectionView"]
  end
```

更细一点，持久化表之间现在大概是这个关系：

```mermaid
erDiagram
  session_events {
    string session_id
    int event_seq
    string session_update_type
    string turn_id
    int entry_index
    json notification_json
  }

  session_compactions {
    string id
    string session_id
    string projection_kind
    int projection_version
    int start_event_seq
    int completed_event_seq
    int source_start_event_seq
    int source_end_event_seq
    int first_kept_event_seq
    string summary
    json replacement_projection_json
    json token_stats_json
    string status
  }

  session_projection_segments {
    string id
    string session_id
    string projection_kind
    int projection_version
    int sort_order
    string segment_type
    int source_start_event_seq
    int source_end_event_seq
    json source_refs_json
    json content_json
    int token_estimate
  }

  session_projection_heads {
    string session_id
    string branch_id
    string projection_kind
    int projection_version
    int head_event_seq
    string active_compaction_id
    int updated_by_event_seq
  }

  session_events ||--o{ session_compactions : "completed_event_seq points to event_seq"
  session_compactions ||--o{ session_projection_segments : "generated_by_compaction_id"
  session_compactions ||--o| session_projection_heads : "active_compaction_id"
  session_projection_heads ||--o{ session_projection_segments : "projection_version selects"
```

一次成功压缩的写入时序是这样：

```mermaid
sequenceDiagram
  participant Hook as Hook / Agent
  participant Eventing as SessionEventingService
  participant Store as SessionPersistence
  participant Events as session_events
  participant C as session_compactions
  participant Seg as session_projection_segments
  participant Head as session_projection_heads
  participant UI as context_frame

  Hook->>Eventing: context_compacted(summary, messages_compacted, tokens_before)
  Eventing->>Events: 读取历史事件，推导 compacted_until_ref
  Eventing->>Eventing: enrich context_compacted payload
  Eventing->>Store: commit_compaction_projection(commit)

  rect rgb(235, 245, 255)
    Store->>Events: 写 completed event，获得真实 event_seq
    Store->>C: 写 compaction，completed_event_seq = 真实 event_seq
    Store->>Seg: 写 summary segment
    Store->>Head: head_event_seq = 真实 event_seq<br/>updated_by_event_seq = 真实 event_seq
  end

  Eventing->>UI: 写 context_frame(kind=compaction_summary)
  UI->>Head: 普通事件推进 head_event_seq 到 context_frame 事件
```

这里有一个容易混淆但很关键的点：
`session_projection_heads.head_event_seq` 不是“生成 summary 的那条事件”，而是**当前 projection + suffix 覆盖到的最后事件**。所以 compaction commit 当下它等于 completed event seq；如果随后又写入 `context_frame`，head 会继续推进，表示模型上下文 suffix 可以覆盖到更靠后的事件。

读取模型上下文时，当前路径是：

```mermaid
flowchart LR
  A["ContextProjector.build_model_context"] --> B{"是否有 projection head?"}

  B -- "没有" --> C["从 session_events 构造 raw transcript"]
  C --> D["估算 raw transcript token_estimate"]
  D --> Z["AgentContextEnvelope"]

  B -- "有，但无 active_compaction_id" --> E["读取 <= head_event_seq 的 raw transcript"]
  E --> F["估算 raw transcript token_estimate"]
  F --> Z

  B -- "有 active_compaction_id" --> G["读取 session_compactions"]
  G --> H["读取 session_projection_segments"]
  H --> I["恢复 CompactionSummary"]
  I --> J["messages_compacted<br/>优先 segment.source_refs_json<br/>再 compaction.token_stats_json"]
  I --> K["compacted_until_ref<br/>优先 segment.source_refs_json<br/>再 compaction.replacement_projection_json"]
  G --> L["计算 suffix_start_event_seq<br/>first_kept_event_seq 优先"]
  L --> M["从 session_events 读取 suffix<br/>suffix_start_event_seq..head_event_seq"]
  H --> N["token_estimate = summary segment + suffix messages"]
  J --> Z
  K --> Z
  M --> Z
  N --> Z
```

前端现在的投影视图刷新也有自己的边界控制：

```mermaid
flowchart TB
  A["SessionChatView rawEvents"] --> B["computeProjectionRefreshKey"]

  B --> C{"事件类型"}
  C -- "agent_message_delta / reasoning_delta / token_usage_updated" --> D["不刷新 projection panel"]
  C -- "turn_completed" --> E["刷新"]
  C -- "context_compacted" --> E
  C -- "platform session_meta_update<br/>key=context_compacted" --> E
  C -- "platform session_meta_update<br/>key=context_frame && kind=compaction_summary" --> E

  E --> F["SessionProjectionView refreshKey 改变"]
  F --> G["GET /sessions/{id}/context/projection"]
  G --> H["展示当前模型可见上下文"]
```

从职责边界看，当前架构可以压缩成这张 mental model：

```mermaid
flowchart TB
  A["session_events<br/>事实源，不可变历史"] --> B["ContextProjector"]
  C["session_compactions<br/>压缩 checkpoint 总账"] --> B
  D["session_projection_segments<br/>模型可见替代片段"] --> B
  E["session_projection_heads<br/>当前 projection 游标"] --> B

  B --> F["AgentContextEnvelope<br/>模型真实输入"]
  B --> G["SessionProjectionViewResponse<br/>UI 审计视图"]

  H["context_frame<br/>人类可读审计事件"] --> A
  H -. "不是事实源，只是展示/审计事件" .-> G
```

我会特别记住三个架构不变量：

1. **事实历史只在 `session_events`**
   压缩不会删除、重写、截断历史事件。它只是提交一个新的 projection checkpoint。

2. **模型上下文不是直接等于事件流**
   有 durable projection 后，模型看到的是：`CompactionSummary segment + suffix transcript`。

3. **边界语义必须来自 checkpoint metadata**
   `messages_compacted` 是消息数，不是 event seq 数；`compacted_until_ref` 是稳定消息引用，不应该靠 event range 临时猜。

当前我最担心的潜在分叉点，也可以直接放在图上看：

```mermaid
flowchart LR
  A["context_compacted event checkpoint"] --> D{"checkpoint 解释逻辑"}
  B["session_compactions metadata"] --> D
  C["session_projection_segments source_refs_json"] --> D

  D --> E["Continuation restore"]
  D --> F["ContextProjector durable restore"]
  D --> G["Projection View"]
  D --> H["未来 fork / rollback"]

  I["风险：如果 E/F/G/H 各自解析 JSON<br/>会长出多套 compaction boundary 语义"]
```

所以我的架构建议很明确：下一步如果继续打磨，不是大拆，而是抽一个小的 `CompactionCheckpoint` 解析层，把下面这些规则集中起来：

- `messages_compacted`: segment `source_refs_json` > compaction `token_stats_json` > `0`
- `compacted_until_ref`: segment `source_refs_json` > compaction `replacement_projection_json` > `None`
- `source_range`: segment range > compaction range
- `token_estimate`: projection/fallback summary + suffix transcript

这样 #33 做 fork / rollback 时就不会自己再发明一套“恢复上下文”的逻辑。当前体系的骨架是对的，最需要防的是这些边界规则散落在 continuation、projector、UI view、lineage API 里慢慢分叉。

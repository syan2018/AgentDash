# Session Replay 现状问题收敛

## 用户现象

- 重新加载同一 session 后，tool call 卡片在页面底部不断堆积
- 会话文本会重复输出多遍
- 某些系统事件会再次触发 UI 副作用

## 当前根因

### 1. 前端 cache 与 replay 叠加

- `useAcpStream` 先恢复旧 `entries`
- transport 再从头 replay 历史
- 非稳定 upsert 的条目被再次 append

### 2. 后端 replay 边界 race

- 先读历史
- 后订阅实时流
- 中间写入的事件可能漏掉

### 3. cursor 语义错误

- `connected.last_event_id` 在 replay 发完前就宣称“已追平”
- 中途断线可能跳过尚未送达的历史

### 4. 副作用去重主键错误

- 当前按前端随机 entry id 去重
- replay 后同一历史事件会再次触发副作用

## 结论

这不是单个 reducer 或单个接口的小 bug，而是 session 事件系统缺少：

- 稳定数据库事实源
- 稳定事件序号
- 稳定 replay cursor
- 稳定前端幂等消费边界

因此本任务按“数据库事件流 + 投影状态 + 稳定 cursor”整体重做。

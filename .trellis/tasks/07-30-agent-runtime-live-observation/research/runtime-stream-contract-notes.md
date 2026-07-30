# Runtime Stream 相关跨层合同摘录

本文件提取 `cross-layer/frontend-backend-contracts.md` 中与本任务直接相关的稳定边界，
用于避免完整 spec 超过子流程上下文注入大小。实现完成后应把最终合同更新回正式 spec。

## 现有稳定边界

- Rust Runtime contract是浏览器 wire shape事实源，TypeScript由生成器产生。
- Runtime Contract、RuntimeWire与Backbone contracts彼此独立，不以JSON中转复用。
- 浏览器不接触 concrete source、Host generation、callback route或placement credential。
- Session baseline来自 Complete Agent authoritative read。
- lane sequence只在当前连接有效，不是跨重启 durable cursor。
- source切换、Lagged、sequence gap或连接断开后丢弃 partial lane并读取 authority。
- UI command availability只来自 Agent owner observation，不能从conversation或Product状态推导。
- RuntimeWire/Relay只承载 Complete Agent transport；其connection epoch和generation不进入
  Product persistence。
- Context projection继续以当前 observation context coordinate作为required lower bound。

## 本任务需要替换的漂移合同

现有文档要求每条 Runtime update同时携带control facts与presentation records。当前实现把它解释为：

```text
收到旧 live event
-> 重新 read 当前 authoritative snapshot
-> 将当前 control/full conversation 与旧 presentation封装为一个 update
```

该解释不能保证时间一致性，也导致每 token完整 history replay。

新合同应明确：

- update中的state只来自 owner在真实状态转移点直接发布的轻量状态；
- ephemeral presentation没有state时保持 `None`，不得事后读取当前状态补齐；
- baseline是完整 read，update不是；
- reset/gap后重新读取 baseline；
-前端control只消费最新state revision，presentation按lane顺序增量归约。

## 必须保留的验证面

- generated contract write/check与TypeScript typecheck；
- connection baseline、duplicate、gap、Lagged、reconnect和typed stream error；
- target切换隔离；
- interaction controls只使用state availability；
- context coordinate revision/digest fence；
- hydration baseline内的 durable presentation只恢复展示，不重放命令式副作用；
- RuntimeWire callback progress保持correlation、顺序和disconnect语义。

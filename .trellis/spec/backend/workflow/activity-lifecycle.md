# Activity Lifecycle and Runtime Evidence

Activity attempt 是 Workflow 产品状态；Agent Runtime operation 是执行事实。Launcher 把 attempt coordinate写入 Runtime actor/source metadata，terminal projector 再以 operation ID幂等推进 attempt。

- `Ready -> Running` 发生在 canonical Runtime operation accepted 后。
- `Running -> terminal` 只消费 canonical operation/turn terminal或BindingLost。
- Tool/Hook/Backbone presentation event不能直接完成 activity。
- retry 创建新 attempt 与新 client command identity；同 attempt replay复用原 operation。
- wait/gate producer 使用 mailbox + operation evidence恢复，不依赖进程内 callback。

必须测试 accepted 前无 Running、duplicate terminal exactly-once、Lost 映射、restart recovery 与 stale attempt fencing。

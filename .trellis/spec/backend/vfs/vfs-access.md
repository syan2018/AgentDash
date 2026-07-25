# VFS Access and Agent Runtime

VFS mount/access由Project、AgentFrame与resource provider拥有；Agent Runtime只消费Business Surface中已闭包的typed resource/tool contribution。

- `AgentRunRuntimeTarget`提供run/agent授权坐标；`AgentRunRuntimeBinding`提供thread/binding identity；current AgentFrame提供resource revision。
- Cloud不访问本机路径；Local Tool Broker adapter以mount/root ref解析物理路径。
- workspace root 与 mount capability 在 tool side effect 前完成校验；执行权限由独立的 AgentRun permission facade 判定，不投影为 VFS access source。
- absolute local path不进入product/wire identity。
- context materialization按binding/thread、surface digest与resource digest隔离。
- resource browser与Agent tool消费同一final VFS surface。

测试覆盖path escape、unavailable root、read/write/list/search scope、stale surface、remote placement与credential redaction。

# Workflow Activity Lifecycle UI

前端以`LifecycleRunView.orchestrations[]`、node attempt、AgentRun product refs与
`AgentRuntimeView/AgentRuntimeUpdate`展示执行。

- lifecycleStore保存产品编排/subject状态；同一`run_id + agent_id`只建立一个
  `AgentRuntimeConnection`，Feed与Composer共享该连接。
- node evidence显示typed `runtime_thread_id` / `runtime_operation_id`，不作为UI主键。
- running/terminal命令状态取`AgentRuntimeView.execution`与`command_availability`，不从Workflow
  status或presentation event反推。
- artifact/status projection按attempt+operation去重。
- debug trace使用canonical Runtime events/context endpoint。

测试覆盖node与operation关联、Lost/failed/completed、duplicate terminal以及跨Project授权。

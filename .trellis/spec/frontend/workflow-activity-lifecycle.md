# Workflow Activity Lifecycle UI

前端以`LifecycleRunView.orchestrations[]`、node attempt、AgentRun product refs与canonical Runtime snapshot/events展示执行。

- lifecycleStore保存产品编排/subject状态；Runtime feed按`run_id + agent_id`独立读取。
- node evidence显示typed `runtime_thread_id` / `runtime_operation_id`，不作为UI主键。
- running/terminal命令状态取Runtime `command_availability`与terminal event，不从Workflow status反推。
- artifact/status projection按attempt+operation去重。
- debug trace使用canonical Runtime events/context endpoint。

测试覆盖node与operation关联、Lost/failed/completed、duplicate terminal以及跨Project授权。

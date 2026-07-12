# Local Runtime无数据库Host状态重建实施计划

## Phase 1: Incarnation contract

- [ ] 盘点offer/binding/dispatch/lease所有generation校验与relay断连收敛路径。
- [ ] 定义`HostIncarnationId`及其在Runtime Wire、offer、binding和diagnostics中的传递。
- [ ] 保证旧incarnation command在Driver side effect前被拒绝。
- [ ] 补跨进程重启、generation重复与迟到command协议测试。

## Phase 2: Production ephemeral repository

- [ ] 将内存Host repository从测试`Fixture`语义拆为production ephemeral实现与测试构造器。
- [ ] 复用完整instance/offer/binding/lease/coordinate invariants和conformance tests。
- [ ] Local bootstrap创建新incarnation并从definitions/profile重建instances/offers。
- [ ] 明确断连Lost与新incarnation rebind/resume数据流。

## Phase 3: Remove Local PostgreSQL

- [ ] 从`build_ws_config`删除`PostgresRuntime`、全局migration和pool注入。
- [ ] 删除`ws_client::Config._session_db_runtime`及相关lifetime plumbing。
- [ ] 清理`agentdash-local`的PostgreSQL依赖与只服务该路径的代码。
- [ ] 验证既有Local DB目录存在或缺失均不被读取。

## Phase 4: Verification

- [ ] Host repository、incarnation、relay重绑与旧命令隔离定向测试。
- [ ] `cargo check/test/clippy`覆盖Host、Local、Relay、Runtime Wire与Tauri crates。
- [ ] `pnpm dev`验证无postgres子进程、Backend online与AgentRun首轮/后续轮次。
- [ ] `pnpm dev:desktop`验证external cloud形态的embedded runner无数据库启动。
- [ ] Standalone Runner前台和service形态验证无数据库启动、断线重连及重启重绑。
- [ ] 更新Local Runtime、Driver Host、Runtime Wire与发布规范。

## Risk / Review Gates

- 在incarnation fencing通过前不得删除durable generation来源。
- 不使用Local DB兼容读取、双写或自动数据迁移。
- 不把云端Managed Runtime durable repository一并改为内存实现。

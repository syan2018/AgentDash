# 手工验收清单（需真实运行环境）

> 自动化已全绿（cargo application 256 + api 131、app-web 417 tests、contracts/migration/fmt/typecheck/desktop:check）。以下需在真实环境跑，逐条勾。

## A. Desktop Local Runtime
- [ ] 桌面 App 登录后，`/api/local-runtime/ensure` 自动调用、本机 runtime 自动上线。
- [ ] 设置页「运行环境」把这台机器标为「本机（这台设备）」，registration source 显示桌面登录授权。
- [ ] 诊断页 relay 状态正常（connecting → registered），`registration_source=desktop_access_token`。

## B. Standalone Runner 上线
- [ ] Project 设置 → 工作空间 → 运行环境 → 接入新服务器：创建 token，明文只展示一次，可复制 setup 命令。
- [ ] Linux server：执行 setup 命令，runner claim 成功、service 启动、上线。
- [ ] Windows server：同上（SCM service）。
- [ ] 上线后该 runner 在 project 内可见，标为「服务器 runner」，不被渲染成可 restart 的桌面 runtime。

## C. Token 生命周期
- [ ] 撤销 token 后，用该 token 新 claim 被拒（403 revoked）。
- [ ] token 过期后新 claim 被拒（401 expired）。
- [ ] 已领取的 relay credential 行为可解释（已上线 runner 继续用已发的 auth_token，符合设计）。
- [ ] 轮换 token：旧失效、新明文只展示一次。

## D. 多 project 复用（后端模型，UI 完整流程属后续任务）
- [ ] 同一台 runner 机器被第二个 project 接入后，解析为**同一 backend_id**（不新增 backend 记录）。
- [ ] 两个 project 各自通过 ProjectBackendAccess 看到它；撤销其一不影响另一个。
  > 若完整「加入另一 project」UI 尚未落地，可用直接写 grant / 第二个 project 的 token claim 来验证后端模型。

## E. 脱敏
- [ ] 日志、status、doctor、UI 复制内容均不出现 access/registration/relay token 明文。

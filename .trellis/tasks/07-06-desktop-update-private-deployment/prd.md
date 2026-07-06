# 私有仓桌面更新部署适配

## Goal

把主仓产出的桌面自动更新标准发布目录接入企业私有发布流程：私有仓负责把 release artifacts、manifest、签名和 stable latest 指针发布到企业 S3-compatible 对象存储，并向 AgentDash 云端部署环境提供 stable manifest HTTP URL。

本任务只描述私有仓后续需要承接的业务职责和验收口径，不把企业对象存储 endpoint、bucket、AK/SK 或私有域名写入主仓。

## Parent Scope

父任务 `07-06-desktop-auto-update-release` 负责主仓通用能力：

- 生成桌面 updater artifact、人工安装包、signature、sha256。
- 生成对象存储无关的 release manifest、stable latest manifest 和 upload plan。
- 云端服务端运行期读取 stable latest manifest HTTP URL。
- 桌面端只调用 AgentDash 云端 update endpoint。

本子任务承接父任务之外的企业部署适配。

## Confirmed Facts

- 企业对象存储兼容 AWS S3 协议。
- 企业对象存储可通过 Cyberduck、S3 Browser、s3cmd、AWS CLI 或 AWS SDK 访问。
- Linux `s3cmd` 示例使用 `host_base`、`host_bucket`、`use_https` 等配置。
- AWS CLI 示例使用 `--endpoint-url` 访问指定对象存储 endpoint，并通过 `s3 cp --recursive` 上传目录。
- AK/SK、endpoint、bucket、ACL、CDN 或内网访问域名属于私有部署事实，只应存在于私有仓、CI secret 或部署环境中。
- 桌面端和服务端 release 节奏相对独立；私有发布流程更新 stable latest 指针后，不应要求重新构建服务端镜像。

## Requirements

- R1: 私有仓必须读取主仓生成的标准发布目录或 upload plan，并把 versioned artifacts 上传到企业 S3-compatible 对象存储。
- R2: 私有仓必须先上传不可变版本目录，再更新 `channels/stable/latest.json` 指针，避免桌面端读取到指向缺失 artifact 的 latest manifest。
- R3: 私有仓必须注入企业对象存储 endpoint、bucket、prefix、AK/SK、ACL、公开或内网 base URL、并发参数和上传工具选择。
- R4: 私有仓必须校验上传结果，至少确认 stable latest manifest 可通过 HTTP URL 读取，manifest 中引用的 updater artifact URL 可访问。
- R5: 私有仓必须把 stable latest manifest HTTP URL 提供给云端服务端运行环境，例如通过部署变量或环境配置注入。
- R6: 私有仓必须明确 stable 发布审批边界，避免未验收的桌面构建覆盖 stable latest 指针。
- R7: 私有仓必须记录回滚 latest 指针的流程：能够把 `channels/stable/latest.json` 指回上一可用桌面版本。
- R8: 私有仓不得把 AK/SK、真实 endpoint、bucket 或私有域名回写到主仓文档、主仓 manifest schema 或桌面端代码。
- R9: 私有仓可选择 AWS CLI、s3cmd 或 AWS SDK 实现上传，但输出语义必须与主仓 upload plan 保持一致。
- R10: 私有仓必须保留发布日志，能追踪本次发布的 version、git_sha、artifact sha256、stable latest 更新时间、操作者或 CI run。

## Expected Private Flow

```text
1. 从主仓 release job 获取 dist/release/desktop 目录或 upload-plan.json
2. 校验 release manifest、signature、sha256 和 artifact 文件存在
3. 注入企业对象存储 endpoint / bucket / prefix / credentials
4. 上传 releases/desktop/{version}/... 下的不可变版本对象
5. 读取远端对象或比对 sha256，确认 versioned artifacts 可用
6. 上传或覆盖 releases/desktop/channels/stable/latest.json
7. 通过 HTTP URL 拉取 stable latest manifest 并校验引用的 artifact URL
8. 更新云端服务端运行环境中的 desktop stable manifest URL
9. 记录发布结果；必要时执行 latest 指针回滚
```

## Non-Goals

- 不在主仓提交企业对象存储 endpoint、bucket、AK/SK 或私有访问域名。
- 不要求主仓直接调用企业 S3 API 上传。
- 不由私有仓改变主仓 release manifest schema；schema 变更应回到父任务或主仓后续任务。
- 不让桌面端持有对象存储凭据或直接理解企业对象存储配置。

## Acceptance Criteria

- [ ] 私有发布流程能消费主仓标准发布目录或 upload plan。
- [ ] 私有发布流程能上传 versioned desktop artifacts，并在全部 versioned artifacts 可读后更新 stable latest 指针。
- [ ] stable latest manifest HTTP URL 能被云端服务端运行环境访问。
- [ ] stable latest manifest 中引用的 updater artifact URL 可被桌面端下载。
- [ ] 私有发布流程能记录 version、git_sha、artifact sha256、stable latest URL、发布时间和发布执行记录。
- [ ] 私有发布流程支持把 stable latest 指针回滚到上一可用版本。
- [ ] 私有仓文档包含企业对象存储接入说明、CI secret、上传命令、验收命令和排障入口。
- [ ] 主仓、桌面端代码和主仓文档不包含企业真实 endpoint、bucket、AK/SK 或私有域名。

## Open Questions

- 私有仓具体使用 AWS CLI、s3cmd 还是 AWS SDK 作为上传实现，由私有仓根据企业 CI/CD 环境决定。

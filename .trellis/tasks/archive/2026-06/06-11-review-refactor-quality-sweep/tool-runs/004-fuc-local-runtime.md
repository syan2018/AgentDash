# 🌸 屎山代码分析报告 🌸

## 📑 目录

- [糟糕指数](#overall-score)
- [评分指标详情](#metrics-details)
- [最屎代码排行榜](#problem-files)
- [诊断结论](#conclusion)

![Score](https://img.shields.io/badge/Score-81%25-brightgreen)

## 糟糕指数 {#overall-score}

| 指标摘要 | 评分 |
|------|-------|
| **糟糕指数** | **81.27/100** |
| 屎山等级 | 😐 微臭青年 |

> 清新宜人，初闻像早晨的露珠

### 📊 统计信息

| 指标 | 数值 |
|--------|-------|
| 总文件数 | 37 |
| 已跳过 | 0 |
| 耗时 | 310ms |

## 评分指标详情 {#metrics-details}

| 指标摘要 | 评分 | 状态 |
|:-----|------:|:------:|
| 循环复杂度 | 11.88% | ✓✓ |
| 认知复杂度 | 9.36% | ✓✓ |
| 嵌套深度 | 2.57% | ✓✓ |
| 函数长度 | 2.96% | ✓✓ |
| 文件长度 | 6.83% | ✓✓ |
| 参数数量 | 6.01% | ✓✓ |
| 代码重复 | 12.84% | ✓✓ |
| 结构分析 | 3.62% | ✓✓ |
| 错误处理 | 54.25% | • |
| 注释比例 | 80.64% | !! |
| 命名规范 | 0.00% | ✓✓ |

## 最屎代码排行榜 {#problem-files}

### 1. tool_executor.rs

**糟糕指数: 43.90**

**问题**: 🔄 复杂度问题: 9, ⚠️ 其他问题: 2, 📋 重复问题: 5, 🏗️ 结构问题: 9, ❌ 错误处理问题: 17, 📝 注释问题: 1

- 🔄 `run_ripgrep()` L483: 复杂度: 19
- 🔄 `scan_file()` L656: 复杂度: 15
- 🔄 `collect_entries()` L806: 复杂度: 14
- 🔄 `run_ripgrep()` L483: 认知复杂度: 25
- 🔄 `scan_file()` L656: 认知复杂度: 23
- 🔍 ...还有 35 个问题实在太屎，列不完了

### 2. handlers\mod.rs

**糟糕指数: 27.51**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 2, 📝 注释问题: 1

- 🔄 `handle()` L112: 复杂度: 31
- 🔄 `handle()` L112: 认知复杂度: 33
- 📏 `handle()` L112: 113 代码量
- 📏 `new()` L59: 12 参数数量

### 3. handlers\relay_mcp_servers.rs

**糟糕指数: 25.70**

**问题**: 🔄 复杂度问题: 3, ⚠️ 其他问题: 1, 🏗️ 结构问题: 1, ❌ 错误处理问题: 4, 📝 注释问题: 1

- 🔄 `parse_relay_mcp_servers()` L11: 复杂度: 30
- 🔄 `parse_relay_mcp_servers()` L11: 认知复杂度: 38
- 🔄 `parse_relay_mcp_servers()` L11: 嵌套深度: 4
- 📏 `parse_relay_mcp_servers()` L11: 114 代码量
- 🏗️ `parse_relay_mcp_servers()` L11: 中等嵌套: 4
- 🔍 ...还有 4 个问题实在太屎，列不完了

### 4. extensions\host\permissions.rs

**糟糕指数: 24.60**

**问题**: 🔄 复杂度问题: 7, ⚠️ 其他问题: 2, 📋 重复问题: 4, 🏗️ 结构问题: 2, ❌ 错误处理问题: 5, 📝 注释问题: 1

- 🔄 `resolve_host_api()` L22: 复杂度: 14
- 🔄 `resolve_http_fetch()` L271: 复杂度: 13
- 🔄 `require_declared_permission()` L332: 复杂度: 11
- 🔄 `resolve_host_api()` L22: 认知复杂度: 16
- 🔄 `resolve_http_fetch()` L271: 认知复杂度: 17
- 🔍 ...还有 13 个问题实在太屎，列不完了

### 5. handlers\prompt.rs

**糟糕指数: 23.58**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 3, 📋 重复问题: 1, 🏗️ 结构问题: 2, ❌ 错误处理问题: 2, 📝 注释问题: 1

- 🔄 `handle_prompt()` L14: 复杂度: 20
- 🔄 `handle_prompt()` L14: 认知复杂度: 26
- 📏 `handle_prompt()` L14: 175 代码量
- 📋 `handle_cancel()` L190: 重复模式: handle_cancel, handle_steer
- 🏗️ `handle_prompt()` L14: 中等嵌套: 3
- 🔍 ...还有 3 个问题实在太屎，列不完了

### 6. handlers\mcp_relay.rs

**糟糕指数: 21.89**

**问题**: 🔄 复杂度问题: 2, 📋 重复问题: 1, ❌ 错误处理问题: 1, 📝 注释问题: 1

- 🔄 `handle_mcp_probe_transport()` L13: 复杂度: 13
- 🔄 `handle_mcp_probe_transport()` L13: 认知复杂度: 17
- 📋 `handle_mcp_list_tools()` L104: 重复模式: handle_mcp_list_tools, handle_mcp_call_tool, handle_mcp_close
- ❌ L183: 未处理的易出错调用

### 7. extensions\host\tests.rs

**糟糕指数: 20.77**

**问题**: ⚠️ 其他问题: 2, 📋 重复问题: 2, 🏗️ 结构问题: 1, ❌ 错误处理问题: 9, 📝 注释问题: 1

- 📋 `local_hello_profile_executes_in_host()` L10: 重复模式: local_hello_profile_executes_in_host, dependency_alias_invokes_provider_channel_in_same_host, runtime_invoke_calls_loaded_extension_action, runtime_invoke_requires_cross_extension_permission, activation_rejects_handlers_not_declared_by_manifest, activation_rejects_manifest_action_without_handler, built_in_host_apis_use_action_permissions_and_workspace_boundary, action_exception_does_not_stop_host_process
- 📋 `channel_invocation_limits_recursive_calls()` L173: 重复模式: channel_invocation_limits_recursive_calls, runtime_invoke_limits_recursive_calls, permission_denied_when_local_profile_is_not_declared, permission_denied_when_action_local_profile_is_not_declared, top_level_local_profile_summary_does_not_gate_action_call, packaged_directory_verifies_bundle_digest, write_channel_env_package, write_runtime_consumer_package
- 🏗️ L1: 文件过大: 1152 行
- ❌ L559: 未处理的易出错调用
- ❌ L568: 未处理的易出错调用
- 🔍 ...还有 7 个问题实在太屎，列不完了

### 8. materialization.rs

**糟糕指数: 18.03**

**问题**: 🔄 复杂度问题: 3, ⚠️ 其他问题: 1, 📋 重复问题: 4, 🏗️ 结构问题: 2, ❌ 错误处理问题: 7, 📝 注释问题: 1

- 🔄 `prepare_entries()` L203: 复杂度: 11
- 🔄 `materialize()` L66: 认知复杂度: 16
- 🔄 `prepare_entries()` L203: 认知复杂度: 17
- 📋 `base_local_root_for_payload()` L164: 重复模式: base_local_root_for_payload, readable_path_only_adds_suffix_for_real_collision
- 📋 `set_file_mode()` L494: 重复模式: set_file_mode, public_materialization_uses_stable_readable_path_across_plan_ids
- 🔍 ...还有 11 个问题实在太屎，列不完了

### 9. workspace_prepare.rs

**糟糕指数: 14.61**

**问题**: 🔄 复杂度问题: 4, ⚠️ 其他问题: 2, ❌ 错误处理问题: 4, 📝 注释问题: 1

- 🔄 `prepare_git_workspace()` L49: 复杂度: 14
- 🔄 `prepare_p4_workspace()` L106: 复杂度: 14
- 🔄 `prepare_git_workspace()` L49: 认知复杂度: 18
- 🔄 `prepare_p4_workspace()` L106: 认知复杂度: 18
- ❌ L57: 未处理的易出错调用
- 🔍 ...还有 3 个问题实在太屎，列不完了

### 10. terminal_manager.rs

**糟糕指数: 14.10**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 1, 📋 重复问题: 1, 🏗️ 结构问题: 1, ❌ 错误处理问题: 8, 📝 注释问题: 1

- 🔄 `spawn()` L36: 复杂度: 13
- 🔄 `spawn()` L36: 认知复杂度: 19
- 📏 `spawn()` L36: 127 代码量
- 📋 `resolve_terminal_cwd_allows_relative_directory_inside_workspace()` L277: 重复模式: resolve_terminal_cwd_allows_relative_directory_inside_workspace, resolve_terminal_cwd_rejects_directory_outside_workspace
- 🏗️ `spawn()` L36: 中等嵌套: 3
- 🔍 ...还有 8 个问题实在太屎，列不完了

### 11. extensions\host\runner\context.mjs

**糟糕指数: 13.06**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 2, 🏗️ 结构问题: 1, ❌ 错误处理问题: 4, 📝 注释问题: 1

- 🔄 `enforceManifestSurface()` L420: 复杂度: 13
- 🔄 `enforceManifestSurface()` L420: 认知复杂度: 19
- 🏗️ `enforceManifestSurface()` L420: 中等嵌套: 3
- ❌ L89: 未处理的易出错调用
- ❌ L94: 未处理的易出错调用
- 🔍 ...还有 2 个问题实在太屎，列不完了

### 12. runtime.rs

**糟糕指数: 11.95**

**问题**: ⚠️ 其他问题: 2, 🏗️ 结构问题: 1, ❌ 错误处理问题: 6, 📝 注释问题: 1

- 📏 `new()` L36: 6 参数数量
- ❌ L174: 忽略了错误返回值
- ❌ L196: 忽略了错误返回值
- ❌ L245: 忽略了错误返回值
- ❌ L246: 忽略了错误返回值
- 🔍 ...还有 2 个问题实在太屎，列不完了

### 13. ws_client.rs

**糟糕指数: 11.69**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 1, 🏗️ 结构问题: 2, ❌ 错误处理问题: 1, 📝 注释问题: 1

- 🔄 `run_session()` L94: 复杂度: 12
- 🔄 `run_session()` L94: 认知复杂度: 18
- 📏 `run_session()` L94: 133 代码量
- 🏗️ `run_until_shutdown()` L48: 中等嵌套: 3
- 🏗️ `run_session()` L94: 中等嵌套: 3
- 🔍 ...还有 1 个问题实在太屎，列不完了

### 14. local_backend_config.rs

**糟糕指数: 11.57**

**问题**: 📋 重复问题: 1, ❌ 错误处理问题: 1, 📝 注释问题: 1

- 📋 `save_local_backend_config_for_root()` L104: 重复模式: save_local_backend_config_for_root, save_and_load_local_backend_config_round_trips_mcp_servers
- ❌ L113: 未处理的易出错调用

### 15. mcp_client_manager.rs

**糟糕指数: 10.40**

**问题**: 📋 重复问题: 1, ❌ 错误处理问题: 3, 📝 注释问题: 1

- 📋 `call_tool()` L69: 重复模式: call_tool, close
- ❌ L50: 未处理的易出错调用
- ❌ L79: 未处理的易出错调用
- ❌ L107: 未处理的易出错调用

## 诊断结论 {#conclusion}

🌸 **微臭青年** - 略有异味，建议适量通风

👍 继续保持，你是编码界的一股清流，代码洁癖者的骄傲

---

*由 [fuck-u-code](https://github.com/Done-0/fuck-u-code) 生成*
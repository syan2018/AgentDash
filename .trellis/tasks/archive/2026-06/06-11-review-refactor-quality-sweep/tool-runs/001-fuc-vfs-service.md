# 🌸 屎山代码分析报告 🌸

## 📑 目录

- [糟糕指数](#overall-score)
- [评分指标详情](#metrics-details)
- [最屎代码排行榜](#problem-files)
- [诊断结论](#conclusion)

![Score](https://img.shields.io/badge/Score-77%25-green)

## 糟糕指数 {#overall-score}

| 指标摘要 | 评分 |
|------|-------|
| **糟糕指数** | **77.20/100** |
| 屎山等级 | 😐 微臭青年 |

> 略带清香，偶尔飘过一丝酸爽

### 📊 统计信息

| 指标 | 数值 |
|--------|-------|
| 总文件数 | 31 |
| 已跳过 | 0 |
| 耗时 | 358ms |

## 评分指标详情 {#metrics-details}

| 指标摘要 | 评分 | 状态 |
|:-----|------:|:------:|
| 循环复杂度 | 17.79% | ✓✓ |
| 认知复杂度 | 14.94% | ✓✓ |
| 嵌套深度 | 9.03% | ✓✓ |
| 函数长度 | 4.97% | ✓✓ |
| 文件长度 | 15.63% | ✓✓ |
| 参数数量 | 10.16% | ✓✓ |
| 代码重复 | 7.00% | ✓✓ |
| 结构分析 | 8.84% | ✓✓ |
| 错误处理 | 69.22% | ⚠ |
| 注释比例 | 64.85% | ⚠ |
| 命名规范 | 0.00% | ✓✓ |

## 最屎代码排行榜 {#problem-files}

### 1. service.rs

**糟糕指数: 46.44**

**问题**: 🔄 复杂度问题: 7, ⚠️ 其他问题: 7, 📋 重复问题: 3, 🏗️ 结构问题: 4, ❌ 错误处理问题: 16, 📝 注释问题: 1

- 🔄 `stat()` L520: 复杂度: 12
- 🔄 `apply_patch_multi()` L679: 复杂度: 14
- 🔄 `grep_inline()` L1087: 复杂度: 26
- 🔄 `stat()` L520: 认知复杂度: 16
- 🔄 `apply_patch_multi()` L679: 认知复杂度: 20
- 🔍 ...还有 31 个问题实在太屎，列不完了

### 2. tools\provider.rs

**糟糕指数: 37.32**

**问题**: 🔄 复杂度问题: 3, ⚠️ 其他问题: 3, 🏗️ 结构问题: 1, ❌ 错误处理问题: 5, 📝 注释问题: 1

- 🔄 `build_tools()` L160: 复杂度: 35
- 🔄 `build_tools()` L160: 认知复杂度: 45
- 🔄 `build_tools()` L160: 嵌套深度: 5
- 📏 `build_tools()` L160: 315 代码量
- 📏 `new()` L71: 6 参数数量
- 🔍 ...还有 6 个问题实在太屎，列不完了

### 3. mount.rs

**糟糕指数: 32.23**

**问题**: 🔄 复杂度问题: 10, ⚠️ 其他问题: 3, 📋 重复问题: 4, 🏗️ 结构问题: 5, ❌ 错误处理问题: 11, 📝 注释问题: 1

- 🔄 `build_derived_vfs()` L59: 复杂度: 18
- 🔄 `mount_owner_kind()` L489: 复杂度: 16
- 🔄 `mount_purpose()` L535: 复杂度: 14
- 🔄 `list_inline_entries()` L1019: 复杂度: 18
- 🔄 `build_derived_vfs()` L59: 认知复杂度: 24
- 🔍 ...还有 27 个问题实在太屎，列不完了

### 4. apply_patch.rs

**糟糕指数: 30.33**

**问题**: 🔄 复杂度问题: 12, ⚠️ 其他问题: 2, 🏗️ 结构问题: 9, ❌ 错误处理问题: 17, 📝 注释问题: 1

- 🔄 `apply_entries_to_target()` L208: 复杂度: 11
- 🔄 `check_patch_boundaries()` L349: 复杂度: 14
- 🔄 `parse_one_entry()` L391: 复杂度: 13
- 🔄 `parse_update_file_chunk()` L486: 复杂度: 15
- 🔄 `seek_sequence()` L669: 复杂度: 12
- 🔍 ...还有 33 个问题实在太屎，列不完了

### 5. provider_skill_asset.rs

**糟糕指数: 28.33**

**问题**: 🔄 复杂度问题: 6, ⚠️ 其他问题: 2, 📋 重复问题: 2, 🏗️ 结构问题: 3, ❌ 错误处理问题: 11, 📝 注释问题: 1

- 🔄 `list_projected_entries()` L197: 复杂度: 18
- 🔄 `search_projected_skill_files()` L319: 复杂度: 11
- 🔄 `list_projected_entries()` L197: 认知复杂度: 26
- 🔄 `search_projected_skill_files()` L319: 认知复杂度: 19
- 🔄 `list_projected_entries()` L197: 嵌套深度: 4
- 🔍 ...还有 17 个问题实在太屎，列不完了

### 6. provider_lifecycle.rs

**糟糕指数: 24.63**

**问题**: 🔄 复杂度问题: 5, ⚠️ 其他问题: 3, 🏗️ 结构问题: 3, ❌ 错误处理问题: 4, 📝 注释问题: 1

- 🔄 `read_text()` L304: 复杂度: 22
- 🔄 `list()` L511: 复杂度: 19
- 🔄 `read_text()` L304: 认知复杂度: 28
- 🔄 `list()` L511: 认知复杂度: 23
- 🔄 `search_text()` L608: 认知复杂度: 16
- 🔍 ...还有 8 个问题实在太屎，列不完了

### 7. provider_inline.rs

**糟糕指数: 23.03**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 2, 📋 重复问题: 3, 🏗️ 结构问题: 3, ❌ 错误处理问题: 5, 📝 注释问题: 1

- 🔄 `grep_text()` L201: 复杂度: 15
- 🔄 `grep_text()` L201: 认知复杂度: 21
- 📋 `list_exposes_binary_metadata()` L511: 重复模式: list_exposes_binary_metadata, search_truncated_when_max_results_reached, read_text_range_default_impl_slices_lines
- 📋 `read_text_rejects_binary_and_search_skips_it()` L563: 重复模式: read_text_rejects_binary_and_search_skips_it, grep_text_supports_regex_and_context_lines
- 📋 `read_binary_returns_bytes_and_metadata()` L613: 重复模式: read_binary_returns_bytes_and_metadata, read_text_returns_version_token_and_modified_at, read_text_range_default_impl_limit_none_reads_to_eof, search_text_substring_does_not_treat_pattern_as_regex
- 🔍 ...还有 8 个问题实在太屎，列不完了

### 8. tools\fs\read.rs

**糟糕指数: 22.87**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 3, 📋 重复问题: 2, 🏗️ 结构问题: 1, ❌ 错误处理问题: 28, 📝 注释问题: 1

- 🔄 `execute()` L122: 复杂度: 17
- 🔄 `execute()` L122: 认知复杂度: 21
- 📏 `execute()` L122: 109 代码量
- 📋 `unchanged_stub_result()` L265: 重复模式: unchanged_stub_result, fs_read_too_many_lines_is_error, fs_read_dedup_returns_unchanged_stub_on_repeat, fs_read_dedup_invalidates_when_token_changes
- 📋 `list()` L532: 重复模式: list, stat, fs_read_image_returns_image_block, fs_read_too_large_bytes_is_error
- 🔍 ...还有 28 个问题实在太屎，列不完了

### 9. tools\fs\shell.rs

**糟糕指数: 22.40**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 3, ❌ 错误处理问题: 2, 📝 注释问题: 1

- 🔄 `execute()` L102: 复杂度: 18
- 🔄 `execute()` L102: 认知复杂度: 22
- 📏 `execute()` L102: 138 代码量
- 📏 `shell_exec_result_text()` L296: 7 参数数量
- ❌ L102: 未处理的易出错调用
- 🔍 ...还有 1 个问题实在太屎，列不完了

### 10. tools\fs\grep.rs

**糟糕指数: 19.85**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 3, 🏗️ 结构问题: 2, ❌ 错误处理问题: 13, 📝 注释问题: 1

- 🔄 `execute()` L142: 复杂度: 18
- 🔄 `execute()` L142: 认知复杂度: 24
- 📏 `execute()` L142: 108 代码量
- 🏗️ `execute()` L142: 中等嵌套: 3
- 🏗️ `format_content()` L253: 中等嵌套: 3
- 🔍 ...还有 13 个问题实在太屎，列不完了

### 11. surface.rs

**糟糕指数: 17.45**

**问题**: 🔄 复杂度问题: 2, 📝 注释问题: 1

- 🔄 `parse_surface_ref()` L78: 复杂度: 30
- 🔄 `parse_surface_ref()` L78: 认知复杂度: 34

### 12. path.rs

**糟糕指数: 15.86**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 1, 🏗️ 结构问题: 4, ❌ 错误处理问题: 13, 📝 注释问题: 1

- 🔄 `normalize_mount_relative_path()` L298: 认知复杂度: 16
- 🔄 `resolve_links()` L200: 嵌套深度: 4
- 🏗️ `parse()` L121: 中等嵌套: 3
- 🏗️ `resolve_links()` L200: 中等嵌套: 4
- 🏗️ `resolve_mount_id()` L262: 中等嵌套: 3
- 🔍 ...还有 14 个问题实在太屎，列不完了

### 13. mutation_dispatcher.rs

**糟糕指数: 14.78**

**问题**: ⚠️ 其他问题: 2, 📋 重复问题: 2, 🏗️ 结构问题: 1, ❌ 错误处理问题: 3, 📝 注释问题: 1

- 📋 `write_text()` L148: 重复模式: write_text, delete_text
- 📋 `ensure_edit_capability()` L343: 重复模式: ensure_edit_capability, dispatcher_mutates_inline_files_through_one_storage_key
- 🏗️ `ensure_edit_capability()` L343: 中等嵌套: 3
- ❌ L352: 未处理的易出错调用
- ❌ L449: 未处理的易出错调用
- 🔍 ...还有 1 个问题实在太屎，列不完了

### 14. provider_routine.rs

**糟糕指数: 14.43**

**问题**: 🔄 复杂度问题: 1, ⚠️ 其他问题: 2, 🏗️ 结构问题: 2, ❌ 错误处理问题: 5, 📝 注释问题: 1

- 🔄 `search_text()` L167: 认知复杂度: 16
- 🏗️ `search_text()` L167: 中等嵌套: 3
- 🏗️ `decode_path_segment()` L446: 中等嵌套: 3
- ❌ L225: 未处理的易出错调用
- ❌ L250: 未处理的易出错调用
- 🔍 ...还有 3 个问题实在太屎，列不完了

### 15. tools\fs\glob.rs

**糟糕指数: 12.95**

**问题**: 🔄 复杂度问题: 2, ⚠️ 其他问题: 2, 🏗️ 结构问题: 1, ❌ 错误处理问题: 12, 📝 注释问题: 1

- 🔄 `execute()` L77: 复杂度: 12
- 🔄 `execute()` L77: 认知复杂度: 18
- 🏗️ `execute()` L77: 中等嵌套: 3
- ❌ L77: 未处理的易出错调用
- ❌ L334: 未处理的易出错调用
- 🔍 ...还有 10 个问题实在太屎，列不完了

## 诊断结论 {#conclusion}

🌸 **微臭青年** - 略有异味，建议适量通风

👍 继续保持，你是编码界的一股清流，代码洁癖者的骄傲

---

*由 [fuck-u-code](https://github.com/Done-0/fuck-u-code) 生成*
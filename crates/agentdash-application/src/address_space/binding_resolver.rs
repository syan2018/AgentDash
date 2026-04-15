//! 统一 context_bindings locator 解析器。
//!
//! 遍历 `WorkflowContextBinding` 列表，对每个 locator 调用 `parse_mount_uri → read_text`，
//! 将结果转换为可注入 session 的文本片段。所有 locator 走相同的 VFS read 链路。

use agentdash_domain::workflow::WorkflowContextBinding;

use super::path::parse_mount_uri;
use super::relay_service::RelayAddressSpaceService;
use crate::runtime::AddressSpace;

/// 单个 binding 的解析结果
#[derive(Debug, Clone)]
pub struct ResolvedBinding {
    pub locator: String,
    pub title: Option<String>,
    pub reason: String,
    pub content: String,
}

/// 解析输出
#[derive(Debug, Clone, Default)]
pub struct ResolveBindingsOutput {
    pub resolved: Vec<ResolvedBinding>,
    pub warnings: Vec<String>,
}

/// 解析 context_bindings 中的 locator，通过 VFS read 获取内容。
///
/// 对每个 binding：
/// - 解析 locator 为 mount_id + path
/// - 调用 address_space_service.read_text
/// - 成功 → 加入 resolved
/// - 失败 + required → 返回 Err
/// - 失败 + !required → 记录 warning 跳过
pub async fn resolve_context_bindings(
    bindings: &[WorkflowContextBinding],
    address_space: &AddressSpace,
    service: &RelayAddressSpaceService,
) -> Result<ResolveBindingsOutput, String> {
    if bindings.is_empty() {
        return Ok(ResolveBindingsOutput::default());
    }

    let mut output = ResolveBindingsOutput::default();

    for binding in bindings {
        let locator = binding.locator.trim();
        if locator.is_empty() {
            continue;
        }

        let resource_ref = match parse_mount_uri(locator, address_space) {
            Ok(r) => r,
            Err(err) => {
                if binding.required {
                    return Err(format!(
                        "context_binding locator 解析失败 (required): `{}` — {err}",
                        locator
                    ));
                }
                output.warnings.push(format!(
                    "context_binding `{}` 已跳过: locator 解析失败 — {err}",
                    locator
                ));
                continue;
            }
        };

        let read_result = service
            .read_text(
                address_space,
                &resource_ref,
                None, // overlay
                None, // identity
            )
            .await;

        match read_result {
            Ok(result) => {
                output.resolved.push(ResolvedBinding {
                    locator: locator.to_string(),
                    title: binding.title.clone(),
                    reason: binding.reason.clone(),
                    content: result.content,
                });
            }
            Err(err) => {
                if binding.required {
                    return Err(format!(
                        "context_binding 读取失败 (required): `{}` — {err}",
                        locator
                    ));
                }
                output
                    .warnings
                    .push(format!("context_binding `{}` 已跳过: {err}", locator));
            }
        }
    }

    Ok(output)
}

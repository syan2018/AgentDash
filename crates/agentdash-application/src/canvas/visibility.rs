use std::collections::BTreeSet;

use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::canvas::CanvasRepository;
use agentdash_spi::AddressSpace;

use crate::address_space::append_canvas_mounts;

/// 根据会话显式声明的 canvas mount_id 列表，向 address space 追加可见 canvas。
///
/// 注意：默认不注入任何 canvas。只有会话里记录过的 mount_id 才会被追加。
pub async fn append_visible_canvas_mounts(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    address_space: &mut AddressSpace,
    visible_mount_ids: &[String],
) -> Result<(), DomainError> {
    let selected = visible_mount_ids
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    if selected.is_empty() {
        return Ok(());
    }

    let canvases = canvas_repo.list_by_project(project_id).await?;
    let visible = canvases
        .into_iter()
        .filter(|canvas| selected.contains(canvas.mount_id.as_str()))
        .collect::<Vec<_>>();
    append_canvas_mounts(address_space, &visible);
    Ok(())
}

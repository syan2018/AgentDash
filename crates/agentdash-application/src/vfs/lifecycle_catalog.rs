use crate::runtime::RuntimeFileEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePathKind {
    File,
    Dir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecyclePathEntry {
    pub path: &'static str,
    pub description: &'static str,
    pub kind: LifecyclePathKind,
    pub virtual_entry: bool,
}

const DIRECTORY_HINT_DESCRIPTION: &str = "Lifecycle journey VFS，包含当前 node/session 投影、tool call 索引、port 产出和可写 records overlay";

pub const LIFECYCLE_PATH_CATALOG: &[LifecyclePathEntry] = &[
    LifecyclePathEntry {
        path: "active",
        description: "当前活跃 run 的概览（JSON）",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "active/steps",
        description: "各步骤执行状态，子路径为 step_key",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "active/steps/{step_key}",
        description: "单步骤详情（JSON）",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "active/artifacts",
        description: "Port output 产出别名，指向 artifacts",
        kind: LifecyclePathKind::Dir,
        virtual_entry: false,
    },
    LifecyclePathEntry {
        path: "active/log",
        description: "执行日志（JSON 数组）",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "artifacts",
        description: "Port output 产出，子路径为 port_key",
        kind: LifecyclePathKind::Dir,
        virtual_entry: false,
    },
    LifecyclePathEntry {
        path: "artifacts/{port_key}",
        description: "指定 port 的产出内容（纯文本；写入受 writable_port_keys 限制）",
        kind: LifecyclePathKind::File,
        virtual_entry: false,
    },
    LifecyclePathEntry {
        path: "state",
        description: "当前 node 步骤状态（JSON）",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/meta",
        description: "当前 node 关联 session 元信息",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/summary",
        description: "当前 node session 摘要",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/conclusions",
        description: "当前 node session 结论",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/events.json",
        description: "当前 node session 原始事件投影",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/terminal",
        description: "当前 node session 终端输出聚合",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/turns",
        description: "当前 node session turn 列表",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/turns/{turn_id}/events.json",
        description: "当前 node 单 turn 原始事件投影",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "tool-calls",
        description: "当前 node session 的 tool call 索引；MCP 也是 tool call",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "tool-calls/{tool_call_id}/raw.json",
        description: "指定 tool call 的原始事件投影",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "tool-calls/{tool_call_id}/request.json",
        description: "指定 tool call 的请求结构",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "tool-calls/{tool_call_id}/result.json",
        description: "指定 tool call 的结果结构",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "tool-calls/{tool_call_id}/stdout.txt",
        description: "指定 tool call 的输出文本",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "writes",
        description: "当前 node session 的写入类 tool call 索引",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "records/{name}",
        description: "当前 node 的可写 journey record overlay",
        kind: LifecyclePathKind::File,
        virtual_entry: false,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/state",
        description: "Node 步骤状态（JSON）",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/meta",
        description: "Node 关联 session 元信息",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/summary",
        description: "Node session 摘要",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/conclusions",
        description: "Node session 结论",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/events.json",
        description: "指定 node session 原始事件投影",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/terminal",
        description: "指定 node session 终端输出聚合",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/turns",
        description: "Node session turn 列表",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/turns/{turn_id}/events.json",
        description: "指定 node 单 turn 原始事件投影",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/tool-calls",
        description: "指定 node session 的 tool call 索引",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/session/writes",
        description: "指定 node session 的写入类 tool call 索引",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "nodes/{step_key}/records/{name}",
        description: "指定 node 的可写 journey record overlay",
        kind: LifecyclePathKind::File,
        virtual_entry: false,
    },
    LifecyclePathEntry {
        path: "runs",
        description: "历史 run 列表",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
];

pub fn lifecycle_directory_hint() -> serde_json::Value {
    serde_json::json!({
        "description": DIRECTORY_HINT_DESCRIPTION,
        "index": LIFECYCLE_PATH_CATALOG
            .iter()
            .map(|entry| serde_json::json!({
                "path": entry.path,
                "description": entry.description,
            }))
            .collect::<Vec<_>>()
    })
}

pub fn lifecycle_root_entries(include_skills: bool) -> Vec<RuntimeFileEntry> {
    let mut entries = vec![
        RuntimeFileEntry::dir("active").as_virtual(),
        RuntimeFileEntry::dir("artifacts"),
        RuntimeFileEntry::file("state").as_virtual(),
        RuntimeFileEntry::dir("session").as_virtual(),
        RuntimeFileEntry::dir("tool-calls").as_virtual(),
        RuntimeFileEntry::file("writes").as_virtual(),
        RuntimeFileEntry::dir("records"),
        RuntimeFileEntry::dir("nodes").as_virtual(),
        RuntimeFileEntry::dir("runs").as_virtual(),
    ];
    if include_skills {
        entries.push(RuntimeFileEntry::dir("skills").as_virtual());
    }
    entries
}

pub fn lifecycle_active_entries(active_log_size: u64) -> Vec<RuntimeFileEntry> {
    vec![
        RuntimeFileEntry::dir("active/steps").as_virtual(),
        RuntimeFileEntry::dir("active/artifacts"),
        RuntimeFileEntry::file("active/log")
            .with_size(active_log_size)
            .as_virtual(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_hint_is_generated_from_catalog() {
        let hint = lifecycle_directory_hint();
        let paths = hint
            .get("index")
            .and_then(|value| value.as_array())
            .expect("index")
            .iter()
            .filter_map(|value| value.get("path").and_then(|path| path.as_str()))
            .collect::<Vec<_>>();

        assert!(paths.contains(&"session/conclusions"));
        assert!(paths.contains(&"nodes/{step_key}/session/conclusions"));
        assert_eq!(paths.len(), LIFECYCLE_PATH_CATALOG.len());
    }

    #[test]
    fn root_entries_share_catalog_surface_names() {
        let entries = lifecycle_root_entries(true)
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();

        assert!(entries.contains(&"active".to_string()));
        assert!(entries.contains(&"skills".to_string()));
        assert!(entries.contains(&"runs".to_string()));
    }
}

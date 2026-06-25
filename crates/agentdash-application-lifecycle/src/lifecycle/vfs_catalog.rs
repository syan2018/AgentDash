use agentdash_spi::platform::mount::RuntimeFileEntry;

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

const DIRECTORY_HINT_DESCRIPTION: &str = "Lifecycle journey VFS，包含 AgentRun delivery session 日志、消息、工具执行记录，以及当前 runtime node 的 artifact / record 投影";

pub const LIFECYCLE_PATH_CATALOG: &[LifecyclePathEntry] = &[
    LifecyclePathEntry {
        path: "state",
        description: "AgentRun delivery session anchor 与 run 状态概览（JSON）",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "execution-log",
        description: "当前 LifecycleRun execution log（JSON）",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/meta",
        description: "AgentRun delivery session 元信息",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/summary",
        description: "AgentRun delivery session 摘要",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/conclusions",
        description: "AgentRun delivery session 结论",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/events.json",
        description: "AgentRun delivery session 原始事件投影",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/items",
        description: "AgentRun delivery session 全量 item 索引",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/items/{item_file}",
        description: "指定 session item 的完整投影",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/messages",
        description: "AgentRun delivery session 用户与 Agent 消息索引",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/messages/{message_file}",
        description: "指定用户或 Agent 消息原文",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/tools",
        description: "AgentRun delivery session 工具 ThreadItem 索引",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/tools/{tool_file}",
        description: "指定工具原始 ThreadItem JSON",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/writes",
        description: "AgentRun delivery session 成功写入类工具 ThreadItem 索引",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/writes/{write_file}",
        description: "指定成功写入类工具原始 ThreadItem JSON",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/summaries",
        description: "AgentRun delivery session 每轮上下文压缩摘要留档",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/summaries/{summary_file}",
        description: "指定上下文压缩摘要 markdown",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/terminal",
        description: "AgentRun delivery session 终端输出聚合",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/turns",
        description: "AgentRun delivery session turn 列表",
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
        path: "node/state",
        description: "当前 anchored runtime node 状态（JSON）",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "node/artifacts",
        description: "当前 anchored runtime node 的 port output 产出",
        kind: LifecyclePathKind::Dir,
        virtual_entry: false,
    },
    LifecyclePathEntry {
        path: "node/artifacts/{port_key}",
        description: "当前 anchored runtime node 指定 port 的产出内容",
        kind: LifecyclePathKind::File,
        virtual_entry: false,
    },
    LifecyclePathEntry {
        path: "node/records/{name}",
        description: "当前 anchored runtime node 的 journey record overlay",
        kind: LifecyclePathKind::File,
        virtual_entry: false,
    },
    LifecyclePathEntry {
        path: "orchestration/state",
        description: "当前 anchored orchestration 实例状态（JSON）",
        kind: LifecyclePathKind::File,
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
        RuntimeFileEntry::file("state").as_virtual(),
        RuntimeFileEntry::dir("session").as_virtual(),
        RuntimeFileEntry::dir("artifacts"),
        RuntimeFileEntry::dir("records"),
    ];
    if include_skills {
        entries.push(RuntimeFileEntry::dir("skills").as_virtual());
    }
    entries
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
        assert!(paths.contains(&"state"));
        assert!(paths.contains(&"execution-log"));
        assert!(paths.contains(&"session/items"));
        assert!(paths.contains(&"session/summaries"));
        assert!(paths.contains(&"session/messages"));
        assert!(paths.contains(&"session/tools"));
        assert!(paths.contains(&"node/state"));
        assert!(paths.contains(&"node/artifacts/{port_key}"));
        assert!(!paths.contains(&"runs"));
        assert_eq!(paths.len(), LIFECYCLE_PATH_CATALOG.len());
    }

    #[test]
    fn root_entries_share_catalog_surface_names() {
        let entries = lifecycle_root_entries(true)
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();

        assert!(entries.contains(&"state".to_string()));
        assert!(entries.contains(&"session".to_string()));
        assert!(entries.contains(&"artifacts".to_string()));
        assert!(entries.contains(&"records".to_string()));
        assert!(entries.contains(&"skills".to_string()));
        assert!(!entries.contains(&"runs".to_string()));
        assert!(!entries.contains(&"nodes".to_string()));
    }
}

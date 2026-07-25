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

const DIRECTORY_HINT_DESCRIPTION: &str = "Lifecycle journey VFS，包含从 canonical Agent history 重建的只读消息、工具与压缩记录，以及当前 runtime node 的 artifact / record 投影";

pub const LIFECYCLE_PATH_CATALOG: &[LifecyclePathEntry] = &[
    LifecyclePathEntry {
        path: "state",
        description: "AgentRun canonical history anchor 与 run 状态概览（JSON）",
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
        description: "AgentRun canonical history 元信息",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/summary",
        description: "AgentRun canonical history 摘要",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/conclusions",
        description: "AgentRun canonical history 结论",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/events.json",
        description: "AgentRun canonical history 完整投影",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/items",
        description: "AgentRun canonical history 全量 item 索引",
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
        description: "AgentRun canonical history 用户与 Agent 消息索引",
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
        description: "AgentRun canonical history 工具 item 索引",
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
        description: "AgentRun canonical history 文件变更 item 索引",
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
        description: "AgentRun canonical history 上下文压缩 item 索引",
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
        description: "AgentRun canonical history 中的终端控制 item（不包含独立 PTY feed）",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "session/turns",
        description: "AgentRun canonical history turn 列表",
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
        path: "agent-runs",
        description: "当前 Lifecycle 下 AgentRun 证据入口索引",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "agent-runs/{agent_id}",
        description: "指定 AgentRun 的证据目录",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "agent-runs/{agent_id}/sessions",
        description: "指定 AgentRun 的 canonical history 投影入口",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "agent-runs/{agent_id}/sessions/messages",
        description: "指定 AgentRun 的用户与 Agent 消息索引",
        kind: LifecyclePathKind::Dir,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "agent-runs/{agent_id}/sessions/messages/{message_file}",
        description: "指定 AgentRun 的用户或 Agent 消息原文",
        kind: LifecyclePathKind::File,
        virtual_entry: true,
    },
    LifecyclePathEntry {
        path: "agent-runs/{agent_id}/sessions/events.json",
        description: "指定 AgentRun 的 canonical history 完整投影",
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
        RuntimeFileEntry::dir("agent-runs").as_virtual(),
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
        assert!(paths.contains(&"agent-runs"));
        assert!(paths.contains(&"agent-runs/{agent_id}/sessions/messages"));
        assert!(paths.contains(&"node/state"));
        assert!(paths.contains(&"node/artifacts/{port_key}"));
        assert!(!paths.contains(&"runs"));
        assert_eq!(paths.len(), LIFECYCLE_PATH_CATALOG.len());
    }
}
use agentdash_platform_spi::platform::mount::RuntimeFileEntry;

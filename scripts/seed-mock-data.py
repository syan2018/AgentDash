#!/usr/bin/env python3
"""
AgentDashboard Mock 数据生成脚本

用途：
  - 生成开发/测试用的 mock 数据（Project、Workspace、Backend、Story、Task）
  - 直接操作 SQLite 数据库，避免 HTTP 编码问题
  - 统一维护数据结构，方便后续扩展

用法：
  python scripts/seed-mock-data.py              # 默认使用 ./agentdash.db
  python scripts/seed-mock-data.py --db path    # 指定数据库路径
  python scripts/seed-mock-data.py --clean      # 清空后重新生成
"""

import argparse
import json
import sqlite3
import uuid
from datetime import datetime, timezone


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# ─── Mock 数据定义 ─────────────────────────────────────────

BACKEND = {
    "id": "local-dev",
    "name": "本地开发环境",
    "endpoint": "http://localhost:3001",
    "auth_token": None,
    "enabled": 1,
    "backend_type": "local",
}

PROJECT = {
    "id": str(uuid.uuid5(uuid.NAMESPACE_DNS, "agentdash-dev-project")),
    "name": "AgentDash 开发",
    "description": "AgentDash 看板系统核心功能开发",
    "backend_id": "local-dev",
    "config": {
        "default_agent_type": "claude-code",
        "default_workspace_id": None,
        "agent_presets": [
            {"name": "claude-code-default", "agent_type": "claude-code", "config": {}},
            {"name": "codex-review", "agent_type": "codex", "config": {"mode": "review"}},
        ],
    },
}

WORKSPACES = [
    {
        "name": "主仓库工作区",
        "container_ref": "/workspace/agentdash-main",
        "workspace_type": "static",
        "status": "ready",
        "git_config": None,
    },
    {
        "name": "前端特性分支",
        "container_ref": "/workspace/agentdash-feature-ui",
        "workspace_type": "git_worktree",
        "status": "active",
        "git_config": {
            "source_repo": "/repos/agentdash",
            "branch": "feature/new-ui",
            "commit_hash": None,
        },
    },
    {
        "name": "临时调试环境",
        "container_ref": "/tmp/agentdash-debug",
        "workspace_type": "ephemeral",
        "status": "pending",
        "git_config": None,
    },
]

STORIES = [
    {
        "title": "用户认证模块开发",
        "description": "实现 JWT 认证、登录注册、权限管理",
        "status": "executing",
        "context": {
            "prd_doc": "docs/auth-spec.md",
            "spec_refs": ["docs/api-design.md", "docs/auth-spec.md"],
            "resource_list": [
                {"name": "认证规范文档", "uri": "docs/auth-spec.md", "resource_type": "spec"},
                {"name": "API 设计文档", "uri": "docs/api-design.md", "resource_type": "content"},
            ],
        },
        "tasks": [
            {
                "title": "设计 JWT Token 结构",
                "description": "定义 Token payload、过期策略和签名算法",
                "status": "completed",
                "workspace_index": 0,
                "agent_binding": {"agent_type": "claude-code", "agent_pid": None, "preset_name": "claude-code-default"},
            },
            {
                "title": "实现登录 API",
                "description": "POST /api/auth/login，支持邮箱+密码登录",
                "status": "running",
                "workspace_index": 0,
                "agent_binding": {"agent_type": "claude-code", "agent_pid": "proc-12345", "preset_name": "claude-code-default"},
            },
            {
                "title": "实现注册 API",
                "description": "POST /api/auth/register，支持邮箱注册+验证",
                "status": "pending",
                "workspace_index": None,
                "agent_binding": {"agent_type": None, "agent_pid": None, "preset_name": None},
            },
        ],
    },
    {
        "title": "Story 看板视图优化",
        "description": "支持拖拽排序、状态筛选、搜索功能",
        "status": "executing",
        "context": {
            "prd_doc": None,
            "spec_refs": ["docs/kanban-ux.md"],
            "resource_list": [
                {"name": "看板 UX 设计稿", "uri": "docs/kanban-ux.md", "resource_type": "content"},
            ],
        },
        "tasks": [
            {
                "title": "看板拖拽排序",
                "description": "基于 dnd-kit 实现 Story 卡片拖拽排序",
                "status": "running",
                "workspace_index": 1,
                "agent_binding": {"agent_type": "claude-code", "agent_pid": "proc-67890", "preset_name": "claude-code-default"},
            },
            {
                "title": "状态筛选器",
                "description": "按 Story 状态筛选看板列",
                "status": "pending",
                "workspace_index": None,
                "agent_binding": {"agent_type": None, "agent_pid": None, "preset_name": None},
            },
        ],
    },
    {
        "title": "实时事件推送系统",
        "description": "基于 SSE 的状态变更实时推送，支持断线重连",
        "status": "failed",
        "context": {"prd_doc": None, "spec_refs": [], "resource_list": []},
        "tasks": [
            {
                "title": "SSE 推送服务端实现",
                "description": "Axum SSE endpoint + broadcast channel",
                "status": "completed",
                "workspace_index": 0,
                "agent_binding": {"agent_type": "claude-code", "agent_pid": None, "preset_name": "claude-code-default"},
            },
            {
                "title": "客户端断线重连",
                "description": "EventSource 自动重连 + Resume 机制",
                "status": "failed",
                "workspace_index": 0,
                "agent_binding": {"agent_type": "claude-code", "agent_pid": None, "preset_name": "claude-code-default"},
            },
        ],
    },
    {
        "title": "Agent 执行引擎集成",
        "description": "对接 Claude Code / Codex，管理 Agent 生命周期",
        "status": "created",
        "context": {"prd_doc": None, "spec_refs": [], "resource_list": []},
        "tasks": [
            {
                "title": "Claude Code 适配器",
                "description": "实现 ACP 协议适配层，对接 Claude Code CLI",
                "status": "pending",
                "workspace_index": None,
                "agent_binding": {"agent_type": None, "agent_pid": None, "preset_name": None},
            },
            {
                "title": "Agent 生命周期管理",
                "description": "启动、监控、停止 Agent 进程的统一接口",
                "status": "pending",
                "workspace_index": None,
                "agent_binding": {"agent_type": None, "agent_pid": None, "preset_name": None},
            },
        ],
    },
    {
        "title": "多后端连接管理",
        "description": "支持同时连接多个远程后端，统一视图展示",
        "status": "created",
        "context": {"prd_doc": None, "spec_refs": [], "resource_list": []},
        "tasks": [
            {
                "title": "后端连接池管理",
                "description": "管理多个后端 WebSocket/HTTP 连接的生命周期",
                "status": "assigned",
                "workspace_index": None,
                "agent_binding": {"agent_type": "codex", "agent_pid": None, "preset_name": "codex-review"},
            },
            {
                "title": "连接状态监控",
                "description": "心跳检测、断线告警、自动重连",
                "status": "pending",
                "workspace_index": None,
                "agent_binding": {"agent_type": None, "agent_pid": None, "preset_name": None},
            },
        ],
    },
]


# ─── 数据库操作 ─────────────────────────────────────────────


def clean_db(conn: sqlite3.Connection):
    """清空所有业务数据（按外键依赖逆序）"""
    conn.execute("DELETE FROM state_changes")
    conn.execute("DELETE FROM tasks")
    conn.execute("DELETE FROM stories")
    conn.execute("DELETE FROM workspaces")
    conn.execute("DELETE FROM projects")
    conn.execute("DELETE FROM backends")
    conn.commit()
    print("[清理] 已清空所有业务数据")


def seed_backend(conn: sqlite3.Connection):
    """插入 mock 后端配置"""
    b = BACKEND
    conn.execute(
        """INSERT OR REPLACE INTO backends (id, name, endpoint, auth_token, enabled, backend_type)
           VALUES (?, ?, ?, ?, ?, ?)""",
        (b["id"], b["name"], b["endpoint"], b["auth_token"], b["enabled"], b["backend_type"]),
    )
    conn.commit()
    print(f"[后端] {b['name']} ({b['id']})")


def seed_project(conn: sqlite3.Connection) -> str:
    """插入 mock 项目，返回 project_id"""
    ts = now_iso()
    p = PROJECT
    config_json = json.dumps(p["config"], ensure_ascii=False)

    conn.execute(
        """INSERT OR REPLACE INTO projects (id, name, description, backend_id, config, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)""",
        (p["id"], p["name"], p["description"], p["backend_id"], config_json, ts, ts),
    )
    conn.commit()
    print(f"[项目] {p['name']} ({p['id'][:8]}...)")
    return p["id"]


def seed_workspaces(conn: sqlite3.Connection, project_id: str) -> list[str]:
    """插入 mock 工作空间，返回 workspace_id 列表"""
    ts = now_iso()
    workspace_ids = []

    for ws_def in WORKSPACES:
        ws_id = str(uuid.uuid4())
        workspace_ids.append(ws_id)
        git_config_json = json.dumps(ws_def["git_config"], ensure_ascii=False) if ws_def["git_config"] else None

        conn.execute(
            """INSERT INTO workspaces (id, project_id, name, container_ref, workspace_type, status, git_config, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (
                ws_id,
                project_id,
                ws_def["name"],
                ws_def["container_ref"],
                ws_def["workspace_type"],
                ws_def["status"],
                git_config_json,
                ts,
                ts,
            ),
        )
        print(f"  [Workspace] {ws_def['name']}  type={ws_def['workspace_type']}  status={ws_def['status']}")

    conn.commit()
    return workspace_ids


def seed_stories_and_tasks(conn: sqlite3.Connection, project_id: str, workspace_ids: list[str]):
    """插入 mock Story 和 Task"""
    ts = now_iso()
    backend_id = BACKEND["id"]

    for story_def in STORIES:
        story_id = str(uuid.uuid4())
        context_json = json.dumps(story_def["context"], ensure_ascii=False)

        conn.execute(
            """INSERT INTO stories (id, project_id, backend_id, title, description, status, context, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (
                story_id,
                project_id,
                backend_id,
                story_def["title"],
                story_def["description"],
                story_def["status"],
                context_json,
                ts,
                ts,
            ),
        )

        conn.execute(
            """INSERT INTO state_changes (entity_id, kind, payload, backend_id, created_at)
               VALUES (?, 'story_created', '{}', ?, ?)""",
            (story_id, backend_id, ts),
        )

        task_count = len(story_def.get("tasks", []))
        print(f"  [Story] {story_def['title']}  status={story_def['status']}  tasks={task_count}")

        for task_def in story_def.get("tasks", []):
            task_id = str(uuid.uuid4())
            ws_idx = task_def.get("workspace_index")
            workspace_id = workspace_ids[ws_idx] if ws_idx is not None else None
            agent_binding_json = json.dumps(task_def["agent_binding"], ensure_ascii=False)

            conn.execute(
                """INSERT INTO tasks (id, story_id, workspace_id, title, description, status, agent_binding, artifacts, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, '[]', ?, ?)""",
                (
                    task_id,
                    story_id,
                    workspace_id,
                    task_def["title"],
                    task_def.get("description", ""),
                    task_def["status"],
                    agent_binding_json,
                    ts,
                    ts,
                ),
            )
            agent_type = task_def["agent_binding"].get("agent_type") or "-"
            ws_name = WORKSPACES[ws_idx]["name"][:12] if ws_idx is not None else "-"
            print(f"    [Task] {task_def['title']}  status={task_def['status']}  agent={agent_type}  ws={ws_name}")

    conn.commit()


def main():
    parser = argparse.ArgumentParser(description="AgentDashboard Mock 数据生成")
    parser.add_argument("--db", default="agentdash.db", help="SQLite 数据库路径 (默认: agentdash.db)")
    parser.add_argument("--clean", action="store_true", help="清空后重新生成")
    args = parser.parse_args()

    conn = sqlite3.Connection(args.db)
    conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")

    if args.clean:
        clean_db(conn)

    print(f"\n{'='*50}")
    print(f"Mock 数据生成 → {args.db}")
    print(f"{'='*50}\n")

    seed_backend(conn)
    project_id = seed_project(conn)
    workspace_ids = seed_workspaces(conn, project_id)
    seed_stories_and_tasks(conn, project_id, workspace_ids)

    project_count = conn.execute("SELECT COUNT(*) FROM projects").fetchone()[0]
    workspace_count = conn.execute("SELECT COUNT(*) FROM workspaces").fetchone()[0]
    story_count = conn.execute("SELECT COUNT(*) FROM stories").fetchone()[0]
    task_count = conn.execute("SELECT COUNT(*) FROM tasks").fetchone()[0]

    print(f"\n{'='*50}")
    print(f"完成！数据库: {args.db}")
    print(f"  Projects:   {project_count}")
    print(f"  Workspaces: {workspace_count}")
    print(f"  Stories:    {story_count}")
    print(f"  Tasks:      {task_count}")
    print(f"{'='*50}\n")

    conn.close()


if __name__ == "__main__":
    main()

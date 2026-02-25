#!/usr/bin/env python3
"""
AgentDashboard Mock 数据生成脚本

用途：
  - 生成开发/测试用的 mock 数据（Backend、Story、Task）
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

STORIES = [
    {
        "title": "用户认证模块开发",
        "description": "实现 JWT 认证、登录注册、权限管理",
        "status": "executing",
        "context": {
            "items": [
                {
                    "id": "ctx-auth-spec",
                    "sourceKind": "spec",
                    "reference": "docs/auth-spec.md",
                    "reason": "认证模块设计规范",
                    "displayName": "认证规范文档",
                    "summary": "定义 JWT Token 结构、刷新策略和权限模型",
                },
                {
                    "id": "ctx-api-design",
                    "sourceKind": "content",
                    "reference": "docs/api-design.md",
                    "reason": "API 接口设计",
                    "displayName": "API 设计文档",
                    "summary": "登录/注册/刷新 Token 的 RESTful 接口定义",
                },
            ]
        },
        "tasks": [
            {
                "title": "设计 JWT Token 结构",
                "description": "定义 Token payload、过期策略和签名算法",
                "status": "completed",
                "agent_type": "planner",
            },
            {
                "title": "实现登录 API",
                "description": "POST /api/auth/login，支持邮箱+密码登录",
                "status": "running",
                "agent_type": "worker",
            },
            {
                "title": "实现注册 API",
                "description": "POST /api/auth/register，支持邮箱注册+验证",
                "status": "pending",
                "agent_type": "worker",
            },
        ],
    },
    {
        "title": "Story 看板视图优化",
        "description": "支持拖拽排序、状态筛选、搜索功能",
        "status": "executing",
        "context": {
            "items": [
                {
                    "id": "ctx-kanban-ux",
                    "sourceKind": "content",
                    "reference": "docs/kanban-ux.md",
                    "reason": "看板交互设计",
                    "displayName": "看板 UX 设计稿",
                    "summary": "拖拽排序、状态列、搜索栏的交互规范",
                },
            ]
        },
        "tasks": [
            {
                "title": "看板拖拽排序",
                "description": "基于 dnd-kit 实现 Story 卡片拖拽排序",
                "status": "running",
                "agent_type": "worker",
            },
            {
                "title": "状态筛选器",
                "description": "按 Story 状态筛选看板列",
                "status": "pending",
                "agent_type": "planner",
            },
        ],
    },
    {
        "title": "实时事件推送系统",
        "description": "基于 SSE 的状态变更实时推送，支持断线重连",
        "status": "failed",
        "context": {"items": []},
        "tasks": [
            {
                "title": "SSE 推送服务端实现",
                "description": "Axum SSE endpoint + broadcast channel",
                "status": "completed",
                "agent_type": "worker",
            },
            {
                "title": "客户端断线重连",
                "description": "EventSource 自动重连 + Resume 机制",
                "status": "failed",
                "agent_type": "worker",
            },
        ],
    },
    {
        "title": "Agent 执行引擎集成",
        "description": "对接 Claude Code / Codex，管理 Agent 生命周期",
        "status": "created",
        "context": {"items": []},
        "tasks": [
            {
                "title": "Claude Code 适配器",
                "description": "实现 ACP 协议适配层，对接 Claude Code CLI",
                "status": "pending",
                "agent_type": "researcher",
            },
            {
                "title": "Agent 生命周期管理",
                "description": "启动、监控、停止 Agent 进程的统一接口",
                "status": "pending",
                "agent_type": "planner",
            },
        ],
    },
    {
        "title": "多后端连接管理",
        "description": "支持同时连接多个远程后端，统一视图展示",
        "status": "created",
        "context": {"items": []},
        "tasks": [
            {
                "title": "后端连接池管理",
                "description": "管理多个后端 WebSocket/HTTP 连接的生命周期",
                "status": "assigned",
                "agent_type": "worker",
            },
            {
                "title": "连接状态监控",
                "description": "心跳检测、断线告警、自动重连",
                "status": "pending",
                "agent_type": "reviewer",
            },
        ],
    },
]


# ─── 数据库操作 ─────────────────────────────────────────────


def clean_db(conn: sqlite3.Connection):
    """清空所有业务数据"""
    conn.execute("DELETE FROM state_changes")
    conn.execute("DELETE FROM tasks")
    conn.execute("DELETE FROM stories")
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


def seed_stories_and_tasks(conn: sqlite3.Connection):
    """插入 mock Story 和 Task"""
    ts = now_iso()
    backend_id = BACKEND["id"]

    for story_def in STORIES:
        story_id = str(uuid.uuid4())
        context_json = json.dumps(story_def["context"], ensure_ascii=False)

        conn.execute(
            """INSERT INTO stories (id, backend_id, title, description, status, context, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)""",
            (
                story_id,
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
            conn.execute(
                """INSERT INTO tasks (id, story_id, title, description, status, agent_type, agent_pid, workspace_path, artifacts, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, '[]', ?, ?)""",
                (
                    task_id,
                    story_id,
                    task_def["title"],
                    task_def.get("description", ""),
                    task_def["status"],
                    task_def.get("agent_type"),
                    ts,
                    ts,
                ),
            )
            print(f"    [Task] {task_def['title']}  status={task_def['status']}  agent={task_def.get('agent_type', '-')}")

    conn.commit()


def main():
    parser = argparse.ArgumentParser(description="AgentDashboard Mock 数据生成")
    parser.add_argument("--db", default="agentdash.db", help="SQLite 数据库路径 (默认: agentdash.db)")
    parser.add_argument("--clean", action="store_true", help="清空后重新生成")
    args = parser.parse_args()

    conn = sqlite3.Connection(args.db)
    conn.execute("PRAGMA journal_mode=WAL")

    if args.clean:
        clean_db(conn)

    print(f"\n{'='*50}")
    print(f"Mock 数据生成 → {args.db}")
    print(f"{'='*50}\n")

    seed_backend(conn)
    seed_stories_and_tasks(conn)

    story_count = conn.execute("SELECT COUNT(*) FROM stories").fetchone()[0]
    task_count = conn.execute("SELECT COUNT(*) FROM tasks").fetchone()[0]

    print(f"\n{'='*50}")
    print(f"完成！数据库: {args.db}")
    print(f"  Stories: {story_count}")
    print(f"  Tasks:   {task_count}")
    print(f"{'='*50}\n")

    conn.close()


if __name__ == "__main__":
    main()

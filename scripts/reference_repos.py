#!/usr/bin/env python3
"""
管理 references/ 下的参考仓。

设计目标：
1. 主仓库不再直接跟踪参考仓内容
2. 用清单文件记录参考仓来源
3. 按需注册、移除、同步到最新
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "references" / "repositories.json"


@dataclass
class RepoConfig:
    name: str
    path: str
    url: str
    branch: str = "main"

    @property
    def abs_path(self) -> Path:
        return ROOT / self.path


def load_manifest() -> list[RepoConfig]:
    if not MANIFEST.exists():
        return []
    data = json.loads(MANIFEST.read_text(encoding="utf-8"))
    repos = data.get("repositories", [])
    return [RepoConfig(**repo) for repo in repos]


def save_manifest(repos: Iterable[RepoConfig]) -> None:
    payload = {
        "repositories": [
            {
                "name": repo.name,
                "path": repo.path,
                "url": repo.url,
                "branch": repo.branch,
            }
            for repo in sorted(repos, key=lambda item: item.name.lower())
        ]
    }
    MANIFEST.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )


def run_git(args: list[str], cwd: Path | None = None, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=str(cwd or ROOT),
        check=check,
        text=True,
        capture_output=True,
    )


def find_repo(repos: list[RepoConfig], name: str) -> RepoConfig:
    for repo in repos:
        if repo.name == name:
            return repo
    raise SystemExit(f"未找到参考仓: {name}")


def is_git_repo(path: Path) -> bool:
    if not path.exists():
        return False
    result = run_git(["rev-parse", "--is-inside-work-tree"], cwd=path, check=False)
    return result.returncode == 0 and result.stdout.strip() == "true"


def is_dirty(path: Path) -> bool:
    result = run_git(["status", "--short"], cwd=path, check=False)
    return result.returncode == 0 and bool(result.stdout.strip())


def ensure_parent(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)


def clone_repo(repo: RepoConfig) -> None:
    ensure_parent(repo.abs_path)
    print(f"[clone] {repo.name} -> {repo.path}")
    subprocess.run(
        [
            "git",
            "clone",
            "--branch",
            repo.branch,
            "--single-branch",
            repo.url,
            str(repo.abs_path),
        ],
        check=True,
        text=True,
    )


def sync_repo(repo: RepoConfig, force: bool = False) -> None:
    path = repo.abs_path
    if not path.exists():
        clone_repo(repo)
        return

    if not is_git_repo(path):
        raise SystemExit(f"{repo.path} 已存在，但不是 Git 仓库，停止同步。")

    if is_dirty(path) and not force:
        print(f"[skip] {repo.name}: 存在未提交改动，使用 --force 才会继续。")
        return

    current_origin = run_git(["remote", "get-url", "origin"], cwd=path, check=False)
    if current_origin.returncode == 0:
        origin_url = current_origin.stdout.strip()
        if origin_url != repo.url:
            raise SystemExit(
                f"{repo.name} 的 origin 不匹配：当前={origin_url}，清单={repo.url}"
            )

    print(f"[sync] {repo.name}")
    run_git(["fetch", "origin"], cwd=path)
    run_git(["checkout", repo.branch], cwd=path)
    run_git(["pull", "--ff-only", "origin", repo.branch], cwd=path)


def show_list(repos: list[RepoConfig]) -> None:
    if not repos:
        print("当前没有已注册的参考仓。")
        return

    for repo in repos:
        path = repo.abs_path
        state = "missing"
        extra = ""
        if is_git_repo(path):
            state = "ready"
            head = run_git(["rev-parse", "--short", "HEAD"], cwd=path, check=False)
            branch = run_git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=path, check=False)
            dirty = " dirty" if is_dirty(path) else ""
            extra = f" {branch.stdout.strip()} {head.stdout.strip()}{dirty}".rstrip()
        elif path.exists():
            state = "non-git"
        print(f"{repo.name}: {repo.path} [{state}] {extra}".rstrip())


def cmd_register(args: argparse.Namespace) -> None:
    repos = load_manifest()
    if any(repo.name == args.name for repo in repos):
        raise SystemExit(f"参考仓已存在: {args.name}")

    path = args.path or f"references/{args.name}"
    repos.append(
        RepoConfig(
            name=args.name,
            path=path,
            url=args.url,
            branch=args.branch,
        )
    )
    save_manifest(repos)
    print(f"已注册参考仓: {args.name} -> {path}")


def cmd_remove(args: argparse.Namespace) -> None:
    repos = load_manifest()
    next_repos = [repo for repo in repos if repo.name != args.name]
    if len(next_repos) == len(repos):
        raise SystemExit(f"参考仓不存在: {args.name}")
    save_manifest(next_repos)
    print(f"已移除参考仓配置: {args.name}")


def cmd_sync(args: argparse.Namespace) -> None:
    repos = load_manifest()
    selected = repos if args.all or not args.names else [find_repo(repos, name) for name in args.names]
    for repo in selected:
        sync_repo(repo, force=args.force)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="管理 references/ 下的参考仓")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("list", help="列出已注册参考仓")

    register = sub.add_parser("register", help="注册新的参考仓")
    register.add_argument("name", help="参考仓名称")
    register.add_argument("url", help="Git 仓库地址")
    register.add_argument("--branch", default="main", help="默认分支，默认 main")
    register.add_argument("--path", help="相对仓库根目录的目标路径，默认 references/<name>")

    remove = sub.add_parser("remove", help="移除参考仓配置")
    remove.add_argument("name", help="参考仓名称")

    sync = sub.add_parser("sync", help="同步参考仓到最新")
    sync.add_argument("names", nargs="*", help="指定要同步的参考仓名；不传则同步全部")
    sync.add_argument("--all", action="store_true", help="显式同步所有已注册参考仓")
    sync.add_argument("--force", action="store_true", help="即使本地有未提交改动也继续拉取")

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()

    if args.command == "list":
        show_list(load_manifest())
        return 0
    if args.command == "register":
        cmd_register(args)
        return 0
    if args.command == "remove":
        cmd_remove(args)
        return 0
    if args.command == "sync":
        cmd_sync(args)
        return 0

    parser.error(f"未知命令: {args.command}")
    return 1


if __name__ == "__main__":
    sys.exit(main())

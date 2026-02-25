# AgentDashboard - Agent 协作说明

使用中文进行文档编写/与用户交流

## 项目背景

本项目旨在构建一个用于维护任意多个项目（包括项目中涉及的生产Task及其进度）的管理工程。

**核心灵感来源：**
1. 胡渊鸣的实践（多 Claude Code 实例并行工作）
2. vibe-kanban（管理多个 coding agent 的看板系统）
3. Trellis（AI 框架和工具包）

**用户的目标：** 用一个统一的看板控制多设备、多类型项目中 agent 协同运转，支持任意数字生产 SOP 的维护和管理。

<!-- TRELLIS:START -->
# Trellis Instructions

These instructions are for AI assistants working in this project.

Use the `/trellis:start` command when starting a new session to:
- Initialize your developer identity
- Understand current project context
- Read relevant guidelines

Use `@/.trellis/` to learn:
- Development workflow (`workflow.md`)
- Project structure guidelines (`spec/`)
- Developer workspace (`workspace/`)

Keep this managed block so 'trellis update' can refresh the instructions.

<!-- TRELLIS:END -->

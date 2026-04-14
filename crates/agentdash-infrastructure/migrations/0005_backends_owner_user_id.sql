-- 为 backends 表添加 owner_user_id 字段
-- 用于标识注册该后端的用户，支持 MCP relay 路由和多租户场景
ALTER TABLE backends ADD COLUMN IF NOT EXISTS owner_user_id TEXT;

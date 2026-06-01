-- AgentFrame execution_profile_json: 存储执行器配置快照。
-- AgentFrameBuilder 将 AgentConfig 序列化写入此字段，
-- RuntimeLaunchRequest.from_frame() 投影时读取。
ALTER TABLE agent_frames ADD COLUMN IF NOT EXISTS execution_profile_json TEXT;

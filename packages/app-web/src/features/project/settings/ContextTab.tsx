import { Link } from "react-router-dom";
import type { Project } from "../../../types";
import { VfsBrowser } from "../../vfs";
import { SectionCard } from "./settings-ui";
import { MountOverviewList } from "./MountOverviewList";

export function ContextTab({ project }: { project: Project }) {
  return (
    <>
      <SectionCard
        title="Project VFS Mount"
        description="Project 级 VFS 挂载点（Inline 文件 / 外部服务）已归入 Assets，CRUD 在资产流程中统一管理。"
      >
        <Link to="/dashboard/assets/vfs-mount" className="agentdash-button-secondary inline-flex">
          打开 VFS Mount 资产
        </Link>
      </SectionCard>

      <SectionCard
        title="解析后的 VFS Mount"
        description="基于当前 Workspace 与项目级 VFS 配置派生出的运行时挂载点概览。"
      >
        <MountOverviewList projectId={project.id} />
      </SectionCard>

      <SectionCard
        title="Runtime Preview"
        description="VFS 预览明确作为派生结果展示，用来解释当前默认配置会解析出什么挂载。"
      >
        <VfsBrowser source={{ source_type: "project_preview", project_id: project.id }} />
      </SectionCard>
    </>
  );
}

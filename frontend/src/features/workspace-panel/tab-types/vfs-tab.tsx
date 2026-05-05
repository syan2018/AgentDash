/* eslint-disable react-refresh/only-export-components */
import { VfsBrowserPanel } from "../../vfs";
import { useWorkspaceData } from "../workspace-data-context";
import type { TabContentRenderProps, TabTypeDescriptor } from "../tab-type-registry";
import { VfsIcon } from "./icons";

const SCHEME = "vfs://";

function parseVfsUri(uri: string): { surfaceRef?: string; mountId?: string } | null {
  if (!uri.startsWith(SCHEME)) return null;
  const rest = uri.slice(SCHEME.length);
  const parts = rest.split("/");
  return {
    surfaceRef: parts[0] || undefined,
    mountId: parts[1] || undefined,
  };
}

function VfsTabContent({ uri }: TabContentRenderProps) {
  const { vfs, runtimeSurface } = useWorkspaceData();
  const parsed = parseVfsUri(uri);

  const hasMounts =
    (vfs && vfs.mounts.length > 0) ||
    (runtimeSurface && runtimeSurface.mounts.length > 0);

  if (!hasMounts) {
    return (
      <div className="flex h-full min-h-[200px] items-center justify-center px-6">
        <p className="text-center text-sm text-muted-foreground">
          当前会话没有挂载的地址空间。
        </p>
      </div>
    );
  }

  return (
    <VfsBrowserPanel
      surface={runtimeSurface}
      vfs={vfs}
      initialMountId={parsed?.mountId}
    />
  );
}

export const vfsTabType: TabTypeDescriptor = {
  typeId: "vfs",
  label: "地址空间",
  icon: VfsIcon,
  allowMultiple: true,
  pinned: false,

  renderContent: (props) => <VfsTabContent {...props} />,

  resolveTitle: (uri) => {
    const parsed = parseVfsUri(uri);
    if (parsed?.mountId) return `VFS: ${parsed.mountId}`;
    return "地址空间";
  },

  parseUri: (uri) => {
    const parsed = parseVfsUri(uri);
    return parsed as Record<string, string> | null;
  },

  buildUri: (params) => {
    const { surfaceRef, mountId } = params;
    if (surfaceRef && mountId) return `${SCHEME}${surfaceRef}/${mountId}`;
    if (surfaceRef) return `${SCHEME}${surfaceRef}`;
    return `${SCHEME}default`;
  },

  defaultUri: `${SCHEME}default`,
  menuOrder: 20,
};

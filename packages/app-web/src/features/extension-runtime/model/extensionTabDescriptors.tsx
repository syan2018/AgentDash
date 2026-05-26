import type {
  ExtensionRuntimeProjectionResponse,
  ExtensionWorkspaceTabProjectionResponse,
} from "../../../types";
import type { TabTypeDescriptor } from "../../workspace-panel/tab-type-registry";
import { ExtensionTabIcon } from "../ui/ExtensionTabIcon";
import { ExtensionCanvasPanel } from "../ui/ExtensionCanvasPanel";
import { ExtensionWebviewPanel } from "../ui/ExtensionWebviewPanel";

interface CreateExtensionTabDescriptorsInput {
  projection: ExtensionRuntimeProjectionResponse;
}

export function createExtensionTabDescriptors({
  projection,
}: CreateExtensionTabDescriptorsInput): TabTypeDescriptor[] {
  return projection.workspace_tabs.map((tab, index) => createExtensionTabDescriptor(tab, index));
}

function createExtensionTabDescriptor(
  tab: ExtensionWorkspaceTabProjectionResponse,
  index: number,
): TabTypeDescriptor {
  return {
    typeId: tab.type_id,
    label: tab.label,
    icon: ExtensionTabIcon,
    allowMultiple: true,
    pinned: false,
    defaultUri: `${tab.uri_scheme}://panel`,
    menuOrder: 200 + index,
    renderContent: (props) => {
      if (tab.renderer.kind === "canvas_panel") {
        return <ExtensionCanvasPanel tab={tab} />;
      }
      return (
        <ExtensionWebviewPanel
          tab={tab}
          uri={props.uri}
          tabId={props.tabId}
          isActive={props.isActive}
        />
      );
    },
    resolveTitle(uri) {
      const parsed = parseExtensionTabUri(tab.uri_scheme, uri);
      if (!parsed) return tab.label;
      return parsed.resource ? `${tab.label}: ${parsed.resource}` : tab.label;
    },
    parseUri(uri) {
      return parseExtensionTabUri(tab.uri_scheme, uri);
    },
    buildUri(params) {
      const resource = params.resource?.trim() || "panel";
      return `${tab.uri_scheme}://${encodeURIComponent(resource)}`;
    },
  };
}

function parseExtensionTabUri(
  scheme: string,
  uri: string,
): Record<string, string> | null {
  const prefix = `${scheme}://`;
  if (!uri.startsWith(prefix)) return null;
  const rawResource = uri.slice(prefix.length);
  return {
    resource: rawResource ? decodeSafe(rawResource) : "panel",
  };
}

function decodeSafe(raw: string): string {
  try {
    return decodeURIComponent(raw);
  } catch {
    return raw;
  }
}

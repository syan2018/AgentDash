import type { WorkspaceIdentityKind } from "../../../types";
import { updatePayloadField } from "./editorHelpers";

interface IdentityFieldsProps {
  identityKind: WorkspaceIdentityKind;
  identityPayload: Record<string, unknown>;
  onPayloadChange: (payload: Record<string, unknown>) => void;
}

export function IdentityFields({
  identityKind,
  identityPayload,
  onPayloadChange,
}: IdentityFieldsProps) {
  const fieldValue = (key: string) => {
    const value = identityPayload[key];
    return typeof value === "string" ? value : "";
  };

  if (identityKind === "git_repo") {
    return (
      <div className="grid gap-3 md:grid-cols-2">
        <input
          value={fieldValue("repo_key")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "repo_key", event.target.value))}
          placeholder="仓库地址，例如 https://github.com/org/repo"
          className="agentdash-form-input"
        />
        <input
          value={fieldValue("branch")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "branch", event.target.value))}
          placeholder="分支，可选，例如 main"
          className="agentdash-form-input"
        />
      </div>
    );
  }

  if (identityKind === "p4_workspace") {
    return (
      <div className="grid gap-3 md:grid-cols-2">
        <input
          value={fieldValue("server_address")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "server_address", event.target.value))}
          placeholder="P4 服务器地址，例如 perforce:1666"
          className="agentdash-form-input"
        />
        <input
          value={fieldValue("stream")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "stream", event.target.value))}
          placeholder="Stream，例如 //depot/main"
          className="agentdash-form-input"
        />
        <input
          value={fieldValue("client_name")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "client_name", event.target.value))}
          placeholder="客户端名称（client）"
          className="agentdash-form-input"
        />
        <input
          value={fieldValue("path_key")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "path_key", event.target.value))}
          placeholder="本地目录，例如 d:/workspaces/app"
          className="agentdash-form-input"
        />
      </div>
    );
  }

  return (
    <input
      value={fieldValue("path_key")}
      onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "path_key", event.target.value))}
      placeholder="本地目录，例如 d:/workspaces/app"
      className="agentdash-form-input"
    />
  );
}

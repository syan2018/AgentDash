import { API_ORIGIN } from "../../../api/origin";

const DEFAULT_WORKSPACE_ROOT = "/srv/agentdash/workspaces";

export interface RunnerSetupCommandInput {
  /** 云端 origin（不含末尾斜杠）。runner 会用它连接云端 backend。 */
  origin: string;
  /** 一次性明文 registration token。 */
  token: string;
  /** runner 名称（建议与 token 名称一致）。 */
  runnerName: string;
  /** runner 上的 workspace 根目录。 */
  workspaceRoot: string;
}

/**
 * 解析当前云端 origin：优先使用显式配置的 API origin，否则回退到当前窗口 location。
 *
 * runner setup 需要一个可被服务器进程访问的云端地址，因此用浏览器当前所在的云端 origin。
 */
export function resolveCloudOrigin(): string {
  const configured = API_ORIGIN.trim();
  if (configured) {
    return stripTrailingSlash(configured);
  }
  if (typeof window !== "undefined" && window.location?.origin) {
    return stripTrailingSlash(window.location.origin);
  }
  return "";
}

export function defaultRunnerWorkspaceRoot(): string {
  return DEFAULT_WORKSPACE_ROOT;
}

function stripTrailingSlash(value: string): string {
  return value.replace(/\/+$/, "");
}

/**
 * 对 shell 参数做最小化引用：仅当值包含空白或 shell 元字符时才加单引号。
 * 空字符串保留为占位符（不引用），让用户能直接看到需要替换的位置。
 */
function shellArg(value: string): string {
  if (value === "") {
    return value;
  }
  if (/^[A-Za-z0-9_./:@%+=-]+$/.test(value)) {
    return value;
  }
  return `'${value.replace(/'/g, "'\\''")}'`;
}

/**
 * 拼装独立 runner 的一键 setup 命令（通用 binary + 显式 origin）。
 *
 * 固定包含 --install-service 与 --start，让服务器部署一步到位。
 */
export function buildRunnerSetupCommand(input: RunnerSetupCommandInput): string {
  const origin = stripTrailingSlash(input.origin.trim());
  const workspaceRoot = input.workspaceRoot.trim() || DEFAULT_WORKSPACE_ROOT;
  const parts = [
    "agentdash-local setup",
    `--server-url ${shellArg(origin)}`,
    `--registration-token ${shellArg(input.token)}`,
    `--runner-name ${shellArg(input.runnerName)}`,
    `--workspace-root ${shellArg(workspaceRoot)}`,
    "--install-service",
    "--start",
  ];
  return parts.join(" ");
}

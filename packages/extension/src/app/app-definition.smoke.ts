import {
  backendService,
  customProtocol,
  defineApp,
  httpProxy,
  localCommand,
  workspaceFiles,
} from "./index.js";
import type {
  AgentDashAppDefinition,
  AgentDashRuntimePermissionKey,
} from "./index.js";

const inputSchema = {
  type: "object",
  additionalProperties: false,
  properties: {
    path: { type: "string" },
  },
  required: ["path"],
};

export const smokeRuntimePermissions: readonly AgentDashRuntimePermissionKey[] = [
  "process.exec",
  "process.shell",
  "workspace.vfs.read",
];

export const smokeAppDefinition: AgentDashAppDefinition = defineApp({
  id: "repo-tools",
  name: "Repo Tools",
  version: "0.1.0",
  panel: {
    entry: "src/main.tsx",
  },
  capabilities: {
    github: httpProxy({
      baseUrl: "https://api.github.com",
      access: "read_write",
      expose: {
        description: "Fetch GitHub repository metadata through the selected local backend.",
        input_schema: inputSchema,
      },
    }),
    gitStatus: localCommand({
      command: "git",
      args: ["status", "--short"],
      expose: {
        description: "Read the current workspace Git status.",
      },
    }),
    files: workspaceFiles({
      access: "read_write",
      expose: {
        description: "Read and write files in the current AgentRun workspace.",
        input_schema: inputSchema,
      },
    }),
    protocol: customProtocol({
      description: "Structured protocol escape hatch.",
      methods: {
        summarize: {
          description: "Summarize a structured payload.",
          permissions: ["extension.protocol.invoke:repo-tools.protocol"],
          expose: {
            description: "Summarize a structured payload for the Agent.",
          },
        },
      },
    }),
    api: backendService({
      entry: "src/server/index.ts",
      runtime: "node",
      routes: ["/api/**"],
      healthPath: "/health",
      expose: {
        description: "Invoke the extension-owned backend service through the selected backend.",
        input_schema: inputSchema,
      },
    }),
  },
});

export const smokeOperationKinds = smokeAppDefinition.operation_catalog.map((operation) => operation.dispatch.kind);
export const smokeArtifactKinds = smokeAppDefinition.artifacts.map((artifact) => artifact.kind);

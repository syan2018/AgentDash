import { defineExtension, type JsonObject } from "@agentdash/extension/host";
import { LOCAL_HELLO_ACTION_KEY, normalizeProfile } from "./shared/schema";

export default defineExtension({
  manifest: {
    manifest_version: "2",
    extension_id: "local-hello",
    package: {
      name: "@agentdash/example-local-hello",
      version: "0.1.0",
    },
    asset_version: "0.1.0",
  },
  activate(ctx) {
    ctx.permissions.require({ kind: "local_profile", access: "read" });
    ctx.runtime.registerAction<Record<string, never>, JsonObject>({
      action_key: LOCAL_HELLO_ACTION_KEY,
      kind: "session_runtime",
      description: "Read the current local runtime profile",
      input_schema: {},
      output_schema: {},
      permissions: ["local.profile.read"],
      async invoke() {
        return normalizeProfile(await ctx.api.local.getProfile());
      },
    });
  },
});

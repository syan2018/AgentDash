pub(super) const EXTENSION_HOST_RUNNER: &str = r#"
import fs from "node:fs/promises";
import path from "node:path";
import readline from "node:readline";
import vm from "node:vm";
import { pathToFileURL } from "node:url";

let active = null;
let currentActionKey = null;
let nextHostApiId = 1;
const pendingHostApi = new Map();

const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function log(level, message) {
  send({ kind: "log", level, message: String(message) });
}

function toJsonValue(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (Array.isArray(value)) return value.map(toJsonValue);
  if (typeof value === "object") {
    const result = {};
    for (const [key, item] of Object.entries(value)) {
      if (typeof item !== "function" && typeof item !== "symbol" && typeof item !== "undefined") {
        result[key] = toJsonValue(item);
      }
    }
    return result;
  }
  return null;
}

function canonicalChannelKey(extensionKey, channelKey) {
  if (typeof channelKey !== "string" || channelKey.trim() === "") {
    throw new Error("channel_key is required");
  }
  return channelKey.includes(".") ? channelKey : `${extensionKey}.${channelKey}`;
}

function channelHandlerKey(channelKey, method) {
  return `${channelKey}#${method}`;
}

function normalizeChannelMethods(methods) {
  if (Array.isArray(methods)) {
    return methods;
  }
  if (methods && typeof methods === "object") {
    return Object.entries(methods).map(([name, method]) => ({ ...method, name: method.name ?? name }));
  }
  throw new Error("protocol channel methods must be an array or object");
}

function createExtensionContext(extensionKey) {
  const actions = new Map();
  const channels = new Map();
  const contributions = {
    commands: [],
    flags: [],
    runtime_actions: [],
    protocol_channels: [],
    workspace_panels: [],
    permissions: [],
  };
  const ctx = {
    api: {
      runtime: {
        async invoke(actionKey, input) {
          return await requestHostApi("runtime.invoke", { action_key: actionKey, input: toJsonValue(input) });
        },
      },
      local: {
        async getProfile() {
          return await requestHostApi("local.get_profile", { action_key: currentActionKey });
        },
      },
      http: {
        async fetch(url, options = {}) {
          return await requestHostApi("http.fetch", { action_key: currentActionKey, url, options: toJsonValue(options) });
        },
        async fetchJson(url, options = {}) {
          const response = await requestHostApi("http.fetch", { action_key: currentActionKey, url, options: toJsonValue(options) });
          if (response && typeof response === "object" && typeof response.body === "string") {
            return JSON.parse(response.body);
          }
          return response;
        },
      },
      workspace: {
        async readText(path) {
          return await requestHostApi("workspace.read_text", { action_key: currentActionKey, path });
        },
        async writeText(path, content) {
          return await requestHostApi("workspace.write_text", { action_key: currentActionKey, path, content });
        },
        async list(path) {
          return await requestHostApi("workspace.list", { action_key: currentActionKey, path });
        },
        async stat(path) {
          return await requestHostApi("workspace.stat", { action_key: currentActionKey, path });
        },
      },
      env: {
        async get(name) {
          return await requestHostApi("env.get", { action_key: currentActionKey, name });
        },
      },
      process: {
        async exec(command, args = [], options = {}) {
          return await requestHostApi("process.exec", { action_key: currentActionKey, command, args: toJsonValue(args), options: toJsonValue(options) });
        },
        async shell(command, options = {}) {
          return await requestHostApi("process.shell", { action_key: currentActionKey, command, options: toJsonValue(options) });
        },
      },
      channels: {
        async invoke(channelKey, method, input) {
          const canonical = canonicalChannelKey(extensionKey, channelKey);
          if (channels.has(channelHandlerKey(canonical, method))) {
            return await invokeRegisteredChannel(channels, canonical, method, input);
          }
          return await requestHostApi("extension.channel_invoke", {
            action_key: currentActionKey,
            channel_key: channelKey,
            method,
            input: toJsonValue(input),
          });
        },
        self(channelKey = "api") {
          const canonical = canonicalChannelKey(extensionKey, channelKey);
          return {
            async invoke(method, input) {
              return await invokeRegisteredChannel(channels, canonical, method, input);
            },
          };
        },
        from(alias, channelKey = null) {
          return {
            async invoke(method, input) {
              return await requestHostApi("extension.channel_invoke", {
                action_key: currentActionKey,
                dependency_alias: alias,
                channel_key: channelKey,
                method,
                input: toJsonValue(input),
              });
            },
          };
        },
      },
    },
    commands: {
      registerCommand(definition) {
        contributions.commands.push(toJsonValue(definition));
      },
    },
    flags: {
      registerFlag(definition) {
        contributions.flags.push(toJsonValue(definition));
      },
    },
    runtime: {
      registerAction(definition) {
        if (!definition || typeof definition.action_key !== "string" || typeof definition.invoke !== "function") {
          throw new Error("runtime action must include action_key and invoke");
        }
        actions.set(definition.action_key, definition);
        const { invoke, ...serializable } = definition;
        contributions.runtime_actions.push(toJsonValue(serializable));
      },
    },
    channels: {
      register(definition) {
        if (!definition || typeof definition.channel_key !== "string") {
          throw new Error("protocol channel must include channel_key");
        }
        const canonical = canonicalChannelKey(extensionKey, definition.channel_key);
        const methods = normalizeChannelMethods(definition.methods);
        if (methods.length === 0) {
          throw new Error("protocol channel must include at least one method");
        }
        const serializableMethods = [];
        for (const method of methods) {
          if (!method || typeof method.name !== "string" || typeof method.invoke !== "function") {
            throw new Error("protocol channel method must include name and invoke");
          }
          channels.set(channelHandlerKey(canonical, method.name), method);
          const { invoke, ...serializable } = method;
          serializableMethods.push(toJsonValue(serializable));
        }
        contributions.protocol_channels.push(toJsonValue({
          ...definition,
          channel_key: canonical,
          methods: serializableMethods,
        }));
      },
    },
    workspace: {
      registerPanel(definition) {
        contributions.workspace_panels.push(toJsonValue(definition));
      },
    },
    permissions: {
      require(permission) {
        contributions.permissions.push(toJsonValue(permission));
      },
    },
    contributions,
  };
  return { ctx, actions, channels, contributions };
}

async function invokeRegisteredChannel(channels, channelKey, method, input) {
  const handler = channels.get(channelHandlerKey(channelKey, method));
  if (!handler) {
    throw new Error(`extension channel method is not registered: ${channelKey}.${method}`);
  }
  return toJsonValue(await handler.invoke(toJsonValue(input)));
}

async function requestHostApi(method, params) {
  const id = `host-api-${nextHostApiId++}`;
  send({ kind: "host_api_request", id, method, params: toJsonValue(params) });
  return await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pendingHostApi.delete(id);
      reject(new Error(`host api timeout: ${method}`));
    }, 30000);
    pendingHostApi.set(id, {
      resolve(value) {
        clearTimeout(timeout);
        resolve(value);
      },
      reject(error) {
        clearTimeout(timeout);
        reject(error);
      },
    });
  });
}

async function loadExtension(bundlePath) {
  const source = await fs.readFile(bundlePath, "utf8");
  const moduleUrl = pathToFileURL(path.resolve(bundlePath)).href;
  const context = vm.createContext({
    console: {
      log: (...args) => log("info", args.join(" ")),
      warn: (...args) => log("warn", args.join(" ")),
      error: (...args) => log("error", args.join(" ")),
    },
    setTimeout,
    clearTimeout,
    structuredClone,
    TextDecoder,
    TextEncoder,
  });
  const module = new vm.SourceTextModule(source, {
    context,
    identifier: `${moduleUrl}?t=${Date.now()}`,
    initializeImportMeta(meta) {
      meta.url = moduleUrl;
    },
    importModuleDynamically(specifier) {
      throw new Error(`extension bundle must be self-contained; dynamic import blocked: ${specifier}`);
    },
  });
  await module.link((specifier) => {
    throw new Error(`extension bundle must be self-contained; import blocked: ${specifier}`);
  });
  await module.evaluate();
  const exported = module.namespace.default ?? module.namespace.extension;
  if (!exported || typeof exported !== "object") {
    throw new Error("extension bundle must export a default extension object");
  }
  return exported;
}

async function activate(params) {
  const extension = await loadExtension(params.bundle_path);
  const { ctx, actions, channels, contributions } = createExtensionContext(params.extension_key);
  active = {
    extension,
    manifest: params.manifest,
    extensionKey: params.extension_key,
    actions,
    channels,
    contributions,
  };
  if (typeof extension.activate === "function") {
    await extension.activate(ctx);
  }
  return healthPayload();
}

async function deactivate() {
  if (active?.extension && typeof active.extension.deactivate === "function") {
    await active.extension.deactivate();
  }
  active = null;
  return healthPayload();
}

async function invokeAction(params) {
  if (!active) throw new Error("extension is not active");
  const actionKey = params.action_key;
  const action = active.actions.get(actionKey);
  if (!action) throw new Error(`extension action is not registered: ${actionKey}`);
  const previous = currentActionKey;
  currentActionKey = actionKey;
  try {
    return toJsonValue(await action.invoke(toJsonValue(params.input)));
  } finally {
    currentActionKey = previous;
  }
}

async function invokeChannel(params) {
  if (!active) throw new Error("extension is not active");
  const channelKey = canonicalChannelKey(active.extensionKey, params.channel_key);
  const method = params.method;
  if (typeof method !== "string" || method.trim() === "") {
    throw new Error("extension channel method is required");
  }
  return await invokeRegisteredChannel(active.channels, channelKey, method, params.input);
}

function healthPayload() {
  return {
    active: Boolean(active),
    extension_id: active?.manifest?.extension_id ?? null,
    action_keys: active ? [...active.actions.keys()].sort() : [],
    channel_keys: active ? [...new Set([...active.channels.keys()].map((key) => key.split('#')[0]))].sort() : [],
    pid: process.pid,
  };
}

async function handleRequest(message) {
  switch (message.method) {
    case "activate":
      return await activate(message.params ?? {});
    case "reload":
      await deactivate();
      return await activate(message.params ?? {});
    case "deactivate":
      return await deactivate();
    case "invoke_action":
      return await invokeAction(message.params ?? {});
    case "invoke_channel":
      return await invokeChannel(message.params ?? {});
    case "health":
      return healthPayload();
    default:
      throw new Error(`unknown extension host method: ${message.method}`);
  }
}

rl.on("line", (line) => {
  void (async () => {
    const message = JSON.parse(line);
    if (message.kind === "host_api_response") {
      const pending = pendingHostApi.get(message.id);
      if (!pending) return;
      pendingHostApi.delete(message.id);
      if (message.error) pending.reject(new Error(message.error));
      else pending.resolve(message.result ?? null);
      return;
    }
    if (message.kind !== "request") return;
    try {
      const result = await handleRequest(message);
      send({ kind: "response", id: message.id, result: toJsonValue(result) });
    } catch (error) {
      send({ kind: "response", id: message.id, error: error instanceof Error ? error.message : String(error) });
    }
  })().catch((error) => {
    send({ kind: "log", level: "error", message: error instanceof Error ? error.message : String(error) });
  });
});
"#;

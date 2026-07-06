import { useMemo, useState } from "react";
import { createExtensionBridge, type JsonObject, type JsonValue } from "@agentdash/extension/browser";

import { PROTOCOL_DEMO_ACTIONS, displayJson } from "../shared/schema";

type PanelState = {
  greet: JsonValue;
  workspace: JsonValue;
  shell: JsonValue;
  channel: JsonValue;
  bridgeChannel: JsonValue;
  error: string | null;
};

const initialState: PanelState = {
  greet: null,
  workspace: null,
  shell: null,
  channel: null,
  bridgeChannel: null,
  error: null,
};

export function App() {
  const bridge = useMemo(() => createExtensionBridge(), []);
  const [name, setName] = useState("AgentDash");
  const [busy, setBusy] = useState(false);
  const [state, setState] = useState(initialState);

  async function runDemo() {
    setBusy(true);
    setState(initialState);
    try {
      const input: JsonObject = { name };
      const greet = await bridge.invokeAction<JsonObject, JsonValue>(
        PROTOCOL_DEMO_ACTIONS.greet,
        input,
      );
      const workspace = await bridge.invokeAction<JsonObject, JsonValue>(
        PROTOCOL_DEMO_ACTIONS.workspaceDemo,
        { file_name: "protocol-demo/panel.txt", content: `hello ${name}` },
      );
      const shell = await bridge.invokeAction<JsonObject, JsonValue>(
        PROTOCOL_DEMO_ACTIONS.shellDemo,
        { label: "panel" },
      );
      const channel = await bridge.invokeAction<JsonObject, JsonValue>(
        PROTOCOL_DEMO_ACTIONS.consumeDemoChannel,
        input,
      );
      const bridgeChannel = await bridge.invokeChannel<JsonObject, JsonValue>(
        "api",
        "greet",
        input,
      );
      setState({ greet, workspace, shell, channel, bridgeChannel, error: null });
    } catch (error) {
      setState({
        ...initialState,
        error: error instanceof Error ? error.message : "Protocol demo failed",
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="shell" aria-label="Protocol Demo">
      <header className="header">
        <p className="eyebrow">AgentDash Extension</p>
        <h1>Protocol Demo</h1>
        <p className="summary">TypeScript host actions and a self-scoped protocol channel.</p>
      </header>
      <div className="toolbar">
        <input
          value={name}
          onChange={(event) => setName(event.currentTarget.value)}
          aria-label="Name"
        />
        <button type="button" onClick={runDemo} disabled={busy}>
          {busy ? "Running" : "Run"}
        </button>
      </div>
      {state.error ? (
        <div className="state error" role="alert">
          {state.error}
        </div>
      ) : null}
      <div className="grid">
        <ResultTile title="Pure TS Action" value={state.greet} />
        <ResultTile title="Workspace API" value={state.workspace} />
        <ResultTile title="Process API" value={state.shell} />
        <ResultTile title="Self Channel" value={state.channel} />
        <ResultTile title="Panel Channel" value={state.bridgeChannel} />
      </div>
    </section>
  );
}

function ResultTile({ title, value }: { title: string; value: JsonValue }) {
  return (
    <article className="tile">
      <h2>{title}</h2>
      <pre>{value == null ? "{}" : displayJson(value)}</pre>
    </article>
  );
}

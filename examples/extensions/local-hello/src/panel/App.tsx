import { useEffect, useMemo, useState } from "react";
import { createExtensionBridge, type JsonObject } from "@agentdash/extension-ui";
import {
  LOCAL_HELLO_ACTION_KEY,
  normalizeProfile,
  type LocalHelloProfile,
} from "../shared/schema";

type ProfileState =
  | { status: "loading" }
  | { status: "ready"; profile: LocalHelloProfile }
  | { status: "error"; message: string };

export function App() {
  const bridge = useMemo(() => createExtensionBridge(), []);
  const [state, setState] = useState<ProfileState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    bridge.invokeAction<Record<string, never>, JsonObject>(LOCAL_HELLO_ACTION_KEY, {})
      .then((profile) => {
        if (!cancelled) {
          setState({ status: "ready", profile: normalizeProfile(profile) });
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setState({
            status: "error",
            message: error instanceof Error ? error.message : "Profile request failed",
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [bridge]);

  if (state.status === "loading") {
    return (
      <div className="state" aria-live="polite">
        Loading Local Hello profile...
      </div>
    );
  }

  if (state.status === "error") {
    return (
      <div className="state error" role="alert">
        {state.message}
      </div>
    );
  }

  return (
    <section className="shell" aria-label="Local Hello profile">
      <header className="header">
        <p className="eyebrow">AgentDash Extension</p>
        <h1>Local Hello</h1>
        <p className="summary">Profile loaded from the local TypeScript extension host.</p>
      </header>
      <dl className="profile">
        <div className="field">
          <dt>Username</dt>
          <dd data-testid="local-hello-username">{state.profile.username}</dd>
        </div>
        <div className="field">
          <dt>Platform</dt>
          <dd data-testid="local-hello-platform">{state.profile.platform}</dd>
        </div>
        <div className="field">
          <dt>Backend</dt>
          <dd data-testid="local-hello-backend">{state.profile.backend_id}</dd>
        </div>
        <div className="field">
          <dt>Session</dt>
          <dd data-testid="local-hello-session">{state.profile.session_id}</dd>
        </div>
      </dl>
    </section>
  );
}

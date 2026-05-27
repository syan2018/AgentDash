// @ts-check

/**
 * @typedef {{ extensionId: string, label: string, panelPath: string, bridgeEndpoint: string }} PreviewHtmlOptions
 */

/**
 * @param {PreviewHtmlOptions} options
 * @returns {string}
 */
export function createPreviewHtml(options) {
  const config = JSON.stringify({
    extensionId: options.extensionId,
    label: options.label,
    panelPath: options.panelPath,
    bridgeEndpoint: options.bridgeEndpoint,
  });
  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>${escapeHtml(options.label)} · AgentDash Extension Preview</title>
    <style>
      :root {
        color-scheme: light;
        font-family: Inter, "Segoe UI", system-ui, sans-serif;
        background: #eef1f5;
        color: #1e293b;
      }
      * { box-sizing: border-box; }
      body {
        margin: 0;
        min-height: 100vh;
        background: linear-gradient(180deg, rgba(255,255,255,0.92), rgba(238,241,245,0.94)), #eef1f5;
      }
      .preview-shell {
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
        min-height: 100vh;
      }
      .topbar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        border-bottom: 1px solid #d8dee8;
        background: #ffffff;
        padding: 12px 18px;
      }
      .title {
        display: flex;
        align-items: baseline;
        gap: 10px;
        min-width: 0;
      }
      .title h1 {
        margin: 0;
        font-size: 15px;
        font-weight: 650;
      }
      .title span {
        color: #64748b;
        font-size: 12px;
      }
      .status {
        border: 1px solid #b7dfc4;
        border-radius: 999px;
        background: #ecfdf3;
        color: #166534;
        padding: 4px 10px;
        font-size: 12px;
        white-space: nowrap;
      }
      .content {
        display: grid;
        grid-template-columns: minmax(0, 1fr) 360px;
        gap: 14px;
        min-height: 0;
        padding: 14px;
      }
      .workspace {
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
        min-width: 0;
        min-height: 0;
        border: 1px solid #d8dee8;
        border-radius: 8px;
        background: #ffffff;
        overflow: hidden;
      }
      .workspace-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        border-bottom: 1px solid #e2e8f0;
        background: #f8fafc;
        padding: 9px 12px;
      }
      .workspace-header strong {
        font-size: 13px;
      }
      .workspace-header code {
        color: #64748b;
        font-size: 12px;
      }
      iframe {
        width: 100%;
        height: 100%;
        min-height: 520px;
        border: 0;
        background: #ffffff;
      }
      .side {
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
        min-height: 0;
        border: 1px solid #d8dee8;
        border-radius: 8px;
        background: #ffffff;
        overflow: hidden;
      }
      .side h2 {
        margin: 0;
        border-bottom: 1px solid #e2e8f0;
        background: #f8fafc;
        padding: 10px 12px;
        font-size: 13px;
      }
      .log {
        display: flex;
        flex-direction: column;
        gap: 8px;
        min-height: 0;
        overflow: auto;
        padding: 10px;
      }
      .entry {
        border: 1px solid #e2e8f0;
        border-radius: 8px;
        padding: 8px;
        background: #ffffff;
      }
      .entry[data-state="error"] {
        border-color: #fecaca;
        background: #fff1f2;
      }
      .entry header {
        display: flex;
        justify-content: space-between;
        gap: 8px;
        font-size: 12px;
        font-weight: 650;
      }
      .entry pre {
        margin: 6px 0 0;
        max-height: 160px;
        overflow: auto;
        color: #475569;
        font-size: 11px;
        line-height: 1.45;
        white-space: pre-wrap;
      }
      @media (max-width: 960px) {
        .content { grid-template-columns: 1fr; }
        .side { min-height: 260px; }
      }
    </style>
  </head>
  <body>
    <main class="preview-shell">
      <div class="topbar">
        <div class="title">
          <h1>AgentDash Extension Preview</h1>
          <span id="extension-name"></span>
        </div>
        <span class="status">dev runtime connected</span>
      </div>
      <section class="content">
        <section class="workspace" aria-label="Workspace panel preview">
          <div class="workspace-header">
            <strong id="panel-label"></strong>
            <code id="panel-path"></code>
          </div>
          <iframe id="extension-frame" title="Extension panel preview" sandbox="allow-scripts allow-same-origin"></iframe>
        </section>
        <aside class="side" aria-label="Bridge request log">
          <h2>Bridge Requests</h2>
          <div class="log" id="request-log"></div>
        </aside>
      </section>
    </main>
    <script type="module">
      const config = ${config};
      const frame = document.getElementById("extension-frame");
      const log = document.getElementById("request-log");
      document.getElementById("extension-name").textContent = config.extensionId;
      document.getElementById("panel-label").textContent = config.label;
      document.getElementById("panel-path").textContent = config.panelPath;
      frame.src = config.panelPath;

      window.addEventListener("message", (event) => {
        if (event.source !== frame.contentWindow) return;
        if (event.origin !== window.location.origin) return;
        const message = event.data;
        if (!message || message.channel !== "agentdash.extension" || message.kind !== "request") return;
        void handleBridgeRequest(message);
      });

      async function handleBridgeRequest(message) {
        const entry = appendLog(message.method, message.params, "pending");
        try {
          const response = await fetch(config.bridgeEndpoint, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ method: message.method, params: message.params ?? {} }),
          });
          const payload = await response.json();
          if (!response.ok || payload.error) {
            const error = payload.error ?? "Extension dev bridge request failed";
            entry.dataset.state = "error";
            entry.querySelector("span").textContent = "error";
            entry.querySelector("pre").textContent = String(error);
            frame.contentWindow?.postMessage({
              channel: "agentdash.extension",
              kind: "response",
              request_id: message.request_id,
              error: String(error),
            }, window.location.origin);
            return;
          }
          entry.dataset.state = "ok";
          entry.querySelector("span").textContent = "ok";
          entry.querySelector("pre").textContent = JSON.stringify(payload.result ?? null, null, 2);
          frame.contentWindow?.postMessage({
            channel: "agentdash.extension",
            kind: "response",
            request_id: message.request_id,
            result: payload.result ?? null,
          }, window.location.origin);
        } catch (error) {
          const messageText = error instanceof Error ? error.message : String(error);
          entry.dataset.state = "error";
          entry.querySelector("span").textContent = "error";
          entry.querySelector("pre").textContent = messageText;
          frame.contentWindow?.postMessage({
            channel: "agentdash.extension",
            kind: "response",
            request_id: message.request_id,
            error: messageText,
          }, window.location.origin);
        }
      }

      function appendLog(method, params, state) {
        const entry = document.createElement("article");
        entry.className = "entry";
        entry.dataset.state = state;
        entry.innerHTML = \`<header><strong></strong><span></span></header><pre></pre>\`;
        entry.querySelector("strong").textContent = method;
        entry.querySelector("span").textContent = state;
        entry.querySelector("pre").textContent = JSON.stringify(params ?? {}, null, 2);
        log.prepend(entry);
        return entry;
      }
    </script>
  </body>
</html>`;
}

/**
 * @param {string} value
 * @returns {string}
 */
function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

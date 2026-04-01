# Live Canvas Runtime Contract / Loader Brief

## 当前定位

1. Canvas 走独立 runtime snapshot API，`canvas_presented` 事件只负责告知 SketchView 要打开哪个画布。
2. Runtime 是自研 `iframe sandbox`，首版无需 Sandpack，目标聚焦“受控白名单 + import map + esbuild-wasm”这类命中率高的执行路径。

## 最小 Runtime Contract

### 1) Snapshot Payload

- 接口：`GET /api/canvases/{canvasId}/runtime-snapshot?session_id={sessionId}`。
- 返回 JSON 包含：
  - `files`: `{ path: string, content: string, type: "code" | "data" }[]`，code 走 compile path，data 纯 JSON 供 `bindings` 访问。
  - `entry`: 相对路径（必须在 files 里存在），用来构建 iframe bootstrap。
  - `bindings`: `{ alias: string, mountPath: string, dataPath: string }[]`，前端可把 `/bindings/{alias}.json` 重定向到 this snapshot 中对应 data file。
  - `importMap`: `{ imports: Record<string,string> }`，由后端根据 Project 白名单生成。
  - `libraries`: `string[]`（可选），用于搭配 CDN 路径做延迟加载提示。

验收点：调用该接口能拿到完整文件+数据组合，`entry` 和 `bindings` 在前端 1:1 映射。

### 2) Iframe Bootstrap

- 构造一个 `srcdoc` / sandbox iframe，在 head 注入：
  - `<script type="importmap">` （来自 snapshot.importMap）。
  - 受控的 `window.__CANVAS_RUNTIME__`，包含 `bindings` 和 `notifyParent`。
  - 加载 `esbuild-wasm`（见下面）并在 worker 内编译 `entry` 里的 TS/TSX。
  - 编译结果挂在 `run()` 入口，`run()` 被自动调用并与 parent 通信。

需要注意：iframe 的 `sandbox="allow-scripts"`，额外给一个 `allow="clipboard-read; clipboard-write"` 视乎是否必要，**绝不加** `allow-same-origin`。

### 3) postMessage 协议

- **父 → iframe**：
  - `type: "snapshot"`（payload: snapshot metadata）用于初始装载。
  - `type: "refresh"`（optional changed files list）用于背后更新时让 iframe 自行 reload / patch。
  - `type: "destroy"` 用于关闭时释放资源。
- **iframe → 父**：
  - `type: "ready"`（entry mounted）。
  - `type: "render_error"`（message, stack, code, filePath）。
  - `type: "console"` 可选，用于 debug（level, args）。
  - `type: "data_request"`（alias），若 Canvas 需要新数据可主动引导父级拉 snapshot。

父端 `SessionPage`（当前 `handleSystemEvent` 已拿到 `update`）可在 `canvas_presented` 后建立 iframe 并监听。

### 4) 错误语义

- 编译错误：iframe 发送 `render_error`，含 `code: "compile"`，并返回 `filePath` 与 `line`.
- 运行时错误：同样 `render_error`，`code: "runtime"`，Parent 展示为 “Canvas 运⾏时报错”。
- 依赖加载失败：iframe 捕获 `import()` 拦截，发送 `render_error` + `code: "dependency"`。
- 绑定数据缺失：父端在 snapshot 生成时预检测，若缺 alias 抛 400；iframe 不可自己 fetch canvas mount 之外的资源。

## esbuild-wasm 评估

- `frontend/package.json` 目前没有 bundler runtime，已有脚本都是 dev time。为了在浏览器内编译 TS/TSX，需要引入 `esbuild-wasm`（`npm` 依赖）。
- 影响文件：
  - 运行时 iframe bootstrap 工程化代码（建议新建 `frontend/src/features/live-canvas/runtime/loader.ts`）。
  - 可能需要 `frontend/src/features/live-canvas/runtime/esbuild-worker.ts` 来 orchestrate `esbuild`.
  - `SessionPage` 层只需接收 ready/error，无需直接引用 esbuild。
- 结合现有 stack：`pnpm run build` 依旧不变，esbuild-wasm 只在浏览器 runtime 下载（可向 CDN）。
- Spike 建议：1 周内实现一个 demo iframe，直接在 `SessionPage` 右侧以 `esbuild-wasm` 编译一个简单 `tsx` entry，验证 init/ready/error 流。

## 支持 / 不支持策略

### 支持

- 文件类型：`ts`/`tsx`/`js`/`jsx`/`json`/`html`/`css`/`md`（作为静态内容，需手动 import）。
- 依赖：只允许 `import` 映射到白名单 CDN（React、ReactDOM、ECharts、@tanstack/table-core、dayjs 之类），由后端 `importMap` 控制。
- 静态资源：图片、字体、二进制仅通过 `data` 文件提供 base64 内容，或由 Canvas 代码通过 `fetch("/assets/...")`（CSP 控制）加载。

### 不支持

- `node:` 模块、动态 `import()` 指向任意 npm。
- CSS / 资源文件自动处理（先手动 embed / via `import` ）。
- 运行时 `npm install`、HMR、WebWorker 混合（首版直接整页 reload）。

## 1 周 Spike 建议

1. 在 `frontend/src/features/live-canvas/runtime/spike.ts` 写一个 loader，按 snapshot 构造 `esbuild-wasm` 编译 pipeline。
2. 用 `SessionPage` 旁边的“抽屉” slot 渲染一个 demo Canvas（`<CanvasPanel>`），只需加载一个 `entry` 输出 “Hello Canvas”。
3. 验证 postMessage（ready/error）和运行时 CSS/React 加载，记录感知延迟与错误格式。

Spike 完成后，我们有实际 loader 模板、postMessage 事件、以及 `esbuild-wasm` 下载/初始化流程，可以快速推动正式分支。

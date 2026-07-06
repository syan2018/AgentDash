// @ts-check

import test from "node:test";
import assert from "node:assert/strict";

import {
  createRoutedFetch,
  describeFetchRouteTarget,
  matchFetchRoute,
  parseFetchRouteBinding,
} from "./fetch-route.js";

test("parseFetchRouteBinding requires an explicit target kind", () => {
  assert.throws(
    () => parseFetchRouteBinding("/api/**=api"),
    /target 必须显式声明/,
  );
});

test("matchFetchRoute matches explicit relative and absolute patterns", () => {
  const api = parseFetchRouteBinding("/api/**=httpProxy:https://api.example.com");
  assert.equal(describeFetchRouteTarget(api.target), "httpProxy:https://api.example.com");
  assert.equal(matchFetchRoute("/api/users", [api], { baseUrl: "https://panel.local/" })?.path, "/api/users");
  assert.equal(matchFetchRoute("/assets/app.js", [api], { baseUrl: "https://panel.local/" }), null);

  const local = parseFetchRouteBinding(
    "http://localhost:4510/api/**=httpProxy:http://localhost:4510",
  );
  assert.equal(matchFetchRoute("http://localhost:4510/api/users", [local])?.route.target.kind, "http_proxy");
  assert.equal(matchFetchRoute("http://localhost:4511/api/users", [local]), null);
});

test("backendService fetch routes accept generated namespaced service keys", async () => {
  const route = parseFetchRouteBinding("/api/**=backendService:repo-tools.api");
  assert.equal(describeFetchRouteTarget(route.target), "backendService:repo-tools.api");
  const routedFetch = createRoutedFetch(
    [route],
    {
      async invokeFetchRoute(request) {
        assert.equal(request.route.target.kind, "backend_service");
        assert.equal(request.route.target.service_key, "repo-tools.api");
        assert.equal(request.url, "https://panel.local/api/search");
        return { status: 204 };
      },
    },
    { baseUrl: "https://panel.local/" },
  );

  const response = await routedFetch("/api/search");

  assert.equal(response.status, 204);
  assert.equal(await response.text(), "");
});

test("createRoutedFetch is opt-in and does not replace global fetch", async () => {
  const originalFetch = globalThis.fetch;
  const route = parseFetchRouteBinding("/api/**=customChannel:demo.api#fetch");
  const routedFetch = createRoutedFetch(
    [route],
    {
      async invokeFetchRoute(request) {
        assert.equal(request.route.target.kind, "custom_channel");
        assert.equal(request.method, "POST");
        assert.equal(request.url, "https://panel.local/api/users");
        assert.equal(request.headers["x-demo"], "yes");
        assert.equal(request.body, "payload");
        return {
          status: 202,
          headers: { "content-type": "application/json" },
          body: '{"ok":true}',
        };
      },
    },
    { baseUrl: "https://panel.local/" },
  );

  assert.equal(globalThis.fetch, originalFetch);
  const response = await routedFetch("/api/users", {
    method: "POST",
    headers: { "x-demo": "yes" },
    body: "payload",
  });
  assert.equal(response.status, 202);
  assert.deepEqual(await response.json(), { ok: true });
  assert.equal(globalThis.fetch, originalFetch);
});

test("createRoutedFetch reads Request POST body and headers", async () => {
  const route = parseFetchRouteBinding("/api/**=backendService:repo-tools.api");
  const routedFetch = createRoutedFetch(
    [route],
    {
      async invokeFetchRoute(request) {
        assert.equal(request.route.target.kind, "backend_service");
        assert.equal(request.method, "POST");
        assert.equal(request.headers["content-type"], "text/plain;charset=UTF-8");
        assert.equal(request.body, "payload");
        return { status: 200, body: "ok" };
      },
    },
    { baseUrl: "https://panel.local/" },
  );

  const request = new Request("https://panel.local/api/users", {
    method: "POST",
    headers: { "content-type": "text/plain;charset=UTF-8" },
    body: "payload",
  });
  const response = await routedFetch(request);

  assert.equal(response.status, 200);
  assert.equal(await response.text(), "ok");
});

test("createRoutedFetch drops response bodies for no-body statuses", async () => {
  const route = parseFetchRouteBinding("/api/**=backendService:repo-tools.api");
  for (const status of [204, 205, 304]) {
    const routedFetch = createRoutedFetch(
      [route],
      {
        async invokeFetchRoute() {
          return { status, body: "ignored" };
        },
      },
      { baseUrl: "https://panel.local/" },
    );

    const response = await routedFetch("/api/search");

    assert.equal(response.status, status);
    assert.equal(await response.text(), "");
  }
});

test("createRoutedFetch rejects undeclared route without fallback", async () => {
  const route = parseFetchRouteBinding("/api/**=backendService:repo-tools.api");
  const routedFetch = createRoutedFetch(
    [route],
    {
      async invokeFetchRoute() {
        throw new Error("unexpected bridge invoke");
      },
    },
    { baseUrl: "https://panel.local/" },
  );

  await assert.rejects(
    () => routedFetch("/private/search"),
    /fetch route 未声明: \/private\/search/,
  );
});

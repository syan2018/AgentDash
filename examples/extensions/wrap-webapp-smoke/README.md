# Wrap Web App Smoke Fixture

This fixture is a prebuilt static web app dist for `agentdash-ext wrap-webapp`.

Run from the repository root:

```powershell
node packages/extension/src/cli/agentdash-ext.js wrap-webapp --dist examples/extensions/wrap-webapp-smoke/dist-static --extension-id wrap-smoke --name "Wrap Smoke"
```

API route smoke:

```powershell
node packages/extension/src/cli/agentdash-ext.js wrap-webapp --dist examples/extensions/wrap-webapp-smoke/dist-api --extension-id wrap-api-smoke --name "Wrap API Smoke" --fetch-route "/api/**=httpProxy:https://api.example.com"
```

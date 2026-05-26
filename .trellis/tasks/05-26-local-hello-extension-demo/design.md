# Design

## Directory

```text
examples/extensions/local-hello/
  package.json
  agentdash.extension.json
  src/extension.ts
  src/panel/App.tsx
  src/panel/main.tsx
  src/shared/schema.ts
  tests/extension.test.ts
  README.md
```

## Extension Behavior

- registers `local-hello.profile`
- permission: `local.profile.read`
- handler calls `api.local.getProfile()`
- panel calls `invokeAction("local-hello.profile", {})`
- panel renders profile fields and error state

## E2E

Test must use packaged artifact:

```text
pack archive -> upload/install -> restart/refresh session -> open plugin tab -> invoke action
```

The test should fail if it only works through local dev ref.

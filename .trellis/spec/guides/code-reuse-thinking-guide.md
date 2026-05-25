# Code Reuse Thinking Guide

Use this guide before adding new utilities, constants, mappers, services, UI primitives, or repeated logic.

## Search First

```powershell
rg -n "functionName|constantName|domainTerm"
rg -n "similar keyword"
```

## Questions

| Question | If yes |
| --- | --- |
| Does a similar function or component already exist? | Use or extend it. |
| Is this pattern repeated three or more times? | Consider extracting a shared abstraction. |
| Is this a shared constant or contract value? | Move it to the owning module or generated contract. |
| Would extraction obscure a simple one-off? | Keep it local. |

## After Batch Changes

- Search for missed instances.
- Check whether similar files still diverge.
- If a reusable rule was learned, update the owning architecture or appendix; do not add task-process notes to spec.

# WI-03 Owner Persistence / Migration

Status: planned

Depends On: WI-02

## Scope

- LifecycleRun typed registry schema/mutation migration。
- owner-local ChannelKey uniqueness/create-if-absent。
- Project owner store contract 与 Project Assets boundary。
- external binding reverse index 设计与 migration。
- owner/ref/locator consistency tests。

## Exit Criteria

- owner document 更新原子且 broad aggregate update 不覆盖 registry。
- inbound binding 不扫描全部 owner documents。
- 没有临时 ProjectConfig/global table fallback。
- 无旧 schema decoder 或双 authority。

## Validation

- migration guard/clean database。
- repository roundtrip/concurrency/uniqueness tests。
- owner router/reverse index tests。

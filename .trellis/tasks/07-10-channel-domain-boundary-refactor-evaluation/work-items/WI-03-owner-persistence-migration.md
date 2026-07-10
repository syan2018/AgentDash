# WI-03 Owner Persistence / Migration

Status: done

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

## Implementation Evidence

- migration `0061_reset_channel_registry_v2.sql` 将 column default 与全部预发布 owner documents 原子重建为显式 V2，不保留旧 schema decoder。
- PostgreSQL owner-document mutation 继续通过 `SELECT ... FOR UPDATE` 与 typed mutation closure 写回目标列；broad aggregate update 不覆盖 registry。
- repository 增加并发 `CreateChannelIfAbsent` 验证，同一 owner-local key 只产生一个 record；Project owner 继续只保留 Project Assets 所需 port contract。
- external binding reverse index 与 provider router 在 WI-04 以明确 provider/index contract 落地，不扫描 owner documents。

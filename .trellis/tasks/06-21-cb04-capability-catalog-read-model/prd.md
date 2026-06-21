# CB04-D Capability catalog read model split

## Goal

将 capability catalog 查询改为 application read model，再由 API adapter 映射 contract DTO。

## Requirements

- Capability catalog query 返回 backend-owned application read model。
- API adapter 映射 `CapabilityCatalogResponse` 和相关 tool/source/scope DTO。
- SPI/platform capability facts 不直接构造 browser-facing generated DTO。

## Acceptance Criteria

- [ ] application capability catalog service 不返回 contract DTO。
- [ ] API route owns final contract DTO mapping。
- [ ] tool descriptor/source/scope projection 行为保持一致。
- [ ] focused tests 覆盖 application read model 与 API DTO mapping。

## Notes

- Medium-risk task; can run after first wave if catalog files are not touched by other workers.

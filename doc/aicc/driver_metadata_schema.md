# AICC Driver Metadata Schema

Driver metadata turns provider-discovered model ids into AICC `ModelMetadata`.
Provider `/models` discovery is only the id source; the resolver owns capability,
API type, mount, cost, latency, and conservative fallback decisions.

## Source Priority

The resolver loads metadata in this override order:

1. builtin metadata bundled under `src/frame/aicc/driver_metadata/`
2. remote cache: `$BUCKYOS_ROOT/etc/aicc/driver_metadata/remote_cache/<driver>.json`
3. local override: `$BUCKYOS_ROOT/etc/aicc/driver_metadata/local/<driver>.json`
4. system-config override materialized at `$BUCKYOS_ROOT/etc/aicc/driver_metadata/system-config/<driver>.json`

For each model id, match priority is:

1. exact `models[].id`
2. wildcard `patterns[].pattern`
3. `defaults`
4. conservative fallback

Exact matches win before patterns, even if the pattern comes from a higher
priority override.

## Document

```json
{
  "schema_version": 1,
  "provider_driver": "openai",
  "revision": "builtin-2026-05-30",
  "models": [],
  "patterns": [],
  "defaults": {},
  "variants": [],
  "signature": null
}
```

Fields:

- `schema_version`: currently `1`.
- `provider_driver`: driver id such as `openai`, `claude`, `google-gemini`, `fal`, `minimax`.
- `revision`: monotonically changing metadata revision string.
- `models`: exact rules keyed by `id`.
- `patterns`: wildcard rules keyed by `pattern`; `*` is the only wildcard.
- `defaults`: default rule when no exact or pattern rule matches.
- `variants`: optional provider option variants for later mount/lowering.
- `signature`: optional signature envelope; verification is not enforced yet.

## Rule

Rules support these fields:

- `id`: exact provider model id for `models`.
- `pattern`: wildcard provider model id pattern for `patterns`.
- `exclude`: drops the provider model from inventory.
- `parameter_scale`: optional display/classification string.
- `api_types`: AICC API types, for example `llm.chat`, `image.txt2img`, `audio.asr`.
- `logical_mounts`: logical mounts. Templates `{driver}`, `{model}`, and `{provider_model_id}` are expanded by the resolver.
- `capabilities`: partial capability patch: `streaming`, `tool_call`, `json_schema`, `web_search`, `vision`, `max_context_tokens`, `max_output_tokens`.
- `estimated_cost_usd`, `estimated_latency_ms`: default scheduler estimates.
- `quality_score`, `latency_class`, `cost_class`: routing attributes.

Unknown fallback is intentionally conservative: it does not declare
`tool_call`, `web_search`, `vision`, or `json_schema`.

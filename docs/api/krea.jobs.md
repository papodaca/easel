# Krea jobs

Sources: [Job lifecycle](https://www.krea.ai/docs/developers/job-lifecycle), [Deprecations](https://www.krea.ai/docs/developers/deprecations), and `GET /jobs/{id}` in the [Krea OpenAPI](https://api.krea.ai/openapi.json).

`GET /jobs/{id}` with `Authorization: Bearer <token>` returns the job. `job_id` is a UUID. `status` is one of `backlogged`, `queued`, `scheduled`, `processing`, `sampling`, `intermediate-complete`, `completed`, `failed`, `cancelled`.

`backlogged` means the job is waiting behind the account concurrency limit. Poll every 2–5 seconds while the status is `backlogged`, `queued`, `scheduled`, `processing`, `sampling`, or `intermediate-complete`. Stop on `completed`, `failed`, or `cancelled`.

On success the image is `result.urls[0]` when `urls` is an array of URI strings. The schema also allows an array of `{type, url}` objects (`type` is `model` or `preview`) or a map of names to URIs. Failures use the sibling `error` object: `code` (required) and optional `message`. `progress` is a number from 0 to 1, or null.

`DELETE /jobs/{id}` cancels a job. The lifecycle page says cancellation is only accepted while the status is `queued` or `processing`.

A deprecated model sends `Deprecation: true` and `Sunset` (an RFC 1123 date) on successful responses. After sunset the call returns `410 Gone`:

```json
{
  "error": "model_unavailable",
  "reason": "deprecated",
  "message": "...",
  "sunset_date": "2026-04-27",
  "replacement": "video/bytedance/seedance-1.0-pro-fast",
  "migration_url": "https://docs.krea.ai/developers/deprecations#seedance-1-0-lite"
}
```

`replacement` and `migration_url` are present when Krea has a replacement. `error` is the stable switch. A 410 is terminal.

`GET /jobs/{id}` documents 404, not 401, when the id is missing or the caller cannot see it. A valid key against a random UUID is therefore not a 401. That is the cheap key check, parallel to BFL's `Task not found`.

Result downloads in the fixtures use `https://cdn.krea.ai/`, which is already allowed by the `.krea.ai` suffix. No other result host was recorded, because this landing did not run a paid live job. FLUX.2 [pro] `safety_tolerance` is capped at 5 in the app even though the OpenAPI maximum is 7. Krea 3 `resolution` is sent as `1K`/`2K`.

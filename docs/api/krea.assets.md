# Krea assets

Sources: [Upload an asset](https://www.krea.ai/docs/api-reference/assets/upload-an-asset). The `image_urls` wording is the Krea 3 operation in [the Krea OpenAPI](https://api.krea.ai/openapi.json), saved in `docs/api/krea.3.md`.

`POST /assets` is `multipart/form-data` with bearer auth. The `file` part is required and at most 75 MB. Accepted types include JPEG, PNG, WebP, HEIC, MP4, MOV, WebM, GLB, WAV, and MP3. `description` is optional.

A 200 body contains:

| Field | Notes |
| --- | --- |
| `id` | UUID |
| `image_url` | URI of the stored asset |
| `uploaded_at` | Timestamp |
| `width`, `height` | Numbers, or null |
| `size_bytes` | Number, or null |
| `mime_type` | String, or null |
| `description` | Optional |

Generation fields such as `image_urls` accept three kinds of entry: an external URL, a `data:` URI, or an asset URL from this upload.

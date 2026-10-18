# Google image models

One-shot generate and edit go to `POST https://generativelanguage.googleapis.com/v1beta/interactions`.

The key is the `x-goog-api-key` header. It is not a query parameter. The request does not send `Api-Revision`, `delivery`, or `background`.

## Shared body

| Field | Value |
| --- | --- |
| `model` | The selected spec id |
| `store` | `false` |
| `input` | A text block for the prompt, then one image block per reference. The canvas image is first. Each image block has `mime_type` and base64 `data`. At most 14 images. |
| `response_format.type` | `image` |
| `response_format.aspect_ratio` | The selected ratio |
| `response_format.image_size` | The selected size, with an uppercase K (`1K`, `2K`, `4K`) |

Aspect ratio and size are not top-level keys. The prep field in the form is still `resolution`. `4K` is the only size that prepares references at 16 megapixels. Every other size stays at 4 megapixels.

## Nano Banana 2.1

Id: `gemini-nano-banana-2.1`

- Image size: `1K`, `2K`, `4K`
- Aspect ratio: `1:1`, `2:3`, `3:2`, `3:4`, `4:3`, `4:5`, `5:4`, `9:16`, `16:9`, `21:9`, `1:4`, `4:1`, `1:8`, `8:1`
- Search grounding: optional. On sends `tools` with one `google_search` object and no `search_types`. Off omits `tools`.

## Nano Banana 2 Lite

Id: `gemini-3.1-flash-lite-image`

- Image size: `1K`
- Aspect ratio: the ten ratios above, without `1:4`, `4:1`, `1:8`, and `8:1`
- No grounding field, and the body has no `tools` key

## Nano Banana Pro

Id: `gemini-3-pro-image`

- Image size: `1K`, `2K`, `4K`
- Aspect ratio: the same ten as Lite
- Search grounding: same `tools` rule as 2.1

None of the three send `image_search`.

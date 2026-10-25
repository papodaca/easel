# Image Generation

Source: https://apiframe.ai/docs/images



`POST /v2/images/generate`

Submit a prompt to generate images. The request returns immediately with a job ID — poll the [Jobs endpoint](/docs/jobs/get) or use [webhooks](/docs/webhooks) to get the result.

Common request body [#common-request-body]

| Field           | Type      | Required | Description                                                     |
| --------------- | --------- | -------- | --------------------------------------------------------------- |
| `prompt`        | string    | Yes      | The image description (1–2,000 characters)                      |
| `model`         | string    | Yes      | The model identifier (see available models below)               |
| `webhookUrl`    | string    | No       | URL to receive webhook notifications                            |
| `webhookEvents` | string\[] | No       | Events to subscribe to: `"progress"`, `"completed"`, `"failed"` |

Each model also accepts its own parameters object — see the individual model pages for details.

Idempotency [#idempotency]

Include an `Idempotency-Key` header to prevent duplicate jobs if the same request is sent multiple times.

Response [#response]

**Status: `202 Accepted`**

```json
{
  "jobId": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "status": "QUEUED"
}
```

Result format [#result-format]

When the job completes, the `result` field on [`GET /v2/jobs/:id`](/docs/jobs/get) (and in the `completed` [webhook](/docs/webhooks) payload) is an object with CDN-hosted image URLs:

```json
{
  "images": [
    "https://cdn2.apiframe.ai/images/a1b2c3d4-e5f6-7890-abcd-ef1234567890-1.png"
  ]
}
```

| Field     | Type      | Description                                                                                       |
| --------- | --------- | ------------------------------------------------------------------------------------------------- |
| `images`  | string\[] | One CDN URL per generated image. Models that support multiple outputs return one entry per image. |
| `gridUrl` | string    | Midjourney only — a single 2×2 composite of the 4 generated images. Omitted for all other models. |

Each model page shows an example result. Result files are hosted on the APIFRAME CDN for **90 days** — download anything you want to keep.

Available models [#available-models]

| Model                                                                                              | Identifier                           | Description                                                                  |
| -------------------------------------------------------------------------------------------------- | ------------------------------------ | ---------------------------------------------------------------------------- |
| [Midjourney](/docs/images/midjourney)                                                              | `"midjourney"`                       | High-quality image generation from text prompts                              |
| [Nano Banana](/docs/images/nano-banana/nano-banana)                                                | `"nano-banana"`                      | Fast generation with image-to-image support                                  |
| [Nano Banana Pro](/docs/images/nano-banana/nano-banana-pro)                                        | `"nano-banana-pro"`                  | Professional-grade with accurate text, up to 4K                              |
| [Nano Banana 2](/docs/images/nano-banana/nano-banana-2)                                            | `"nano-banana-2"`                    | High-efficiency with Google Search grounding, up to 4K                       |
| [Nano Banana 2 Lite](/docs/images/nano-banana/nano-banana-2-lite)                                  | `"nano-banana-2-lite"`               | Fastest, cheapest Nano Banana for high-volume 1K generation                  |
| [Flux 1.1 Pro](/docs/images/flux/flux-1-1-pro)                                                     | `"flux-1.1-pro"`                     | Fast, high-quality generation with excellent prompt adherence                |
| [Flux 1.1 Pro Ultra](/docs/images/flux/flux-1-1-pro-ultra)                                         | `"flux-1.1-pro-ultra"`               | High-resolution (up to 4MP) with raw mode                                    |
| [Flux 2 Pro](/docs/images/flux/flux-2-pro)                                                         | `"flux-2-pro"`                       | Image generation and editing with multi-reference support                    |
| [Flux 2 Dev](/docs/images/flux/flux-2-dev)                                                         | `"flux-2-dev"`                       | Open-weight generation and editing, optimized for speed                      |
| [Flux 2 Flex](/docs/images/flux/flux-2-flex)                                                       | `"flux-2-flex"`                      | Specialized for typography and fine detail                                   |
| [Flux 2 Max](/docs/images/flux/flux-2-max)                                                         | `"flux-2-max"`                       | Highest fidelity model with best editing consistency                         |
| [Seedream 3](/docs/images/seedream/seedream-3)                                                     | `"seedream-3"`                       | High-resolution (2K) text-to-image by ByteDance                              |
| [Seedream 3.1](/docs/images/seedream/seedream-3-1)                                                 | `"seedream-3.1"`                     | Cinematic-quality with style control (Dreamina 3.1)                          |
| [Seedream 4](/docs/images/seedream/seedream-4)                                                     | `"seedream-4"`                       | Generation and editing with multi-reference, up to 4K                        |
| [Seedream 4.5](/docs/images/seedream/seedream-4-5)                                                 | `"seedream-4.5"`                     | Stronger spatial understanding, up to 4K                                     |
| [Seedream 5 Lite](/docs/images/seedream/seedream-5-lite)                                           | `"seedream-5-lite"`                  | Latest model with reasoning and example-based editing                        |
| [Seedream 5.0 Pro](/docs/images/seedream/seedream-5-pro)                                           | `"seedream-5-pro"`                   | Flagship 1K/2K generation with up to 10 reference images                     |
| [Ideogram V2](/docs/images/ideogram/ideogram-v2)                                                   | `"ideogram-v2"`                      | High-quality generation with superior text rendering                         |
| [Ideogram V2 Turbo](/docs/images/ideogram/ideogram-v2-turbo)                                       | `"ideogram-v2-turbo"`                | Faster V2 variant optimized for speed                                        |
| [Ideogram V2a](/docs/images/ideogram/ideogram-v2a)                                                 | `"ideogram-v2a"`                     | Cost-efficient generation with strong text rendering                         |
| [Ideogram V2a Turbo](/docs/images/ideogram/ideogram-v2a-turbo)                                     | `"ideogram-v2a-turbo"`               | Fastest and most affordable Ideogram model                                   |
| [Ideogram V3 Balanced](/docs/images/ideogram/ideogram-v3-balanced)                                 | `"ideogram-v3-balanced"`             | Balanced quality/speed with style presets and references                     |
| [Ideogram V3 Turbo](/docs/images/ideogram/ideogram-v3-turbo)                                       | `"ideogram-v3-turbo"`                | Fast V3 with style presets and references                                    |
| [Ideogram V3 Quality](/docs/images/ideogram/ideogram-v3-quality)                                   | `"ideogram-v3-quality"`              | Highest-quality V3 for production-ready images                               |
| [Ideogram V4 Balanced](/docs/images/ideogram/ideogram-v4-balanced)                                 | `"ideogram-v4-balanced"`             | Balanced V4 quality/speed with sharper text rendering                        |
| [Ideogram V4 Turbo](/docs/images/ideogram/ideogram-v4-turbo)                                       | `"ideogram-v4-turbo"`                | Fast, affordable V4 for quick iterations                                     |
| [Ideogram V4 Quality](/docs/images/ideogram/ideogram-v4-quality)                                   | `"ideogram-v4-quality"`              | Highest-quality V4 for production-ready images                               |
| [Ideogram Character](/docs/images/ideogram/ideogram-character)                                     | `"ideogram-character"`               | Character-consistent generation with reference images                        |
| [Stable Diffusion 3.5 Large](/docs/images/stable-diffusion/stable-diffusion-3-5-large)             | `"stable-diffusion-3.5-large"`       | Stability AI's highest-quality SD 3.5 tier for detailed images               |
| [Stable Diffusion 3.5 Large Turbo](/docs/images/stable-diffusion/stable-diffusion-3-5-large-turbo) | `"stable-diffusion-3.5-large-turbo"` | Near-Large quality in a few steps for fast generation                        |
| [Stable Diffusion 3.5 Medium](/docs/images/stable-diffusion/stable-diffusion-3-5-medium)           | `"stable-diffusion-3.5-medium"`      | Smaller, efficient SD 3.5 balancing quality and speed                        |
| [Stable Diffusion 3](/docs/images/stable-diffusion/stable-diffusion-3)                             | `"stable-diffusion-3"`               | Original SD 3 Medium MMDiT with strong prompt adherence                      |
| [Imagen 3](/docs/images/imagen/imagen-3)                                                           | `"imagen-3"`                         | Google's highest quality text-to-image with rich lighting and detail         |
| [Imagen 3 Fast](/docs/images/imagen/imagen-3-fast)                                                 | `"imagen-3-fast"`                    | Faster, cheaper Imagen 3 for speed-sensitive use cases                       |
| [Imagen 4](/docs/images/imagen/imagen-4)                                                           | `"imagen-4"`                         | Google's flagship Imagen 4 with superior typography and detail               |
| [Imagen 4 Ultra](/docs/images/imagen/imagen-4-ultra)                                               | `"imagen-4-ultra"`                   | Ultra quality Imagen 4 for maximum fidelity                                  |
| [Imagen 4 Fast](/docs/images/imagen/imagen-4-fast)                                                 | `"imagen-4-fast"`                    | Fast Imagen 4 optimized for speed and cost                                   |
| [DALL-E 3](/docs/images/dalle/dall-e-3)                                                            | `"dall-e-3"`                         | OpenAI's latest with improved detail, text rendering, and prompt adherence   |
| [DALL-E 2](/docs/images/dalle/dall-e-2)                                                            | `"dall-e-2"`                         | OpenAI's classic image generation from text descriptions                     |
| [GPT Image 1.5](/docs/images/gpt-image/gpt-image-1-5)                                              | `"gpt-image-1.5"`                    | OpenAI's latest with tiered quality, editing, and multi-image output         |
| [Kling Image](/docs/images/kling-image/kling-image)                                                | `"kling-image"`                      | Kling's image generation with reference images and multi-image output        |
| [Qwen Image](/docs/images/qwen-image/qwen-image)                                                   | `"qwen-image"`                       | Qwen v1 with text-to-image, image-to-image, and image editing                |
| [Qwen Image 2](/docs/images/qwen-image/qwen-image-2)                                               | `"qwen-image-2"`                     | Qwen v2 unified generation and editing model                                 |
| [Qwen Image 2 Pro](/docs/images/qwen-image/qwen-image-2-pro)                                       | `"qwen-image-2-pro"`                 | Qwen v2 Pro with higher fidelity output                                      |
| [Wan Image 2.6](/docs/images/wan-image/wan-image-2-6)                                              | `"wan-image-2.6"`                    | Alibaba's Wan 2.6 image generation and editing model                         |
| [Wan Image 2.7](/docs/images/wan-image/wan-image-2-7)                                              | `"wan-image-2.7"`                    | Alibaba's Wan 2.7 image generation and editing with thinking mode            |
| [Wan Image 2.7 Pro](/docs/images/wan-image/wan-image-2-7-pro)                                      | `"wan-image-2.7-pro"`                | Wan 2.7 Pro with higher fidelity output                                      |
| [Grok Image 2](/docs/images/grok-imagine/grok-image)                                               | `"grok-image"`                       | xAI Imagine Image 2.0. Instruction-following generation and editing up to 2K |
| [Grok Image Fast](/docs/images/grok-imagine/grok-imagine-image)                                    | `"grok-imagine-image"`               | xAI's fast image generation with strong creative control and text rendering  |
| [Reve Create](/docs/images/reve/reve-create)                                                       | `"reve-create"`                      | High-quality generation with stunning aesthetics and text rendering          |

Error responses [#error-responses]

| Status | Error                | Meaning                              |
| ------ | -------------------- | ------------------------------------ |
| `400`  | Validation error     | Invalid parameters — check `details` |
| `401`  | Unauthorized         | Missing or invalid API key           |
| `402`  | Insufficient credits | Not enough credits to run this model |
| `503`  | Service unavailable  | Job queue is temporarily down        |

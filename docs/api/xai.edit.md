#### Model Capabilities

# Image Editing

Edit an existing image by providing a source image along with your prompt. The model understands the image content and applies your requested changes.

> [!WARNING]
>
> The OpenAI SDK's `images.edit()` method is not supported for image editing because it uses `multipart/form-data`, while the xAI API requires `application/json`. Use the xAI SDK, Vercel AI SDK, or direct HTTP requests instead.

With the xAI SDK, use the same `sample()` method; just add the `image_url` parameter:

```javascript customLanguage="javascriptAISDK"
import { xai } from "@ai-sdk/xai";
import { generateImage } from "ai";
import fs from "fs";

// Load image and encode as base64
const imageBuffer = fs.readFileSync("photo.png");
const base64Image = imageBuffer.toString("base64");

const { image } = await generateImage({
    model: xai.image("grok-imagine-image-2.0"),
    prompt: {
        text: "Render this as a pencil sketch with detailed shading",
        images: [`data:image/png;base64,${base64Image}`],
    },
});

console.log(image.base64);
```

```bash
# Using a public URL as the source image
curl -X POST https://api.x.ai/v1/images/edits \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $XAI_API_KEY" \
  -d '{
    "model": "grok-imagine-image-2.0",
    "prompt": "Render this as a pencil sketch with detailed shading",
    "image": {
      "url": "https://docs.x.ai/assets/api-examples/images/style-realistic.png",
      "type": "image_url"
    }
  }'
```

```python customLanguage="pythonXAI"
import base64
import xai_sdk

client = xai_sdk.Client()

# Load image from file and encode as base64
with open("photo.png", "rb") as f:
    image_data = base64.b64encode(f.read()).decode("utf-8")

response = client.image.sample(
    prompt="Render this as a pencil sketch with detailed shading",
    model="grok-imagine-image-2.0",
    image_url=f"data:image/png;base64,{image_data}",
)

print(response.url)
```

You can provide the source image as:

* A **public URL** pointing to an image
* A **base64-encoded data URI** (e.g., `data:image/jpeg;base64,...`)
* A **`file_id`** from the [Files API](/developers/files) — see [Imagine → Files API Integration](/developers/model-capabilities/imagine/files/inputs)

## Multi-turn editing

Chain multiple edits together by using each output as the input for the next. This enables iterative refinement; start with a base image and progressively add details, adjust styles, or make corrections.

## Style transfer

The `grok-imagine-image-2.0` model supports a wide range of visual styles, from ultra-realistic photography to anime, oil paintings, and pencil sketches. Transform existing images by describing the desired aesthetic in your prompt.

## Related

* [Image Generation](/developers/model-capabilities/images/generation) — Generate images from text prompts
* [Multi-Image Editing](/developers/model-capabilities/images/multi-image-editing) — Edit with multiple source images
* [Image Prompting Guide](/developers/model-capabilities/images/prompting-guide) — When to edit, and how to phrase an edit prompt
* [API Reference](/developers/rest-api-reference) — Full endpoint documentation
* [Imagine API Landing Page](https://x.ai/api/imagine) — Showcase of the Imagine API in action

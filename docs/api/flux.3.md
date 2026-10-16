> ## Documentation Index
> Fetch the complete documentation index at: https://docs.bfl.ml/llms.txt
> Use this file to discover all available pages before exploring further.

# Generate an image with FLUX 3

> Generate or edit images with FLUX 3 and retrieve the asynchronous result.

<RequestExample>
  ```bash Text to image theme={null}
  curl --request POST \
    --url https://api.bfl.ai/v1/flux-3-image \
    --header "x-key: $BFL_API_KEY" \
    --header "Content-Type: application/json" \
    --data '{
    "prompt": "Ultra-wide cinematic shot of a fog-drenched coastal highway at dawn, cliffs on one side, turquoise sea on the other, a single vintage car with headlights on, soft golden rim light",
    "aspect_ratio": "21:9"
  }'
  ```

  ```bash Edit an image theme={null}
  curl --request POST \
    --url https://api.bfl.ai/v1/flux-3-image \
    --header "x-key: $BFL_API_KEY" \
    --header "Content-Type: application/json" \
    --data '{
    "prompt": "change the color of only one bird in the middle to #e01075",
    "images": [
      "https://cdn.sanity.io/images/2gpum2i6/production/4792198dfeba9223bf3ecf020fed2942de7f0bd7-1800x1200.webp"
    ]
  }'
  ```

  ```bash Multi reference theme={null}
  curl --request POST \
    --url https://api.bfl.ai/v1/flux-3-image \
    --header "x-key: $BFL_API_KEY" \
    --header "Content-Type: application/json" \
    --data '{
    "prompt": "Turn Image 1 in the Style of Image 2",
    "images": [
      "https://cdn.sanity.io/images/2gpum2i6/production/ababcf5c9cf86074d71c92d6362ff63545d90661-1800x1201.webp",
      "https://cdn.sanity.io/images/2gpum2i6/production/dc642f5b01040c4afc46c1b4f8c4945152932683-1350x1800.webp"
    ]
  }'
  ```

  ```bash Layout with boxes theme={null}
  curl --request POST \
    --url https://api.bfl.ai/v1/flux-3-image \
    --header "x-key: $BFL_API_KEY" \
    --header "Content-Type: application/json" \
    --data '{
    "prompt": "Minimalist graphic illustration featuring a black silhouette of a person <silhouette_1> centered against a solid, vibrant chartreuse background <background_1>. The figure is captured in a dynamic, mid-stride running pose, facing toward the left side of the frame. The image relies entirely on the high-contrast relationship between the two colors, mimicking a digital recreation of a screen-printed or risograph aesthetic, with no discernible light source or shadow. [{\"id\":\"background_1\",\"bbox\":[0,0,1000,1000],\"desc\":\"A flat field of neon yellow-green possessing a subtle, tactile paper texture with fine, organic grain and slight variations in color saturation, giving the flat surface a sense of physical depth.\"},{\"id\":\"silhouette_1\",\"bbox\":[150,150,850,850],\"desc\":\"A black silhouette of a person in motion, composed of a dense, stippled texture that resembles physical ink on paper or a low-resolution digital dither. The leading edges, including the front of the head, chest, and forward leg, are relatively solid and opaque. The trailing edges, such as the back, outstretched rear arm, and lifted back leg, dissolve into a spray of coarse, square-shaped pixels and scattered dots. The black ink shows a slight textural irregularity, as if pressed onto a porous surface.\"}]",
    "aspect_ratio": "1:1"
  }'
  ```

  ```bash Edit with boxes theme={null}
  curl --request POST \
    --url https://api.bfl.ai/v1/flux-3-image \
    --header "x-key: $BFL_API_KEY" \
    --header "Content-Type: application/json" \
    --data '{
    "prompt": "In <ref_image_0>, change the large tiger <animal_1> and the small glowing butterfly <insect_1> to be pink. Keep the massive fallen log <log_1>, the falling snow <snow_1>, and the dark background trees <trees_1> exactly unchanged. [{\"id\":\"animal_1\",\"from\":null,\"src_bbox\":null,\"tgt_bbox\":[250,50,850,650],\"desc\":\"Make both pink.\"},{\"id\":\"insect_1\",\"from\":null,\"src_bbox\":null,\"tgt_bbox\":[650,680,750,750],\"desc\":\"Make both pink.\"},{\"id\":\"log_1\",\"from\":\"ref_image_0\",\"src_bbox\":[600,0,1000,1000],\"tgt_bbox\":[600,0,1000,1000],\"desc\":\"A massive, fallen tree log stretching horizontally across the foreground. The surface features deep, rough-textured bark, weathered cracks, and small pockets of frost.\"},{\"id\":\"snow_1\",\"from\":\"ref_image_0\",\"src_bbox\":[0,0,1000,1000],\"tgt_bbox\":[0,0,1000,1000],\"desc\":\"Numerous white snowflakes of varying sizes falling across the scene. Some flakes are sharp and distinct, while others closer to the lens appear as blurred white streaks.\"},{\"id\":\"trees_1\",\"from\":\"ref_image_0\",\"src_bbox\":[0,0,650,1000],\"tgt_bbox\":[0,0,650,1000],\"desc\":\"A dense stand of tall, dark, vertical tree trunks receding into the distance. A cool, blue misty light permeates the spaces between the trees, glowing most intensely from the right edge.\"}]",
    "images": [
      "https://cdn.sanity.io/images/2gpum2i6/production/b6476b163e11444d6d5490ac1fa1e610d4eab85e-1360x768.webp"
    ]
  }'
  ```

  ```bash Move with boxes theme={null}
  curl --request POST \
    --url https://api.bfl.ai/v1/flux-3-image \
    --header "x-key: $BFL_API_KEY" \
    --header "Content-Type: application/json" \
    --data '{
    "prompt": "In <ref_image_0>, move the miniature grey amigurumi knight figure <knight_1> upwards and to the left along the yarn cliff <cliff_1>. Leave the polished steel tapestry needle <needle_1> in its original position in the air. Keep the rest of the image completely unchanged, preserving the blurred yarn backdrop <backdrop_1>, the large knitted dragon <dragon_1>, and the fiery orange thread <fire_1> suspended between them. [{\"id\":\"knight_1\",\"from\":\"ref_image_0\",\"src_bbox\":[500,150,850,350],\"tgt_bbox\":[194,55,544,255],\"desc\":\"A miniature amigurumi knight figure crocheted from thick grey woolen yarn. It has visible, intricate stitches forming a rounded helmet shape and a cylindrical body, with tiny stubby arms reaching upwards against the cliff.\"},{\"id\":\"backdrop_1\",\"from\":\"ref_image_0\",\"src_bbox\":[0,0,1000,1000],\"tgt_bbox\":[0,0,1000,1000],\"desc\":\"A soft, out-of-focus expanse of cream ivory and melange indigo knitted yarn, providing a blurred studio backdrop with tangible fiber fuzz catching the ambient light.\"},{\"id\":\"cliff_1\",\"from\":\"ref_image_0\",\"src_bbox\":[300,0,1000,450],\"tgt_bbox\":[300,0,1000,450],\"desc\":\"A steep, uneven cliff face constructed from stacked, thick yarn skeins in rich shades of mustard yellow, terracotta, and melange indigo, featuring visible woven wool textures and loose, stray fiber strands.\"},{\"id\":\"needle_1\",\"from\":\"ref_image_0\",\"src_bbox\":[420,250,550,450],\"tgt_bbox\":[420,250,550,450],\"desc\":\"A real, oversized polished steel tapestry needle, gleaming under the studio lighting. It features a blunt tip and an elongated eye, grasped like a weapon in the knight'"'"'s yarn-stub hand.\"},{\"id\":\"dragon_1\",\"from\":\"ref_image_0\",\"src_bbox\":[100,600,950,1000],\"tgt_bbox\":[100,600,950,1000],\"desc\":\"A large, fluffy knitted dragon made of highly textured, fuzzy yarn in terracotta and mustard yellow. It has a rounded snout, large crocheted eyes, and a wide-open mouth, with visible loose fibers creating a soft, tactile surface.\"},{\"id\":\"fire_1\",\"from\":\"ref_image_0\",\"src_bbox\":[350,400,650,650],\"tgt_bbox\":[350,400,650,650],\"desc\":\"Curled, sculptural puffs of fiery orange angora thread, suspended in mid-air. The thread is highly textured, fuzzy, and chaotic, bursting from the dragon'"'"'s mouth and stretching horizontally across the center.\"}]",
    "images": [
      "https://cdn.sanity.io/images/2gpum2i6/production/aa7b28af831835fcef86e770262c8c30fa308c2c-1360x768.webp"
    ]
  }'
  ```
</RequestExample>


## OpenAPI

````yaml https://api.bfl.ai/openapi.json POST /v1/flux-3-image
openapi: 3.1.0
info:
  title: BFL API
  description: Authorize with an API key from your user profile.
  version: 0.0.1
servers:
  - url: https://api.bfl.ai
    description: BFL API
security: []
tags:
  - name: Models
    description: >-
      Generation task endpoints. These endpoints allow you to submit generation
      tasks.
  - name: Utility
    description: >-
      These utility endpoints allow you to check the results of submitted tasks
      and to manage your finetunes.
paths:
  /v1/flux-3-image:
    post:
      tags:
        - Models
      summary: Generate an image with FLUX 3.
      description: Submits an image generation task with FLUX 3.
      operationId: flux_3_image_v1_flux_3_image_post
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/Flux3ImageInputs'
      responses:
        '200':
          description: Successful Response
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AsyncResponse'
        '422':
          description: Validation Error
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/HTTPValidationError'
      security:
        - APIKeyHeader: []
components:
  schemas:
    Flux3ImageInputs:
      properties:
        prompt:
          type: string
          title: Prompt
          description: Free-form prompt describing the image.
        images:
          anyOf:
            - type: string
            - items:
                type: string
              type: array
            - type: 'null'
          title: Images
          description: >-
            Optional reference image(s) the prompt edits or draws from; each is
            an http(s) URL or base64, one to 10 total.
        aspect_ratio:
          anyOf:
            - type: string
              enum:
                - '21:9'
                - '2:1'
                - '16:9'
                - '3:2'
                - '7:5'
                - '4:3'
                - '5:4'
                - '1:1'
                - '4:5'
                - '3:4'
                - '5:7'
                - '2:3'
                - '9:16'
                - '1:2'
                - '9:21'
            - type: string
              const: auto
          title: Aspect Ratio
          description: >-
            Output aspect ratio. Under `auto` a ratio the prompt asks for wins,
            otherwise the output keeps the first reference image's framing;
            without references the prompt decides, else 1:1.
          default: auto
        resolution:
          type: string
          enum:
            - 768sq
            - 1k
            - 1.5k
            - 2k
            - 4k
          title: Resolution
          description: >-
            Image resolution class, an equal-pixel-area tier: `768sq`, `1k`,
            `1.5k`, `2k`, or `4k`. Exact dimensions vary with the aspect ratio.
          default: 1k
        safety_tolerance:
          type: integer
          maximum: 7
          minimum: 0
          title: Safety Tolerance
          description: >-
            Harm moderation tolerance from 0 to 7, with 0 the strictest. The
            route and organization determine the approved maximum and
            per-category ceilings. Unapproved levels return 422 with approved
            levels.
          default: 2
        grounding:
          type: boolean
          title: Grounding
          description: >-
            When true (default) the prompt may be grounded in external research:
            web search and image search. When false, both are off.
          default: true
        version:
          type: string
          const: latest
          title: Version
          description: >-
            Endpoint version. `latest` (default) serves the current release;
            dated pinnable release tags are added here as they are published.
          default: latest
      additionalProperties: false
      type: object
      required:
        - prompt
      title: Flux3ImageInputs
    AsyncResponse:
      properties:
        id:
          type: string
          title: Id
        polling_url:
          type: string
          title: Polling Url
        cost:
          anyOf:
            - type: number
            - type: 'null'
          title: Cost
          description: Cost in credits for this request
        input_mp:
          anyOf:
            - type: number
            - type: 'null'
          title: Input Mp
          description: Input megapixels (2 decimal places)
        output_mp:
          anyOf:
            - type: number
            - type: 'null'
          title: Output Mp
          description: Output megapixels (2 decimal places)
      type: object
      required:
        - id
        - polling_url
      title: AsyncResponse
    HTTPValidationError:
      properties:
        detail:
          items:
            $ref: '#/components/schemas/ValidationError'
          type: array
          title: Detail
      type: object
      title: HTTPValidationError
    ValidationError:
      properties:
        loc:
          items:
            anyOf:
              - type: string
              - type: integer
          type: array
          title: Location
        msg:
          type: string
          title: Message
        type:
          type: string
          title: Error Type
      type: object
      required:
        - loc
        - msg
        - type
      title: ValidationError
  securitySchemes:
    APIKeyHeader:
      type: apiKey
      in: header
      name: x-key

````

This documentation is built and hosted on [Mintlify](https://mintlify.com), a developer documentation platform.

> ## Documentation Index
> Fetch the complete documentation index at: https://docs.stability.ai/llms.txt
> Use this file to discover all available pages before exploring further.

# Creative Upscale (async)

> Takes images between 64x64 and 1 megapixel and upscales them all the way to **4K** resolution.  Put more
generally, it can upscale images ~20-40x times while preserving, and often enhancing, quality.
Creative Upscale **works best on highly degraded images and is not for photos of 1mp or above** as it performs
heavy reimagining (controlled by creativity scale).


### How to use
Please invoke this endpoint with a `POST` request.

The headers of the request must include an API key in the `authorization` field. The body of the request must be
`multipart/form-data`.

The body of the request should include:
- `image`
- `prompt`

The body may optionally include:
- `seed`
- `negative_prompt`
- `output_format`
- `creativity`
- `style_preset`

> **Note:** for more details about these parameters please see the request schema below.

### Results
After invoking this endpoint with the required parameters, use the `id` in the response to poll for results at the
[results/{id} endpoint](https://docs.stability.ai/api-reference/results/fetch-async-generation-result).  Rate-limiting or other errors may occur if you poll more than once every 10 seconds.

### Credits
Flat rate of 60 credits per successful generation.  You will not be charged for failed generations.



## OpenAPI

````yaml /openapi.json post /v2beta/stable-image/upscale/creative
openapi: 3.0.1
info:
  version: v2beta
  title: StabilityAI REST API
  description: >-
    Welcome to the Stability Platform API. As of March 2024, we are building the
    REST v2beta API service to be the primary API service for the Stability
    Platform.

    All AI services on other APIs (gRPC, REST v1, RESTv2alpha) will continue to
    be maintained, however they will not receive

    new features or parameters.


    If you are a REST v2alpha user, we strongly recommend that you adjust the
    URL calls for the specific services that you are using over to the
    equivalent REST v2beta URL. Normally, this means simply replacing "v2alpha"
    with "v2beta". We are not deprecating v2alpha URLs at this time for users
    that are currently using them.


    #### Authentication


    You will need your [Stability API
    key](https://platform.stability.ai/dashboard/api-keys) in order to make
    requests to this API.

    Make sure you never share your API key with anyone, and you never commit it
    to a public repository. Include this key in

    the `Authorization` header of your requests.


    #### Rate limiting


    This API is rate-limited to 150 requests every 10 seconds. If you exceed
    this limit, you will receive a `429` response

    and be timed out for 60 seconds. If you find this limit too restrictive,
    please reach out to us via [this
    form](https://kb.stability.ai/knowledge-base/kb-tickets/new).


    #### Support


    Please see our [FAQ](https://docs.stability.ai/faq) for answers to common
    questions. If you have any other questions or concerns,

    please reach out to us via [this
    form](https://kb.stability.ai/knowledge-base/kb-tickets/new).


    To see the health of our APIs, please check our [Status
    Page](https://stabilityai.instatus.com/).
servers:
  - url: https://api.stability.ai
security:
  - STABILITY_API_KEY: []
tags:
  - name: Edit
    description: >-
      Tools for editing your own and generated images.


      **[Erase](https://docs.stability.ai/api-reference/edit/erase)**


      The Erase service removes unwanted objects, such as blemishes on portraits
      or items on desks, using image masks.


      **[Outpaint](https://docs.stability.ai/api-reference/edit/outpaint)**


      The outpaint service inserts additional content in an image to fill in the
      space in any direction, allowing you to "zoom-out" of an image.


      **[Inpaint](https://docs.stability.ai/api-reference/edit/inpaint)**


      The Inpaint service modifies images by filling in or replacing specified
      areas with new content based on the content of a "mask" image.


      **[Search and
      Replace](https://docs.stability.ai/api-reference/edit/search-and-replace)**


      The Search and Replace service, similar to inpaint, allows to replace
      specified areas with new content, but this time with the help of a prompt
      instead of a mask. The service will automatically segment the object and
      replace it with the object requested in the prompt.


      **[Search and
      Recolor](https://docs.stability.ai/api-reference/edit/search-and-recolor)**


      The Search and Recolor service is another derivative of the inpaint
      service and provides the ability to change the color of a specific object
      in an image using a prompt. The Search and Recolor service will
      automatically segment the object and recolor it using the colors requested
      in the prompt.


      **[Remove
      Background](https://docs.stability.ai/api-reference/edit/remove-background)**


      The Remove Background service accurately segments the foreground from an
      image to removes the background.
  - name: Upscale
    description: >-
      Tools for increasing the size and resolution of your existing images.


      **[Fast Upscaler](https://docs.stability.ai/api-reference/upscale/fast)**


      This service enhances image resolution by 4x using predictive and
      generative AI. This lightweight and fast service (processing in ~1 second)
      is ideal for enhancing the quality of compressed images, making it
      suitable for social media posts and other applications.


      **[Conservative
      Upscaler](https://docs.stability.ai/api-reference/upscale/conservative)**


      This service can upscale images by 20 to 40 times up to a 4 megapixel
      output image with minimal alteration to the original image. The
      Conservative Upscaler can upscale images as small as 64x64 pixels directly
      to a 4 megapixel output. Use this option if you directly need a 4
      megapixel output.


      **[Creative
      Upscaler](https://docs.stability.ai/api-reference/upscale/creative-upscale-async)**


      The service can upscale highly degraded images (lower than 1 megapixel)
      with a creative twist to provide high resolution results.
  - name: Generate
    description: >-
      Tools to generate new images from text, or create variations of existing
      images. Our different services include:


      **[Stable Image
      Ultra](https://docs.stability.ai/api-reference/generate/stable-image-ultra)**:
      Photorealistic, Large-Scale Output


      Our state of the art text to image model based on Stable Diffusion 3.5.
      Stable Image Ultra Produces the highest quality, photorealistic outputs
      perfect for professional print media and large format applications. Stable
      Image Ultra excels at rendering exceptional detail and realism.


      **[Stable Image
      Core](https://docs.stability.ai/api-reference/generate/stable-image-core)**:
      Fast and Affordable


      Optimized for fast and aﬀordable image generation, great for rapidly
      iterating on concepts during ideation. Stable Image Core is the next
      generation model following Stable Diffusion XL.


      **[Stable Diffusion 3.5 Model
      Suite](https://docs.stability.ai/api-reference/generate/stable-diffusion-35)**:
      Stability AI's latest base models


      The different versions of our open models are available via API, letting
      you test and adjust speed and quality based on your use case. All model
      versions strike a balance between generation speed and output quality and
      are ideal for creating high-volume, high-quality digital assets like
      websites, newsletters, and marketing materials.
  - name: Control
    description: >-
      Tools for generating precise, controlled variations of existing images or
      sketches.


      **[Sketch](https://docs.stability.ai/api-reference/control/sketch)**


      This service upgrades sketches to refined outputs with precise control.
      For non-sketch images, it allows detailed manipulation of the final
      appearance by leveraging the contour lines and edges within the image.


      **[Structure](https://docs.stability.ai/api-reference/control/structure)**


      This service excels in generating images by maintaining the structure of
      an input image, making it especially valuable for advanced content
      creation scenarios such as recreating scenes or rendering characters from
      models.


      **[Style](https://docs.stability.ai/api-reference/control/style-guide)**


      This service extracts stylistic elements from an input image (control
      image) and uses it to guide the creation of an output image based on the
      prompt. The result is a new image in the same style as the control image.
  - name: Results
    description: Tools for fetching the results of your async generations.
  - name: Stable Audio 2
    description: >-
      Tools to generate music and sound from text or audio, or transform
      existing audio clips into new compositions. For our most advanced audio
      model, see Stable Audio 3.0. Our different services include:


      **Stable Audio 2.5**: Fast, Long-Form Music & Audio Generation


      Generates up to 3-minute, 44.1 kHz stereo compositions. Stable Audio 2.5
      supports text-to-audio, audio-to-audio, and audio-inpaint workflows -
      allowing creators to upload a sound and transform it into new instruments,
      styles, or genres using natural language prompts. Ideal for music
      production, cinematic sound design, and remixing.


      **Stable Audio 2.0**: High-Quality Audio Generation


      Built for text-to-audio and audio-to-audio generation, also capable of
      generating up to 3-minute, 44.1 kHz stereo. Stable Audio 2.0 is great for
      ideation, music demos, and ambient soundscapes. It's optimized for
      creative professionals seeking detailed and extended outputs from simple
      prompts.


      Stable Audio models were exclusively trained on licensed data from the
      [AudioSparx](https://www.audiosparx.com/) music library, honoring opt-out
      requests and ensuring fair compensation for creators. Additionally, Stable
      Audio 2.5 was pre-trained on licensed data from
      [Freesound](https://freesound.org/). Read more about the model
      capabilities [here](https://stability.ai/news/stable-audio-2-0).
    x-displayName: Stable Audio 2.5
  - name: Stable Audio
    description: >-
      Tools to generate music and sound from text or audio, or transform
      existing audio clips into new compositions. Our different services
      include:


      **Stable Audio 3.0**: Fast, Best-Quality, Long-Form Music & Audio
      Generation


      Our most advanced audio generation model, capable of generating up to
      6-minute, 44.1 kHz stereo compositions. Stable Audio 3.0 supports
      text-to-audio, audio-to-audio, and audio-inpaint workflows - allowing
      creators to upload a sound and transform it into new instruments, styles,
      or genres using natural language prompts. Ideal for music production,
      cinematic sound design, and remixing.


      Stable Audio models were exclusively trained on licensed data from the
      [AudioSparx](https://www.audiosparx.com/) music library, honoring opt-out
      requests and ensuring fair compensation for creators. Additionally, Stable
      Audio 3.0 was pre-trained on licensed data from
      [Freesound](https://freesound.org/). Read more about the model
      capabilities [here](https://stability.ai/news/stable-audio-3-0).
    x-displayName: Stable Audio 3.0
paths:
  /v2beta/stable-image/upscale/creative:
    post:
      tags:
        - Upscale
      summary: Creative Upscale (async)
      description: >-
        Takes images between 64x64 and 1 megapixel and upscales them all the way
        to **4K** resolution.  Put more

        generally, it can upscale images ~20-40x times while preserving, and
        often enhancing, quality.

        Creative Upscale **works best on highly degraded images and is not for
        photos of 1mp or above** as it performs

        heavy reimagining (controlled by creativity scale).



        ### How to use

        Please invoke this endpoint with a `POST` request.


        The headers of the request must include an API key in the
        `authorization` field. The body of the request must be

        `multipart/form-data`.


        The body of the request should include:

        - `image`

        - `prompt`


        The body may optionally include:

        - `seed`

        - `negative_prompt`

        - `output_format`

        - `creativity`

        - `style_preset`


        > **Note:** for more details about these parameters please see the
        request schema below.


        ### Results

        After invoking this endpoint with the required parameters, use the `id`
        in the response to poll for results at the

        [results/{id}
        endpoint](https://docs.stability.ai/api-reference/results/fetch-async-generation-result). 
        Rate-limiting or other errors may occur if you poll more than once every
        10 seconds.


        ### Credits

        Flat rate of 60 credits per successful generation.  You will not be
        charged for failed generations.
      parameters:
        - $ref: '#/components/parameters/StabilityClientID'
        - $ref: '#/components/parameters/StabilityClientUserID'
        - $ref: '#/components/parameters/StabilityClientVersion'
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                image:
                  type: string
                  description: >-
                    The image you wish to upscale.


                    Supported Formats:

                    - jpeg

                    - png

                    - webp


                    Validation Rules:

                    - Every side must be at least 64 pixels

                    - Total pixel count must be between 4,096 and 1,048,576
                    pixels
                  format: binary
                  example: ./some/image.png
                prompt:
                  $ref: '#/components/schemas/Prompt'
                negative_prompt:
                  $ref: '#/components/schemas/NegativePrompt'
                output_format:
                  type: string
                  enum:
                    - jpeg
                    - png
                    - webp
                  default: png
                  description: Dictates the `content-type` of the generated image.
                seed:
                  $ref: '#/components/schemas/Seed'
                creativity:
                  type: number
                  minimum: 0.1
                  maximum: 0.5
                  default: 0.3
                  description: >-
                    Indicates how creative the model should be when upscaling an
                    image.

                    Higher values will result in more details being added to the
                    image during upscaling.
                style_preset:
                  $ref: '#/components/schemas/StylePreset'
              required:
                - image
                - prompt
      responses:
        '200':
          description: Upscale was started.
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    $ref: '#/components/schemas/GenerationID'
                required:
                  - id
        '400':
          $ref: '#/components/responses/BadRequest'
        '403':
          $ref: '#/components/responses/ContentModerated'
        '413':
          $ref: '#/components/responses/PayloadTooLarge'
        '422':
          $ref: '#/components/responses/ImageClassificationFailed'
        '429':
          $ref: '#/components/responses/RateLimited'
        '500':
          $ref: '#/components/responses/InternalError'
      x-codeSamples:
        - lang: python
          label: Python
          source: |-
            import requests

            response = requests.post(
                f"https://api.stability.ai/v2beta/stable-image/upscale/creative",
                headers={
                    "authorization": f"Bearer sk-MYAPIKEY",
                    "accept": "image/*"
                },
                files={
                    "image": open("./kitten-in-space.png", "rb")
                },
                data={
                    "prompt": "cute fluffy white kitten floating in space, pastel colors",
                    "output_format": "webp",
                },
            )

            print("Generation ID:", response.json().get('id'))
        - lang: javascript
          label: JavaScript
          source: |-
            import fs from "node:fs";
            import axios from "axios";
            import FormData from "form-data";

            const payload = {
              image: fs.createReadStream("./kitten-in-space.png"),
              prompt: "cute fluffy white kitten floating in space, pastel colors",
              output_format: "webp"
            };

            const response = await axios.postForm(
              `https://api.stability.ai/v2beta/stable-image/upscale/creative`,
              axios.toFormData(payload, new FormData()),
              {
                validateStatus: undefined,
                headers: { 
                  Authorization: `Bearer sk-MYAPIKEY`
                },
              },
            );

            console.log("Generation ID:", response.data.id);
        - lang: bash
          label: cURL
          source: >-
            curl -f -sS
            "https://api.stability.ai/v2beta/stable-image/upscale/creative" \
              -H "authorization: Bearer sk-MYAPIKEY" \
              -F image=@"./kitten-in-rainforest.png" \
              -F prompt="cute fluffy white kitten sitting in a rainforest, pastel colors" \
              -F output_format=webp \
              -o "./output.json"
components:
  parameters:
    StabilityClientID:
      schema:
        type: string
        maxLength: 256
        description: >-
          The name of your application, used to help us communicate app-specific
          debugging or moderation issues to you.
        example: my-awesome-app
      required: false
      name: stability-client-id
      in: header
    StabilityClientUserID:
      schema:
        type: string
        maxLength: 256
        description: >-
          A unique identifier for your end user. Used to help us communicate
          user-specific debugging or moderation issues to you. Feel free to
          obfuscate this value to protect user privacy.
        example: DiscordUser#9999
      required: false
      name: stability-client-user-id
      in: header
    StabilityClientVersion:
      schema:
        type: string
        maxLength: 256
        description: >-
          The version of your application, used to help us communicate
          version-specific debugging or moderation issues to you.
        example: 1.2.1
      required: false
      name: stability-client-version
      in: header
  schemas:
    Prompt:
      type: string
      minLength: 1
      maxLength: 10000
      description: >-
        What you wish to see in the output image. A strong, descriptive prompt
        that clearly defines

        elements, colors, and subjects will lead to better results.


        To control the weight of a given word use the format `(word:weight)`,

        where `word` is the word you'd like to control the weight of and
        `weight`

        is a value between 0 and 1. For example: `The sky was a crisp (blue:0.3)
        and (green:0.8)`

        would convey a sky that was blue and green, but more green than blue.
    NegativePrompt:
      type: string
      maxLength: 10000
      description: >-
        A blurb of text describing what you **do not** wish to see in the output
        image.

        This is an advanced feature.
    Seed:
      type: number
      minimum: 0
      maximum: 4294967294
      default: 0
      description: >-
        A specific value that is used to guide the 'randomness' of the
        generation. (Omit this parameter or pass `0` to use a random seed.)
    StylePreset:
      type: string
      enum:
        - enhance
        - anime
        - photographic
        - digital-art
        - comic-book
        - fantasy-art
        - line-art
        - analog-film
        - neon-punk
        - isometric
        - low-poly
        - origami
        - modeling-compound
        - cinematic
        - 3d-model
        - pixel-art
        - tile-texture
      description: Guides the image model towards a particular style.
    GenerationID:
      type: string
      minLength: 64
      maxLength: 64
      description: >-
        The `id` of a generation, typically used for async generations, that can
        be used to check the status of the generation or retrieve the result.
      example: a6dc6c6e20acda010fe14d71f180658f2896ed9b4ec25aa99a6ff06c796987c4
    Error:
      type: object
      properties:
        id:
          type: string
          minLength: 1
          description: >-
            A unique identifier associated with this error. Please include this
            in any [support
            tickets](https://kb.stability.ai/knowledge-base/kb-tickets/new)

            you file, as it will greatly assist us in diagnosing the root cause
            of the problem.
          example: a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4
        name:
          type: string
          minLength: 1
          description: >-
            Short-hand name for an error, useful for discriminating between
            errors with the same status code.
          example: bad_request
        errors:
          type: array
          items:
            type: string
          minItems: 1
          description: One or more error messages indicating what went wrong.
          example:
            - 'some-field: is required'
      required:
        - id
        - name
        - errors
    ContentModerationResponse:
      type: object
      properties:
        id:
          type: string
          minLength: 1
          description: >-
            A unique identifier associated with this error. Please include this
            in any [support
            tickets](https://kb.stability.ai/knowledge-base/kb-tickets/new)

            you file, as it will greatly assist us in diagnosing the root cause
            of the problem.
          example: a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4
        name:
          type: string
          minLength: 1
          description: >-
            Our content moderation system has flagged some part of your request
            and subsequently denied it.  You were not charged for this request. 
            While this may at times be frustrating, it is necessary to maintain
            the integrity of our platform and ensure a safe experience for all
            users.


            If you would like to provide feedback, please use the [Support
            Form](https://kb.stability.ai/knowledge-base/kb-tickets/new).
          enum:
            - content_moderation
        errors:
          type: array
          items:
            type: string
          minItems: 1
          description: One or more error messages indicating what went wrong.
          example:
            - 'some-field: is required'
      required:
        - id
        - name
        - errors
      description: Your request was flagged by our content moderation system.
      example:
        id: ed14db44362126aab3cbd25cca51ffe3
        name: content_moderation
        errors:
          - >-
            Your request was flagged by our content moderation system, as a
            result your request was denied and you were not charged.
  responses:
    BadRequest:
      description: Invalid parameter(s), see the `errors` field for details.
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'
    ContentModerated:
      description: Your request was flagged by our content moderation system.
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ContentModerationResponse'
    PayloadTooLarge:
      description: Your request was larger than 10MiB.
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'
          example:
            id: 4212a4b66fbe1cedca4bf2133d35dca5
            name: payload_too_large
            errors:
              - 'body: payloads cannot be larger than 10MiB in size'
    ImageClassificationFailed:
      description: >-
        Your request was well-formed, but rejected. See the `errors` field for
        details.
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'
          examples:
            Invalid Language:
              value:
                id: ff54b236a3acdde1522cb1ba641c43ed
                name: invalid_language
                errors:
                  - English is the only supported language for this service.
            Public Figure Detected:
              value:
                id: ff54b236a3acdde1522cb1ba641c43ed
                name: public_figure
                errors:
                  - >-
                    Our system detected the likeness of a public figure in your
                    image. To comply with our guidelines, this request cannot be
                    processed. Please upload a different image.
    RateLimited:
      description: You have made more than 150 requests in 10 seconds.
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'
          example:
            id: rate_limit_exceeded
            name: rate_limit_exceeded
            errors:
              - >-
                You have exceeded the rate limit of 150 requests within a 10
                second period, and have been timed out for 60 seconds.
    InternalError:
      description: >-
        An internal error occurred. If the problem persists [contact
        support](https://kb.stability.ai/knowledge-base/kb-tickets/new).
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Error'
          example:
            id: 2a1b2d4eafe2bc6ab4cd4d5c6133f513
            name: internal_error
            errors:
              - An unexpected server error has occurred, please try again later.
  securitySchemes:
    STABILITY_API_KEY:
      type: http
      scheme: bearer
      description: >-
        Your [Stability API
        key](https://platform.stability.ai/dashboard/api-keys), sent as a bearer
        token in the `Authorization` header.

````

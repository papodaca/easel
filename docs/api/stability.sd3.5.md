> ## Documentation Index
> Fetch the complete documentation index at: https://docs.stability.ai/llms.txt
> Use this file to discover all available pages before exploring further.

# Stable Diffusion 3.5

> Generate using Stable Diffusion 3.5 models, Stability AI latest base model:

- **Stable Diffusion 3.5 Large**: At 8 billion parameters, with superior quality and
  prompt adherence, this base model is the most powerful in the Stable Diffusion
  family. This model is ideal for professional use cases at 1 megapixel resolution.

- **Stable Diffusion 3.5 Large Turbo**: A distilled version of Stable Diffusion 3.5 Large.
  SD3.5 Large Turbo generates high-quality images with exceptional prompt adherence
  in just 4 steps, making it considerably faster than Stable Diffusion 3.5 Large.

- **Stable Diffusion 3.5 Medium**: With 2.5 billion parameters, the model delivers an
  optimal balance between prompt accuracy and image quality, making it an efficient
  choice for fast high-performance image generation.

- **Stable Diffusion 3.5 Flash**: A distilled version of Stable Diffusion 3.5 Medium. 
  SD3.5 Flash generates high-quality images with a 4 step process instead of 40, making 
  it faster than Stable Diffusion 3.5 Medium.

Read more about the model capabilities [here](https://stability.ai/news/introducing-stable-diffusion-3-5).

As of April 17, 2025, we have deprecated the Stable Diffusion 3.0 APIs and will be automatically
re-routing calls to Stable Diffusion 3.0 models to Stable Diffusion 3.5 APIs at no extra cost.
You can read more in the [release notes](https://docs.stability.ai/release-notes).

### How to use
Please invoke this endpoint with a `POST` request.

The headers of the request must include an API key in the `authorization` field. The body of the request must be
`multipart/form-data`.  The accept header should be set to one of the following:
- `image/*` to receive the image in the format specified by the `output_format` parameter.
- `application/json` to receive the image encoded as base64 in a JSON response.

#### **Generating with a prompt**
Commonly referred to as **text-to-image**, this mode generates an image from text alone. While the only required
parameter is the `prompt`, it also supports an `aspect_ratio` parameter which can be used to control the
aspect ratio of the generated image.

#### **Generating with a prompt *and* an image**
Commonly referred to as **image-to-image**, this mode also generates an image from text but uses an existing image as the
starting point. The required parameters are:
- `prompt` - text to generate the image from
- `image` - the image to use as the starting point for the generation
- `strength` - controls how much influence the `image` parameter has on the output image
- `mode` - must be set to `image-to-image`

> **Note:** maximum request size is 10MiB.

#### **Optional Parameters:**
Both modes support the following optional parameters:
- `model` - the model to use (SD 3.5 Large, SD 3.5 Large Turbo, SD 3.5 Medium, SD 3.5 Flash)
- `output_format` - the format of the output image
- `seed` - the randomness seed to use for the generation
- `negative_prompt` - keywords of what you **do not** wish to see in the output image
- `cfg_scale` - controls how strictly the diffusion process adheres to the prompt text
- `style_preset` - guides the image model towards a particular style

> **Note:** for more details about these parameters please see the request schema below.

### Output
The resolution of the generated image will be 1MP. The default resolution is 1024x1024.

### Credits
- **SD 3.5 Large**: Flat rate of 6.5 credits per successful generation.
- **SD 3.5 Large Turbo**: Flat rate of 4 credits per successful generation.
- **SD 3.5 Medium**: Flat rate of 3.5 credits per successful generation.
- **SD 3.5 Flash**: Flat rate of 2.5 credits per successful generation.

As always, you will not be charged for failed generations.



## OpenAPI

````yaml /openapi.json post /v2beta/stable-image/generate/sd3
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
  /v2beta/stable-image/generate/sd3:
    post:
      tags:
        - Generate
      summary: Stable Diffusion 3.5
      description: >-
        Generate using Stable Diffusion 3.5 models, Stability AI latest base
        model:


        - **Stable Diffusion 3.5 Large**: At 8 billion parameters, with superior
        quality and
          prompt adherence, this base model is the most powerful in the Stable Diffusion
          family. This model is ideal for professional use cases at 1 megapixel resolution.

        - **Stable Diffusion 3.5 Large Turbo**: A distilled version of Stable
        Diffusion 3.5 Large.
          SD3.5 Large Turbo generates high-quality images with exceptional prompt adherence
          in just 4 steps, making it considerably faster than Stable Diffusion 3.5 Large.

        - **Stable Diffusion 3.5 Medium**: With 2.5 billion parameters, the
        model delivers an
          optimal balance between prompt accuracy and image quality, making it an efficient
          choice for fast high-performance image generation.

        - **Stable Diffusion 3.5 Flash**: A distilled version of Stable
        Diffusion 3.5 Medium. 
          SD3.5 Flash generates high-quality images with a 4 step process instead of 40, making 
          it faster than Stable Diffusion 3.5 Medium.

        Read more about the model capabilities
        [here](https://stability.ai/news/introducing-stable-diffusion-3-5).


        As of April 17, 2025, we have deprecated the Stable Diffusion 3.0 APIs
        and will be automatically

        re-routing calls to Stable Diffusion 3.0 models to Stable Diffusion 3.5
        APIs at no extra cost.

        You can read more in the [release
        notes](https://docs.stability.ai/release-notes).


        ### How to use

        Please invoke this endpoint with a `POST` request.


        The headers of the request must include an API key in the
        `authorization` field. The body of the request must be

        `multipart/form-data`.  The accept header should be set to one of the
        following:

        - `image/*` to receive the image in the format specified by the
        `output_format` parameter.

        - `application/json` to receive the image encoded as base64 in a JSON
        response.


        #### **Generating with a prompt**

        Commonly referred to as **text-to-image**, this mode generates an image
        from text alone. While the only required

        parameter is the `prompt`, it also supports an `aspect_ratio` parameter
        which can be used to control the

        aspect ratio of the generated image.


        #### **Generating with a prompt *and* an image**

        Commonly referred to as **image-to-image**, this mode also generates an
        image from text but uses an existing image as the

        starting point. The required parameters are:

        - `prompt` - text to generate the image from

        - `image` - the image to use as the starting point for the generation

        - `strength` - controls how much influence the `image` parameter has on
        the output image

        - `mode` - must be set to `image-to-image`


        > **Note:** maximum request size is 10MiB.


        #### **Optional Parameters:**

        Both modes support the following optional parameters:

        - `model` - the model to use (SD 3.5 Large, SD 3.5 Large Turbo, SD 3.5
        Medium, SD 3.5 Flash)

        - `output_format` - the format of the output image

        - `seed` - the randomness seed to use for the generation

        - `negative_prompt` - keywords of what you **do not** wish to see in the
        output image

        - `cfg_scale` - controls how strictly the diffusion process adheres to
        the prompt text

        - `style_preset` - guides the image model towards a particular style


        > **Note:** for more details about these parameters please see the
        request schema below.


        ### Output

        The resolution of the generated image will be 1MP. The default
        resolution is 1024x1024.


        ### Credits

        - **SD 3.5 Large**: Flat rate of 6.5 credits per successful generation.

        - **SD 3.5 Large Turbo**: Flat rate of 4 credits per successful
        generation.

        - **SD 3.5 Medium**: Flat rate of 3.5 credits per successful generation.

        - **SD 3.5 Flash**: Flat rate of 2.5 credits per successful generation.


        As always, you will not be charged for failed generations.
      parameters:
        - $ref: '#/components/parameters/AcceptImage'
        - $ref: '#/components/parameters/StabilityClientID'
        - $ref: '#/components/parameters/StabilityClientUserID'
        - $ref: '#/components/parameters/StabilityClientVersion'
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                prompt:
                  allOf:
                    - $ref: '#/components/schemas/Prompt'
                    - description: >-
                        What you wish to see in the output image. A strong,
                        descriptive prompt that clearly defines

                        elements, colors, and subjects will lead to better
                        results.
                mode:
                  type: string
                  enum:
                    - text-to-image
                    - image-to-image
                  default: text-to-image
                  description: >-
                    Controls whether this is a text-to-image or image-to-image
                    generation, which affects which parameters are required:

                    - **text-to-image** requires only the `prompt` parameter

                    - **image-to-image** requires the `prompt`, `image`, and
                    `strength` parameters
                  title: GenerationMode
                image:
                  type: string
                  description: >-
                    The image to use as the starting point for the generation.


                    Supported formats:
                      - jpeg
                      - png
                      - webp

                    Supported dimensions:
                      - Every side must be at least 64 pixels

                    > **Important:** This parameter is only valid for
                    **image-to-image** requests.
                  format: binary
                strength:
                  type: number
                  minimum: 0
                  maximum: 1
                  description: >-
                    Sometimes referred to as _denoising_, this parameter
                    controls how much influence the

                    `image` parameter has on the generated image.  A value of 0
                    would yield an image that

                    is identical to the input.  A value of 1 would be as if you
                    passed in no image at all.


                    > **Important:** This parameter is only valid for
                    **image-to-image** requests. For SD 3.5 Flash, the
                     best results for image-to-image generation are achieved with a `strength` between .94 - .97.
                aspect_ratio:
                  type: string
                  enum:
                    - '21:9'
                    - '16:9'
                    - '3:2'
                    - '5:4'
                    - '1:1'
                    - '4:5'
                    - '2:3'
                    - '9:16'
                    - '9:21'
                  default: '1:1'
                  description: >-
                    Controls the aspect ratio of the generated image. Defaults
                    to 1:1.


                    > **Important:** This parameter is only valid for
                    **text-to-image** requests.
                model:
                  type: string
                  enum:
                    - sd3.5-large
                    - sd3.5-large-turbo
                    - sd3.5-medium
                  default: sd3.5-large
                  description: >-
                    The model to use for generation.


                    - `sd3.5-large` requires 6.5 credits per generation

                    - `sd3.5-large-turbo` requires 4 credits per generation

                    - `sd3.5-medium` requires 3.5 credits per generation

                    - `sd3.5-flash` requires 2.5 credits per generation

                    - As of the April 17, 2025, `sd3-large`, `sd3-large-turbo`
                    and `sd3-medium`
                      are re-routed to their `sd3.5-[model version]` equivalent, at the same price.
                seed:
                  $ref: '#/components/schemas/Seed'
                output_format:
                  type: string
                  enum:
                    - png
                    - jpeg
                    - webp
                  default: png
                  description: Dictates the `content-type` of the generated image.
                style_preset:
                  $ref: '#/components/schemas/StylePreset'
                negative_prompt:
                  allOf:
                    - $ref: '#/components/schemas/NegativePrompt'
                    - description: >-
                        Keywords of what you **do not** wish to see in the
                        output image.

                        This is an advanced feature.
                cfg_scale:
                  type: number
                  minimum: 1
                  maximum: 10
                  description: >-
                    How strictly the diffusion process adheres to the prompt
                    text (higher values keep your image closer to your prompt).
                    The _Large_ and _Medium_ models use a default of `4`. The
                    _Turbo_ and _Flash_ model uses a default of `1`.
              required:
                - prompt
      responses:
        '200':
          $ref: '#/components/responses/ImageGenerationSuccess'
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
                f"https://api.stability.ai/v2beta/stable-image/generate/sd3",
                headers={
                    "authorization": f"Bearer sk-MYAPIKEY",
                    "accept": "image/*"
                },
                files={"none": ''},
                data={
                    "prompt": "Lighthouse on a cliff overlooking the ocean",
                    "output_format": "jpeg",
                },
            )

            if response.status_code == 200:
                with open("./lighthouse.jpeg", 'wb') as file:
                    file.write(response.content)
            else:
                raise Exception(str(response.json()))
        - lang: javascript
          label: JavaScript
          source: |-
            import fs from "node:fs";
            import axios from "axios";
            import FormData from "form-data";

            const payload = {
              prompt: "Lighthouse on a cliff overlooking the ocean",
              output_format: "jpeg"
            };

            const response = await axios.postForm(
              `https://api.stability.ai/v2beta/stable-image/generate/sd3`,
              axios.toFormData(payload, new FormData()),
              {
                validateStatus: undefined,
                responseType: "arraybuffer",
                headers: { 
                  Authorization: `Bearer sk-MYAPIKEY`, 
                  Accept: "image/*" 
                },
              },
            );

            if(response.status === 200) {
              fs.writeFileSync("./lighthouse.jpeg", Buffer.from(response.data));
            } else {
              throw new Error(`${response.status}: ${response.data.toString()}`);
            }
        - lang: bash
          label: cURL
          source: >-
            curl -f -sS
            "https://api.stability.ai/v2beta/stable-image/generate/sd3" \
              -H "authorization: Bearer sk-MYAPIKEY" \
              -H "accept: image/*" \
              -F prompt="Lighthouse on a cliff overlooking the ocean" \
              -F output_format="jpeg" \
              -o "./lighthouse.jpeg"
components:
  parameters:
    AcceptImage:
      schema:
        type: string
        default: image/*
        description: >-
          Specify `image/*` to receive the bytes of the image directly.
          Otherwise specify `application/json` to receive the image as base64
          encoded JSON.
        enum:
          - image/*
          - application/json
      required: false
      name: accept
      in: header
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
    NegativePrompt:
      type: string
      maxLength: 10000
      description: >-
        A blurb of text describing what you **do not** wish to see in the output
        image.

        This is an advanced feature.
    ImageBinaryResponse:
      type: string
      description: |-
        The bytes of the generated image.

        The `finish-reason` and `seed` will be present as headers.
      format: binary
    ImageJSONResponse:
      type: object
      properties:
        image:
          type: string
          description: The generated image, encoded to base64.
          example: AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1...
        seed:
          allOf:
            - $ref: '#/components/schemas/Seed'
            - description: The seed used as random noise for this generation.
              example: 343940597
        finish_reason:
          type: string
          enum:
            - SUCCESS
            - CONTENT_FILTERED
          description: >-
            The reason the generation finished.


            - `SUCCESS` = successful generation.

            - `CONTENT_FILTERED` = successful generation, however the output
            violated our content moderation

            policy and has been blurred as a result.
          example: SUCCESS
      required:
        - image
        - finish_reason
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
    ImageGenerationSuccess:
      description: Generation was successful.
      headers:
        x-request-id:
          $ref: '#/components/headers/RequestID'
        content-type:
          description: |-
            The format of the generated image.

             To receive the bytes of the image directly, specify `image/*` in the accept header. To receive the bytes base64 encoded inside of a JSON payload, specify `application/json`.
          examples:
            jpeg:
              description: raw bytes
              value: image/jpeg
            jpegJSON:
              description: base64 encoded
              value: application/json; type=image/jpeg
            png:
              description: raw bytes
              value: image/png
            pngJSON:
              description: base64 encoded
              value: application/json; type=image/png
            webp:
              description: raw bytes
              value: image/webp
            webpJSON:
              description: base64 encoded
              value: application/json; type=image/webp
          schema:
            type: string
        finish-reason:
          $ref: '#/components/headers/ImageFinishReason'
        seed:
          $ref: '#/components/headers/Seed'
      content:
        image/jpeg:
          example: The bytes of the generated jpeg
          schema:
            $ref: '#/components/schemas/ImageBinaryResponse'
        application/json; type=image/jpeg:
          schema:
            $ref: '#/components/schemas/ImageJSONResponse'
        image/png:
          example: The bytes of the generated png
          schema:
            $ref: '#/components/schemas/ImageBinaryResponse'
        application/json; type=image/png:
          schema:
            $ref: '#/components/schemas/ImageJSONResponse'
        image/webp:
          example: The bytes of the generated webp
          schema:
            $ref: '#/components/schemas/ImageBinaryResponse'
        application/json; type=image/webp:
          schema:
            $ref: '#/components/schemas/ImageJSONResponse'
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
  headers:
    RequestID:
      description: A unique identifier for this request.
      schema:
        type: string
    ImageFinishReason:
      schema:
        type: string
        enum:
          - SUCCESS
          - CONTENT_FILTERED
      description: >-
        Indicates the reason the generation finished.


        - `SUCCESS` = successful generation.

        - `CONTENT_FILTERED` = successful generation, however the output
        violated our content moderation

        policy and has been blurred as a result.


        > **NOTE:** This header is absent on JSON encoded responses because it
        is present in the body as `finish_reason`.
    Seed:
      description: >-
        The seed used as random noise for this generation.


        > **NOTE:** This header is absent on JSON encoded responses because it
        is present in the body as `seed`.
      example: '343940597'
      schema:
        type: string
  securitySchemes:
    STABILITY_API_KEY:
      type: http
      scheme: bearer
      description: >-
        Your [Stability API
        key](https://platform.stability.ai/dashboard/api-keys), sent as a bearer
        token in the `Authorization` header.

````

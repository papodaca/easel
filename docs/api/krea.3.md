> ## Documentation Index
> Fetch the complete documentation index at: https://www.krea.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Krea 3 Beta

> Generate and edit images with Krea 3.

<Accordion title="Deprecated request aliases">
  These aliases are accepted for compatibility only and are not shown in the request schema.

  | Deprecated | Use instead | Accepted until |
  | - | - | - |
  | `thinking_mode` | `thinking_mode` | 2026-10-23 |
</Accordion>


## OpenAPI

````yaml https://api.krea.ai/openapi.json post /generate/image/krea/krea-3
openapi: 3.1.0
info:
  title: Krea API
  version: v1
servers:
  - url: https://api.krea.ai
    description: Krea API
security:
  - bearerAuth: []
tags:
  - name: General
    description: Core API operations including job management and billing information
  - name: Krea
    description: Krea-owned generation models including Krea 2
  - name: Assets
    description: >-
      Asset management endpoints for uploading and managing images, videos,
      audio files, and 3D models
  - name: Styles
    description: Style (LoRA) generation and management endpoints
  - name: Node Apps
    description: Executing custom node apps built in our nodes tool
  - name: Image
    description: Image generation endpoints
  - name: Image Enhance
    description: Image enhance endpoints
  - name: Video
    description: Video generation endpoints
  - name: Video Enhance
    description: Video enhance and upscale endpoints
  - name: 3D
    description: 3D model generation endpoints
  - name: Audio
    description: Speech and audio generation endpoints
  - name: Usage
    description: Workspace usage reporting
  - name: Utilities
    description: Miscellaneous utility endpoints
paths:
  /generate/image/krea/krea-3:
    post:
      tags:
        - Krea
      summary: Krea 3 Beta
      description: Generate and edit images with Krea 3.
      parameters:
        - name: X-Webhook-URL
          in: header
          required: false
          schema:
            type: string
            format: uri
          description: >-
            URL to receive a POST request when the job completes. The webhook
            will receive the job data including results. When signing is
            configured, deliveries are signed with ECDSA P-256 (ES256); verify
            them with the keys at https://api.krea.ai/.well-known/jwks.json (see
            X-Krea-Webhook-* headers). Treat requests without these headers as
            unauthenticated.
        - name: X-Api-Zero-Data-Retention
          in: header
          required: false
          schema:
            type: string
            enum:
              - '1'
          description: >-
            Enable Zero Data Retention for this request. Refer to the
            [documentation](/developers/zdr).
      requestBody:
        content:
          application/json:
            example:
              prompt: >-
                Ethereal lakeside, moonlit dance of spirits, reflecting romantic
                supernatural anime.
            schema:
              type: object
              properties:
                prompt:
                  type: string
                  minLength: 1
                thinking_mode:
                  type: string
                  enum:
                    - fast
                    - high
                image_urls:
                  description: Reference image URLs to use for the generation
                  maxItems: 8
                  type: array
                  items:
                    type: string
                    maxLength: 1024
                    format: uri
                    description: >-
                      Can be an external URL, base64 data URI, or uploaded asset
                      URL.
                resolution:
                  type: string
                  enum:
                    - 1K
                    - 2K
                aspect_ratio:
                  type: string
                  enum:
                    - '1:1'
                    - '4:5'
                    - '3:4'
                    - '2:3'
                    - '9:16'
                    - '5:4'
                    - '4:3'
                    - '3:2'
                    - '16:9'
                    - '21:9'
                seed:
                  type: number
                regional_edit:
                  type: boolean
              required:
                - prompt
              additionalProperties: false
      responses:
        '200':
          description: >-
            The resulting job data. This will be returned in a pending state
            until the job is completed. See
            [`/jobs/{id}`](#tag/default/get/jobs/{id}) for retrieving the
            results.
          content:
            application/json:
              schema:
                type: object
                properties:
                  job_id:
                    type: string
                    format: uuid
                    pattern: >-
                      ^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$
                  status:
                    type: string
                    enum:
                      - backlogged
                      - queued
                      - scheduled
                      - processing
                      - sampling
                      - intermediate-complete
                      - completed
                      - failed
                      - cancelled
                  created_at:
                    type: string
                    format: date-time
                    pattern: >-
                      ^(?:(?:\d\d[2468][048]|\d\d[13579][26]|\d\d0[48]|[02468][048]00|[13579][26]00)-02-29|\d{4}-(?:(?:0[13578]|1[02])-(?:0[1-9]|[12]\d|3[01])|(?:0[469]|11)-(?:0[1-9]|[12]\d|30)|(?:02)-(?:0[1-9]|1\d|2[0-8])))T(?:(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d(?:\.\d+)?(?:Z))$
                  completed_at:
                    type: 'null'
                  result:
                    type: 'null'
                  error:
                    type: object
                    properties:
                      code:
                        type: string
                      message:
                        type: string
                    required:
                      - code
                    additionalProperties: false
                  progress:
                    anyOf:
                      - type: number
                        minimum: 0
                        maximum: 1
                      - type: 'null'
                required:
                  - job_id
                  - status
                  - created_at
                  - completed_at
                  - result
                additionalProperties: false
        '400':
          description: The request body is invalid.
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                required:
                  - error
                additionalProperties: false
        '401':
          description: You are not authenticated.
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                required:
                  - error
                additionalProperties: false
        '402':
          description: You have run out of credits.
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                required:
                  - error
                additionalProperties: false
        '429':
          description: You have reached the maximum number of concurrent jobs.
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                required:
                  - error
                additionalProperties: false
components:
  securitySchemes:
    bearerAuth:
      scheme: bearer
      bearerFormat: Bearer
      type: http

````

This documentation is built and hosted on [Mintlify](https://mintlify.com), a developer documentation platform.

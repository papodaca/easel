> ## Documentation Index
> Fetch the complete documentation index at: https://docs.bfl.ml/llms.txt
> Use this file to discover all available pages before exploring further.

# Get the user's credits

> Get the user's credits



## OpenAPI

````yaml https://api.bfl.ai/openapi.json get /v1/credits
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
  /v1/credits:
    get:
      summary: Get the user's credits
      description: Get the user's credits
      operationId: get_credits_v1_credits_get
      responses:
        '200':
          description: Successful Response
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/CreditsResponse'
      security:
        - APIKeyHeader: []
components:
  schemas:
    CreditsResponse:
      properties:
        credits:
          type: number
          title: Credits
          description: User's current credit balance
      type: object
      required:
        - credits
      title: CreditsResponse
  securitySchemes:
    APIKeyHeader:
      type: apiKey
      in: header
      name: x-key

````

This documentation is built and hosted on [Mintlify](https://mintlify.com), a developer documentation platform.

# BFL `get_result`

Sources: [Get Result](https://docs.bfl.ml/api-reference/utility/get-result), [Integration guidelines](https://docs.bfl.ml/api_integration/integration_guidelines), [Errors](https://docs.bfl.ml/api_integration/errors), [FLUX 3 Text to Image](https://docs.bfl.ml/flux_3/flux3_image_generate). Field names below are from the OpenAPI for `GET /v1/get_result` on `https://api.bfl.ai`.

Poll with `GET` on the `polling_url` from the submit response. Send that URL unchanged, with the `x-key` header. The documented path is `GET /v1/get_result` and it requires query parameter `id`, the task id. Do not build that URL yourself. `api.bfl.ai` and the regional hosts hand work to whichever cluster accepted it, and only `polling_url` points there.

A 200 body is either `SettledCostResultResponse` or `ResultResponse`.

Both objects carry:

- `id`, string. Task id for retrieving the result. Required.
- `status`. Required. One of the values below.
- `result`, an untyped object or null.
- `progress`, a number or null.
- `details`, an object or null. Extra keys are allowed.
- `preview`, an object or null. Extra keys are allowed.

`SettledCostResultResponse` also requires numeric `cost`, the settled cost in credits. `ResultResponse` has no `cost`. A finished response uses the settled form.

`status` is one of:

- `Task not found`
- `Pending`
- `Reasoning`
- `Generating`
- `Request Moderated`
- `Content Moderated`
- `No Card Generated`
- `Ready`
- `Error`

`Request Moderated` and `Content Moderated` leave `result` null. `details["Moderation Reasons"]` is the array of category names. `Ready` puts the image at `result.sample`. The same object carries `result.prompt`, the prompt the model actually used, and `result.duration`, generation time in seconds.

A 422 body is `HTTPValidationError`. `detail` is an array of `ValidationError` objects. Each has `loc`, an array of strings or integers, plus `msg` and `type`.

The FLUX 3 image pages say `result.sample` lasts 1 hour. The integration guide says delivery URLs expire after 10 minutes and that hostnames under `delivery.*.bfl.ai` move. Download immediately and treat 10 minutes as the lifetime.

Most endpoints allow 24 concurrent requests. `429` means back off.

Reference images are an `http(s)` URL or base64, one to 10. Send the raw base64 string, not a `data:` URI. A reference over 16 megapixels is HTTP 400. One smaller than 256×256 is HTTP 422, and `detail` names the field.

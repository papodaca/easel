# BFL `get_result`

Sources: [Get Result](https://docs.bfl.ml/api-reference/utility/get-result), [Integration guidelines](https://docs.bfl.ml/api_integration/integration_guidelines), [Errors](https://docs.bfl.ml/api_integration/errors), [FLUX 3 Text to Image](https://docs.bfl.ml/flux_3/flux3_image_generate).

Poll with `GET` on the `polling_url` from the submit response, sent unchanged, with the `x-key` header. Do not rebuild that URL from the task id. `api.bfl.ai` and the regional hosts hand work to whichever cluster accepted it, and only `polling_url` points there.

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

`Request Moderated` and `Content Moderated` leave `result` null. `details["Moderation Reasons"]` is the array of category names. A finished response uses `SettledCostResultResponse`, which requires numeric `cost` in credits. `Ready` puts the image at `result.sample`. The same object carries `result.prompt` (the prompt the model actually used) and `result.duration` (generation time in seconds).

The FLUX 3 image pages say `result.sample` lasts 1 hour. The integration guide says delivery URLs expire after 10 minutes and that hostnames under `delivery.*.bfl.ai` move. Download immediately and treat 10 minutes as the lifetime.

Most endpoints allow 24 concurrent requests. `429` means back off.

Reference images are an `http(s)` URL or base64, one to 10. Send the raw base64 string, not a `data:` URI. A reference over 16 megapixels is HTTP 400. One smaller than 256×256 is HTTP 422, and `detail` names the field.

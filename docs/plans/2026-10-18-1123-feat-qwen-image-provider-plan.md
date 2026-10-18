---
title: Qwen Image Provider - Plan
type: feat
date: 2026-10-18
deepened: 2026-10-18
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
---

# Qwen Image Provider - Plan

## Goal Capsule

- **Objective:** A person can pick Qwen, pick `qwen-image-3.0-pro` or `qwen-image-2.1-pro`, and get one generated or edited picture into the same history as Black Forest Labs, Krea, Google, OpenAI, and xAI.
- **Means:** A Qwen provider on the synchronous multimodal generation URL, with the picture downloaded before any history row (KTD2).
- **Authority hierarchy:** R-IDs govern behavior. KTDs govern mechanism. `docs/api/qwen.create.md` and `docs/api/qwen.edit.md` are the local checklist. Where the create note says every model takes `negative_prompt` and the October 2026 editing API reference says `qwen-image-2.1-pro` does not, the API reference wins.
- **Stop conditions:** Stop if a successful response returns the picture as bytes instead of a URL. Stop if the picture URL requires the API key. Stop if the two model ids stop sharing the synchronous generation URL.
- **Execution profile:** Feature work. Prove the request bodies, the URL download, and the failure mapping with tests before treating the form as done.
- **Finishes and ships:** The implementer lands the work on the current branch.

---

## Product Contract

### Summary

Qwen joins Black Forest Labs, Krea, Google, OpenAI, and xAI. A person picks `qwen-image-3.0-pro` or `qwen-image-2.1-pro`, generates from a prompt or edits from the canvas, and one picture lands in the existing history. The form offers width, height, and, on `qwen-image-3.0-pro` only, a negative prompt. Wan stays out.

### Problem Frame

The header lists providers, and the body lists the selected provider's models. A picture that arrives as bytes can already land in history. Qwen is not a provider. The local notes describe one synchronous call that returns a temporary image URL, and that URL is the only picture in the response.

### Key Decisions

- **Wan stays out.** (session-settled: user-approved — chosen over including the Wan image models: Wan uses a separate async task API.) Governs R1.
- **Both current Qwen-Image ids.** (session-settled: user-directed — chosen over `qwen-image-3.0-pro` alone: include `qwen-image-2.1-pro`.) Governs R1, R2, R5.
- **One picture, with size on the form and prompt rewriting left on.** (session-settled: user-approved — chosen over a multi-image batch and extra rewriting controls: history keeps one picture.) Conflict: the editing API reference says `qwen-image-2.1-pro` does not support `negative_prompt`, so that field exists only on `qwen-image-3.0-pro`. Governs R6, R7.

### Requirements

**Choosing a model**

- R1. The person can generate and edit with `qwen-image-3.0-pro` and `qwen-image-2.1-pro` on the synchronous generation URL. Wan models are not listed.
- R2. With Qwen selected, the model list is those two ids, in that order.
- R3. Switching provider or model keeps the prompt, the canvas, the boxes, and the references, and drops a width, height, or negative prompt the new model does not offer.
- R4. Generate sends the model that was selected when the click happened. Black Forest Labs, Krea, Google, OpenAI, and xAI keep their current requests.

**Qwen request**

- R5. Zero prepared images send one text object. One or more send each image, then the text. `qwen-image-3.0-pro` accepts at most 3 images. `qwen-image-2.1-pro` accepts at most 10. Past that count, Generate stays off and no request is sent.
- R6. The form offers width and height on both models, and a negative prompt on `qwen-image-3.0-pro` only. Both dimensions empty omits `parameters.size`. Both set requires a pixel count from 262144 through 4194304 and a long-side to short-side ratio from 1 through 8, and a miss disables Generate. One dimension set disables Generate. An empty negative prompt is omitted. Prompt rewriting, thinking, watermark, seed, and image count are omitted.
- R7. The request asks for one picture, and history keeps the first image URL in the response.
- R8. References travel as data URIs, canvas first when a canvas image exists. Generate stays off when a prepared image is missing, when an image that would be sent has a long side more than 10 times its short side, or when an encoded image is over 10 MB. The over-cap line is `At most 3 images.` or `At most 10 images.`

**Results**

- R9. A successful Qwen picture appears in history and the strip the same way a finished xAI picture does, and the row is written only after the PNG bytes are stored. The file is `result.png`.
- R10. A `DataInspectionFailed` moderation block, a rejected key, an unknown model, and a body with no usable image leave a failed history row and no image file. A non-HTTPS URL, a URL with userinfo, a literal private, loopback, or link-local address, a redirect, and a download whose bytes are not a PNG are the no-usable-image case. The picture GET sends no API key. Its debug line and its transport error keep the scheme, host, and path, and replace the query with `<redacted>`, while the GET itself still sends the original query.
- R11. A missing key disables Generate before HTTP, with the existing no-key line. Settings has a Qwen key row. The key test is a GET of the generation URL, treats 401 as an invalid key, and does not generate an image. The status bar does not show a Qwen balance.
- R12. A rate limit leaves no history row, shows `rate limited`, and does not send a second request.

### Success Criteria

- Picking Qwen and generating with an empty canvas puts one PNG in the history strip.
- An edit with one source image on `qwen-image-3.0-pro`, and an edit with four source images on `qwen-image-2.1-pro`, each put one PNG in that strip.
- Four source images on `qwen-image-3.0-pro` leave Generate off.
- `qwen-image-2.1-pro` shows no negative prompt, and a FLUX 3 generate still posts to Black Forest Labs.

### Scope Boundaries

- Two models, `qwen-image-3.0-pro` and `qwen-image-2.1-pro`. One picture per request. Synchronous generation and edit on one URL. Width, height, and a negative prompt on the model that accepts it. A settings key. A result in the existing history.
- Boxes stay on the canvas and stay out of the request.
- The existing 4 megapixel JPEG prep stays. Qwen does not add a resolution tier for that prep.

#### Deferred to Follow-Up Work

- Wan image models, async task polling, image sets, and color palettes.
- More than one output picture.
- A control for prompt rewriting, thinking, seed, watermark, or `prompt_extend_mode`.
- The Model Studio models list on `dashscope-intl.aliyuncs.com`, and `POST /api/v1/tokens`.
- Other Qwen image ids, including ids retired on 10 October 2026.
- A DNS check that a result hostname is a public address.
- A Qwen balance in the status bar.

### Acceptance Examples

- AE1. Empty canvas, prompt set, key present.
  - **Covers:** R5, R7, R9
  - **Given:** Qwen and `qwen-image-3.0-pro` are selected, both dimensions are empty, and no source image is loaded.
  - **When:** the person generates.
  - **Then:** one PNG is in the history strip and no second picture is stored.
- AE2. One source image.
  - **Covers:** R5, R8
  - **Given:** the canvas holds one image and `qwen-image-3.0-pro` is selected.
  - **When:** the person generates.
  - **Then:** the edit uses that image as a data URI, and one PNG replaces it in the strip.
- AE3. Four source images on the smaller cap.
  - **Covers:** R5, R8
  - **Given:** four images were loaded under `qwen-image-2.1-pro`, and the person switches to `qwen-image-3.0-pro`.
  - **When:** the form refreshes.
  - **Then:** Generate is off, the line is `At most 3 images.`, and no request is sent.
- AE4. Negative prompt on the model that rejects it.
  - **Covers:** R3, R6
  - **Given:** `qwen-image-3.0-pro` has a negative prompt, and the person switches to `qwen-image-2.1-pro`.
  - **When:** the form refreshes.
  - **Then:** the negative prompt field is gone, and a later generate does not send `negative_prompt`.
- AE5. Moderation with an image URL present.
  - **Covers:** R10
  - **Given:** the response is HTTP 400 with code `DataInspectionFailed` and also carries an image URL.
  - **When:** the job finishes.
  - **Then:** history has a failed row, no image file, and no picture GET.

### Sources

- `docs/api/qwen.create.md` and `docs/api/qwen.edit.md`. These name the synchronous URL, bearer auth, `DASHSCOPE_API_KEY`, `parameters.size` as `width*height`, the pixel and aspect bounds, the image caps, the temporary image URL, and the 24 hour expiry.
- `https://docs.qwencloud.com/api-reference/image-generation/qwen-text-to-image` and `https://docs.qwencloud.com/api-reference/image-generation/qwen-image-editing`, fetched 18 October 2026. `qwen-image-2.1-pro` does not support `negative_prompt`. Omitting `size` is valid. Defaults are `n` 1, `prompt_extend` true, `enable_thinking` true, and `watermark` false. Output is PNG.
- `https://docs.qwencloud.com/api-reference/preparation/error-messages`. HTTP 400 `DataInspectionFailed` and `data_inspection_failed` are moderation. HTTP 401 `InvalidApiKey` and `invalid_api_key` are a bad key. HTTP 404 `ModelNotFound` and `model_not_found` are an unknown model. HTTP 429 `Throttling` and `Throttling.*` are rate limits. The catalog also lists HTTP 400 with message `Model not exist.`
- `https://docs.qwencloud.com/resources/faq-images-videos`. The object-storage host list is not published and can change.
- `https://www.alibabacloud.com/help/en/model-studio/list-models`, updated 11 October 2026. `GET /api/v1/models` is on `dashscope-intl.aliyuncs.com` and regional Model Studio hosts, not on `maas.qwencloudapi.com`.
- `https://docs.qwencloud.com/changelog/model-deprecations/old-models`. `qwen-image-3.0-pro` and `qwen-image-2.1-pro` are not in the 10 October 2026 retirement table.

---

## Planning Contract

### Key Technical Decisions

- KTD1. One provider module serves both specs on `POST https://maas.qwencloudapi.com/api/v1/services/aigc/multimodal-generation/generation`. Auth is bearer. The API host allowlist is the exact host `maas.qwencloudapi.com`. `ProviderId::Qwen` serializes as `qwen`. The env var is `DASHSCOPE_API_KEY`. Registration is appended after xAI. The content array is built in the Qwen module. `map_images` stays unused, because it emits shapes this API does not take.
- KTD2. A picture URL cannot ride the two branches `finish_submit` already has. `inline_image` returns bytes and `finish_inline` writes history immediately, and `parse_submit` returning `Ok` always calls `history.begin` and then polls. A new provider method, defaulting to no URL, is checked between those branches. It has three outcomes. No image URL in the body returns `NoImage` through `finish_unpolled`. An image URL that fails an R10 check does the same, and does not GET and does not call `parse_submit`. An image URL that passes stays in `Submitting` and GETs through `download_request`. A 200 whose body starts with the PNG signature calls `finish_inline` with mime `image/png`. Any other GET result, including a transport error, a 302, and a non-PNG body, calls `finish_unpolled` with `NoImage`. `start_download` stays unused, because it needs a poll row and it accepts JPEG and WebP. `parse_submit` returns a protocol error only when a 200 still contains an image URL after the new method reported that it had none, so a missed branch cannot poll. `allows_url` stays the API host list. R10 redacts the query on `log_request`, on the status line in `execute`, and on `TransportError`'s display text. `strip_url` is the wrong helper, because it drops the host and the path. The GET still sends the original query.
- KTD3. `parameters.size` is the string `width*height` when R6 accepts both dimensions, and the field is absent otherwise. `negative_prompt` is sent only for `qwen-image-3.0-pro` when the text is non-empty. The R6 and R8 checks run in the model's submit validation. The width and height controls do not use the FLUX.2 step of 16, the 64-pixel floor, the 4 megapixel area rule, or a maximum of 4096. Their help states the R6 bounds. Upload prep still reads `resolution` and therefore stays on the `1k` tier.
- KTD4. HTTP 401 is `Auth`. HTTP 404, and HTTP 400 whose message is `Model not exist.`, are `ModelNotFound`. HTTP 400 whose code is `DataInspectionFailed` or `data_inspection_failed` is `Blocked`, including when an image URL is also present. HTTP 429, and a body whose code is `Throttling` or starts with `Throttling.`, are `RateLimited` with no retry delay. Any other non-image status is `ProviderError::Http` and writes no history row. The settings probe is one new arm of `key_probe`: `GET` of the generation URL, Qwen auth, empty body. `GET /api/v1/models` would send the key to `dashscope-intl.aliyuncs.com`, which the API allowlist rejects. `POST /api/v1/tokens` mints a credential and is not a GET. A POST of the generation URL is a generate, which R11 excludes. `test_key` stays as it is. 401 is invalid. Qwen is not on the Black Forest Labs and Krea 403 arm. 400, 404, 405, and 429 toast key accepted. 405 is the likely answer from a GET of a POST route.

### High-Level Technical Design

```mermaid
sequenceDiagram
  participant Form
  participant Runner
  participant API
  participant Store
  Form->>Runner: Generate
  Runner->>API: POST generation, bearer
  API-->>Runner: image URL
  Runner->>Store: GET url, no key
  Store-->>Runner: PNG bytes
  Runner->>Runner: history row after the bytes
```

```mermaid
flowchart TB
  A[Submit response] --> B{Status and code}
  B -->|200 and a PNG URL| C[Download, then inline history]
  B -->|200, no usable URL| D[Failed row, NoImage]
  B -->|400 moderation| E[Failed row, Blocked]
  B -->|401| F[Failed row, Auth]
  B -->|404 or Model not exist| G[Failed row, ModelNotFound]
  B -->|429 or Throttling| H[No history row]
  B -->|other| H
```

### Sequencing

U1 lands the id and the two specs. U2 builds the JSON and classifies the response. U3 is the download, and it waits on U2. U4 may be written after U2. The form is not done until U3's download tests pass.

---

## Implementation Units

### U1. Register Qwen

- **Goal:** Qwen appears after xAI, with two model ids and a settings key, and the other providers still resolve.
- **Requirements:** R1, R2, R4, R11
- **Dependencies:** none
- **Files:**
  - `crates/easel-core/src/model_spec.rs`
  - `crates/easel-core/src/providers/mod.rs`
  - `crates/easel-core/src/providers/qwen/mod.rs`
  - `crates/easel-core/src/history.rs`
  - `crates/easel/src/settings/mod.rs`
  - `crates/easel/src/settings/credentials.rs`
  - `crates/easel/src/net/mod.rs`
  - `crates/easel/src/jobs/runner.rs`
  - `crates/easel/src/side_panel/validation.rs`
- **Approach:**
  1. Add `ProviderId::Qwen` per KTD1, and grow every exhaustive match.
  2. Append two specs. `supports_boxes` is false. `beta` is false. Image slots are 3 and 10.
  3. Store the key under `https://maas.qwencloudapi.com`. The probe is the KTD4 GET.
- **Patterns to follow:** `ProviderId::Xai` in `crates/easel-core/src/model_spec.rs`, `GrokImagine::model_spec` in `crates/easel-core/src/providers/xai/mod.rs`, and the xAI credential test in `crates/easel/src/settings/credentials.rs`.
- **Test scenarios:**
  - `allows_url` accepts `https://maas.qwencloudapi.com` and rejects `https://dashscope-intl.aliyuncs.com` and `https://api.x.ai`.
  - `resolve_model` returns the Qwen provider for both ids, and `flux-3-image` still returns Black Forest Labs.
  - A history row for `qwen-image-3.0-pro` stores the provider name `qwen`.
  - `DASHSCOPE_API_KEY` fills the session the same way `XAI_API_KEY` does, and the stored credential uses the Qwen base URL.
  - The key probe is a GET of the generation URL with Qwen auth and an empty body.
  - `missing_key_line` for Qwen matches the existing no-key wording.
- **Verification:** Both ids resolve to Qwen, a FLUX id still resolves to Black Forest Labs, and the probe does not POST.

### U2. Build the synchronous request

- **Goal:** Generate and edit post one URL, and the body follows R5, R6, and R7.
- **Requirements:** R4, R5, R6, R7, R8
- **Dependencies:** U1
- **Files:**
  - `crates/easel-core/src/providers/qwen/mod.rs`
- **Approach:**
  1. Zero images send one text object. Images, when present, are data URIs before that text.
  2. Copy size and negative prompt per KTD3. Omit `n`, `prompt_extend`, `enable_thinking`, `watermark`, `seed`, and `prompt_extend_mode`.
  3. Cap `reference_steps` at the selected spec's image max.
- **Execution note:** Write the body tests first and make them pass before the form refusal in U4.
- **Patterns to follow:** `crates/easel-core/src/providers/xai/mod.rs` for data URIs and for branching on the prepared-image count. `dimension` in `crates/easel-core/src/providers/bfl/flux2_pro.rs` for the width and height fields, without that file's cross rules.
- **Test scenarios:**
  - Zero images post the generation URL with `model` `qwen-image-3.0-pro`, one text object, and no `image` object. The body has no `n`, no `prompt_extend`, no `size`, and no `X-DashScope-Async` header.
  - Width 1024 and height 1536 send `parameters.size` `1024*1536` on a generation and on an edit.
  - Width 0 and height 0 omit `parameters.size`. Width 1024 and height 0 produce a field error and no request.
  - Width 2048 and height 256 is accepted. Width 4096 and height 4096 is rejected. Width 2048 and height 200 is rejected.
  - One image sends `content` length 2, the data URI first and the text second.
  - Three images on `qwen-image-2.1-pro` send three image objects, canvas first, then the text.
  - A non-empty negative prompt is sent for `qwen-image-3.0-pro` and omitted for `qwen-image-2.1-pro`. An empty one is omitted for both.
  - Four prepared images on `qwen-image-3.0-pro` produce a validation error and no image objects in a built body.
- **Verification:** The generation and edit bodies match R5 and R6, and a FLUX request is unchanged.

### U3. Download the result into history

- **Goal:** The first image URL becomes one PNG in history, and the failure cases in R10 and R12 do not fetch or do not store.
- **Requirements:** R7, R9, R10, R12
- **Dependencies:** U2
- **Files:**
  - `crates/easel-core/src/providers/qwen/mod.rs`
  - `crates/easel/src/jobs/runner.rs`
  - `crates/easel/src/jobs/qwen_tests.rs`
- **Approach:** Follow KTD2 and KTD4. An R10 rejection calls `finish_unpolled` with `NoImage` and does not GET. Check the PNG signature before `finish_inline`. Do not call `history.begin`, `start_download`, or `finish_download`.
- **Patterns to follow:** `crates/easel/src/jobs/xai_tests.rs` for a one-shot submit that does not poll. `download_request` in `crates/easel-core/src/providers/mod.rs` for the unauthenticated GET. `crates/easel/src/jobs/openai_tests.rs` for the scripted transport.
- **Test scenarios:**
  - Covers AE1. A 200 whose first content image is `https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/a.png` is followed by one unauthenticated GET, writes `result.png`, stores provider `qwen`, and does not poll.
  - A 200 with two image URLs stores the first PNG and does not GET the second.
  - A 200 with `http://example.com/a.png` records `NoImage`, writes a failed row, and performs no GET.
  - A 200 with `https://user:pass@example.com/a.png` records `NoImage` and performs no GET.
  - A 200 with `https://127.0.0.1/a.png` records `NoImage` and performs no GET.
  - A picture GET that returns 302 records `NoImage` and does not follow the redirect.
  - A picture GET that returns JPEG bytes records `NoImage` and writes no image file.
  - A picture GET whose `Content-Type` is `image/png` and whose bytes are not a PNG records `NoImage`.
  - A picture URL with a query is requested with that query, and the debug line for that GET does not contain the query.
  - Covers AE5. HTTP 400 with code `DataInspectionFailed` and an image URL records `Blocked`, writes no file, and performs no GET.
  - HTTP 400 with code `data_inspection_failed` records `Blocked`.
  - HTTP 400 with message `Model not exist.` records `ModelNotFound` and a failed row.
  - HTTP 404 with code `model_not_found` records `ModelNotFound`.
  - A 401 records `Auth` and a failed row with no image file.
  - A 429 records `RateLimited`, writes no history row, and posts once.
  - HTTP 400 with code `InvalidParameter` writes no history row.
  - Dismiss during the post drops a late 200, and no picture GET runs.
- **Verification:** A PNG URL lands without a poll row, a moderated body is not fetched, and a loopback URL is not fetched.

### U4. Form fields and the image gates

- **Goal:** The form shows R6, and a count, aspect, or file past R8 cannot generate.
- **Requirements:** R3, R5, R6, R8, R11
- **Dependencies:** U2. Not done until U3's download tests pass.
- **Files:**
  - `crates/easel-core/src/providers/qwen/mod.rs`
  - `crates/easel/src/side_panel/validation.rs`
  - `crates/easel/src/side_panel/form_tests.rs`
- **Approach:** Put width and height on both specs. Put the negative prompt on `qwen-image-3.0-pro` only. Disable Generate from the selected spec's image max, from a missing prepared image, from the 10:1 input ratio, and from a 10 MB encoded image. Do not route the overflow through the truncation notice.
- **Patterns to follow:** `openai_caps_disable_generate` in `crates/easel/src/side_panel/form_tests.rs`, and `refusal_messages` in `crates/easel-core/src/providers/openai/mod.rs`.
- **Test scenarios:**
  - With `qwen-image-3.0-pro` selected, the form offers width, height, and a negative prompt, and width and height start empty.
  - With `qwen-image-2.1-pro` selected, the form offers width and height and no negative prompt.
  - Covers AE3. Four prepared images on `qwen-image-3.0-pro` disable Generate, the line is `At most 3 images.`, and the truncation notice is absent.
  - Ten prepared images on `qwen-image-2.1-pro` leave Generate enabled. Eleven disable it with `At most 10 images.`
  - Covers AE4. A negative prompt set on `qwen-image-3.0-pro` is absent after a switch to `qwen-image-2.1-pro`, and the prompt and canvas remain.
  - Width 1024 with height empty disables Generate with `Set both width and height, or leave both at 0.` Width 2048 and height 256 do not. Width 100 and height 100 disables Generate with `Width times height must be from 262144 through 4194304.`
  - A long side more than 8 times the short side disables Generate with `The long side must be at most 8 times the short side.`
  - An encoded image over 10 MB disables Generate with `An encoded image is over 10 MB.` An image whose long side is more than 10 times its short side disables Generate with `An image's long side is more than 10 times its short side.`
  - A missing prepared image disables Generate with `A prepared image is missing.`
  - An empty prompt disables Generate with the existing empty-prompt line.
  - A missing Qwen key disables Generate with the existing no-key line.
  - Switching from Qwen back to FLUX 3 keeps the prompt and the canvas, and the FLUX request is unchanged.
- **Verification:** Four images cannot generate on `qwen-image-3.0-pro`, ten can on `qwen-image-2.1-pro`, and the negative prompt is absent on `qwen-image-2.1-pro`.

---

## Verification Contract

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run -p easel-core` covers the request bodies.
- `cargo nextest run -p easel` covers the response classification, the runner, the form, and the key probe.

---

## Definition of Done

- U1 through U4 meet their verification lines.
- Black Forest Labs, Krea, Google, OpenAI, and xAI requests are unchanged.
- The four commands in the Verification Contract pass.
- Abandoned experimental code is not left in the diff.

---

## System-Wide Impact

`ProviderId` is matched exhaustively, so U1 touches every arm: settings, the credential store, the key map, history, the runner cache key, and the missing-key label. Credentials stay under `https://maas.qwencloudapi.com`. The job runner's inline path is shared. Qwen reaches it only after the PNG bytes exist, and it must not start a poll (KTD2). `Auth`, `Blocked`, `ModelNotFound`, and `NoImage` write a failed history row. `RateLimited` and other HTTP failures write none (KTD4). The status bar requests a balance only for Black Forest Labs, so a Qwen selection shows none (R11). Upload prep still keys off `resolution`, so a Qwen `size` does not change the encoded input. Other providers' request builders stay as they are.

---

## Risks & Dependencies

- The settings test treats every status other than 401 as a valid key. A GET of the generation URL may return 400 for a good key, which still toasts key accepted, matching OpenAI today. If a bad key also returns a status other than 401, the toast is wrong and the generate POST remains the place a 401 becomes `Auth`.
- `enable_thinking` defaults to true when omitted, and it runs only while prompt rewriting is on. R6 omits both, so a generate can take longer than the other synchronous providers. The form has no switch for it.
- The picture host is not on an allowlist. R10 blocks non-HTTPS URLs, userinfo, and literal private addresses. A public hostname that resolves to a private address is still fetched. Redirects are already refused. `log_request` prints the full URL at debug, and header redaction does not cover the query. R10 redacts that query on the picture GET. The request still sends it.
- Sending `negative_prompt` to `qwen-image-2.1-pro` is unsupported, and the docs do not say whether the API ignores it or returns 400. KTD3 omits it.
- A 200 that returns an image URL and no PNG bytes, or a URL that expires before the GET, is `NoImage` and a failed row. The plan does not retry the POST.
- Sibling moderation codes such as `ip_infringement_suspect` are not mapped. They fall through to HTTP and write no history row.
- The mainline deprecation page fetched on 18 October 2026 did not render a full table. The old-models table does not name either id. A later retirement would fail as `ModelNotFound`.

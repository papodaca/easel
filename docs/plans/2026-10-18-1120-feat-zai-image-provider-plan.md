---
title: Z.ai Image Provider - Plan
type: feat
date: 2026-10-18
deepened: 2026-10-18
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-plan-bootstrap
execution: code
---

# Z.ai Image Provider - Plan

## Goal Capsule

- **Objective:** A person can pick Z.ai, pick glm-image, and get one generated picture into the same history as Black Forest Labs, Krea, Google, OpenAI, and xAI.
- **Means:** A synchronous generations call, then a download of the picture link (KTD3, KTD4).
- **Authority hierarchy:** R-IDs govern behavior. KTDs govern mechanism. `docs/api/zai.create.md` is the local checklist. The 18 October 2026 docs.z.ai pages fill the gaps that file leaves, named in Sources.
- **Stop conditions:** Stop if a successful body returns the picture as bytes instead of a URL. Stop if the file is only reachable by following a redirect.
- **Execution profile:** Feature work. Prove the request body, the download, and the failure mapping with tests before treating the form as done.
- **Finishes and ships:** The implementer lands the work on the current branch.

---

## Product Contract

### Summary

Z.ai joins the existing image providers with glm-image. Generate puts one picture in the existing history. The form offers quality and the documented sizes. Settings gets a key, and the temporary picture link is saved locally as a file.

Edits, video, custom pixel sizes, and the end-user id stay out.

### Problem Frame

The header lists providers, and the body lists the selected provider's models. A picture can already land in history when the provider returns bytes on the POST, or when a poll later returns a URL. Z.ai is not a provider. The local notes describe one model, glm-image, and a success body whose picture is a temporary URL.

### Key Decisions

- **Synchronous generate, then download the picture.** (session-settled: user-approved — chosen over an async job and a poll: the async call only offers hd.) Governs R5, R7, R9.
- **Text to image only.** (session-settled: user-approved — chosen over sending the canvas as an edit: the notes have no edit route.) Governs R5, R8.

### Requirements

**Choosing a model**

- R1. The person can generate with `glm-image`.
- R2. With Z.ai selected, the model list is that one id.
- R3. Switching provider or model keeps the prompt, the canvas, the boxes, and the references, and drops a quality or size the new model does not offer.
- R4. Generate sends the model that was selected when the click happened. Black Forest Labs, Krea, Google, OpenAI, and xAI keep their current requests.

**Z.ai request**

- R5. The JSON body sends `model`, `prompt`, `quality`, and `size`. It does not send images or `user_id`. A loaded canvas stays on screen.
- R6. The form offers quality `hd` and `standard`, default `hd`, and sizes `1280x1280` (default), `1568x1056`, `1056x1568`, `1472x1088`, `1088x1472`, `1728x960`, `960x1728`. The stored value is sent.
- R7. History keeps one picture, from the first URL in `data`.
- R8. Generate stays available when images are loaded. The existing truncation notice counts zero of those images as sent. No reference is uploaded.

**Results**

- R9. A successful download appears in history the same way an inline picture does. PNG, JPEG, and WEBP are named from the file bytes. A `content_filter` array on a 200 that also has a URL does not change that outcome and does not add a toast.
- R10. A rejected key, a safety block, a 200 with no usable URL, and a download that does not yield a stored file leave a failed history row and no image file. The card text for each case is KTD5 and KTD4.
- R11. A missing key disables Generate with the existing no-key line. Settings has a Z.ai key row. The status bar does not show a Z.ai balance.
- R12. A rate limit leaves no history row, shows `rate limited`, and does not send a second POST.

### Success Criteria

- Picking Z.ai and generating with an empty canvas puts one picture in the history strip.
- A loaded canvas is absent from the request, and one picture still lands in that strip.
- A FLUX 3 generate still posts to Black Forest Labs.

### Scope Boundaries

- One model, `glm-image`. One picture per request. Synchronous text-to-image. Quality and the seven sizes in R6. A settings key. A downloaded file in the existing history.
- Boxes stay on the canvas and stay out of the request.
- The picture URL is not stored. Dismiss during the POST or the download drops a late response and writes no row.

#### Deferred to Follow-Up Work

- The async generations call and result polling.
- Following redirects to fetch the file.
- A key test that proves the key can call `glm-image`.
- Showing `content_filter` in the history card.
- More than one retry of a 404 on the file URL.
- Custom width and height outside the R6 list, and `user_id`.

### Acceptance Examples

- AE1. Empty canvas, prompt set, key present.
  - **Covers:** R5, R7, R9
  - **Given:** Z.ai and `glm-image` are selected and no source image is loaded.
  - **When:** the person generates.
  - **Then:** one picture is in the history strip and no second picture is stored.
- AE2. Canvas loaded.
  - **Covers:** R5, R8
  - **Given:** the canvas holds one image.
  - **When:** the person generates.
  - **Then:** the request has no image, the notice counts zero images sent, and one picture lands in the strip.
- AE3. Safety block.
  - **Covers:** R10
  - **Given:** the generations call returns HTTP 400 with code 1301.
  - **When:** the job finishes.
  - **Then:** history has a failed row and no image file.
- AE4. Picture link on an unexpected host.
  - **Covers:** R9, R10
  - **Given:** the 200 body carries a URL whose host is not on the allow list.
  - **When:** the job finishes.
  - **Then:** no file GET is sent, and history has a failed row with no image file.
- AE5. Rate limit.
  - **Covers:** R12
  - **Given:** the generations call returns HTTP 429 with code 1302.
  - **When:** the job finishes.
  - **Then:** history has no row, and no second POST is sent.

### Sources

- `docs/api/zai.create.md`. Synchronous `POST /paas/v4/images/generations` on `https://api.z.ai/api`, bearer auth, model `glm-image`, quality `hd` or `standard`, the R6 sizes, and a 200 body of `created`, `data[].url`, and optional `content_filter`. The URL expires after 30 days. No base64 field.
- `docs/api/zai.async.create.md` and `docs/api/zai.async.results.md`. Async create returns a task id and allows only `hd`. The result GET is not the generations response. Both stay out of this plan except as the settings probe URL in KTD1.
- `https://docs.z.ai/openapi.json`, fetched 18 October 2026. Confirms the sync schema, the missing base64 field, and that the only documented GET is `GET /paas/v4/async-result/{id}`.
- `https://docs.z.ai/api-reference/api-code.md`, fetched 18 October 2026. Business codes used in KTD5. No `retry_after`.
- `https://docs.z.ai/guides/image/glm-image.md`, fetched 18 October 2026. The caller downloads the returned URL. `hd` is about 20 seconds. No shutdown date.
- `https://github.com/SillyTavern/SillyTavern/issues/5480`, April 2026. One observed file host was `maas-watermark-prod-new.cn-wlcb.ufileos.com`, with `UCloudPublicKey` in the query. The maintainer said the global region was still a `z.ai` host. Official docs do not name the file host.

---

## Planning Contract

### Key Technical Decisions

- KTD1. Register Z.ai the way xAI is registered. `ProviderId` variant `Zai`, serde name `zai`, display name `Z.ai`, bearer auth, base URL `https://api.z.ai`, env `ZAI_API_KEY`. History stores the provider name `zai`. The settings probe is `GET https://api.z.ai/api/paas/v4/async-result/easel-key-probe` with bearer auth. Docs publish no success status for an unknown task id, so the plan does not treat 200 as the proof of a good key. A 401 is an invalid key. A 429 is not accepted. The toast is `couldn't reach the API`, not `invalid key` and not `key accepted`. Any other HTTP status is accepted. The probe does not prove the key can generate, and the toast does not say so. Governs R1, R2, R11.
- KTD2. The generations body is the four R5 fields, posted to `https://api.z.ai/api/paas/v4/images/generations`. Quality and size come from the schema, including the R6 defaults. `supports_boxes` is false. `ImageSlots` is a list with max 0, so Generate stays on and the existing notice applies. `reference_steps` is empty, and `build_submit` ignores `JobRequest.images`. The POST `max_body_bytes` is at most 1_000_000. That is the transport cutoff for the 60 second budget. A cap of 1 MiB is already over it, and the JSON size does not select the budget. The file GET keeps the existing 100 MiB cap. (session-settled: user-approved — chosen over sending the canvas as an edit: the notes have no edit route.) Governs R4, R5, R6, R8.
- KTD3. A 200 with an allowlisted URL is not inline bytes and is not `Submitted`. The provider reports that URL, and the runner GETs it. The slot stays out of the downloading state, so the poll deadline and the poll download path never run. After a good GET, the inline finish stores the bytes, with an empty interaction id and an empty suggestion. That path mints an id, names the file from the mime, and writes no poll URL, so resume skips the job. A dismiss during the POST or the GET drops the late response and writes no row. Quit during either request leaves nothing to resume. `finish_submit` today only stores inline bytes or starts a poll, so this is a third branch. The single 404 retry is a second GET in this branch only. The shared client does not grow a retry. (session-settled: user-approved — chosen over an async job and a poll: the async call only offers hd.) Governs R7, R9, R10.
- KTD4. Allow `https` hosts `api.z.ai`, any host ending in `.z.ai`, and the exact host `maas-watermark-prod-new.cn-wlcb.ufileos.com`. Do not allow a `.ufileos.com` suffix. Reject a URL that names a port other than 443. Check `allows_url` on the same string the GET will use, and do not call the transport when it rejects the URL. Keep the query string. The GET is the existing download request, so `auth` stays unset and the POST headers are not copied. Credentialed Z.ai requests stay on `https://api.z.ai`. Leave redirects disabled. Any 3xx fails immediately, with no second request, so a `Location` on another host is never fetched. A 404 is retried once, on the same URL, with no credential. A second 404, a timeout, a body over 100 MiB, a 401 or 403 from the file host, or bytes that are not PNG, JPEG, or WEBP fail the job. Sniff with `sniff_format` and ignore `Content-Type`. A download failure writes a failed history row through the unpolled writer, with protocol text `image download failed`, and does not mark the job expired. Every debug line for that file GET keeps the path and shows `<redacted>` when a query is present. The GET still sends the original query. Other providers' request logs stay as they are. Governs R9, R10.
- KTD5. Classify the generations response before any GET. Read `code` and `message` from the top-level object, and from an `error` object when `code` is an integer or a decimal string. A 200 with a URL, including one that also has `content_filter`, downloads per KTD3 and KTD4. A 200 with no URL is `NoImage` and writes a failed row. A URL outside KTD4 is `NoImage`, no GET, and a failed row. HTTP 400 code 1301 is `Blocked`, failed row, card text `blocked by safety`. HTTP 401 is `Auth`, failed row. HTTP 429 codes 1302 and 1305 are `RateLimited` with no retry delay, one POST, and no history row. The card text is `rate limited`. HTTP 429 codes 1113, 1308, 1309, 1310, 1311, 1314, 1315, and 1316 through 1321 are plan, balance, or reset-time rejections a second click will not clear. They are `Validation` whose message is the server text, and the provider marks that result as finished for this key so the runner writes a failed row. The business code is not re-read in the runner. Any other POST failure uses the server `message` on `Validation` when that string is non-empty, and otherwise the existing HTTP status text. Those results are not marked finished, so they write no history row. `finish_submit` today writes a row only for `Blocked`, `Auth`, `ModelNotFound`, and `NoImage`, which is why the finished flag exists. Governs R9, R10, R12.

### High-Level Technical Design

The POST never carries the canvas. The picture file is a second request, then the existing inline history write.

```mermaid
flowchart TB
  form[Quality and size] --> post[POST generations]
  canvas[Loaded canvas] --> stay[Stays on screen]
  post --> url[Picture URL]
  url --> get[GET with no key]
  get --> file[One history file]
```

```mermaid
flowchart TB
  resp{Generations response}
  resp -->|allowlisted URL| get[GET the file]
  resp -->|no URL or bad host| noimage[Failed row, no GET]
  resp -->|1301 or 401| failed[Failed row]
  resp -->|429 rate limit| live[No history row]
  resp -->|429 plan or balance code| message[Failed row, server message]
  get -->|PNG, JPEG, or WEBP| save[Inline history file]
  get -->|404| retry[Same GET once]
  retry -->|image bytes| save
  retry -->|still failing| dlfail[Failed row, not expired]
  get -->|3xx, timeout, or bad bytes| dlfail
```

### Sequencing

U1 lands the provider id so later units can name it. U2 builds the request and the form. U3 classifies the POST. U4 depends on U3 and performs the download.

---

## Implementation Units

### U1. Register the provider

- **Goal:** Z.ai appears in the provider menu and in settings, and history can name it.
- **Requirements:** R1, R2, R11
- **Dependencies:** none
- **Files:**
  - `crates/easel-core/src/model_spec.rs`
  - `crates/easel-core/src/providers/mod.rs`
  - `crates/easel-core/src/history.rs`
  - `crates/easel/src/settings/mod.rs`
  - `crates/easel/src/settings/credentials.rs`
  - `crates/easel/src/net/mod.rs`
  - `crates/easel/src/jobs/runner.rs`
  - `crates/easel/src/side_panel/validation.rs`
  - `crates/easel/src/side_panel/switch_tests.rs`
  - `crates/easel/src/status_bar.rs`
- **Approach:** Fill every exhaustive `ProviderId` match. Follow the xAI arm for auth, the settings row, the missing-key label, and `cache_key`. Credentials stay keyed by the base URL. The probe URL and the accepted statuses are KTD1. The host list is KTD4, so a result URL on the file host passes `allows_url` and a lookalike host does not.
- **Patterns to follow:** The xAI arms in those files, and `xai_does_not_ask_for_credits` in `crates/easel/src/status_bar.rs`.
- **Test scenarios:**
  - The host list accepts `https://api.z.ai/api/paas/v4/images/generations`, `https://cdn.z.ai/a.png`, and `https://maas-watermark-prod-new.cn-wlcb.ufileos.com/a.png?UCloudPublicKey=token`.
  - The host list rejects `http://cdn.z.ai/a.png`, `https://user:pass@cdn.z.ai/a`, `https://api.z.ai.evil.example/a.png`, and `https://evil.ufileos.com/a.png`.
  - The key probe is `GET https://api.z.ai/api/paas/v4/async-result/easel-key-probe` with bearer auth and no key in the URL.
  - A 401 probe is an invalid key. A 429 probe toasts `couldn't reach the API` and does not toast `invalid key` or `key accepted`. A 404 probe is accepted.
  - With Z.ai selected and a key present, the status bar does not request a balance.
  - With a key present, Z.ai appears in the provider menu.
- **Verification:** The menu can list Z.ai, and settings can store a key for `https://api.z.ai`.

### U2. Build the generations request

- **Goal:** Generate posts the R5 JSON, and the form offers R6.
- **Requirements:** R3, R4, R5, R6, R8
- **Dependencies:** U1
- **Files:**
  - `crates/easel-core/src/providers/zai/mod.rs`
  - `crates/easel-core/src/providers/mod.rs`
  - `crates/easel/src/side_panel/form_tests.rs`
- **Approach:**
  1. Add one `ModelSpec` and one provider type, registered in `ModelRegistry::specs` and `resolve_model`.
  2. Post the four R5 fields per KTD2. Do not add `user_id`.
  3. Put the R6 selects on the spec. A stored value survives a model switch only when the new schema lists it.
  4. Return no reference steps, and ignore prepared images, per KTD2.
- **Execution note:** Write the body test first and make it pass before the form cases.
- **Patterns to follow:** The select helper in `crates/easel-core/src/providers/openai/mod.rs`. The xAI registration in `crates/easel-core/src/providers/mod.rs`.
- **Test scenarios:**
  - A prompt, quality `standard`, and size `1728x960` post `https://api.z.ai/api/paas/v4/images/generations` with bearer auth, `model` `glm-image`, and those three values. The body has no image field and no `user_id`.
  - Defaults post quality `hd` and size `1280x1280`. The request's `max_body_bytes` is at most 1_000_000.
  - A `JobRequest` that already holds image bytes still posts a body with no image field.
  - `reference_steps` for a prepared image is empty.
  - With Z.ai selected, the form offers the R6 qualities and sizes, and the initial values are `hd` and `1280x1280`.
  - One loaded image, a prompt, and a key leave Generate enabled, and the notice is `0 of 1 images will be sent`.
  - A canvas plus two references, a prompt, and a key leave Generate enabled, and the notice is `0 of 3 images will be sent`.
  - An empty prompt disables Generate with the existing empty-prompt line.
  - With the Z.ai model selected, a missing key disables Generate with the existing no-key line, and a present key does not. A Z.ai key alone does not enable Generate while another model is selected.
  - A stored quality the Z.ai schema does not list becomes `hd` on switch. A listed size `1280x1280` carries onto a model that offers it, and a size the new model does not offer is dropped.
  - `resolve_model` for `flux-3-image` still returns the Black Forest Labs provider.
- **Verification:** The posted JSON matches R5 and R6, a loaded canvas is not in the body, and a FLUX id still resolves to Black Forest Labs.

### U3. Classify the generations response

- **Goal:** The provider reports an allowlisted URL or a KTD5 error, and does not download the file itself.
- **Requirements:** R7, R9, R10, R12
- **Dependencies:** U2
- **Files:**
  - `crates/easel-core/src/providers/zai/mod.rs`
  - `crates/easel-core/src/providers/mod.rs`
- **Approach:** Decode per KTD5. Report a picture URL as neither inline bytes nor `Submitted`. The default report is nothing, so other providers stay on their current finish. Keep the first URL and drop later ones. Do not treat `content_filter` as a failure when a URL is present. Accept both error JSON shapes in KTD5. Mark finished only the KTD5 results a second click will not clear.
- **Execution note:** Write these classification tests and make them pass before U4 fetches a URL.
- **Patterns to follow:** The response match in `crates/easel-core/src/providers/xai/mod.rs`, with the URL kept instead of rejected.
- **Test scenarios:**
  - A 200 with `data` length 2 reports the first URL and not the second.
  - A 200 with a URL and `content_filter` role `assistant` level 1 still reports that URL.
  - A 200 with `content_filter` and `data` `[]` is `NoImage`.
  - A 200 whose URL is `https://evil.example/a.png` is `NoImage`.
  - A 200 whose URL is `https://cdn.z.ai/a.png?UCloudPublicKey=token` reports that URL, query included.
  - A top-level `{"code":1301,"message":"unsafe"}` on HTTP 400 is `Blocked`.
  - An `error` object whose `code` is the string `1301` on HTTP 400 is `Blocked`.
  - HTTP 401 is `Auth`.
  - HTTP 429 code 1302 is `RateLimited` with no retry delay. Code 1305 is the same.
  - HTTP 429 code 1113 with message `no balance` is `Validation` with that message, and it is marked finished. Codes 1308, 1309, 1310, 1311, 1314, 1315, and 1316 through 1321 are the same shape.
  - HTTP 400 code 1214 with a message is `Validation` with that message, and it is not marked finished.
  - HTTP 500 with no message is not `Validation` and is not `Blocked`.
  - A non-JSON 200 is `NoImage`.
- **Verification:** Each KTD5 class is distinguishable in a unit test, and no test performs a file GET.

### U4. Download the picture into history

- **Goal:** An allowlisted URL becomes one history file, and a bad download becomes a failed row.
- **Requirements:** R7, R9, R10, R12
- **Dependencies:** U3
- **Files:**
  - `crates/easel/src/jobs/runner.rs`
  - `crates/easel/src/jobs/mod.rs`
  - `crates/easel/src/jobs/zai_tests.rs`
  - `crates/easel/src/net/reqwest_transport.rs`
  - `crates/easel/src/image_load.rs`
- **Approach:**
  1. On the KTD3 URL, call `allows_url` on that string. If it rejects, do not call the transport.
  2. GET with the existing download request. `auth` stays unset. Do not enter the downloading state, and do not call the poll download path or its deadline.
  3. Sniff with `sniff_format`. On PNG, JPEG, or WEBP, store through the inline finish and pass that mime. Ignore `Content-Type`.
  4. When the provider marks the result finished, persist with the unpolled writer. Otherwise leave the live card only.
  5. On download failure, call that same writer with the KTD4 protocol error. Do not mark the job expired. Retry a 404 once inside this branch.
  6. Debug lines for this file GET keep the path and show `<redacted>` when a query is present. Other providers' logs stay as they are.
- **Patterns to follow:** `crates/easel/src/jobs/xai_tests.rs` for a one-shot runner proof, included from `crates/easel/src/jobs/mod.rs` the way `xai_tests` is. The inline finish for the file name. `credentialed_redirect_is_not_followed` and the debug-log test beside `debug_log_omits_a_megabyte_body` in `crates/easel/src/net/reqwest_transport.rs`. `sniff_format` in `crates/easel/src/image_load.rs`.
- **Test scenarios:**
  - A 200 URL `https://cdn.z.ai/a.png` followed by PNG bytes writes `result.png`, stores provider `zai`, sends the GET with no `Authorization` header and no key in the URL, and does not poll.
  - JPEG magic writes `result.jpg`. WEBP magic writes `result.webp`.
  - A 200 with two URLs downloads only the first.
  - A `content_filter` array beside a good URL still writes the file.
  - A URL on `evil.example` performs no GET and writes a failed row with `NoImage`.
  - A URL `https://cdn.z.ai:8443/a.png` performs no GET. `allows_url` ignores the port today, so this rejection is the KTD4 check in front of the GET.
  - A 302 from an allowlisted host writes a failed row, sends no second request, and is not expired.
  - A file-host 401, a file-host 403, a timeout, and a body over 100 MiB each write one failed row with protocol text `image download failed`, and the job is not expired.
  - A 404 then a PNG body writes the file. Two 404s write a failed row and send two GETs, not three.
  - HTML bytes with `Content-Type: image/png` write a failed row and no file.
  - A result marked finished, with server text `no balance`, writes a failed row with that text. Classification of each generations status stays in U3.
  - A result that is not marked finished writes no history row.
  - Dismiss during the POST drops a late 200. For the GET, park after the POST returns, dismiss, then deliver the file body. No row is written.
  - A debug log line for this file GET does not contain the query, and it does contain `<redacted>`. The case sits with the transport debug-log tests, because the scripted job transport does not print those lines.
- **Verification:** A PNG URL lands as `result.png` without a poll, a 302 does not, and a 429 rate limit leaves no history row.

---

## Verification Contract

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run -p easel-core` covers the request body and the response classes.
- `cargo nextest run -p easel` covers the runner, the form, the key probe, and the menu.

---

## Definition of Done

- U1 through U4 meet their verification lines.
- Black Forest Labs, Krea, Google, OpenAI, and xAI requests are unchanged.
- The four commands in the Verification Contract pass.
- Abandoned experimental code is not left in the diff.

---

## System-Wide Impact

`ProviderId` is matched exhaustively, so U1 touches every arm: settings, the credential store, the key map, history, the runner cache key, and the missing-key label. Credentialed requests stay on `https://api.z.ai`. The new finish calls the existing download request, leaves `auth` unset, and does not copy the POST headers. It calls `allows_url` on the URL it is about to send, and it does not call the transport when that returns false. The job runner gains a third finish after submit, used only when the provider reports a picture URL. That path stays out of the downloading state. Polling and inline bytes stay as they are. `Auth`, `Blocked`, and `NoImage` already write a failed history row. A result the provider marks finished also writes that row. Other results still write none. Debug lines for the Z.ai file GET keep the path and show `<redacted>` when a query is present. Other providers' request logs stay as they are. The status bar requests a balance only for Black Forest Labs.

---

## Risks & Dependencies

- Official docs do not name the file host. KTD4 allows any `https` host ending in `.z.ai`, plus one UCloud host seen in April 2026. A host outside that list fails closed. A name that does end in `.z.ai` receives the storage-token query. The API key stays off that GET because `auth` is unset. Reusing the POST's `auth` would hand the file host the Z.ai key.
- A URL with an explicit port other than 443 is rejected. A URL with no port is allowed. `allows_url` today ignores the port, so the port check is new and applies before this GET.
- The image error example is a top-level integer `code`. The errors guide also shows `error.code` as a string. KTD5 accepts both. A third shape falls through to the no-row HTTP failure.
- The settings probe treats 401 as invalid and 429 as not accepted. Every other status still toasts key accepted. A Coding Plan key that fails with a status other than those two can pass the probe and then fail the generations call.
- `hd` is documented at about 20 seconds. The POST uses the existing 60 second budget. A slower response is a timeout with no history row.
- The file URL lasts 30 days and this plan does not store it. Dismiss or quit during the GET loses the picture.
- The China platform's image notes say `glm-image` is `hd` only. The `api.z.ai` schema fetched 18 October 2026 still lists `standard`. This plan sends `standard` when the form has it.
- The storage token stays in the query the file GET sends. Debug lines for that GET show `<redacted>` instead of the token. Other providers' logs are unchanged.

# Get Job

Source: https://apiframe.ai/docs/jobs/get



`GET /v2/jobs/:id`

Retrieve a single job by its ID. Use this to poll for job completion or to check the result of a completed job.

Path parameters [#path-parameters]

| Parameter | Type          | Description                                   |
| --------- | ------------- | --------------------------------------------- |
| `id`      | string (UUID) | The job ID returned from a generation request |

Response [#response]

Queued job [#queued-job]

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "status": "QUEUED",
  "model": "midjourney",
  "progress": 0,
  "result": null,
  "error": null,
  "creditCost": 10,
  "webhookStatus": null,
  "createdAt": "2026-03-15T10:00:00.000Z",
  "completedAt": null
}
```

Processing job [#processing-job]

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "status": "PROCESSING",
  "model": "midjourney",
  "progress": 65,
  "result": null,
  "error": null,
  "creditCost": 10,
  "webhookStatus": null,
  "createdAt": "2026-03-15T10:00:00.000Z",
  "completedAt": null
}
```

Completed job [#completed-job]

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "status": "COMPLETED",
  "model": "midjourney",
  "progress": 100,
  "result": {
    "images": [
      "https://cdn2.apiframe.ai/images/a1b2c3d4-e5f6-7890-abcd-ef1234567890-1.png",
      "https://cdn2.apiframe.ai/images/a1b2c3d4-e5f6-7890-abcd-ef1234567890-2.png",
      "https://cdn2.apiframe.ai/images/a1b2c3d4-e5f6-7890-abcd-ef1234567890-3.png",
      "https://cdn2.apiframe.ai/images/a1b2c3d4-e5f6-7890-abcd-ef1234567890-4.png"
    ],
    "gridUrl": "https://cdn2.apiframe.ai/images/a1b2c3d4-e5f6-7890-abcd-ef1234567890-grid.png"
  },
  "error": null,
  "creditCost": 10,
  "webhookStatus": "sent",
  "createdAt": "2026-03-15T10:00:00.000Z",
  "completedAt": "2026-03-15T10:00:32.000Z"
}
```

The shape of the `result` object depends on the job's modality:

| Modality | Shape                                                                                                                           |
| -------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Image    | `{ "images": [...] }` — plus `"gridUrl"` for Midjourney. See [Image result format](/docs/images#result-format).                 |
| Video    | `{ "videoUrl": "..." }` — or `{ "videos": [...] }` for Midjourney Video. See [Video result format](/docs/videos#result-format). |
| Music    | `{ "tracks": [...] }` with per-track metadata. See [Music result format](/docs/music#result-format).                            |

Each model page shows an example result for that model.

Failed job [#failed-job]

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "status": "FAILED",
  "model": "midjourney",
  "progress": 0,
  "result": null,
  "error": "Provider returned an error: content policy violation",
  "creditCost": 0,
  "webhookStatus": "sent",
  "createdAt": "2026-03-15T10:00:00.000Z",
  "completedAt": "2026-03-15T10:00:05.000Z"
}
```

Status flow [#status-flow]

```
QUEUED → PROCESSING → COMPLETED
                    → FAILED
```

| Status       | Meaning                                                             |
| ------------ | ------------------------------------------------------------------- |
| `QUEUED`     | Waiting to be picked up by a worker                                 |
| `PROCESSING` | Actively generating — `progress` updates in real-time (0–100)       |
| `COMPLETED`  | Done — `result` contains the CDN URLs of the generated assets       |
| `FAILED`     | Error occurred — `error` contains the reason. Credits are refunded. |

Polling strategy [#polling-strategy]

When polling for job completion, we recommend:

1. Poll every **2–3 seconds** for image jobs
2. Poll every **5–10 seconds** for video and music jobs (they take longer)
3. Stop polling when `status` is `COMPLETED` or `FAILED`

For real-time updates without polling, use [webhooks](/docs/webhooks).

Code examples [#code-examples]

<Tabs items="[&#x22;cURL&#x22;, &#x22;Python&#x22;, &#x22;Node.js&#x22;, &#x22;Go&#x22;]">
  <Tab value="cURL">
    ```bash
    curl -H "X-API-Key: afk_your_api_key_here" \
      https://api.apiframe.ai/v2/jobs/a1b2c3d4-e5f6-7890-abcd-ef1234567890
    ```
  </Tab>

  <Tab value="Python">
    ```python
    import time
    import requests

    headers = {"X-API-Key": "afk_your_api_key_here"}
    job_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890"

    while True:
        response = requests.get(
            f"https://api.apiframe.ai/v2/jobs/{job_id}",
            headers=headers,
        )
        job = response.json()

        if job["status"] == "COMPLETED":
            print(f"Result: {job['result']}")
            break
        elif job["status"] == "FAILED":
            print(f"Error: {job['error']}")
            break
        else:
            print(f"Status: {job['status']} ({job['progress']}%)")
            time.sleep(3)
    ```
  </Tab>

  <Tab value="Node.js">
    ```javascript
    const headers = { "X-API-Key": "afk_your_api_key_here" };
    const jobId = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";

    while (true) {
      const res = await fetch(
        `https://api.apiframe.ai/v2/jobs/${jobId}`,
        { headers }
      );
      const job = await res.json();

      if (job.status === "COMPLETED") {
        console.log("Result:", job.result);
        break;
      } else if (job.status === "FAILED") {
        console.error("Error:", job.error);
        break;
      }

      console.log(`${job.status} (${job.progress}%)`);
      await new Promise((r) => setTimeout(r, 3000));
    }
    ```
  </Tab>

  <Tab value="Go">
    ```go
    jobID := "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
    url := "https://api.apiframe.ai/v2/jobs/" + jobID

    for {
        req, _ := http.NewRequest("GET", url, nil)
        req.Header.Set("X-API-Key", "afk_your_api_key_here")
        resp, _ := http.DefaultClient.Do(req)

        var job map[string]interface{}
        json.NewDecoder(resp.Body).Decode(&job)
        resp.Body.Close()

        status := job["status"].(string)
        if status == "COMPLETED" || status == "FAILED" {
            break
        }
        time.Sleep(3 * time.Second)
    }
    ```
  </Tab>
</Tabs>

Error responses [#error-responses]

| Status | Error          | Meaning                                     |
| ------ | -------------- | ------------------------------------------- |
| `400`  | Invalid job ID | The ID is not a valid UUID                  |
| `401`  | Unauthorized   | Missing or invalid API key                  |
| `404`  | Job not found  | No job with this ID exists for your account |

Try it [#try-it]

<Playground method="GET" path="/v2/jobs/:id" />

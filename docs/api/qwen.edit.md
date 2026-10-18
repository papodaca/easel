> ## Documentation Index
> Fetch the complete documentation index at: https://docs.qwencloud.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Image editing

> Modify images via text

## Getting started

This example shows how to use `qwen-image-3.0-pro` to generate two edited images from three input images and a prompt.

<Note>
  Input prompt: The girl in Image 1 wears the black dress from Image 2 and sits in the pose from Image 3.
</Note>

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "20%", paddingBottom: "8px"}}>**Input image 1**</th>
      <th style={{textAlign: "left", width: "22%", paddingBottom: "8px"}}>**Input image 2**</th>
      <th style={{textAlign: "left", width: "20%", paddingBottom: "8px"}}>**Input image 3**</th>
      <th style={{textAlign: "left", width: "20%", paddingBottom: "8px"}}>**Output images (multiple images)**</th>

      <th style={{textAlign: "left", width: "18%", paddingBottom: "8px"}} />
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image99" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011682.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image98" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011684.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image89" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011683.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image100" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011681.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="imageout2" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6903291671/p1022524.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>
    </tr>
  </tbody>
</table>

Before making a call, [get an API key](/api-reference/preparation/api-key) and export the API key as an environment variable.

To call the API using the SDK, [install the DashScope SDK](/api-reference/preparation/install-sdk). The SDK is available for Python and Java.

The Qwen image editing models support one to three input images; `qwen-image-2.1-pro` supports one to ten. The `qwen-image-2.0`/`qwen-image-3.0` series and `qwen-image-2.1-pro` can generate one to six images. The URLs for the generated images are **valid for 24 hours**. Download the images to your local device promptly.

<Tabs>
  <Tab title="Python">
    ```python
    import json
    import os
    import dashscope
    from dashscope import MultiModalConversation

    dashscope.base_http_api_url = 'https://maas.qwencloudapi.com/api/v1'

    # The model supports one to three input images.
    messages = [
      {
        "role": "user",
        "content": [
          {"image": "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/thtclx/input1.png"},
          {"image": "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/iclsnx/input2.png"},
          {"image": "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/gborgw/input3.png"},
          {"text": "Make the girl from Image 1 wear the black dress from Image 2 and sit in the pose from Image 3."}
        ]
      }
    ]

    # Replace with your API key if the environment variable is not set: api_key="sk-xxx"
    api_key = os.getenv("DASHSCOPE_API_KEY")

    # qwen-image-2.0/3.0 series supports outputting 1 to 6 images. This example shows how to output 2 images.
    response = MultiModalConversation.call(
      api_key=api_key,
      model="qwen-image-3.0-pro",
      messages=messages,
      stream=False,
      n=2,
      watermark=False,
      negative_prompt=" ",
      prompt_extend=True,
      size="1024*1536",
    )

    if response.status_code == 200:
      # To view the full response, uncomment the following line.
      # print(json.dumps(response, ensure_ascii=False))
      for i, content in enumerate(response.output.choices[0].message.content):
        print(f"URL of output image {i+1}: {content['image']}")
    else:
      print(f"HTTP status code: {response.status_code}")
      print(f"Error code: {response.code}")
      print(f"Error message: {response.message}")
      print("For more information, see the documentation: https://docs.qwencloud.com/api-reference/preparation/error-messages")
    ```

    <Accordion title="Response example">
      ```json
      {
        "status_code": 200,
        "request_id": "fa41f9f9-3cb6-434d-a95d-4ae6b9xxxxxx",
        "code": "",
        "message": "",
        "output": {
          "text": null,
          "finish_reason": null,
          "choices": [
            {
              "finish_reason": "stop",
              "message": {
                "role": "assistant",
                "content": [
                  {
                    "image": "https://dashscope-result-hz.oss-cn-hangzhou.aliyuncs.com/xxx.png?Expires=xxx"
                  },
                  {
                    "image": "https://dashscope-result-hz.oss-cn-hangzhou.aliyuncs.com/xxx.png?Expires=xxx"
                  }
                ]
              }
            }
          ],
          "audio": null
        },
        "usage": {
          "output_height": 1536,
          "output_width": 1024,
          "input_image_count": 3,
          "input_image_type": "qima_input_1k",
          "output_image_count": 2,
          "output_image_type": "qima_output_1k"
        }
      }
      ```
    </Accordion>
  </Tab>

  <Tab title="Java">
    ```java
    import com.alibaba.dashscope.aigc.multimodalconversation.MultiModalConversation;
    import com.alibaba.dashscope.aigc.multimodalconversation.MultiModalConversationParam;
    import com.alibaba.dashscope.aigc.multimodalconversation.MultiModalConversationResult;
    import com.alibaba.dashscope.common.MultiModalMessage;
    import com.alibaba.dashscope.common.Role;
    import com.alibaba.dashscope.exception.ApiException;
    import com.alibaba.dashscope.exception.NoApiKeyException;
    import com.alibaba.dashscope.exception.UploadFileException;
    import com.alibaba.dashscope.utils.JsonUtils;
    import com.alibaba.dashscope.utils.Constants;

    import java.io.IOException;
    import java.util.Arrays;
    import java.util.Collections;
    import java.util.HashMap;
    import java.util.Map;
    import java.util.List;

    public class QwenImageEdit {

      static {
        Constants.baseHttpApiUrl = "https://maas.qwencloudapi.com/api/v1";
      }

      // If you have not configured the environment variable, replace the following line with your API key: apiKey="sk-xxx"
      static String apiKey = System.getenv("DASHSCOPE_API_KEY");

      public static void call() throws ApiException, NoApiKeyException, UploadFileException, IOException {

        MultiModalConversation conv = new MultiModalConversation();

        // The model supports one to three input images.
        MultiModalMessage userMessage = MultiModalMessage.builder().role(Role.USER.getValue())
            .content(Arrays.asList(
                Collections.singletonMap("image", "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/thtclx/input1.png"),
                Collections.singletonMap("image", "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/iclsnx/input2.png"),
                Collections.singletonMap("image", "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/gborgw/input3.png"),
                Collections.singletonMap("text", "Make the girl from Image 1 wear the black dress from Image 2 and sit in the pose from Image 3.")
            )).build();
        // qwen-image-2.0/3.0 series supports outputting 1 to 6 images. This example shows how to output 2 images.
        Map<String, Object> parameters = new HashMap<>();
        parameters.put("watermark", false);
        parameters.put("negative_prompt", " ");
        parameters.put("n", 2);
        parameters.put("prompt_extend", true);
        parameters.put("size", "1024*1536");

        MultiModalConversationParam param = MultiModalConversationParam.builder()
            .apiKey(apiKey)
            .model("qwen-image-3.0-pro")
            .messages(Collections.singletonList(userMessage))
            .parameters(parameters)
            .build();

        MultiModalConversationResult result = conv.call(param);
        // To view the complete response, uncomment the following line.
        // System.out.println(JsonUtils.toJson(result));
        List<Map<String, Object>> contentList = result.getOutput().getChoices().get(0).getMessage().getContent();
        int imageIndex = 1;
        for (Map<String, Object> content : contentList) {
          if (content.containsKey("image")) {
            System.out.println("URL of output image " + imageIndex + ": " + content.get("image"));
            imageIndex++;
          }
        }
      }

      public static void main(String[] args) {
        try {
          call();
        } catch (ApiException | NoApiKeyException | UploadFileException | IOException e) {
          System.out.println(e.getMessage());
        }
      }
    }
    ```

    <Accordion title="Sample response">
      ```json
      {
        "requestId": "46281da9-9e02-941c-ac78-be88b8xxxxxx",
        "usage": {
          "output_height": 1536,
          "output_width": 1024,
          "input_image_count": 3,
          "input_image_type": "qima_input_1k",
          "output_image_count": 2,
          "output_image_type": "qima_output_1k"
        },
        "output": {
          "choices": [
            {
              "finish_reason": "stop",
              "message": {
                "role": "assistant",
                "content": [
                  {
                    "image": "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/xxx.png?Expires=xxx"
                  },
                  {
                    "image": "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/xxx.png?Expires=xxx"
                  }
                ]
              }
            }
          ]
        }
      }
      ```
    </Accordion>
  </Tab>

  <Tab title="curl">
    ```curl
    curl --location 'https://maas.qwencloudapi.com/api/v1/services/aigc/multimodal-generation/generation' \
    --header 'Content-Type: application/json' \
    --header "Authorization: Bearer $DASHSCOPE_API_KEY" \
    --data '{
      "model": "qwen-image-3.0-pro",
      "input": {
        "messages": [
          {
            "role": "user",
            "content": [
              {
                "image": "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/thtclx/input1.png"
              },
              {
                "image": "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/iclsnx/input2.png"
              },
              {
                "image": "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20250925/gborgw/input3.png"
              },
              {
                "text": "Make the girl from Image 1 wear the black dress from Image 2 and sit in the pose from Image 3."
              }
            ]
          }
        ]
      },
      "parameters": {
        "n": 2,
        "negative_prompt": " ",
        "prompt_extend": true,
        "watermark": false,
        "size": "1024*1536"
      }
    }'
    ```

    <Accordion title="Response example">
      ```json
      {
        "output": {
          "choices": [
            {
              "finish_reason": "stop",
              "message": {
                "role": "assistant",
                "content": [
                  {
                    "image": "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/xxx.png?Expires=xxx"
                  },
                  {
                    "image": "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/xxx.png?Expires=xxx"
                  }
                ]
              }
            }
          ]
        },
        "usage": {
          "output_height": 1536,
          "output_width": 1024,
          "input_image_count": 3,
          "input_image_type": "qima_input_1k",
          "output_image_count": 2,
          "output_image_type": "qima_output_1k"
        },
        "request_id": "bf37ca26-0abe-98e4-8065-xxxxxx"
      }
      ```
    </Accordion>
  </Tab>
</Tabs>

<Accordion title="Download images to your local device">
  <Tabs>
    <Tab title="Python">
      ```python
      import requests

      def download_image(image_url, save_path='output.png'):
        try:
          response = requests.get(image_url, stream=True, timeout=300)  # Set timeout
          response.raise_for_status()  # Raise an exception if the HTTP status code is not 200
          with open(save_path, 'wb') as f:
            for chunk in response.iter_content(chunk_size=8192):
              f.write(chunk)
          print(f"Image downloaded successfully to: {save_path}")

        except requests.exceptions.RequestException as e:
          print(f"Image download failed: {e}")

      image_url = "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/xxx.png?Expires=xxx"
      download_image(image_url, save_path='output.png')
      ```
    </Tab>

    <Tab title="Java">
      ```java
      import java.io.FileOutputStream;
      import java.io.InputStream;
      import java.net.HttpURLConnection;
      import java.net.URL;

      public class ImageDownloader {
        public static void downloadImage(String imageUrl, String savePath) {
          try {
            URL url = new URL(imageUrl);
            HttpURLConnection connection = (HttpURLConnection) url.openConnection();
            connection.setConnectTimeout(5000);
            connection.setReadTimeout(300000);
            connection.setRequestMethod("GET");
            InputStream inputStream = connection.getInputStream();
            FileOutputStream outputStream = new FileOutputStream(savePath);
            byte[] buffer = new byte[8192];
            int bytesRead;
            while ((bytesRead = inputStream.read(buffer)) != -1) {
              outputStream.write(buffer, 0, bytesRead);
            }
            inputStream.close();
            outputStream.close();

            System.out.println("Image downloaded successfully to: " + savePath);
          } catch (Exception e) {
            System.err.println("Image download failed: " + e.getMessage());
          }
        }

        public static void main(String[] args) {
          String imageUrl = "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/xxx.png?Expires=xxx";
          String savePath = "output.png";
          downloadImage(imageUrl, savePath);
        }
      }
      ```
    </Tab>
  </Tabs>
</Accordion>

## Input instructions

### Input images (messages)

The `messages` parameter is an array that must contain a single object. This object must include the `role` and `content` properties. The `role` property must be set to `user`. The `content` property must include both `image` (one to three images, or one to ten for the `qwen-image-2.1 series`) and `text` (one editing instruction).

The input images must meet the following requirements:

- The supported image formats are JPG, JPEG, PNG, BMP, TIFF, WEBP, and GIF.

  <Note>
    The output image is in PNG format. For animated GIFs, only the first frame is processed.
  </Note>

- For best results, the image resolution should be between 384 and 3072 pixels for both width and height. A low resolution may result in a blurry output, while a high resolution increases processing time.

- The aspect ratio (longer side to shorter side) must not exceed 10:1.

- The size of a single image file cannot exceed 10 MB.

```json
"messages": [
  {
    "role": "user",
    "content": [
      { "image": "Public URL or Base64 data of Image 1" },
      { "image": "Public URL or Base64 data of Image 2" },
      { "image": "Public URL or Base64 data of Image 3" },
      { "text": "Your editing instruction, for example: 'The girl in Image 1 wears the black dress from Image 2 and sits in the pose from Image 3'" }
    ]
  }
]
```

### Image input order

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "27%", paddingBottom: "8px"}}>**Input image 1**</th>
      <th style={{textAlign: "left", width: "20%", paddingBottom: "8px"}}>**Input image 2**</th>
      <th style={{textAlign: "left", width: "27%", paddingBottom: "8px"}}>**Output image**</th>

      <th style={{textAlign: "left", width: "27%", paddingBottom: "8px"}} />
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image (19)-转换自-png" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/3509593671/p1025838.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />

        Image 1
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image (20)-转换自-png" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/3509593671/p1025839.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />

        Image 2
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="04e0fc39-7ad6-41e0-9df9-1f69ac3ce825-转换自-png" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/3509593671/p1021092.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />

        Prompt: Move Image 1 onto Image 2
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="36ed450d-bd54-4169-b13f-3d0f26d9d360-转换自-png" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/3509593671/p1021093.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />

        Prompt: Move Image 2 onto Image 1
      </td>
    </tr>
  </tbody>
</table>

### Image input methods

**Public URL**

- You can provide a publicly accessible image URL that supports the HTTP or HTTPS protocol.
- Example value: `https://xxxx/img.png`.

**Base64 encoding**

Convert the image file to a Base64-encoded string and concatenate it in the following format: `data:<mime_type>;base64,<base64_data>`.

- `<mime_type>`: The media type of the image, which must correspond to the file format.
- `<base64_data>`: The Base64-encoded string of the file.
- Example value: `data:image/jpeg;base64,GDU7MtCZz...` (The example is truncated for demonstration purposes.)

### More parameters

Adjust the generation results using the following **optional** parameters:

- **n**: The number of images to generate. The default value is 1. The qwen-image-2.0/3.0 series and qwen-image-2.1-pro models support generating one to six images.

- **negative\_prompt**: Describes content to exclude from the image, such as "blur" or "extra fingers". This parameter helps optimize the quality of the generated image.

- **watermark**: Specifies whether to add a "Qwen-Image" watermark to the bottom-right corner of the image. The default value is `false`.

- **seed**: The random number seed. The value must be an integer from `[0, 2147483647]`. If this parameter is not specified, the algorithm generates a random number to use as the seed. Using the same seed value helps ensure consistent generation results.

The following **optional** parameters are available only for the qwen-image-2.0/3.0 series and qwen-image-2.1-pro:

- **size**: The resolution of the output image. The format is `width*height`, such as `"1024*2048"`. For the qwen-image-2.0/3.0 series models and qwen-image-2.1-pro, you can set the width and height freely. The total pixels of the output image must be between 512 x 512 and 2048 x 2048. For the qwen-image-2.0 series, the resolution defaults to the same as the input image (the last image if multiple are provided); for the qwen-image-3.0 series and qwen-image-2.1-pro, the model auto-recommends a resolution from the prompt when size is omitted.
- **prompt\_extend:** Enables or disables the prompt rewriting feature. The default value is `true`. If enabled, the model optimizes the prompt. This feature can significantly improve the results for simple or less descriptive prompts.
- **prompt\_extend\_mode**: The prompt rewriting mode. Supported by the qwen-image-3.0 series and qwen-image-2.1-pro only. Image editing supports only the default value `direct` (direct prompt enhancement, DPE); passing `agent` returns a 400 error.
- **enable\_thinking**: Enables thinking mode. Supported by the qwen-image-3.0 series and qwen-image-2.1-pro only, for direct image editing (not supported for agent-mode image editing). Requires `prompt_extend=true`. Enhances model reasoning to improve image quality but increases generation time. The default value is `true`.

For a complete list of parameters, see [Qwen-Image-Edit API reference](/api-reference/image-generation/qwen-image-editing).

## Overview

### Multi-image fusion

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image 1**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image 2**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image 3**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Output image**</th>
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image83" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011712.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image103" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011753.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="1" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/4405461671/p1012002.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="2" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/4405461671/p1012004.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> The girl in Image 1 wears the necklace from Image 2 and carries the bag from Image 3 on her left shoulder.
      </td>
    </tr>
  </tbody>
</table>

### Subject consistency

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>**Output image 1**</th>
      <th style={{textAlign: "left", width: "34%", paddingBottom: "8px"}}>**Output image 2**</th>
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image5" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011789.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image4" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011790.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change the image to an ID photo with a blue background. The person is wearing a white shirt, a black suit, and a striped tie.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image6" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011791.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> The person is wearing a white shirt, a gray suit, and a striped tie. One hand rests on the tie. The background is light-colored.
      </td>
    </tr>

    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image12" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1012037.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image13" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1012038.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> The air conditioner is placed in a living room next to a sofa.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image14" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1012039.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Mist is added from the air conditioner's vent, extending over the sofa. Green leaves are also added.
      </td>
    </tr>
  </tbody>
</table>

### Sketch creation

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>**Output image**</th>

      <th style={{textAlign: "left", width: "34%", paddingBottom: "8px"}} />
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image42" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011821.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image43" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011822.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Generate an image that matches the detailed shape outlined in Image 1 and follows this description: A young woman smiles on a sunny day. She wears round brown sunglasses with a leopard print frame. Her hair is neatly tied up, she wears pearl earrings, a dark blue scarf with purple star patterns, and a black leather jacket.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image44" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011824.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Generate an image that matches the detailed shape outlined in Image 1 and follows this description: An elderly man smiles at the camera. His face is wrinkled, his hair is messy in the wind, and he wears round-framed reading glasses. He has a worn-out red scarf with star patterns around his neck and is wearing a cotton-padded jacket.
      </td>
    </tr>
  </tbody>
</table>

### Creative product generation

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Output image**</th>

      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}} />

      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}} />
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="bear" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999719.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image23" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011685.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Make this bear sit under the moon (represented by a light gray crescent outline on a white background), holding a guitar, with small stars and speech bubbles with phrases such as "Be Kind" floating around.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image22" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011686.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Print this design on a T-shirt and a paper tote bag. A female model is displaying these items. The woman is also wearing a baseball cap with "Be kind" written on it.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image21" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011687.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> A hyper-realistic 1/7 scale character model, designed as a commercial finished product, is placed on a desk with an iMac that has a white keyboard. The model stands on a clean, round, transparent acrylic base with no labels or text. Professional studio lighting highlights the sculpted details. On the iMac screen in the background, the ZBrush modeling process for the same model is displayed. Next to the model, place a packaging box with a transparent window on the front, showing only the clear plastic shell inside. The box is slightly taller than the model and reasonably sized to hold it.
      </td>
    </tr>

    <tr>
      <td style={{verticalAlign: "top"}} />

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p999632.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> This bear is wearing an astronaut suit and pointing into the distance.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999633.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> This bear is wearing a gorgeous ball gown, with its arms spread in an elegant dance pose.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999630.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> This bear is wearing sportswear, holding a basketball, with one leg bent.
      </td>
    </tr>
  </tbody>
</table>

### Generate image from depth map

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>**Output image**</th>

      <th style={{textAlign: "left", width: "34%", paddingBottom: "8px"}} />
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image36" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011810.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image37" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011811.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Generate an image that matches the depth map outlined in Image 1 and follows this description: A blue bicycle is parked in a side alley, with a few weeds growing from cracks in the stone in the background.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image38" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011812.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Generate an image that matches the depth map outlined in Image 1 and follows this description: A worn-out red bicycle is parked on a muddy path, with a dense primeval forest in the background.
      </td>
    </tr>
  </tbody>
</table>

### Generate image from keypoints

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>**Output image**</th>

      <th style={{textAlign: "left", width: "34%", paddingBottom: "8px"}} />
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image40" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011817.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image41" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011818.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Generate an image that matches the human pose outlined in Image 1 and follows this description: A Chinese woman in a Hanfu is holding an oil-paper umbrella in the rain, with a Suzhou garden in the background.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image39" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011819.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Generate an image that matches the human pose outlined in Image 1 and follows this description: A young man stands on a subway platform. He wears a baseball cap, a T-shirt, and jeans. A train is speeding by behind him.
      </td>
    </tr>
  </tbody>
</table>

### Text editing

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "35%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "35%", paddingBottom: "8px"}}>**Output image**</th>
      <th style={{textAlign: "left", width: "15%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "15%", paddingBottom: "8px"}}>**Output image**</th>
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999641.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://img.alicdn.com/imgextra/i4/O1CN01tVRmgU1KTA9Kvy6vN_!!6000000001164-49-tps-1248-832.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Replace 'HEALTH INSURANCE' on the Scrabble tiles with '<strong>Tomorrow will be better</strong>'.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1000039.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1000062.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change the phrase "Take a Breather" on the note to "<strong>Relax and Recharge</strong>".
      </td>
    </tr>
  </tbody>
</table>

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Output image**</th>

      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}} />

      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}} />
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image53" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011772.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image45" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011774.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change "Qwen-Image" to a black ink-drip font.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image46" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011775.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change "Qwen-Image" to a black handwriting font.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image49" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011776.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change "Qwen-Image" to a black pixel font.
      </td>
    </tr>

    <tr>
      <td style={{verticalAlign: "top"}} />

      <td style={{verticalAlign: "top"}}>
        <img alt="image54" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011777.jpeg" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change "Qwen-Image" to red.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image57" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011779.jpeg" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change "Qwen-Image" to a blue-purple gradient.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image59" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011780.jpeg" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change "Qwen-Image" to candy colors.
      </td>
    </tr>

    <tr>
      <td style={{verticalAlign: "top"}} />

      <td style={{verticalAlign: "top"}}>
        <img alt="image63" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011783.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change the material of "Qwen-Image" to metal.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image64" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011784.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change the material of "Qwen-Image" to clouds.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image67" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011786.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Change the material of "Qwen-Image" to glass.
      </td>
    </tr>
  </tbody>
</table>

### Add, delete, replace, and modify

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Output image**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Output image**</th>
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999647.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p999648.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Add a small wooden sign in front of the penguin that says "Welcome to Penguin Beach".
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p999649.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999650.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Remove the hair from the plate.
      </td>
    </tr>
  </tbody>
</table>

### Viewpoint transformation

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Output image**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "25%", paddingBottom: "8px"}}>**Output image**</th>
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999964.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p999968.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Get a front view.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p999969.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999970.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Face left.
      </td>
    </tr>

    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p999974.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p999975.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Get a rear view.
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999971.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p999972.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Face right.
      </td>
    </tr>
  </tbody>
</table>

### Old photo processing

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <thead>
    <tr>
      <th style={{textAlign: "left", width: "50%", paddingBottom: "8px"}}>**Input image**</th>
      <th style={{textAlign: "left", width: "50%", paddingBottom: "8px"}}>**Output image**</th>
    </tr>
  </thead>

  <tbody>
    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999552.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p999554.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Restore the old photo, remove scratches, reduce noise, enhance details, high resolution, realistic image, natural skin tone, clear facial features, no distortion.
      </td>
    </tr>

    <tr>
      <td style={{verticalAlign: "top"}}>
        <img alt="image31" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/5844029571/p1011757.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
      </td>

      <td style={{verticalAlign: "top"}}>
        <img alt="image32" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/6844029571/p1011759.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} /> Intelligently colorize the image based on its content to make it more vivid.
      </td>
    </tr>
  </tbody>
</table>

## Billing and rate limits

For free quota and pricing, see [pricing](/developer-guides/getting-started/pricing).

For rate limits, see [Rate limits](/developer-guides/administration/rate-limits).

**Billing details:**

- Billing is based on the **number of successfully generated images**. Failed model calls or processing errors do not incur fees or consume the free quota.
- You can enable the 'Free quota only' feature to avoid extra charges after your free quota is used up. For more information, see [Free quota for new users](/resources/free-quota).

## API reference

For the input and output parameters of the API, see [Qwen - image editing](/api-reference/image-generation/qwen-image-editing).

## Error codes

If a call fails, see [Error messages](/api-reference/preparation/error-messages).

## FAQ

**Q: What languages do the Qwen image editing models support?**

They officially support Simplified Chinese and **English**. Other languages may work, but results are not guaranteed.

**Q: How do I view model invocation metrics?**

One hour after a model invocation completes, go to the [**Analytics**](https://home.qwencloud.com/analytics) page to view metrics such as invocation count and success rate. For more information, see [Billing and cost management](/resources/bill-query).

**Q: How do I get the domain name whitelist for image storage?**

Images generated by models are stored in OSS. The API returns a temporary public URL. **To configure a firewall whitelist for this download URL**, note the following: The underlying storage may change dynamically. This topic does not provide a fixed OSS domain name whitelist to prevent access issues caused by outdated information. If you have security control requirements, contact your account manager to obtain the latest OSS domain name list.

For more information, see [Images & videos FAQ](/resources/faq-images-videos).

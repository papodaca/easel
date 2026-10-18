> ## Documentation Index
> Fetch the complete documentation index at: https://docs.qwencloud.com/llms.txt
> Use this file to discover all available pages before exploring further.

# Text-to-image

> Generate images from text prompts.

To compare models and choose the right one, see [Image models](/developer-guides/getting-started/image-models). **Try it online**: [QwenCloud](https://home.qwencloud.com/try-ai).

## Model performance

### Qwen-Image

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <tr>
    <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>Complex text</th>
    <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>Long paragraphs</th>
    <th style={{textAlign: "left", width: "34%", paddingBottom: "8px"}}>Complex layouts</th>
  </tr>

  <tr>
    <td style={{verticalAlign: "top"}}>
      <img alt="Complex text" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/7661432671/p1023656.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>

    <td style={{verticalAlign: "top"}}>
      <img alt="Long paragraphs" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/7661432671/p1023657.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>

    <td style={{verticalAlign: "top"}}>
      <img alt="Complex layouts" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/1494147571/p1005434.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>
  </tr>

  <tr>
    <td style={{verticalAlign: "top"}}>
      Poster design
    </td>

    <td style={{verticalAlign: "top"}}>
      Illustration design
    </td>

    <td style={{verticalAlign: "top"}}>
      Realistic photography
    </td>
  </tr>

  <tr>
    <td style={{verticalAlign: "top"}}>
      <img alt="Poster design" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/1494147571/p1005440.png" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>

    <td style={{verticalAlign: "top"}}>
      <img alt="Illustration design" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/1494147571/p1005413.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>

    <td style={{verticalAlign: "top"}}>
      <img alt="Realistic photography" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/1494147571/p1005424.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>
  </tr>
</table>

<Accordion title="Click to view prompts">
  **Complex text**: Bookstore window display. A sign displays "New Arrivals This Week". Below, a shelf tag with the text "Best-Selling Novels Here". To the side, a colorful poster advertises "Author Meet And Greet on Saturday" with a central portrait of the author. There are four books on the bookshelf, namely "The light between worlds" "When stars are scattered" "The silent patient" "The night circus"

  **Long paragraphs**: A young girl dressed in a school uniform stands in a classroom, writing on the blackboard. Centered on the board, neatly inscribed in white chalk, is the text: "Introducing Qwen-Image, a foundational image generation model that excels in complex text rendering and precise image editing." Soft natural light streams through the windows, casting gentle shadows. The scene is rendered in a realistic photographic style, with finely detailed textures, shallow depth of field, and warm tonal hues. The girl's focused expression and the chalk dust suspended in the air add a sense of movement and vitality. Background elements-including student desks and educational posters-are slightly blurred to emphasize the central action. Ultra-high 32K resolution, DSLR-quality imagery, soft bokeh effect, and documentary-style composition.

  **Complex layouts**: Create a classroom PPT slide for a speech. It features artistic, decorative shapes framing neatly arranged textual info as an elegant infographic. Center title: 'Habits for Emotional Wellbeing', surrounded by a symmetrical floral pattern. Left upper: 'Practice Mindfulness' + minimalist lotus icon + text 'Be present, observe without judging, accept without resisting'. Downward: 'Cultivate Gratitude' + open hand illustration + text 'Appreciate simple joys and acknowledge positivity daily'. Bottom - left: 'Stay Connected' + minimalistic chat bubble icon + text 'Build and maintain meaningful relationships to sustain emotional energy'. Bottom right: 'Prioritize Sleep' + crescent moon illustration + text 'Quality sleep benefits both body and mind'. Upward right: 'Regular Physical Activity' + jogging runner icon + text 'Exercise boosts mood and relieves anxiety'. Top right: 'Continuous Learning' + book icon + text 'Engage in new skill and knowledge for growth'. The layout balances clarity & artistry, guiding viewers naturally. --ar 16:9 --style clean - presentation.

  **Poster design**: Healing-style hand-drawn poster featuring three puppies playing with a ball on lush green grass, adorned with decorative elements such as birds and stars. The main title "Come Play Ball!" is prominently displayed at the top in bold, blue cartoon font. Below it, the subtitle "Come \[Show Off Your Skills]!" appears in green font. A speech bubble adds playful charm with the text: "Hehe, watch me amaze my little friends next!" At the bottom, supplementary text reads: "We get to play ball with our friends again!" The color palette centers on fresh greens and blues, accented with bright pink and yellow tones to highlight a cheerful, childlike atmosphere.

  **Illustration design**: A vibrant and lively illustration of a sunny, bustling commercial street scene, slice of life. In the foreground, a young boy in a white shirt and shorts is intently choosing items from a market stall. The stall is filled with snacks, drinks, and daily goods. The stall owner, a middle-aged man in an apron, is organizing the products. A wooden sign with "Qwen-Image" in a handwritten style hangs above the stall. The background features modern, colorful buildings with prominent signs for "QwenCloud" "Text-to-Image". The sky is azure blue with fluffy white clouds and soaring seagulls. Art Style: Realism illustration, delicate and soft, vibrant colors, rich layers, subtle hand-drawn texture, detailed, strong light and shadow, full composition, strong sense of depth, cheerful and relaxing atmosphere.

  **Realistic photography**: A realistic, high-fashion street-style photograph of a young Asian woman. She stands confidently on a vibrant, neon-lit city street at night. She is wearing a sleek black bomber jacket with a subtle white geometric logo and the word "Qwen" embroidered on the back, paired with dark cargo pants. The background is filled with the glowing signs and soft bokeh of city lights, creating a cinematic and atmospheric mood. The lighting is dramatic, with highlights from the neon signs casting colors onto her face and jacket. In the bottom-right corner, overlayed text reads "Neon Dreams" and "Urban Pulse". The text is in a modern, stylish, sans-serif font with a slight neon glow effect, seamlessly integrated into the composition. The entire image should be a masterpiece, ultra-detailed, 8K, UHD, with sharp focus and professional photographic quality, capturing a candid yet powerful urban moment.
</Accordion>

### Wan series

<table style={{width: "100%", borderCollapse: "separate", borderSpacing: "8px 0", tableLayout: "fixed"}}>
  <tr>
    <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>Portrait photography</th>
    <th style={{textAlign: "left", width: "33%", paddingBottom: "8px"}}>Realistic photography</th>
    <th style={{textAlign: "left", width: "34%", paddingBottom: "8px"}}>Painting styles</th>
  </tr>

  <tr>
    <td style={{verticalAlign: "top"}}>
      <img alt="Portrait photography" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/7661432671/p1023583.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>

    <td style={{verticalAlign: "top"}}>
      <img alt="Realistic photography" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/7661432671/p1023584.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>

    <td style={{verticalAlign: "top"}}>
      <img alt="Painting styles" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/7661432671/p1023585.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>
  </tr>

  <tr>
    <td style={{verticalAlign: "top"}}>
      Text generation
    </td>

    <td style={{verticalAlign: "top"}}>
      Poster design
    </td>

    <td style={{verticalAlign: "top"}}>
      Image set generation
    </td>
  </tr>

  <tr>
    <td style={{verticalAlign: "top"}}>
      <img alt="Text generation" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/7661432671/p1023588.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>

    <td style={{verticalAlign: "top"}}>
      <img alt="Poster design" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/7661432671/p1023589.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>

    <td style={{verticalAlign: "top"}}>
      <img alt="Image set generation" src="https://help-static-aliyun-doc.aliyuncs.com/assets/img/en-US/7661432671/p1023591.webp" style={{display: "block", width: "100%", margin: "0 0 8px 0"}} />
    </td>
  </tr>
</table>

<Accordion title="Click to view prompts">
  **Portrait photography**: hyper-realistic Scandinavian woman portrait, flowing platinum blonde hair and piercing blue eyes with prominent freckles, sharp intellectual gaze, Nordic cold-toned directional lighting creating icy atmosphere, minimalist modern styling with clean lines, shallow depth-of-field with a blurred, cold-gradient background, authentic Nordic facial features and porcelain skin texture.

  **Realistic photography**: a fish-eye perspective forest scene with dramatic perspective distortion, ultra-detailed red fox staring into lens with piercing amber eyes, hyper-realistic fur texture showing individual guard hairs and undercoat layers, radially warped trees forming circular background patterns, watercolor painting style with translucent washes and organic pigment bleeding, soft pastel palette of moss green and earth ochre tones, painterly lighting with atmospheric glow through canopy gaps

  **Painting styles**: Vintage oil painting style pastoral scene, a farmer herding sheep across a meadow full of wildflowers, a windmill in the distance turning under blue sky and white clouds, smoke curling from the chimney of a wooden house, bright and soft colors, full of tranquility and comfort.

  **Text generation**: A page from a botanical illustration book, hand-drawn watercolor style, depicting a "dandelion" and labeling its various parts.

  **Poster design**: Cinematic poster scene: Extreme macro close-up of eye in wooden crack. Minimalist monochrome, watercolor-CGI fusion, low saturation. Slow push-in with tremor for surreal intensity. Vast negative space, hidden title. Optimized for immersive video generation.

  **Image set generation**: Memories of an old man's life, four portraits in different frames, depicting his childhood (black and white photo), youth (military uniform photo), middle age (business suit work photo), and old age (photo with his wife).
</Accordion>

## Model availability

For model details and pricing, see [Image models](/developer-guides/getting-started/image-models).

## Getting started

### Prerequisites

[Get an API key](/api-reference/preparation/api-key) and export it as an environment variable. To use the SDK, [install it](/api-reference/preparation/install-sdk).

<Note>
  SDK version **1.25.15+** (Python) or **2.22.13+** (Java) is required.
</Note>

### Sample code

All Wan models support asynchronous calls. `wan2.7-image-pro`, `wan2.7-image`, `wan2.6-image`, and `wan2.6-t2i` also support synchronous calls. All Qwen-Image models support synchronous calls, and qwen-image-3.0 series and `qwen-image-2.1-pro` also support asynchronous calls.

<Tabs>
  <Tab title="Synchronous (Qwen-Image)">
    **Request example**

    <CodeGroup>
      ```python Python
      import json
      import os
      import dashscope
      from dashscope import MultiModalConversation

      dashscope.base_http_api_url = 'https://maas.qwencloudapi.com/api/v1'

      messages = [
        {
          "role": "user",
          "content": [
            {"text": "A vertical outdoor portrait photograph carrying the warm atmosphere of a late-afternoon street scene from top to bottom. The upper left and top of the frame are largely covered by dark green vines and small orange flowers cascading naturally from the building cornice; sunlit leaves catch yellow-green highlights while shaded ones deepen to dark green, forming a dense yet soft background layering. Across the upper-left to upper-centre area is a dark blue horizontal sign, its surface dim with a slightly matte texture, bearing the large white blackletter words Il Messaggero. The text sits slightly above centre on the left, partly veiled by the foreground foliage, set in a high-contrast typeface with decorative sharp terminals and pronounced stroke variation. Beneath the sign is the glass display window of a newsstand, its black metal framing dividing the case into multiple rectangular sections filled with newspapers, magazines and book covers, most of them illegible from the shallow depth of field and light reflections, creating a background texture of pale paper interlaced with dark frames. The upper right is a zone of strong backlight, sunlight pouring in from the end of the street, background buildings blurred into soft beige-grey planes with a pronounced shallow depth of field. Right of centre stands a young adult woman shown from above the knee up, glancing back at the camera with a smile, body turned slightly right, shoulders and back toward the viewer, her posture natural and relaxed. Her long, thick black wavy hair is traced with a golden rim light from the backlight, the ends fanning outward on the right, light and airy. Her skin is fair, her face a soft oval, brows finely shaped, eyes bright with sheer eye makeup and distinct lashes, natural highlights across her face, lips a soft coral red, smiling to show her teeth, expression warm and open. She wears small earrings and a black spaghetti-strap backless dress in deep black with a clean silhouette, the thin straps extending from shoulder to back, the line of her back clearly defined. From the lower left to the centre of the frame she holds a large bouquet of roses in both arms, mostly orange, apricot, pink and pale peach, petals curling in layers with sunlit edges, green leaves and long stems trailing from beneath, a vivid colour contrast against the black dress. The right background is a sunlit city street, its ground in warm grey and golden tones; distant buildings, street fixtures and a blurred red circular traffic sign sit in the lower-right distance, reduced by the out-of-focus falloff to colour blocks and outlines. The whole photograph carries a warm filmic treatment with fine grain, soft contrast and pronounced backlit edge light; the subject is the visual focus, while the newsstand, flowering vines, street and sunlight together create a romantic, bright, urban-stroll atmosphere."}
          ]
        }
      ]

      # If you haven't set the environment variable, replace the line below with: api_key="sk-xxx"
      api_key = os.getenv("DASHSCOPE_API_KEY")

      response = MultiModalConversation.call(
        api_key=api_key,
        model="qwen-image-3.0-pro",
        messages=messages,
        result_format='message',
        stream=False,
        watermark=False,
        prompt_extend=True,
        negative_prompt="Low resolution, low quality, distorted limbs, malformed fingers, oversaturated colors, wax-figure appearance, lack of facial detail, excessive smoothness, AI-looking artifacts, chaotic composition, blurry or warped text.",
        size='2048*2048'
      )

      if response.status_code == 200:
        print(json.dumps(response, ensure_ascii=False))
      else:
        print(f"HTTP status code: {response.status_code}")
        print(f"Error code: {response.code}")
        print(f"Error message: {response.message}")
      ```

      ```java Java
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

      public class QwenImage {

        static {
          Constants.baseHttpApiUrl = "https://maas.qwencloudapi.com/api/v1";
        }

        // If you haven't set the environment variable, replace the line below with: static String apiKey="sk-xxx"
        static String apiKey = System.getenv("DASHSCOPE_API_KEY");

        public static void call() throws ApiException, NoApiKeyException, UploadFileException, IOException {

          MultiModalConversation conv = new MultiModalConversation();

          MultiModalMessage userMessage = MultiModalMessage.builder().role(Role.USER.getValue())
              .content(Arrays.asList(
                  Collections.singletonMap("text", "A vertical outdoor portrait photograph carrying the warm atmosphere of a late-afternoon street scene from top to bottom. The upper left and top of the frame are largely covered by dark green vines and small orange flowers cascading naturally from the building cornice; sunlit leaves catch yellow-green highlights while shaded ones deepen to dark green, forming a dense yet soft background layering. Across the upper-left to upper-centre area is a dark blue horizontal sign, its surface dim with a slightly matte texture, bearing the large white blackletter words Il Messaggero. The text sits slightly above centre on the left, partly veiled by the foreground foliage, set in a high-contrast typeface with decorative sharp terminals and pronounced stroke variation. Beneath the sign is the glass display window of a newsstand, its black metal framing dividing the case into multiple rectangular sections filled with newspapers, magazines and book covers, most of them illegible from the shallow depth of field and light reflections, creating a background texture of pale paper interlaced with dark frames. The upper right is a zone of strong backlight, sunlight pouring in from the end of the street, background buildings blurred into soft beige-grey planes with a pronounced shallow depth of field. Right of centre stands a young adult woman shown from above the knee up, glancing back at the camera with a smile, body turned slightly right, shoulders and back toward the viewer, her posture natural and relaxed. Her long, thick black wavy hair is traced with a golden rim light from the backlight, the ends fanning outward on the right, light and airy. Her skin is fair, her face a soft oval, brows finely shaped, eyes bright with sheer eye makeup and distinct lashes, natural highlights across her face, lips a soft coral red, smiling to show her teeth, expression warm and open. She wears small earrings and a black spaghetti-strap backless dress in deep black with a clean silhouette, the thin straps extending from shoulder to back, the line of her back clearly defined. From the lower left to the centre of the frame she holds a large bouquet of roses in both arms, mostly orange, apricot, pink and pale peach, petals curling in layers with sunlit edges, green leaves and long stems trailing from beneath, a vivid colour contrast against the black dress. The right background is a sunlit city street, its ground in warm grey and golden tones; distant buildings, street fixtures and a blurred red circular traffic sign sit in the lower-right distance, reduced by the out-of-focus falloff to colour blocks and outlines. The whole photograph carries a warm filmic treatment with fine grain, soft contrast and pronounced backlit edge light; the subject is the visual focus, while the newsstand, flowering vines, street and sunlight together create a romantic, bright, urban-stroll atmosphere.")
              )).build();

          Map<String, Object> parameters = new HashMap<>();
          parameters.put("watermark", false);
          parameters.put("prompt_extend", true);
          parameters.put("negative_prompt", "Low resolution, low quality, distorted limbs, malformed fingers, oversaturated colors, wax-figure appearance, lack of facial detail, excessive smoothness, AI-looking artifacts, chaotic composition, blurry or warped text.");
          parameters.put("size", "2048*2048");

          MultiModalConversationParam param = MultiModalConversationParam.builder()
              .apiKey(apiKey)
              .model("qwen-image-3.0-pro")
              .messages(Collections.singletonList(userMessage))
              .parameters(parameters)
              .build();

          MultiModalConversationResult result = conv.call(param);
          System.out.println(JsonUtils.toJson(result));
        }

        public static void main(String[] args) {
          try {
            call();
          } catch (ApiException | NoApiKeyException | UploadFileException | IOException e) {
            System.out.println(e.getMessage());
          }
          System.exit(0);
        }
      }
      ```

      ```bash curl
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
                      "text": "A vertical outdoor portrait photograph carrying the warm atmosphere of a late-afternoon street scene from top to bottom. The upper left and top of the frame are largely covered by dark green vines and small orange flowers cascading naturally from the building cornice; sunlit leaves catch yellow-green highlights while shaded ones deepen to dark green, forming a dense yet soft background layering. Across the upper-left to upper-centre area is a dark blue horizontal sign, its surface dim with a slightly matte texture, bearing the large white blackletter words Il Messaggero. The text sits slightly above centre on the left, partly veiled by the foreground foliage, set in a high-contrast typeface with decorative sharp terminals and pronounced stroke variation. Beneath the sign is the glass display window of a newsstand, its black metal framing dividing the case into multiple rectangular sections filled with newspapers, magazines and book covers, most of them illegible from the shallow depth of field and light reflections, creating a background texture of pale paper interlaced with dark frames. The upper right is a zone of strong backlight, sunlight pouring in from the end of the street, background buildings blurred into soft beige-grey planes with a pronounced shallow depth of field. Right of centre stands a young adult woman shown from above the knee up, glancing back at the camera with a smile, body turned slightly right, shoulders and back toward the viewer, her posture natural and relaxed. Her long, thick black wavy hair is traced with a golden rim light from the backlight, the ends fanning outward on the right, light and airy. Her skin is fair, her face a soft oval, brows finely shaped, eyes bright with sheer eye makeup and distinct lashes, natural highlights across her face, lips a soft coral red, smiling to show her teeth, expression warm and open. She wears small earrings and a black spaghetti-strap backless dress in deep black with a clean silhouette, the thin straps extending from shoulder to back, the line of her back clearly defined. From the lower left to the centre of the frame she holds a large bouquet of roses in both arms, mostly orange, apricot, pink and pale peach, petals curling in layers with sunlit edges, green leaves and long stems trailing from beneath, a vivid colour contrast against the black dress. The right background is a sunlit city street, its ground in warm grey and golden tones; distant buildings, street fixtures and a blurred red circular traffic sign sit in the lower-right distance, reduced by the out-of-focus falloff to colour blocks and outlines. The whole photograph carries a warm filmic treatment with fine grain, soft contrast and pronounced backlit edge light; the subject is the visual focus, while the newsstand, flowering vines, street and sunlight together create a romantic, bright, urban-stroll atmosphere."
            }
                  ]
          }
              ]
        },
        "parameters": {
              "negative_prompt": "Low resolution, low quality, distorted limbs, malformed fingers, oversaturated colors, wax-figure appearance, lack of facial detail, excessive smoothness, AI-looking artifacts, chaotic composition, blurry or warped text.",
              "prompt_extend": true,
              "watermark": false,
              "size": "2048*2048"
        }
      }'
      ```
    </CodeGroup>

    **Response example**

    <Accordion title="Full JSON response">
      ```json
      {
        "status_code": 200,
        "request_id": "d2d1a8c0-325f-9b9d-8b90-xxxxxx",
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
                    "image": "https://dashscope-result-intl.oss-cn-shanghai.aliyuncs.com/xxx.png?Expires=xxx"
                  }
                ]
              }
            }
          ]
        },
        "usage": {
          "output_height": 2048,
          "output_width": 2048,
          "input_image_count": 0,
          "input_image_type": "qima_input_2k",
          "output_image_count": 1,
          "output_image_type": "qima_output_2k"
        }
      }
      ```
    </Accordion>
  </Tab>

  <Tab title="Asynchronous (Wan)">
    **Request example**

    <Note>
      With curl, submit a task (POST) first, then query the result (GET) using the returned `task_id`. The `task_id` is valid for 24 hours.
    </Note>

    <CodeGroup>
      ```python Python
      import os
      import dashscope
      from dashscope.aigc.image_generation import ImageGeneration
      from dashscope.api_entities.dashscope_response import Message

      dashscope.base_http_api_url = 'https://maas.qwencloudapi.com/api/v1'

      # If you have not configured the environment variable, replace the following line with your API key: api_key="sk-xxx"
      api_key = os.getenv("DASHSCOPE_API_KEY")

      def main():
        message = Message(
          role="user",
          content=[
            {
              "text": "A young woman taking a casual selfie outdoors, natural lighting, warm tones, soft bokeh background with greenery"
            }
          ]
        )

        # Submit async task
        print("Submitting async task...")
        response = ImageGeneration.async_call(
          model="wan2.7-image-pro",
          api_key=api_key,
          messages=[message],
          enable_sequential=False,
          n=1,
          size="2K"
        )

        if response.status_code == 200:
          print(f"Task submitted, task ID: {response.output.task_id}")

          # Wait for task completion
          status = ImageGeneration.wait(task=response, api_key=api_key)

          if status.output.task_status == "SUCCEEDED":
            print("Task completed!")
            print(status)
          else:
            print(f"Task failed, status: {status.output.task_status}")
        else:
          print(f"Task creation failed: {response.code} - {response.message}")

      if __name__ == "__main__":
        try:
          main()
        except Exception as e:
          print(f"Error: {e}")
      ```

      ```java Java
      import com.alibaba.dashscope.aigc.imagegeneration.*;
      import com.alibaba.dashscope.exception.ApiException;
      import com.alibaba.dashscope.exception.NoApiKeyException;
      import com.alibaba.dashscope.exception.UploadFileException;
      import com.alibaba.dashscope.utils.Constants;
      import com.alibaba.dashscope.utils.JsonUtils;

      import java.util.Collections;

      public class Main {

        static {
          Constants.baseHttpApiUrl = "https://maas.qwencloudapi.com/api/v1";
        }

        // If you have not configured the environment variable, replace the following line with your API key: apiKey="sk-xxx"
        static String apiKey = System.getenv("DASHSCOPE_API_KEY");

        public static ImageGenerationResult waitTask(String taskId)
            throws ApiException, NoApiKeyException {
          ImageGeneration imageGeneration = new ImageGeneration();
          return imageGeneration.wait(taskId, apiKey);
        }

        public static void asyncCall() throws ApiException, NoApiKeyException, UploadFileException {
          ImageGenerationMessage message = ImageGenerationMessage.builder()
              .role("user")
              .content(Collections.singletonList(
                  Collections.singletonMap("text", "A young woman taking a casual selfie outdoors, natural lighting, warm tones, soft bokeh background with greenery")
              )).build();

          ImageGenerationParam param = ImageGenerationParam.builder()
              .apiKey(apiKey)
              .model("wan2.7-image-pro")
              .messages(Collections.singletonList(message))
              .enableSequential(false)
              .n(1)
              .size("2K")
              .build();

          ImageGeneration imageGeneration = new ImageGeneration();
          ImageGenerationResult taskResult = null;
          try {
            System.out.println("---- Submitting async task ----");
            taskResult = imageGeneration.asyncCall(param);
          } catch (ApiException | NoApiKeyException | UploadFileException e) {
            throw new RuntimeException(e.getMessage());
          }
          System.out.println("Task created: " + JsonUtils.toJson(taskResult));

          // Wait for task completion
          String taskId = taskResult.getOutput().getTaskId();
          ImageGenerationResult result = waitTask(taskId);
          System.out.println(JsonUtils.toJson(result));
        }

        public static void main(String[] args) {
          try {
            asyncCall();
          } catch (ApiException | NoApiKeyException | UploadFileException e) {
            System.out.println(e.getMessage());
          }
        }
      }
      ```

      ```bash curl (Step 1: Submit task)
      curl --location 'https://maas.qwencloudapi.com/api/v1/services/aigc/image-generation/generation' \
      --header 'Content-Type: application/json' \
      --header "Authorization: Bearer $DASHSCOPE_API_KEY" \
      --header 'X-DashScope-Async: enable' \
      --data '{
        "model": "wan2.7-image-pro",
        "input": {
          "messages": [
            {
              "role": "user",
              "content": [
                {
                  "text": "A young woman taking a casual selfie outdoors, natural lighting, warm tones, soft bokeh background with greenery"
                }
              ]
            }
          ]
        },
        "parameters": {
          "size": "2K",
          "n": 1,
          "watermark": false,
          "thinking_mode": true
        }
      }'
      ```

      ```bash curl (Step 2: Query result)
      curl -X GET https://maas.qwencloudapi.com/api/v1/tasks/{task_id} \
      --header "Authorization: Bearer $DASHSCOPE_API_KEY"
      ```
    </CodeGroup>

    **Create task response**

    <Accordion title="Full JSON response">
      ```json
      {
        "output": {
          "task_status": "PENDING",
          "task_id": "0385dc79-5ff8-4d82-bcb6-xxxxxx"
        },
        "request_id": "4909100c-7b5a-9f92-bfe5-xxxxxx"
      }
      ```
    </Accordion>

    **Query result response**

    <Accordion title="Full JSON response">
      ```json
      {
        "status_code": 200,
        "request_id": "56e318fd-ed60-99e8-8ca1-cdef25ca4xxx",
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
                    "image": "https://dashscope-result-intl.oss-cn-shanghai.aliyuncs.com/xxxxxx.png?Expires=xxxxxx",
                    "type": "image"
                  }
                ]
              }
            }
          ],
          "audio": null,
          "task_id": "77093787-a217-4c29-9cd4-ca7b5ac86xxx",
          "task_status": "SUCCEEDED",
          "submit_time": "2026-03-31 23:04:46.166",
          "scheduled_time": "2026-03-31 23:04:46.208",
          "end_time": "2026-03-31 23:05:11.664",
          "finished": true
        },
        "usage": {
          "input_tokens": 720,
          "output_tokens": 11,
          "characters": 0,
          "size": "2048*2048",
          "total_tokens": 731,
          "image_count": 1
        }
      }
      ```
    </Accordion>
  </Tab>
</Tabs>

## Key capabilities

### Instruction following

**Parameters**:

- **Prompt** (required): Describes the desired content, style, and composition. Pass the prompt in the following format:
  - **Qwen-Image, Wan 2.7, and `wan2.6-t2i`**: Use `input.messages[].content[].text`. See the sample code in the corresponding tab under [Sample code](#sample-code).
  - **Wan 2.5 and earlier**: Use `input.prompt`.
- **negative\_prompt** (optional): Describes elements to exclude from the image, such as "blurry" or "extra fingers". Set via `parameters.negative_prompt`. Supported by all models **except** `wan2.7-image-pro` and `wan2.7-image`.

<Note>
  `wan2.7-image-pro` and `wan2.7-image` do **not** support `negative_prompt`. Use a positive prompt to guide generation instead.
</Note>

**Writing tips**: Structured prompts tend to produce better results. See [Text-to-image prompt guide](/developer-guides/accuracy-tuning/image-generation).

### Enable prompt rewriting

**Parameter**: `parameters.prompt_extend` (bool, default: `true`).

Automatically expands short prompts to improve image quality, adding 3-4 seconds of latency.

<Note>
  `wan2.7-image-pro` and `wan2.7-image` do **not** support `prompt_extend`. Use `thinking_mode` instead — see [Wan 2.7 parameters](#wan-2-7-parameters).
</Note>

**When to use**:

- **Enable** when your prompt is simple or broad — this can significantly improve quality.
- **Disable** (`false`) when you need fine-grained control, have already written a detailed prompt, or are sensitive to latency.

### Choose the prompt rewriting mode

**Parameter**: `parameters.prompt_extend_mode` (string, default: `direct`). Supported by the qwen-image-3.0 series and qwen-image-2.1-pro only, and takes effect when `prompt_extend` is `true`.

| Value    | Description                                              | When to use                                                         |
| -------- | -------------------------------------------------------- | ------------------------------------------------------------------- |
| `direct` | Direct prompt enhancement (DPE). The default.            | Most scenarios                                                      |
| `agent`  | Agent prompt enhancement (APE). Finer-grained rewriting. | Very short prompts where the model needs to fill in a lot of detail |

<Note>
  `agent` is supported for text-to-image only. Passing it for image editing returns a 400 error.
</Note>

### Enable thinking mode

**Parameter**: `parameters.enable_thinking` (boolean, default: `true`). Supported by the qwen-image-3.0 series and qwen-image-2.1-pro only, for text-to-image and direct image editing — not supported for agent-mode image editing. Requires `prompt_extend=true`.

Enhances model reasoning to improve image quality, but increases generation time.

### Set the output image resolution

**Parameter**: `parameters.size` (string), in the format `"width*height"`.

| Model                                     | Size format                   | Supported range                    | Default                          | Aspect ratio |
| ----------------------------------------- | ----------------------------- | ---------------------------------- | -------------------------------- | ------------ |
| qwen-image-3.0 series, qwen-image-2.1-pro | Custom `"width*height"`       | 512\*512 – 2048\*2048              | Auto-recommended from the prompt | 1:8 – 8:1    |
| qwen-image-2.0 series                     | Custom `"width*height"`       | 512\*512 – 2048\*2048              | 2048\*2048 (1:1)                 | —            |
| `wan2.7-image-pro`                        | Shorthand or `"width*height"` | 768\*768 – 4096\*4096              | `"2K"` (2048\*2048)              | 1:8 – 8:1    |
| `wan2.7-image`                            | Shorthand or `"width*height"` | 768\*768 – 2048\*2048              | `"2K"` (2048\*2048)              | 1:8 – 8:1    |
| `wan2.6-image`                            | Custom `"width*height"`       | 768\*768 – 1280\*1280              | Matches input (≤1280\*1280)      | 1:4 – 4:1    |
| `wan2.6-t2i`, `wan2.5-t2i-preview`        | Custom `"width*height"`       | 1280\*1280 – 1440\*1440            | 1280\*1280                       | 1:4 – 4:1    |
| wan2.2 and earlier t2i models             | Custom `"width*height"`       | \[512, 1440] per side, ≤1440\*1440 | 1024\*1024 (1:1)                 | —            |

<Note>
  `wan2.6-image` is listed here for its interleaved text-image generation mode only. For image editing, see [Image editing](/developer-guides/image-generation/image-editing).
</Note>

**Shorthand sizes** (wan2.7 only; cannot mix with pixel values):

| Shorthand | Resolution | wan2.7-image-pro    | wan2.7-image        |
| --------- | ---------- | ------------------- | ------------------- |
| `"1K"`    | 1024\*1024 | Supported           | Supported           |
| `"2K"`    | 2048\*2048 | Supported (default) | Supported (default) |
| `"4K"`    | 4096\*4096 | Supported           | Not supported       |

**Recommended resolutions by pixel range**:

| Aspect ratio | 4K         | 2K         | 1K         |
| ------------ | ---------- | ---------- | ---------- |
| 1:1          | 4096\*4096 | 2048\*2048 | 1280\*1280 |
| 16:9         | 4096\*2304 | 2688\*1536 | 1696\*960  |
| 9:16         | 2304\*4096 | 1536\*2688 | 960\*1696  |
| 4:3          | 4096\*3072 | 2368\*1728 | 1472\*1104 |
| 3:4          | 3072\*4096 | 1728\*2368 | 1104\*1472 |

- **4K**: wan2.7-image-pro only.
- **2K**: wan2.7-image-pro, wan2.7-image, qwen-image-2.0/3.0 series, qwen-image-2.1-pro.
- **1K**: Wan t2i models.

### Set the number of images

**Parameter**: `parameters.n` (integer).

| Model                                         | Range  | Default |
| --------------------------------------------- | ------ | ------- |
| wan2.7 (`enable_sequential=false`)            | 1–4    | 4       |
| wan2.7 (`enable_sequential=true`)             | 1–12   | 12      |
| qwen-image-2.0/3.0 series, qwen-image-2.1-pro | 1–6    | 1       |
| wan2.6-image (`enable_interleave=false`)      | 1–4    | 4       |
| wan2.6-image (`enable_interleave=true`)       | 1 only | 1       |
| wan2.6-t2i / wan2.5 and earlier               | 1–4    | 4       |

<Note>
  Cost = unit price x number of successfully generated images. Set `n` to 1 during testing.
</Note>

When using `wan2.6-image` in interleaved text-image mode (`enable_interleave=true`), `n` must be 1. To control the maximum number of generated images, use `parameters.max_images` (range: 1–5, default: 5). The actual count is determined by the model and may be less than the specified maximum.

### Wan 2.7 parameters

The following parameters are exclusive to `wan2.7-image-pro` and `wan2.7-image`.

- **`enable_sequential`** (bool, default: `false`): Enables image set generation. When `true`, you can generate 1-12 coherent images per request by setting `n` between 1 and 12.

  <Warning>
    When `enable_sequential` is set to `true`, `thinking_mode` and `color_palette` are unavailable.
  </Warning>

- **`thinking_mode`** (bool, default: `true`): Enables enhanced reasoning for better prompt understanding and image quality. Only available when `enable_sequential` is `false`.

- **`color_palette`** (array): Defines a custom color theme. Specify 3-10 colors (8 recommended), each with a hex value and a ratio (percentage string). Ratios must sum to 100%. Only available when `enable_sequential` is `false`.

<Accordion title="Color palette example">
  ```json
  "color_palette": [
    {"hex": "#C2D1E6", "ratio": "23.51%"},
    {"hex": "#CDD8E9", "ratio": "20.13%"},
    {"hex": "#B5C8DB", "ratio": "15.88%"},
    {"hex": "#C0B5B4", "ratio": "13.27%"},
    {"hex": "#DAE0EC", "ratio": "10.11%"},
    {"hex": "#636574", "ratio": "8.93%"},
    {"hex": "#CACAD2", "ratio": "5.55%"},
    {"hex": "#CBD4E4", "ratio": "2.62%"}
  ]
  ```
</Accordion>

## Going live

### Fault tolerance

- **Rate limits**: A `Throttling` error code or HTTP 429 means rate limiting is active. See [Rate limits](/developer-guides/administration/rate-limits).
- **Async task polling**: Poll every 3 seconds for the first 30 seconds, then increase the interval. Set a final timeout (e.g. 2 minutes) and treat the task as failed if it expires.

### Risk prevention

- **Result persistence**: Image URLs expire after 24 hours. Download and store images in your own storage (e.g. OSS) immediately after retrieval.
- **Content moderation**: All `prompt` and `negative_prompt` inputs are moderated. Non-compliant input is blocked with a `DataInspectionFailed` error.
- **Copyright and compliance**: Prompts that reference brand trademarks, celebrity likenesses, or copyrighted IP may pose infringement risks. You are responsible for any resulting liabilities.

## API reference

- [Qwen - Synchronous](/api-reference/image-generation/qwen-text-to-image)
- [Z-Image](/api-reference/image-generation/z-image)
- [Wan 2.7 - Image generation & editing](/api-reference/image-generation/wan27-image-gen-edit/create-task)
- [Wan 2.6 - Image generation & editing](/api-reference/image-generation/wan26-image-gen-edit/create-task)
- [Wan - text-to-image V2](/api-reference/image-generation/wan-text-to-image-v2/create-task)

## Error codes

If a call fails, see [Error messages](/api-reference/preparation/error-messages).

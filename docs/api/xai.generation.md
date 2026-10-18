#### Imagine API

# Image Prompting Guide

This guide covers prompts for `grok-imagine-image-2.0` stills: a new image from text, or an edit of an existing one.

The score that matters is whether the subject in the picture matches the subject in the prompt. Palette, film grain, and styling come after that.

## Request settings

Generate a new image with [`/v1/images/generations`](/developers/model-capabilities/images/generation):

```bash
curl -X POST https://api.x.ai/v1/images/generations \
  -H "Authorization: Bearer $XAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "grok-imagine-image-2.0",
    "prompt": "...",
    "aspect_ratio": "3:2",
    "resolution": "2k",
    "quality": "medium",
    "response_format": "b64_json",
    "n": 1
  }'
```

Use [`/v1/images/edits`](/developers/model-capabilities/images/editing) when a picture already has the right subject and layout:

```bash
curl -X POST https://api.x.ai/v1/images/edits \
  -H "Authorization: Bearer $XAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "grok-imagine-image-2.0",
    "prompt": "...",
    "image": {"url": "data:image/jpeg;base64,...", "type": "image_url"},
    "aspect_ratio": "3:2",
    "resolution": "2k",
    "quality": "medium",
    "response_format": "b64_json"
  }'
```

Lock `aspect_ratio` across a series so the images sit together; `3:2` suits most photographs. [`quality`](/developers/model-capabilities/images/generation#quality) accepts `low`, `medium`, or `auto`, and `auto` currently serves `low` on generation and `medium` on edit. Pin `medium` and `2k` when fine detail has to be recognizable. Generated URLs are temporary, so request `b64_json` when you plan to keep the file, or store it with [Imagine outputs](/developers/model-capabilities/imagine/files/outputs).

## Write the subject first

Open with one sentence that names the subject, the state or form it is in, what holds or supports it, and the setting.

> A matte sage-green ceramic pour-over dripper sitting on a glass carafe half full of coffee, on a pale oak counter against white square tile.

Then describe the subject as something you could point at and recognize:

* Every key element, by name, in the form it takes in the picture. A wool coat is heavy twill with a visible weave. Coffee in a glass carafe is dark amber, lighter at the edges. A paperback has a creased spine. Feta in a salad is irregular crumbles.
* How many. Two mugs. One open notebook. One person, seen from the waist up. One wedge pulled a finger-width aside.
* Where the eye enters. A 45-degree three-quarter view at counter height, or a straight-down overhead of the desk. State it once in the subject paragraph, and repeat it in the composition paragraph in the same words.

A prompt that leads with mood, film stock, or a list of things to avoid produces a styled scene whose subject is a stand-in.

## Shape of a prompt

Keep the same order in every prompt of a series:

1. **Subject.** What it is, its form, what holds or supports it, the setting.
2. **Details and focal point.** Key elements, materials, count, and the camera angle, written as one cluster that fills the frame.
3. **Art direction.** The color fields and the two or three props, each named: oak counter, white tile, linen towel, one brass spoon.
4. **Light.** The source, its direction, and how hard it is, for example hard afternoon sun from the left with on-camera fill flash. Say what the highlights and shadows look like, and ask for an exposure bright enough to read the subject.
5. **Color.** The subject's colors and the background colors as separate fields: sage green, dark amber, pale oak, white.
6. **Composition.** Crop, what the frame cuts off, and what stays sharp. One scene.

Write prose that describes one photograph. A line that reads like an instruction ("make the coat darker") describes an attribute of that single picture; the model has no before-and-after to work from.

## Say the thing you want to see

Describe the material. "Brushed steel with fine horizontal grain" gives the model a surface to render, while "not shiny" gives it nothing to draw.

A long avoid list is easy to ignore, and it spends the prompt on objects that then show up anyway. Keep a constraint only when it is a count or a specific object, and pair it with the thing you want: "one stainless spoon resting on the saucer."

Name each object as what it is. A plate, a bowl, and an enameled skillet are different things, and so are a tote, a satchel, and a backpack. When the distinction matters, describe the feature that makes it that object: for a skillet, a cream enamel wall rising around the food and a handle cropped by the frame. A thin brown line on a ceramic rim reads as a plate.

State texture once, physically. Repeated texture words get painted onto the wrong surface: cottage-cheese curds turn into a pebbled skin on the wrap that holds them, Parmesan flakes turn into paper squares, and potato chunks climb out of a frittata to sit on top as garnish.

Ask for light that leaves the subject readable. "Bright and vivid, hard sun, flash, warm shadows with texture" holds. "Slightly underexposed" and "crushed shadows" pull the same picture dark. Leave out chromatic fringing, dust, and scratches; the model draws them as defects.

## Generate, then edit

Generate when there is no picture yet, or when the subject itself is wrong: it is a different object, a key element is missing, or the count is off.

Edit when the subject and the layout are already right and one to three details need to change. Lead with those changes. Follow with one sentence that keeps the same subject, angle, crop, and prop placement.

> Replace the metal watch strap with a brown leather strap with cream stitching. Keep the same wrist, the same shirt cuff, the same angle, and the same desk.

An edit that opens with "keep everything" barely moves. An edit that pastes in a full new prompt restages the scene: the subject drifts, a second copy of a prop appears, and one object turns into a similar one, such as a plate into a pan.

Change the words when a fix does not land. Repeating "more realistic" turns into a new texture on the wrong part of the picture. Point at the part instead: "the cut face shows separate russet potato chunks in moist egg," or "the leather shows creases where the strap bends."

## How to tell a prompt worked

Judge each picture against the original prompt, even after several rounds of edits. The question is whether this subject matches that prompt:

* The subject is the one named.
* Each element named in the prompt is visible, in the form the prompt describes.
* The count matches.
* Each object is the object the prompt named.
* The picture is bright enough to see the subject.

A matching background with the wrong subject is a miss. Save the prompt next to the image. On the next pass, change only the detail that failed and leave the rest of the original prompt in place, so the subject and the layout stay put.

## Short prompts

A single still outside a series needs four to six sentences: subject and details, then setting, then light, then crop.

> A pair of white leather low-top sneakers with gum rubber soles, shot from a low three-quarter angle. The laces are loosely tied, the toe boxes show soft creases, and the left shoe sits slightly ahead of the right. They stand on pale gray concrete against a warm off-white wall. Hard morning sun from the right casts long, sharp shadows, with soft fill on the shaded side. 50mm product photograph, both shoes sharp, cropped just above the heel tabs.

> A woman in her sixties with short silver hair and reading glasses pushed up on her head, laughing while she pots a small olive tree at a wooden workbench. Her hands are dusted with dark soil and her sleeves are rolled to the elbow. Terracotta pots and a steel trowel sit on the bench beside her. Soft window light from the left, bright and warm, with gentle shadows. 35mm photograph at eye level, framed from the waist up, her face in sharp focus.

> A vegetarian Greek salad in a warm cream ceramic bowl, shot from a slight overhead angle. Cucumber, tomato, thick red onion, irregular feta crumbles, and wrinkled Kalamata olives overlap in olive oil, with oregano on the tomatoes. The bowl sits on butter-yellow linen over terracotta. A warm pita is partly visible at the right edge and a fork rests on the rim. Hard summer sun and on-camera flash, bright and vivid, with warm shadows. 35mm photograph, food filling the bowl, one frame.

## Related

* [Image Generation](/developers/model-capabilities/images/generation): request parameters for aspect ratio, resolution, and quality
* [Image Editing](/developers/model-capabilities/images/editing): edit an existing image with a prompt
* [Multi-Image Editing](/developers/model-capabilities/images/multi-image-editing): edit with several source images

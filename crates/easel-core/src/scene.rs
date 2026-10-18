use crate::geometry::{Rect, Size};
use crate::prose::{self, RefChange};
use serde::{Deserialize, Serialize};

fn current_version() -> u32 {
    1
}

/// Project-relative path of an image file, plus the pixel size used as a frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRef {
    pub path: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

impl ImageRef {
    #[must_use]
    pub fn new(path: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            path: path.into(),
            width,
            height,
        }
    }

    #[must_use]
    pub fn size(&self) -> Option<Size> {
        if self.width == 0 || self.height == 0 {
            None
        } else {
            Some(Size {
                w: self.width,
                h: self.height,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoxId(String);

impl BoxId {
    pub fn parse(raw: &str) -> Result<Self, String> {
        if is_box_id(raw) {
            Ok(Self(raw.to_string()))
        } else {
            Err(format!("invalid box id `{raw}`"))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for BoxId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

fn is_box_id(raw: &str) -> bool {
    let Some((prefix, suffix)) = raw.rsplit_once('_') else {
        return false;
    };
    let mut chars = prefix.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && !suffix.is_empty()
        && suffix.chars().all(|c| c.is_ascii_digit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoxKind {
    Layout,
    New,
    Keep,
    Move,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoxElement {
    pub id: BoxId,
    pub kind: BoxKind,
    pub desc: String,
    #[serde(default)]
    pub src: Option<Rect>,
    #[serde(default)]
    pub tgt: Option<Rect>,
    #[serde(default)]
    pub from_image: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scene {
    #[serde(default = "current_version")]
    pub version: u32,
    #[serde(default)]
    pub source_image: Option<ImageRef>,
    #[serde(default)]
    pub references: Vec<ImageRef>,
    #[serde(default)]
    pub boxes: Vec<BoxElement>,
    #[serde(default)]
    pub prose: String,
    #[serde(default)]
    pub aspect_ratio: Option<String>,
}

impl Scene {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            version: current_version(),
            source_image: None,
            references: Vec::new(),
            boxes: Vec::new(),
            prose: String::new(),
            aspect_ratio: None,
        }
    }

    /// Source pixel size, or a 1000-wide virtual frame for a concrete ratio.
    #[must_use]
    pub fn frame(&self) -> Option<Size> {
        if let Some(size) = self.source_image.as_ref().and_then(ImageRef::size) {
            return Some(size);
        }
        virtual_frame(self.aspect_ratio.as_deref())
    }

    #[must_use]
    pub fn allowed_kinds(&self) -> &'static [BoxKind] {
        if self.source_image.is_some() {
            &[BoxKind::New, BoxKind::Keep, BoxKind::Move, BoxKind::Remove]
        } else {
            &[BoxKind::Layout]
        }
    }

    pub fn add_box(&mut self, rect: Rect, kind: BoxKind) -> BoxId {
        self.add_box_on(rect, kind, 0)
    }

    pub fn add_box_on(&mut self, rect: Rect, kind: BoxKind, slot: usize) -> BoxId {
        self.add_box_with_prefix_on(rect, kind, "box", slot)
    }

    pub fn add_box_with_prefix(&mut self, rect: Rect, kind: BoxKind, prefix: &str) -> BoxId {
        self.add_box_with_prefix_on(rect, kind, prefix, 0)
    }

    fn add_box_with_prefix_on(
        &mut self,
        rect: Rect,
        kind: BoxKind,
        prefix: &str,
        slot: usize,
    ) -> BoxId {
        let id = self.next_id(prefix);
        let (src, tgt, from_image) = placed_rect(kind, rect, slot);
        self.boxes.push(BoxElement {
            id: id.clone(),
            kind,
            desc: String::new(),
            src,
            tgt,
            from_image,
        });
        id
    }

    pub fn update_box(&mut self, id: &BoxId, edit: impl FnOnce(&mut BoxElement)) -> bool {
        let Some(element) = self.boxes.iter_mut().find(|element| &element.id == id) else {
            return false;
        };
        edit(element);
        true
    }

    pub fn remove_box(&mut self, id: &BoxId) -> bool {
        let before = self.boxes.len();
        self.boxes.retain(|element| &element.id != id);
        self.boxes.len() != before
    }

    pub fn rescale_boxes_to_frame(
        &mut self,
        from: crate::geometry::Size,
        to: crate::geometry::Size,
    ) {
        for element in &mut self.boxes {
            if let Some(rect) = element.src {
                element.src = Some(crate::geometry::Rect::from_grid(rect.to_grid(from), to));
            }
            if let Some(rect) = element.tgt {
                element.tgt = Some(crate::geometry::Rect::from_grid(rect.to_grid(from), to));
            }
        }
    }

    pub fn collapse_move_boxes(&mut self) {
        for element in &mut self.boxes {
            if element.kind == BoxKind::Move
                && element.from_image.unwrap_or(0) == 0
                && let Some(tgt) = element.tgt
            {
                element.src = Some(tgt);
            }
        }
    }

    pub fn rename_box(&mut self, id: &BoxId, new_raw: &str) -> Result<(), String> {
        let new_id = BoxId::parse(new_raw)?;
        if self
            .boxes
            .iter()
            .any(|element| element.id == new_id && &element.id != id)
        {
            return Err(format!("box id `{new_raw}` is already in use"));
        }
        let Some(element) = self.boxes.iter_mut().find(|element| &element.id == id) else {
            return Err(format!("no box `{id}`", id = id.as_str()));
        };
        let old = element.id.as_str().to_string();
        element.id = new_id;
        self.prose = prose::rename_tag(&self.prose, &old, new_raw);
        Ok(())
    }

    pub fn set_source(&mut self, image: ImageRef) {
        let previous = self.frame();
        let replacing = self.source_image.is_some();
        if self.source_image.is_none() && !self.references.is_empty() {
            self.prose = prose::shift_ref_tags(&self.prose, RefChange::InsertedAt(0));
            retarget_boxes(&mut self.boxes, RefChange::InsertedAt(0));
        }
        let next = image.size();
        self.source_image = Some(image);
        let (Some(previous), Some(next)) = (previous, next) else {
            return;
        };
        if replacing {
            for element in &mut self.boxes {
                if element.from_image == Some(0) {
                    rescale_rects(element, previous, next);
                }
            }
        } else {
            for element in &mut self.boxes {
                if element.kind == BoxKind::Layout {
                    rescale_rects(element, previous, next);
                }
            }
        }
    }

    pub fn clear_source(&mut self) {
        if self.source_image.is_none() {
            return;
        }
        self.source_image = None;
        self.prose = prose::shift_ref_tags(&self.prose, RefChange::Removed(0));
        retarget_boxes(&mut self.boxes, RefChange::Removed(0));
    }

    pub fn add_reference(&mut self, image: ImageRef) {
        self.references.push(image);
    }

    pub fn remove_reference(&mut self, index: usize) {
        let mut slots = self.slots();
        if index >= slots.len() {
            return;
        }
        slots.remove(index);
        self.prose = prose::shift_ref_tags(&self.prose, RefChange::Removed(index));
        retarget_boxes(&mut self.boxes, RefChange::Removed(index));
        self.write_slots(slots);
    }

    pub fn move_reference(&mut self, from: usize, to: usize) {
        let mut slots = self.slots();
        if from >= slots.len() || to >= slots.len() || from == to {
            return;
        }
        let image = slots.remove(from);
        slots.insert(to, image);
        self.prose = prose::shift_ref_tags(&self.prose, RefChange::Moved { from, to });
        retarget_boxes(&mut self.boxes, RefChange::Moved { from, to });
        self.write_slots(slots);
    }

    #[must_use]
    pub fn slots(&self) -> Vec<ImageRef> {
        let mut slots = Vec::new();
        if let Some(source) = &self.source_image {
            slots.push(source.clone());
        }
        slots.extend(self.references.iter().cloned());
        slots
    }

    fn write_slots(&mut self, slots: Vec<ImageRef>) {
        if let Some((source, rest)) = slots.split_first() {
            self.source_image = Some(source.clone());
            self.references = rest.to_vec();
        } else {
            self.source_image = None;
            self.references.clear();
        }
    }

    /// Next unused `{prefix}_{n}` id, one past the highest existing suffix.
    #[must_use]
    pub fn next_id(&self, prefix: &str) -> BoxId {
        let mut max_suffix = 0u64;
        for element in &self.boxes {
            let Some((id_prefix, suffix)) = element.id.as_str().rsplit_once('_') else {
                continue;
            };
            if id_prefix != prefix {
                continue;
            }
            let Ok(suffix) = suffix.parse::<u64>() else {
                continue;
            };
            max_suffix = max_suffix.max(suffix);
        }
        BoxId::parse(&format!("{prefix}_{}", max_suffix + 1)).expect("prefix produced a box id")
    }
}

fn placed_rect(
    kind: BoxKind,
    rect: Rect,
    slot: usize,
) -> (Option<Rect>, Option<Rect>, Option<usize>) {
    match kind {
        BoxKind::Layout => (Some(rect), None, None),
        BoxKind::New => (None, Some(rect), Some(slot)),
        BoxKind::Keep => (Some(rect), Some(rect), Some(slot)),
        BoxKind::Move | BoxKind::Remove => (Some(rect), None, Some(slot)),
    }
}

fn rescale_rects(element: &mut BoxElement, from: Size, to: Size) {
    if from == to {
        return;
    }
    if let Some(src) = element.src {
        element.src = Some(Rect::from_grid(src.to_grid(from), to));
    }
    if let Some(tgt) = element.tgt {
        element.tgt = Some(Rect::from_grid(tgt.to_grid(from), to));
    }
}

fn virtual_frame(ratio: Option<&str>) -> Option<Size> {
    let ratio = ratio?;
    if ratio.is_empty() || ratio == "auto" {
        return None;
    }
    let (width, height) = ratio.split_once(':')?;
    let width: u32 = width.parse().ok()?;
    let height: u32 = height.parse().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(Size {
        w: 1000,
        h: 1000 * height / width,
    })
}

fn retarget_boxes(boxes: &mut Vec<BoxElement>, change: RefChange) {
    if let RefChange::Removed(removed) = change {
        boxes.retain(|element| element.from_image != Some(removed));
        for element in boxes {
            if let Some(index) = element.from_image
                && index > removed
            {
                element.from_image = Some(index - 1);
            }
        }
        return;
    }
    for element in boxes {
        element.from_image = element.from_image.and_then(|index| change.map_index(index));
    }
}

#[cfg(test)]
mod tests {
    use super::{BoxElement, BoxId, BoxKind, ImageRef, Scene};
    use crate::geometry::{Rect, Size};

    fn element(id: &str) -> BoxElement {
        BoxElement {
            id: BoxId::parse(id).unwrap(),
            kind: BoxKind::New,
            desc: String::new(),
            src: None,
            tgt: None,
            from_image: None,
        }
    }

    #[test]
    fn box_id_accepts_the_documented_pattern() {
        assert!(BoxId::parse("animal_1").is_ok());
        assert!(BoxId::parse("Animal_1").is_err());
        assert!(BoxId::parse("animal").is_err());
        assert!(BoxId::parse("1_animal").is_err());
    }

    #[test]
    fn next_id_skips_to_one_past_the_highest_suffix() {
        let scene = Scene {
            version: 1,
            source_image: None,
            references: Vec::new(),
            boxes: vec![element("animal_1"), element("animal_3")],
            prose: String::new(),
            aspect_ratio: None,
        };
        assert_eq!(scene.next_id("animal").as_str(), "animal_4");
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn grid_scaled(rect: Rect, from: crate::geometry::Size, to: crate::geometry::Size) -> Rect {
        Rect::from_grid(rect.to_grid(from), to)
    }

    #[test]
    fn add_box_allocates_ids_past_the_highest_suffix() {
        let mut scene = Scene::empty();
        let first = scene.add_box_with_prefix(rect(0, 0, 10, 10), BoxKind::Layout, "animal");
        let second = scene.add_box_with_prefix(rect(0, 0, 10, 10), BoxKind::Layout, "animal");
        assert_eq!(first.as_str(), "animal_1");
        assert_eq!(second.as_str(), "animal_2");
        scene.remove_box(&first);
        let third = scene.add_box_with_prefix(rect(0, 0, 10, 10), BoxKind::Layout, "animal");
        assert_eq!(third.as_str(), "animal_3");
    }

    #[test]
    fn rename_box_rewrites_prose_and_rejects_collisions() {
        let mut scene = Scene::empty();
        let id = scene.add_box_with_prefix(rect(0, 0, 10, 10), BoxKind::Layout, "cat");
        scene.prose = "make <cat_1> orange".into();
        scene.rename_box(&id, "cat_2").unwrap();
        assert_eq!(scene.prose, "make <cat_2> orange");
        assert_eq!(scene.boxes[0].id.as_str(), "cat_2");

        scene.add_box_with_prefix(rect(0, 0, 10, 10), BoxKind::Layout, "cat");
        let err = scene.rename_box(&scene.boxes[0].id.clone(), "cat_3");
        assert!(err.is_err(), "cat_3 is already in use");
        assert!(scene.rename_box(&scene.boxes[0].id.clone(), "Cat").is_err());
    }

    #[test]
    fn allowed_kinds_follow_the_source_image() {
        let mut scene = Scene::empty();
        assert_eq!(scene.allowed_kinds(), &[BoxKind::Layout]);
        scene.set_source(ImageRef::new("src.png", 100, 100));
        assert_eq!(
            scene.allowed_kinds(),
            &[BoxKind::New, BoxKind::Keep, BoxKind::Move, BoxKind::Remove]
        );
    }

    #[test]
    fn removing_the_source_shifts_reference_tags_and_from_image() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("src.png", 100, 100));
        scene.add_reference(ImageRef::new("extra.png", 100, 100));
        scene.prose = "use <ref_image_1>".into();
        scene.boxes.push(element("log_1"));
        scene.boxes[0].from_image = Some(1);
        scene.remove_reference(0);
        assert_eq!(scene.prose, "use <ref_image_0>");
        assert_eq!(scene.boxes[0].from_image, Some(0));
        assert_eq!(scene.source_image.as_ref().unwrap().path, "extra.png");
        assert!(scene.references.is_empty());
    }

    #[test]
    fn moving_a_reference_rewrites_every_affected_tag() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("a.png", 10, 10));
        scene.add_reference(ImageRef::new("b.png", 10, 10));
        scene.add_reference(ImageRef::new("c.png", 10, 10));
        scene.prose = "<ref_image_0> <ref_image_1> <ref_image_2>".into();
        scene.move_reference(2, 0);
        assert_eq!(scene.prose, "<ref_image_1> <ref_image_2> <ref_image_0>");
        assert_eq!(scene.source_image.as_ref().unwrap().path, "c.png");
        assert_eq!(scene.references[0].path, "a.png");
        assert_eq!(scene.references[1].path, "b.png");
    }

    #[test]
    fn frame_uses_the_source_or_a_virtual_ratio() {
        let mut scene = Scene::empty();
        scene.aspect_ratio = Some("auto".into());
        scene.boxes.push(element("box_1"));
        assert_eq!(scene.frame(), None);

        scene.boxes.clear();
        scene.aspect_ratio = Some("16:9".into());
        assert_eq!(
            scene.frame(),
            Some(crate::geometry::Size { w: 1000, h: 562 })
        );

        scene.set_source(ImageRef::new("src.png", 4000, 2250));
        assert_eq!(
            scene.frame(),
            Some(crate::geometry::Size { w: 4000, h: 2250 })
        );
    }

    #[test]
    fn set_source_and_clear_source_rescale_boxes_through_the_grid() {
        let mut scene = Scene::empty();
        scene.aspect_ratio = Some("16:9".into());
        let drawn = rect(100, 100, 300, 300);
        scene.add_box(drawn, BoxKind::Layout);
        scene.set_source(ImageRef::new("wide.png", 4000, 2250));
        assert_eq!(scene.boxes[0].kind, BoxKind::Layout);
        let wide = crate::geometry::Size { w: 4000, h: 2250 };
        let virtual_16_9 = crate::geometry::Size { w: 1000, h: 562 };
        let scaled = scene.boxes[0].src.unwrap();
        assert_eq!(scaled, grid_scaled(drawn, virtual_16_9, wide));
        assert!((scaled.x - 400).abs() <= 2 && (scaled.y - 400).abs() <= 2);
        assert!((scaled.w - 1200).abs() <= 2 && (scaled.h - 1200).abs() <= 2);

        let mut square_scene = Scene::empty();
        square_scene.aspect_ratio = Some("1:1".into());
        square_scene.set_source(ImageRef::new("sq.png", 2000, 2000));
        square_scene.add_reference(ImageRef::new("extra.png", 400, 200));
        let placed = rect(400, 400, 200, 200);
        square_scene.add_box(placed, BoxKind::Keep);
        let survivor = rect(10, 10, 20, 20);
        square_scene.boxes.push(BoxElement {
            id: BoxId::parse("box_2").unwrap(),
            kind: BoxKind::Keep,
            desc: String::new(),
            src: Some(survivor),
            tgt: Some(survivor),
            from_image: Some(1),
        });
        square_scene.clear_source();
        assert_eq!(
            square_scene.frame(),
            Some(crate::geometry::Size { w: 1000, h: 1000 })
        );
        assert_eq!(square_scene.boxes.len(), 1);
        assert_eq!(square_scene.boxes[0].from_image, Some(0));
        assert_eq!(square_scene.boxes[0].src, Some(survivor));
        assert_eq!(square_scene.boxes[0].tgt, Some(survivor));
    }

    #[test]
    fn scene_round_trip() {
        let scene = Scene {
            version: 1,
            source_image: Some(ImageRef::new("source.png", 0, 0)),
            references: vec![ImageRef::new("ref.png", 0, 0)],
            boxes: vec![BoxElement {
                id: BoxId::parse("animal_1").unwrap(),
                kind: BoxKind::Move,
                desc: "fox".into(),
                src: Some(Rect {
                    x: 1,
                    y: 2,
                    w: 3,
                    h: 4,
                }),
                tgt: None,
                from_image: Some(0),
            }],
            prose: "keep the trees".into(),
            aspect_ratio: Some("3:2".into()),
        };
        let json = serde_json::to_value(&scene).unwrap();
        let back: Scene = serde_json::from_value(json).unwrap();
        assert_eq!(scene, back);
    }

    #[test]
    fn promote_doubles_a_move_box_and_collapses_it() {
        let mut scene = Scene::empty();
        scene.boxes.push(BoxElement {
            id: BoxId::parse("box_1").unwrap(),
            kind: BoxKind::Move,
            desc: String::new(),
            src: Some(Rect {
                x: 100,
                y: 100,
                w: 200,
                h: 200,
            }),
            tgt: Some(Rect {
                x: 500,
                y: 500,
                w: 200,
                h: 200,
            }),
            from_image: Some(0),
        });
        scene.rescale_boxes_to_frame(Size { w: 1000, h: 1000 }, Size { w: 2000, h: 2000 });
        scene.collapse_move_boxes();
        let element = &scene.boxes[0];
        let expected = Rect {
            x: 1000,
            y: 1000,
            w: 400,
            h: 400,
        };
        assert_eq!(element.src, Some(expected));
        assert_eq!(element.tgt, Some(expected));
    }

    #[test]
    fn new_box_on_a_later_slot_records_that_slot() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("a.png", 100, 100));
        scene.add_reference(ImageRef::new("b.png", 400, 200));
        let id = scene.add_box_on(rect(10, 10, 20, 20), BoxKind::New, 1);
        let element = scene.boxes.iter().find(|element| element.id == id).unwrap();
        assert_eq!(element.from_image, Some(1));
        assert_eq!(element.tgt, Some(rect(10, 10, 20, 20)));
        assert!(element.src.is_none());

        let mut layout = Scene::empty();
        layout.aspect_ratio = Some("1:1".into());
        layout.add_box(rect(0, 0, 10, 10), BoxKind::Layout);
        assert_eq!(layout.boxes[0].from_image, None);
    }

    fn keep_on(scene: &mut Scene, id: &str, slot: usize, spot: Rect) {
        scene.boxes.push(BoxElement {
            id: BoxId::parse(id).unwrap(),
            kind: BoxKind::Keep,
            desc: String::new(),
            src: Some(spot),
            tgt: Some(spot),
            from_image: Some(slot),
        });
    }

    #[test]
    fn a_box_on_the_second_tile_survives_replacing_the_source() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("a.png", 100, 100));
        scene.add_reference(ImageRef::new("b.png", 400, 200));
        let spot = rect(10, 10, 10, 10);
        scene.add_box(spot, BoxKind::Keep);
        keep_on(&mut scene, "box_2", 1, spot);
        scene.set_source(ImageRef::new("c.png", 200, 200));
        let source_box = scene
            .boxes
            .iter()
            .find(|element| element.from_image == Some(0))
            .unwrap();
        let other = scene
            .boxes
            .iter()
            .find(|element| element.from_image == Some(1))
            .unwrap();
        assert_eq!(other.src, Some(spot));
        assert_eq!(
            source_box.src,
            Some(grid_scaled(
                spot,
                Size { w: 100, h: 100 },
                Size { w: 200, h: 200 },
            ))
        );
    }

    #[test]
    fn moving_the_source_keeps_both_rectangles() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("a.png", 100, 100));
        scene.add_reference(ImageRef::new("b.png", 400, 200));
        let spot = rect(10, 10, 10, 10);
        scene.add_box(spot, BoxKind::Keep);
        keep_on(&mut scene, "box_2", 1, spot);
        scene.move_reference(0, 1);
        let on_source = scene
            .boxes
            .iter()
            .find(|element| element.id.as_str() == "box_1")
            .unwrap();
        let on_extra = scene
            .boxes
            .iter()
            .find(|element| element.id.as_str() == "box_2")
            .unwrap();
        assert_eq!(on_source.src, Some(spot));
        assert_eq!(on_source.from_image, Some(1));
        assert_eq!(on_extra.src, Some(spot));
        assert_eq!(on_extra.from_image, Some(0));
    }

    #[test]
    fn removing_a_middle_tile_deletes_its_box_and_shifts_the_next() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("a.png", 100, 100));
        scene.add_reference(ImageRef::new("b.png", 100, 100));
        scene.add_reference(ImageRef::new("c.png", 100, 100));
        scene.prose = "use <ref_image_2>".into();
        let spot = rect(10, 10, 10, 10);
        keep_on(&mut scene, "gone_1", 1, spot);
        keep_on(&mut scene, "stay_1", 2, spot);
        scene.remove_reference(1);
        assert!(
            scene
                .boxes
                .iter()
                .all(|element| element.id.as_str() != "gone_1")
        );
        assert_eq!(scene.boxes.len(), 1);
        assert_eq!(scene.boxes[0].id.as_str(), "stay_1");
        assert_eq!(scene.boxes[0].from_image, Some(1));
        assert_eq!(scene.boxes[0].src, Some(spot));
        assert_eq!(scene.prose, "use <ref_image_1>");
    }

    #[test]
    fn collapse_move_boxes_leaves_a_move_on_another_tile() {
        let mut scene = Scene::empty();
        let src = rect(10, 10, 20, 20);
        let tgt = rect(40, 40, 20, 20);
        for (id, slot) in [("box_1", 0), ("box_2", 1)] {
            scene.boxes.push(BoxElement {
                id: BoxId::parse(id).unwrap(),
                kind: BoxKind::Move,
                desc: String::new(),
                src: Some(src),
                tgt: Some(tgt),
                from_image: Some(slot),
            });
        }
        scene.collapse_move_boxes();
        assert_eq!(scene.boxes[0].src, Some(tgt));
        assert_eq!(scene.boxes[0].tgt, Some(tgt));
        assert_eq!(scene.boxes[1].src, Some(src));
        assert_eq!(scene.boxes[1].tgt, Some(tgt));
    }
}

use crate::geometry::Rect;
use serde::{Deserialize, Serialize};

fn current_version() -> u32 {
    1
}

/// Project-relative path of an image file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRef {
    pub path: String,
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

#[cfg(test)]
mod tests {
    use super::{BoxElement, BoxId, BoxKind, ImageRef, Scene};
    use crate::geometry::Rect;

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

    #[test]
    fn scene_round_trip() {
        let scene = Scene {
            version: 1,
            source_image: Some(ImageRef {
                path: "source.png".into(),
            }),
            references: vec![ImageRef {
                path: "ref.png".into(),
            }],
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
}

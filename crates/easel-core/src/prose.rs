use crate::scene::{BoxId, Scene};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub span: (usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmatched {
    pub tags_without_box: Vec<String>,
    pub boxes_without_tag: Vec<BoxId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefChange {
    Removed(usize),
    Moved { from: usize, to: usize },
    InsertedAt(usize),
}

impl RefChange {
    #[must_use]
    pub fn map_index(self, index: usize) -> Option<usize> {
        match self {
            Self::Removed(removed) => {
                if index == removed {
                    None
                } else if index > removed {
                    Some(index - 1)
                } else {
                    Some(index)
                }
            }
            Self::Moved { from, to } => Some(moved_index(index, from, to)),
            Self::InsertedAt(at) => {
                if index >= at {
                    Some(index + 1)
                } else {
                    Some(index)
                }
            }
        }
    }
}

#[must_use]
pub fn tags(prose: &str) -> Vec<Tag> {
    let mut found = Vec::new();
    let mut index = 0;
    let bytes = prose.as_bytes();
    while index < bytes.len() {
        if bytes[index] == b'<'
            && let Some(relative) = prose[index + 1..].find('>')
        {
            let close = index + 1 + relative;
            let name = &prose[index + 1..close];
            if is_recognized(name) {
                found.push(Tag {
                    name: name.to_string(),
                    span: (index, close + 1),
                });
                index = close + 1;
                continue;
            }
        }
        index += prose[index..].chars().next().map_or(1, char::len_utf8);
    }
    found
}

#[must_use]
pub fn rename_tag(prose: &str, old: &str, new: &str) -> String {
    rewrite(prose, |name| {
        if name == old {
            Some(format!("<{new}>"))
        } else {
            None
        }
    })
}

#[must_use]
pub fn shift_ref_tags(prose: &str, change: RefChange) -> String {
    rewrite(prose, |name| {
        let index = ref_index(name)?;
        Some(match change.map_index(index) {
            Some(next) => format!("<ref_image_{next}>"),
            None => String::new(),
        })
    })
}

#[must_use]
pub fn unmatched(prose: &str, scene: &Scene) -> Unmatched {
    let found = tags(prose);
    let mut tags_without_box = Vec::new();
    for tag in &found {
        if ref_index(&tag.name).is_some() {
            continue;
        }
        if !scene
            .boxes
            .iter()
            .any(|element| element.id.as_str() == tag.name)
        {
            tags_without_box.push(tag.name.clone());
        }
    }
    let boxes_without_tag = scene
        .boxes
        .iter()
        .filter(|element| !found.iter().any(|tag| tag.name == element.id.as_str()))
        .map(|element| element.id.clone())
        .collect();
    Unmatched {
        tags_without_box,
        boxes_without_tag,
    }
}

fn rewrite(prose: &str, mut replace: impl FnMut(&str) -> Option<String>) -> String {
    let found = tags(prose);
    let mut out = String::new();
    let mut cursor = 0;
    for tag in found {
        out.push_str(&prose[cursor..tag.span.0]);
        match replace(&tag.name) {
            Some(replacement) => out.push_str(&replacement),
            None => out.push_str(&prose[tag.span.0..tag.span.1]),
        }
        cursor = tag.span.1;
    }
    out.push_str(&prose[cursor..]);
    out
}

fn is_recognized(name: &str) -> bool {
    ref_index(name).is_some() || is_box_tag(name)
}

fn ref_index(name: &str) -> Option<usize> {
    let rest = name.strip_prefix("ref_image_")?;
    if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

fn is_box_tag(name: &str) -> bool {
    let Some((prefix, suffix)) = name.split_once('_') else {
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

fn moved_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < to {
        if index > from && index <= to {
            index - 1
        } else {
            index
        }
    } else if index >= to && index < from {
        index + 1
    } else {
        index
    }
}

#[cfg(test)]
mod tests {
    use super::{RefChange, Tag, tags, unmatched};
    use crate::scene::{BoxElement, BoxId, BoxKind, Scene};

    fn scene_with(prose: &str, ids: &[&str]) -> Scene {
        let mut scene = Scene::empty();
        scene.prose = prose.to_string();
        for id in ids {
            scene.boxes.push(BoxElement {
                id: BoxId::parse(id).unwrap(),
                kind: BoxKind::Layout,
                desc: String::new(),
                src: None,
                tgt: None,
                from_image: None,
            });
        }
        scene
    }

    #[test]
    fn tags_find_box_ids_and_reference_images() {
        let found = tags("make <cat_1> orange and use <ref_image_1>");
        assert_eq!(
            found,
            vec![
                Tag {
                    name: "cat_1".into(),
                    span: (5, 12),
                },
                Tag {
                    name: "ref_image_1".into(),
                    span: (28, 41),
                },
            ]
        );
    }

    #[test]
    fn unmatched_splits_orphan_tags_from_untagged_boxes() {
        let scene = scene_with("see <dog_1> and <cat_1>", &["cat_1", "log_1"]);
        let report = unmatched(&scene.prose, &scene);
        assert_eq!(report.tags_without_box, vec!["dog_1".to_string()]);
        assert_eq!(
            report
                .boxes_without_tag
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec!["log_1"]
        );
    }

    #[test]
    fn rename_and_reference_shifts_rewrite_spans() {
        assert_eq!(
            super::rename_tag("make <cat_1> orange", "cat_1", "cat_2"),
            "make <cat_2> orange"
        );
        assert_eq!(
            super::shift_ref_tags(
                "keep <ref_image_1> and <ref_image_0>",
                RefChange::Removed(0)
            ),
            "keep <ref_image_0> and "
        );
        let moved = super::shift_ref_tags(
            "<ref_image_0> <ref_image_1> <ref_image_2>",
            RefChange::Moved { from: 2, to: 0 },
        );
        assert_eq!(moved, "<ref_image_1> <ref_image_2> <ref_image_0>");
    }
}

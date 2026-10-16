use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use easel_core::session::{ReadSession, SessionFile, SessionStore, own_image};
use gpui::{App, Context, Entity};

use crate::app_state::AppState;
use crate::image_load::{self, DecodedImage};

pub struct Session {
    app_state: Entity<AppState>,
    path: PathBuf,
    image_dir: PathBuf,
    armed: bool,
    dirty: bool,
    saves: u32,
}

impl Session {
    pub fn attach(
        app_state: Entity<AppState>,
        path: PathBuf,
        image_dir: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let watched = app_state.clone();
        cx.observe(&watched, |this, _, cx| this.note(cx)).detach();
        Self {
            app_state,
            path,
            image_dir,
            armed: false,
            dirty: false,
            saves: 0,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn save_count(&self) -> u32 {
        self.saves
    }

    fn note(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        if self.armed {
            return;
        }
        self.armed = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            this.update(cx, |this, cx| this.flush(cx)).ok();
        })
        .detach();
    }

    fn flush(&mut self, cx: &mut Context<Self>) {
        self.armed = false;
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.saves += 1;
        let app = self.app_state.read(cx);
        let scene = app.scene.clone();
        let values = app.values.clone();
        let source = app.source_ref.clone();
        let image = app
            .image
            .as_ref()
            .map(|image| Arc::clone(&image.rgba_for_export));
        let path = self.path.clone();
        let image_dir = self.image_dir.clone();
        let state = self.app_state.clone();
        cx.spawn(async move |_this, cx| {
            let owned = if source.is_none() {
                if let Some(image) = image {
                    cx.background_executor()
                        .spawn(async move { own_image(&image_dir, &image) })
                        .await
                        .ok()
                } else {
                    None
                }
            } else {
                None
            };
            let source = owned.clone().or(source);
            let file = SessionFile {
                version: easel_core::session::VERSION,
                scene,
                values,
                source: source.clone(),
                references: Vec::new(),
            };
            let write = cx
                .background_executor()
                .spawn(async move { SessionStore::open(path).write(&file) })
                .await;
            if write.is_err() {
                return;
            }
            if let Some(owned) = owned {
                state.update(cx, |app, _| {
                    if app.source_ref.is_none() {
                        app.source_ref = Some(owned);
                    }
                });
            }
        })
        .detach();
    }
}

pub fn restore(state: Entity<AppState>, path: PathBuf, cx: &mut App) {
    cx.spawn(async move |cx| {
        let loaded = cx
            .background_executor()
            .spawn(async move { load_restored(&path) })
            .await;
        let Some((file, image)) = loaded else {
            return;
        };
        cx.update(|cx| {
            state.update(cx, |app, cx| {
                app.scene = file.scene;
                app.values = file.values;
                app.source_ref = file.source;
                app.image = image;
                if app.image.is_none() {
                    app.scene.source_image = None;
                    app.source_ref = None;
                }
                cx.notify();
            });
        });
    })
    .detach();
}

fn load_restored(path: &std::path::Path) -> Option<(SessionFile, Option<Arc<DecodedImage>>)> {
    let ReadSession::Loaded { file, .. } = SessionStore::open(path).read().ok()? else {
        return None;
    };
    let file = *file;
    let image = file.source.as_ref().and_then(|source| {
        let bytes = std::fs::read(&source.path).ok()?;
        image_load::decode_image(&bytes, "session").ok()
    });
    let image = image.map(Arc::new);
    if file.source.is_some() && image.is_none() {
        let mut file = file;
        file.scene.source_image = None;
        file.source = None;
        return Some((file, None));
    }
    Some((file, image))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use easel_core::geometry::Rect;
    use easel_core::history::{Completion, HistoryStore};
    use easel_core::scene::BoxKind;
    use easel_core::session::{SessionFile, SessionStore, own_image};
    use gpui::{TestAppContext, prelude::*};
    use image::{DynamicImage, Rgb, RgbImage};
    use serde_json::json;

    use super::{Session, restore};
    use crate::app_state::AppState;
    use crate::jobs::promote;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "easel-app-session-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn png() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/pixel.png"
        ))
        .unwrap()
    }

    #[gpui::test]
    fn an_oversized_owned_image_is_dropped_and_the_rest_restores(cx: &mut TestAppContext) {
        let root = temp_dir("huge");
        let image = root.join("huge.png");
        std::fs::write(&image, huge_png()).unwrap();
        let mut scene = easel_core::scene::Scene::empty();
        scene.prose = "kept".into();
        let file = SessionFile {
            version: 1,
            scene,
            values: [("resolution".into(), json!("2k"))].into(),
            source: Some(easel_core::session::SourceRef {
                path: image.display().to_string(),
                hash: "unused".into(),
                owned: true,
            }),
            references: Vec::new(),
        };
        SessionStore::open(root.join("session.json"))
            .write(&file)
            .unwrap();
        let state = cx.new(|_| AppState::new());
        cx.update(|cx| restore(state.clone(), root.join("session.json"), cx));
        cx.run_until_parked();
        let (prose, resolution, image) = state.read_with(cx, |app, _| {
            (
                app.scene.prose.clone(),
                app.values.get("resolution").cloned(),
                app.image.is_some(),
            )
        });
        assert_eq!(prose, "kept");
        assert_eq!(resolution, Some(json!("2k")));
        assert!(!image);
    }

    #[gpui::test]
    fn three_rapid_changes_save_once_without_rewriting_pixels(cx: &mut TestAppContext) {
        let root = temp_dir("debounce");
        let dir = root.join("session");
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 4, Rgb([1, 1, 1])));
        let owned = own_image(&dir, &image).unwrap();
        let png_path = std::path::PathBuf::from(&owned.path);
        let before = std::fs::metadata(&png_path).unwrap().modified().unwrap();
        let state = cx.new(|_| AppState::new());
        state.update(cx, |app, _| {
            app.source_ref = Some(owned);
        });
        let session =
            cx.new(|cx| Session::attach(state.clone(), root.join("session.json"), dir, cx));
        for prose in ["one", "two", "three"] {
            state.update(cx, |app, cx| {
                app.scene.prose = prose.into();
                cx.notify();
            });
        }
        assert_eq!(session.read_with(cx, |session, _| session.save_count()), 0);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(session.read_with(cx, |session, _| session.save_count()), 1);
        let after = std::fs::metadata(&png_path).unwrap().modified().unwrap();
        assert_eq!(before, after);
        let text = std::fs::read_to_string(root.join("session.json")).unwrap();
        assert!(text.contains("three"), "{text}");
    }

    #[gpui::test]
    fn a_pasted_image_restores_with_the_same_box(cx: &mut TestAppContext) {
        let root = temp_dir("paste");
        let dir = root.join("session");
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(5000, 4000, Rgb([4, 5, 6])));
        let owned = own_image(&dir, &image).unwrap();
        let mut scene = easel_core::scene::Scene::empty();
        scene.set_source(easel_core::scene::ImageRef::new(
            owned.path.clone(),
            5000,
            4000,
        ));
        scene.add_box(
            Rect {
                x: 4000,
                y: 3000,
                w: 500,
                h: 500,
            },
            BoxKind::New,
        );
        let file = SessionFile {
            version: 1,
            scene,
            values: [("aspect_ratio".into(), json!("auto"))].into(),
            source: Some(owned),
            references: Vec::new(),
        };
        SessionStore::open(root.join("session.json"))
            .write(&file)
            .unwrap();
        let state = cx.new(|_| AppState::new());
        cx.update(|cx| restore(state.clone(), root.join("session.json"), cx));
        cx.run_until_parked();
        let (width, height, rect) = state.read_with(cx, |app, _| {
            let image = app.image.as_ref().unwrap();
            (image.size.w, image.size.h, app.scene.boxes[0].tgt)
        });
        assert_eq!((width, height), (5000, 4000));
        assert_eq!(
            rect,
            Some(Rect {
                x: 4000,
                y: 3000,
                w: 500,
                h: 500,
            })
        );
    }

    #[gpui::test]
    fn a_promoted_image_survives_discarding_its_history_entry(cx: &mut TestAppContext) {
        let root = temp_dir("promote");
        let history = Arc::new(HistoryStore::open(root.join("history")).unwrap());
        let state = cx.new(|_| AppState::new());
        let session = cx.new(|cx| {
            Session::attach(
                state.clone(),
                root.join("session.json"),
                root.join("session"),
                cx,
            )
        });
        state.update(cx, |app, cx| {
            app.scene.add_box(
                Rect {
                    x: 1,
                    y: 2,
                    w: 3,
                    h: 4,
                },
                BoxKind::Layout,
            );
            promote(app, &png(), cx).unwrap();
        });
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(session.read_with(cx, |session, _| session.save_count()), 1);
        let id = easel_core::providers::JobId::parse("job1").unwrap();
        history
            .begin(
                &id,
                &easel_core::job::JobRequest {
                    model_id: "flux-3-image".into(),
                    compiled_prompt: "a red square".into(),
                    params: std::collections::BTreeMap::new(),
                    images: Vec::new(),
                    box_snapshot: Vec::new(),
                    created_at: "2026-10-15T00:00:00Z".into(),
                },
                &easel_core::providers::Submitted {
                    job_id: id.clone(),
                    poll_url: "https://api.bfl.ai/v1/get_result?id=job1".parse().unwrap(),
                    quote: None,
                },
            )
            .unwrap();
        history
            .complete(
                &id,
                &png(),
                "image/png",
                &png(),
                &Completion {
                    cost: None,
                    prompt: None,
                    duration: None,
                },
            )
            .unwrap();
        history.remove(&id).unwrap();
        let restored = cx.new(|_| AppState::new());
        cx.update(|cx| restore(restored.clone(), root.join("session.json"), cx));
        cx.run_until_parked();
        let (has_image, boxes) =
            restored.read_with(cx, |app, _| (app.image.is_some(), app.scene.boxes.len()));
        assert!(has_image);
        assert_eq!(boxes, 1);
    }

    fn huge_png() -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&100_000u32.to_be_bytes());
        bytes.extend_from_slice(&100_000u32.to_be_bytes());
        bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
        bytes
    }
}

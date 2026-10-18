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
        let model_id = app.model_id.clone();
        let source_ref = app.source_ref.clone();
        let source_image = app
            .image
            .as_ref()
            .map(|image| Arc::clone(&image.rgba_for_export));
        let extras: Vec<_> = app
            .extras
            .iter()
            .enumerate()
            .map(|(index, image)| {
                (
                    app.extra_refs.get(index).cloned().flatten(),
                    Arc::clone(&image.rgba_for_export),
                )
            })
            .collect();
        let path = self.path.clone();
        let image_dir = self.image_dir.clone();
        let state = self.app_state.clone();
        cx.spawn(async move |_this, cx| {
            let mut source = source_ref;
            if source.is_none()
                && let Some(image) = source_image
            {
                let dir = image_dir.clone();
                source = cx
                    .background_executor()
                    .spawn(async move { own_image(&dir, &image) })
                    .await
                    .ok();
            }
            let mut references = Vec::new();
            let mut owned_extras = Vec::new();
            for (reference, image) in extras {
                if let Some(reference) = reference {
                    references.push(reference);
                    continue;
                }
                let dir = image_dir.clone();
                if let Ok(owned) = cx
                    .background_executor()
                    .spawn(async move { own_image(&dir, &image) })
                    .await
                {
                    owned_extras.push(owned.clone());
                    references.push(owned);
                }
            }
            let file = SessionFile {
                version: easel_core::session::VERSION,
                scene,
                values,
                model_id,
                source: source.clone(),
                references,
            };
            let write = cx
                .background_executor()
                .spawn(async move { SessionStore::open(path).write(&file) })
                .await;
            if write.is_err() {
                return;
            }
            state.update(cx, |app, _| {
                if app.source_ref.is_none()
                    && let Some(source) = source
                {
                    app.source_ref = Some(source);
                }
                if app.extra_refs.len() < app.extras.len() {
                    app.extra_refs.resize(app.extras.len(), None);
                }
                let mut owned = owned_extras.into_iter();
                for reference in &mut app.extra_refs {
                    if reference.is_none() {
                        *reference = owned.next();
                    }
                }
            });
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
        let Some((file, mut images)) = loaded else {
            return;
        };
        cx.update(|cx| {
            state.update(cx, |app, cx| {
                if easel_core::providers::ModelRegistry::specs()
                    .iter()
                    .any(|spec| spec.id == file.model_id)
                {
                    app.model_id = file.model_id.clone();
                    app.remember_model();
                }
                app.scene = file.scene;
                app.values = app.reconcile_values(file.values);
                let mut refs = Vec::new();
                if file.source.is_some() || app.scene.source_image.is_some() {
                    refs.push(file.source);
                }
                refs.extend(file.references.into_iter().map(Some));
                while refs.len() < images.len() {
                    refs.push(None);
                }
                for index in (0..images.len()).rev() {
                    if images[index].is_none() {
                        app.scene.remove_reference(index);
                        images.remove(index);
                        refs.remove(index);
                    }
                }
                app.image = None;
                app.extras.clear();
                app.source_ref = None;
                app.extra_refs.clear();
                if app.scene.source_image.is_some() {
                    app.image = images.first().cloned().flatten();
                    app.source_ref = refs.first().cloned().flatten();
                    app.extras = images.into_iter().skip(1).flatten().collect();
                    app.extra_refs = refs.into_iter().skip(1).collect();
                }
                cx.notify();
            });
        });
    })
    .detach();
}

fn load_restored(path: &std::path::Path) -> Option<(SessionFile, Vec<Option<Arc<DecodedImage>>>)> {
    let ReadSession::Loaded { file, .. } = SessionStore::open(path).read().ok()? else {
        return None;
    };
    let file = *file;
    let mut images = Vec::new();
    if file.source.is_some() || file.scene.source_image.is_some() {
        images.push(decode_ref(file.source.as_ref()));
    }
    for reference in &file.references {
        images.push(decode_ref(Some(reference)));
    }
    Some((file, images))
}

fn decode_ref(source: Option<&easel_core::session::SourceRef>) -> Option<Arc<DecodedImage>> {
    let source = source?;
    let bytes = std::fs::read(&source.path).ok()?;
    image_load::decode_image(&bytes, "session")
        .ok()
        .map(Arc::new)
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
            model_id: String::new(),
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
        assert_eq!(
            state.read_with(cx, |app, _| app.values.get("safety_tolerance").cloned()),
            Some(json!(2))
        );
        assert_eq!(
            state.read_with(cx, |app, _| app.values.get("grounding").cloned()),
            Some(json!(true))
        );
    }

    #[gpui::test]
    fn restore_puts_the_saved_prompt_in_the_editor(cx: &mut TestAppContext) {
        let root = temp_dir("prompt");
        let mut scene = easel_core::scene::Scene::empty();
        scene.prose = "a harbor at dusk".into();
        let file = SessionFile {
            version: 1,
            scene,
            values: [
                ("aspect_ratio".into(), json!("nope")),
                ("resolution".into(), json!("1k")),
            ]
            .into(),
            model_id: String::new(),
            source: None,
            references: Vec::new(),
        };
        SessionStore::open(root.join("session.json"))
            .write(&file)
            .unwrap();
        let state = cx.new(|_| AppState::new());
        let editor = cx.new(|cx| crate::side_panel::PromptEditor::new(state.clone(), cx));
        cx.update(|cx| restore(state.clone(), root.join("session.json"), cx));
        cx.run_until_parked();
        assert_eq!(
            editor.read_with(cx, |editor, cx| editor.prose(cx)),
            "a harbor at dusk"
        );
        let keys: std::sync::Arc<dyn crate::net::KeyResolver> = std::sync::Arc::new(
            crate::net::MapKeys::default().insert(easel_core::model_spec::ProviderId::Bfl, "key"),
        );
        let validation = cx.new(|cx| crate::side_panel::Validation::new(state.clone(), keys, cx));
        let messages = validation.read_with(cx, |validation, cx| validation.messages(cx));
        assert!(
            messages.iter().all(|message| {
                !message.contains("required") && !message.contains("not one of the options")
            }),
            "{messages:?}"
        );
        assert_eq!(
            state.read_with(cx, |app, _| app.values.get("aspect_ratio").cloned()),
            Some(json!("auto"))
        );
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
            model_id: String::new(),
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
                None,
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

    fn decoded_color(
        color: [u8; 3],
        width: u32,
        height: u32,
    ) -> Arc<crate::image_load::DecodedImage> {
        let dynamic = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb(color)));
        let mut bytes = Vec::new();
        dynamic
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        Arc::new(crate::image_load::decode_image(&bytes, "tile").unwrap())
    }

    fn flush_session(
        cx: &mut TestAppContext,
        state: &gpui::Entity<AppState>,
        root: &std::path::Path,
    ) {
        let _session = cx.new(|cx| {
            Session::attach(
                state.clone(),
                root.join("session.json"),
                root.join("session"),
                cx,
            )
        });
        state.update(cx, |app, cx| {
            app.scene.prose.push(' ');
            cx.notify();
        });
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(500));
        cx.run_until_parked();
    }

    #[gpui::test]
    fn relaunch_restores_the_source_and_the_extra(cx: &mut TestAppContext) {
        let root = temp_dir("two");
        let state = cx.new(|_| AppState::new());
        state.update(cx, |app, cx| {
            app.set_image(decoded_color([1, 2, 3], 20, 10), cx);
            app.add_extra(decoded_color([4, 5, 6], 30, 12), cx).unwrap();
            let spot = Rect {
                x: 1,
                y: 2,
                w: 3,
                h: 4,
            };
            app.scene.add_box(spot, BoxKind::Keep);
            app.scene.add_box_on(spot, BoxKind::Keep, 1);
        });
        flush_session(cx, &state, &root);
        let restored = cx.new(|_| AppState::new());
        cx.update(|cx| restore(restored.clone(), root.join("session.json"), cx));
        cx.run_until_parked();
        let (source, extra, indexes) = restored.read_with(cx, |app, _| {
            (
                app.image.as_ref().unwrap().size,
                app.extras[0].size,
                app.scene
                    .boxes
                    .iter()
                    .map(|element| element.from_image)
                    .collect::<Vec<_>>(),
            )
        });
        assert_eq!(source, easel_core::geometry::Size { w: 20, h: 10 });
        assert_eq!(extra, easel_core::geometry::Size { w: 30, h: 12 });
        assert_eq!(indexes, [Some(0), Some(1)]);
    }

    #[gpui::test]
    fn a_missing_extra_drops_that_slot_and_shifts_the_next_box(cx: &mut TestAppContext) {
        let root = temp_dir("missing-extra");
        let dir = root.join("session");
        let source = own_image(
            &dir,
            &DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([1, 1, 1]))),
        )
        .unwrap();
        let gone = own_image(
            &dir,
            &DynamicImage::ImageRgb8(RgbImage::from_pixel(9, 9, Rgb([2, 2, 2]))),
        )
        .unwrap();
        let kept = own_image(
            &dir,
            &DynamicImage::ImageRgb8(RgbImage::from_pixel(10, 10, Rgb([3, 3, 3]))),
        )
        .unwrap();
        std::fs::remove_file(&gone.path).unwrap();
        let mut scene = easel_core::scene::Scene::empty();
        scene.set_source(easel_core::scene::ImageRef::new(&source.path, 8, 8));
        scene.add_reference(easel_core::scene::ImageRef::new(&gone.path, 9, 9));
        scene.add_reference(easel_core::scene::ImageRef::new(&kept.path, 10, 10));
        scene.boxes.push(easel_core::scene::BoxElement {
            id: easel_core::scene::BoxId::parse("box_1").unwrap(),
            kind: BoxKind::Keep,
            desc: String::new(),
            src: Some(Rect {
                x: 1,
                y: 1,
                w: 2,
                h: 2,
            }),
            tgt: Some(Rect {
                x: 1,
                y: 1,
                w: 2,
                h: 2,
            }),
            from_image: Some(2),
        });
        SessionStore::open(root.join("session.json"))
            .write(&SessionFile {
                version: 1,
                scene,
                values: Default::default(),
                model_id: String::new(),
                source: Some(source),
                references: vec![gone, kept],
            })
            .unwrap();
        let state = cx.new(|_| AppState::new());
        cx.update(|cx| restore(state.clone(), root.join("session.json"), cx));
        cx.run_until_parked();
        let (extras, from_image) = state.read_with(cx, |app, _| {
            (app.extras.len(), app.scene.boxes[0].from_image)
        });
        assert_eq!(extras, 1);
        assert_eq!(from_image, Some(1));
    }

    #[gpui::test]
    fn flush_keeps_an_existing_source_file_and_owns_a_new_extra(cx: &mut TestAppContext) {
        let root = temp_dir("keep-source");
        let dir = root.join("session");
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([1, 2, 3])));
        let owned = own_image(&dir, &image).unwrap();
        let before = std::fs::metadata(&owned.path).unwrap().modified().unwrap();
        let state = cx.new(|_| AppState::new());
        let source_bytes = std::fs::read(&owned.path).unwrap();
        let decoded = Arc::new(crate::image_load::decode_image(&source_bytes, "source").unwrap());
        state.update(cx, |app, cx| {
            app.set_image(decoded, cx);
            app.source_ref = Some(owned.clone());
            app.add_extra(decoded_color([7, 7, 7], 12, 12), cx).unwrap();
        });
        flush_session(cx, &state, &root);
        let after = std::fs::metadata(&owned.path).unwrap().modified().unwrap();
        assert_eq!(before, after);
        let file = match SessionStore::open(root.join("session.json"))
            .read()
            .unwrap()
        {
            easel_core::session::ReadSession::Loaded { file, .. } => file,
            other => panic!("{other:?}"),
        };
        assert_eq!(file.source.as_ref().unwrap().path, owned.path);
        assert_eq!(file.references.len(), 1);
        assert!(std::path::Path::new(&file.references[0].path).exists());
    }

    #[gpui::test]
    fn a_moved_source_restores_in_the_swapped_order(cx: &mut TestAppContext) {
        let root = temp_dir("swapped");
        let dir = root.join("session");
        let left = own_image(
            &dir,
            &DynamicImage::ImageRgb8(RgbImage::from_pixel(11, 11, Rgb([1, 0, 0]))),
        )
        .unwrap();
        let right = own_image(
            &dir,
            &DynamicImage::ImageRgb8(RgbImage::from_pixel(13, 13, Rgb([0, 0, 1]))),
        )
        .unwrap();
        let state = cx.new(|_| AppState::new());
        let left_bytes = std::fs::read(&left.path).unwrap();
        let right_bytes = std::fs::read(&right.path).unwrap();
        state.update(cx, |app, cx| {
            app.set_image(
                Arc::new(crate::image_load::decode_image(&left_bytes, "left").unwrap()),
                cx,
            );
            app.source_ref = Some(left.clone());
            app.add_extra(
                Arc::new(crate::image_load::decode_image(&right_bytes, "right").unwrap()),
                cx,
            )
            .unwrap();
            app.extra_refs.push(Some(right.clone()));
            app.move_slot(0, 1, cx);
        });
        flush_session(cx, &state, &root);
        let restored = cx.new(|_| AppState::new());
        cx.update(|cx| restore(restored.clone(), root.join("session.json"), cx));
        cx.run_until_parked();
        let (source, extra) = restored.read_with(cx, |app, _| {
            (
                app.source_ref.as_ref().unwrap().path.clone(),
                app.extra_refs[0].as_ref().unwrap().path.clone(),
            )
        });
        assert_eq!(source, right.path);
        assert_eq!(extra, left.path);
    }

    #[gpui::test]
    fn a_missing_source_promotes_the_extra(cx: &mut TestAppContext) {
        let root = temp_dir("missing-source");
        let dir = root.join("session");
        let source = own_image(
            &dir,
            &DynamicImage::ImageRgb8(RgbImage::from_pixel(6, 6, Rgb([1, 1, 1]))),
        )
        .unwrap();
        let extra = own_image(
            &dir,
            &DynamicImage::ImageRgb8(RgbImage::from_pixel(7, 7, Rgb([2, 2, 2]))),
        )
        .unwrap();
        std::fs::remove_file(&source.path).unwrap();
        let mut scene = easel_core::scene::Scene::empty();
        scene.set_source(easel_core::scene::ImageRef::new(&source.path, 6, 6));
        scene.add_reference(easel_core::scene::ImageRef::new(&extra.path, 7, 7));
        SessionStore::open(root.join("session.json"))
            .write(&SessionFile {
                version: 1,
                scene,
                values: Default::default(),
                model_id: String::new(),
                source: Some(source),
                references: vec![extra.clone()],
            })
            .unwrap();
        let state = cx.new(|_| AppState::new());
        cx.update(|cx| restore(state.clone(), root.join("session.json"), cx));
        cx.run_until_parked();
        let (size, refs) = state.read_with(cx, |app, _| {
            (app.image.as_ref().unwrap().size, app.scene.references.len())
        });
        assert_eq!(size, easel_core::geometry::Size { w: 7, h: 7 });
        assert_eq!(refs, 0);
        assert_eq!(
            state.read_with(cx, |app, _| app.source_ref.as_ref().unwrap().path.clone()),
            extra.path
        );
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

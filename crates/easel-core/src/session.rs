use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use image::DynamicImage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::image_prep::encode_png;
use crate::scene::Scene;

pub const VERSION: u32 = 1;
const MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionFile {
    pub version: u32,
    pub scene: Scene,
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub source: Option<SourceRef>,
    #[serde(default)]
    pub references: Vec<SourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub path: String,
    pub hash: String,
    pub owned: bool,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session version {0} is newer than this app")]
    Version(u32),
    #[error("session file is larger than 256 MB")]
    TooLarge,
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Parse(String),
}

#[derive(Debug)]
pub enum ReadSession {
    Missing,
    Loaded {
        file: Box<SessionFile>,
        warnings: Vec<String>,
    },
}

pub struct SessionStore {
    path: PathBuf,
    fail_rename: AtomicBool,
}

impl SessionStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            fail_rename: AtomicBool::new(false),
        }
    }

    pub fn fail_next_rename(&self) {
        self.fail_rename.store(true, Ordering::SeqCst);
    }

    pub fn read(&self) -> Result<ReadSession, SessionError> {
        if !self.path.exists() {
            return Ok(ReadSession::Missing);
        }
        let len = fs::metadata(&self.path).map_err(io)?.len();
        if len > MAX_BYTES {
            return Err(SessionError::TooLarge);
        }
        let bytes = fs::read(&self.path).map_err(io)?;
        match serde_json::from_slice::<SessionFile>(&bytes) {
            Ok(file) => self.accept(file),
            Err(error) => self.restore_backup(error.to_string()),
        }
    }

    pub fn write(&self, file: &SessionFile) -> Result<(), SessionError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        if self.path.exists() {
            let previous = fs::read(&self.path).map_err(io)?;
            write_private(&backup_path(&self.path), &previous)?;
        }
        let bytes =
            serde_json::to_vec_pretty(file).map_err(|err| SessionError::Parse(err.to_string()))?;
        let tmp = temp_path(&self.path);
        write_private(&tmp, &bytes)?;
        if self.fail_rename.swap(false, Ordering::SeqCst) {
            return Err(SessionError::Io("rename failed".into()));
        }
        fs::rename(&tmp, &self.path).map_err(io)?;
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600)).map_err(io)?;
        self.remove_unreferenced(file)?;
        Ok(())
    }

    fn accept(&self, file: SessionFile) -> Result<ReadSession, SessionError> {
        if file.version > VERSION {
            return Err(SessionError::Version(file.version));
        }
        let mut warnings = Vec::new();
        warn_hash(file.source.as_ref(), &mut warnings);
        for reference in &file.references {
            warn_hash(Some(reference), &mut warnings);
        }
        Ok(ReadSession::Loaded {
            file: Box::new(file),
            warnings,
        })
    }

    fn restore_backup(&self, detail: String) -> Result<ReadSession, SessionError> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let corrupt = self
            .path
            .with_file_name(format!("session.corrupt-{stamp}.json"));
        fs::rename(&self.path, &corrupt).map_err(io)?;
        let bak = backup_path(&self.path);
        if !bak.exists() {
            return Err(SessionError::Parse(detail));
        }
        let bytes = fs::read(&bak).map_err(io)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(SessionError::TooLarge);
        }
        let file = serde_json::from_slice::<SessionFile>(&bytes)
            .map_err(|err| SessionError::Parse(err.to_string()))?;
        self.accept(file)
    }

    fn remove_unreferenced(&self, file: &SessionFile) -> Result<(), SessionError> {
        let Some(parent) = self.path.parent() else {
            return Ok(());
        };
        let dir = parent.join("session");
        if !dir.exists() {
            return Ok(());
        }
        let mut keep = Vec::new();
        if let Some(source) = file.source.as_ref().filter(|source| source.owned) {
            keep.push(PathBuf::from(&source.path));
        }
        for reference in file.references.iter().filter(|reference| reference.owned) {
            keep.push(PathBuf::from(&reference.path));
        }
        for entry in fs::read_dir(&dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            if !entry.file_type().map_err(io)?.is_file() {
                continue;
            }
            let path = entry.path();
            if keep.iter().any(|kept| same_file(&path, kept)) {
                continue;
            }
            fs::remove_file(path).map_err(io)?;
        }
        Ok(())
    }
}

#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub fn own_image(dir: &Path, image: &DynamicImage) -> Result<SourceRef, SessionError> {
    fs::create_dir_all(dir).map_err(io)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(io)?;
    let bytes = encode_png(image);
    let hash = content_hash(&bytes);
    let path = dir.join(format!("{hash}.png"));
    if !path.exists() {
        write_private(&path, &bytes)?;
    }
    Ok(SourceRef {
        path: path.display().to_string(),
        hash,
        owned: true,
    })
}

fn warn_hash(source: Option<&SourceRef>, warnings: &mut Vec<String>) {
    let Some(source) = source else {
        return;
    };
    let Ok(bytes) = fs::read(&source.path) else {
        return;
    };
    let hash = content_hash(&bytes);
    if hash != source.hash {
        warnings.push(format!("{} changed since it was saved", source.path));
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session.json");
    path.with_file_name(format!("{name}.bak"))
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session.json");
    path.with_file_name(format!("{name}.tmp"))
}

fn same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), SessionError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(io)?;
    Ok(())
}

fn io(error: impl std::fmt::Display) -> SessionError {
    SessionError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use image::{DynamicImage, Rgb, RgbImage};
    use serde_json::json;

    use super::{ReadSession, SessionError, SessionFile, SessionStore, SourceRef, own_image};
    use crate::geometry::Rect;
    use crate::scene::{BoxElement, BoxId, BoxKind, Scene};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "easel-session-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn sample() -> SessionFile {
        let mut scene = Scene::empty();
        scene.prose = "a red square".into();
        scene.aspect_ratio = Some("1:1".into());
        scene.boxes.push(BoxElement {
            id: BoxId::parse("box_1").unwrap(),
            kind: BoxKind::Layout,
            desc: "square".into(),
            src: Some(Rect {
                x: 1,
                y: 2,
                w: 3,
                h: 4,
            }),
            tgt: None,
            from_image: None,
        });
        SessionFile {
            version: 1,
            scene,
            values: [("resolution".into(), json!("1k"))].into(),
            model_id: String::new(),
            source: Some(SourceRef {
                path: "source.png".into(),
                hash: "abc".into(),
                owned: false,
            }),
            references: Vec::new(),
        }
    }

    #[test]
    fn a_session_round_trips_and_a_newer_version_is_rejected() {
        let root = temp_dir("round");
        let path = root.join("session.json");
        let store = SessionStore::open(&path);
        let file = sample();
        store.write(&file).unwrap();
        let ReadSession::Loaded {
            file: loaded,
            warnings,
        } = store.read().unwrap()
        else {
            panic!("missing");
        };
        assert_eq!(*loaded, file);
        assert!(warnings.is_empty());
        let mut newer = file;
        newer.version = 99;
        store.write(&newer).unwrap();
        let error = store.read().unwrap_err();
        assert!(matches!(error, SessionError::Version(99)), "{error}");
    }

    #[test]
    fn a_changed_file_still_loads_with_a_hash_warning() {
        let root = temp_dir("hash");
        let image = root.join("source.png");
        fs::write(&image, b"png-bytes").unwrap();
        let hash = blake3::hash(b"png-bytes").to_hex().to_string();
        let mut file = sample();
        file.source = Some(SourceRef {
            path: image.display().to_string(),
            hash,
            owned: false,
        });
        let store = SessionStore::open(root.join("session.json"));
        store.write(&file).unwrap();
        fs::write(&image, b"png-bytes-changed").unwrap();
        let ReadSession::Loaded {
            file: loaded,
            warnings,
        } = store.read().unwrap()
        else {
            panic!("missing");
        };
        assert_eq!(
            loaded.source.as_ref().unwrap().path,
            image.display().to_string()
        );
        assert!(!warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn a_bad_file_restores_the_backup_and_huge_files_are_refused() {
        let root = temp_dir("backup");
        let path = root.join("session.json");
        let store = SessionStore::open(&path);
        store.write(&sample()).unwrap();
        assert_eq!(mode_of(&path), 0o600);
        store.fail_next_rename();
        let error = store.write(&sample()).unwrap_err();
        assert!(error.to_string().contains("rename"), "{error}");
        assert_eq!(mode_of(&root.join("session.json.tmp")), 0o600);
        fs::write(&path, b"{").unwrap();
        let ReadSession::Loaded { file, .. } = store.read().unwrap() else {
            panic!("backup was not used");
        };
        assert_eq!(file.scene.prose, "a red square");
        assert!(root.read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("session.corrupt-")
        }));
        let huge = root.join("huge.json");
        let file = fs::File::create(&huge).unwrap();
        file.set_len(256 * 1024 * 1024 + 1).unwrap();
        let error = SessionStore::open(&huge).read().unwrap_err();
        assert!(matches!(error, SessionError::TooLarge), "{error}");
    }

    #[test]
    fn owned_images_are_written_once_and_a_missing_session_is_empty() {
        let root = temp_dir("own");
        let dir = root.join("session");
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([9, 8, 7])));
        let first = own_image(&dir, &image).unwrap();
        let second = own_image(&dir, &image).unwrap();
        assert_eq!(first, second);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        let store = SessionStore::open(root.join("session.json"));
        assert!(matches!(store.read().unwrap(), ReadSession::Missing));
        let mut file = sample();
        file.source = Some(first);
        store.write(&file).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        file.source = None;
        store.write(&file).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    }

    fn mode_of(path: &std::path::Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
}

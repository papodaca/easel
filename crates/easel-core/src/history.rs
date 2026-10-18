use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::ProviderError;
use crate::image_prep::ImageSummary;
use crate::job::{EncodedImage, JobRequest, JobState};
use crate::providers::{JobId, Submitted, bfl::protocol::extract_result_mime};
use crate::scene::BoxElement;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Protocol(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Completion {
    pub cost: Option<f64>,
    pub prompt: Option<String>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: JobId,
    pub state: JobState,
    pub poll_url: Option<String>,
    pub cost: Option<f64>,
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedRequest {
    pub model_id: String,
    pub compiled_prompt: String,
    pub params: serde_json::Map<String, serde_json::Value>,
    pub images: Vec<ImageSummary>,
    pub box_snapshot: Vec<BoxElement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntrySummary {
    pub id: JobId,
    pub state: JobState,
    pub cost: Option<f64>,
    pub created_at: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRequest {
    model_id: String,
    compiled_prompt: String,
    params: serde_json::Value,
    images: Vec<ImageSummary>,
    box_snapshot: Vec<BoxElement>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Meta {
    state: JobState,
    model_id: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    interaction_id: String,
    #[serde(default)]
    suggestion: String,
    created_at: String,
    cost: Option<f64>,
    prompt: Option<String>,
    duration: Option<f64>,
    poll_url: Option<String>,
    error: Option<ProviderError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexFile {
    entries: Vec<IndexRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexRow {
    id: String,
    created_at: String,
    state: JobState,
    bytes: u64,
    cost: Option<f64>,
}

pub struct HistoryStore {
    root: PathBuf,
    fail_rename: AtomicBool,
}

impl HistoryStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(io_err)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(io_err)?;
        Ok(Self {
            root,
            fail_rename: AtomicBool::new(false),
        })
    }

    pub fn fail_next_index_rename(&self) {
        self.fail_rename.store(true, Ordering::SeqCst);
    }

    pub fn begin(
        &self,
        id: &JobId,
        request: &JobRequest,
        submitted: &Submitted,
    ) -> Result<Entry, StoreError> {
        let dir = self.entry_dir(id)?;
        fs::create_dir_all(&dir).map_err(io_err)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(io_err)?;
        let stored = StoredRequest {
            model_id: request.model_id.clone(),
            compiled_prompt: request.compiled_prompt.clone(),
            params: serde_json::Value::Object(request.params.clone().into_iter().collect()),
            images: request.images.iter().map(EncodedImage::summary).collect(),
            box_snapshot: request.box_snapshot.clone(),
            created_at: request.created_at.clone(),
        };
        write_private(
            &dir.join("request.json"),
            &serde_json::to_vec_pretty(&stored)
                .map_err(|err| StoreError::Protocol(err.to_string()))?,
        )?;
        let state = JobState::Polling {
            attempt: 0,
            progress: None,
        };
        let meta = Meta {
            state: state.clone(),
            model_id: request.model_id.clone(),
            provider: provider_name(&request.model_id),
            created_at: request.created_at.clone(),
            cost: submitted.quote.map(|quote| quote.cost),
            prompt: None,
            duration: None,
            poll_url: Some(submitted.poll_url.to_string()),
            error: None,
            interaction_id: String::new(),
            suggestion: String::new(),
        };
        self.write_meta(id, &meta)?;
        self.upsert_row(IndexRow {
            id: id.as_str().to_string(),
            created_at: request.created_at.clone(),
            state,
            bytes: dir_bytes(&dir)?,
            cost: meta.cost,
        })?;
        Ok(Entry {
            id: id.clone(),
            state: meta.state,
            poll_url: meta.poll_url,
            cost: meta.cost,
            provider: meta.provider,
            model_id: meta.model_id,
        })
    }

    pub fn complete(
        &self,
        id: &JobId,
        result_bytes: &[u8],
        _mime: &str,
        thumb_bytes: &[u8],
        completion: &Completion,
    ) -> Result<(), StoreError> {
        let dir = self.entry_dir(id)?;
        let mime = extract_result_mime(None, result_bytes);
        let (file_name, state) = match mime {
            Some("image/png") => ("result.png", None),
            Some("image/jpeg") => ("result.jpg", None),
            Some("image/webp") => ("result.webp", None),
            _ => (
                "result.bin",
                Some(ProviderError::Protocol("unsupported result format".into())),
            ),
        };
        write_private(&dir.join(file_name), result_bytes)?;
        if state.is_none() {
            write_private(&dir.join("thumb.jpg"), thumb_bytes)?;
        }
        let mut meta = self.read_meta(id)?;
        meta.cost = completion.cost.or(meta.cost);
        meta.prompt = completion.prompt.clone();
        meta.duration = completion.duration;
        meta.poll_url = None;
        meta.error = state.clone();
        meta.state = match state {
            Some(error) => JobState::Failed { error },
            None => JobState::Done {
                result_path: file_name.into(),
                cost: meta.cost,
            },
        };
        self.write_meta(id, &meta)?;
        self.upsert_row(IndexRow {
            id: id.as_str().to_string(),
            created_at: meta.created_at,
            state: meta.state,
            bytes: dir_bytes(&dir)?,
            cost: meta.cost,
        })?;
        Ok(())
    }

    /// Write a finished Google image without a poll row.
    pub fn complete_inline(
        &self,
        id: &JobId,
        request: &JobRequest,
        interaction_id: &str,
        suggestion: &str,
        result_bytes: &[u8],
        mime: &str,
        thumb_bytes: &[u8],
    ) -> Result<(), StoreError> {
        let dir = self.prepare_dir(id)?;
        self.write_request(&dir, request)?;
        let file_name = match mime {
            "image/jpeg" => "result.jpg",
            "image/webp" => "result.webp",
            _ => "result.png",
        };
        write_private(&dir.join(file_name), result_bytes)?;
        write_private(&dir.join("thumb.jpg"), thumb_bytes)?;
        let state = JobState::Done {
            result_path: file_name.into(),
            cost: None,
        };
        let meta = Meta {
            state: state.clone(),
            model_id: request.model_id.clone(),
            provider: provider_name(&request.model_id),
            created_at: request.created_at.clone(),
            cost: None,
            prompt: None,
            duration: None,
            poll_url: None,
            error: None,
            interaction_id: interaction_id.to_owned(),
            suggestion: suggestion.to_owned(),
        };
        self.write_meta(id, &meta)?;
        self.upsert_row(IndexRow {
            id: id.as_str().to_string(),
            created_at: request.created_at.clone(),
            state,
            bytes: dir_bytes(&dir)?,
            cost: None,
        })?;
        Ok(())
    }

    /// Write a failed Google row that was never polled. `fail` cannot create this row.
    pub fn fail_unpolled(
        &self,
        id: &JobId,
        request: &JobRequest,
        error: &ProviderError,
    ) -> Result<(), StoreError> {
        let dir = self.prepare_dir(id)?;
        self.write_request(&dir, request)?;
        let state = JobState::Failed {
            error: error.clone(),
        };
        let meta = Meta {
            state: state.clone(),
            model_id: request.model_id.clone(),
            provider: provider_name(&request.model_id),
            created_at: request.created_at.clone(),
            cost: None,
            prompt: None,
            duration: None,
            poll_url: None,
            error: Some(error.clone()),
            interaction_id: String::new(),
            suggestion: String::new(),
        };
        self.write_meta(id, &meta)?;
        self.upsert_row(IndexRow {
            id: id.as_str().to_string(),
            created_at: request.created_at.clone(),
            state,
            bytes: dir_bytes(&dir)?,
            cost: None,
        })?;
        Ok(())
    }

    pub fn suggestion(&self, id: &JobId) -> String {
        self.read_meta(id)
            .map(|meta| meta.suggestion)
            .unwrap_or_default()
    }

    pub fn fail(&self, id: &JobId, error: &ProviderError) -> Result<(), StoreError> {
        let mut meta = self.read_meta(id)?;
        meta.error = Some(error.clone());
        meta.state = JobState::Failed {
            error: error.clone(),
        };
        meta.poll_url = None;
        self.write_meta(id, &meta)?;
        self.upsert_row(IndexRow {
            id: id.as_str().to_string(),
            created_at: meta.created_at,
            state: meta.state,
            bytes: dir_bytes(&self.entry_dir(id)?)?,
            cost: meta.cost,
        })?;
        Ok(())
    }

    pub fn set_state(&self, id: &JobId, state: JobState) -> Result<(), StoreError> {
        let mut meta = self.read_meta(id)?;
        if state.is_terminal() {
            meta.poll_url = None;
        }
        meta.state = state;
        let created_at = meta.created_at.clone();
        let cost = meta.cost;
        let stored = meta.state.clone();
        self.write_meta(id, &meta)?;
        self.upsert_row(IndexRow {
            id: id.as_str().to_string(),
            created_at,
            state: stored,
            bytes: dir_bytes(&self.entry_dir(id)?)?,
            cost,
        })?;
        Ok(())
    }

    pub fn remove(&self, id: &JobId) -> Result<(), StoreError> {
        let dir = self.entry_dir(id)?;
        if dir.exists() {
            let canonical = dir.canonicalize().map_err(io_err)?;
            let root = self.root.canonicalize().map_err(io_err)?;
            if canonical.parent() != Some(root.as_path()) {
                return Err(StoreError::Protocol(
                    "refusing to delete a directory outside the history root".into(),
                ));
            }
            fs::remove_dir_all(&canonical).map_err(io_err)?;
        }
        let mut rows = self.read_index()?;
        rows.retain(|row| row.id != id.as_str());
        self.write_index(&rows)
    }

    pub fn result_bytes(&self, id: &JobId) -> Result<Vec<u8>, StoreError> {
        let dir = self.entry_dir(id)?;
        for name in ["result.png", "result.jpg", "result.webp", "result.bin"] {
            let path = dir.join(name);
            if path.exists() {
                return fs::read(path).map_err(io_err);
            }
        }
        Err(StoreError::Io("result file is missing".into()))
    }

    pub fn thumb_bytes(&self, id: &JobId) -> Option<Vec<u8>> {
        let path = self.entry_dir(id).ok()?.join("thumb.jpg");
        fs::read(path).ok()
    }

    pub fn load_request(&self, id: &JobId) -> Result<LoadedRequest, StoreError> {
        let path = self.entry_dir(id)?.join("request.json");
        let bytes = fs::read(path).map_err(io_err)?;
        let stored: StoredRequest =
            serde_json::from_slice(&bytes).map_err(|err| StoreError::Protocol(err.to_string()))?;
        let params = match stored.params {
            serde_json::Value::Object(map) => map,
            other => {
                let mut map = serde_json::Map::new();
                map.insert("value".into(), other);
                map
            }
        };
        Ok(LoadedRequest {
            model_id: stored.model_id,
            compiled_prompt: stored.compiled_prompt,
            params,
            images: stored.images,
            box_snapshot: stored.box_snapshot,
        })
    }

    pub fn list(&self) -> Result<Vec<EntrySummary>, StoreError> {
        let mut rows = self.read_index()?;
        rows.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        let mut listed = Vec::new();
        for row in rows {
            let Ok(id) = JobId::parse(&row.id) else {
                eprintln!("warning: skipping history id {}", row.id);
                continue;
            };
            listed.push(EntrySummary {
                id,
                state: row.state,
                cost: row.cost,
                created_at: row.created_at,
                bytes: row.bytes,
            });
        }
        Ok(listed)
    }

    pub fn in_flight(&self) -> Result<Vec<Entry>, StoreError> {
        let mut entries = Vec::new();
        for summary in self.list()? {
            if !summary.state.is_in_flight() {
                continue;
            }
            let meta = self.read_meta(&summary.id)?;
            entries.push(Entry {
                id: summary.id,
                state: meta.state,
                poll_url: meta.poll_url,
                cost: meta.cost,
                provider: meta.provider,
                model_id: meta.model_id,
            });
        }
        Ok(entries)
    }

    pub fn evict_to(&self, max_entries: usize, max_bytes: u64) -> Result<(), StoreError> {
        loop {
            let rows = self.read_index()?;
            let completed: Vec<_> = rows
                .iter()
                .filter(|row| JobId::parse(&row.id).is_ok() && !row.state.is_in_flight())
                .cloned()
                .collect();
            let total: u64 = rows
                .iter()
                .filter(|row| JobId::parse(&row.id).is_ok())
                .map(|row| row.bytes)
                .sum();
            if completed.len() <= max_entries && total <= max_bytes {
                return Ok(());
            }
            let Some(oldest) = completed
                .iter()
                .min_by(|left, right| left.created_at.cmp(&right.created_at))
            else {
                return Ok(());
            };
            let id = JobId::parse(&oldest.id).map_err(StoreError::Protocol)?;
            self.remove(&id)?;
        }
    }

    fn entry_dir(&self, id: &JobId) -> Result<PathBuf, StoreError> {
        Ok(self.root.join(id.as_str()))
    }

    fn prepare_dir(&self, id: &JobId) -> Result<PathBuf, StoreError> {
        let dir = self.entry_dir(id)?;
        fs::create_dir_all(&dir).map_err(io_err)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(io_err)?;
        Ok(dir)
    }

    fn write_request(&self, dir: &std::path::Path, request: &JobRequest) -> Result<(), StoreError> {
        let stored = StoredRequest {
            model_id: request.model_id.clone(),
            compiled_prompt: request.compiled_prompt.clone(),
            params: serde_json::Value::Object(request.params.clone().into_iter().collect()),
            images: request.images.iter().map(EncodedImage::summary).collect(),
            box_snapshot: request.box_snapshot.clone(),
            created_at: request.created_at.clone(),
        };
        write_private(
            &dir.join("request.json"),
            &serde_json::to_vec_pretty(&stored)
                .map_err(|err| StoreError::Protocol(err.to_string()))?,
        )
    }

    fn write_meta(&self, id: &JobId, meta: &Meta) -> Result<(), StoreError> {
        write_private(
            &self.entry_dir(id)?.join("meta.json"),
            &serde_json::to_vec_pretty(meta)
                .map_err(|err| StoreError::Protocol(err.to_string()))?,
        )
    }

    fn read_meta(&self, id: &JobId) -> Result<Meta, StoreError> {
        let bytes = fs::read(self.entry_dir(id)?.join("meta.json")).map_err(io_err)?;
        serde_json::from_slice(&bytes).map_err(|err| StoreError::Protocol(err.to_string()))
    }

    fn read_index(&self) -> Result<Vec<IndexRow>, StoreError> {
        let path = self.root.join("history.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&path).map_err(io_err)?;
        let file: IndexFile =
            serde_json::from_slice(&bytes).map_err(|err| StoreError::Protocol(err.to_string()))?;
        Ok(file.entries)
    }

    fn upsert_row(&self, row: IndexRow) -> Result<(), StoreError> {
        let mut rows = self.read_index()?;
        if let Some(existing) = rows.iter_mut().find(|existing| existing.id == row.id) {
            *existing = row;
        } else {
            rows.insert(0, row);
        }
        self.write_index(&rows)
    }

    fn write_index(&self, rows: &[IndexRow]) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(&IndexFile {
            entries: rows.to_vec(),
        })
        .map_err(|err| StoreError::Protocol(err.to_string()))?;
        let tmp = self.root.join("history.json.tmp");
        write_private(&tmp, &bytes)?;
        if self.fail_rename.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Io("index rename failed".into()));
        }
        fs::rename(&tmp, self.root.join("history.json")).map_err(io_err)?;
        fs::set_permissions(
            self.root.join("history.json"),
            fs::Permissions::from_mode(0o600),
        )
        .map_err(io_err)?;
        Ok(())
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(io_err)?;
    file.write_all(bytes).map_err(io_err)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(io_err)?;
    Ok(())
}

fn dir_bytes(dir: &Path) -> Result<u64, StoreError> {
    let mut total = 0;
    for entry in fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let meta = entry.metadata().map_err(io_err)?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

fn provider_name(model_id: &str) -> String {
    use crate::model_spec::ProviderId;
    use crate::providers::ModelRegistry;

    ModelRegistry::specs()
        .iter()
        .find(|spec| spec.id == model_id)
        .map(|spec| match spec.provider {
            ProviderId::Bfl => "bfl",
            ProviderId::Krea => "krea",
            ProviderId::Google => "google",
        })
        .unwrap_or("")
        .to_owned()
}

fn io_err(error: impl std::fmt::Display) -> StoreError {
    StoreError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Completion, HistoryStore};
    use crate::geometry::Size;
    use crate::image_prep::ImageSummary;
    use crate::job::{EncodedImage, JobRequest, JobState};
    use crate::providers::{CostQuote, JobId, Submitted};
    use crate::scene::{BoxElement, BoxKind, Scene};
    use serde_json::json;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SCRATCH: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> PathBuf {
        let n = SCRATCH.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("easel-history-{nanos}-{n}"));
        let _ = fs::remove_dir_all(&path);
        path
    }

    fn request_with_images() -> JobRequest {
        JobRequest {
            model_id: "flux-3-image".into(),
            compiled_prompt: "a fox".into(),
            params: serde_json::from_value(json!({"resolution": "1k"})).unwrap(),
            images: vec![
                EncodedImage {
                    media_type: "image/png".into(),
                    bytes: b"PAYLOAD-ONE-SHOULD-NOT-BE-STORED".to_vec(),
                    width: 300,
                    height: 300,
                    content_hash: "hash-one".into(),
                },
                EncodedImage {
                    media_type: "image/jpeg".into(),
                    bytes: b"PAYLOAD-TWO-SHOULD-NOT-BE-STORED".to_vec(),
                    width: 400,
                    height: 200,
                    content_hash: "hash-two".into(),
                },
            ],
            box_snapshot: vec![BoxElement {
                id: crate::scene::BoxId::parse("animal_1").unwrap(),
                kind: BoxKind::New,
                desc: "fox".into(),
                src: None,
                tgt: None,
                from_image: None,
            }],
            created_at: "2026-10-15T00:00:00Z".into(),
        }
    }

    fn submitted(id: &str) -> Submitted {
        Submitted {
            job_id: JobId::parse(id).unwrap(),
            poll_url: "https://api.bfl.ai/v1/get_result?id=abc".parse().unwrap(),
            quote: Some(CostQuote {
                cost: 1.5,
                input_mp: 1.0,
                output_mp: 2.0,
            }),
        }
    }

    fn png_bytes(extra: usize) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend(std::iter::repeat_n(1, extra));
        bytes
    }

    fn mode_of(path: &std::path::Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn begin_then_complete_writes_the_entry_and_lists_it_done() {
        let root = scratch();
        let store = HistoryStore::open(&root).unwrap();
        assert_eq!(mode_of(&root), 0o700);
        let id = JobId::parse("job1").unwrap();
        store
            .begin(&id, &request_with_images(), &submitted("job1"))
            .unwrap();
        store
            .complete(
                &id,
                &png_bytes(32),
                "image/png",
                b"thumb",
                &Completion {
                    cost: Some(2.0),
                    prompt: Some("expanded".into()),
                    duration: Some(1.2),
                },
            )
            .unwrap();
        assert!(root.join("job1/result.png").is_file());
        assert!(root.join("job1/thumb.jpg").is_file());
        assert!(root.join("job1/request.json").is_file());
        assert!(root.join("job1/meta.json").is_file());
        for name in ["result.png", "thumb.jpg", "request.json", "meta.json"] {
            assert_eq!(mode_of(&root.join("job1").join(name)), 0o600, "{name}");
        }
        assert_eq!(mode_of(&root.join("history.json")), 0o600);
        let listed = store.list().unwrap();
        assert!(matches!(listed[0].state, JobState::Done { .. }));
        let request_bytes = fs::read(root.join("job1/request.json")).unwrap();
        assert!(request_bytes.len() < 64 * 1024);
        let request_text = String::from_utf8(request_bytes).unwrap();
        assert!(!request_text.contains("PAYLOAD-"));
        let summaries: Vec<ImageSummary> = serde_json::from_str::<serde_json::Value>(&request_text)
            .unwrap()["images"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| serde_json::from_value(value.clone()).unwrap())
            .collect();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].content_hash, "hash-one");
        let meta = fs::read(root.join("job1/meta.json")).unwrap();
        let index = fs::read(root.join("history.json")).unwrap();
        assert!(!meta.windows(9).any(|window| window == b"delivery."));
        assert!(!index.windows(9).any(|window| window == b"delivery."));
        let _ = Size { w: 1, h: 1 };
        let _ = Scene::empty();
    }

    #[test]
    fn fail_remove_and_reopen_round_trip_in_flight() {
        let root = scratch();
        let id = JobId::parse("inflight1").unwrap();
        {
            let store = HistoryStore::open(&root).unwrap();
            store
                .begin(&id, &request_with_images(), &submitted("inflight1"))
                .unwrap();
        }
        let store = HistoryStore::open(&root).unwrap();
        let pending = store.in_flight().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].poll_url.as_deref(),
            Some("https://api.bfl.ai/v1/get_result?id=abc")
        );
        assert!(pending[0].state.is_in_flight());
        store
            .fail(&id, &crate::error::ProviderError::Protocol("nope".into()))
            .unwrap();
        let meta = fs::read_to_string(root.join("inflight1/meta.json")).unwrap();
        assert!(meta.contains("nope"));
        assert!(matches!(
            store.list().unwrap()[0].state,
            JobState::Failed { .. }
        ));
        store.remove(&id).unwrap();
        assert!(!root.join("inflight1").exists());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn a_failed_index_rename_keeps_the_previous_index() {
        let root = scratch();
        let store = HistoryStore::open(&root).unwrap();
        let id = JobId::parse("keepme").unwrap();
        store
            .begin(&id, &request_with_images(), &submitted("keepme"))
            .unwrap();
        let before = fs::read(root.join("history.json")).unwrap();
        store.fail_next_index_rename();
        assert!(store.fail(&id, &crate::error::ProviderError::Auth).is_err());
        assert_eq!(fs::read(root.join("history.json")).unwrap(), before);
        let tmp = root.join("history.json.tmp");
        assert!(tmp.is_file());
        assert_eq!(mode_of(&tmp), 0o600);
    }

    #[test]
    fn eviction_keeps_in_flight_and_honors_the_byte_cap() {
        let root = scratch();
        let store = HistoryStore::open(&root).unwrap();
        for (index, name) in ["oldest", "old", "mid", "new"].into_iter().enumerate() {
            let id = JobId::parse(name).unwrap();
            let mut request = request_with_images();
            request.created_at = format!("2026-10-15T00:00:0{index}Z");
            store.begin(&id, &request, &submitted(name)).unwrap();
            store
                .complete(
                    &id,
                    &png_bytes(600_000),
                    "image/png",
                    b"t",
                    &Completion::default(),
                )
                .unwrap();
        }
        let flying = JobId::parse("flying").unwrap();
        store
            .begin(&flying, &request_with_images(), &submitted("flying"))
            .unwrap();
        store.evict_to(3, u64::MAX).unwrap();
        let ids: Vec<_> = store
            .list()
            .unwrap()
            .into_iter()
            .map(|entry| entry.id.as_str().to_string())
            .collect();
        assert!(ids.contains(&"flying".to_string()));
        assert!(ids.contains(&"old".to_string()));
        assert!(!ids.contains(&"oldest".to_string()));
        assert!(!root.join("oldest").exists());

        store.evict_to(100, 1_000_000).unwrap();
        let completed = store
            .list()
            .unwrap()
            .into_iter()
            .filter(|entry| !entry.state.is_in_flight())
            .count();
        assert_eq!(completed, 1);
        assert!(root.join("flying").is_dir());
    }

    #[test]
    fn bad_index_ids_are_skipped_and_never_deleted() {
        let root = scratch();
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/keep.txt"), b"safe").unwrap();
        fs::write(
            root.join("history.json"),
            br#"{"entries":[{"id":"..","created_at":"2026-10-15T00:00:00Z","state":"queued","bytes":0,"cost":null},{"id":"x/y","created_at":"2026-10-15T00:00:00Z","state":"queued","bytes":0,"cost":null}]}"#,
        )
        .unwrap();
        let store = HistoryStore::open(&root).unwrap();
        assert!(store.list().unwrap().is_empty());
        store.evict_to(0, 0).unwrap();
        assert_eq!(fs::read(root.join("notes/keep.txt")).unwrap(), b"safe");
    }

    #[test]
    fn unknown_magic_fails_the_entry_and_a_read_only_root_keeps_the_index() {
        let root = scratch();
        let store = HistoryStore::open(&root).unwrap();
        let id = JobId::parse("gifjob").unwrap();
        store
            .begin(&id, &request_with_images(), &submitted("gifjob"))
            .unwrap();
        store
            .complete(
                &id,
                b"GIF89a-not-really",
                "image/png",
                b"t",
                &Completion::default(),
            )
            .unwrap();
        assert!(root.join("gifjob/result.bin").is_file());
        let meta = fs::read_to_string(root.join("gifjob/meta.json")).unwrap();
        assert!(meta.contains("unsupported result format"));
        assert!(matches!(
            store.list().unwrap()[0].state,
            JobState::Failed { .. }
        ));

        let root = scratch();
        let store = HistoryStore::open(&root).unwrap();
        let id = JobId::parse("locked").unwrap();
        store
            .begin(&id, &request_with_images(), &submitted("locked"))
            .unwrap();
        let before = fs::read(root.join("history.json")).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
        let failed = store.complete(
            &id,
            &png_bytes(8),
            "image/png",
            b"t",
            &Completion::default(),
        );
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(failed.is_err());
        assert_eq!(fs::read(root.join("history.json")).unwrap(), before);
    }

    #[test]
    fn a_bfl_begin_still_records_provider_bfl() {
        let root = scratch();
        let store = HistoryStore::open(&root).unwrap();
        let id = JobId::parse("bfljob").unwrap();
        store
            .begin(&id, &request_with_images(), &submitted("bfljob"))
            .unwrap();
        let meta: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("bfljob/meta.json")).unwrap()).unwrap();
        assert_eq!(meta["provider"], "bfl");
        assert!(meta["poll_url"].is_string());
    }

    #[test]
    fn complete_writes_a_missing_interaction_id_back_as_empty() {
        let root = scratch();
        let store = HistoryStore::open(&root).unwrap();
        let id = JobId::parse("oldjob").unwrap();
        store
            .begin(&id, &request_with_images(), &submitted("oldjob"))
            .unwrap();
        let path = root.join("oldjob/meta.json");
        let mut meta: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        meta.as_object_mut().unwrap().remove("interaction_id");
        fs::write(&path, serde_json::to_vec_pretty(&meta).unwrap()).unwrap();
        store
            .complete(
                &id,
                &png_bytes(8),
                "image/png",
                b"thumb",
                &Completion::default(),
            )
            .unwrap();
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["interaction_id"], "");
        assert_eq!(written["model_id"], "flux-3-image");
    }

    #[test]
    fn inline_png_and_jpeg_keep_the_payload_bytes() {
        let root = scratch();
        let store = HistoryStore::open(&root).unwrap();
        let png_id = JobId::parse("gpng").unwrap();
        let png = png_bytes(4);
        let interaction = "i".repeat(70);
        store
            .complete_inline(
                &png_id,
                &request_with_images(),
                &interaction,
                "",
                &png,
                "image/png",
                b"thumb",
            )
            .unwrap();
        assert_eq!(fs::read(root.join("gpng/result.png")).unwrap(), png);
        assert!(!root.join("gpng").join(&interaction).exists());
        let meta: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("gpng/meta.json")).unwrap()).unwrap();
        assert_eq!(meta["interaction_id"], interaction);
        assert!(meta["poll_url"].is_null());
        assert_ne!(png_id.as_str(), interaction);

        let jpeg_id = JobId::parse("gjpeg").unwrap();
        let jpeg = b"jpeg-payload".to_vec();
        store
            .complete_inline(
                &jpeg_id,
                &request_with_images(),
                "",
                "",
                &jpeg,
                "image/jpeg",
                b"thumb",
            )
            .unwrap();
        assert_eq!(fs::read(root.join("gjpeg/result.jpg")).unwrap(), jpeg);
    }
}

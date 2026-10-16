mod runner;

use std::path::PathBuf;

#[allow(unused_imports)]
pub use runner::JobRunner;

pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("ai", "easel", "easel")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("easel-data"))
}

mod promote;
mod runner;
mod submit;

#[cfg(test)]
mod chutes_tests;
#[cfg(test)]
mod krea_tests;
#[cfg(test)]
mod openai_tests;
#[cfg(test)]
mod xai_tests;

use std::path::PathBuf;

#[allow(unused_imports)]
pub use promote::{Inspection, inspection_from, promote};
#[allow(unused_imports)]
pub use runner::{JobRunner, LiveJob};
pub use submit::build_request;

pub fn data_dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("easel-test-data-{}", std::process::id()));
    }
    directories::ProjectDirs::from("ai", "easel", "easel")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("easel-data"))
}

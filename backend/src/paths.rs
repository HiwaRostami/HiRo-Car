use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct ResolvedData {
    pub code_descriptions: PathBuf,
    pub model_pids: PathBuf,
    pub requests: PathBuf,
    pub vpic_bundled: bool,
}

static RESOLVED: OnceLock<ResolvedData> = OnceLock::new();

pub fn set_resolved(data: ResolvedData) {
    let _ = RESOLVED.set(data);
}

pub fn code_desc_db_path() -> String {
    RESOLVED
        .get()
        .map(|data| data.code_descriptions.to_string_lossy().into_owned())
        .unwrap_or_else(|| "./data/code-descriptions.sqlite".to_string())
}

pub fn mode22_pids_db_path() -> String {
    RESOLVED
        .get()
        .map(|data| data.model_pids.to_string_lossy().into_owned())
        .unwrap_or_else(|| "./data/model-pids.sqlite".to_string())
}

pub fn requests_json_path() -> String {
    RESOLVED
        .get()
        .map(|data| data.requests.to_string_lossy().into_owned())
        .unwrap_or_else(|| "./data/requests.json".to_string())
}

pub fn vpic_database_bundled() -> bool {
    RESOLVED.get().map(|data| data.vpic_bundled).unwrap_or(true)
}

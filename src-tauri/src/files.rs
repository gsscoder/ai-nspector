//! Read-only access to one file of the open repo.

use std::fs;
use std::path::{Component, Path};

use crate::error::AppError;

const MAX_BYTES: u64 = 1_000_000;

pub(crate) fn read_repo_file(repo: &Path, rel: &str) -> Result<String, AppError> {
    let rel_path = Path::new(rel);
    // Only plain relative paths: no `..`, no root, no drive prefix.
    if rel_path.is_absolute()
        || !rel_path.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(AppError::Invalid(format!("Path must be relative and inside the repo: {rel}")));
    }
    let root = repo
        .canonicalize()
        .map_err(|e| AppError::Io("Cannot open the repo folder".into(), e))?;
    let full = root
        .join(rel_path)
        .canonicalize()
        .map_err(|e| AppError::Io(format!("Cannot open {rel}"), e))?;
    // A symlink inside the repo must not lead out of it.
    if !full.starts_with(&root) {
        return Err(AppError::Invalid(format!("Path must be relative and inside the repo: {rel}")));
    }
    let size = fs::metadata(&full)
        .map_err(|e| AppError::Io(format!("Cannot open {rel}"), e))?
        .len();
    if size > MAX_BYTES {
        return Err(AppError::Invalid(format!("{rel} is too large")));
    }
    let bytes = fs::read(&full).map_err(|e| AppError::Io(format!("Cannot read {rel}"), e))?;
    String::from_utf8(bytes).map_err(|_| AppError::Invalid(format!("{rel} is not a UTF-8 text file")))
}

#[tauri::command]
pub async fn read_file(repo_path: String, path: String) -> Result<String, AppError> {
    tauri::async_runtime::spawn_blocking(move || read_repo_file(Path::new(&repo_path), &path))
        .await
        .map_err(|e| AppError::Platform("Reading did not finish".into(), e.into()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_file_inside_the_repo() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/a.ts"), "let a = 1;").unwrap();
        assert_eq!(read_repo_file(dir.path(), "src/a.ts").unwrap(), "let a = 1;");
    }

    #[test]
    fn rejects_parent_and_absolute_paths() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), "secret").unwrap();
        assert!(read_repo_file(dir.path(), "../x").is_err());
        assert!(read_repo_file(dir.path(), "a/../../x").is_err());
        assert!(read_repo_file(dir.path(), outside.path().to_str().unwrap()).is_err());
    }

    #[test]
    fn too_large_and_binary_files_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("big.txt"), vec![b'a'; 1_000_001]).unwrap();
        fs::write(dir.path().join("bin.dat"), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read_repo_file(dir.path(), "big.txt").unwrap_err().to_string(), "big.txt is too large");
        assert!(read_repo_file(dir.path(), "bin.dat").unwrap_err().to_string().contains("UTF-8"));
    }

    #[test]
    fn missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_repo_file(dir.path(), "nope.ts").is_err());
    }
}

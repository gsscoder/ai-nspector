//! Helpers for repo-relative paths with forward slashes. The empty string is the repo root.

/// Directory part of `path`; empty for a file at the root.
pub(super) fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |slash| &path[..slash])
}

pub(super) fn join(dir: &str, relative: &str) -> String {
    if dir.is_empty() {
        relative.to_string()
    } else {
        format!("{dir}/{relative}")
    }
}

/// Resolves `.` and `..` segments. Returns `None` when the path climbs above the root.
pub(super) fn normalize(path: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    Some(segments.join("/"))
}

/// Longest directory that contains every path; empty when there is none or no path.
pub(super) fn common_dir<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let mut common: Option<Vec<&str>> = None;
    for path in paths {
        let dir: Vec<&str> = parent_dir(path).split('/').filter(|s| !s.is_empty()).collect();
        common = Some(match common {
            None => dir,
            Some(prefix) => prefix
                .into_iter()
                .zip(dir)
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect(),
        });
    }
    common.unwrap_or_default().join("/")
}

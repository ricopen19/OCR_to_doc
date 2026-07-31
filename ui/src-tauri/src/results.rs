use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::job::RecentResultEntry;
use crate::paths::{resolve_output_root_from_disk, resolve_project_root};

pub fn find_output_path(
    result_root: &Path,
    project_root: &Path,
    filename: &str,
) -> Option<PathBuf> {
    if result_root.exists() {
        if let Ok(entries) = fs::read_dir(result_root) {
            for entry in entries.flatten() {
                let candidate = entry.path().join(filename);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    let candidate = result_root.join(filename);
    if candidate.exists() {
        return Some(candidate);
    }

    let candidate = project_root.join(filename);
    if candidate.exists() {
        return Some(candidate);
    }

    None
}

pub fn open_path_with_default_app(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("explorer");
        c.arg(path);
        c
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };

    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to open path: {e}"))
}

pub fn validate_result_dir_name(dir_name: &str) -> Result<(), String> {
    if dir_name.is_empty() {
        return Err("dirName is empty".into());
    }
    if dir_name.contains('/') || dir_name.contains('\\') || dir_name.contains("..") {
        return Err("invalid dirName".into());
    }
    Ok(())
}

pub fn canonicalize_dir(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|e| format!("failed to canonicalize path: {e}"))
}

pub fn parse_page_range_from_dir(dir_name: &str) -> Option<String> {
    let pos = dir_name.rfind("_p")?;
    let rest = &dir_name[(pos + 2)..];
    let mut parts = rest.splitn(2, '-');
    let start = parts.next()?;
    let end = parts.next()?;
    if start.is_empty() || end.is_empty() {
        return None;
    }
    if !start.chars().all(|c| c.is_ascii_digit()) || !end.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("p{}-{}", start, end))
}

pub fn pick_best_file_in_dir(dir: &Path, dir_name: &str) -> Option<String> {
    let candidates = [
        format!("{dir_name}_merged.docx"),
        format!("{dir_name}.docx"),
        format!("{dir_name}.xlsx"),
        format!("{dir_name}_merged.xlsx"),
        format!("{dir_name}.csv"),
        format!("{dir_name}_merged.csv"),
        format!("{dir_name}_merged.md"),
        format!("{dir_name}.md"),
    ];

    for filename in candidates {
        if dir.join(&filename).exists() {
            return Some(filename);
        }
    }

    // fallback: scan directory for known extensions
    if let Ok(entries) = fs::read_dir(dir) {
        let mut docx = None;
        let mut xlsx = None;
        let mut csv = None;
        let mut md = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let lower = name.to_lowercase();
            if docx.is_none() && lower.ends_with(".docx") {
                docx = Some(name);
                continue;
            }
            if xlsx.is_none() && lower.ends_with(".xlsx") {
                xlsx = Some(name);
                continue;
            }
            if csv.is_none() && lower.ends_with(".csv") {
                csv = Some(name);
                continue;
            }
            if md.is_none() && lower.ends_with(".md") {
                md = Some(name);
            }
        }
        return docx.or(xlsx).or(csv).or(md);
    }

    None
}

pub fn list_recent_results(limit: Option<u32>) -> Result<Vec<RecentResultEntry>, String> {
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let result_root = resolve_output_root_from_disk(&project_root);
    if !result_root.exists() {
        return Ok(vec![]);
    }

    let mut dirs: Vec<(u64, String)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&result_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let modified = path
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let ms = modified
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            dirs.push((ms, name));
        }
    }

    dirs.sort_by(|(a, _), (b, _)| b.cmp(a));
    let take_n = limit.unwrap_or(10).max(1) as usize;
    let mut results = Vec::new();

    for (updated_at_ms, dir_name) in dirs.into_iter().take(take_n) {
        let dir_path = result_root.join(&dir_name);
        let best_file = pick_best_file_in_dir(&dir_path, &dir_name);
        let page_range = parse_page_range_from_dir(&dir_name);
        results.push(RecentResultEntry {
            dir_name,
            updated_at_ms,
            page_range,
            best_file,
        });
    }

    Ok(results)
}

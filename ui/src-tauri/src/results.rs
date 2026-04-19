use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::job::RecentResultEntry;
use crate::paths::{resolve_output_root_from_disk, resolve_project_root};

pub fn collect_output_files(
    result_root: &Path,
    project_root: &Path,
    inputs: &[String],
    formats: &[String],
) -> Vec<PathBuf> {
    fn push_unique(found: &mut Vec<PathBuf>, path: PathBuf) {
        if path.exists() && !found.contains(&path) {
            found.push(path);
        }
    }

    fn pick_latest_result_dir(result_root: &Path, stem: &str) -> Option<PathBuf> {
        if !result_root.exists() {
            return None;
        }

        let mut candidates: Vec<(SystemTime, PathBuf)> = Vec::new();

        let direct = result_root.join(stem);
        if direct.is_dir() {
            let modified = direct
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((modified, direct));
        }

        if let Ok(entries) = fs::read_dir(result_root) {
            let prefix = format!("{stem}_");
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
                if !name.starts_with(&prefix) {
                    continue;
                }
                let modified = path
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                candidates.push((modified, path));
            }
        }

        candidates.sort_by(|(a, _), (b, _)| b.cmp(a));
        candidates.first().map(|(_, p)| p.clone())
    }

    let mut found = Vec::new();
    for input in inputs {
        let input_path = PathBuf::from(input);
        let stem_owned = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();
        let stem = stem_owned.as_str();
        let mut md_found_for_input = false;
        let mut result_dir_opt: Option<PathBuf> = None;

        // result/<stem> もしくは result/<stem>_*（ページ範囲指定などの suffix 付き）の最新ディレクトリ内
        if let Some(result_dir) = pick_latest_result_dir(result_root, stem) {
            result_dir_opt = Some(result_dir.clone());
            let dir_name = result_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            for fmt in formats {
                if fmt == "xlsx" {
                    push_unique(&mut found, result_dir.join(format!("{dir_name}.xlsx")));
                    push_unique(&mut found, result_dir.join(format!("{stem}.xlsx")));
                    push_unique(
                        &mut found,
                        result_dir.join(format!("{dir_name}_merged.xlsx")),
                    );
                    push_unique(&mut found, result_dir.join(format!("{stem}_merged.xlsx")));
                    continue;
                }
                if fmt == "csv" {
                    if let Ok(entries) = fs::read_dir(&result_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if !path.is_file() {
                                continue;
                            }
                            if path.extension().map(|e| e == "csv").unwrap_or(false) {
                                push_unique(&mut found, path);
                            }
                        }
                    }
                    continue;
                }

                let merged = result_dir.join(format!("{dir_name}_merged.{fmt}"));
                push_unique(&mut found, merged.clone());
                let legacy_merged = result_dir.join(format!("{stem}_merged.{fmt}"));
                push_unique(&mut found, legacy_merged.clone());
                let stem_path = result_dir.join(format!("{stem}.{fmt}"));
                push_unique(&mut found, stem_path.clone());
                let dir_name_path = result_dir.join(format!("{dir_name}.{fmt}"));
                push_unique(&mut found, dir_name_path.clone());
                if fmt == "md"
                    && (merged.exists()
                        || legacy_merged.exists()
                        || stem_path.exists()
                        || dir_name_path.exists())
                {
                    md_found_for_input = true;
                }
            }
        }

        // ルート直下に <stem>_merged.<fmt> / <stem>.<fmt>
        for fmt in formats {
            let c1 = result_root.join(format!("{}_merged.{}", stem, fmt));
            let c2 = result_root.join(format!("{}.{}", stem, fmt));
            let c1_exists = c1.exists();
            let c2_exists = c2.exists();
            push_unique(&mut found, c1);
            push_unique(&mut found, c2);
            if fmt == "md" && (c1_exists || c2_exists) {
                md_found_for_input = true;
            }
            if result_root != project_root {
                let c3 = project_root.join(format!("{}_merged.{}", stem, fmt));
                let c4 = project_root.join(format!("{}.{}", stem, fmt));
                let c3_exists = c3.exists();
                let c4_exists = c4.exists();
                push_unique(&mut found, c3);
                push_unique(&mut found, c4);
                if fmt == "md" && (c3_exists || c4_exists) {
                    md_found_for_input = true;
                }
            }
        }

        if !md_found_for_input && formats.iter().any(|f| f == "md") {
            if let Some(result_dir) = result_dir_opt {
                if let Ok(entries) = fs::read_dir(&result_dir) {
                    let mut candidates = Vec::new();
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_file() {
                            continue;
                        }
                        if path.extension().map(|e| e == "md").unwrap_or(false) {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if name.starts_with("page_") {
                                    candidates.push(path);
                                }
                            }
                        }
                    }
                    candidates.sort();
                    if let Some(first) = candidates.first() {
                        push_unique(&mut found, first.clone());
                    }
                }
            }
        }
    }
    found
}

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

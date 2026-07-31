use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// page_*.md をソートして結合し、merged.md を生成する。
pub fn merge_page_markdowns(
    result_dir: &Path,
    base_name: &str,
    add_page_heading: bool,
) -> Result<PathBuf, String> {
    let page_files = collect_page_files(result_dir)?;
    if page_files.is_empty() {
        return Err("結合対象の md ファイルがありません".to_string());
    }

    let output_path = result_dir.join(format!("{base_name}_merged.md"));
    let mut merged = String::new();
    let mut current_page: Option<u32> = None;
    let mut page_chunks: Vec<String> = Vec::new();
    let mut first_section = true;

    for pf in &page_files {
        if current_page.is_none() {
            current_page = Some(pf.page);
        } else if Some(pf.page) != current_page {
            flush_page(
                &mut merged,
                current_page.unwrap(),
                &page_chunks,
                add_page_heading,
                &mut first_section,
            );
            page_chunks.clear();
            current_page = Some(pf.page);
        }
        let content = fs::read_to_string(&pf.path)
            .map_err(|e| format!("{}: 読み込み失敗: {e}", pf.path.display()))?;
        page_chunks.push(content.trim().to_string());
    }

    // 最後のページをフラッシュ
    if let Some(page) = current_page {
        flush_page(
            &mut merged,
            page,
            &page_chunks,
            add_page_heading,
            &mut first_section,
        );
    }

    fs::write(&output_path, &merged)
        .map_err(|e| format!("merged.md 書き込み失敗: {e}"))?;

    // ページ単位の md を削除
    for pf in &page_files {
        let _ = fs::remove_file(&pf.path);
    }

    Ok(output_path)
}

#[derive(Debug)]
struct PageFile {
    page: u32,
    part: u32,
    path: PathBuf,
}

fn collect_page_files(dir: &Path) -> Result<Vec<PageFile>, String> {
    let pattern = Regex::new(r"(?:.*_)?page_?(\d+)(?:_p(\d+))?\.md$").unwrap();
    let mut files: Vec<PageFile> = Vec::new();

    let entries = fs::read_dir(dir)
        .map_err(|e| format!("ディレクトリ読み込み失敗 {}: {e}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("エントリ読み込み失敗: {e}"))?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // merged.md は除外
        if name.contains("merged") {
            continue;
        }

        if let Some(caps) = pattern.captures(name) {
            let page: u32 = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
            let part: u32 = caps
                .get(2)
                .map(|m| m.as_str().parse().unwrap_or(0))
                .unwrap_or(0);
            files.push(PageFile { page, part, path });
        }
    }

    files.sort_by(|a, b| a.page.cmp(&b.page).then(a.part.cmp(&b.part)));
    Ok(files)
}

fn flush_page(
    output: &mut String,
    page: u32,
    chunks: &[String],
    add_heading: bool,
    first_section: &mut bool,
) {
    let page_text: String = chunks
        .iter()
        .filter(|c| !c.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");

    if page_text.trim().is_empty() {
        return;
    }

    if add_heading {
        if !*first_section {
            output.push('\n');
        }
        output.push_str(&format!("# Page {page}\n\n"));
        *first_section = false;
    }

    output.push_str(&page_text);
    output.push_str("\n\n");
}

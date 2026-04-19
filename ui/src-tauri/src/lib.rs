mod ollama;
mod ocr;
mod markdown;
mod export;
mod settings;
mod job;
mod paths;
mod cli;
mod results;
mod environment;

use settings::AppSettings;
use job::{
    AppState, JobInfo, JobStatus, RunOptions, CropRect,
    RunJobResponse, ProgressResponse, ResultResponse, RecentResultEntry,
    EnvironmentStatus, PreviewResponse,
};
use paths::{
    apply_python_env, default_gpu_device, resolve_python_entry, resolve_python_bin,
    resolve_config_dir, resolve_output_root, resolve_output_root_from_disk,
    resolve_project_root,
};
use results::{
    collect_output_files, find_output_path, open_path_with_default_app,
    validate_result_dir_name, canonicalize_dir, pick_best_file_in_dir,
};

use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
    thread,
    time::Instant,
};
use tauri::{Manager, State};
use tauri_plugin_dialog;
use uuid::Uuid;

pub use cli::run_cli_if_requested;

pub(crate) fn load_settings_from_disk(project_root: &std::path::Path) -> Result<AppSettings, String> {
    let config_dir = resolve_config_dir(project_root);
    settings::load_settings_from_disk(project_root, &config_dir)
}

// default_gpu_device は paths.rs に移動済み

fn apply_window_settings(app: &tauri::AppHandle, project_root: &std::path::Path) {
    let settings = load_settings_from_disk(project_root).ok();
    let width = settings
        .as_ref()
        .and_then(|s| s.window_width)
        .unwrap_or(1200)
        .max(720);
    let height = settings
        .as_ref()
        .and_then(|s| s.window_height)
        .unwrap_or(760)
        .max(540);

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize::new(
            720.0, 540.0,
        ))));
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            width as f64,
            height as f64,
        )));
    }
}

/// Ollama (GLM-OCR) ベースの OCR ジョブ実行コマンド。
/// 既存の run_job（Python subprocess）と並行して使用可能。
#[tauri::command]
async fn run_job_ollama(
    paths: Vec<String>,
    options: Option<RunOptions>,
    state: State<'_, Arc<AppState>>,
) -> Result<RunJobResponse, String> {
    if paths.is_empty() {
        return Err("入力ファイルがありません".into());
    }

    let exe_dir = std::env::current_exe().map_err(|e| format!("exe path 取得失敗: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("project root 解決失敗")?;
    let settings = load_settings_from_disk(&project_root).ok();
    let output_root = resolve_output_root(&project_root, settings.as_ref());
    fs::create_dir_all(&output_root)
        .map_err(|e| format!("出力ディレクトリ作成失敗: {e}"))?;

    let opts = options.unwrap_or_else(|| RunOptions {
        formats: vec!["md".into()],
        image_as_pdf: false,
        enable_figure: true,
        use_gpu: false,
        mode: String::new(),
        docx_engine: None,
        chunk_size: None,
        enable_rest: false,
        rest_seconds: None,
        pdf_dpi: None,
        excel_mode: None,
        excel_meta_sheet: None,
        excel_symbol_fallback: None,
        file_options: None,
    });

    let job_id = Uuid::new_v4().to_string();
    {
        let mut jobs = state.jobs.lock().map_err(|e| format!("lock: {e}"))?;
        jobs.insert(
            job_id.clone(),
            JobInfo {
                status: JobStatus::Running,
                progress: 0.0,
                log: vec!["Ollama OCR ジョブ開始".into()],
                outputs: vec![],
                output_paths: vec![],
                preview: None,
                error: None,
                current_message: Some("Ollama に接続中...".into()),
                page_current: None,
                page_total: None,
                eta_seconds: None,
            },
        );
    }

    let state_arc: Arc<AppState> = state.inner().clone();
    let job_id_clone = job_id.clone();
    let dpi = opts.pdf_dpi.unwrap_or(300);
    let enable_figure = opts.enable_figure;
    let formats = opts.formats.clone();
    let file_options = opts.file_options.clone();
    let python_bin = resolve_python_bin(&project_root);
    let project_root_clone = project_root.clone();

    // バックグラウンドスレッドで OCR 実行
    tokio::spawn(async move {
        let total_files = paths.len();

        for (file_idx, input_path_str) in paths.iter().enumerate() {
            let input_path = std::path::PathBuf::from(input_path_str);
            let stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");

            // file_options からページ範囲を取得
            let (start_page, end_page) = file_options
                .as_ref()
                .and_then(|m| m.get(input_path_str))
                .map(|fo| (fo.start, fo.end))
                .unwrap_or((None, None));

            // ページ範囲指定があればディレクトリ名にサフィックスを付加
            let dir_name = match (start_page, end_page) {
                (Some(s), Some(e)) => format!("{stem}_p{s}-{e}"),
                (Some(s), None) => format!("{stem}_p{s}-"),
                (None, Some(e)) => format!("{stem}_p1-{e}"),
                (None, None) => stem.to_string(),
            };
            let result_dir = output_root.join(&dir_name);

            let detect_script = resolve_python_entry(&project_root_clone, "detect_figures.py");
            let ocr_options = ocr::pipeline::OcrOptions {
                ocr_model: "glm-ocr".to_string(),
                dpi,
                poppler_path: None,
                enable_figure,
                python_bin: Some(python_bin.clone()),
                detect_figures_script: Some(detect_script),
                start_page,
                end_page,
            };

            // 進捗コールバック
            let state_cb = state_arc.clone();
            let job_id_cb = job_id_clone.clone();
            let file_start = (file_idx as f32 / total_files as f32) * 100.0;
            let file_range = 100.0 / total_files as f32;
            let on_progress: ocr::pipeline::ProgressCallback =
                Box::new(move |current, total, msg| {
                    if let Ok(mut jobs) = state_cb.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_cb) {
                            let ocr_ratio = current as f32 / total.max(1) as f32;
                            job.progress = file_start + file_range * 0.90 * ocr_ratio;
                            job.current_message = Some(msg.to_string());
                            job.page_current = Some(current);
                            job.page_total = Some(total);
                        }
                    }
                });

            // OCR 実行
            match ocr::pipeline::run_ocr_pipeline(
                &input_path,
                &result_dir,
                &ocr_options,
                Some(&on_progress),
            )
            .await
            {
                Ok(_md_paths) => {
                    // Markdown マージ
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_clone) {
                            job.progress = file_start + file_range * 0.92;
                            job.current_message = Some("Markdown マージ中...".into());
                        }
                    }

                    let _ = markdown::merge_page_markdowns(&result_dir, &dir_name, true);

                    let merged_md = result_dir.join(format!("{dir_name}_merged.md"));

                    // docx エクスポート（Python 呼び出し）
                    if formats.iter().any(|f| f == "docx") {
                        if let Ok(mut jobs) = state_arc.jobs.lock() {
                            if let Some(job) = jobs.get_mut(&job_id_clone) {
                                job.progress = file_start + file_range * 0.94;
                                job.current_message =
                                    Some("docx 変換中...".into());
                            }
                        }

                        if merged_md.exists() {
                            let export_script =
                                resolve_python_entry(&project_root_clone, "export_docx.py");
                            if export_script.exists() {
                                let mut cmd = Command::new(&python_bin);
                                apply_python_env(&mut cmd);
                                cmd.arg(&export_script).arg(&merged_md);
                                let _ = cmd.status();
                            }
                        }
                    }

                    // xlsx / csv エクスポート（Markdown テーブル → xlsx + csv）
                    let want_xlsx = formats.iter().any(|f| f == "xlsx");
                    let want_csv = formats.iter().any(|f| f == "csv");
                    if (want_xlsx || want_csv) && merged_md.exists() {
                        if let Ok(mut jobs) = state_arc.jobs.lock() {
                            if let Some(job) = jobs.get_mut(&job_id_clone) {
                                job.progress = file_start + file_range * 0.97;
                                job.current_message =
                                    Some("xlsx/csv 変換中...".into());
                            }
                        }

                        let xlsx_path = result_dir.join(format!("{dir_name}.xlsx"));
                        let export_script =
                            resolve_python_entry(&project_root_clone, "export_excel_poc.py");
                        if export_script.exists() {
                            let mut cmd = Command::new(&python_bin);
                            apply_python_env(&mut cmd);
                            cmd.arg(&export_script)
                                .arg(&merged_md)
                                .arg(&xlsx_path)
                                .arg("--format").arg("markdown");
                            if want_csv {
                                cmd.arg("--csv-dir").arg(&result_dir);
                            }
                            let _ = cmd.status();
                            // xlsx が不要なら削除
                            if !want_xlsx {
                                let _ = fs::remove_file(&xlsx_path);
                            }
                        }
                    }

                    // 出力ファイル収集
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_clone) {
                            let mut outputs = Vec::new();
                            let mut output_paths = Vec::new();
                            if let Ok(entries) = fs::read_dir(&result_dir) {
                                for entry in entries.flatten() {
                                    let path = entry.path();
                                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                        if name.ends_with("_merged.md")
                                            || name.ends_with("_merged.docx")
                                            || name.ends_with(".xlsx")
                                            || name.ends_with(".csv")
                                        {
                                            outputs.push(name.to_string());
                                            output_paths
                                                .push(path.to_string_lossy().to_string());
                                        }
                                    }
                                }
                            }

                            // プレビュー（merged.md の内容）
                            let merged_md_preview = result_dir.join(format!("{dir_name}_merged.md"));
                            let preview = fs::read_to_string(&merged_md_preview).ok();

                            job.outputs = outputs;
                            job.output_paths = output_paths;
                            job.preview = preview;
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_clone) {
                            job.status = JobStatus::Error;
                            job.error = Some(e);
                            job.current_message = Some("エラーが発生しました".into());
                        }
                    }
                    return;
                }
            }
        }

        // 完了
        if let Ok(mut jobs) = state_arc.jobs.lock() {
            if let Some(job) = jobs.get_mut(&job_id_clone) {
                if job.status != JobStatus::Error {
                    job.status = JobStatus::Done;
                    job.progress = 100.0;
                    job.current_message = Some("完了".into());
                }
            }
        }
    });

    Ok(RunJobResponse { job_id })
}

#[tauri::command]
fn run_job(
    paths: Vec<String>,
    options: Option<RunOptions>,
    cleanup_paths: Option<Vec<String>>,
    state: State<Arc<AppState>>,
) -> Result<RunJobResponse, String> {
    if paths.is_empty() {
        return Err("no input files".into());
    }

    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let dispatcher = resolve_python_entry(&project_root, "dispatcher.py");
    if !dispatcher.exists() {
        return Err(format!(
            "dispatcher.py not found at {}",
            dispatcher.display()
        ));
    }

    let settings = load_settings_from_disk(&project_root).ok();
    let output_root = resolve_output_root(&project_root, settings.as_ref());
    if let Err(e) = fs::create_dir_all(&output_root) {
        return Err(format!(
            "failed to create output root {}: {e}",
            output_root.display()
        ));
    }

    let python_bin = resolve_python_bin(&project_root);

    let job_id = Uuid::new_v4().to_string();
    {
        let mut jobs = state
            .jobs
            .lock()
            .map_err(|e| format!("lock poisoned: {e}"))?;
        jobs.insert(
            job_id.clone(),
            JobInfo {
                status: JobStatus::Running,
                progress: 0.0,
                log: vec!["job started".into()],
                outputs: vec![],
                output_paths: vec![],
                preview: None,
                error: None,
                current_message: None,
                page_current: None,
                page_total: None,
                eta_seconds: None,
            },
        );
    }

    let state_arc: Arc<AppState> = state.inner().clone();
    let dispatcher_path = dispatcher.clone();
    let (
        formats,
        image_as_pdf,
        enable_figure,
        use_gpu,
        mode,
        docx_engine,
        chunk_size,
        enable_rest,
        rest_seconds,
        pdf_dpi,
        excel_mode,
        excel_meta_sheet,
        excel_symbol_fallback,
        file_opts_map,
    ) = match options {
        Some(o) => (
            o.formats,
            o.image_as_pdf,
            o.enable_figure,
            o.use_gpu,
            Some(o.mode),
            o.docx_engine,
            o.chunk_size,
            o.enable_rest,
            o.rest_seconds,
            o.pdf_dpi,
            o.excel_mode,
            o.excel_meta_sheet,
            o.excel_symbol_fallback,
            o.file_options,
        ),
        None => (
            vec!["md".into()],
            false,
            true,
            false,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
        ),
    };
    let python_bin_cloned = python_bin.clone();
    let project_root_cloned = project_root.clone();
    let output_root_cloned = output_root.clone();
    let paths_cloned = paths.clone();
    let job_id_cloned = job_id.clone();
    let cleanup_paths_cloned = cleanup_paths.unwrap_or_default();

    thread::spawn(move || {
        let mut outputs = Vec::new();
        let paths_len = paths_cloned.len();
        for (idx, p) in paths_cloned.iter().enumerate() {
            let mut cmd = Command::new(&python_bin_cloned);
            apply_python_env(&mut cmd);
            cmd.env("OCR_TO_DOC_CLEANUP_IMAGES", "1");
            // Force unbuffered output for Python
            cmd.arg("-u");

            cmd.arg(&dispatcher_path).arg(p);

            // Global args
            if !formats.is_empty() {
                cmd.arg("--formats");
                for fmt in &formats {
                    cmd.arg(fmt);
                }
            }
            cmd.arg("--output-root").arg(&output_root_cloned);
            if let Some(engine) = &docx_engine {
                if !engine.is_empty() {
                    cmd.arg("--docx-engine").arg(engine);
                }
            }
            if let Some(em) = &excel_mode {
                if !em.is_empty() {
                    cmd.arg("--excel-mode").arg(em);
                }
            }
            if let Some(v) = excel_meta_sheet {
                if v {
                    cmd.arg("--excel-meta");
                } else {
                    cmd.arg("--no-excel-meta");
                }
            }
            if let Some(v) = excel_symbol_fallback {
                if v {
                    cmd.arg("--excel-symbol-fallback");
                } else {
                    cmd.arg("--no-excel-symbol-fallback");
                }
            }
            if image_as_pdf {
                cmd.arg("--image-as-pdf");
            }
            if enable_figure {
                cmd.arg("--figure");
            } else {
                cmd.arg("--no-figure");
            }
            cmd.arg("--device")
                .arg(if use_gpu { default_gpu_device() } else { "cpu" });
            if let Some(m) = &mode {
                cmd.arg("--mode").arg(m);
            }

            // File specific options (Crop) - dispatcher の通常引数として渡す
            if let Some(opts_map) = &file_opts_map {
                if let Some(f_opts) = opts_map.get(p) {
                    if let Some(crop) = &f_opts.crop {
                        cmd.arg("--crop").arg(format!(
                            "{:.6},{:.6},{:.6},{:.6}",
                            crop.left, crop.top, crop.width, crop.height
                        ));
                    }
                }
            }

            // Extra args (passed to ocr_chanked.py via --)
            // Collect all extra args first
            let mut extra_args = Vec::new();

            // Stability settings
            if let Some(cs) = chunk_size {
                extra_args.push(format!("--chunk-size"));
                extra_args.push(cs.to_string());
            }
            if let Some(dpi) = pdf_dpi {
                extra_args.push("--dpi".into());
                extra_args.push(dpi.to_string());
            }
            if enable_rest {
                extra_args.push("--enable-rest".into());
            }
            if let Some(rs) = rest_seconds {
                if enable_rest {
                    extra_args.push(format!("--rest-seconds"));
                    extra_args.push(rs.to_string());
                }
            }

            // File specific options (Page range)
            if let Some(opts_map) = &file_opts_map {
                if let Some(f_opts) = opts_map.get(p) {
                    // Match by full path string
                    if let Some(s) = f_opts.start {
                        extra_args.push("--start".into());
                        extra_args.push(s.to_string());
                    }
                    if let Some(e) = f_opts.end {
                        extra_args.push("--end".into());
                        extra_args.push(e.to_string());
                    }
                }
            }

            if !extra_args.is_empty() {
                cmd.arg("--");
                for arg in extra_args {
                    cmd.arg(arg);
                }
            }

            cmd.current_dir(&project_root_cloned);

            // Pipe output to read in real-time
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let log_line = format!("spawn: {:?}", cmd);
            if let Ok(mut jobs) = state_arc.jobs.lock() {
                if let Some(job) = jobs.get_mut(&job_id_cloned) {
                    job.log.push(log_line.clone());
                    // Start of this file processing
                    let base_progress = (idx as f32) / paths_len as f32 * 100.0;
                    job.progress = base_progress.min(99.0);
                }
            }

            match cmd.spawn() {
                Ok(mut child) => {
                    let stdout = child.stdout.take().expect("failed to get stdout");
                    let stderr = child.stderr.take().expect("failed to get stderr");

                    // Clone state for threads
                    let state_out = state_arc.clone();
                    let job_id_out = job_id_cloned.clone();

                    // Stdout reader thread
                    let stdout_handle = thread::spawn(move || {
                        use std::collections::VecDeque;
                        use std::io::{BufRead, BufReader};
                        let reader = BufReader::new(stdout);
                        let mut range_start: Option<u32> = None;
                        let mut range_end: Option<u32> = None;
                        let mut page_started_at: Option<Instant> = None;
                        let mut recent_secs: VecDeque<f32> = VecDeque::new();
                        const ETA_WINDOW: usize = 5;

                        let parse_range = |line: &str| -> Option<(u32, u32)> {
                            let prefix = "処理範囲:";
                            let rest = line.strip_prefix(prefix)?.trim();
                            let mut parts = rest.split('〜');
                            let start = parts.next()?.trim().parse::<u32>().ok()?;
                            let end = parts.next()?.trim().parse::<u32>().ok()?;
                            Some((start, end))
                        };

                        let parse_page_marker = |line: &str, marker: &str| -> Option<(u32, u32)> {
                            // e.g. "--- Page 3/9 (abs 3/12) ---" / "--- Done 3/9 ---"
                            let start = format!("--- {marker} ");
                            let rest = line.strip_prefix(&start)?;
                            let head = rest.split_whitespace().next()?; // "3/9"
                            let mut parts = head.split('/');
                            let cur = parts.next()?.parse::<u32>().ok()?;
                            let total = parts.next()?.parse::<u32>().ok()?;
                            Some((cur, total))
                        };

                        for line in reader.lines() {
                            if let Ok(l) = line {
                                if let Ok(mut jobs) = state_out.jobs.lock() {
                                    if let Some(job) = jobs.get_mut(&job_id_out) {
                                        job.log.push(l.clone());

                                        let file_start = (idx as f32) / paths_len as f32 * 100.0;
                                        let file_end =
                                            ((idx as f32) + 1.0) / paths_len as f32 * 100.0;
                                        let file_span = (file_end - file_start).max(1.0);

                                        if let Some((s, e)) = parse_range(&l) {
                                            range_start = Some(s);
                                            range_end = Some(e);
                                            let total = e.saturating_sub(s).saturating_add(1);
                                            job.page_total = Some(total);
                                            job.eta_seconds = None;
                                        }

                                        if let Some((cur, total_in_run)) =
                                            parse_page_marker(&l, "Page")
                                        {
                                            job.page_current = Some(cur);
                                            job.page_total = Some(total_in_run);
                                            job.current_message = Some(format!(
                                                "PDF変換中: {cur}/{total_in_run}ページ"
                                            ));
                                            job.eta_seconds = None;
                                            page_started_at = Some(Instant::now());
                                        }

                                        if let Some((cur, total_in_run)) =
                                            parse_page_marker(&l, "Done")
                                        {
                                            if let Some(started) = page_started_at.take() {
                                                let secs = started.elapsed().as_secs_f32();
                                                if secs.is_finite() && secs > 0.0 {
                                                    recent_secs.push_back(secs);
                                                    while recent_secs.len() > ETA_WINDOW {
                                                        recent_secs.pop_front();
                                                    }
                                                }
                                            }

                                            job.page_current = Some(cur);
                                            job.page_total = Some(total_in_run);

                                            let (start_page, end_page) =
                                                match (range_start, range_end) {
                                                    (Some(s), Some(e)) => (s, e),
                                                    _ => (1, total_in_run),
                                                };
                                            let total_pages = end_page
                                                .saturating_sub(start_page)
                                                .saturating_add(1)
                                                .max(1);
                                            let done_pages = cur
                                                .saturating_sub(start_page)
                                                .saturating_add(1)
                                                .min(total_pages);
                                            let remaining_pages = end_page.saturating_sub(cur);

                                            let ocr_ratio = done_pages as f32 / total_pages as f32;
                                            let target_progress =
                                                file_start + file_span * (0.90 * ocr_ratio);
                                            if target_progress.is_finite()
                                                && target_progress > job.progress
                                            {
                                                job.progress = target_progress.min(99.0);
                                            }

                                            if !recent_secs.is_empty() && remaining_pages > 0 {
                                                let avg = recent_secs.iter().copied().sum::<f32>()
                                                    / recent_secs.len() as f32;
                                                if avg.is_finite() && avg > 0.0 {
                                                    job.eta_seconds = Some(
                                                        (avg * remaining_pages as f32).round()
                                                            as u32,
                                                    );
                                                }
                                            } else {
                                                job.eta_seconds = None;
                                            }

                                            job.current_message = Some(format!(
                                                "PDF変換中: {cur}/{total_in_run}ページ"
                                            ));
                                        }

                                        if l.contains("--- merged_md.py を実行 ---")
                                            || l.contains("--- postprocess.py を実行 ---")
                                        {
                                            job.current_message =
                                                Some("後処理: Markdown結合中".into());
                                            job.eta_seconds = None;
                                            let target = file_start + file_span * 0.92;
                                            if target > job.progress {
                                                job.progress = target.min(99.0);
                                            }
                                        }
                                        if l.contains("[dispatcher] Converting to docx") {
                                            job.current_message = Some("後処理: Word変換中".into());
                                            job.eta_seconds = None;
                                            let target = file_start + file_span * 0.96;
                                            if target > job.progress {
                                                job.progress = target.min(99.0);
                                            }
                                        }
                                        if l.contains("[dispatcher] processing excel_via=json") {
                                            job.current_message =
                                                Some("後処理: Excel変換中".into());
                                            job.eta_seconds = None;
                                            let target = file_start + file_span * 0.99;
                                            if target > job.progress {
                                                job.progress = target.min(99.0);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });

                    // Stderr reader thread
                    let state_err = state_arc.clone();
                    let job_id_err = job_id_cloned.clone();
                    let stderr_handle = thread::spawn(move || {
                        use std::io::{BufRead, BufReader};
                        let reader = BufReader::new(stderr);
                        for line in reader.lines() {
                            if let Ok(l) = line {
                                if let Ok(mut jobs) = state_err.jobs.lock() {
                                    if let Some(job) = jobs.get_mut(&job_id_err) {
                                        job.log.push(format!("[err] {}", l));
                                    }
                                }
                            }
                        }
                    });

                    // Wait for finish
                    let status = child.wait();
                    stdout_handle.join().unwrap_or(());
                    stderr_handle.join().unwrap_or(());

                    match status {
                        Ok(s) if s.success() => {
                            if let Ok(mut jobs) = state_arc.jobs.lock() {
                                if let Some(job) = jobs.get_mut(&job_id_cloned) {
                                    job.progress =
                                        ((idx as f32 + 1.0) / paths_len as f32 * 100.0).min(100.0);
                                }
                            }
                            outputs.push(p.clone());
                        }
                        Ok(_) => {
                            if let Ok(mut jobs) = state_arc.jobs.lock() {
                                if let Some(job) = jobs.get_mut(&job_id_cloned) {
                                    job.status = JobStatus::Error;
                                    job.error =
                                        Some("dispatcher failed (non-zero exit code)".into());
                                }
                            }
                            return;
                        }
                        Err(e) => {
                            if let Ok(mut jobs) = state_arc.jobs.lock() {
                                if let Some(job) = jobs.get_mut(&job_id_cloned) {
                                    job.status = JobStatus::Error;
                                    job.error = Some(format!("failed to spawn python: {e}"));
                                }
                            }
                            return;
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_cloned) {
                            job.status = JobStatus::Error;
                            job.error = Some(format!("failed to spawn python: {e}"));
                        }
                    }
                    return;
                }
            }
        }

        // set done
        if let Ok(mut jobs) = state_arc.jobs.lock() {
            if let Some(job) = jobs.get_mut(&job_id_cloned) {
                job.status = JobStatus::Done;
                job.progress = 100.0;
                let output_files = collect_output_files(
                    &output_root_cloned,
                    &project_root_cloned,
                    &paths_cloned,
                    &formats,
                );
                job.outputs = output_files
                    .iter()
                    .map(|p| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                job.output_paths = output_files
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();

                // Markdownプレビュー: 最初に見つかった md を読む
                if let Some(md_path) = output_files
                    .iter()
                    .find(|p| p.extension().map(|e| e == "md").unwrap_or(false))
                {
                    if let Ok(content) = fs::read_to_string(md_path) {
                        job.preview = Some(content);
                    } else {
                        job.preview = Some(format!(
                            "failed to read markdown preview: {}",
                            md_path.display()
                        ));
                    }
                } else {
                    job.preview = Some(format!(
                        "Converted markdown for: {} (md preview not found)",
                        outputs.join(", ")
                    ));
                }
            }
        }

        if !cleanup_paths_cloned.is_empty() {
            for path in cleanup_paths_cloned {
                if let Err(e) = fs::remove_file(&path) {
                    if let Ok(mut jobs) = state_arc.jobs.lock() {
                        if let Some(job) = jobs.get_mut(&job_id_cloned) {
                            job.log.push(format!("cleanup failed: {path} ({e})"));
                        }
                    }
                }
            }
        }
    });

    Ok(RunJobResponse { job_id })
}

fn sanitize_extension(extension: Option<String>) -> String {
    let fallback = "png".to_string();
    let ext = extension.unwrap_or_default().to_lowercase();
    let trimmed = ext.trim().trim_start_matches('.');
    let clean: String = trimmed.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if clean.is_empty() {
        fallback
    } else {
        clean
    }
}

#[tauri::command]
fn save_clipboard_image(data: Vec<u8>, extension: Option<String>) -> Result<String, String> {
    if data.is_empty() {
        return Err("empty clipboard image data".into());
    }
    let ext = sanitize_extension(extension);
    let dir = std::env::temp_dir().join("ocr_to_doc");
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create temp dir: {e}"))?;
    let filename = format!("clipboard_{}.{}", Uuid::new_v4(), ext);
    let path = dir.join(filename);
    fs::write(&path, data).map_err(|e| format!("failed to write clipboard image: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn render_preview(
    path: String,
    page: Option<u32>,
    crop: Option<CropRect>,
    max_long_edge: Option<u32>,
    pdf_dpi: Option<u32>,
) -> Result<PreviewResponse, String> {
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let python_bin = resolve_python_bin(&project_root);

    let helper = resolve_python_entry(&project_root, "ui_preview.py");
    if !helper.exists() {
        return Err(format!("ui_preview.py not found at {}", helper.display()));
    }

    let mut cmd = Command::new(&python_bin);
    apply_python_env(&mut cmd);
    cmd.arg("-u")
        .arg(helper)
        .arg("--input")
        .arg(&path)
        .arg("--page")
        .arg(page.unwrap_or(1).to_string());

    if let Some(c) = crop {
        cmd.arg("--crop").arg(format!(
            "{:.6},{:.6},{:.6},{:.6}",
            c.left, c.top, c.width, c.height
        ));
    }
    if let Some(max_le) = max_long_edge {
        cmd.arg("--max-long-edge").arg(max_le.to_string());
    }
    if let Some(dpi) = pdf_dpi {
        cmd.arg("--pdf-dpi").arg(dpi.to_string());
    }

    cmd.current_dir(&project_root);

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run preview helper: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("preview helper failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    serde_json::from_str::<PreviewResponse>(&stdout)
        .map_err(|e| format!("failed to parse preview helper output: {e}"))
}

// resolve_python_entry, resolve_python_bin は paths.rs に移動済み

#[tauri::command]
fn get_progress(job_id: String, state: State<Arc<AppState>>) -> Result<ProgressResponse, String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    if let Some(job) = jobs.get(&job_id) {
        return Ok(ProgressResponse {
            status: job.status.clone(),
            progress: job.progress,
            log: job.log.clone(),
            error: job.error.clone(),
            current_message: job.current_message.clone(),
            page_current: job.page_current,
            page_total: job.page_total,
            eta_seconds: job.eta_seconds,
        });
    }
    Err("job not found".into())
}

// collect_output_files, find_output_path 等は results.rs に移動済み

#[tauri::command]
fn get_result(job_id: String, state: State<Arc<AppState>>) -> Result<ResultResponse, String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;
    if let Some(job) = jobs.get(&job_id) {
        return Ok(ResultResponse {
            outputs: job.outputs.clone(),
            preview: job.preview.clone(),
        });
    }
    Err("job not found".into())
}

#[tauri::command]
fn save_file(
    job_id: String,
    filename: String,
    dest_path: String,
    state: State<Arc<AppState>>,
) -> Result<(), String> {
    println!(
        "[save_file] called with job_id={}, filename={}, dest_path={}",
        job_id, filename, dest_path
    );
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;

    if let Some(job) = jobs.get(&job_id) {
        println!("[save_file] job found. outputs={:?}", job.outputs);
        // filename が outputs に含まれているか確認 (セキュリティ対策)
        if !job.outputs.contains(&filename) {
            println!("[save_file] filename not in outputs");
            return Err(format!("file not found in job outputs: {}", filename));
        }

        if let Some(index) = job.outputs.iter().position(|name| name == &filename) {
            if let Some(path) = job.output_paths.get(index) {
                let src = PathBuf::from(path);
                if src.exists() {
                    fs::copy(&src, &dest_path).map_err(|e| format!("failed to copy file: {e}"))?;
                    return Ok(());
                }
            }
        }

        // fallback: 元ファイルを探す
        let exe_dir =
            std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
        let project_root =
            resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
        let result_root = resolve_output_root_from_disk(&project_root);
        if let Some(src) = find_output_path(&result_root, &project_root, &filename) {
            fs::copy(&src, &dest_path).map_err(|e| format!("failed to copy file: {e}"))?;
            return Ok(());
        }
        return Err(format!("source file not found: {}", filename));
    }
    Err("job not found".into())
}

#[tauri::command]
fn open_output(
    job_id: String,
    filename: String,
    state: State<Arc<AppState>>,
) -> Result<(), String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;

    let job = jobs.get(&job_id).ok_or("job not found")?;
    // filename が outputs に含まれているか確認 (セキュリティ対策)
    if !job.outputs.contains(&filename) {
        return Err(format!("file not found in job outputs: {}", filename));
    }

    if let Some(index) = job.outputs.iter().position(|name| name == &filename) {
        if let Some(path) = job.output_paths.get(index) {
            let src = PathBuf::from(path);
            if src.exists() {
                return open_path_with_default_app(&src);
            }
        }
    }

    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let result_root = resolve_output_root_from_disk(&project_root);
    let src =
        find_output_path(&result_root, &project_root, &filename).ok_or("source file not found")?;
    open_path_with_default_app(&src)
}

#[tauri::command]
fn open_output_dir(job_id: String, state: State<Arc<AppState>>) -> Result<(), String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|e| format!("lock poisoned: {e}"))?;

    let job = jobs.get(&job_id).ok_or("job not found")?;

    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let result_root = resolve_output_root_from_disk(&project_root);

    if let Some(first_path) = job.output_paths.first() {
        let src = PathBuf::from(first_path);
        if let Some(parent) = src.parent() {
            if parent.exists() {
                return open_path_with_default_app(parent);
            }
        }
    }

    if let Some(first) = job.outputs.first() {
        if let Some(src) = find_output_path(&result_root, &project_root, first) {
            if let Some(parent) = src.parent() {
                return open_path_with_default_app(parent);
            }
        }
    }

    // 出力が見つからない場合は result フォルダを開く
    open_path_with_default_app(&result_root)
}

#[tauri::command]
fn open_input_file(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is empty".into());
    }
    let p = PathBuf::from(trimmed);
    if !p.exists() {
        return Err(format!("file not found: {}", p.display()));
    }
    open_path_with_default_app(&p)
}

#[tauri::command]
fn open_readme() -> Result<(), String> {
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;

    let candidates = ["readme.md", "README.md"];
    for name in candidates {
        let path = project_root.join(name);
        if path.exists() {
            return open_path_with_default_app(&path);
        }
    }
    Err("README not found".into())
}

#[tauri::command]
fn list_recent_results(limit: Option<u32>) -> Result<Vec<RecentResultEntry>, String> {
    results::list_recent_results(limit)
}

#[tauri::command]
fn open_result_dir(dir_name: String) -> Result<(), String> {
    validate_result_dir_name(&dir_name)?;
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;

    let result_root = resolve_output_root_from_disk(&project_root);
    let dir_path = result_root.join(&dir_name);
    if !dir_path.is_dir() {
        return Err("result dir not found".into());
    }

    let result_root_canon = canonicalize_dir(&result_root)?;
    let dir_canon = canonicalize_dir(&dir_path)?;
    if !dir_canon.starts_with(&result_root_canon) {
        return Err("invalid result dir".into());
    }

    open_path_with_default_app(&dir_canon)
}

#[tauri::command]
fn open_result_file(dir_name: String) -> Result<(), String> {
    validate_result_dir_name(&dir_name)?;
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;

    let result_root = resolve_output_root_from_disk(&project_root);
    let dir_path = result_root.join(&dir_name);
    if !dir_path.is_dir() {
        return Err("result dir not found".into());
    }

    let result_root_canon = canonicalize_dir(&result_root)?;
    let dir_canon = canonicalize_dir(&dir_path)?;
    if !dir_canon.starts_with(&result_root_canon) {
        return Err("invalid result dir".into());
    }

    let best = pick_best_file_in_dir(&dir_canon, &dir_name).ok_or("no output file found")?;
    let file_path = dir_canon.join(&best);
    let file_canon = canonicalize_dir(&file_path)?;
    if !file_canon.starts_with(&dir_canon) {
        return Err("invalid output file".into());
    }
    open_path_with_default_app(&file_canon)
}

#[tauri::command]
async fn check_environment() -> Result<EnvironmentStatus, String> {
    environment::check_environment().await
}

#[tauri::command]
fn load_settings() -> Result<AppSettings, String> {
    let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?;
    let project_root = resolve_project_root(&exe_dir).unwrap_or_else(|| PathBuf::from("."));
    load_settings_from_disk(&project_root)
}

#[tauri::command]
fn save_settings(settings: AppSettings) -> Result<(), String> {
    let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?;
    let project_root = resolve_project_root(&exe_dir).unwrap_or_else(|| PathBuf::from("."));
    let config_dir = resolve_config_dir(&project_root);
    settings::save_settings_to_disk(&settings, &config_dir)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            run_job,
            run_job_ollama,
            save_clipboard_image,
            render_preview,
            get_progress,
            get_result,
            save_file,
            open_output,
            open_output_dir,
            open_input_file,
            open_readme,
            list_recent_results,
            open_result_dir,
            open_result_file,
            check_environment,
            load_settings,
            save_settings
        ])
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?;
            if let Some(project_root) = resolve_project_root(&exe_dir) {
                apply_window_settings(app.handle(), &project_root);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tauri_plugin_dialog;
use uuid::Uuid;

const APP_SUPPORT_DIR_NAME: &str = "ocr-to-doc";

fn apply_python_env(cmd: &mut Command) {
    // Prevent UnicodeEncodeError on Windows where stdout/stderr encoding can be non-UTF-8 (e.g. cp1252).
    cmd.env("PYTHONUTF8", "1");
    cmd.env("PYTHONIOENCODING", "utf-8");
}

pub fn run_cli_if_requested() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let is_self_test = args.iter().any(|a| a == "--self-test");
    let cli_index = args.iter().position(|a| a == "--cli");
    if !is_self_test && cli_index.is_none() {
        return None;
    }

    if let Some(idx) = cli_index {
        let mut input: Option<String> = None;
        let mut passthrough: Vec<String> = Vec::new();
        let mut i = idx + 1;
        while i < args.len() {
            if args[i] == "--" {
                passthrough.extend_from_slice(&args[i + 1..]);
                break;
            }
            if input.is_none() && !args[i].starts_with('-') {
                input = Some(args[i].clone());
            }
            i += 1;
        }
        let input = match input {
            Some(p) => p,
            None => {
                eprintln!("[cli] usage: ocr-to-doc.exe --cli <input> [-- <dispatcher args>]");
                return Some(2);
            }
        };

        let exe_path = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[cli] failed to get current_exe: {e}");
                return Some(2);
            }
        };
        let project_root = match resolve_project_root(&exe_path) {
            Some(p) => p,
            None => {
                eprintln!(
                    "[cli] failed to resolve project root from {}",
                    exe_path.display()
                );
                return Some(2);
            }
        };
        let python_bin = resolve_python_bin(&project_root);
        let dispatcher = resolve_python_entry(&project_root, "dispatcher.py");
        if !dispatcher.exists() {
            eprintln!("[cli] dispatcher not found: {}", dispatcher.display());
            return Some(2);
        }

        let settings = load_settings_from_disk(&project_root).ok();
        let output_root = resolve_output_root(&project_root, settings.as_ref());
        if let Err(e) = fs::create_dir_all(&output_root) {
            eprintln!(
                "[cli] failed to create output root {}: {e}",
                output_root.display()
            );
            return Some(2);
        }

        let mut cmd = Command::new(&python_bin);
        apply_python_env(&mut cmd);
        cmd.arg("-u")
            .arg(&dispatcher)
            .arg(&input)
            .arg("--output-root")
            .arg(&output_root);
        for a in passthrough {
            cmd.arg(a);
        }
        cmd.current_dir(&project_root);

        let status = match cmd.status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[cli] failed to run dispatcher: {e}");
                return Some(1);
            }
        };
        return Some(if status.success() { 0 } else { 1 });
    }

    let mut input_path: Option<String> = None;
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "--input" && i + 1 < args.len() {
            input_path = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        i += 1;
    }

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[self-test] failed to get current_exe: {e}");
            return Some(2);
        }
    };
    let project_root = match resolve_project_root(&exe_path) {
        Some(p) => p,
        None => {
            eprintln!(
                "[self-test] failed to resolve project root from {}",
                exe_path.display()
            );
            return Some(2);
        }
    };

    let output_root = project_root.join("result_ci");
    let trace_path = match fs::create_dir_all(&output_root) {
        Ok(_) => output_root.join("self_test.trace.txt"),
        Err(e) => {
            let fallback = project_root.join("self_test.trace.txt");
            let _ = fs::write(
                &fallback,
                format!("[self-test] failed to create result_ci: {e}\n"),
            );
            fallback
        }
    };
    let mut log_trace = |msg: &str| {
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&trace_path)
        {
            let _ = writeln!(f, "{msg}");
        }
    };
    log_trace(&format!(
        "[self-test] exe={} project_root={}",
        exe_path.display(),
        project_root.display()
    ));

    let python_bin = resolve_python_bin(&project_root);
    let dispatcher = resolve_python_entry(&project_root, "dispatcher.py");
    if !dispatcher.exists() {
        log_trace(&format!(
            "[self-test] dispatcher not found: {}",
            dispatcher.display()
        ));
        eprintln!("[self-test] dispatcher not found: {}", dispatcher.display());
        return Some(2);
    }

    let fixture = input_path.map(PathBuf::from).unwrap_or_else(|| {
        project_root
            .join("resources")
            .join("fixtures")
            .join("sample.png")
    });
    if !fixture.exists() {
        log_trace(&format!(
            "[self-test] input not found: {}",
            fixture.display()
        ));
        eprintln!("[self-test] input not found: {}", fixture.display());
        eprintln!("[self-test] hint: pass --input <path> or place resources/fixtures/sample.png");
        return Some(3);
    }

    let output_root_arg = output_root.to_string_lossy().to_string();

    let fixture_stem = fixture
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("fixture");
    eprintln!(
        "[self-test] project_root={} python={} input={} output_root={}",
        project_root.display(),
        python_bin,
        fixture.display(),
        output_root.display()
    );
    log_trace(&format!(
        "[self-test] python={} dispatcher={} input={} output_root={}",
        python_bin,
        dispatcher.display(),
        fixture.display(),
        output_root.display()
    ));

    let output = {
        let mut cmd = Command::new(&python_bin);
        apply_python_env(&mut cmd);
        cmd.arg("-u")
            .arg(&dispatcher)
            .arg(&fixture)
            .args(["--formats", "md", "docx"])
            .args(["--mode", "lite"])
            .args(["--device", "cpu"])
            .args(["--no-figure"])
            .args(["--output-root", &output_root_arg])
            .current_dir(&project_root);
        match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                log_trace(&format!("[self-test] failed to spawn dispatcher: {e}"));
                eprintln!("[self-test] failed to spawn dispatcher: {e}");
                return Some(1);
            }
        }
    };

    // GUI subsystem on Windows may not surface stdout/stderr; always persist for debugging.
    let _ = fs::write(
        output_root.join("self_test.stdout.txt"),
        String::from_utf8_lossy(&output.stdout).as_bytes(),
    );
    let _ = fs::write(
        output_root.join("self_test.stderr.txt"),
        String::from_utf8_lossy(&output.stderr).as_bytes(),
    );

    if !output.status.success() {
        log_trace(&format!(
            "[self-test] dispatcher failed with status={}",
            output.status
        ));
        eprintln!(
            "[self-test] dispatcher failed with status={}",
            output.status
        );
        eprintln!(
            "--- stdout ---\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Some(1);
    }

    let out_dir = output_root.join(fixture_stem);
    let md = out_dir.join("page_001.md");
    let docx = out_dir.join(format!("{fixture_stem}.docx"));

    if !md.exists() {
        log_trace(&format!(
            "[self-test] expected markdown not found: {}",
            md.display()
        ));
        eprintln!("[self-test] expected markdown not found: {}", md.display());
        return Some(4);
    }
    if !docx.exists() {
        log_trace(&format!(
            "[self-test] expected docx not found: {}",
            docx.display()
        ));
        eprintln!("[self-test] expected docx not found: {}", docx.display());
        return Some(4);
    }
    if md.metadata().map(|m| m.len()).unwrap_or(0) < 10 {
        log_trace(&format!(
            "[self-test] markdown looks too small: {}",
            md.display()
        ));
        eprintln!("[self-test] markdown looks too small: {}", md.display());
        return Some(4);
    }
    if docx.metadata().map(|m| m.len()).unwrap_or(0) < 1000 {
        log_trace(&format!(
            "[self-test] docx looks too small: {}",
            docx.display()
        ));
        eprintln!("[self-test] docx looks too small: {}", docx.display());
        return Some(4);
    }

    log_trace(&format!(
        "[self-test] ok: {} {}",
        md.display(),
        docx.display()
    ));
    eprintln!("[self-test] ok: {} {}", md.display(), docx.display());
    Some(0)
}

#[derive(Default)]
struct AppState {
    jobs: Mutex<HashMap<String, JobInfo>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JobInfo {
    status: JobStatus,
    progress: f32,
    log: Vec<String>,
    outputs: Vec<String>,
    output_paths: Vec<String>,
    preview: Option<String>,
    error: Option<String>,
    current_message: Option<String>,
    page_current: Option<u32>,
    page_total: Option<u32>,
    eta_seconds: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum JobStatus {
    Idle,
    Running,
    Done,
    Error,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunOptions {
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    image_as_pdf: bool,
    #[serde(default)]
    enable_figure: bool,
    #[serde(default)]
    use_gpu: bool,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    docx_engine: Option<String>,
    #[serde(default)]
    chunk_size: Option<u32>,
    #[serde(default)]
    enable_rest: bool,
    #[serde(default)]
    rest_seconds: Option<u32>,
    #[serde(default)]
    pdf_dpi: Option<u32>,
    #[serde(default)]
    excel_mode: Option<String>,
    #[serde(default)]
    excel_meta_sheet: Option<bool>,
    #[serde(default)]
    excel_symbol_fallback: Option<bool>,
    #[serde(default)]
    file_options: Option<HashMap<String, FileSpecificOptions>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CropRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FileSpecificOptions {
    start: Option<u32>,
    end: Option<u32>,
    crop: Option<CropRect>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunJobResponse {
    job_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressResponse {
    status: JobStatus,
    progress: f32,
    log: Vec<String>,
    error: Option<String>,
    current_message: Option<String>,
    page_current: Option<u32>,
    page_total: Option<u32>,
    eta_seconds: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultResponse {
    outputs: Vec<String>,
    preview: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentResultEntry {
    dir_name: String,
    updated_at_ms: u64,
    page_range: Option<String>,
    best_file: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentStatus {
    project_root: String,
    os: String,
    dispatcher_found: bool,
    dispatcher_path: Option<String>,
    result_dir_found: bool,
    result_root: String,
    python_bin: String,
    python_found: bool,
    python_path: Option<String>,
    poppler_found: bool,
    poppler_path: Option<String>,
    resource_roots: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewResponse {
    data_url: String,
    page_count: Option<u32>,
    page: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    image_as_pdf: bool,
    #[serde(default)]
    enable_figure: bool,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    docx_engine: Option<String>,
    #[serde(default)]
    excel_mode: Option<String>,
    #[serde(default)]
    use_gpu: bool,
    #[serde(default)]
    output_root: Option<String>,
    #[serde(default = "default_excel_meta_sheet")]
    excel_meta_sheet: bool,
    #[serde(default = "default_excel_symbol_fallback")]
    excel_symbol_fallback: bool,
    #[serde(default)]
    chunk_size: Option<u32>,
    #[serde(default)]
    enable_rest: bool,
    #[serde(default)]
    rest_seconds: Option<u32>,
    #[serde(default)]
    pdf_dpi: Option<u32>,
    #[serde(default)]
    window_width: Option<u32>,
    #[serde(default)]
    window_height: Option<u32>,
    #[serde(default = "default_preview_quality")]
    preview_quality: String,
}

fn default_excel_meta_sheet() -> bool {
    true
}

fn default_excel_symbol_fallback() -> bool {
    true
}

fn default_preview_quality() -> String {
    "light".to_string()
}

fn load_settings_from_disk(project_root: &std::path::Path) -> Result<AppSettings, String> {
    // Ensure configs directory exists
    let config_dir = resolve_config_dir(project_root);
    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }

    let settings_path = config_dir.join("settings.json");
    if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        let settings: AppSettings = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        Ok(settings)
    } else {
        let legacy_path = project_root.join("configs").join("settings.json");
        if legacy_path.exists() {
            let content = fs::read_to_string(&legacy_path).map_err(|e| e.to_string())?;
            let settings: AppSettings =
                serde_json::from_str(&content).map_err(|e| e.to_string())?;
            return Ok(settings);
        }
        // Return defaults
        Ok(AppSettings {
            formats: vec!["md".into()],
            image_as_pdf: false,
            enable_figure: true,
            mode: Some("lite".into()),
            docx_engine: Some("python".into()),
            excel_mode: Some("layout".into()),
            use_gpu: false,
            output_root: None,
            excel_meta_sheet: true,
            excel_symbol_fallback: true,
            chunk_size: Some(10),
            enable_rest: false,
            rest_seconds: Some(10),
            pdf_dpi: Some(200),
            window_width: Some(1200),
            window_height: Some(760),
            preview_quality: default_preview_quality(),
        })
    }
}

fn default_gpu_device() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "mps"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "cuda"
    }
}

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

/// Resolve python entry script path with priority:
/// 1) project_root/resources/py/<filename> (or _up_/resources/py)
/// 2) project_root/<filename> (legacy)
fn resolve_python_entry(project_root: &std::path::Path, filename: &str) -> PathBuf {
    for root in resolve_resource_roots(project_root) {
        let res = root.join("py").join(filename);
        if res.exists() {
            return res;
        }
    }
    project_root.join(filename)
}

/// Resolve python binary path with priority:
/// 1) env PYTHON_BIN
/// 2) project_root/resources/python/python(.exe) (portable runtime, or _up_/resources/python)
/// 3) project_root/resources/.venv/(Scripts|bin)/python(.exe) (legacy, or _up_/resources/.venv)
/// 4) project_root/.venv/(Scripts|bin)/python(.exe)
/// 5) "python"
fn resolve_python_bin(project_root: &std::path::Path) -> String {
    if let Ok(bin) = std::env::var("PYTHON_BIN") {
        if !bin.is_empty() {
            return bin;
        }
    }

    // resources/python (portable runtime)
    for root in resolve_resource_roots(project_root) {
        #[cfg(target_os = "windows")]
        let res_python = root.join("python").join("python.exe");
        #[cfg(not(target_os = "windows"))]
        let res_python = root.join("python").join("bin").join("python");
        if res_python.exists() {
            return res_python.to_string_lossy().to_string();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let res_python3 = root.join("python").join("bin").join("python3");
            if res_python3.exists() {
                return res_python3.to_string_lossy().to_string();
            }
        }
    }

    // resources/.venv (配布用に同梱する場合)
    for root in resolve_resource_roots(project_root) {
        #[cfg(target_os = "windows")]
        let res_venv = root.join(".venv").join("Scripts").join("python.exe");
        #[cfg(not(target_os = "windows"))]
        let res_venv = root.join(".venv").join("bin").join("python");
        if res_venv.exists() {
            return res_venv.to_string_lossy().to_string();
        }
    }

    #[cfg(target_os = "windows")]
    let venv = project_root
        .join(".venv")
        .join("Scripts")
        .join("python.exe");
    #[cfg(not(target_os = "windows"))]
    let venv = project_root.join(".venv").join("bin").join("python");
    if venv.exists() {
        return venv.to_string_lossy().to_string();
    }
    "python".into()
}

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

/// 入力パスに応じて出力候補を探す
fn collect_output_files(
    result_root: &std::path::Path,
    project_root: &std::path::Path,
    inputs: &[String],
    formats: &[String],
) -> Vec<PathBuf> {
    fn push_unique(found: &mut Vec<PathBuf>, path: PathBuf) {
        if path.exists() && !found.contains(&path) {
            found.push(path);
        }
    }

    fn pick_latest_result_dir(result_root: &std::path::Path, stem: &str) -> Option<PathBuf> {
        if !result_root.exists() {
            return None;
        }

        let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();

        let direct = result_root.join(stem);
        if direct.is_dir() {
            let modified = direct
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
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
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
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
                    // dispatcher は <output_dir.name>.xlsx を作る
                    push_unique(&mut found, result_dir.join(format!("{dir_name}.xlsx")));
                    push_unique(&mut found, result_dir.join(format!("{stem}.xlsx")));
                    // 念のため
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

                // ocr_chanked のマージ出力 + export_docx の変換結果は <output_dir.name>_merged.<fmt>
                let merged = result_dir.join(format!("{dir_name}_merged.{fmt}"));
                push_unique(&mut found, merged.clone());
                // 旧ルール互換
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

fn is_app_bundle_resource_dir(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str());
    if name != Some("Resources") {
        return false;
    }
    let contents = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());
    if contents != Some("Contents") {
        return false;
    }
    path.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        == Some("app")
}

fn resolve_app_support_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        return Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_SUPPORT_DIR_NAME),
        );
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))?;
        return Some(PathBuf::from(base).join(APP_SUPPORT_DIR_NAME));
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(base).join(APP_SUPPORT_DIR_NAME));
        }
        let home = std::env::var_os("HOME")?;
        return Some(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(APP_SUPPORT_DIR_NAME),
        );
    }
}

fn resolve_config_dir(project_root: &std::path::Path) -> PathBuf {
    if is_app_bundle_resource_dir(project_root) {
        if let Some(app_support) = resolve_app_support_dir() {
            return app_support.join("configs");
        }
    }
    project_root.join("configs")
}

fn expand_tilde_path(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(trimmed)
}

fn resolve_default_output_root(project_root: &std::path::Path) -> PathBuf {
    if is_app_bundle_resource_dir(project_root) {
        if let Some(app_support) = resolve_app_support_dir() {
            return app_support.join("result");
        }
    }
    project_root.join("result")
}

fn resolve_output_root(project_root: &std::path::Path, settings: Option<&AppSettings>) -> PathBuf {
    if let Some(s) = settings {
        if let Some(custom) = s.output_root.as_ref() {
            let trimmed = custom.trim();
            if !trimmed.is_empty() {
                return expand_tilde_path(trimmed);
            }
        }
    }
    resolve_default_output_root(project_root)
}

fn resolve_output_root_from_disk(project_root: &std::path::Path) -> PathBuf {
    let settings = load_settings_from_disk(project_root).ok();
    resolve_output_root(project_root, settings.as_ref())
}

fn resolve_resource_roots(project_root: &std::path::Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let direct = project_root.join("resources");
    roots.push(direct);
    let up = project_root.join("_up_").join("resources");
    if up != roots[0] {
        roots.push(up);
    }
    roots
}

/// Walk ancestors from exe_dir to find dispatcher.py; return its parent (project root)
fn resolve_project_root(exe_dir: &std::path::Path) -> Option<PathBuf> {
    let mut resource_candidate: Option<PathBuf> = None;
    for anc in exe_dir.ancestors() {
        let legacy = anc.join("dispatcher.py");
        let res = anc.join("resources").join("py").join("dispatcher.py");
        let app_res = anc
            .join("Contents")
            .join("Resources")
            .join("resources")
            .join("py")
            .join("dispatcher.py");
        let app_res_up = anc
            .join("Contents")
            .join("Resources")
            .join("_up_")
            .join("resources")
            .join("py")
            .join("dispatcher.py");
        if app_res.exists() {
            return Some(anc.join("Contents").join("Resources"));
        }
        if app_res_up.exists() {
            return Some(anc.join("Contents").join("Resources"));
        }
        if legacy.exists() {
            return Some(anc.to_path_buf());
        }
        if res.exists() && resource_candidate.is_none() {
            resource_candidate = Some(anc.to_path_buf());
        }
    }
    resource_candidate
}

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

fn find_output_path(
    result_root: &std::path::Path,
    project_root: &std::path::Path,
    filename: &str,
) -> Option<PathBuf> {
    // 1. result ディレクトリ内を探索
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

    // 2. result 直下
    let candidate = result_root.join(filename);
    if candidate.exists() {
        return Some(candidate);
    }

    // 3. プロジェクトルート直下
    let candidate = project_root.join(filename);
    if candidate.exists() {
        return Some(candidate);
    }

    None
}

fn open_path_with_default_app(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };

    #[cfg(target_os = "windows")]
    let mut cmd = {
        // explorer はファイル/フォルダ両方を開ける
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

fn validate_result_dir_name(dir_name: &str) -> Result<(), String> {
    if dir_name.is_empty() {
        return Err("dirName is empty".into());
    }
    if dir_name.contains('/') || dir_name.contains('\\') || dir_name.contains("..") {
        return Err("invalid dirName".into());
    }
    Ok(())
}

fn canonicalize_dir(path: &std::path::Path) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|e| format!("failed to canonicalize path: {e}"))
}

fn parse_page_range_from_dir(dir_name: &str) -> Option<String> {
    // 例: foo_p3-9 -> "p3-9"
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

fn pick_best_file_in_dir(dir: &std::path::Path, dir_name: &str) -> Option<String> {
    let candidates = [
        // docx
        format!("{dir_name}_merged.docx"),
        format!("{dir_name}.docx"),
        // xlsx (dispatcher は <output_dir.name>.xlsx)
        format!("{dir_name}.xlsx"),
        format!("{dir_name}_merged.xlsx"),
        // csv（複数になる可能性があるので、代表として単体名も見る）
        format!("{dir_name}.csv"),
        format!("{dir_name}_merged.csv"),
        // md
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
                .duration_since(UNIX_EPOCH)
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
fn check_environment() -> Result<EnvironmentStatus, String> {
    let exe_dir = std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let project_root = resolve_project_root(&exe_dir).ok_or("failed to resolve project root")?;
    let settings = load_settings_from_disk(&project_root).ok();
    let result_root = resolve_output_root(&project_root, settings.as_ref());
    let result_dir_found = result_root.exists();
    let python_bin = resolve_python_bin(&project_root);
    let python_path = if python_bin == "python" {
        None
    } else {
        Some(python_bin.clone())
    };
    let python_found = python_path
        .as_ref()
        .map(|p| PathBuf::from(p).is_file())
        .unwrap_or(false);

    let resource_roots = resolve_resource_roots(&project_root);
    let resource_roots_display = resource_roots
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    let mut dispatcher_candidates = resource_roots
        .iter()
        .map(|root| root.join("py").join("dispatcher.py"))
        .collect::<Vec<_>>();
    dispatcher_candidates.push(project_root.join("dispatcher.py"));
    let dispatcher_path = dispatcher_candidates
        .into_iter()
        .find(|p| p.is_file());
    let dispatcher_found = dispatcher_path.is_some();

    let mut poppler_candidates = Vec::new();
    for root in &resource_roots {
        let base = root.join("py").join("poppler");
        #[cfg(target_os = "windows")]
        {
            poppler_candidates.push(base.join("Library").join("bin"));
            poppler_candidates.push(base.join("win").join("bin"));
        }
        #[cfg(target_os = "macos")]
        {
            poppler_candidates.push(base.join("macos").join("bin"));
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let os_name = std::env::consts::OS;
            poppler_candidates.push(base.join(os_name).join("bin"));
        }
    }
    let poppler_path = poppler_candidates.into_iter().find(|dir| {
        if !dir.is_dir() {
            return false;
        }
        #[cfg(target_os = "windows")]
        let pdfinfo = dir.join("pdfinfo.exe");
        #[cfg(not(target_os = "windows"))]
        let pdfinfo = dir.join("pdfinfo");
        #[cfg(target_os = "windows")]
        let pdftoppm = dir.join("pdftoppm.exe");
        #[cfg(not(target_os = "windows"))]
        let pdftoppm = dir.join("pdftoppm");
        pdfinfo.exists() || pdftoppm.exists()
    });
    let poppler_found = poppler_path.is_some();

    Ok(EnvironmentStatus {
        project_root: project_root.to_string_lossy().to_string(),
        os: std::env::consts::OS.to_string(),
        dispatcher_found,
        dispatcher_path: dispatcher_path.map(|p| p.to_string_lossy().to_string()),
        result_dir_found,
        result_root: result_root.to_string_lossy().to_string(),
        python_bin,
        python_found,
        python_path,
        poppler_found,
        poppler_path: poppler_path.map(|p| p.to_string_lossy().to_string()),
        resource_roots: resource_roots_display,
    })
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
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    }

    let settings_path = config_dir.join("settings.json");
    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;

    fs::write(settings_path, content).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            run_job,
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

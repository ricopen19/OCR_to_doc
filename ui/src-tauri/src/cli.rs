use std::{fs, io::Write, path::PathBuf, process::Command};

use crate::paths::{
    apply_python_env, resolve_output_root, resolve_project_root, resolve_python_bin,
    resolve_python_entry,
};
use crate::load_settings_from_disk;

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

    // --self-test mode
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

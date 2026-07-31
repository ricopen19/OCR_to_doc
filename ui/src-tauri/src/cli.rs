use std::{fs, io::Write, process::Command};

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
    let log_trace = |msg: &str| {
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
    let export_script = resolve_python_entry(&project_root, "export_docx.py");
    if !export_script.exists() {
        log_trace(&format!(
            "[self-test] export_docx.py not found: {}",
            export_script.display()
        ));
        eprintln!("[self-test] export_docx.py not found: {}", export_script.display());
        return Some(2);
    }

    // write a minimal markdown for the export test
    let test_md = output_root.join("self_test.md");
    if let Err(e) = fs::write(&test_md, "# Self-test\n\nThis document was generated by the self-test.\n") {
        log_trace(&format!("[self-test] failed to write test markdown: {e}"));
        eprintln!("[self-test] failed to write test markdown: {e}");
        return Some(1);
    }

    eprintln!(
        "[self-test] project_root={} python={} export_script={} output_root={}",
        project_root.display(),
        python_bin,
        export_script.display(),
        output_root.display()
    );
    log_trace(&format!(
        "[self-test] python={} export_script={} output_root={}",
        python_bin,
        export_script.display(),
        output_root.display()
    ));

    let output = {
        let mut cmd = Command::new(&python_bin);
        apply_python_env(&mut cmd);
        cmd.arg("-u")
            .arg(&export_script)
            .arg(&test_md)
            .current_dir(&project_root);
        match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                log_trace(&format!("[self-test] failed to spawn export_docx: {e}"));
                eprintln!("[self-test] failed to spawn export_docx: {e}");
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
            "[self-test] export_docx failed with status={}",
            output.status
        ));
        eprintln!(
            "[self-test] export_docx failed with status={}",
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

    let docx = output_root.join("self_test.docx");
    if !docx.exists() {
        log_trace(&format!(
            "[self-test] expected docx not found: {}",
            docx.display()
        ));
        eprintln!("[self-test] expected docx not found: {}", docx.display());
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

    log_trace(&format!("[self-test] ok: {}", docx.display()));
    eprintln!("[self-test] ok: {}", docx.display());
    Some(0)
}

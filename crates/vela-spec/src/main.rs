//! Vela spec and fixture runner.
//!
//! This is a test harness, not the user-facing product. It prints a summary
//! and exits non-zero when an assertion fails.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vela_core::spec::{run_wast, WastReport};

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        args.push("tests/fixtures".to_string());
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in &args {
        let path = Path::new(arg);
        if path.is_dir() {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
                .map(|rd| {
                    rd.filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| p.extension().map(|x| x == "wast").unwrap_or(false))
                        .collect()
                })
                .unwrap_or_default();
            entries.sort();
            files.extend(entries);
        } else {
            files.push(path.to_path_buf());
        }
    }

    let mut reports: Vec<WastReport> = Vec::new();
    for file in &files {
        let name = file.display().to_string();
        match std::fs::read_to_string(file) {
            Ok(src) => reports.push(run_wast(&name, &src)),
            Err(e) => {
                eprintln!("{name}: cannot read: {e}");
                reports.push(WastReport {
                    file: name,
                    total: 1,
                    passed: 0,
                    failed: 1,
                    failures: vec![vela_core::spec::Failure {
                        line: 0,
                        message: format!("cannot read file: {e}"),
                    }],
                });
            }
        }
    }

    let mut total = 0;
    let mut passed = 0;
    let mut failed_files = 0;
    for r in &reports {
        total += r.total;
        passed += r.passed;
        if !r.ok() {
            failed_files += 1;
            println!("FAIL {}", r.summary());
            for f in &r.failures {
                println!("  line {}: {}", f.line, f.message);
            }
        } else {
            println!("ok   {}", r.summary());
        }
    }
    println!(
        "\n{passed}/{total} assertions passed in {} file(s), {failed_files} file(s) failed",
        files.len()
    );
    if failed_files == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

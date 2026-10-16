//! F12: the read path has no environment switches and no memory-mapped
//! constructor; the fork no longer depends on memmap2.
mod hardening_common;

use std::path::{Path, PathBuf};
use std::process::Command;

use hardening_common::serial;

/// Writer modules keep their own `PERF` and `ACADRUST_RAW_*` switches; they
/// are not compiled into the shipped reader once F16 gates them.
const WRITER_PATHS: &[&str] = &[
    "src/io/dwg/dwg_writer.rs",
    "src/io/dwg/dwg_stream_writers/",
    "src/io/dwg/file_headers/file_header_ac15.rs",
    "src/io/dwg/file_headers/file_header_ac18.rs",
    "src/io/dwg/file_headers/file_header_ac21.rs",
    "src/io/dxf/writer/",
];

const CHILD: &str = "HARDENING_SURFACE_CHILD";

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_read_path_has_no_environment_switches_or_mmap() {
    let _guard = serial();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    let mut offenders = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if WRITER_PATHS.iter().any(|writer| relative.starts_with(writer)) {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for (index, line) in text.lines().enumerate() {
            if ["env::var", "var_os(", "from_mmap", "memmap2"]
                .iter()
                .any(|needle| line.contains(needle))
            {
                offenders.push(format!("{relative}:{}", index + 1));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:?}");
}

#[test]
fn the_fork_no_longer_depends_on_memmap2() {
    let _guard = serial();
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(!manifest.contains("memmap2"));
}

#[test]
fn perf_and_raw_switches_print_nothing() {
    let _guard = serial();
    if std::env::var_os(CHILD).is_some() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/anonymous-names.dwg");
        let document = acadrust::DwgReader::from_file(path).unwrap().read().unwrap();
        println!("child read {} entities", document.entities().count());
        return;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "perf_and_raw_switches_print_nothing",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .env("PERF", "1")
        .env("ACADRUST_RAW_ALL", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("child read"), "{stdout}");
    assert!(stderr.trim().is_empty(), "stderr: {stderr}");
}

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use crate::app::paths::instance_dir;

/// Soft cap per file so we don't ship multi‑MB logs to the UI.
const MAX_FILE_CHARS: usize = 512_000;
/// How many newest crash reports to include.
const MAX_CRASH_REPORTS: usize = 3;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisLogSource {
    pub kind: String,
    pub name: String,
    pub path: String,
    pub content: String,
    pub truncated: bool,
    pub modified_at: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstanceAnalysisLogs {
    pub sources: Vec<AnalysisLogSource>,
}

fn modified_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_text_tail(path: &Path, max_chars: usize) -> Result<(String, bool), String> {
    let raw = fs::read(path).map_err(|e| format!("Не удалось прочитать {}: {e}", path.display()))?;
    // Minecraft logs are typically UTF-8; fall back to lossy.
    let full = String::from_utf8_lossy(&raw);
    if full.len() <= max_chars {
        return Ok((full.into_owned(), false));
    }
    // Prefer a char-boundary tail so we don't split UTF-8.
    let mut start = full.len().saturating_sub(max_chars);
    while start > 0 && !full.is_char_boundary(start) {
        start -= 1;
    }
    // Drop partial first line after truncation.
    let tail = &full[start..];
    let trimmed = match tail.find('\n') {
        Some(i) if i + 1 < tail.len() => &tail[i + 1..],
        _ => tail,
    };
    Ok((trimmed.to_string(), true))
}

fn push_file_source(
    out: &mut Vec<AnalysisLogSource>,
    kind: &str,
    path: &Path,
    max_chars: usize,
) {
    if !path.is_file() {
        return;
    }
    let Ok((content, truncated)) = read_text_tail(path, max_chars) else {
        return;
    };
    if content.trim().is_empty() {
        return;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("log")
        .to_string();
    out.push(AnalysisLogSource {
        kind: kind.to_string(),
        name,
        path: path.to_string_lossy().to_string(),
        content,
        truncated,
        modified_at: modified_secs(path),
    });
}

fn collect_crash_reports(crash_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(crash_dir) else {
        return Vec::new();
    };
    let mut files: Vec<(u64, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            if !p.is_file() {
                return false;
            }
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            name.ends_with(".txt")
                || name.ends_with(".log")
                || name.contains("crash")
        })
        .map(|p| (modified_secs(&p), p))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files
        .into_iter()
        .take(MAX_CRASH_REPORTS)
        .map(|(_, p)| p)
        .collect()
}

/// Read `logs/latest.log`, optional `crash-report.log`, and newest crash-reports
/// for the given profile/instance.
#[tauri::command]
pub fn read_instance_analysis_logs(profile_id: String) -> Result<InstanceAnalysisLogs, String> {
    let id = profile_id.trim();
    if id.is_empty() || id.starts_with("__") {
        return Ok(InstanceAnalysisLogs {
            sources: Vec::new(),
        });
    }

    let root = instance_dir(id)?;
    let mut sources: Vec<AnalysisLogSource> = Vec::new();

    let logs_dir = root.join("logs");
    push_file_source(&mut sources, "latest", &logs_dir.join("latest.log"), MAX_FILE_CHARS);
    // Some packs / tools write a flat crash-report.log next to latest.log
    push_file_source(
        &mut sources,
        "crash_report",
        &logs_dir.join("crash-report.log"),
        MAX_FILE_CHARS,
    );
    push_file_source(
        &mut sources,
        "crash_report",
        &root.join("crash-report.log"),
        MAX_FILE_CHARS,
    );

    let crash_dir = root.join("crash-reports");
    for path in collect_crash_reports(&crash_dir) {
        push_file_source(&mut sources, "crash_report", &path, MAX_FILE_CHARS);
    }

    // Prefer richer crash reports first when the UI lists sources.
    sources.sort_by(|a, b| {
        let rank = |k: &str| match k {
            "crash_report" => 0,
            "latest" => 1,
            _ => 2,
        };
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then(b.modified_at.cmp(&a.modified_at))
    });

    Ok(InstanceAnalysisLogs { sources })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn read_text_tail_truncates_on_char_boundary() {
        let dir = std::env::temp_dir().join(format!(
            "mc16_analysis_logs_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.log");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "line-one").unwrap();
        writeln!(f, "line-two-привіт").unwrap();
        writeln!(f, "line-three").unwrap();
        drop(f);

        let (content, truncated) = read_text_tail(&path, 20).unwrap();
        assert!(truncated);
        assert!(!content.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}

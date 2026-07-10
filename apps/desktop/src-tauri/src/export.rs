// Markdown export, format carried over from the legacy app:
// [M:SS] **Speaker**: text

use std::path::PathBuf;

use tauri::AppHandle;

use crate::db::{Db, MeetingRow, Segment};

pub fn format_timestamp(ms: u64) -> String {
    let total = ms / 1000;
    format!("{}:{:02}", total / 60, total % 60)
}

pub fn meeting_markdown(meeting: &MeetingRow, segments: &[Segment]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", meeting.title));
    out.push_str(&format!("- Date: {}\n", meeting.created_at));
    out.push_str(&format!(
        "- Duration: {}\n",
        format_timestamp(meeting.duration_seconds * 1000)
    ));
    if let Some(model) = &meeting.whisper_model {
        out.push_str(&format!("- Transcribed with: whisper {model}\n"));
    }

    out.push_str("\n## Transcript\n\n");
    if segments.is_empty() {
        out.push_str("_No transcript._\n");
    } else {
        for s in segments {
            out.push_str(&format!(
                "[{}] **{}**: {}\n\n",
                format_timestamp(s.start_ms),
                s.speaker,
                s.text
            ));
        }
    }
    out
}

/// Sanitize a meeting title into a safe filename stem.
pub fn safe_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() { "meeting".to_string() } else { trimmed }
}

pub fn export_dir(app: &AppHandle, db: &Db) -> Result<PathBuf, String> {
    if let Some(dir) = db.get_setting("export_folder")? {
        return Ok(PathBuf::from(dir));
    }
    // Default: ~/Documents/Harknotes
    let docs = tauri::Manager::path(app)
        .document_dir()
        .map_err(|e| format!("resolve documents dir: {e}"))?;
    Ok(docs.join("Harknotes"))
}

pub fn export_meeting(app: &AppHandle, db: &Db, meeting_id: &str) -> Result<String, String> {
    let meeting = db.get_meeting(meeting_id)?;
    let segments = db.transcript(meeting_id)?;
    let markdown = meeting_markdown(&meeting, &segments);

    let dir = export_dir(app, db)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create export dir: {e}"))?;
    let path = dir.join(format!("{}.md", safe_filename(&meeting.title)));
    std::fs::write(&path, markdown).map_err(|e| format!("write export: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_markdown() {
        let meeting = MeetingRow {
            id: "m1".into(),
            title: "Standup".into(),
            created_at: "2026-07-09 16:52:30".into(),
            duration_seconds: 125,
            status: "ready".into(),
            whisper_model: Some("base".into()),
        };
        let segments = vec![
            Segment { speaker: "User".into(), text: "Hi all.".into(), start_ms: 500, end_ms: 1500 },
            Segment { speaker: "Meeting".into(), text: "Hello!".into(), start_ms: 2000, end_ms: 2600 },
        ];
        let md = meeting_markdown(&meeting, &segments);
        assert!(md.starts_with("# Standup\n"));
        assert!(md.contains("- Duration: 2:05\n"));
        assert!(md.contains("[0:00] **User**: Hi all.\n"));
        assert!(md.contains("[0:02] **Meeting**: Hello!\n"));
    }

    #[test]
    fn sanitizes_filenames() {
        assert_eq!(safe_filename("a/b:c*d"), "a-b-c-d");
        assert_eq!(safe_filename("  .. "), "meeting");
        assert_eq!(safe_filename("Meeting Jul 9, 16:52"), "Meeting Jul 9, 16-52");
    }
}

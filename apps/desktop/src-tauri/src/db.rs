// SQLite storage via rusqlite (bundled). All access goes through Db so the
// transcription worker (Rust threads) and IPC commands share one connection.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Done/Failed are written via SQL in complete/fail_chunk
pub enum ChunkStatus {
    Pending,
    Silent,
    Done,
    Failed,
}

impl ChunkStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ChunkStatus::Pending => "pending",
            ChunkStatus::Silent => "silent",
            ChunkStatus::Done => "done",
            ChunkStatus::Failed => "failed",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// Row id once persisted; None for freshly parsed segments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub speaker: String,
    pub text: String,
    /// Absolute ms from recording start.
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub meeting_id: String,
    pub title: String,
    pub created_at: String,
    /// Where the match came from: "title" | "transcript" | "notes" | "summary".
    pub source: String,
    pub snippet: String,
    /// Set for transcript matches — jump target in the meeting view.
    pub start_ms: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingRow {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub duration_seconds: u64,
    pub status: String,
    pub whisper_model: Option<String>,
    /// JSON blob produced by the summarizer (schema owned by the frontend).
    pub summary_json: Option<String>,
    pub summarized_at: Option<String>,
    /// The user's own notes (markdown), taken during or after the meeting.
    pub notes: String,
    pub tags: Vec<String>,
}

#[derive(Clone)]
pub struct PendingChunk {
    pub meeting_id: String,
    pub stream: String,
    pub idx: u32,
    pub wav_path: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("open db: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("enable WAL: {e}"))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("enable foreign keys: {e}"))?;
        conn.execute_batch(SCHEMA).map_err(|e| format!("migrate: {e}"))?;
        migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        migrate(&conn).unwrap();
        Self { conn: Mutex::new(conn) }
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        f(&conn).map_err(|e| e.to_string())
    }

    // -- meetings ---------------------------------------------------------

    pub fn insert_meeting(&self, id: &str, title: &str, whisper_model: &str) -> Result<(), String> {
        self.with(|c| {
            c.execute(
                "INSERT INTO meetings (id, title, created_at, status, whisper_model)
                 VALUES (?1, ?2, datetime('now'), 'recording', ?3)",
                (id, title, whisper_model),
            )
            .map(|_| ())
        })
    }

    /// Recording is over: record duration and enter 'transcribing'. Callers
    /// follow up with try_mark_ready() so the ready transition (and its event)
    /// happens in exactly one place.
    pub fn finish_recording(&self, id: &str, duration_seconds: u64) -> Result<(), String> {
        self.with(|c| {
            c.execute(
                "UPDATE meetings SET duration_seconds = ?2, status = 'transcribing'
                 WHERE id = ?1",
                (id, duration_seconds),
            )
            .map(|_| ())
        })
    }

    /// Mark meeting ready if recording is over and nothing is pending.
    /// Returns true when the meeting just became ready.
    pub fn try_mark_ready(&self, id: &str) -> Result<bool, String> {
        self.with(|c| {
            let n = c.execute(
                "UPDATE meetings SET status = 'ready'
                 WHERE id = ?1 AND status = 'transcribing'
                   AND (SELECT COUNT(*) FROM chunks
                         WHERE meeting_id = ?1 AND status = 'pending') = 0",
                [id],
            )?;
            Ok(n > 0)
        })
    }

    pub fn get_meeting(&self, id: &str) -> Result<MeetingRow, String> {
        self.with(|c| {
            c.query_row(
                "SELECT id, title, created_at, duration_seconds, status, whisper_model,
                        summary_json, summarized_at, notes, tags
                 FROM meetings WHERE id = ?1",
                [id],
                map_meeting_row,
            )
        })
    }

    pub fn save_summary(&self, id: &str, summary_json: &str) -> Result<(), String> {
        self.with(|c| {
            c.execute(
                "UPDATE meetings SET summary_json = ?2, summarized_at = datetime('now')
                 WHERE id = ?1",
                (id, summary_json),
            )
            .map(|_| ())
        })
    }

    pub fn rename_meeting(&self, id: &str, title: &str) -> Result<(), String> {
        self.with(|c| {
            c.execute("UPDATE meetings SET title = ?2 WHERE id = ?1", (id, title))
                .map(|_| ())
        })
    }

    pub fn delete_meeting(&self, id: &str) -> Result<(), String> {
        self.with(|c| {
            c.execute("DELETE FROM meetings WHERE id = ?1", [id]).map(|_| ())
        })
    }

    pub fn list_meetings(&self) -> Result<Vec<MeetingRow>, String> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, title, created_at, duration_seconds, status, whisper_model,
                        summary_json, summarized_at, notes, tags
                 FROM meetings ORDER BY created_at DESC",
            )?;
            let rows = stmt
                .query_map([], map_meeting_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    // -- chunks -----------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn insert_chunk(
        &self,
        meeting_id: &str,
        stream: &str,
        idx: u32,
        wav_path: &str,
        start_ms: u64,
        duration_ms: u64,
        status: ChunkStatus,
    ) -> Result<(), String> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO chunks
                   (meeting_id, stream, idx, wav_path, start_ms, duration_ms, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                (
                    meeting_id,
                    stream,
                    idx,
                    wav_path,
                    start_ms,
                    duration_ms,
                    status.as_str(),
                ),
            )
            .map(|_| ())
        })
    }

    /// Mark a chunk transcribed and store its segments as rows (idempotent —
    /// a retried chunk replaces its previous segments).
    pub fn complete_chunk(
        &self,
        meeting_id: &str,
        stream: &str,
        idx: u32,
        segments: &[Segment],
    ) -> Result<(), String> {
        self.with(|c| {
            c.execute(
                "DELETE FROM segments WHERE meeting_id = ?1 AND stream = ?2 AND chunk_idx = ?3",
                (meeting_id, stream, idx),
            )?;
            for s in segments {
                c.execute(
                    "INSERT INTO segments (meeting_id, stream, chunk_idx, start_ms, end_ms, speaker, text)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    (meeting_id, stream, idx, s.start_ms, s.end_ms, &s.speaker, &s.text),
                )?;
            }
            c.execute(
                "UPDATE chunks SET status = 'done', error = NULL
                 WHERE meeting_id = ?1 AND stream = ?2 AND idx = ?3",
                (meeting_id, stream, idx),
            )
            .map(|_| ())
        })
    }

    pub fn update_segment(&self, segment_id: i64, text: &str) -> Result<(), String> {
        self.with(|c| {
            c.execute("UPDATE segments SET text = ?2 WHERE id = ?1", (segment_id, text))
                .map(|_| ())
        })
    }

    /// Rename a speaker across one meeting; returns affected segment count.
    pub fn rename_speaker(&self, meeting_id: &str, from: &str, to: &str) -> Result<usize, String> {
        self.with(|c| {
            c.execute(
                "UPDATE segments SET speaker = ?3 WHERE meeting_id = ?1 AND speaker = ?2",
                (meeting_id, from, to),
            )
        })
    }

    pub fn set_notes(&self, meeting_id: &str, notes: &str) -> Result<(), String> {
        self.with(|c| {
            c.execute("UPDATE meetings SET notes = ?2 WHERE id = ?1", (meeting_id, notes))
                .map(|_| ())
        })
    }

    pub fn set_tags(&self, meeting_id: &str, tags: &[String]) -> Result<(), String> {
        let json = serde_json::to_string(tags).map_err(|e| e.to_string())?;
        self.with(|c| {
            c.execute("UPDATE meetings SET tags = ?2 WHERE id = ?1", (meeting_id, json))
                .map(|_| ())
        })
    }

    pub fn fail_chunk(
        &self,
        meeting_id: &str,
        stream: &str,
        idx: u32,
        error: &str,
    ) -> Result<(), String> {
        self.with(|c| {
            c.execute(
                "UPDATE chunks SET status = 'failed', error = ?4
                 WHERE meeting_id = ?1 AND stream = ?2 AND idx = ?3",
                (meeting_id, stream, idx, error),
            )
            .map(|_| ())
        })
    }

    /// Recover meetings left in 'recording' by a crash: estimate duration from
    /// the last persisted chunk and move them to 'transcribing'. Returns the
    /// affected meeting ids so the caller can run readiness checks.
    pub fn finalize_stale_recordings(&self) -> Result<Vec<String>, String> {
        let ids: Vec<String> = self.with(|c| {
            let mut stmt =
                c.prepare("SELECT id FROM meetings WHERE status = 'recording'")?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?;
            Ok(rows)
        })?;
        for id in &ids {
            self.with(|c| {
                c.execute(
                    "UPDATE meetings SET status = 'transcribing',
                       duration_seconds = COALESCE(
                         (SELECT MAX(start_ms + duration_ms) / 1000 FROM chunks
                           WHERE meeting_id = ?1), 0)
                     WHERE id = ?1",
                    [id],
                )
                .map(|_| ())
            })?;
        }
        Ok(ids)
    }

    /// Chunks to (re-)transcribe at startup: pending from a crash, plus failed
    /// ones worth retrying.
    pub fn resumable_chunks(&self) -> Result<Vec<PendingChunk>, String> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT meeting_id, stream, idx, wav_path, start_ms, duration_ms
                 FROM chunks WHERE status IN ('pending', 'failed')
                 ORDER BY meeting_id, start_ms",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(PendingChunk {
                        meeting_id: r.get(0)?,
                        stream: r.get(1)?,
                        idx: r.get(2)?,
                        wav_path: r.get(3)?,
                        start_ms: r.get::<_, i64>(4)? as u64,
                        duration_ms: r.get::<_, i64>(5)? as u64,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// All transcribed segments of a meeting, ordered by absolute time.
    pub fn transcript(&self, meeting_id: &str) -> Result<Vec<Segment>, String> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, speaker, text, start_ms, end_ms FROM segments
                 WHERE meeting_id = ?1 ORDER BY start_ms, id",
            )?;
            let rows = stmt
                .query_map([meeting_id], |r| {
                    Ok(Segment {
                        id: Some(r.get(0)?),
                        speaker: r.get(1)?,
                        text: r.get(2)?,
                        start_ms: r.get::<_, i64>(3)? as u64,
                        end_ms: r.get::<_, i64>(4)? as u64,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Case-insensitive substring search over titles, transcripts, notes and
    /// summaries. LIKE is instant at local scale (hundreds of meetings); FTS5
    /// can replace this if it ever isn't.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!(
            "%{}%",
            q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
        );

        self.with(|c| {
            let mut out: Vec<SearchResult> = Vec::new();

            let mut stmt = c.prepare(
                "SELECT id, title, created_at FROM meetings
                 WHERE title LIKE ?1 ESCAPE '\\' ORDER BY created_at DESC LIMIT ?2",
            )?;
            let titles = stmt
                .query_map((&pattern, limit as i64), |r| {
                    Ok(SearchResult {
                        meeting_id: r.get(0)?,
                        title: r.get(1)?,
                        created_at: r.get(2)?,
                        source: "title".into(),
                        snippet: r.get(1)?,
                        start_ms: None,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.extend(titles);

            let mut stmt = c.prepare(
                "SELECT s.meeting_id, m.title, m.created_at, s.text, s.start_ms
                 FROM segments s JOIN meetings m ON m.id = s.meeting_id
                 WHERE s.text LIKE ?1 ESCAPE '\\'
                 ORDER BY m.created_at DESC, s.start_ms LIMIT ?2",
            )?;
            let segs = stmt
                .query_map((&pattern, limit as i64), |r| {
                    Ok(SearchResult {
                        meeting_id: r.get(0)?,
                        title: r.get(1)?,
                        created_at: r.get(2)?,
                        source: "transcript".into(),
                        snippet: r.get(3)?,
                        start_ms: Some(r.get::<_, i64>(4)? as u64),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.extend(segs);

            let mut stmt = c.prepare(
                "SELECT id, title, created_at, notes,
                        CASE WHEN notes LIKE ?1 ESCAPE '\\' THEN 'notes' ELSE 'summary' END
                 FROM meetings
                 WHERE notes LIKE ?1 ESCAPE '\\'
                    OR coalesce(summary_json,'') LIKE ?1 ESCAPE '\\'
                 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let others = stmt
                .query_map((&pattern, limit as i64), |r| {
                    let source: String = r.get(4)?;
                    let notes: String = r.get(3)?;
                    Ok(SearchResult {
                        meeting_id: r.get(0)?,
                        title: r.get(1)?,
                        created_at: r.get(2)?,
                        snippet: if source == "notes" { notes } else { String::new() },
                        source,
                        start_ms: None,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.extend(others);

            out.truncate(limit);
            Ok(out)
        })
    }

    // -- settings ---------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        self.with(|c| {
            c.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (key, value),
            )
            .map(|_| ())
        })
    }
}

fn map_meeting_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MeetingRow> {
    let tags_json: String = r.get(9)?;
    Ok(MeetingRow {
        id: r.get(0)?,
        title: r.get(1)?,
        created_at: r.get(2)?,
        duration_seconds: r.get::<_, i64>(3)? as u64,
        status: r.get(4)?,
        whisper_model: r.get(5)?,
        summary_json: r.get(6)?,
        summarized_at: r.get(7)?,
        notes: r.get(8)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
    })
}

/// Idempotent post-schema migrations for databases created by older builds.
fn migrate(conn: &Connection) -> Result<(), String> {
    let has_column = |table: &str, column: &str| -> Result<bool, String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| e.to_string())?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        Ok(names.iter().any(|n| n == column))
    };

    if !has_column("meetings", "notes")? {
        conn.execute("ALTER TABLE meetings ADD COLUMN notes TEXT NOT NULL DEFAULT ''", [])
            .map_err(|e| e.to_string())?;
    }
    if !has_column("meetings", "tags")? {
        conn.execute("ALTER TABLE meetings ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'", [])
            .map_err(|e| e.to_string())?;
    }

    // One-time move of legacy chunk segment blobs into the segments table.
    if has_column("chunks", "segments_json")? {
        let blobs: Vec<(String, String, u32, String)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT meeting_id, stream, idx, segments_json FROM chunks
                     WHERE segments_json IS NOT NULL",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map_err(|e| e.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        for (meeting_id, stream, idx, json) in blobs {
            let segments: Vec<Segment> = match serde_json::from_str(&json) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for s in segments {
                conn.execute(
                    "INSERT INTO segments (meeting_id, stream, chunk_idx, start_ms, end_ms, speaker, text)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    (&meeting_id, &stream, idx, s.start_ms, s.end_ms, &s.speaker, &s.text),
                )
                .map_err(|e| e.to_string())?;
            }
            conn.execute(
                "UPDATE chunks SET segments_json = NULL
                 WHERE meeting_id = ?1 AND stream = ?2 AND idx = ?3",
                (&meeting_id, &stream, idx),
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meetings (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  duration_seconds INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'recording',
  whisper_model TEXT,
  summary_json TEXT,
  summarized_at TEXT,
  notes TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS chunks (
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  stream TEXT NOT NULL,
  idx INTEGER NOT NULL,
  wav_path TEXT NOT NULL,
  start_ms INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  segments_json TEXT,
  error TEXT,
  PRIMARY KEY (meeting_id, stream, idx)
);

CREATE TABLE IF NOT EXISTS segments (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  stream TEXT NOT NULL,
  chunk_idx INTEGER NOT NULL,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL,
  speaker TEXT NOT NULL,
  text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_segments_meeting ON segments(meeting_id, start_ms);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_lifecycle_and_resume() {
        let db = Db::open_in_memory();
        db.insert_meeting("m1", "Test", "base").unwrap();
        db.insert_chunk("m1", "mic", 0, "/tmp/a.wav", 0, 45_000, ChunkStatus::Pending)
            .unwrap();
        db.insert_chunk("m1", "system", 0, "/tmp/b.wav", 500, 45_000, ChunkStatus::Pending)
            .unwrap();
        db.insert_chunk("m1", "mic", 1, "/tmp/c.wav", 45_000, 10_000, ChunkStatus::Silent)
            .unwrap();

        // Crash-resume picks up only pending/failed.
        assert_eq!(db.resumable_chunks().unwrap().len(), 2);

        db.complete_chunk(
            "m1",
            "mic",
            0,
            &[Segment {
                id: None,
                speaker: "User".into(),
                text: "hello".into(),
                start_ms: 100,
                end_ms: 900,
            }],
        )
        .unwrap();
        // Not ready while still recording (status != transcribing).
        assert!(!db.try_mark_ready("m1").unwrap());
        // Recording finished with system/0 still pending -> stays transcribing.
        db.finish_recording("m1", 55).unwrap();
        assert!(!db.try_mark_ready("m1").unwrap());

        // A failed chunk (retry already spent) does NOT block readiness —
        // it stays resumable for the next launch instead.
        db.fail_chunk("m1", "system", 0, "timeout").unwrap();
        assert_eq!(db.resumable_chunks().unwrap().len(), 1);
        assert!(db.try_mark_ready("m1").unwrap());
        assert!(!db.try_mark_ready("m1").unwrap()); // only fires once

        // Late retry success still lands in the transcript.
        db.complete_chunk(
            "m1",
            "system",
            0,
            &[Segment {
                id: None,
                speaker: "Meeting".into(),
                text: "world".into(),
                start_ms: 600,
                end_ms: 1400,
            }],
        )
        .unwrap();
        let transcript = db.transcript("m1").unwrap();
        assert_eq!(transcript.len(), 2);
        assert_eq!(transcript[0].text, "hello");
        assert_eq!(transcript[1].speaker, "Meeting");
    }

    #[test]
    fn recovers_stale_recordings() {
        let db = Db::open_in_memory();
        db.insert_meeting("stale", "Crashed", "base").unwrap();
        db.insert_chunk("stale", "mic", 0, "/tmp/a.wav", 0, 45_000, ChunkStatus::Pending)
            .unwrap();
        db.insert_chunk("stale", "mic", 1, "/tmp/b.wav", 45_000, 30_000, ChunkStatus::Silent)
            .unwrap();
        db.insert_meeting("fine", "Normal", "base").unwrap();
        db.finish_recording("fine", 10).unwrap();
        db.try_mark_ready("fine").unwrap();

        let ids = db.finalize_stale_recordings().unwrap();
        assert_eq!(ids, vec!["stale".to_string()]);

        let m = db.get_meeting("stale").unwrap();
        assert_eq!(m.status, "transcribing");
        assert_eq!(m.duration_seconds, 75); // (45000+30000)/1000

        // Not ready until the pending chunk completes.
        assert!(!db.try_mark_ready("stale").unwrap());
        db.complete_chunk("stale", "mic", 0, &[]).unwrap();
        assert!(db.try_mark_ready("stale").unwrap());
    }

    #[test]
    fn segment_editing_rename_notes_tags_search() {
        let db = Db::open_in_memory();
        db.insert_meeting("m1", "Roadmap sync", "base").unwrap();
        db.insert_chunk("m1", "mic", 0, "/tmp/a.wav", 0, 45_000, ChunkStatus::Pending)
            .unwrap();
        db.complete_chunk(
            "m1",
            "mic",
            0,
            &[
                Segment { id: None, speaker: "User".into(), text: "Ship the kraken feature".into(), start_ms: 0, end_ms: 2000 },
                Segment { id: None, speaker: "Meeting".into(), text: "Agreed, next sprint".into(), start_ms: 2000, end_ms: 4000 },
            ],
        )
        .unwrap();

        // Edit a segment.
        let t = db.transcript("m1").unwrap();
        assert_eq!(t.len(), 2);
        let seg_id = t[0].id.unwrap();
        db.update_segment(seg_id, "Ship the export feature").unwrap();
        assert_eq!(db.transcript("m1").unwrap()[0].text, "Ship the export feature");

        // Retry idempotency: completing the same chunk replaces its segments.
        db.complete_chunk(
            "m1",
            "mic",
            0,
            &[Segment { id: None, speaker: "User".into(), text: "Only one now".into(), start_ms: 0, end_ms: 1000 }],
        )
        .unwrap();
        assert_eq!(db.transcript("m1").unwrap().len(), 1);

        // Speaker rename.
        assert_eq!(db.rename_speaker("m1", "User", "Gav").unwrap(), 1);
        assert_eq!(db.transcript("m1").unwrap()[0].speaker, "Gav");

        // Notes + tags round-trip.
        db.set_notes("m1", "kraken was a codename").unwrap();
        db.set_tags("m1", &["work".into(), "roadmap".into()]).unwrap();
        let m = db.get_meeting("m1").unwrap();
        assert_eq!(m.notes, "kraken was a codename");
        assert_eq!(m.tags, vec!["work", "roadmap"]);

        // Search across sources; LIKE wildcards must be escaped.
        let hits = db.search("only one", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "transcript");
        assert_eq!(hits[0].start_ms, Some(0));
        let hits = db.search("kraken", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, "notes");
        let hits = db.search("roadmap sync", 10).unwrap();
        assert_eq!(hits[0].source, "title");
        assert!(db.search("100%", 10).unwrap().is_empty());
        assert!(db.search("  ", 10).unwrap().is_empty());
    }

    #[test]
    fn migrates_legacy_segment_blobs() {
        let db = Db::open_in_memory();
        db.insert_meeting("m1", "Old", "base").unwrap();
        db.insert_chunk("m1", "mic", 0, "/tmp/a.wav", 0, 45_000, ChunkStatus::Done)
            .unwrap();
        // Simulate a pre-refactor row with a JSON blob.
        db.with(|c| {
            c.execute(
                "UPDATE chunks SET segments_json =
                 '[{\"speaker\":\"User\",\"text\":\"legacy\",\"startMs\":10,\"endMs\":20}]'
                 WHERE meeting_id = 'm1'",
                [],
            )
        })
        .unwrap();

        db.with(|c| migrate_for_test(c)).unwrap();

        let t = db.transcript("m1").unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].text, "legacy");
        assert_eq!(t[0].start_ms, 10);
    }

    // Expose migrate to the blob test through the connection guard.
    fn migrate_for_test(c: &Connection) -> rusqlite::Result<()> {
        migrate(c).map_err(|_| rusqlite::Error::InvalidQuery)
    }

    #[test]
    fn settings_roundtrip() {
        let db = Db::open_in_memory();
        assert_eq!(db.get_setting("whisper_model").unwrap(), None);
        db.set_setting("whisper_model", "base").unwrap();
        db.set_setting("whisper_model", "small").unwrap();
        assert_eq!(db.get_setting("whisper_model").unwrap().as_deref(), Some("small"));
    }
}

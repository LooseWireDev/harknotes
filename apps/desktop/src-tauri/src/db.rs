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
    pub speaker: String,
    pub text: String,
    /// Absolute ms from recording start.
    pub start_ms: u64,
    pub end_ms: u64,
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
        Ok(Self { conn: Mutex::new(conn) })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
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
                        summary_json, summarized_at
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
                        summary_json, summarized_at
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

    pub fn complete_chunk(
        &self,
        meeting_id: &str,
        stream: &str,
        idx: u32,
        segments: &[Segment],
    ) -> Result<(), String> {
        let json = serde_json::to_string(segments).map_err(|e| e.to_string())?;
        self.with(|c| {
            c.execute(
                "UPDATE chunks SET status = 'done', segments_json = ?4, error = NULL
                 WHERE meeting_id = ?1 AND stream = ?2 AND idx = ?3",
                (meeting_id, stream, idx, json),
            )
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
        let jsons: Vec<String> = self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT segments_json FROM chunks
                 WHERE meeting_id = ?1 AND segments_json IS NOT NULL",
            )?;
            let rows = stmt
                .query_map([meeting_id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<String>>>()?;
            Ok(rows)
        })?;

        let mut segments: Vec<Segment> = Vec::new();
        for json in jsons {
            let mut chunk: Vec<Segment> =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            segments.append(&mut chunk);
        }
        segments.sort_by_key(|s| s.start_ms);
        Ok(segments)
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
    Ok(MeetingRow {
        id: r.get(0)?,
        title: r.get(1)?,
        created_at: r.get(2)?,
        duration_seconds: r.get::<_, i64>(3)? as u64,
        status: r.get(4)?,
        whisper_model: r.get(5)?,
        summary_json: r.get(6)?,
        summarized_at: r.get(7)?,
    })
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
  summarized_at TEXT
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
    fn settings_roundtrip() {
        let db = Db::open_in_memory();
        assert_eq!(db.get_setting("whisper_model").unwrap(), None);
        db.set_setting("whisper_model", "base").unwrap();
        db.set_setting("whisper_model", "small").unwrap();
        assert_eq!(db.get_setting("whisper_model").unwrap().as_deref(), Some("small"));
    }
}

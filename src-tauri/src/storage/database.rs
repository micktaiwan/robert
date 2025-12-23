use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use uuid::Uuid;

use super::models::{AudioSource, Recording, Transcription};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new() -> Result<Self> {
        let path = Self::db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        let db = Self { conn };
        db.run_migrations()?;
        Ok(db)
    }

    fn db_path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("com", "robert", "Robert")
            .ok_or_else(|| anyhow!("Could not find app directories"))?;
        Ok(dirs.data_dir().join("robert.db"))
    }

    fn run_migrations(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS recordings (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                ended_at TEXT,
                is_active INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS transcriptions (
                id TEXT PRIMARY KEY,
                recording_id TEXT NOT NULL,
                text TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                source TEXT NOT NULL,
                FOREIGN KEY (recording_id) REFERENCES recordings(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_transcriptions_recording
            ON transcriptions(recording_id);
            "#,
        )?;
        Ok(())
    }

    pub fn create_recording(&self, name: &str) -> Result<Recording> {
        let recording = Recording {
            id: Uuid::new_v4(),
            name: name.to_string(),
            created_at: Utc::now(),
            ended_at: None,
            is_active: true,
        };

        self.conn.execute(
            "INSERT INTO recordings (id, name, created_at, is_active) VALUES (?1, ?2, ?3, ?4)",
            params![
                recording.id.to_string(),
                recording.name,
                recording.created_at.to_rfc3339(),
                recording.is_active as i32
            ],
        )?;

        Ok(recording)
    }

    pub fn end_recording(&self, id: Uuid) -> Result<()> {
        let now = Utc::now();
        self.conn.execute(
            "UPDATE recordings SET ended_at = ?1, is_active = 0 WHERE id = ?2",
            params![now.to_rfc3339(), id.to_string()],
        )?;
        Ok(())
    }

    pub fn add_transcription(
        &self,
        recording_id: Uuid,
        text: &str,
        source: AudioSource,
    ) -> Result<Transcription> {
        let transcription = Transcription {
            id: Uuid::new_v4(),
            recording_id,
            text: text.to_string(),
            timestamp: Utc::now(),
            source,
        };

        self.conn.execute(
            "INSERT INTO transcriptions (id, recording_id, text, timestamp, source) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                transcription.id.to_string(),
                transcription.recording_id.to_string(),
                transcription.text,
                transcription.timestamp.to_rfc3339(),
                transcription.source.as_str()
            ],
        )?;

        Ok(transcription)
    }

    pub fn list_recordings(&self) -> Result<Vec<Recording>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, ended_at, is_active FROM recordings ORDER BY created_at DESC",
        )?;

        let recordings = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let created_at: String = row.get(2)?;
                let ended_at: Option<String> = row.get(3)?;
                let is_active: i32 = row.get(4)?;

                Ok(Recording {
                    id: Uuid::parse_str(&id).unwrap_or_default(),
                    name,
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    ended_at: ended_at.and_then(|s| {
                        DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .ok()
                    }),
                    is_active: is_active != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(recordings)
    }

    pub fn get_transcriptions(&self, recording_id: Uuid) -> Result<Vec<Transcription>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, recording_id, text, timestamp, source FROM transcriptions WHERE recording_id = ?1 ORDER BY timestamp ASC",
        )?;

        let transcriptions = stmt
            .query_map([recording_id.to_string()], |row| {
                let id: String = row.get(0)?;
                let rec_id: String = row.get(1)?;
                let text: String = row.get(2)?;
                let timestamp: String = row.get(3)?;
                let source: String = row.get(4)?;

                Ok(Transcription {
                    id: Uuid::parse_str(&id).unwrap_or_default(),
                    recording_id: Uuid::parse_str(&rec_id).unwrap_or_default(),
                    text,
                    timestamp: DateTime::parse_from_rfc3339(&timestamp)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    source: AudioSource::from_str(&source),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(transcriptions)
    }

    pub fn get_recording(&self, id: Uuid) -> Result<Option<Recording>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, ended_at, is_active FROM recordings WHERE id = ?1",
        )?;

        let mut rows = stmt.query([id.to_string()])?;

        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            let ended_at: Option<String> = row.get(3)?;
            let is_active: i32 = row.get(4)?;

            Ok(Some(Recording {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                name,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                ended_at: ended_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&Utc))
                        .ok()
                }),
                is_active: is_active != 0,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_recording_by_name(&self, name: &str) -> Result<Option<Recording>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, ended_at, is_active FROM recordings WHERE name = ?1",
        )?;

        let mut rows = stmt.query([name])?;

        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let created_at: String = row.get(2)?;
            let ended_at: Option<String> = row.get(3)?;
            let is_active: i32 = row.get(4)?;

            Ok(Some(Recording {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                name,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                ended_at: ended_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .map(|dt| dt.with_timezone(&Utc))
                        .ok()
                }),
                is_active: is_active != 0,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn rename_recording(&self, id: Uuid, new_name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE recordings SET name = ?1 WHERE id = ?2",
            params![new_name, id.to_string()],
        )?;
        Ok(())
    }

    pub fn delete_recording(&self, id: Uuid) -> Result<()> {
        self.conn.execute(
            "DELETE FROM transcriptions WHERE recording_id = ?1",
            [id.to_string()],
        )?;
        self.conn.execute(
            "DELETE FROM recordings WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(())
    }

    pub fn get_full_transcription_text(&self, recording_id: Uuid) -> Result<String> {
        let transcriptions = self.get_transcriptions(recording_id)?;
        let text = transcriptions
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(text)
    }
}

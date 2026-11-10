use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("app dir error: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir error: {e}"))?;
    Ok(dir.join("notely.db"))
}

pub fn open(path: &std::path::Path) -> Result<Connection, rusqlite::Error> {
    let db = Connection::open(path)?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS hotkeys (action TEXT PRIMARY KEY, binding TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS meetings (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           title TEXT NOT NULL DEFAULT '',
           profile_id TEXT NOT NULL DEFAULT '',
           started_at INTEGER NOT NULL,
           ended_at INTEGER,
           transcript_json TEXT NOT NULL DEFAULT '[]',
           notes_json TEXT,
           captures_json TEXT NOT NULL DEFAULT '[]'
         );",
    )?;
    Ok(db)
}

pub fn get_setting(db: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    let mut stmt = db.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn set_setting(db: &Connection, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    db.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_hotkeys(db: &Connection) -> Result<HashMap<String, String>, rusqlite::Error> {
    let mut stmt = db.prepare("SELECT action, binding FROM hotkeys")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut map = HashMap::new();
    for r in rows {
        let (k, v) = r?;
        map.insert(k, v);
    }
    Ok(map)
}

pub fn set_hotkeys(db: &Connection, hotkeys: &HashMap<String, String>) -> Result<(), rusqlite::Error> {
    for (k, v) in hotkeys {
        db.execute(
            "INSERT INTO hotkeys (action, binding) VALUES (?1, ?2)
             ON CONFLICT(action) DO UPDATE SET binding = excluded.binding",
            params![k, v],
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingRow {
    pub id: i64,
    pub title: String,
    pub profile_id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub transcript_json: String,
    pub notes_json: Option<String>,
}

pub fn insert_meeting(
    db: &Connection,
    title: &str,
    profile_id: &str,
    started_at: i64,
) -> Result<i64, rusqlite::Error> {
    db.execute(
        "INSERT INTO meetings (title, profile_id, started_at) VALUES (?1, ?2, ?3)",
        params![title, profile_id, started_at],
    )?;
    Ok(db.last_insert_rowid())
}

pub fn finish_meeting(
    db: &Connection,
    id: i64,
    ended_at: i64,
    transcript_json: &str,
    notes_json: Option<&str>,
) -> Result<(), rusqlite::Error> {
    db.execute(
        "UPDATE meetings SET ended_at = ?1, transcript_json = ?2, notes_json = ?3 WHERE id = ?4",
        params![ended_at, transcript_json, notes_json, id],
    )?;
    Ok(())
}

pub fn list_meetings(db: &Connection, limit: i64) -> Result<Vec<MeetingRow>, rusqlite::Error> {
    let mut stmt = db.prepare(
        "SELECT id, title, profile_id, started_at, ended_at, transcript_json, notes_json
         FROM meetings ORDER BY started_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(MeetingRow {
            id: row.get(0)?,
            title: row.get(1)?,
            profile_id: row.get(2)?,
            started_at: row.get(3)?,
            ended_at: row.get(4)?,
            transcript_json: row.get(5)?,
            notes_json: row.get(6)?,
        })
    })?;
    rows.collect()
}

pub fn search_meetings(db: &Connection, q: &str) -> Result<Vec<MeetingRow>, rusqlite::Error> {
    let like = format!("%{q}%");
    let mut stmt = db.prepare(
        "SELECT id, title, profile_id, started_at, ended_at, transcript_json, notes_json
         FROM meetings WHERE title LIKE ?1 OR transcript_json LIKE ?1 OR notes_json LIKE ?1
         ORDER BY started_at DESC LIMIT 100",
    )?;
    let rows = stmt.query_map(params![like], |row| {
        Ok(MeetingRow {
            id: row.get(0)?,
            title: row.get(1)?,
            profile_id: row.get(2)?,
            started_at: row.get(3)?,
            ended_at: row.get(4)?,
            transcript_json: row.get(5)?,
            notes_json: row.get(6)?,
        })
    })?;
    rows.collect()
}

pub fn get_meeting(db: &Connection, id: i64) -> Result<Option<MeetingRow>, rusqlite::Error> {
    let mut stmt = db.prepare(
        "SELECT id, title, profile_id, started_at, ended_at, transcript_json, notes_json
         FROM meetings WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id], |row| {
        Ok(MeetingRow {
            id: row.get(0)?,
            title: row.get(1)?,
            profile_id: row.get(2)?,
            started_at: row.get(3)?,
            ended_at: row.get(4)?,
            transcript_json: row.get(5)?,
            notes_json: row.get(6)?,
        })
    })?;
    if let Some(r) = rows.next() {
        Ok(Some(r?))
    } else {
        Ok(None)
    }
}

pub fn delete_meeting(db: &Connection, id: i64) -> Result<(), rusqlite::Error> {
    db.execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
    Ok(())
}

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
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch(
        "PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
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
         );
         CREATE TABLE IF NOT EXISTS chats (
           id TEXT PRIMARY KEY, title TEXT NOT NULL, profile_id TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL,
           started_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS chat_turns (
           request_id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
           question TEXT NOT NULL, answer TEXT NOT NULL, profile_id TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL, created_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS chat_turns_conversation ON chat_turns(conversation_id,created_at);",
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
        "UPDATE meetings SET ended_at = COALESCE(ended_at, ?1), transcript_json = ?2, notes_json = COALESCE(?3, notes_json) WHERE id = ?4",
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
    let like = format!("%{}%", q.replace('!', "!!").replace('%', "!%").replace('_', "!_"));
    let mut stmt = db.prepare(
        "SELECT id, title, profile_id, started_at, ended_at, transcript_json, notes_json
         FROM meetings WHERE title LIKE ?1 ESCAPE '!' OR transcript_json LIKE ?1 ESCAPE '!' OR notes_json LIKE ?1 ESCAPE '!'
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummary {
    pub id: String, pub title: String, pub profile_id: String, pub provider: String, pub model: String,
    pub started_at: i64, pub updated_at: i64, pub turn_count: i64,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurn {
    pub request_id: String, pub question: String, pub answer: String, pub profile_id: String,
    pub provider: String, pub model: String, pub created_at: i64,
}

pub fn save_chat_turn(db: &Connection, conversation_id: &str, turn: &ChatTurn) -> Result<(), rusqlite::Error> {
    let tx = db.unchecked_transaction()?;
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM chat_turns WHERE request_id=?1)", [&turn.request_id], |r| r.get::<_, bool>(0))? { return Ok(()); }
    let title: String = turn.question.chars().take(90).collect();
    tx.execute("INSERT INTO chats (id,title,profile_id,provider,model,started_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?6)
      ON CONFLICT(id) DO UPDATE SET profile_id=excluded.profile_id,provider=excluded.provider,model=excluded.model,updated_at=excluded.updated_at",
      params![conversation_id,title,turn.profile_id,turn.provider,turn.model,turn.created_at])?;
    tx.execute("INSERT INTO chat_turns (request_id,conversation_id,question,answer,profile_id,provider,model,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
      params![turn.request_id,conversation_id,turn.question,turn.answer,turn.profile_id,turn.provider,turn.model,turn.created_at])?;
    tx.commit()
}

pub fn list_chats(db: &Connection, query: &str) -> Result<Vec<ChatSummary>, rusqlite::Error> {
    let like = format!("%{}%", query.replace('!', "!!").replace('%', "!%").replace('_', "!_"));
    let mut stmt = db.prepare("SELECT c.id,c.title,c.profile_id,c.provider,c.model,c.started_at,c.updated_at,(SELECT COUNT(*) FROM chat_turns WHERE conversation_id=c.id)
      FROM chats c WHERE c.title LIKE ?1 ESCAPE '!' OR EXISTS (SELECT 1 FROM chat_turns t WHERE t.conversation_id=c.id AND (t.question LIKE ?1 ESCAPE '!' OR t.answer LIKE ?1 ESCAPE '!')) ORDER BY c.updated_at DESC LIMIT 100")?;
    let rows = stmt.query_map([like], |r| Ok(ChatSummary { id:r.get(0)?, title:r.get(1)?, profile_id:r.get(2)?, provider:r.get(3)?, model:r.get(4)?, started_at:r.get(5)?, updated_at:r.get(6)?, turn_count:r.get(7)? }))?;
    rows.collect()
}
pub fn chat_turns(db: &Connection, id: &str) -> Result<Vec<ChatTurn>, rusqlite::Error> {
    let mut stmt = db.prepare("SELECT request_id,question,answer,profile_id,provider,model,created_at FROM chat_turns WHERE conversation_id=?1 ORDER BY created_at,rowid")?;
    let rows = stmt.query_map([id], |r| Ok(ChatTurn { request_id:r.get(0)?,question:r.get(1)?,answer:r.get(2)?,profile_id:r.get(3)?,provider:r.get(4)?,model:r.get(5)?,created_at:r.get(6)? }))?;
    rows.collect()
}
pub fn chat_context(db: &Connection, id: &str) -> Result<String, rusqlite::Error> {
    let mut stmt = db.prepare("SELECT question,answer FROM chat_turns WHERE conversation_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 8")?;
    let rows = stmt.query_map([id], |r| Ok((r.get::<_, String>(0)?,r.get::<_, String>(1)?)))?;
    let mut turns: Vec<_> = rows.collect::<Result<_,_>>()?;
    turns.reverse();
    Ok(turns.iter().map(|(question,answer)| format!("User: {}\nNotely: {}", question.chars().take(1500).collect::<String>(), answer.chars().take(3500).collect::<String>())).collect::<Vec<_>>().join("\n\n"))
}

pub fn delete_chat(db: &Connection, id: &str) -> Result<(), rusqlite::Error> {
    db.execute("DELETE FROM chats WHERE id=?1",[id])?;
    Ok(())
}

#[cfg(test)]
mod history_tests {
    use super::*;
    #[test]
    fn chats_persist_search_resume_and_delete_without_touching_meetings() {
        let path = std::env::temp_dir().join(format!("notely-history-{}.db", uuid::Uuid::new_v4()));
        let db = open(&path).unwrap();
        let meeting = insert_meeting(&db,"Design review","meeting",100).unwrap();
        finish_meeting(&db,meeting,200,"[]",None).unwrap();
        finish_meeting(&db,meeting,900,"[]",Some("{\"summary\":\"Decision agreed\"}")).unwrap();
        assert_eq!(get_meeting(&db,meeting).unwrap().unwrap().ended_at,Some(200));
        let mut turn = ChatTurn { request_id:"request-1".into(),question:"Plan the café launch".into(),answer:"Budget is 25% of revenue".into(),profile_id:"general".into(),provider:"openai".into(),model:"test-model".into(),created_at:300 };
        save_chat_turn(&db,"chat-1",&turn).unwrap();
        save_chat_turn(&db,"chat-1",&turn).unwrap(); // Event retries must be idempotent.
        turn.request_id="request-2".into(); turn.question="What next?".into();turn.answer="Draft the announcement".into();turn.profile_id="writing".into();turn.created_at=400;
        save_chat_turn(&db,"chat-1",&turn).unwrap();
        turn.request_id="request-3".into();turn.question="Other conversation".into();turn.created_at=500;
        save_chat_turn(&db,"chat-2",&turn).unwrap();
        drop(db);
        let db = open(&path).unwrap();
        let rows = list_chats(&db,"").unwrap();
        assert_eq!(rows.len(),2); assert_eq!(rows[0].id,"chat-2");
        assert_eq!(rows[1].turn_count,2);assert_eq!(rows[1].title,"Plan the café launch");assert_eq!(rows[1].profile_id,"writing");
        assert_eq!(list_chats(&db,"25%").unwrap().len(),1);
        assert_eq!(list_chats(&db,"café").unwrap().len(),1);
        assert!(list_chats(&db,"_").unwrap().is_empty());
        assert!(list_chats(&db,"' OR 1=1 --").unwrap().is_empty());
        assert_eq!(chat_turns(&db,"chat-1").unwrap().len(),2);
        let context = chat_context(&db,"chat-1").unwrap();
        assert!(context.contains("25% of revenue") && context.contains("What next?"));
        assert!(!context.contains("Other conversation"));
        delete_chat(&db,"chat-1").unwrap();
        assert!(chat_turns(&db,"chat-1").unwrap().is_empty());
        assert_eq!(list_chats(&db,"").unwrap().len(),1);
        assert_eq!(list_meetings(&db,100).unwrap().len(),1);
        drop(db);std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn followup_context_is_bounded_and_keeps_recent_unicode() {
        let db = open(std::path::Path::new(":memory:")).unwrap();
        for i in 0..12 {
            save_chat_turn(&db,"chat",&ChatTurn { request_id:format!("r{i}"),question:format!("Q{i}"),answer:"é".repeat(5000),profile_id:"general".into(),provider:"claude".into(),model:"test".into(),created_at:i }).unwrap();
        }
        let context = chat_context(&db,"chat").unwrap();
        assert!(!context.contains("User: Q0\n"));assert!(context.contains("User: Q11\n"));
        assert!(context.chars().count() < 41000);
    }
}

pub fn checkpoint_meeting(db:&Connection,id:i64,transcript:&str)->Result<(),rusqlite::Error>{
    db.execute("UPDATE meetings SET transcript_json=?1 WHERE id=?2 AND ended_at IS NULL",params![transcript,id])?;Ok(())
}
pub fn save_meeting_notes(db:&Connection,id:i64,notes:&str)->Result<(),rusqlite::Error>{
    if db.execute("UPDATE meetings SET notes_json=?1 WHERE id=?2",params![notes,id])?==0{return Err(rusqlite::Error::QueryReturnedNoRows);}
    Ok(())
}

#[cfg(test)]
mod meeting_tests {
    use super::*;
    #[test]
    fn transcript_checkpoints_survive_restart_and_notes_do_not_change_end_time(){
        let path=std::env::temp_dir().join(format!("notely-meeting-{}.db",uuid::Uuid::new_v4()));
        let db=open(&path).unwrap();let id=insert_meeting(&db,"Fixture meeting","meeting",100).unwrap();
        let transcript:Vec<_>=(0..650).map(|i|crate::state::TranscriptLine{id:format!("line-{i}"),speaker:if i%2==0{"me"}else{"other"}.into(),text:format!("Meeting detail {i}: café launch"),is_final:true,started_at:100+i}).collect();
        let json=serde_json::to_string(&transcript).unwrap();checkpoint_meeting(&db,id,&json).unwrap();drop(db);
        let db=open(&path).unwrap();let saved=get_meeting(&db,id).unwrap().unwrap();assert!(saved.ended_at.is_none());
        let lines:Vec<crate::state::TranscriptLine>=serde_json::from_str(&saved.transcript_json).unwrap();assert_eq!(lines.len(),650);assert_eq!(lines[0].id,"line-0");assert_eq!(lines[649].id,"line-649");
        finish_meeting(&db,id,900,&json,None).unwrap();
        save_meeting_notes(&db,id,r#"{"summary":"Café launch agreed","decisions":[],"actionItems":[],"followUpEmail":""}"#).unwrap();
        checkpoint_meeting(&db,id,"[]").unwrap(); // Late packets cannot replace a finished transcript.
        let row=get_meeting(&db,id).unwrap().unwrap();assert_eq!(row.ended_at,Some(900));assert_eq!(row.transcript_json,json);assert!(row.notes_json.is_some());
        drop(db);std::fs::remove_file(path).unwrap();
    }
}

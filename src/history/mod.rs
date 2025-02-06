use anyhow::{Context, Result};
use chrono::prelude::*;
use once_cell::sync::OnceCell;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::PathBuf;

use crate::app::Message;

static DB_POOL: OnceCell<Pool<SqliteConnectionManager>> = OnceCell::new();

pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub model: String,
    pub timestamp: DateTime<Local>,
    pub tags: Vec<String>,
    pub parent_id: Option<i64>,
}

pub struct ConversationPreview {
    pub id: i64,
    pub title: String,
    pub timestamp: DateTime<Local>,
    pub message_count: i64,
}

pub fn init_db() -> Result<()> {
    let db_path = get_db_path()?;
    
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let manager = SqliteConnectionManager::file(&db_path);
    let pool = Pool::new(manager)?;
    
    // Create tables
    let conn = pool.get()?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS conversations (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            model TEXT NOT NULL,
            timestamp DATETIME NOT NULL,
            tags TEXT,
            parent_id INTEGER,
            FOREIGN KEY(parent_id) REFERENCES conversations(id)
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY,
            conversation_id INTEGER NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp DATETIME NOT NULL,
            FOREIGN KEY(conversation_id) REFERENCES conversations(id)
        )",
        [],
    )?;

    // Create full-text search index
    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            content,
            content='messages',
            content_rowid='id'
        )",
        [],
    )?;

    // Create trigger to keep FTS index up to date
    conn.execute(
        "CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
        END",
        [],
    )?;

    DB_POOL.set(pool).unwrap();
    Ok(())
}

pub fn save_conversation(title: &str, model: &str, messages: &[Message], tags: &[String]) -> Result<i64> {
    let conn = DB_POOL.get().context("Database not initialized")?.get()?;
    let timestamp = Local::now();

    conn.execute(
        "INSERT INTO conversations (title, model, timestamp, tags, parent_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            title,
            model,
            timestamp,
            tags.join(","),
            None::<i64>
        ],
    )?;

    let conversation_id = conn.last_insert_rowid();

    for message in messages {
        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![
                conversation_id,
                message.role,
                message.content,
                message.timestamp
            ],
        )?;
    }

    Ok(conversation_id)
}

pub fn load_conversation(id: i64) -> Result<(Conversation, Vec<Message>)> {
    let conn = DB_POOL.get().context("Database not initialized")?.get()?;

    let conversation = conn.query_row(
        "SELECT id, title, model, timestamp, tags, parent_id FROM conversations WHERE id = ?1",
        params![id],
        |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                timestamp: row.get(3)?,
                tags: row.get::<_, String>(4)?
                    .split(',')
                    .map(|s| s.to_string())
                    .collect(),
                parent_id: row.get(5)?,
            })
        },
    )?;

    let mut stmt = conn.prepare(
        "SELECT role, content, timestamp FROM messages 
         WHERE conversation_id = ?1 
         ORDER BY timestamp ASC"
    )?;

    let messages = stmt.query_map(params![id], |row| {
        Ok(Message {
            role: row.get(0)?,
            content: row.get(1)?,
            timestamp: row.get(2)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok((conversation, messages))
}

pub fn search_conversations(query: &str) -> Result<Vec<ConversationPreview>> {
    let conn = DB_POOL.get().context("Database not initialized")?.get()?;

    let mut stmt = conn.prepare(
        "SELECT DISTINCT 
            c.id,
            c.title,
            c.timestamp,
            (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
         FROM conversations c
         JOIN messages m ON m.conversation_id = c.id
         JOIN messages_fts f ON f.rowid = m.id
         WHERE messages_fts MATCH ?1
         ORDER BY c.timestamp DESC"
    )?;

    let results = stmt.query_map(params![query], |row| {
        Ok(ConversationPreview {
            id: row.get(0)?,
            title: row.get(1)?,
            timestamp: row.get(2)?,
            message_count: row.get(3)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

pub fn list_recent_conversations(limit: i64) -> Result<Vec<ConversationPreview>> {
    let conn = DB_POOL.get().context("Database not initialized")?.get()?;

    let mut stmt = conn.prepare(
        "SELECT 
            c.id,
            c.title,
            c.timestamp,
            (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
         FROM conversations c
         ORDER BY c.timestamp DESC
         LIMIT ?1"
    )?;

    let results = stmt.query_map(params![limit], |row| {
        Ok(ConversationPreview {
            id: row.get(0)?,
            title: row.get(1)?,
            timestamp: row.get(2)?,
            message_count: row.get(3)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

pub fn fork_conversation(parent_id: i64, model: &str, messages: &[Message], tags: &[String]) -> Result<i64> {
    let conn = DB_POOL.get().context("Database not initialized")?.get()?;
    let timestamp = Local::now();

    // Get parent conversation title
    let parent_title = conn.query_row(
        "SELECT title FROM conversations WHERE id = ?1",
        params![parent_id],
        |row| row.get::<_, String>(0)
    )?;

    let title = format!("{} (fork)", parent_title);

    conn.execute(
        "INSERT INTO conversations (title, model, timestamp, tags, parent_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            title,
            model,
            timestamp,
            tags.join(","),
            Some(parent_id)
        ],
    )?;

    let conversation_id = conn.last_insert_rowid();

    for message in messages {
        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, timestamp) VALUES (?1, ?2, ?3, ?4)",
            params![
                conversation_id,
                message.role,
                message.content,
                message.timestamp
            ],
        )?;
    }

    Ok(conversation_id)
}

pub fn get_conversation_tree(root_id: i64) -> Result<Vec<Conversation>> {
    let conn = DB_POOL.get().context("Database not initialized")?.get()?;
    
    // Get both ancestors and descendants using two recursive CTEs
    let mut stmt = conn.prepare(
        "WITH RECURSIVE
        ancestors AS (
            SELECT id, title, model, timestamp, tags, parent_id, 0 as depth
            FROM conversations
            WHERE id = ?1
            UNION ALL
            SELECT c.id, c.title, c.model, c.timestamp, c.tags, c.parent_id, a.depth - 1
            FROM conversations c
            JOIN ancestors a ON c.id = a.parent_id
        ),
        descendants AS (
            SELECT id, title, model, timestamp, tags, parent_id, 0 as depth
            FROM conversations
            WHERE id = ?1
            UNION ALL
            SELECT c.id, c.title, c.model, c.timestamp, c.tags, c.parent_id, d.depth + 1
            FROM conversations c
            JOIN descendants d ON c.parent_id = d.id
        )
        SELECT id, title, model, timestamp, tags, parent_id
        FROM (SELECT * FROM ancestors UNION SELECT * FROM descendants)
        GROUP BY id
        ORDER BY parent_id NULLS FIRST, timestamp ASC"
    )?;

    let results = stmt.query_map(params![root_id], |row| {
        Ok(Conversation {
            id: row.get(0)?,
            title: row.get(1)?,
            model: row.get(2)?,
            timestamp: row.get(3)?,
            tags: row.get::<_, String>(4)?
                .split(',')
                .map(|s| s.to_string())
                .collect(),
            parent_id: row.get(5)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

fn get_db_path() -> Result<PathBuf> {
    let mut path = dirs::config_dir()
        .context("Failed to determine config directory")?;
    path.push("llm-chat");
    path.push("history.db");
    Ok(path)
}

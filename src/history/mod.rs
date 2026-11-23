use anyhow::{Context, Result};
use chrono::prelude::*;
use rusqlite::{params, Connection, Row};
use std::path::PathBuf;

use crate::app::Message;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS conversations (
        id INTEGER PRIMARY KEY,
        title TEXT NOT NULL,
        model TEXT NOT NULL,
        timestamp DATETIME NOT NULL,
        tags TEXT,
        parent_id INTEGER,
        FOREIGN KEY(parent_id) REFERENCES conversations(id)
    );

    CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY,
        conversation_id INTEGER NOT NULL,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        timestamp DATETIME NOT NULL,
        FOREIGN KEY(conversation_id) REFERENCES conversations(id)
    );

    CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
        content,
        content='messages',
        content_rowid='id'
    );

    CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
        INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
    END;
";

pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub model: String,
    pub parent_id: Option<i64>,
}

pub struct ConversationPreview {
    pub id: i64,
    pub title: String,
    pub timestamp: DateTime<Local>,
    pub message_count: i64,
}

pub struct History {
    conn: Connection,
}

impl History {
    pub fn open_default() -> Result<Self> {
        let db_path = get_db_path()?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create the history directory")?;
        }
        let conn = Connection::open(&db_path).context("Failed to open the history database")?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)
            .context("Failed to create the history tables")?;
        Ok(Self { conn })
    }

    pub fn save_conversation(&self, title: &str, model: &str, messages: &[Message]) -> Result<i64> {
        self.insert_conversation(title, model, None, messages)
    }

    pub fn fork_conversation(
        &self,
        parent_id: i64,
        model: &str,
        messages: &[Message],
    ) -> Result<i64> {
        let parent_title: String = self.conn.query_row(
            "SELECT title FROM conversations WHERE id = ?1",
            params![parent_id],
            |row| row.get(0),
        )?;
        let title = format!("{} (fork)", parent_title);
        self.insert_conversation(&title, model, Some(parent_id), messages)
    }

    pub fn append_messages(&self, conversation_id: i64, messages: &[Message]) -> Result<()> {
        let transaction = self.conn.unchecked_transaction()?;
        insert_messages(&transaction, conversation_id, messages)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_conversation(&self, id: i64) -> Result<(Conversation, Vec<Message>)> {
        let conversation = self.conn.query_row(
            "SELECT id, title, model, parent_id FROM conversations WHERE id = ?1",
            params![id],
            conversation_from_row,
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT role, content, timestamp FROM messages
             WHERE conversation_id = ?1
             ORDER BY id ASC",
        )?;
        let messages = stmt
            .query_map(params![id], |row| {
                Ok(Message {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    timestamp: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok((conversation, messages))
    }

    pub fn search_conversations(&self, query: &str) -> Result<Vec<ConversationPreview>> {
        let query = fts_query(query);
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT
                c.id,
                c.title,
                c.timestamp,
                (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
             FROM conversations c
             JOIN messages m ON m.conversation_id = c.id
             JOIN messages_fts f ON f.rowid = m.id
             WHERE messages_fts MATCH ?1
             ORDER BY c.timestamp DESC, c.id DESC",
        )?;
        let results = stmt
            .query_map(params![query], preview_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub fn list_recent_conversations(&self, limit: i64) -> Result<Vec<ConversationPreview>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                c.id,
                c.title,
                c.timestamp,
                (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
             FROM conversations c
             ORDER BY c.timestamp DESC, c.id DESC
             LIMIT ?1",
        )?;
        let results = stmt
            .query_map(params![limit], preview_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub fn conversation_tree(&self, id: i64) -> Result<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            "WITH RECURSIVE
             ancestors(id, parent_id) AS (
                 SELECT id, parent_id FROM conversations WHERE id = ?1
                 UNION ALL
                 SELECT c.id, c.parent_id
                 FROM conversations c
                 JOIN ancestors a ON c.id = a.parent_id
             ),
             family(id) AS (
                 SELECT id FROM ancestors WHERE parent_id IS NULL
                 UNION ALL
                 SELECT c.id
                 FROM conversations c
                 JOIN family f ON c.parent_id = f.id
             )
             SELECT c.id, c.title, c.model, c.parent_id
             FROM conversations c
             JOIN family f ON c.id = f.id
             ORDER BY c.id ASC",
        )?;
        let results = stmt
            .query_map(params![id], conversation_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    fn insert_conversation(
        &self,
        title: &str,
        model: &str,
        parent_id: Option<i64>,
        messages: &[Message],
    ) -> Result<i64> {
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO conversations (title, model, timestamp, tags, parent_id) VALUES (?1, ?2, ?3, '', ?4)",
            params![title, model, Local::now(), parent_id],
        )?;
        let conversation_id = transaction.last_insert_rowid();
        insert_messages(&transaction, conversation_id, messages)?;
        transaction.commit()?;
        Ok(conversation_id)
    }
}

fn insert_messages(conn: &Connection, conversation_id: i64, messages: &[Message]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO messages (conversation_id, role, content, timestamp) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for message in messages {
        stmt.execute(params![
            conversation_id,
            message.role,
            message.content,
            message.timestamp
        ])?;
    }
    Ok(())
}

fn conversation_from_row(row: &Row) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get(0)?,
        title: row.get(1)?,
        model: row.get(2)?,
        parent_id: row.get(3)?,
    })
}

fn preview_from_row(row: &Row) -> rusqlite::Result<ConversationPreview> {
    Ok(ConversationPreview {
        id: row.get(0)?,
        title: row.get(1)?,
        timestamp: row.get(2)?,
        message_count: row.get(3)?,
    })
}

fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn get_db_path() -> Result<PathBuf> {
    let mut path = dirs::config_dir().context("Failed to determine config directory")?;
    path.push("llm-chat");
    path.push("history.db");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str) -> Message {
        Message::new(role, content)
    }

    fn contents(messages: &[Message]) -> Vec<(&str, &str)> {
        messages
            .iter()
            .map(|message| (message.role.as_str(), message.content.as_str()))
            .collect()
    }

    #[test]
    fn saved_conversation_loads_back_in_insertion_order() -> Result<()> {
        let history = History::open_in_memory()?;
        let at = Local::now();
        let messages: Vec<Message> = [
            ("user", "How do I read a file in Rust?"),
            ("assistant", "Call std::fs::read_to_string."),
            ("user", "And line by line?"),
        ]
        .into_iter()
        .map(|(role, content)| Message {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: at,
        })
        .collect();

        let id = history.save_conversation(
            "How do I read a file in Rust?",
            "openai/gpt-4o",
            &messages,
        )?;
        let (conversation, loaded) = history.load_conversation(id)?;

        assert_eq!(conversation.id, id);
        assert_eq!(conversation.title, "How do I read a file in Rust?");
        assert_eq!(conversation.model, "openai/gpt-4o");
        assert_eq!(conversation.parent_id, None);
        assert_eq!(contents(&loaded), contents(&messages));
        assert_eq!(loaded[0].timestamp, at);
        Ok(())
    }

    #[test]
    fn appended_messages_extend_the_same_conversation() -> Result<()> {
        let history = History::open_in_memory()?;
        let id = history.save_conversation(
            "first question",
            "openai/gpt-4o",
            &[
                message("user", "first question"),
                message("assistant", "first answer"),
            ],
        )?;
        history.append_messages(
            id,
            &[
                message("user", "second question"),
                message("assistant", "second answer"),
            ],
        )?;

        let (_, loaded) = history.load_conversation(id)?;
        assert_eq!(
            contents(&loaded),
            vec![
                ("user", "first question"),
                ("assistant", "first answer"),
                ("user", "second question"),
                ("assistant", "second answer"),
            ]
        );

        let recent = history.list_recent_conversations(10)?;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, id);
        assert_eq!(recent[0].message_count, 4);
        Ok(())
    }

    #[test]
    fn recent_conversations_come_newest_first_up_to_the_limit() -> Result<()> {
        let history = History::open_in_memory()?;
        let first =
            history.save_conversation("first", "openai/gpt-4o", &[message("user", "first")])?;
        let second =
            history.save_conversation("second", "openai/gpt-4o", &[message("user", "second")])?;
        let third =
            history.save_conversation("third", "openai/gpt-4o", &[message("user", "third")])?;

        let ids: Vec<i64> = history
            .list_recent_conversations(2)?
            .into_iter()
            .map(|preview| preview.id)
            .collect();
        assert_eq!(ids, vec![third, second]);
        assert!(!ids.contains(&first));
        Ok(())
    }

    #[test]
    fn search_finds_conversations_by_the_words_in_their_messages() -> Result<()> {
        let history = History::open_in_memory()?;
        let rust = history.save_conversation(
            "How do I parse JSON in Rust?",
            "openai/gpt-4o",
            &[
                message("user", "How do I parse JSON in Rust?"),
                message("assistant", "Add serde_json and call from_str in main.rs."),
            ],
        )?;
        let python = history.save_conversation(
            "How do I parse JSON in Python?",
            "openai/gpt-4o",
            &[
                message("user", "How do I parse JSON in Python?"),
                message("assistant", "Use json.loads, it's in the standard library."),
            ],
        )?;

        let found = |query: &str| -> Result<Vec<i64>> {
            Ok(history
                .search_conversations(query)?
                .into_iter()
                .map(|preview| preview.id)
                .collect())
        };

        assert_eq!(found("serde_json")?, vec![rust]);
        assert_eq!(found("parse json")?, vec![python, rust]);
        assert_eq!(found("main.rs")?, vec![rust]);
        assert_eq!(found("it's")?, vec![python]);
        assert_eq!(found("\"standard library\"")?, vec![python]);
        assert!(found("haskell")?.is_empty());
        assert!(found("   ")?.is_empty());

        let previews = history.search_conversations("serde_json")?;
        assert_eq!(previews[0].title, "How do I parse JSON in Rust?");
        assert_eq!(previews[0].message_count, 2);
        Ok(())
    }

    #[test]
    fn search_sees_messages_appended_later() -> Result<()> {
        let history = History::open_in_memory()?;
        let id = history.save_conversation(
            "question",
            "openai/gpt-4o",
            &[message("user", "question")],
        )?;
        assert!(history.search_conversations("tokio")?.is_empty());

        history.append_messages(id, &[message("assistant", "Spawn a tokio task.")])?;
        let found: Vec<i64> = history
            .search_conversations("tokio")?
            .into_iter()
            .map(|preview| preview.id)
            .collect();
        assert_eq!(found, vec![id]);
        Ok(())
    }

    #[test]
    fn fork_copies_the_messages_under_the_new_model() -> Result<()> {
        let history = History::open_in_memory()?;
        let shared = [
            message("user", "Explain ownership"),
            message("assistant", "Every value has a single owner."),
        ];
        let parent = history.save_conversation(
            "Explain ownership",
            "anthropic/claude-3.5-sonnet",
            &shared,
        )?;
        let fork = history.fork_conversation(parent, "openai/gpt-4o", &shared)?;
        history.append_messages(fork, &[message("user", "Now explain borrowing")])?;

        let (forked, fork_messages) = history.load_conversation(fork)?;
        assert_eq!(forked.title, "Explain ownership (fork)");
        assert_eq!(forked.model, "openai/gpt-4o");
        assert_eq!(forked.parent_id, Some(parent));
        assert_eq!(
            contents(&fork_messages),
            vec![
                ("user", "Explain ownership"),
                ("assistant", "Every value has a single owner."),
                ("user", "Now explain borrowing"),
            ]
        );

        let (original, parent_messages) = history.load_conversation(parent)?;
        assert_eq!(original.model, "anthropic/claude-3.5-sonnet");
        assert_eq!(contents(&parent_messages), contents(&shared));
        Ok(())
    }

    #[test]
    fn forking_a_missing_conversation_fails() -> Result<()> {
        let history = History::open_in_memory()?;
        assert!(history.fork_conversation(42, "openai/gpt-4o", &[]).is_err());
        assert!(history.list_recent_conversations(10)?.is_empty());
        Ok(())
    }

    #[test]
    fn tree_holds_every_branch_of_the_family() -> Result<()> {
        let history = History::open_in_memory()?;
        let root = history.save_conversation(
            "root",
            "anthropic/claude-3.5-sonnet",
            &[message("user", "root")],
        )?;
        let unrelated = history.save_conversation(
            "unrelated",
            "openai/gpt-4o",
            &[message("user", "unrelated")],
        )?;
        let first = history.fork_conversation(root, "openai/gpt-4o", &[])?;
        let second = history.fork_conversation(root, "openai/gpt-4", &[])?;
        let nested = history.fork_conversation(first, "anthropic/claude-3.5-sonnet", &[])?;

        let tree = |id: i64| -> Result<Vec<(i64, Option<i64>)>> {
            Ok(history
                .conversation_tree(id)?
                .into_iter()
                .map(|conversation| (conversation.id, conversation.parent_id))
                .collect())
        };
        let family = vec![
            (root, None),
            (first, Some(root)),
            (second, Some(root)),
            (nested, Some(first)),
        ];

        assert_eq!(tree(root)?, family);
        assert_eq!(tree(second)?, family);
        assert_eq!(tree(nested)?, family);
        assert_eq!(tree(unrelated)?, vec![(unrelated, None)]);
        Ok(())
    }
}

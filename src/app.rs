use std::cell::Cell;

use anyhow::anyhow;
use chrono::{DateTime, Local};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::task::AbortHandle;

use crate::api::{ChatMessage, OpenRouterClient, StreamEvent};
use crate::commands::clipboard;
use crate::config::{self, Config};
use crate::history::{ConversationPreview, History};

#[derive(Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Local>,
}

impl Message {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: content.into(),
            timestamp: Local::now(),
        }
    }

    fn is_chat(&self) -> bool {
        self.role == "user" || self.role == "assistant"
    }
}

struct PendingReply {
    reply: Message,
    events: UnboundedReceiver<StreamEvent>,
    task: AbortHandle,
}

pub struct App {
    pub messages: Vec<Message>,
    pub input: String,
    pub input_cursor_position: usize,
    pub input_cursor_line: usize,
    pub input_mode: InputMode,
    pub current_model: String,
    pub should_quit: bool,
    pub current_conversation_id: Option<i64>,
    pub show_tree: bool,
    pub scroll_offset: u16,
    pub max_scroll: Cell<u16>,
    pub scroll_speed: u16,
    pub search_query: String,
    pub search_mode: bool,
    pub search_results: Vec<usize>,
    pub current_search_index: usize,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub history: History,
    client: Option<OpenRouterClient>,
    pending: Option<PendingReply>,
    saved_count: usize,
}

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
    Command,
}

impl App {
    pub fn new(config: Config, history: History) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            input_cursor_position: 0,
            input_cursor_line: 0,
            input_mode: InputMode::Normal,
            current_model: config
                .default_model
                .unwrap_or_else(|| config::DEFAULT_MODEL.to_string()),
            should_quit: false,
            current_conversation_id: None,
            show_tree: false,
            scroll_offset: 0,
            max_scroll: Cell::new(0),
            scroll_speed: 3,
            search_query: String::new(),
            search_mode: false,
            search_results: Vec::new(),
            current_search_index: 0,
            input_history: Vec::new(),
            history_index: None,
            history,
            client: config.api_key.map(OpenRouterClient::new),
            pending: None,
            saved_count: 0,
        }
    }

    pub fn pending_reply(&self) -> Option<&Message> {
        self.pending.as_ref().map(|pending| &pending.reply)
    }

    pub fn handle_input(&mut self, event: Event) {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                self.handle_key_event(key);
            }
        }
    }

    pub async fn next_stream_event(&mut self) -> StreamEvent {
        match self.pending.as_mut() {
            Some(pending) => pending.events.recv().await.unwrap_or_else(|| {
                StreamEvent::Finished(Err(anyhow!("The request stopped before it finished")))
            }),
            None => std::future::pending().await,
        }
    }

    pub fn handle_stream_event(&mut self, event: StreamEvent) {
        self.apply_stream_event(event);
        while let Some(event) = self
            .pending
            .as_mut()
            .and_then(|pending| pending.events.try_recv().ok())
        {
            self.apply_stream_event(event);
        }
    }

    fn apply_stream_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::Delta(text) => {
                if let Some(pending) = self.pending.as_mut() {
                    pending.reply.content.push_str(&text);
                }
            }
            StreamEvent::Finished(outcome) => {
                self.finish_reply();
                if let Err(error) = outcome {
                    self.notice(format!("Error: {error:#}"));
                }
            }
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.cancel_reply();
            return;
        }
        match self.input_mode {
            InputMode::Normal => self.handle_normal_mode(key),
            InputMode::Editing => self.handle_editing_mode(key),
            InputMode::Command => self.handle_command_mode(key),
        }
    }

    fn handle_normal_mode(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('i') => self.input_mode = InputMode::Editing,
            KeyCode::Char(':') => {
                self.input_mode = InputMode::Command;
                self.input.clear();
                self.input.push(':');
            }
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(self.scroll_speed),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(self.scroll_speed),
            KeyCode::Char('J') => self.scroll_down(self.scroll_speed * 5),
            KeyCode::Char('K') => self.scroll_up(self.scroll_speed * 5),
            KeyCode::PageDown | KeyCode::Char('f') => self.scroll_down(20),
            KeyCode::PageUp | KeyCode::Char('b') => self.scroll_up(20),
            KeyCode::Char('d') => self.scroll_down(10),
            KeyCode::Char('u') => self.scroll_up(10),
            KeyCode::Char('G') => self.scroll_offset = u16::MAX,
            KeyCode::Char('g') => self.scroll_offset = 0,
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.input_mode = InputMode::Command;
                self.input.clear();
                self.input.push('/');
            }
            KeyCode::Char('n') if !self.search_results.is_empty() => {
                self.current_search_index =
                    (self.current_search_index + 1) % self.search_results.len();
                self.jump_to_message(self.search_results[self.current_search_index]);
            }
            KeyCode::Char('N') if !self.search_results.is_empty() => {
                if self.current_search_index == 0 {
                    self.current_search_index = self.search_results.len() - 1;
                } else {
                    self.current_search_index -= 1;
                }
                self.jump_to_message(self.search_results[self.current_search_index]);
            }
            _ => {}
        }
    }

    fn handle_editing_mode(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.input_mode = InputMode::Normal;
                self.input_cursor_position = 0;
                self.input_cursor_line = 0;
            }
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if !self.input.trim().is_empty() {
                    self.submit_message();
                }
            }
            (KeyCode::Enter, _) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
                self.insert_char('\n');
                self.input_cursor_line += 1;
                self.input_cursor_position = 0;
            }
            (KeyCode::Char(c), _) => {
                self.insert_char(c);
                self.input_cursor_position += 1;
            }
            (KeyCode::Backspace, _) => {
                let index = self.cursor_byte_index();
                if let Some(removed) = self.input[..index].chars().next_back() {
                    if removed == '\n' {
                        self.input_cursor_line = self.input_cursor_line.saturating_sub(1);
                        self.input_cursor_position = self.line_length(self.input_cursor_line);
                    } else {
                        self.input_cursor_position = self.input_cursor_position.saturating_sub(1);
                    }
                    self.input.remove(index - removed.len_utf8());
                }
            }
            (KeyCode::Left, _) => {
                if self.input_cursor_position > 0 {
                    self.input_cursor_position -= 1;
                } else if self.input_cursor_line > 0 {
                    self.input_cursor_line -= 1;
                    self.input_cursor_position = self.line_length(self.input_cursor_line);
                }
            }
            (KeyCode::Right, _) => {
                if self.input_cursor_position < self.line_length(self.input_cursor_line) {
                    self.input_cursor_position += 1;
                } else if self.input_cursor_line + 1 < self.line_count() {
                    self.input_cursor_line += 1;
                    self.input_cursor_position = 0;
                }
            }
            (KeyCode::Up, KeyModifiers::NONE) => {
                if self.line_count() <= 1 {
                    self.navigate_history_backward();
                } else if self.input_cursor_line > 0 {
                    self.input_cursor_line -= 1;
                    self.input_cursor_position = self
                        .input_cursor_position
                        .min(self.line_length(self.input_cursor_line));
                }
            }
            (KeyCode::Down, KeyModifiers::NONE) => {
                if self.line_count() <= 1 {
                    self.navigate_history_forward();
                } else if self.input_cursor_line + 1 < self.line_count() {
                    self.input_cursor_line += 1;
                    self.input_cursor_position = self
                        .input_cursor_position
                        .min(self.line_length(self.input_cursor_line));
                }
            }
            (KeyCode::Up, KeyModifiers::ALT) => self.navigate_history_backward(),
            (KeyCode::Down, KeyModifiers::ALT) => self.navigate_history_forward(),
            (KeyCode::Home, _) => self.input_cursor_position = 0,
            (KeyCode::End, _) => {
                self.input_cursor_position = self.line_length(self.input_cursor_line);
            }
            _ => {}
        }
    }

    fn handle_command_mode(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
            KeyCode::Enter => {
                self.execute_command();
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace if self.input.len() > 1 => {
                self.input.pop();
            }
            _ => {}
        }
    }

    fn submit_message(&mut self) {
        if self.reply_in_progress() {
            return;
        }
        let Some(client) = self.client.clone() else {
            self.notice(format!(
                "No API key. Set {} before starting, or add api_key to {} and run :reload.",
                config::API_KEY_VARIABLE,
                config::get_config_path()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| String::from("the config file")),
            ));
            return;
        };

        let content = std::mem::take(&mut self.input);
        self.input_history.push(content.clone());
        self.history_index = None;
        self.input_cursor_position = 0;
        self.input_cursor_line = 0;
        self.messages.push(Message::new("user", content));
        self.scroll_offset = u16::MAX;

        let request = self.chat_messages();
        let model = self.current_model.clone();
        let (sender, events) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let outcome = client.stream_chat(&model, &request, &sender).await;
            sender.send(StreamEvent::Finished(outcome))
        });

        self.pending = Some(PendingReply {
            reply: Message::new("assistant", ""),
            events,
            task: task.abort_handle(),
        });
    }

    fn chat_messages(&self) -> Vec<ChatMessage> {
        self.messages
            .iter()
            .filter(|message| message.is_chat())
            .map(|message| ChatMessage {
                role: message.role.clone(),
                content: message.content.clone(),
            })
            .collect()
    }

    fn cancel_reply(&mut self) {
        if self.pending.is_some() {
            self.finish_reply();
            self.notice("Reply cancelled.");
        }
    }

    fn finish_reply(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.task.abort();
            if !pending.reply.content.is_empty() {
                self.messages.push(pending.reply);
                self.save_new_messages();
            }
        }
    }

    fn save_new_messages(&mut self) {
        let unsaved: Vec<Message> = self
            .messages
            .iter()
            .skip(self.saved_count)
            .filter(|message| message.is_chat())
            .cloned()
            .collect();
        self.saved_count = self.messages.len();
        if unsaved.is_empty() {
            return;
        }

        let saved = match self.current_conversation_id {
            Some(id) => self.history.append_messages(id, &unsaved).map(|()| id),
            None => {
                let title = unsaved
                    .iter()
                    .find(|message| message.role == "user")
                    .map_or("", |message| message.content.as_str());
                self.history
                    .save_conversation(title, &self.current_model, &unsaved)
            }
        };
        match saved {
            Ok(id) => self.current_conversation_id = Some(id),
            Err(error) => self.notice(format!("Could not save the conversation: {error:#}")),
        }
    }

    fn reply_in_progress(&mut self) -> bool {
        if self.pending.is_none() {
            return false;
        }
        self.notice("A reply is still streaming. Wait for it to finish or press Ctrl+C to cancel it.");
        true
    }

    fn notice(&mut self, content: impl Into<String>) {
        self.messages.push(Message::new("system", content));
        self.scroll_offset = u16::MAX;
    }

    fn execute_command(&mut self) {
        if self.search_mode && self.input.starts_with('/') {
            self.search_query = self.input[1..].to_string();
            self.perform_search();
            self.search_mode = false;
            return;
        }

        let input = self.input.clone();
        let command = input.trim_start_matches(':').trim();
        let (name, argument) = command
            .split_once(' ')
            .map_or((command, ""), |(name, argument)| (name, argument.trim()));

        match name {
            "q" | "quit" => self.should_quit = true,
            "m" if !argument.is_empty() => self.current_model = argument.to_string(),
            "c" | "clear" => self.clear_conversation(),
            "tree" => self.show_tree = !self.show_tree,
            "fork" if !argument.is_empty() => self.fork_conversation(argument),
            "load" => self.load_conversation(argument),
            "list" => self.list_conversations(),
            "search" if !argument.is_empty() => self.search_history(argument),
            "config" => self.open_config(),
            "copy" => self.copy_last_reply(),
            "copy-code" => self.copy_code_blocks(),
            "export-code" if !argument.is_empty() => self.export_code_blocks(argument),
            "reload" => self.reload_config(),
            "scrollspeed" => self.set_scroll_speed(argument),
            _ => self.notice(format!("Unknown command: {command}")),
        }
    }

    fn clear_conversation(&mut self) {
        if self.reply_in_progress() {
            return;
        }
        self.messages.clear();
        self.saved_count = 0;
        self.current_conversation_id = None;
    }

    fn fork_conversation(&mut self, model: &str) {
        if self.reply_in_progress() {
            return;
        }
        let Some(parent_id) = self.current_conversation_id else {
            self.notice("No active conversation to fork");
            return;
        };
        let messages: Vec<Message> = self
            .messages
            .iter()
            .filter(|message| message.is_chat())
            .cloned()
            .collect();
        match self.history.fork_conversation(parent_id, model, &messages) {
            Ok(id) => {
                self.current_conversation_id = Some(id);
                self.saved_count = self.messages.len();
                self.current_model = model.to_string();
                self.notice(format!(
                    "Forked conversation to ID: {id} with model: {model}"
                ));
            }
            Err(error) => self.notice(format!("Error forking conversation: {error:#}")),
        }
    }

    fn load_conversation(&mut self, argument: &str) {
        if self.reply_in_progress() {
            return;
        }
        let Ok(id) = argument.parse::<i64>() else {
            self.notice("Usage: :load <id>");
            return;
        };
        match self.history.load_conversation(id) {
            Ok((conversation, messages)) => {
                self.messages = messages;
                self.saved_count = self.messages.len();
                self.current_model = conversation.model;
                self.current_conversation_id = Some(conversation.id);
                self.notice(format!("Loaded conversation {id}"));
            }
            Err(error) => self.notice(format!("Error loading conversation: {error:#}")),
        }
    }

    fn list_conversations(&mut self) {
        match self.history.list_recent_conversations(10) {
            Ok(conversations) => {
                self.notice(format_previews("Recent conversations:", &conversations));
            }
            Err(error) => self.notice(format!("Error listing conversations: {error:#}")),
        }
    }

    fn search_history(&mut self, query: &str) {
        match self.history.search_conversations(query) {
            Ok(conversations) => self.notice(format_previews(
                &format!("Search results for '{query}':"),
                &conversations,
            )),
            Err(error) => self.notice(format!("Error searching conversations: {error:#}")),
        }
    }

    fn open_config(&mut self) {
        match config::get_config_path() {
            Ok(path) => {
                if let Err(error) = std::process::Command::new("xdg-open").arg(&path).spawn() {
                    self.notice(format!("Error opening config: {error}"));
                }
            }
            Err(error) => self.notice(format!("Error opening config: {error:#}")),
        }
    }

    fn last_reply(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|message| message.role == "assistant")
    }

    fn copy_last_reply(&mut self) {
        let Some(reply) = self.last_reply() else {
            self.notice("No reply to copy.");
            return;
        };
        match clipboard::copy_to_clipboard(&reply.content) {
            Ok(()) => self.notice("Last reply copied to clipboard."),
            Err(error) => self.notice(format!("Error copying to clipboard: {error}")),
        }
    }

    fn copy_code_blocks(&mut self) {
        let Some(reply) = self.last_reply() else {
            self.notice("No reply to copy code from.");
            return;
        };
        let blocks = clipboard::extract_code_blocks(&reply.content);
        if blocks.is_empty() {
            self.notice("No code blocks found in the last reply.");
            return;
        }
        match clipboard::copy_to_clipboard(&blocks.join("\n\n")) {
            Ok(()) => self.notice(format!("{} code block(s) copied to clipboard.", blocks.len())),
            Err(error) => self.notice(format!("Error copying to clipboard: {error}")),
        }
    }

    fn export_code_blocks(&mut self, filepath: &str) {
        let Some(reply) = self.last_reply() else {
            self.notice("No reply to export code from.");
            return;
        };
        match clipboard::export_code_blocks(&reply.content, filepath) {
            Ok(()) => self.notice(format!("Code blocks exported to {filepath}")),
            Err(error) => self.notice(format!("Error exporting code blocks: {error}")),
        }
    }

    fn reload_config(&mut self) {
        match Config::load() {
            Ok(config) => {
                self.client = config.api_key.map(OpenRouterClient::new);
                self.notice("Configuration reloaded successfully.");
            }
            Err(error) => self.notice(format!("Error reloading config: {error:#}")),
        }
    }

    fn set_scroll_speed(&mut self, argument: &str) {
        match argument.parse::<u16>() {
            Ok(speed) => {
                self.scroll_speed = speed.clamp(1, 20);
                self.notice(format!("Scroll speed set to {}", self.scroll_speed));
            }
            Err(_) => self.notice("Invalid scroll speed. Please use a number between 1 and 20."),
        }
    }

    fn scroll_up(&mut self, lines: u16) {
        self.scroll_offset = self
            .scroll_offset
            .min(self.max_scroll.get())
            .saturating_sub(lines);
    }

    fn scroll_down(&mut self, lines: u16) {
        let offset = self.scroll_offset.saturating_add(lines);
        self.scroll_offset = if offset >= self.max_scroll.get() {
            u16::MAX
        } else {
            offset
        };
    }

    fn jump_to_message(&mut self, message_index: usize) {
        let lines_per_message = 5;
        self.scroll_offset = (message_index * lines_per_message) as u16;
    }

    fn perform_search(&mut self) {
        self.search_results.clear();
        if self.search_query.is_empty() {
            return;
        }

        let query = self.search_query.to_lowercase();
        for (i, message) in self.messages.iter().enumerate() {
            if message.content.to_lowercase().contains(&query) {
                self.search_results.push(i);
            }
        }

        if !self.search_results.is_empty() {
            self.current_search_index = 0;
            self.jump_to_message(self.search_results[0]);
        }
    }

    fn insert_char(&mut self, c: char) {
        let index = self.cursor_byte_index();
        self.input.insert(index, c);
    }

    fn line_count(&self) -> usize {
        self.input.split('\n').count()
    }

    fn line_length(&self, line: usize) -> usize {
        self.input
            .split('\n')
            .nth(line)
            .map_or(0, |text| text.chars().count())
    }

    fn cursor_byte_index(&self) -> usize {
        let mut index = 0;
        for (number, line) in self.input.split('\n').enumerate() {
            if number == self.input_cursor_line {
                return index
                    + line
                        .char_indices()
                        .nth(self.input_cursor_position)
                        .map_or(line.len(), |(offset, _)| offset);
            }
            index += line.len() + 1;
        }
        self.input.len()
    }

    fn move_cursor_to_end(&mut self) {
        self.input_cursor_line = self.line_count() - 1;
        self.input_cursor_position = self.line_length(self.input_cursor_line);
    }

    fn navigate_history_backward(&mut self) {
        if self.input_history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                self.history_index = Some(self.input_history.len() - 1);
                self.input = self.input_history[self.input_history.len() - 1].clone();
            }
            Some(index) if index > 0 => {
                self.history_index = Some(index - 1);
                self.input = self.input_history[index - 1].clone();
            }
            _ => return,
        }

        self.move_cursor_to_end();
    }

    fn navigate_history_forward(&mut self) {
        match self.history_index {
            Some(index) if index + 1 < self.input_history.len() => {
                self.history_index = Some(index + 1);
                self.input = self.input_history[index + 1].clone();
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
            }
            None => return,
        }

        self.move_cursor_to_end();
    }
}

fn format_previews(heading: &str, conversations: &[ConversationPreview]) -> String {
    let mut content = format!("{heading}\n");
    for conversation in conversations {
        content.push_str(&format!(
            "ID: {} | {} | {} messages | {}\n",
            conversation.id,
            conversation.timestamp.format("%Y-%m-%d %H:%M"),
            conversation.message_count,
            conversation.title.lines().next().unwrap_or_default()
        ));
    }
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result};

    fn test_app() -> Result<App> {
        Ok(App::new(Config::default(), History::open_in_memory()?))
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_input(Event::Key(KeyEvent::new(code, modifiers)));
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    fn run_command(app: &mut App, command: &str) {
        press(app, KeyCode::Char(':'), KeyModifiers::NONE);
        type_text(app, command);
        press(app, KeyCode::Enter, KeyModifiers::NONE);
    }

    fn roles_and_contents(messages: &[Message]) -> Vec<(&str, &str)> {
        messages
            .iter()
            .map(|message| (message.role.as_str(), message.content.as_str()))
            .collect()
    }

    fn start_reply(app: &mut App, task: AbortHandle) -> mpsc::UnboundedSender<StreamEvent> {
        let (sender, events) = mpsc::unbounded_channel();
        app.pending = Some(PendingReply {
            reply: Message::new("assistant", ""),
            events,
            task,
        });
        sender
    }

    async fn exchange(app: &mut App, question: &str, answer: &[&str]) -> Result<()> {
        app.messages.push(Message::new("user", question));
        let sender = start_reply(app, tokio::spawn(async {}).abort_handle());
        for piece in answer {
            sender.send(StreamEvent::Delta(piece.to_string()))?;
        }
        sender.send(StreamEvent::Finished(Ok(())))?;
        let event = app.next_stream_event().await;
        app.handle_stream_event(event);
        Ok(())
    }

    #[tokio::test]
    async fn streamed_replies_are_assembled_and_every_exchange_is_saved() -> Result<()> {
        let mut app = test_app()?;

        exchange(&mut app, "first question", &["first ", "answer"]).await?;
        assert!(app.pending_reply().is_none());
        let id = app
            .current_conversation_id
            .context("the first reply should start a saved conversation")?;

        exchange(&mut app, "second question", &["second answer"]).await?;
        assert_eq!(app.current_conversation_id, Some(id));

        let (conversation, saved) = app.history.load_conversation(id)?;
        assert_eq!(conversation.title, "first question");
        assert_eq!(
            roles_and_contents(&saved),
            vec![
                ("user", "first question"),
                ("assistant", "first answer"),
                ("user", "second question"),
                ("assistant", "second answer"),
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn fork_sends_later_replies_to_a_child_on_the_new_model() -> Result<()> {
        let mut app = test_app()?;
        exchange(&mut app, "Explain ownership", &["Every value has a single owner."]).await?;
        let parent = app
            .current_conversation_id
            .context("the first reply should start a saved conversation")?;

        run_command(&mut app, "fork openai/gpt-4o");
        let fork = app
            .current_conversation_id
            .context("the fork should become the current conversation")?;
        assert_ne!(fork, parent);
        assert_eq!(app.current_model, "openai/gpt-4o");

        exchange(&mut app, "Now explain borrowing", &["A reference borrows a value."]).await?;

        let (forked, fork_messages) = app.history.load_conversation(fork)?;
        assert_eq!(forked.parent_id, Some(parent));
        assert_eq!(forked.model, "openai/gpt-4o");
        assert_eq!(
            roles_and_contents(&fork_messages),
            vec![
                ("user", "Explain ownership"),
                ("assistant", "Every value has a single owner."),
                ("user", "Now explain borrowing"),
                ("assistant", "A reference borrows a value."),
            ]
        );

        let (_, parent_messages) = app.history.load_conversation(parent)?;
        assert_eq!(parent_messages.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn ctrl_c_cancels_a_streaming_reply_and_keeps_what_arrived() -> Result<()> {
        let mut app = test_app()?;
        app.messages.push(Message::new("user", "Write a long story"));
        let task = tokio::spawn(std::future::pending::<()>());
        let sender = start_reply(&mut app, task.abort_handle());

        sender.send(StreamEvent::Delta("Once upon".to_string()))?;
        let event = app.next_stream_event().await;
        app.handle_stream_event(event);
        assert_eq!(
            app.pending_reply().map(|reply| reply.content.as_str()),
            Some("Once upon")
        );

        run_command(&mut app, "c");
        assert_eq!(app.messages[0].content, "Write a long story");
        assert!(app.pending_reply().is_some());

        press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.pending_reply().is_none());
        assert!(task
            .await
            .expect_err("the request task should be aborted")
            .is_cancelled());
        assert!(sender
            .send(StreamEvent::Delta(" a time".to_string()))
            .is_err());

        let chat: Vec<(String, String)> = app
            .chat_messages()
            .into_iter()
            .map(|message| (message.role, message.content))
            .collect();
        assert_eq!(
            chat,
            vec![
                (String::from("user"), String::from("Write a long story")),
                (String::from("assistant"), String::from("Once upon")),
            ]
        );
        assert_eq!(
            app.messages.last().map(|message| message.content.as_str()),
            Some("Reply cancelled.")
        );
        Ok(())
    }

    #[test]
    fn notices_are_not_sent_to_the_model() -> Result<()> {
        let mut app = test_app()?;
        app.messages.push(Message::new("user", "hello"));
        run_command(&mut app, "list");
        app.messages.push(Message::new("assistant", "hi"));

        assert_eq!(app.messages.len(), 3);
        let roles: Vec<String> = app
            .chat_messages()
            .into_iter()
            .map(|message| message.role)
            .collect();
        assert_eq!(roles, vec!["user", "assistant"]);
        Ok(())
    }

    #[test]
    fn insert_mode_edits_accented_text_by_character() -> Result<()> {
        let mut app = test_app()?;
        press(&mut app, KeyCode::Char('i'), KeyModifiers::NONE);

        type_text(&mut app, "éa");
        assert_eq!(app.input, "éa");

        press(&mut app, KeyCode::Left, KeyModifiers::NONE);
        type_text(&mut app, "ç");
        assert_eq!(app.input, "éça");

        press(&mut app, KeyCode::End, KeyModifiers::NONE);
        press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(app.input, "é");

        press(&mut app, KeyCode::Enter, KeyModifiers::ALT);
        type_text(&mut app, "ü");
        assert_eq!(app.input, "é\nü");
        assert_eq!((app.input_cursor_line, app.input_cursor_position), (1, 1));

        press(&mut app, KeyCode::Home, KeyModifiers::NONE);
        press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(app.input, "éü");
        assert_eq!((app.input_cursor_line, app.input_cursor_position), (0, 1));
        Ok(())
    }

    #[test]
    fn scrolling_up_from_the_bottom_moves_at_once() -> Result<()> {
        let mut app = test_app()?;
        app.max_scroll.set(100);
        app.scroll_offset = u16::MAX;

        press(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.scroll_offset, 97);

        press(&mut app, KeyCode::Char('G'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('u'), KeyModifiers::NONE);
        assert_eq!(app.scroll_offset, 90);

        press(&mut app, KeyCode::Char('f'), KeyModifiers::NONE);
        assert_eq!(app.scroll_offset, u16::MAX);
        Ok(())
    }
}

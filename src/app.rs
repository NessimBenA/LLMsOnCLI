use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use std::time::{Duration, Instant};

use crate::api::{ChatMessage as ApiMessage, LLMClient, OpenRouterClient};
use crate::commands::clipboard;
use crate::config::Config;

#[derive(Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Local>,
}

pub struct App {
    pub messages: Vec<Message>,
    pub input: String,
    pub input_cursor_position: usize,
    pub input_cursor_line: usize,
    pub input_mode: InputMode,
    pub current_model: String,
    pub should_quit: bool,
    pub last_tick: Instant,
    pub tick_rate: Duration,
    pub current_conversation_id: Option<i64>,
    pub show_tree: bool,
    pub scroll_offset: u16,
    pub scroll_speed: u16,
    pub search_query: String,
    pub search_mode: bool,
    pub search_results: Vec<usize>,
    pub current_search_index: usize,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    config: Config,
    client: Option<OpenRouterClient>,
}

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
    Command,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        let client = config
            .api_key
            .clone()
            .map(|key| OpenRouterClient::new(key));

        Ok(Self {
            messages: Vec::new(),
            input: String::new(),
            input_cursor_position: 0,
            input_cursor_line: 0,
            input_mode: InputMode::Normal,
            current_model: config.default_model.clone().unwrap_or_else(|| String::from("anthropic/claude-3.5-sonnet")),
            should_quit: false,
            last_tick: Instant::now(),
            tick_rate: Duration::from_millis(250),
            current_conversation_id: None,
            show_tree: false,
            scroll_offset: 0,
            scroll_speed: 3,
            search_query: String::new(),
            search_mode: false,
            search_results: Vec::new(),
            current_search_index: 0,
            input_history: Vec::new(),
            history_index: None,
            config,
            client,
        })
    }

    pub async fn handle_events(&mut self) -> Result<Option<Event>> {
        if event::poll(self.tick_rate)? {
            return Ok(Some(event::read()?));
        }
        Ok(None)
    }

    pub async fn handle_input(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) => self.handle_key_event(key).await?,
            _ => {}
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        match self.input_mode {
            InputMode::Normal => self.handle_normal_mode(key),
            InputMode::Editing => self.handle_editing_mode(key).await?,
            InputMode::Command => self.handle_command_mode(key).await?,
        }
        Ok(())
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
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_add(self.scroll_speed);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed);
            }
            KeyCode::Char('J') => {
                // Fast scroll down
                self.scroll_offset = self.scroll_offset.saturating_add(self.scroll_speed * 5);
            }
            KeyCode::Char('K') => {
                // Fast scroll up
                self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_speed * 5);
            }
            KeyCode::PageDown | KeyCode::Char('f') => {
                // Page down
                self.scroll_offset = self.scroll_offset.saturating_add(20);
            }
            KeyCode::PageUp | KeyCode::Char('b') => {
                // Page up
                self.scroll_offset = self.scroll_offset.saturating_sub(20);
            }
            KeyCode::Char('d') => {
                // Half page down
                self.scroll_offset = self.scroll_offset.saturating_add(10);
            }
            KeyCode::Char('u') => {
                // Half page up
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            KeyCode::Char('G') => {
                // Scroll to bottom (we'll calculate the actual max in the UI)
                self.scroll_offset = u16::MAX;
            }
            KeyCode::Char('g') => {
                // Scroll to top
                self.scroll_offset = 0;
            }
            KeyCode::Char('/') => {
                // Start search
                self.search_mode = true;
                self.input_mode = InputMode::Command;
                self.input.clear();
                self.input.push('/');
            }
            KeyCode::Char('n') => {
                // Next search result
                if !self.search_results.is_empty() {
                    self.current_search_index = (self.current_search_index + 1) % self.search_results.len();
                    self.jump_to_message(self.search_results[self.current_search_index]);
                }
            }
            KeyCode::Char('N') => {
                // Previous search result
                if !self.search_results.is_empty() {
                    if self.current_search_index == 0 {
                        self.current_search_index = self.search_results.len() - 1;
                    } else {
                        self.current_search_index -= 1;
                    }
                    self.jump_to_message(self.search_results[self.current_search_index]);
                }
            }
            _ => {}
        }
    }

    async fn handle_editing_mode(&mut self, key: KeyEvent) -> Result<()> {
        use crossterm::event::KeyModifiers;
        
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.input_mode = InputMode::Normal;
                self.input_cursor_position = 0;
                self.input_cursor_line = 0;
            }
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if !self.input.trim().is_empty() {
                    self.submit_message().await?;
                    self.input_cursor_position = 0;
                    self.input_cursor_line = 0;
                }
            }
            (KeyCode::Enter, KeyModifiers::CONTROL) | (KeyCode::Enter, KeyModifiers::SHIFT) => {
                // Insert newline
                let pos = self.get_absolute_cursor_position();
                self.input.insert(pos, '\n');
                self.input_cursor_position = 0;
                self.input_cursor_line += 1;
            }
            (KeyCode::Char(c), _) => {
                let pos = self.get_absolute_cursor_position();
                self.input.insert(pos, c);
                self.input_cursor_position += 1;
            }
            (KeyCode::Backspace, _) => {
                let pos = self.get_absolute_cursor_position();
                if pos > 0 {
                    let removed = self.input.remove(pos - 1);
                    if removed == '\n' {
                        // Moving to previous line
                        self.input_cursor_line = self.input_cursor_line.saturating_sub(1);
                        let lines: Vec<&str> = self.input.lines().collect();
                        if self.input_cursor_line < lines.len() {
                            self.input_cursor_position = lines[self.input_cursor_line].len();
                        }
                    } else {
                        self.input_cursor_position = self.input_cursor_position.saturating_sub(1);
                    }
                }
            }
            (KeyCode::Left, _) => {
                if self.input_cursor_position > 0 {
                    self.input_cursor_position -= 1;
                } else if self.input_cursor_line > 0 {
                    // Move to end of previous line
                    self.input_cursor_line -= 1;
                    let lines: Vec<&str> = self.input.lines().collect();
                    if self.input_cursor_line < lines.len() {
                        self.input_cursor_position = lines[self.input_cursor_line].len();
                    }
                }
            }
            (KeyCode::Right, _) => {
                let lines: Vec<&str> = self.input.lines().collect();
                if self.input_cursor_line < lines.len() {
                    let current_line_len = lines[self.input_cursor_line].len();
                    if self.input_cursor_position < current_line_len {
                        self.input_cursor_position += 1;
                    } else if self.input_cursor_line < lines.len() - 1 {
                        // Move to start of next line
                        self.input_cursor_line += 1;
                        self.input_cursor_position = 0;
                    }
                }
            }
            (KeyCode::Up, KeyModifiers::NONE) => {
                let lines: Vec<&str> = self.input.lines().collect();
                if lines.len() <= 1 {
                    // Single line - navigate history
                    self.navigate_history_backward();
                } else if self.input_cursor_line > 0 {
                    // Multi-line - move cursor up
                    self.input_cursor_line -= 1;
                    if self.input_cursor_line < lines.len() {
                        let line_len = lines[self.input_cursor_line].len();
                        self.input_cursor_position = self.input_cursor_position.min(line_len);
                    }
                }
            }
            (KeyCode::Down, KeyModifiers::NONE) => {
                let lines: Vec<&str> = self.input.lines().collect();
                if lines.len() <= 1 {
                    // Single line - navigate history
                    self.navigate_history_forward();
                } else if self.input_cursor_line < lines.len() - 1 {
                    // Multi-line - move cursor down
                    self.input_cursor_line += 1;
                    let line_len = lines[self.input_cursor_line].len();
                    self.input_cursor_position = self.input_cursor_position.min(line_len);
                }
            }
            (KeyCode::Up, KeyModifiers::ALT) => {
                // Force history navigation even in multi-line
                self.navigate_history_backward();
            }
            (KeyCode::Down, KeyModifiers::ALT) => {
                // Force history navigation even in multi-line
                self.navigate_history_forward();
            }
            (KeyCode::Home, _) => {
                self.input_cursor_position = 0;
            }
            (KeyCode::End, _) => {
                let lines: Vec<&str> = self.input.lines().collect();
                if self.input_cursor_line < lines.len() {
                    self.input_cursor_position = lines[self.input_cursor_line].len();
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_command_mode(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
            KeyCode::Enter => {
                self.execute_command().await?;
                self.input_mode = InputMode::Normal;
                self.input.clear();
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            KeyCode::Backspace => {
                if self.input.len() > 1 {
                    self.input.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn submit_message(&mut self) -> Result<()> {
        // Save to history
        if !self.input.trim().is_empty() {
            self.input_history.push(self.input.clone());
            self.history_index = None;
        }
        
        // Reset scroll position when submitting a new message
        self.scroll_offset = 0;
        let user_message = Message {
            role: String::from("user"),
            content: self.input.clone(),
            timestamp: chrono::Local::now(),
        };
        self.messages.push(user_message);
        self.input.clear();

        if let Some(client) = &self.client {
            let api_messages: Vec<ApiMessage> = self
                .messages
                .iter()
                .map(|m| ApiMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect();

            match client.chat(api_messages, &self.current_model).await {
                Ok(response) => {
                    if let Some(choice) = response.choices.first() {
                        let assistant_message = Message {
                            role: choice.message.role.clone(),
                            content: choice.message.content.clone(),
                            timestamp: chrono::Local::now(),
                        };
                        self.messages.push(assistant_message.clone());

                        // Auto-save conversation after each response
                        if self.current_conversation_id.is_none() {
                            if let Ok(id) = crate::history::save_conversation(
                                &self.messages.first().map(|m| m.content.clone()).unwrap_or_default(),
                                &self.current_model,
                                &self.messages,
                                &[],
                            ) {
                                self.current_conversation_id = Some(id);
                            }
                        }
                    }
                }
                Err(e) => {
                    let error_message = Message {
                        role: String::from("system"),
                        content: format!("Error: {}", e),
                        timestamp: chrono::Local::now(),
                    };
                    self.messages.push(error_message);
                }
            }
        } else {
            let error_message = Message {
                role: String::from("system"),
                content: String::from("Error: API key not configured. Use :config to set it up."),
                timestamp: chrono::Local::now(),
            };
            self.messages.push(error_message);
        }
        Ok(())
    }

    async fn execute_command(&mut self) -> Result<()> {
        if self.search_mode && self.input.starts_with('/') {
            // Handle search
            self.search_query = self.input[1..].to_string();
            self.perform_search();
            self.search_mode = false;
            return Ok(());
        }
        
        let cmd = self.input.trim_start_matches(':');
        match cmd {
            "q" | "quit" => self.should_quit = true,
            cmd if cmd.starts_with("m ") => {
                let model = cmd.trim_start_matches("m ").trim();
                self.current_model = model.to_string();
            }
            "c" | "clear" => {
                self.messages.clear();
                self.current_conversation_id = None;
            }
            "tree" => {
                self.show_tree = !self.show_tree;
                if self.current_conversation_id.is_some() {
                    if let Ok(tree) = crate::history::get_conversation_tree(self.current_conversation_id.unwrap()) {
                        // Find the root conversation (the earliest ancestor)
                        if let Some(root) = tree.iter()
                            .filter(|c| c.parent_id.is_none())
                            .min_by_key(|c| c.timestamp) {
                            
                            // Create a map of parent_id to children for efficient lookup
                            let mut children: std::collections::HashMap<Option<i64>, Vec<&crate::history::Conversation>> = 
                                std::collections::HashMap::new();
                            for conv in &tree {
                                children.entry(conv.parent_id)
                                    .or_default()
                                    .push(conv);
                                // Sort children by timestamp
                                if let Some(list) = children.get_mut(&conv.parent_id) {
                                    list.sort_by_key(|c| c.timestamp);
                                }
                            }

                            let mut content = String::from("Conversation tree:\n\n");
                            let current_id = self.current_conversation_id.unwrap();

                            fn print_tree(
                                conv: &crate::history::Conversation,
                                children: &std::collections::HashMap<Option<i64>, Vec<&crate::history::Conversation>>,
                                prefix: &str,
                                is_last: bool,
                                current_id: i64,
                                content: &mut String
                            ) {
                                let branch = if is_last { "└──" } else { "├──" };
                                let model_info = if conv.model != "anthropic/claude-3.5-sonnet" {
                                    format!(" [{}]", conv.model)
                                } else {
                                    String::new()
                                };
                                
                                let title = if conv.id == current_id {
                                    format!("* {}", conv.title)
                                } else {
                                    conv.title.clone()
                                };
                                
                                content.push_str(&format!("{}{} {}{}\n", 
                                    prefix,
                                    branch,
                                    title,
                                    model_info
                                ));

                                if let Some(child_convs) = children.get(&Some(conv.id)) {
                                    let new_prefix = format!("{}{}",
                                        prefix,
                                        if is_last { "    " } else { "│   " }
                                    );
                                    
                                    for (i, child) in child_convs.iter().enumerate() {
                                        print_tree(
                                            child,
                                            children,
                                            &new_prefix,
                                            i == child_convs.len() - 1,
                                            current_id,
                                            content
                                        );
                                    }
                                }
                            }

                            print_tree(root, &children, "", true, current_id, &mut content);
                            
                            let message = Message {
                                role: String::from("system"),
                                content,
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(message);
                        }
                    }
                }
            },
            cmd if cmd.starts_with("fork ") => {
                if let Some(current_id) = self.current_conversation_id {
                    let model = cmd.trim_start_matches("fork ").trim();
                    match crate::history::fork_conversation(current_id, model, &self.messages, &[]) {
                        Ok(new_id) => {
                            self.current_conversation_id = Some(new_id);
                            self.current_model = model.to_string();
                            let success_message = Message {
                                role: String::from("system"),
                                content: format!("Forked conversation to ID: {} with model: {}", new_id, model),
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(success_message);
                        }
                        Err(e) => {
                            let error_message = Message {
                                role: String::from("system"),
                                content: format!("Error forking conversation: {}", e),
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(error_message);
                        }
                    }
                } else {
                    let error_message = Message {
                        role: String::from("system"),
                        content: String::from("No active conversation to fork"),
                        timestamp: chrono::Local::now(),
                    };
                    self.messages.push(error_message);
                }
            },
            cmd if cmd.starts_with("load ") => {
                if let Ok(id) = cmd.trim_start_matches("load ").trim().parse::<i64>() {
                    match crate::history::load_conversation(id) {
                        Ok((conv, messages)) => {
                            self.messages = messages;
                            self.current_model = conv.model;
                            self.current_conversation_id = Some(conv.id);
                            let success_message = Message {
                                role: String::from("system"),
                                content: format!("Loaded conversation {}", id),
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(success_message);
                        }
                        Err(e) => {
                            let error_message = Message {
                                role: String::from("system"),
                                content: format!("Error loading conversation: {}", e),
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(error_message);
                        }
                    }
                }
            }
            "list" => {
                match crate::history::list_recent_conversations(10) {
                    Ok(conversations) => {
                        let mut content = String::from("Recent conversations:\n");
                        for conv in conversations {
                            content.push_str(&format!(
                                "ID: {} | {} | {} messages | {}\n",
                                conv.id,
                                conv.timestamp.format("%Y-%m-%d %H:%M"),
                                conv.message_count,
                                conv.title
                            ));
                        }
                        let message = Message {
                            role: String::from("system"),
                            content,
                            timestamp: chrono::Local::now(),
                        };
                        self.messages.push(message);
                    }
                    Err(e) => {
                        let error_message = Message {
                            role: String::from("system"),
                            content: format!("Error listing conversations: {}", e),
                            timestamp: chrono::Local::now(),
                        };
                        self.messages.push(error_message);
                    }
                }
            }
            cmd if cmd.starts_with("search ") => {
                let query = cmd.trim_start_matches("search ").trim();
                match crate::history::search_conversations(query) {
                    Ok(conversations) => {
                        let mut content = format!("Search results for '{}':\n", query);
                        for conv in conversations {
                            content.push_str(&format!(
                                "ID: {} | {} | {} messages | {}\n",
                                conv.id,
                                conv.timestamp.format("%Y-%m-%d %H:%M"),
                                conv.message_count,
                                conv.title
                            ));
                        }
                        let message = Message {
                            role: String::from("system"),
                            content,
                            timestamp: chrono::Local::now(),
                        };
                        self.messages.push(message);
                    }
                    Err(e) => {
                        let error_message = Message {
                            role: String::from("system"),
                            content: format!("Error searching conversations: {}", e),
                            timestamp: chrono::Local::now(),
                        };
                        self.messages.push(error_message);
                    }
                }
            }
            "config" => {
                // Open config file in default editor
                if let Ok(path) = crate::config::get_config_path() {
                    if let Some(path_str) = path.to_str() {
                        if let Err(e) = std::process::Command::new("xdg-open")
                            .arg(path_str)
                            .spawn() {
                                let error_message = Message {
                                    role: String::from("system"),
                                    content: format!("Error opening config: {}", e),
                                    timestamp: chrono::Local::now(),
                                };
                                self.messages.push(error_message);
                            }
                    }
                }
            }
            "copy" => {
                if let Some(last_message) = self.messages.last() {
                    match clipboard::copy_to_clipboard(&last_message.content) {
                        Ok(_) => {
                            let success_message = Message {
                                role: String::from("system"),
                                content: String::from("Last message copied to clipboard."),
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(success_message);
                        }
                        Err(e) => {
                            let error_message = Message {
                                role: String::from("system"),
                                content: format!("Error copying to clipboard: {}", e),
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(error_message);
                        }
                    }
                } else {
                    let error_message = Message {
                        role: String::from("system"),
                        content: String::from("No message to copy."),
                        timestamp: chrono::Local::now(),
                    };
                    self.messages.push(error_message);
                }
            }
            "copy-code" => {
                if let Some(last_message) = self.messages.last() {
                    let blocks = clipboard::extract_code_blocks(&last_message.content);
                    if blocks.is_empty() {
                        let error_message = Message {
                            role: String::from("system"),
                            content: String::from("No code blocks found in the last message."),
                            timestamp: chrono::Local::now(),
                        };
                        self.messages.push(error_message);
                    } else {
                        match clipboard::copy_to_clipboard(&blocks.join("\n\n")) {
                            Ok(_) => {
                                let success_message = Message {
                                    role: String::from("system"),
                                    content: format!("{} code block(s) copied to clipboard.", blocks.len()),
                                    timestamp: chrono::Local::now(),
                                };
                                self.messages.push(success_message);
                            }
                            Err(e) => {
                                let error_message = Message {
                                    role: String::from("system"),
                                    content: format!("Error copying to clipboard: {}", e),
                                    timestamp: chrono::Local::now(),
                                };
                                self.messages.push(error_message);
                            }
                        }
                    }
                } else {
                    let error_message = Message {
                        role: String::from("system"),
                        content: String::from("No message to copy code from."),
                        timestamp: chrono::Local::now(),
                    };
                    self.messages.push(error_message);
                }
            }
            cmd if cmd.starts_with("export-code ") => {
                if let Some(last_message) = self.messages.last() {
                    let filepath = cmd.trim_start_matches("export-code ").trim();
                    match clipboard::export_code_blocks(&last_message.content, filepath) {
                        Ok(_) => {
                            let success_message = Message {
                                role: String::from("system"),
                                content: format!("Code blocks exported to {}", filepath),
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(success_message);
                        }
                        Err(e) => {
                            let error_message = Message {
                                role: String::from("system"),
                                content: format!("Error exporting code blocks: {}", e),
                                timestamp: chrono::Local::now(),
                            };
                            self.messages.push(error_message);
                        }
                    }
                } else {
                    let error_message = Message {
                        role: String::from("system"),
                        content: String::from("No message to export code from."),
                        timestamp: chrono::Local::now(),
                    };
                    self.messages.push(error_message);
                }
            }
            "reload" => {
                match Config::load() {
                    Ok(config) => {
                        self.client = config.api_key.clone().map(|key| OpenRouterClient::new(key));
                        self.config = config;
                        let success_message = Message {
                            role: String::from("system"),
                            content: String::from("Configuration reloaded successfully."),
                            timestamp: chrono::Local::now(),
                        };
                        self.messages.push(success_message);
                    }
                    Err(e) => {
                        let error_message = Message {
                            role: String::from("system"),
                            content: format!("Error reloading config: {}", e),
                            timestamp: chrono::Local::now(),
                        };
                        self.messages.push(error_message);
                    }
                }
            }
            cmd if cmd.starts_with("scrollspeed ") => {
                if let Ok(speed) = cmd.trim_start_matches("scrollspeed ").trim().parse::<u16>() {
                    self.scroll_speed = speed.max(1).min(20);
                    let message = Message {
                        role: String::from("system"),
                        content: format!("Scroll speed set to {}", self.scroll_speed),
                        timestamp: chrono::Local::now(),
                    };
                    self.messages.push(message);
                } else {
                    let error_message = Message {
                        role: String::from("system"),
                        content: String::from("Invalid scroll speed. Please use a number between 1 and 20."),
                        timestamp: chrono::Local::now(),
                    };
                    self.messages.push(error_message);
                }
            }
            _ => {
                let error_message = Message {
                    role: String::from("system"),
                    content: format!("Unknown command: {}", cmd),
                    timestamp: chrono::Local::now(),
                };
                self.messages.push(error_message);
            }
        }
        Ok(())
    }
    
    fn jump_to_message(&mut self, message_index: usize) {
        // Calculate the scroll offset to show the message
        // This is a simplified version - in a real implementation,
        // we'd need to calculate based on actual rendered line counts
        let lines_per_message = 5; // Estimate
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
    
    fn get_absolute_cursor_position(&self) -> usize {
        let lines: Vec<&str> = self.input.lines().collect();
        let mut position = 0;
        
        for i in 0..self.input_cursor_line.min(lines.len()) {
            position += lines[i].len() + 1; // +1 for newline
        }
        
        position + self.input_cursor_position
    }
    
    fn navigate_history_backward(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        
        match self.history_index {
            None => {
                // Save current input before navigating
                self.history_index = Some(self.input_history.len() - 1);
                self.input = self.input_history[self.input_history.len() - 1].clone();
            }
            Some(index) if index > 0 => {
                self.history_index = Some(index - 1);
                self.input = self.input_history[index - 1].clone();
            }
            _ => return,
        }
        
        // Reset cursor to end of input
        let lines: Vec<&str> = self.input.lines().collect();
        self.input_cursor_line = lines.len().saturating_sub(1);
        self.input_cursor_position = lines.last().map(|l| l.len()).unwrap_or(0);
    }
    
    fn navigate_history_forward(&mut self) {
        match self.history_index {
            Some(index) if index < self.input_history.len() - 1 => {
                self.history_index = Some(index + 1);
                self.input = self.input_history[index + 1].clone();
            }
            Some(index) if index == self.input_history.len() - 1 => {
                // Return to original input
                self.history_index = None;
                self.input.clear();
            }
            _ => return,
        }
        
        // Reset cursor to end of input
        let lines: Vec<&str> = self.input.lines().collect();
        self.input_cursor_line = lines.len().saturating_sub(1);
        self.input_cursor_position = lines.last().map(|l| l.len()).unwrap_or(0);
    }
}

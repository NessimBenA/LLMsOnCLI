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
    pub input_mode: InputMode,
    pub current_model: String,
    pub should_quit: bool,
    pub last_tick: Instant,
    pub tick_rate: Duration,
    pub current_conversation_id: Option<i64>,
    pub show_tree: bool,
    pub scroll_offset: u16,
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
            input_mode: InputMode::Normal,
            current_model: config.default_model.clone().unwrap_or_else(|| String::from("anthropic/claude-3.5-sonnet")),
            should_quit: false,
            last_tick: Instant::now(),
            tick_rate: Duration::from_millis(250),
            current_conversation_id: None,
            show_tree: false,
            scroll_offset: 0,
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
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            KeyCode::Char('G') => {
                // Scroll to bottom (we'll calculate the actual max in the UI)
                self.scroll_offset = u16::MAX;
            }
            KeyCode::Char('g') => {
                // Scroll to top
                self.scroll_offset = 0;
            }
            _ => {}
        }
    }

    async fn handle_editing_mode(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.input_mode = InputMode::Normal,
            KeyCode::Enter => {
                if !self.input.trim().is_empty() {
                    self.submit_message().await?;
                }
            }
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            KeyCode::Backspace => {
                self.input.pop();
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
}

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::rc::Rc;

use crate::app::{App, InputMode};

mod chat;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = if app.show_tree {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70),
                Constraint::Percentage(30),
            ])
            .split(f.size())
    } else {
        Rc::from(vec![f.size()].as_slice())
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(chunks[0]);

    draw_messages(f, app, main_chunks[0]);
    draw_status_line(f, app, main_chunks[1]);
    draw_input(f, app, main_chunks[2]);

    if app.show_tree {
        draw_tree(f, app, chunks[1]);
    }
}

fn draw_tree(f: &mut Frame, app: &App, area: Rect) {
    let tree_block = Block::default()
        .borders(Borders::ALL)
        .title("Conversation Tree");

    let mut content = String::new();
    if let Some(id) = app.current_conversation_id {
        if let Ok(tree) = crate::history::get_conversation_tree(id) {
            // Create a map of parent_id to children for efficient lookup
            let mut children: std::collections::HashMap<Option<i64>, Vec<&crate::history::Conversation>> = 
                std::collections::HashMap::new();
            for conv in &tree {
                children.entry(conv.parent_id)
                    .or_default()
                    .push(conv);
            }

            // Find the root conversation (the one with no parent or the earliest ancestor)
            if let Some(root) = tree.iter()
                .filter(|c| c.parent_id.is_none())
                .min_by_key(|c| c.timestamp) {
                fn print_tree(
                    conv: &crate::history::Conversation,
                    children: &std::collections::HashMap<Option<i64>, Vec<&crate::history::Conversation>>,
                    prefix: &str,
                    is_last: bool,
                    current_id: i64,
                    content: &mut String
                ) {
                    let branch = if is_last { "└──" } else { "├──" };
                    let model_info = if conv.model != "gpt-4" {
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
                        
                        // Sort children by timestamp
                        let mut sorted_children = child_convs.clone();
                        sorted_children.sort_by_key(|c| c.timestamp);
                        
                        for (i, child) in sorted_children.iter().enumerate() {
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

                print_tree(root, &children, "", true, id, &mut content);
            }
        }
    } else {
        content.push_str("No conversation loaded");
    }

    let tree = Paragraph::new(content)
        .block(tree_block)
        .wrap(Wrap { trim: true });

    f.render_widget(tree, area);
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let messages_block = Block::default()
        .borders(Borders::ALL)
        .title("Messages");

    let messages = chat::format_messages(&app.messages);
    
    // Calculate the total height of the content
    let mut content_height = 0;
    for line in messages.lines.iter() {
        // Account for wrapped lines
        let line_content = line.spans.iter()
            .map(|span| span.content.to_string())
            .collect::<String>();
        let wrapped_height = textwrap::wrap(
            &line_content,
            (area.width as usize).saturating_sub(2) // Account for borders
        ).len();
        content_height += wrapped_height;
    }

    // Calculate the maximum scroll offset
    let max_scroll = content_height.saturating_sub(area.height as usize);
    let scroll = if app.scroll_offset == u16::MAX {
        // If scroll_offset is MAX, we want to scroll to bottom
        max_scroll as u16
    } else {
        // Otherwise use the current scroll_offset, but don't exceed max_scroll
        app.scroll_offset.min(max_scroll as u16)
    };

    let messages_paragraph = Paragraph::new(messages)
        .block(messages_block)
        .wrap(Wrap { trim: true })
        .scroll((scroll, 0));

    f.render_widget(messages_paragraph, area);
}

fn draw_status_line(f: &mut Frame, app: &App, area: Rect) {
    let mode = match app.input_mode {
        InputMode::Normal => "NORMAL",
        InputMode::Editing => "INSERT",
        InputMode::Command => "COMMAND",
    };

    let mut status_parts = vec![
        Span::styled(
            format!(" {} ", mode),
            Style::default().fg(Color::Black).bg(Color::Green),
        ),
        Span::raw(" | "),
        Span::raw(format!("Model: {} ", app.current_model)),
    ];

    if let Some(id) = app.current_conversation_id {
        status_parts.extend_from_slice(&[
            Span::raw(" | "),
            Span::raw(format!("Chat ID: {} ", id)),
        ]);
    }

    let status = Line::from(status_parts);

    let status_bar = Paragraph::new(status);
    f.render_widget(status_bar, area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title("Input");

    let input = Paragraph::new(app.input.as_str())
        .style(match app.input_mode {
            InputMode::Editing | InputMode::Command => Style::default().fg(Color::Yellow),
            InputMode::Normal => Style::default(),
        })
        .block(input_block);

    f.render_widget(input, area);
}

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;
use std::rc::Rc;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, InputMode};
use crate::history::Conversation;

mod chat;
mod syntax;

type Children<'a> = HashMap<Option<i64>, Vec<&'a Conversation>>;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = if app.show_tree {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(f.size())
    } else {
        Rc::from(vec![f.size()].as_slice())
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(5),
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

    let content = match app.current_conversation_id {
        Some(id) => match app.history.conversation_tree(id) {
            Ok(tree) => tree_text(&tree, id),
            Err(error) => format!("Error loading the tree: {error:#}"),
        },
        None => String::from("No conversation loaded"),
    };

    let tree = Paragraph::new(content)
        .block(tree_block)
        .wrap(Wrap { trim: false });

    f.render_widget(tree, area);
}

fn tree_text(tree: &[Conversation], current_id: i64) -> String {
    let mut children: Children = HashMap::new();
    for conversation in tree {
        children
            .entry(conversation.parent_id)
            .or_default()
            .push(conversation);
    }

    let mut text = String::new();
    if let Some(roots) = children.get(&None) {
        for (index, root) in roots.iter().enumerate() {
            push_branch(
                &mut text,
                root,
                &children,
                "",
                index + 1 == roots.len(),
                current_id,
            );
        }
    }
    text
}

fn push_branch(
    text: &mut String,
    conversation: &Conversation,
    children: &Children,
    prefix: &str,
    is_last: bool,
    current_id: i64,
) {
    let branch = if is_last { "└──" } else { "├──" };
    let marker = if conversation.id == current_id { "* " } else { "" };
    let title = conversation.title.lines().next().unwrap_or_default();
    text.push_str(&format!(
        "{prefix}{branch} {marker}{title} [{}]\n",
        conversation.model
    ));

    if let Some(forks) = children.get(&Some(conversation.id)) {
        let fork_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
        for (index, fork) in forks.iter().enumerate() {
            push_branch(
                text,
                fork,
                children,
                &fork_prefix,
                index + 1 == forks.len(),
                current_id,
            );
        }
    }
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let messages_block = Block::default()
        .borders(Borders::ALL)
        .title("Messages");

    let messages = chat::format_messages(app.messages.iter().chain(app.pending_reply()));

    let mut content_height = 0;
    for line in messages.lines.iter() {
        let line_content = line
            .spans
            .iter()
            .map(|span| span.content.to_string())
            .collect::<String>();
        let wrapped_height =
            textwrap::wrap(&line_content, (area.width as usize).saturating_sub(2)).len();
        content_height += wrapped_height;
    }

    let max_scroll =
        u16::try_from(content_height.saturating_sub(area.height as usize)).unwrap_or(u16::MAX);
    app.max_scroll.set(max_scroll);
    let scroll = app.scroll_offset.min(max_scroll);

    let messages_paragraph = Paragraph::new(messages)
        .block(messages_block)
        .wrap(Wrap { trim: false })
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
        status_parts.extend_from_slice(&[Span::raw(" | "), Span::raw(format!("Chat ID: {} ", id))]);
    }

    if let Some(reply) = app.pending_reply() {
        let state = if reply.content.is_empty() {
            "Waiting for the reply"
        } else {
            "Receiving the reply"
        };
        status_parts.extend_from_slice(&[
            Span::raw(" | "),
            Span::styled(
                format!("{state}, Ctrl+C cancels "),
                Style::default().fg(Color::Yellow),
            ),
        ]);
    }

    let status = Line::from(status_parts);

    let status_bar = Paragraph::new(status);
    f.render_widget(status_bar, area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(match app.input_mode {
            InputMode::Editing => "Input (Enter sends, Alt+Enter adds a line)",
            InputMode::Command => "Command",
            InputMode::Normal => "Input",
        });

    let input = Paragraph::new(app.input.as_str())
        .style(match app.input_mode {
            InputMode::Editing | InputMode::Command => Style::default().fg(Color::Yellow),
            InputMode::Normal => Style::default(),
        })
        .block(input_block)
        .wrap(Wrap { trim: false });

    f.render_widget(input, area);

    if matches!(app.input_mode, InputMode::Editing | InputMode::Command) {
        let cursor_x = if app.input.is_empty() {
            area.x + 1
        } else {
            let lines: Vec<&str> = app.input.split('\n').collect();
            let cursor_x_offset = if app.input_cursor_line < lines.len() {
                let line = lines[app.input_cursor_line];
                let chars_before_cursor: String =
                    line.chars().take(app.input_cursor_position).collect();
                chars_before_cursor.width() as u16
            } else {
                0
            };
            area.x + 1 + cursor_x_offset
        };

        let cursor_y = area.y + 1 + app.input_cursor_line as u16;

        if cursor_x < area.x + area.width - 1 && cursor_y < area.y + area.height - 1 {
            f.set_cursor(cursor_x, cursor_y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Message;
    use crate::history::History;
    use anyhow::Result;

    #[test]
    fn tree_text_draws_every_fork_and_marks_the_current_one() -> Result<()> {
        let history = History::open_in_memory()?;
        let root = history.save_conversation(
            "Explain ownership",
            "anthropic/claude-3.5-sonnet",
            &[Message::new("user", "Explain ownership")],
        )?;
        let first = history.fork_conversation(root, "openai/gpt-4o", &[])?;
        history.fork_conversation(root, "openai/gpt-4", &[])?;
        let nested = history.fork_conversation(first, "anthropic/claude-3.5-sonnet", &[])?;

        let expected = "\
└── Explain ownership [anthropic/claude-3.5-sonnet]
    ├── Explain ownership (fork) [openai/gpt-4o]
    │   └── * Explain ownership (fork) (fork) [anthropic/claude-3.5-sonnet]
    └── Explain ownership (fork) [openai/gpt-4]
";
        assert_eq!(tree_text(&history.conversation_tree(nested)?, nested), expected);
        Ok(())
    }
}

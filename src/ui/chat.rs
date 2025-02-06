use ratatui::{
    style::{Color, Style},
    text::{Line, Span, Text},
};

use crate::app::Message;

pub fn format_messages(messages: &[Message]) -> Text<'static> {
    let mut text = Text::default();
    
    for message in messages {
        // Add timestamp
        text.extend(Text::from(Line::from(Span::styled(
            format!("[{}]", message.timestamp.format("%H:%M:%S")),
            Style::default().fg(Color::DarkGray),
        ))));

        // Add role indicator with appropriate color
        let (role_display, style) = match message.role.as_str() {
            "user" => (
                "You",
                Style::default().fg(Color::Blue),
            ),
            "assistant" => (
                "[Assistant]",
                Style::default().fg(Color::Green),
            ),
            _ => (
                "System",
                Style::default().fg(Color::Yellow),
            ),
        };

        text.extend(Text::from(Line::from(Span::styled(
            role_display,
            style,
        ))));

        // Add message content
        let content_lines: Vec<Line> = message
            .content
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::Reset),
                ))
            })
            .collect();

        text.extend(Text::from(content_lines));

        // Add separator between messages
        text.extend(Text::from(vec![
            Line::from(""),
            Line::from(""),
        ]));
    }

    text
}

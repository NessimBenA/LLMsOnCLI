use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

use super::syntax::{extract_code_blocks, highlight_code_block};
use crate::app::Message;

pub fn format_messages<'a>(messages: impl IntoIterator<Item = &'a Message>) -> Text<'static> {
    let mut text = Text::default();

    for message in messages {
        // Add timestamp
        text.extend(Text::from(Line::from(Span::styled(
            format!("[{}]", message.timestamp.format("%H:%M:%S")),
            Style::default().fg(Color::DarkGray),
        ))));

        // Add role indicator with appropriate color
        let (role_display, style) = match message.role.as_str() {
            "user" => ("You", Style::default().fg(Color::Blue)),
            "assistant" => ("[Assistant]", Style::default().fg(Color::Green)),
            _ => ("System", Style::default().fg(Color::Yellow)),
        };

        text.extend(Text::from(Line::from(Span::styled(role_display, style))));

        // Process message content with syntax highlighting
        let content_lines = format_message_content(&message.content);
        text.extend(Text::from(content_lines));

        // Add separator between messages
        text.extend(Text::from(vec![Line::from(""), Line::from("")]));
    }

    text
}

fn format_message_content(content: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let code_blocks = extract_code_blocks(content);
    let content_lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    for (block_idx, (language, code, start_line, end_line)) in code_blocks.iter().enumerate() {
        // Add lines before the code block
        while i < *start_line {
            lines.push(format_text_line(content_lines[i]));
            i += 1;
        }

        // Add code block header
        let lang_display = language.as_deref().unwrap_or("text").to_string();
        lines.push(Line::from(vec![
            Span::styled("```", Style::default().fg(Color::DarkGray)),
            Span::styled(lang_display.clone(), Style::default().fg(Color::Cyan)),
            Span::styled(
                format!(" [{}]", block_idx + 1),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ),
        ]));

        // Add highlighted code
        let highlighted_lines = highlight_code_block(code, &lang_display);
        for line in highlighted_lines {
            lines.push(line);
        }

        // Skip the code block lines in the original content
        i = end_line + 1;

        // Add code block footer
        lines.push(Line::from(Span::styled(
            "```",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Add any remaining lines after the last code block
    while i < content_lines.len() {
        lines.push(format_text_line(content_lines[i]));
        i += 1;
    }

    lines
}

fn format_text_line(line: &str) -> Line<'static> {
    // Check for headers first
    if line.starts_with("# ") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    } else if line.starts_with("## ") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ));
    } else if line.starts_with("### ") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));
    } else if line.starts_with("> ") {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
    }

    // Handle list items
    if line.starts_with("- ") || line.starts_with("* ") {
        let mut spans = vec![Span::styled("• ", Style::default().fg(Color::Yellow))];
        let rest = &line[2..];
        spans.extend(parse_inline_markdown(rest));
        return Line::from(spans);
    }

    // Parse inline markdown
    let spans = parse_inline_markdown(line);
    if spans.is_empty() {
        Line::from("")
    } else {
        Line::from(spans)
    }
}

fn parse_inline_markdown(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current_text = String::new();

    while let Some(ch) = chars.next() {
        match ch {
            '`' => {
                // Inline code
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }

                let mut code = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == '`' {
                        chars.next();
                        break;
                    }
                    code.push(chars.next().unwrap());
                }

                if !code.is_empty() {
                    spans.push(Span::styled(
                        format!(" {} ", code),
                        Style::default()
                            .bg(Color::Rgb(40, 42, 54))
                            .fg(Color::Rgb(248, 248, 242)),
                    ));
                } else {
                    current_text.push('`');
                }
            }
            '*' => {
                // Check for bold or italic
                if let Some(&'*') = chars.peek() {
                    // Bold
                    chars.next(); // consume second *
                    if !current_text.is_empty() {
                        spans.push(Span::raw(current_text.clone()));
                        current_text.clear();
                    }

                    let mut bold_text = String::new();
                    let mut found_closing = false;
                    while let Some(next_ch) = chars.next() {
                        if next_ch == '*' && chars.peek() == Some(&'*') {
                            chars.next(); // consume second *
                            found_closing = true;
                            break;
                        }
                        bold_text.push(next_ch);
                    }

                    if found_closing && !bold_text.is_empty() {
                        spans.push(Span::styled(
                            bold_text,
                            Style::default().add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        current_text.push_str("**");
                        current_text.push_str(&bold_text);
                    }
                } else {
                    // Italic
                    if !current_text.is_empty() {
                        spans.push(Span::raw(current_text.clone()));
                        current_text.clear();
                    }

                    let mut italic_text = String::new();
                    let mut found_closing = false;
                    while let Some(&next_ch) = chars.peek() {
                        if next_ch == '*' && chars.clone().nth(1) != Some('*') {
                            chars.next(); // consume closing *
                            found_closing = true;
                            break;
                        }
                        italic_text.push(chars.next().unwrap());
                    }

                    if found_closing && !italic_text.is_empty() {
                        spans.push(Span::styled(
                            italic_text,
                            Style::default().add_modifier(Modifier::ITALIC),
                        ));
                    } else {
                        current_text.push('*');
                        current_text.push_str(&italic_text);
                    }
                }
            }
            _ => {
                current_text.push(ch);
            }
        }
    }

    if !current_text.is_empty() {
        spans.push(Span::raw(current_text));
    }

    spans
}

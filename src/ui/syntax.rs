use lazy_static::lazy_static;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style as SyntectStyle, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

lazy_static! {
    static ref SYNTAX_SET: SyntaxSet = SyntaxSet::load_defaults_newlines();
    static ref THEME_SET: ThemeSet = ThemeSet::load_defaults();
}

/// Convert syntect Style to ratatui Style
fn syntect_to_ratatui_style(style: SyntectStyle) -> Style {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);

    let mut ratatui_style = Style::default().fg(fg);

    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::BOLD)
    {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::ITALIC)
    {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::UNDERLINE)
    {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }

    ratatui_style
}

/// Highlight code block and return formatted lines
pub fn highlight_code_block(code: &str, language: &str) -> Vec<Line<'static>> {
    let syntax = SYNTAX_SET
        .find_syntax_by_token(language)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(language))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut lines = Vec::new();

    for line in LinesWithEndings::from(code) {
        let highlighted = highlighter.highlight_line(line, &SYNTAX_SET).unwrap();

        let spans: Vec<Span> = highlighted
            .into_iter()
            .map(|(style, text)| Span::styled(text.to_string(), syntect_to_ratatui_style(style)))
            .collect();

        // Remove trailing newline from the last span if present
        let mut spans = spans;
        if let Some(last_span) = spans.last_mut() {
            if last_span.content.ends_with('\n') {
                let mut content = last_span.content.to_string();
                content.pop();
                last_span.content = content.into();
            }
        }

        lines.push(Line::from(spans));
    }

    lines
}

/// Extract code blocks from markdown text
pub fn extract_code_blocks(text: &str) -> Vec<(Option<String>, String, usize, usize)> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].starts_with("```") {
            let start_line = i;
            let language = lines[i][3..].trim().to_string();
            let language = if language.is_empty() {
                None
            } else {
                Some(language)
            };

            i += 1;
            let code_start = i;

            // Find the closing ```
            while i < lines.len() && !lines[i].starts_with("```") {
                i += 1;
            }

            if i < lines.len() {
                let code = lines[code_start..i].join("\n");
                blocks.push((language, code, start_line, i));
            }
        }
        i += 1;
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_blocks() {
        let text = r#"Some text
```rust
fn main() {
    println!("Hello");
}
```
More text
```python
print("Hello")
```"#;

        let blocks = extract_code_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, Some("rust".to_string()));
        assert_eq!(blocks[1].0, Some("python".to_string()));
    }

    #[test]
    fn test_highlight_code_block() {
        let code = "fn main() {\n    println!(\"Hello\");\n}";
        let lines = highlight_code_block(code, "rust");
        assert!(!lines.is_empty());
    }
}

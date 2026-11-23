use anyhow::Result;
use copypasta::{ClipboardContext, ClipboardProvider};
use std::fs;
use std::path::Path;

pub fn copy_to_clipboard(content: &str) -> Result<()> {
    let mut ctx = match ClipboardContext::new() {
        Ok(ctx) => ctx,
        Err(e) => anyhow::bail!("Failed to initialize clipboard: {}", e),
    };

    if let Err(e) = ctx.set_contents(content.to_owned()) {
        anyhow::bail!("Failed to set clipboard contents: {}", e);
    }
    Ok(())
}

pub fn extract_code_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut current_block = String::new();

    for line in content.lines() {
        if line.trim().starts_with("```") {
            if in_block {
                // End of block
                blocks.push(current_block.trim().to_string());
                current_block.clear();
            }
            in_block = !in_block;
            continue;
        }

        if in_block {
            current_block.push_str(line);
            current_block.push('\n');
        }
    }

    blocks
}

pub fn export_code_blocks(content: &str, filepath: &str) -> Result<()> {
    let blocks = extract_code_blocks(content);
    if blocks.is_empty() {
        anyhow::bail!("No code blocks found in the message");
    }

    let path = Path::new(filepath);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = blocks.join("\n\n");
    fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_blocks() {
        let content = r#"Here's some code:
```rust
fn main() {
    println!("Hello");
}
```
And another:
```python
print("Hello")
```"#;

        let blocks = extract_code_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], "fn main() {\n    println!(\"Hello\");\n}");
        assert_eq!(blocks[1], "print(\"Hello\")");
    }
}

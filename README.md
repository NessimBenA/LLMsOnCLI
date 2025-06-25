# llm-chat

A fast, minimal terminal app for LLM interactions with vim-like commands, syntax highlighting, and powerful UI features.

## Features

### Core Features
- Zero configuration needed to start
- Instant response time
- Vim-like command interface
- Support for multiple LLM models via OpenRouter
- Clean, minimal TUI design
- Automatic conversation history with SQLite
- Full-text search across past conversations
- Visual conversation tree view

### New UI/UX Features
- **Syntax highlighting** for code blocks with automatic language detection
- **Markdown rendering** with bold, italic, headers, lists, and blockquotes
- **Enhanced navigation** with smooth scrolling and vim-style commands
- **Multi-line input editing** with full cursor control
- **Input history** with up/down arrow navigation
- **Configurable scroll speed** for better reading experience
- **In-conversation search** with highlighting and navigation

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/yourusername/llm-chat
cd llm-chat

# Build and install
cargo install --path .
```

## Configuration

Create a configuration file at `~/.config/llm-chat/config.toml`:

```toml
api_key = "your-openrouter-api-key"
default_model = "anthropic/claude-3.5-sonnet"  # optional, defaults to claude-3.5-sonnet
```

You can also use the `:config` command within the app to open the configuration file.

## Usage

### Basic Commands

- `i` - Enter insert mode to type a message
- `:copy` - Copy the last message to clipboard
- `:copy-code` - Copy all code blocks from the last message to clipboard
- `:export-code <filepath>` - Export all code blocks from the last message to a file
- `:q` or `:quit` - Quit the application
- `:c` or `:clear` - Clear chat history
- `:m <model>` - Switch to a different model
- `:config` - Open configuration file
- `:reload` - Reload configuration
- `:scrollspeed <1-20>` - Set scroll speed (default: 3)

### History and Search Commands

- `:list` - Show recent conversations
- `:load <id>` - Load a specific conversation
- `:search <query>` - Search through conversation history
- `:tree` - Toggle conversation tree view
- `:fork <model>` - Fork current conversation with a different model
- `/` - Search within current conversation (forward)
- `n` - Next search result
- `N` - Previous search result

### Conversation Forking

The app supports creating conversation branches with different models:

1. Start a conversation normally
2. Use `:fork <model>` to create a branch with a different model (e.g., `:fork claude-2`)
3. Use `:tree` to visualize the conversation hierarchy
4. Use `:load <id>` to switch between conversations in the tree

The tree view shows:
- Parent-child relationships between conversations
- Current conversation marked with an asterisk (*)
- Model information when different from the default
- Visual branches using ASCII characters

### Input Modes

- **Normal Mode** - Default mode for navigation
- **Insert Mode** - For typing messages (enter with `i`)
- **Command Mode** - For executing commands (enter with `:`)

### Keyboard Shortcuts

#### Mode Switching
- `i` - Enter insert mode to type a message
- `ESC` - Return to normal mode
- `:` - Enter command mode
- `/` - Start search in current conversation

#### Navigation (Normal Mode)
- `j` or `↓` - Scroll down slowly
- `k` or `↑` - Scroll up slowly
- `J` - Fast scroll down (5x speed)
- `K` - Fast scroll up (5x speed)
- `f` or `PageDown` - Page down
- `b` or `PageUp` - Page up
- `d` - Half page down
- `u` - Half page up
- `G` - Jump to bottom (latest messages)
- `g` - Jump to top
- `n` - Next search result
- `N` - Previous search result

#### Input Editing (Insert Mode)
- `Enter` - Send message
- `Ctrl+Enter` or `Shift+Enter` - Insert newline (multi-line input)
- `↑` - Previous line in multi-line input OR navigate input history
- `↓` - Next line in multi-line input OR navigate input history
- `Alt+↑` - Force navigate to previous input history
- `Alt+↓` - Force navigate to next input history
- `←` / `→` - Move cursor left/right
- `Home` - Jump to start of line
- `End` - Jump to end of line
- `Backspace` - Delete character before cursor

Note: New messages automatically scroll to the bottom when they arrive.

### Enhanced Display Features

#### Syntax Highlighting
Code blocks are automatically highlighted based on the language specified:
- Supports all major programming languages
- Each code block is numbered for easy reference (e.g., `[1]`, `[2]`)
- Preserves original formatting and indentation

#### Markdown Rendering
The app renders markdown elements for better readability:
- **Bold text** using `**text**`
- *Italic text* using `*text*`
- Headers (`#`, `##`, `###`) with different colors
- Lists (`-`, `*`) with bullet points
- Blockquotes (`>`) with special styling
- Inline code with background highlighting

### Conversation Management

Conversations are automatically saved to `~/.config/llm-chat/history.db`. Each conversation is assigned a unique ID that can be used to load it later. The conversation tree view (`:tree`) shows the hierarchical structure of conversations, including:

- Main conversation threads (root conversations)
- Forked conversations with different models
- Parent-child relationships using ASCII tree structure (`├──`, `└──`, `│`)
- Current active conversation marked with an asterisk (*)
- Model information in brackets (e.g., `[claude-2]`) when different from default

The tree structure makes it easy to:
- Experiment with different models on the same conversation
- Compare responses from different models
- Maintain multiple conversation branches
- Navigate between related conversations

Example tree view:
```
Conversation tree:

└── Initial conversation
    ├── Fork with Claude [anthropic/claude-3.5-sonnet]
    │   └── * Current conversation branch
    └── Alternative approach [claude-2]
```

Search functionality supports full-text search across all conversations, making it easy to find past discussions or code snippets. Search results include conversation IDs that can be used with `:load` to jump to specific points in the conversation tree.

### Code Block Management

The app provides special commands for working with code blocks (text enclosed in triple backticks):

- `:copy-code` extracts all code blocks from the last message and copies them to the clipboard
- `:export-code <filepath>` saves all code blocks from the last message to a specified file
- When multiple code blocks exist in a message, they are separated by newlines for readability
- Code blocks in any programming language are supported with syntax highlighting
- The original formatting and indentation of code blocks is preserved

Example of a code block that can be copied or exported:
```rust
fn main() {
    println!("Hello, World!");
}
```

## Tips and Tricks

1. **Quick Model Switching**: Use `:m gpt-4` to quickly switch models mid-conversation
2. **Efficient Navigation**: Combine `J`/`K` for fast scrolling and `j`/`k` for precise positioning
3. **Multi-line Code Input**: Use `Ctrl+Enter` when typing code to preserve formatting
4. **History Navigation**: Use arrow keys to quickly reuse previous prompts
5. **Search and Jump**: Use `/` to search for specific content, then `n`/`N` to navigate results
6. **Fork for Comparison**: Fork conversations to compare responses from different models

## Development

### Prerequisites

- Rust 1.70 or higher
- Cargo
- SQLite 3.x

### Building

```bash
cargo build --release
```

### Running Tests

```bash
cargo test
```

## License

MIT License
# llm-chat

A fast, minimal terminal app for LLM interactions with vim-like commands and powerful scripting capabilities.

## Features

- Zero configuration needed to start
- Instant response time
- Vim-like command interface
- Support for multiple LLM models via OpenRouter
- Clean, minimal TUI design
- Automatic conversation history
- Full-text search across past conversations
- Visual conversation tree view

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

### History and Search Commands

- `:list` - Show recent conversations
- `:load <id>` - Load a specific conversation
- `:search <query>` - Search through conversation history
- `:tree` - Toggle conversation tree view
- `:fork <model>` - Fork current conversation with a different model

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

#### Navigation
- `j` or `↓` - Scroll down through messages
- `k` or `↑` - Scroll up through messages
- `G` - Jump to bottom (latest messages)
- `g` - Jump to top
- `ENTER` - Send message (in insert mode) or execute command (in command mode)

Note: New messages automatically scroll to the bottom when they arrive.

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
- Code blocks in any programming language are supported
- The original formatting and indentation of code blocks is preserved

Example of a code block that can be copied or exported:
```rust
fn main() {
    println!("Hello, World!");
}
```

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

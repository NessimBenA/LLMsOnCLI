# llm-chat

A terminal chat client for [OpenRouter](https://openrouter.ai). You type in a vim-style interface, the reply streams in as the model writes it, and every conversation is saved to a local SQLite database that you can search. You can fork a conversation to another model and compare the branches.

## Features

- Replies stream in as they are generated. The interface stays live while you wait: you can scroll, start typing the next message, or cancel the reply with Ctrl+C.
- Normal, insert and command modes, with vim keys for scrolling and searching.
- Any model available on OpenRouter. Switch with `:m <model>`.
- Every exchange is saved to SQLite. `:search` runs a full-text search over all saved conversations.
- `:fork <model>` copies the current conversation into a new branch that uses another model. `:tree` shows all the branches.
- Syntax highlighting for fenced code blocks, and basic Markdown: headers, lists, bold, italic, inline code and quotes.
- Copy the last reply or its code blocks to the clipboard, or write the code blocks to a file.

## Install

You need a Rust toolchain (see https://rustup.rs). On Linux you also need a C compiler, `pkg-config` and the OpenSSL headers (`libssl-dev` on Debian and Ubuntu).

```sh
cargo install --git https://github.com/NessimBenA/LLMsOnCLI
```

This installs a binary called `llm-chat`. Add `--locked` to build with the exact dependency versions from `Cargo.lock`.

## API key

Get a key from https://openrouter.ai/keys and put it in the `OPENROUTER_API_KEY` environment variable before you start the app. This reads it without echoing it or saving it in your shell history:

```sh
read -rs OPENROUTER_API_KEY && export OPENROUTER_API_KEY
llm-chat
```

To set it for every session, export it from your shell profile instead.

## Configuration

The config file is optional. It lives at `~/.config/llm-chat/config.toml` on Linux and `~/Library/Application Support/llm-chat/config.toml` on macOS, and takes two keys:

- `default_model`: the model to start with. Without it the app uses `anthropic/claude-3.5-sonnet`.
- `api_key`: your OpenRouter key, used only when `OPENROUTER_API_KEY` is not set.

```toml
default_model = "openai/gpt-4o"
```

`:config` opens the file with `xdg-open` and `:reload` reads it again.

## Keybindings

The app starts in normal mode. Press `i` to type a message and `Esc` to go back.

Normal mode:

| Key | Action |
| --- | --- |
| `i` | Insert mode |
| `:` | Command mode |
| `/` | Search the current conversation |
| `n` / `N` | Next / previous search match |
| `j` / `k`, `Down` / `Up` | Scroll down / up by the scroll speed (3 lines by default) |
| `J` / `K` | Scroll five times faster |
| `d` / `u` | Scroll down / up 10 lines |
| `f` / `b`, `PageDown` / `PageUp` | Scroll down / up 20 lines |
| `G` / `g` | Jump to the bottom / top |
| `q` | Quit |

Insert mode:

| Key | Action |
| --- | --- |
| `Enter` | Send the message |
| `Alt+Enter`, `Ctrl+J` | New line. `Ctrl+Enter` and `Shift+Enter` also work in terminals that send them as separate keys |
| `Left` / `Right`, `Home` / `End` | Move the cursor |
| `Up` / `Down` | Move between lines, or step through sent messages when the input has one line |
| `Alt+Up` / `Alt+Down` | Step through sent messages |
| `Backspace` | Delete the character before the cursor |
| `Esc` | Back to normal mode |

In any mode, `Ctrl+C` cancels the reply that is streaming and keeps the part that already arrived. In command mode, `Enter` runs the command and `Esc` drops it.

## Commands

| Command | Action |
| --- | --- |
| `:m <model>` | Switch model, for example `:m openai/gpt-4o` |
| `:fork <model>` | Copy the current conversation into a new branch that uses `<model>` |
| `:tree` | Show or hide the conversation tree |
| `:list` | List the 10 most recent conversations |
| `:search <words>` | List saved conversations with a message that contains all the words |
| `:load <id>` | Open a saved conversation |
| `:c`, `:clear` | Start a new conversation |
| `:copy` | Copy the last reply to the clipboard |
| `:copy-code` | Copy the code blocks of the last reply |
| `:export-code <file>` | Write the code blocks of the last reply to a file |
| `:scrollspeed <1-20>` | Set how many lines `j` and `k` scroll |
| `:config` | Open the config file |
| `:reload` | Reload the config file |
| `:q`, `:quit` | Quit |

While a reply is streaming, `:clear`, `:load` and `:fork` are refused. Let the reply finish or cancel it first.

## History and forks

Conversations are saved in `history.db`, next to the config file. A conversation is saved when its first reply arrives, and each later exchange is added to it. Messages from the app itself, such as errors and command output, are not saved and are not sent to the model.

`:fork <model>` saves a copy of the current conversation as a child of it and switches to the new model, so the next messages go to the branch and the original stays as it was. `:tree` shows the whole family of the current conversation, with the model of each branch and a `*` on the one you are in. Use `:list` or `:search` to find an ID and `:load <id>` to move between branches.

## Development

```sh
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo build --release
```

The stream parser follows OpenRouter's streaming documentation (https://openrouter.ai/docs/api_reference/streaming), and its tests use the chunk formats shown there.

## License

MIT. See [LICENSE](LICENSE).

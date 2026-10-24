# jive

A modern terminal user interface (TUI) for the [J programming language](https://www.jsoftware.com/), built in Rust with [ratatui](https://github.com/ratatui-org/ratatui).

## Features

- **Interactive J REPL** - Run J expressions with a responsive, non-blocking interface powered by the J engine.
- **Offline J Documentation** - Browse the bundled J Dictionary and Phrases without internet access.
- **Context-Aware Help** - Press `F1` for help on the symbol/name under the cursor.
- **Fuzzy Search** - Press `Ctrl+P` to search across all bundled help pages.
- **Example Navigation** - Cycle through examples in the help pane with `e`, insert them with `y`.
- **Explain Mode** - Press `F2` or use `/explain` to break down a sentence into documented J primitives.
- **Command History** - Navigate previous inputs with `↑`/`↓`, or use reverse search (`Ctrl+R`).
- **Tokyo Night Theme** - Clean, readable syntax highlighting optimized for the terminal.

<img width="1293" height="761" alt="jive-ss" src="https://github.com/user-attachments/assets/3f5d273e-bd2f-4353-bc2d-e6276f860d20" />


## Requirements

- [Rust](https://rustup.rs/) (2024 edition)
- A J engine (`ijconsole` or J shared library) accessible to the application
- A terminal with support for 256 colors (Kitty/Sixel optional for graphics)
- Python3 for generating help pages

## Building

```bash
# Build all crates
cargo build

# Build the TUI
cargo build -p jive

# Run the TUI
cargo run -p jive
```

## Usage

### Keybindings

#### REPL

| Key | Action |
|---|---|
| `Enter` | Evaluate input |
| `↑` / `↓` | Navigate command history |
| `PgUp` / `PgDn` | Scroll history |
| `Ctrl+A` / `Ctrl+E` | Move to start/end of line |
| `Ctrl+K` / `Ctrl+U` | Kill to end/start of line |
| `Ctrl+R` | Reverse history search |
| `Esc` | Quit (when help/search closed) |

#### Help Pane

| Key | Action |
|---|---|
| `F1` | Toggle help |
| `Tab` | Switch focus between REPL and help |
| `Ctrl+P` | Open fuzzy search |
| `/help <topic>` | Jump to a help page |
| `/explain <sentence>` | Explain J sentence components |
| `↑` / `↓` | Scroll help |
| `PgUp` / `PgDn` | Page scroll |
| `b` / `Backspace` | Go back in help history |
| `f` | Go forward in help history |
| `e` | Cycle through examples (highlights in-page) |
| `y` | Insert selected example into REPL |
| `o` | Open source documentation in browser |
| `Esc` | Close help/search |

### Slash Commands

- `/help` - Open search
- `/help <topic>` - Open a specific topic (symbol, name, or phrase)
- `/explain <sentence>` - Analyze and explain a J sentence
- `/clear` - Clear history
- `/quit` / `/exit` - Exit

## Architecture

- **`crates/tui`** - Ratatui-based frontend using Elm Architecture (Model-View-Update)
- **`crates/lib`** - J engine interface (process or FFI bindings)
- **`crates/help`** - Bundled help index, search, and types
- **`tools/gen_help.py`** - Generates `crates/help/data/help.json` from J documentation sources

## Development

```bash
# Run tests
cargo test

# Lint
cargo clippy --all-targets

# Regenerate help data (requires J docs source)
python3 tools/gen_help.py
```

## TODO

- [ ] Improve example line detection to prefer command lines over output in mixed blocks
- [ ] Smarter scrolling to keep highlighted examples centered in view
- [ ] Support graphics rendering (Kitty/Sixel) for plots via `ratatui-image`
- [ ] Add more curated idioms and examples

## License

This project is in early development. See individual crate licenses for details.

# X-ray Shell

A lightweight desktop shell application built with **Rust** and **egui (eframe)**. X-ray Shell provides a simple graphical interface for executing system commands, viewing command output, and navigating directories.

## Features

- **Command Execution:** Execute system commands directly from the GUI.
- **Directory Navigation:** Change the current working directory using `cd`.
- **Clear Terminal:** Clear the output using the `clear` command.
- **Standard Output:** Display command output in the terminal window.
- **Error Handling:** Display command errors and standard error output.
- **Auto Scroll:** Automatically scroll to the latest command output.
- **Keyboard Support:** Press `Enter` to execute commands.
- **Terminal UI:** Monospace fonts and a minimal terminal-inspired interface.

## Tech Stack

- [Rust](https://www.rust-lang.org/)
- [egui](https://github.com/emilk/egui)
- [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)

## Getting Started

### Prerequisites

- Rust
- Cargo

### Installation

Clone the repository:

```bash
git clone <repository-url>
cd x-ray-shell
```

### Run the application

```bash
cargo run
```

### Build for production

```bash
cargo build --release
```

The compiled executable will be available inside:

```text
target/release/
```

## Usage

Launch X-ray Shell and enter commands in the input field at the bottom.

| Command | Description |
|---|---|
| `pwd` | Display current directory |
| `ls` | List directory contents |
| `cd /path` | Change working directory |
| `clear` | Clear terminal output |
| `whoami` | Display current username |
| `date` | Display system date |

You can also execute other system commands supported by your operating system.

## Project Structure

```text
x-ray-shell/
├── Cargo.toml
├── Cargo.lock
├── src/
│   └── main.rs
└── README.md
```

## Current Limitations

- Commands run synchronously and may temporarily freeze the UI.
- Interactive commands are not supported.
- Shell features such as pipes, redirects, and environment variable expansion are not implemented.
- Command parsing currently uses whitespace splitting.
- The current working directory is shared with the application process.

## Future Improvements

- [ ] Command history and arrow-key navigation.
- [ ] Tab-based command autocomplete.
- [ ] Interactive terminal support using `portable-pty`.
- [ ] Support for pipes and command chaining.
- [ ] Multiple terminal tabs.
- [ ] Syntax highlighting.
- [ ] Asynchronous command execution.
- [ ] Custom terminal themes.
- [ ] Process management.
- [ ] Cross-platform support (Linux, macOS, Windows).

## License

MIT License (or choose your preferred license).

---

**X-ray Shell** — A minimal desktop shell built with Rust.

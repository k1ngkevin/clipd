# clipd

<div align="center">
  <img width="1000" alt="clipboard manager tui" src="https://github.com/user-attachments/assets/4e4c3946-c873-49c5-84b2-ea4d0244b6a6" />
</div>

`clipd` is a keyboard-driven text clipboard history manager for Wayland. It
captures clipboard changes through
[`wl-clipboard`](https://github.com/bugaevc/wl-clipboard), stores them in a
local SQLite database, and provides both an interactive terminal UI and CLI
commands for managing the history.

## Features

- Automatic text clipboard capture with `wl-paste`
- Fast, case-insensitive fuzzy search
- Timestamped, Unicode-aware snippets and a scrollable full-entry preview
- Keyboard-driven copy, navigation, and deletion
- Local SQLite storage with no external database dependency
- CLI commands for scripting and direct history management

## Requirements

- Wayland session
- `wl-copy` and `wl-paste`, provided by
  [`wl-clipboard`](https://github.com/bugaevc/wl-clipboard), available on
  `PATH`
- Rust

## Installation

Build a release binary with Cargo:

```sh
cargo build --release
```

The binary will be available at `target/release/clipd`. To install it into
Cargo's binary directory instead, run:

```sh
cargo install --path .
```

## Quick start

Keep the clipboard watcher running in a terminal, compositor startup command,
or user service:

```sh
clipd watch
```

The watcher runs in the foreground and records text clipboard changes. Start
the interactive picker in another terminal:

```sh
clipd
```

The TUI loads the current history when it starts. Copying an entry from the
TUI moves it to the top of the history and exits, making `clipd` suitable for
use from a terminal shortcut or launcher. Reopen it to see entries captured
while the TUI was open.

## TUI controls

History rows show a timestamp and a short preview. Newlines are displayed as
`\n`, and long previews are truncated to 50 display columns without splitting
multibyte characters.

### History

| Key          | Action                                                |
| ------------ | ----------------------------------------------------- |
| `j` / `Down` | Select the next entry, wrapping at the end            |
| `k` / `Up`   | Select the previous entry, wrapping at the beginning  |
| `Enter`      | Copy the selected entry, move it to the top, and exit |
| `Backspace`  | Delete the selected entry immediately                 |
| `Space`      | Open the full entry preview                           |
| `/`          | Start fuzzy search                                    |
| `q` / `Esc`  | Exit                                                  |

### Search

Search results update as you type and are ranked using case-insensitive fuzzy
matching, with prefix matches preferred.

| Key                     | Action                                                           |
| ----------------------- | ---------------------------------------------------------------- |
| Typing and editing keys | Update the search query                                          |
| `Enter`                 | Keep the filtered results and return to history mode             |
| `Esc`                   | Clear the query, restore all entries, and return to history mode |

### Full preview

| Key          | Action                                       |
| ------------ | -------------------------------------------- |
| `j` / `Down` | Scroll down                                  |
| `k` / `Up`   | Scroll up                                    |
| `Space`      | Return to the history list                   |
| `Enter`      | Copy the entry, move it to the top, and exit |
| `q` / `Esc`  | Exit                                         |

## CLI commands

The TUI and CLI use the same history database.

| Command              | Description                                                           |
| -------------------- | --------------------------------------------------------------------- |
| `clipd`              | Open the interactive TUI                                              |
| `clipd watch`        | Watch for text clipboard changes in the foreground                    |
| `clipd store [TEXT]` | Store an argument, or read the entry from standard input when omitted |
| `clipd list [LIMIT]` | Print recent entries and their IDs; the default limit is `20`         |
| `clipd select <ID>`  | Copy an entry to the Wayland clipboard                                |
| `clipd delete <ID>`  | Delete an entry immediately                                           |
| `clipd clear`        | Delete the entire history without confirmation                        |

example:

```sh
clipd store "hello world"
printf 'hello world' | clipd store
clipd list
clipd list 50
clipd select 1
clipd delete 1
clipd clear
```

Run `clipd --help` or `clipd <command> --help` for the complete CLI help.

## Data and privacy

Clipboard history is stored as plaintext in:

```text
$XDG_CONFIG_HOME/clipd/clipd_history.db
```

When `XDG_CONFIG_HOME` is not set, this is typically
`~/.config/clipd/clipd_history.db`. `clipd` attempts to set the directory to
mode `0700` and the database file to mode `0600`.

## Development

```sh
cargo test
cargo run
```

`cargo run` opens the TUI; use `cargo run -- watch` to run the watcher from a
development build.

## License

[MIT](LICENSE)

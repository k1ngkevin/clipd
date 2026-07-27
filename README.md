# clipd

`clipd` is a simple clipboard history manager for Wayland. It stores text
clipboard entries in a local SQLite database and lets you list, restore, and
delete them from the command line.

## Requirements

- Rust
- [`wl-clipboard`](https://github.com/bugaevc/wl-clipboard), which provides
  `wl-copy` and `wl-paste`

## Install

Build the project with Cargo:

```sh
cargo build --release
```

The executable will be available at `target/release/clipd`. You can also
install it to your Cargo binary directory:

```sh
cargo install --path .
```

## Usage

Start watching the clipboard and saving copied text:

```sh
clipd watch
```

Store text manually:

```sh
clipd store "hello world"
echo "hello world" | clipd store
```

View recent entries:

```sh
clipd list
clipd list 50
```

Copy an entry back to the clipboard using its ID:

```sh
clipd select 1
```

Delete an entry or clear the entire history:

```sh
clipd delete 1
clipd clear
```

Run `clipd --help` or `clipd <command> --help` for more information.

## Data

Clipboard history is stored in `clipd/clipd_history.db` inside your operating
system's configuration directory.

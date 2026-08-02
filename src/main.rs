pub mod app;
pub mod wayland;

use crate::wayland::{
    clear_db, delete_entry, initialize_db, list_entries, select_entry, store_entry, watch_clipboard,
};
use anyhow::{Context, Result};
use app::App;
use clap::{Parser, Subcommand};
use std::{
    env, fs,
    io::{self, Read},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
};

#[derive(Parser, Debug)]
#[command(name = "clipd")]
#[command(version = "0.1")]
#[command(about = "clipboard manager gui for wayland", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// start listening to clipboard, similar to wl-paste --watch
    Watch,

    /// Read text from stdin into clipboard and save
    Store {
        #[arg(allow_hyphen_values = true)]
        content: Option<String>,
    },

    /// List out data stored in clipboard (default is 20)
    List {
        #[arg(default_value_t = 20)]
        limit: i64,
    },

    /// Select item into current clipboard by id
    Select { id: i64 },

    /// Delete stored clipboard item by id
    Delete { id: i64 },

    /// Clear clipboard stored history
    Clear,
}

fn main() -> Result<()> {
    let db_path = find_or_create_db()?;

    let db_path = db_path
        .to_str()
        .context("database path contains invalid UTF-8")?;

    let conn = initialize_db(&db_path)?;

    let cli = Cli::parse();

    match cli.command {
        None => ratatui::run(|terminal| App::new(&conn).run(terminal)),
        Some(Commands::Watch) => watch_clipboard(),

        Some(Commands::List { limit }) => {
            let clipboard_entries = list_entries(&conn, limit)?;
            for (i, entry) in clipboard_entries.iter().enumerate() {
                println!(
                    "{}. [id {}] [timestamp {}]\n{}",
                    i + 1,
                    entry.id,
                    entry.timestamp,
                    entry.data
                );
            }
            Ok(())
        }

        Some(Commands::Store { content }) => {
            let content = match content {
                Some(content) => content,
                None => {
                    let mut buf = String::new();
                    io::stdin().lock().read_to_string(&mut buf)?;
                    buf
                }
            };

            let clipboard_state = match env::var("CLIPBOARD_STATE") {
                Ok(state) => Some(state),
                Err(env::VarError::NotPresent) => None,
                Err(env::VarError::NotUnicode(_)) => {
                    eprintln!("clipboard state contains invalid Unicode");
                    return Ok(());
                }
            };

            if let Some(state) = clipboard_state.as_deref()
                && !matches!(state, "data" | "nil" | "clear" | "sensitive")
            {
                eprintln!("unknown clipboard state: {state}");
            }

            if should_store_clipboard_state(clipboard_state.as_deref()) {
                store_entry(&conn, content)?;
            }
            Ok(())
        }

        Some(Commands::Select { id }) => select_entry(&conn, id),

        Some(Commands::Delete { id }) => delete_entry(&conn, id),

        Some(Commands::Clear) => {
            clear_db(&conn)?;
            Ok(())
        }
    }
}

pub fn find_or_create_db() -> Result<PathBuf> {
    let mut path = dirs::config_dir().context("error finding home directory")?;
    path.push("clipd");

    fs::create_dir_all(&path).context("failed to create clipboard database")?;

    if let Err(err) = fs::set_permissions(&path, fs::Permissions::from_mode(0o700)) {
        eprintln!(
            "failed to set 0700 permissions on database directory: {}",
            err
        );
    }

    path.push("clipd_history.db");

    Ok(path)
}

fn should_store_clipboard_state(state: Option<&str>) -> bool {
    matches!(state, None | Some("data"))
}

#[cfg(test)]
mod tests {
    use super::should_store_clipboard_state;

    #[test]
    fn stores_normal_and_manual_clipboard_content() {
        assert!(should_store_clipboard_state(Some("data")));
        assert!(should_store_clipboard_state(None));
    }

    #[test]
    fn skips_non_data_clipboard_states() {
        for state in ["nil", "clear", "sensitive", "future-state"] {
            assert!(!should_store_clipboard_state(Some(state)));
        }
    }
}

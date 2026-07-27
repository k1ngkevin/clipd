pub mod wayland;

use crate::wayland::{
    clear_db, delete_entry, initialize_db, list_entries, select_entry, store_entry, watch_clipboard,
};
use anyhow::{Context, Ok, Result};
use clap::{Parser, Subcommand};
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};

#[derive(Parser, Debug)]
#[command(name = "clipd")]
#[command(version = "0.1")]
#[command(about = "clipboard manager tui for wayland", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

    /// Delete stored clipabord item by id
    Delete { id: i64 },

    /// Clear clipboard stored history
    Clear,
}

fn main() -> Result<()> {
    let db_path = find_and_create_db()?;

    let db_path = db_path
        .to_str()
        .context("database path contains invalid UTF-8")?;

    let conn = initialize_db(&db_path)?;

    let cli = Cli::parse();
    match cli.command {
        Commands::Watch => watch_clipboard(),

        Commands::List { limit } => {
            let clipboard_entires = list_entries(conn, limit)?;
            for (i, entry) in clipboard_entires.iter().enumerate() {
                println!("{}. [id {}] {}", i + 1, entry.id, entry.data);
            }
            Ok(())
        }

        Commands::Store { content } => {
            let content = match content {
                Some(content) => content,
                None => {
                    let mut buf = String::new();
                    io::stdin().lock().read_to_string(&mut buf)?;
                    buf
                }
            };

            store_entry(conn, content)?;
            Ok(())
        }

        Commands::Select { id } => select_entry(conn, id),

        Commands::Delete { id } => delete_entry(&conn, id),

        Commands::Clear => {
            clear_db(conn)?;
            Ok(())
        }
    }
}

pub fn find_and_create_db() -> Result<PathBuf> {
    let mut path = dirs::config_dir().context("error finding home directory")?;
    path.push("clipd");

    fs::create_dir_all(&path).context("failed to create clipboard database")?;
    path.push("clipd_history.db");

    Ok(path)
}

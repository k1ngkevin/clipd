use anyhow::Context;
use rusqlite::{Connection, OptionalExtension, Result, params};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    {env, fs},
};

#[derive(Clone, Debug)]
pub struct ClipboardEntry {
    pub id: i64,
    pub data: String,
    pub timestamp: String,
}

pub fn initialize_db(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS clipd (
        id INTEGER PRIMARY KEY,
        sort_order INTEGER NOT NULL DEFAULT 0,
        data TEXT NOT NULL,
        timestamp TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
    )
    ",
        (),
    )?;

    if let Err(err) = fs::set_permissions(&db_path, fs::Permissions::from_mode(0o600)) {
        eprintln!("failed to set 0600 permissions on database file: {}", err);
    }

    Ok(conn)
}

pub fn store_entry(conn: &Connection, data: String) -> Result<i64> {
    conn.execute(
        "INSERT INTO clipd 
        (sort_order, data) 
        VALUES (
        (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM clipd), 
        ?1
        )",
        params![data],
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn promote_entry(conn: &Connection, id: i64) -> anyhow::Result<()> {
    let updated = conn.execute(
        "UPDATE clipd
        SET sort_order = (
            SELECT COALESCE(MAX(sort_order), 0) + 1
            FROM clipd
        )
        WHERE id = ?1
        ",
        params![id],
    )?;

    if updated == 0 {
        anyhow::bail!("no clipboard entry with id: {id}");
    }

    Ok(())
}

pub fn list_entries(conn: &Connection, limit: i64) -> Result<Vec<ClipboardEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, data, timestamp
        FROM clipd 
        ORDER BY sort_order DESC, id DESC 
        LIMIT ?1",
    )?;

    let clipboard_iter = stmt.query_map(params![limit], |row| {
        Ok(ClipboardEntry {
            id: row.get(0)?,
            data: row.get(1)?,
            timestamp: row.get(2)?,
        })
    })?;

    let mut clipboard_entries = Vec::new();
    for entry in clipboard_iter {
        clipboard_entries.push(entry?);
    }

    Ok(clipboard_entries)
}

pub fn count_entries(conn: &Connection) -> Result<i64> {
    let count: i64 = conn.query_row(
        "
    SELECT COUNT(*) from clipd 
    ",
        [],
        |row| row.get(0),
    )?;

    Ok(count)
}

pub fn select_entry(conn: &Connection, id: i64) -> anyhow::Result<()> {
    let data: Option<String> = conn
        .query_row("SELECT data FROM clipd WHERE id = ?1", params![id], |row| {
            row.get(0)
        })
        .optional()?;

    let data = data.with_context(|| format!("no clipboard entry with id: {id}"))?;

    let mut child = Command::new("wl-copy").stdin(Stdio::piped()).spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(data.as_bytes())?;
    }

    child.wait()?;
    Ok(())
}

pub fn delete_entry(conn: &Connection, id: i64) -> anyhow::Result<()> {
    let deleted = conn.execute("DELETE FROM clipd WHERE id = ?1", params![id])?;

    if deleted == 0 {
        anyhow::bail!("no clipboard entry with id: {id}");
    }

    Ok(())
}

pub fn clear_db(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM clipd", ())?;
    Ok(())
}

pub fn watch_clipboard() -> anyhow::Result<()> {
    let executable = env::current_exe().context("failed to locate current executable")?;

    let status = Command::new("wl-paste")
        .args(["--type", "text", "--watch"])
        .arg(executable)
        .arg("store")
        .status()
        .context("failed to start wl-paste")?;

    if !status.success() {
        anyhow::bail!("wl-paste exited status: {status}")
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute(
            "CREATE TABLE clipd (
                id INTEGER PRIMARY KEY,
                sort_order INTEGER NOT NULL DEFAULT 0,    
                data TEXT NOT NULL,
                timestamp TEXT DEFAULT CURRENT_TIMESTAMP NOT NULL
            )",
            (),
        )
        .expect("create clipd table");
        conn
    }

    #[test]
    fn store_entry_stores_data_and_returns_id() {
        let conn = test_db();
        let id = store_entry(&conn, "hello world".to_string()).expect("store entry");

        let data: String = conn
            .query_row("SELECT data FROM clipd WHERE id = ?1", params![id], |row| {
                row.get(0)
            })
            .expect("find entry using returned id");

        assert_eq!(data, "hello world");
    }
    #[test]
    fn store_entry_assigns_highest_sort_order() {
        let conn = test_db();

        let first_id = store_entry(&conn, "first entry".to_string()).expect("stores first entry");
        let second_id =
            store_entry(&conn, "second entry".to_string()).expect("stores second entry");
        let third_id = store_entry(&conn, "third entry".to_string()).expect("stores third entry");

        let first_order: i64 = conn
            .query_row(
                "SELECT sort_order FROM clipd WHERE id = ?1",
                [first_id],
                |row| row.get(0),
            )
            .expect("find sort_order using id");

        let second_order: i64 = conn
            .query_row(
                "SELECT sort_order FROM clipd WHERE id = ?1",
                [second_id],
                |row| row.get(0),
            )
            .expect("find sort_order using id");

        let third_order: i64 = conn
            .query_row(
                "SELECT sort_order FROM clipd WHERE id = ?1",
                [third_id],
                |row| row.get(0),
            )
            .expect("find sort_order using id");

        assert_eq!(first_order, 1);
        assert_eq!(second_order, 2);
        assert_eq!(third_order, 3);
    }

    #[test]
    fn promote_entries_assigns_highest() {
        let conn = test_db();

        let first_id = store_entry(&conn, "first entry".to_string()).expect("stores first entry");
        let second_id =
            store_entry(&conn, "second entry".to_string()).expect("stores second entry");
        let third_id = store_entry(&conn, "third entry".to_string()).expect("stores third entry");

        promote_entry(&conn, first_id).expect("promotes entry");

        let first_order: i64 = conn
            .query_row(
                "SELECT sort_order FROM clipd WHERE id = ?1",
                [first_id],
                |row| row.get(0),
            )
            .expect("find sort_order using id");

        let second_order: i64 = conn
            .query_row(
                "SELECT sort_order FROM clipd WHERE id = ?1",
                [second_id],
                |row| row.get(0),
            )
            .expect("find sort_order using id");

        let third_order: i64 = conn
            .query_row(
                "SELECT sort_order FROM clipd WHERE id = ?1",
                [third_id],
                |row| row.get(0),
            )
            .expect("find sort_order using id");

        assert!(first_order > second_order);
        assert!(first_order > third_order);
        assert!(third_order > second_order);
    }

    #[test]
    fn list_entries_returns_newest_first() {
        let conn = test_db();

        store_entry(&conn, "first entry".to_string()).expect("stores first entry");
        store_entry(&conn, "second entry".to_string()).expect("stores second entry");
        store_entry(&conn, "third entry".to_string()).expect("stores third entry");

        let entries = list_entries(&conn, 5).expect("list entries");

        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].data, "third entry");
        assert_eq!(entries[1].data, "second entry");
        assert_eq!(entries[2].data, "first entry");

        assert!(entries[0].id > entries[1].id);
        assert!(entries[1].id > entries[2].id);
    }

    #[test]
    fn list_entries_returns_highest_order() {
        let conn = test_db();

        let first_id = store_entry(&conn, "first entry".to_string()).expect("stores first entry");
        store_entry(&conn, "second entry".to_string()).expect("stores second entry");
        store_entry(&conn, "third entry".to_string()).expect("stores third entry");

        promote_entry(&conn, first_id).expect("promotes entry");

        let entries = list_entries(&conn, 5).expect("list entries");
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].data, "first entry");
        assert_eq!(entries[1].data, "third entry");
        assert_eq!(entries[2].data, "second entry");
    }

    #[test]
    fn list_entries_respects_limit() {
        let conn = test_db();

        store_entry(&conn, "first entry".to_string()).expect("stores first entry");
        store_entry(&conn, "second entry".to_string()).expect("stores second entry");
        store_entry(&conn, "third entry".to_string()).expect("stores third entry");

        let entries = list_entries(&conn, 2).expect("list entries");

        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].data, "third entry");
        assert_eq!(entries[1].data, "second entry");

        assert!(entries[0].id > entries[1].id);
    }

    #[test]
    fn clear_db_removes_all_entries() {
        let conn = test_db();
        let data = vec!["blue", "red", "orange", "yellow", "purple"];
        for item in data.iter() {
            store_entry(&conn, item.to_string()).expect("insert into database");
        }

        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM clipd", (), |row| row.get::<_, i64>(0))
                .expect("count entries") as usize,
            data.len()
        );

        clear_db(&conn).expect("clear database");

        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM clipd", (), |row| row.get::<_, i64>(0))
                .expect("count entries"),
            0
        );
    }

    #[test]
    fn select_entry_errors_for_missing_id() {
        let conn = test_db();
        let error = select_entry(&conn, 69).expect_err("cannot select missing entry");

        assert_eq!(error.to_string(), "no clipboard entry with id: 69");
    }

    #[test]
    fn promote_entry_errors_for_missing_id() {
        let conn = test_db();
        let error = promote_entry(&conn, 69).expect_err("cannot promote missing entry");

        assert_eq!(error.to_string(), "no clipboard entry with id: 69");
    }

    #[test]
    fn delete_entry_removes_existing_entry() {
        let conn = test_db();
        conn.execute("INSERT INTO clipd (data) VALUES (?1)", params!["example"])
            .expect("insert test entry");
        let id = conn.last_insert_rowid();

        delete_entry(&conn, id).expect("delete existing entry");

        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM clipd", (), |row| row.get::<_, i64>(0))
                .expect("count entries"),
            0
        );
    }

    #[test]
    fn delete_entry_errors_for_missing_id() {
        let conn = test_db();
        let error = delete_entry(&conn, 42).expect_err("cannot delete missing entry");

        assert_eq!(error.to_string(), "no clipboard entry with id: 42");
    }
}

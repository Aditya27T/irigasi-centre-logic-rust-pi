use anyhow::{Context, Result};
use rusqlite::Connection;

pub fn open(path: &str) -> Result<Connection> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("membuat direktori {}", parent.display()))?;
        }
    }
    let conn = Connection::open(path).with_context(|| format!("membuka SQLite {path}"))?;
    conn.execute_batch(include_str!("schema.sql"))
        .context("inisialisasi schema SQLite")?;
    Ok(conn)
}

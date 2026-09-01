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
    // WAL + busy_timeout supaya connection ini bisa hidup berdampingan dengan connection
    // lain (dashboard) di file SQLite yang sama tanpa SQLITE_BUSY.
    conn.pragma_update(None, "journal_mode", "WAL")
        .context("set journal_mode WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)
        .context("set busy_timeout")?;
    conn.execute_batch(include_str!("schema.sql"))
        .context("inisialisasi schema SQLite")?;
    migrate_twin_state_columns(&conn).context("migrasi kolom twin_state")?;
    Ok(conn)
}

/// SQLite tidak punya `ADD COLUMN IF NOT EXISTS` -- untuk database yang sudah ada dari
/// sebelum kolom valve_*_source/expires_at ditambahkan, cek dulu lewat pragma_table_info
/// baru ALTER TABLE kalau memang belum ada. Database baru sudah dapat kolom ini langsung
/// dari CREATE TABLE di schema.sql, jadi loop ini jadi no-op untuknya.
fn migrate_twin_state_columns(conn: &Connection) -> Result<()> {
    let mut existing = std::collections::HashSet::new();
    conn.pragma(None, "table_info", "twin_state", |row| {
        existing.insert(row.get::<_, String>(1)?);
        Ok(())
    })?;

    let wanted: &[(&str, &str)] = &[
        ("valve_1_source", "TEXT NOT NULL DEFAULT 'auto'"),
        ("valve_1_expires_at", "INTEGER"),
        ("valve_2_source", "TEXT NOT NULL DEFAULT 'auto'"),
        ("valve_2_expires_at", "INTEGER"),
    ];
    for (name, def) in wanted {
        if !existing.contains(*name) {
            conn.execute_batch(&format!("ALTER TABLE twin_state ADD COLUMN {name} {def}"))
                .with_context(|| format!("menambah kolom twin_state.{name}"))?;
        }
    }
    Ok(())
}

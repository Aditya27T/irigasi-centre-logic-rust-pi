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
    conn.pragma_update(None, "journal_mode", "WAL")
        .context("set journal_mode WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)
        .context("set busy_timeout")?;
    conn.execute_batch(include_str!("schema.sql"))
        .context("inisialisasi schema SQLite")?;
    migrate_twin_state_columns(&conn).context("migrasi kolom twin_state")?;
    // Sengaja NON-fatal: kode sudah tidak menyentuh kolom ini sama sekali, jadi kegagalan drop
    // tidak berbahaya. Sebaliknya `?` di sini akan menggagalkan startup dan membuat
    // irigasi.service crashloop di Pi (Restart=on-failure + RestartSec=5) hanya karena
    // pembersihan kosmetik.
    if let Err(err) = drop_twin_state_water_balance(&conn) {
        tracing::warn!("Gagal drop kolom twin_state.water_balance_mm (diabaikan): {err:#}");
    }
    Ok(conn)
}

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

fn drop_twin_state_water_balance(conn: &Connection) -> Result<()> {
    let mut existing = std::collections::HashSet::new();
    conn.pragma(None, "table_info", "twin_state", |row| {
        existing.insert(row.get::<_, String>(1)?);
        Ok(())
    })?;
    if existing.contains("water_balance_mm") {
        conn.execute_batch("ALTER TABLE twin_state DROP COLUMN water_balance_mm")
            .context("drop column water_balance_mm")?;
    }
    Ok(())
}

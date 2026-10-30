use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::models::{Settings, SettingsInput};

pub fn get_settings(conn: &Connection) -> AppResult<Settings> {
    let mut s = Settings::default();
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (k, v) = row?;
        match k.as_str() {
            "refresh_interval_minutes" => {
                if let Ok(n) = v.parse::<i64>() {
                    s.refresh_interval_minutes = n;
                }
            }
            "theme" => s.theme = v,
            _ => {}
        }
    }
    Ok(s)
}

fn upsert(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Applies only the provided fields and returns the resulting settings.
pub fn update_settings(conn: &Connection, input: &SettingsInput) -> AppResult<Settings> {
    if let Some(n) = input.refresh_interval_minutes {
        upsert(conn, "refresh_interval_minutes", &n.to_string())?;
    }
    if let Some(t) = &input.theme {
        upsert(conn, "theme", t)?;
    }
    get_settings(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_pool;

    #[test]
    fn defaults_and_partial_update() {
        let (pool, _d) = test_pool();
        let conn = pool.get().unwrap();
        let s = get_settings(&conn).unwrap();
        assert_eq!(s.refresh_interval_minutes, 30);
        assert_eq!(s.theme, "system");

        let updated = update_settings(
            &conn,
            &SettingsInput {
                refresh_interval_minutes: Some(15),
                theme: None,
            },
        )
        .unwrap();
        assert_eq!(updated.refresh_interval_minutes, 15);
        assert_eq!(updated.theme, "system");
    }
}

use crate::room::PlayerData;
use anyhow::Result;
use sqlite;
use sqlite::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct Database {
    conn: Arc<sqlite::ConnectionThreadSafe>,
}

impl Database {
    pub fn new() -> Result<Self> {
        let conn = sqlite::Connection::open_thread_safe("./db.sqlite3")?;
        conn.execute("PRAGMA foreign_keys=ON")?;
        Ok(Database {
            conn: Arc::new(conn),
        })
    }

    pub fn migrate(&self) -> Result<()> {
        Ok(self.conn.execute(
            "
            CREATE TABLE IF NOT EXISTS \"user\" (
                id integer PRIMARY KEY,
                name text UNIQUE
            );
            CREATE TABLE IF NOT EXISTS visit (
                user_id integer PRIMARY KEY REFERENCES \"user\"(id) ON DELETE CASCADE,
                visitor_id integer NULL REFERENCES \"user\"(id) ON DELETE SET NULL,
                visited_at TEXT NOT NULL
            );
        ",
        )?)
    }

    pub fn change_name(&self, old_name: &str, new_name: &str) -> Result<()> {
        let res = self
            .conn
            .prepare(
                "
                UPDATE \"user\"
                SET name = :new_name
                WHERE name = :name
                ",
            )?
            .into_iter()
            .bind(&[(":name", old_name), (":new_name", new_name)][..])?
            .next();
        if let Some(Err(err)) = res {
            if let Some(message) = err.message {
                if message == "UNIQUE constraint failed: user.name" {
                    self.conn
                        .prepare(
                            "
                            DELETE FROM \"user\"
                            WHERE name = :name
                            ",
                        )?
                        .into_iter()
                        .bind(&[(":name", old_name)][..])?
                        .next();
                }
            }
        }
        if self.conn.change_count() == 0 {
            self.conn
                .prepare(
                    "
                INSERT INTO user(name) VALUES(:name)
                ON CONFLICT (name) DO UPDATE SET
                    name = excluded.name
            ",
                )?
                .into_iter()
                .bind((":name", new_name))?
                .next();
        }
        Ok(())
    }

    pub fn all_players(&self) -> Result<Vec<PlayerData>> {
        let mut rows = self
            .conn
            .prepare(
                "
                SELECT id, name, visited_at IS NOT NULL AS visited
                FROM \"user\" AS u
                LEFT JOIN visit AS v ON u.id = v.user_id
                ORDER BY id
            ",
            )?
            .into_iter()
            .bind::<&[(&str, Value)]>(&[][..])?;

        let mut result = vec![];

        while let Some(r) = rows.next() {
            r?;
            result.push(PlayerData {
                id: rows.read::<i64, _>("id")? as u64,
                name: rows.read::<String, _>("name")?,
                visited: rows.read::<i64, _>("visited")? != 0,
            })
        }

        Ok(result)
    }

    pub fn get_player(&self, user_id: u64) -> Result<Option<PlayerData>> {
        let mut rows = self
            .conn
            .prepare(
                "
                SELECT id, name, visited_at IS NOT NULL AS visited
                FROM \"user\" AS u
                LEFT JOIN visit AS v ON u.id = v.user_id
                WHERE id = :id
            ",
            )?
            .into_iter()
            .bind(&[(":id", Value::Integer(user_id as i64))][..])?;

        Ok(if let Some(Ok(_)) = rows.next() {
            Some(PlayerData {
                id: rows.read::<i64, _>("id")? as u64,
                name: rows.read::<String, _>("name")?,
                visited: rows.read::<i64, _>("visited")? != 0,
            })
        } else {
            None
        })
    }

    pub fn get_player_by_name(&self, name: &str) -> Result<Option<PlayerData>> {
        let mut rows = self
            .conn
            .prepare(
                "
                SELECT id, name, visited_at IS NOT NULL AS visited
                FROM \"user\" AS u
                LEFT JOIN visit AS v ON u.id = v.user_id
                WHERE name = :name
            ",
            )?
            .into_iter()
            .bind(&[(":name", name)][..])?;

        Ok(if let Some(Ok(_)) = rows.next() {
            Some(PlayerData {
                id: rows.read::<i64, _>("id")? as u64,
                name: rows.read::<String, _>("name")?,
                visited: rows.read::<i64, _>("visited")? != 0,
            })
        } else {
            None
        })
    }

    pub(crate) fn cleared(&self, user_id: u64) -> Result<()> {
        self.conn
            .prepare(
                "
                    DELETE FROM visit
                    WHERE user_id = :user_id
                ",
            )?
            .into_iter()
            .bind(&[(":user_id", Value::Integer(user_id as i64))][..])?
            .next();
        Ok(())
    }

    pub(crate) fn visited(&self, visitor_id: u64, visited_id: u64) -> Result<()> {
        self.conn
            .prepare(
                "
                    INSERT INTO visit(user_id, visitor_id, visited_at)
                    VALUES (:user_id, :visitor_id, datetime('now'))
                    ON CONFLICT (user_id) DO UPDATE SET
                        visitor_id = excluded.visitor_id,
                        visited_at = excluded.visited_at
                ",
            )?
            .into_iter()
            .bind(
                &[
                    (":user_id", Value::Integer(visited_id as i64)),
                    (":visitor_id", Value::Integer(visitor_id as i64)),
                ][..],
            )?
            .next();
        Ok(())
    }
}

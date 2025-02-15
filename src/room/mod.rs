use anyhow::{Context, Result};
use crate::db;

#[derive(Clone)]
pub struct PlayerData {
    pub id: u64,
    pub name: String,
    pub visited: bool,
}

pub struct Room {
    db: db::Database,
}

impl Room {
    pub fn new(db: db::Database) -> Self {
        Room{
            db,
        }
    }

    pub fn all_players(&self) -> Result<Vec<PlayerData>> {
        Ok(self.db.all_players()?)
    }

    pub fn change_name(&self, old_name: &str, new_name: &str) -> Result<()> {
        Ok(self.db.change_name(old_name, new_name)?)
    }

    pub fn visited(&self, player_id: u64, visitor_name: &str) -> Result<Option<PlayerData>> {
        let visitor = self.db.get_player_by_name(visitor_name)?.context("visitor not found")?;

        self.db.visited(visitor.id, player_id)?;

        Ok(self.db.get_player(player_id)?)
    }

    pub fn cleared(&self, name: &str) -> Result<Option<PlayerData>> {
        let player = self.db.get_player_by_name(name)?;
        Ok(match player {
            None => None,
            Some(player) => {
                self.db.cleared(player.id)?;
                self.db.get_player(player.id)?
            }
        })
    }
}
use std::collections::HashMap;
use std::sync::{Arc};
use itertools::Itertools;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone)]
pub struct PlayerData {
    pub id: u64,
    pub name: Arc<str>,
    pub visited: bool,
    created_at: std::time::Instant,
}

pub struct Room {
    players: HashMap<Arc<str>, PlayerData>,
    id_counter: AtomicU64,
}

impl Room {
    pub fn new() -> Self {
        Room{
            players: Default::default(),
            id_counter: AtomicU64::new(0),
        }
    }

    pub fn all_players(&self) -> Vec<PlayerData> {
        let mut result = self.players
            .values()
            .cloned()
            .collect_vec();
        result.sort_by_key(|x| x.created_at);
        result
    }

    pub fn change_name(&mut self, old_name: &str, new_name: String) -> () {
        let data_option = self.players
            .remove(old_name);

        let name:Arc<str> = new_name.into();
        let data = match data_option {
            None => PlayerData{
                id: self.id_counter.fetch_add(1, Ordering::Relaxed),
                name: name.clone(),
                created_at: std::time::Instant::now(),
                visited: false,
            },
            Some(mut d) => {
                d.name = name.clone();
                d
            }
        };

        self.players.insert(name, data);
    }

    pub fn visited(&mut self, player_id: u64) -> Option<PlayerData> {
        if let Some(data) = self.players.values_mut().find(|p| p.id == player_id) {
            data.visited = true;
            return Some(data.clone())
        }
        None
    }

    pub fn cleared(&mut self, name: &str) -> Option<PlayerData> {
        if let Some(data) = self.players.get_mut(name) {
            data.visited = false;
            return Some(data.clone())
        }
        None
    }
}
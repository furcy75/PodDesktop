//! Gestion de la file d'attente ordonnée (`queue`).
//!
//! Implémente les 4 stratégies d'insertion du Cahier des Charges v4.2 :
//! 1. En bas de la file (`Bottom`)
//! 2. En haut de la file (`Top` — position 0)
//! 3. Après l'épisode en cours (`AfterCurrent` — position 1 sans interrompre la lecture)
//! 4. Tri intelligent par date de publication croissante ou décroissante (`SmartDateAsc` / `SmartDateDesc`)

use crate::db::Database;
use crate::models::QueueInsertStrategy;
use rusqlite::params;

impl Database {
    /// Retourne la liste ordonnée des `episode_id` dans la file d'attente.
    pub fn get_queue_ids(&self) -> Result<Vec<i64>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT episode_id FROM queue ORDER BY position ASC")
            .map_err(|e| format!("Erreur get_queue_ids : {e}"))?;
        let ids = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| format!("Erreur lecture queue : {e}"))?
            .flatten()
            .collect();
        Ok(ids)
    }

    /// Insère un épisode dans la file selon la stratégie choisie.
    /// Si l'épisode est déjà dans la file, il est déplacé à sa nouvelle position (sauf pour `Bottom`
    /// où il conserve sa place si déjà présent, ou est replacé si demandé explicitement).
    pub fn enqueue_episode(
        &self,
        episode_id: i64,
        strategy: QueueInsertStrategy,
    ) -> Result<(), String> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Erreur transaction enqueue_episode : {e}"))?;

        // Charge la file actuelle avec les dates de publication pour les stratégies intelligentes
        let mut current: Vec<(i64, i64)> = {
            let mut stmt = tx
                .prepare(
                    "SELECT q.episode_id, e.pub_date
                     FROM queue q
                     INNER JOIN episodes e ON e.id = q.episode_id
                     ORDER BY q.position ASC",
                )
                .map_err(|e| format!("Erreur lecture queue actuelle : {e}"))?;
            let rows: Vec<(i64, i64)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| format!("Erreur map queue : {e}"))?
                .flatten()
                .collect();
            rows
        };

        let target_pub_date: i64 = tx
            .query_row(
                "SELECT pub_date FROM episodes WHERE id = ?1",
                [episode_id],
                |r| r.get(0),
            )
            .map_err(|e| format!("Épisode introuvable ({episode_id}) : {e}"))?;

        // Retire l'épisode s'il était déjà présent afin de le repositionner proprement
        current.retain(|(id, _)| *id != episode_id);

        match strategy {
            QueueInsertStrategy::Bottom => {
                current.push((episode_id, target_pub_date));
            }
            QueueInsertStrategy::Top => {
                current.insert(0, (episode_id, target_pub_date));
            }
            QueueInsertStrategy::AfterCurrent => {
                let idx = if current.is_empty() { 0 } else { 1 };
                current.insert(idx, (episode_id, target_pub_date));
            }
            QueueInsertStrategy::SmartDateAsc => {
                // Conserve l'épisode en cours (index 0) s'il y en a un, puis insère trié par date croissante
                if current.is_empty() {
                    current.push((episode_id, target_pub_date));
                } else {
                    let pos = current
                        .iter()
                        .skip(1)
                        .position(|(_, d)| *d > target_pub_date)
                        .map(|p| p + 1)
                        .unwrap_or(current.len());
                    current.insert(pos, (episode_id, target_pub_date));
                }
            }
            QueueInsertStrategy::SmartDateDesc => {
                if current.is_empty() {
                    current.push((episode_id, target_pub_date));
                } else {
                    let pos = current
                        .iter()
                        .skip(1)
                        .position(|(_, d)| *d < target_pub_date)
                        .map(|p| p + 1)
                        .unwrap_or(current.len());
                    current.insert(pos, (episode_id, target_pub_date));
                }
            }
        }

        tx.execute("DELETE FROM queue", [])
            .map_err(|e| format!("Erreur reset queue : {e}"))?;
        for (pos, (eid, _)) in current.into_iter().enumerate() {
            tx.execute(
                "INSERT INTO queue (position, episode_id) VALUES (?1, ?2)",
                params![pos as i64, eid],
            )
            .map_err(|e| format!("Erreur réécriture queue : {e}"))?;
        }

        // Un épisode ajouté à la file est retiré de la Boîte de réception (décision prise)
        let _ = tx.execute(
            "UPDATE episodes SET inbox_state = 0 WHERE id = ?1 AND inbox_state = 1",
            [episode_id],
        );

        tx.commit()
            .map_err(|e| format!("Erreur commit enqueue_episode : {e}"))?;
        Ok(())
    }

    /// Retire un ou plusieurs épisodes de la file d'attente et renumérote sans trou.
    pub fn remove_from_queue(&self, episode_ids: &[i64]) -> Result<(), String> {
        if episode_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Erreur transaction remove_from_queue : {e}"))?;

        let remaining: Vec<i64> = {
            let mut stmt = tx
                .prepare("SELECT episode_id FROM queue ORDER BY position ASC")
                .map_err(|e| format!("Erreur lecture queue : {e}"))?;
            let rows: Vec<i64> = stmt
                .query_map([], |r| r.get::<_, i64>(0))
                .map_err(|e| format!("Erreur map queue : {e}"))?
                .flatten()
                .filter(|id| !episode_ids.contains(id))
                .collect();
            rows
        };

        tx.execute("DELETE FROM queue", [])
            .map_err(|e| format!("Erreur clear queue : {e}"))?;
        for (pos, eid) in remaining.into_iter().enumerate() {
            tx.execute(
                "INSERT INTO queue (position, episode_id) VALUES (?1, ?2)",
                params![pos as i64, eid],
            )
            .map_err(|e| format!("Erreur insert queue : {e}"))?;
        }
        tx.commit()
            .map_err(|e| format!("Erreur commit remove_from_queue : {e}"))?;
        Ok(())
    }

    /// Réordonne un épisode dans la file (utilisé par le Drag & Drop précis de l'interface).
    pub fn reorder_queue(&self, episode_id: i64, new_index: usize) -> Result<(), String> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Erreur transaction reorder_queue : {e}"))?;

        let mut ids: Vec<i64> = {
            let mut stmt = tx
                .prepare("SELECT episode_id FROM queue ORDER BY position ASC")
                .map_err(|e| format!("Erreur lecture queue : {e}"))?;
            let rows: Vec<i64> = stmt
                .query_map([], |r| r.get::<_, i64>(0))
                .map_err(|e| format!("Erreur map queue : {e}"))?
                .flatten()
                .collect();
            rows
        };

        let Some(old_pos) = ids.iter().position(|&id| id == episode_id) else {
            return Err("Épisode absent de la file d'attente".to_string());
        };
        ids.remove(old_pos);
        let bounded = new_index.min(ids.len());
        ids.insert(bounded, episode_id);

        tx.execute("DELETE FROM queue", [])
            .map_err(|e| format!("Erreur clear queue : {e}"))?;
        for (pos, eid) in ids.into_iter().enumerate() {
            tx.execute(
                "INSERT INTO queue (position, episode_id) VALUES (?1, ?2)",
                params![pos as i64, eid],
            )
            .map_err(|e| format!("Erreur réinsertion queue : {e}"))?;
        }

        tx.commit()
            .map_err(|e| format!("Erreur commit reorder_queue : {e}"))?;
        Ok(())
    }

    /// Retourne l'épisode suivant dans la file après `current_episode_id` (ou position 0).
    pub fn get_next_in_queue(&self, current_episode_id: Option<i64>) -> Result<Option<i64>, String> {
        let ids = self.get_queue_ids()?;
        if ids.is_empty() {
            return Ok(None);
        }
        if let Some(curr) = current_episode_id {
            if let Some(pos) = ids.iter().position(|&id| id == curr) {
                return Ok(ids.get(pos + 1).copied());
            }
        }
        Ok(ids.first().copied())
    }

    /// Vide intégralement la file d'attente.
    pub fn clear_queue(&self) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM queue", [])
            .map_err(|e| format!("Erreur clear_queue : {e}"))?;
        Ok(())
    }
}

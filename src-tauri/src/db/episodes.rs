//! Opérations SQLite sur les épisodes (`episodes`), recherche plein texte FTS5,
//! marquage intelligent comme lu/non lu, favoris et santé du stockage.

use crate::db::Database;
use crate::models::{Episode, EpisodeQuery, FeedStorageUsage, StorageHealth};
use rusqlite::{params, types::Value, OptionalExtension};
use std::path::Path;

/// Entrée d'épisode parsée depuis RSS ou scannée depuis un dossier local.
#[derive(Debug, Clone)]
pub struct UpsertEpisodeInput {
    pub guid: String,
    pub title: String,
    pub pub_date: i64,
    pub description: Option<String>,
    pub audio_url: String,
    pub mime_type: Option<String>,
    pub local_file_path: Option<String>,
    pub file_size: i64,
    pub duration_ms: i64,
    pub transcript_url: Option<String>,
    pub transcript_type: Option<String>,
    pub transcript_data: Option<String>,
    /// Si true, l'épisode est automatiquement marqué comme lu (ex: filtré par exclude_keywords).
    pub auto_mark_played: bool,
    /// Si true, l'épisode nouvellement découvert arrive dans la Boîte de réception (Inbox).
    pub put_in_inbox: bool,
}

/// Résultat d'une opération de marquage comme lu/non lu (permet au contrôleur d'auto-supprimer les fichiers locaux).
#[derive(Debug, Clone, serde::Serialize)]
pub struct MarkPlayedResult {
    pub affected_ids: Vec<i64>,
    pub files_to_delete: Vec<String>,
}

const EPISODE_SELECT_COLS: &str = "
    e.id, e.feed_id, f.title AS feed_title,
    COALESCE(f.local_cover_path, f.image_url) AS feed_image_url,
    f.funding_url AS feed_funding_url,
    e.guid, e.title, e.pub_date, e.description, e.audio_url,
    e.mime_type, e.local_file_path, e.file_size, e.duration_ms,
    e.position_ms, e.is_played, e.is_favorite, e.inbox_state,
    e.played_at, e.transcript_url, e.transcript_type, e.transcript_data,
    q.position AS queue_position,
    EXISTS(SELECT 1 FROM chapters c WHERE c.episode_id = e.id) AS has_chapters
";

fn row_to_episode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Episode> {
    Ok(Episode {
        id: row.get(0)?,
        feed_id: row.get(1)?,
        feed_title: row.get(2)?,
        feed_image_url: row.get(3)?,
        feed_funding_url: row.get(4)?,
        guid: row.get(5)?,
        title: row.get(6)?,
        pub_date: row.get(7)?,
        description: row.get(8)?,
        audio_url: row.get(9)?,
        mime_type: row.get(10)?,
        local_file_path: row.get(11)?,
        file_size: row.get(12)?,
        duration_ms: row.get(13)?,
        position_ms: row.get(14)?,
        is_played: row.get::<_, i64>(15)? != 0,
        is_favorite: row.get::<_, i64>(16)? != 0,
        inbox_state: row.get(17)?,
        played_at: row.get(18)?,
        transcript_url: row.get(19)?,
        transcript_type: row.get(20)?,
        transcript_data: row.get(21)?,
        queue_position: row.get(22)?,
        has_chapters: row.get::<_, i64>(23)? != 0,
    })
}

/// Construit une requête FTS5 sûre à partir d'une saisie utilisateur libre (supporte les préfixes `mot*`).
pub fn build_fts5_query(raw: &str) -> Option<String> {
    let terms: Vec<String> = raw
        .split_whitespace()
        .map(|w| {
            // Supprime les caractères spéciaux FTS5 pour éviter toute erreur de syntaxe
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

impl Database {
    /// Récupère un épisode complet par son identifiant.
    pub fn get_episode_by_id(&self, episode_id: i64) -> Result<Option<Episode>, String> {
        let conn = self.lock()?;
        let sql = format!(
            "SELECT {EPISODE_SELECT_COLS}
             FROM episodes e
             INNER JOIN feeds f ON f.id = e.feed_id
             LEFT JOIN queue q ON q.episode_id = e.id
             WHERE e.id = ?1"
        );
        conn.query_row(&sql, [episode_id], row_to_episode)
            .optional()
            .map_err(|e| format!("Erreur get_episode_by_id : {e}"))
    }

    /// Requête unifiée pour toutes les vues de l'application avec recherche FTS5,
    /// filtres combinables et tris multi-colonnes (optimisée pour le Virtual Scrolling).
    pub fn query_episodes(&self, q: &EpisodeQuery) -> Result<Vec<Episode>, String> {
        let conn = self.lock()?;
        let mut joins = String::from(
            "FROM episodes e
             INNER JOIN feeds f ON f.id = e.feed_id
             LEFT JOIN queue q ON q.episode_id = e.id ",
        );
        let mut wheres: Vec<String> = Vec::new();
        let mut bind_values: Vec<Value> = Vec::new();

        // Filtre selon la vue active
        match q.view.as_str() {
            "queue" => wheres.push("q.position IS NOT NULL".to_string()),
            "inbox" => wheres.push("e.inbox_state = 1 AND e.is_played = 0".to_string()),
            "downloads" => {
                wheres.push("e.local_file_path IS NOT NULL AND e.local_file_path != ''".to_string())
            }
            "favorites" => wheres.push("e.is_favorite = 1".to_string()),
            "history" => wheres.push("e.played_at IS NOT NULL".to_string()),
            "feed" => {
                if let Some(fid) = q.feed_id {
                    wheres.push("e.feed_id = ?".to_string());
                    bind_values.push(Value::Integer(fid));
                }
            }
            "tag" => {
                if let Some(tid) = q.tag_id {
                    joins.push_str("INNER JOIN feed_tags ft ON ft.feed_id = e.feed_id ");
                    wheres.push("ft.tag_id = ?".to_string());
                    bind_values.push(Value::Integer(tid));
                }
            }
            _ => {} // "all"
        }

        // Filtre podcast ou tag optionnel additionnel (ex: Favoris filtrés par podcast/tag)
        if q.view != "feed" {
            if let Some(fid) = q.feed_id {
                wheres.push("e.feed_id = ?".to_string());
                bind_values.push(Value::Integer(fid));
            }
        }
        if q.view != "tag" {
            if let Some(tid) = q.tag_id {
                joins.push_str("INNER JOIN feed_tags ft2 ON ft2.feed_id = e.feed_id ");
                wheres.push("ft2.tag_id = ?".to_string());
                bind_values.push(Value::Integer(tid));
            }
        }

        // Recherche plein texte instantanée via FTS5 (titre + description + transcription)
        if let Some(ref raw_search) = q.search {
            if let Some(fts_query) = build_fts5_query(raw_search) {
                joins.push_str("INNER JOIN episodes_fts fts ON fts.rowid = e.id ");
                wheres.push("episodes_fts MATCH ?".to_string());
                bind_values.push(Value::Text(fts_query));
            }
        }

        // Filtres combinables
        if q.only_unplayed {
            wheres.push("e.is_played = 0".to_string());
        }
        if q.only_in_progress {
            wheres.push("e.is_played = 0 AND e.position_ms > 0".to_string());
        }
        if q.only_downloaded {
            wheres.push("e.local_file_path IS NOT NULL AND e.local_file_path != ''".to_string());
        }
        if q.only_favorites {
            wheres.push("e.is_favorite = 1".to_string());
        }
        if q.only_with_chapters {
            wheres.push("EXISTS(SELECT 1 FROM chapters c2 WHERE c2.episode_id = e.id)".to_string());
        }

        let where_clause = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };

        // Tri sécurisé (liste blanche de colonnes)
        let dir = if q.sort_asc { "ASC" } else { "DESC" };
        let order_clause = if q.view == "queue" && q.sort_by.is_none() {
            "ORDER BY q.position ASC".to_string()
        } else if q.view == "history" && q.sort_by.is_none() {
            "ORDER BY e.played_at DESC, e.id DESC".to_string()
        } else {
            match q.sort_by.as_deref().unwrap_or("pub_date") {
                "title" => format!("ORDER BY LOWER(e.title) {dir}"),
                "feed_title" => format!("ORDER BY LOWER(f.title) {dir}, e.pub_date DESC"),
                "duration_ms" => format!("ORDER BY e.duration_ms {dir}"),
                "file_size" => format!("ORDER BY e.file_size {dir}"),
                "played_at" => format!("ORDER BY COALESCE(e.played_at, 0) {dir}"),
                "queue_position" => format!("ORDER BY COALESCE(q.position, 9999999) {dir}"),
                _ => format!("ORDER BY e.pub_date {dir}, e.id {dir}"),
            }
        };

        let limit_clause = match (q.limit, q.offset) {
            (Some(lim), Some(off)) => format!("LIMIT {lim} OFFSET {off}"),
            (Some(lim), None) => format!("LIMIT {lim}"),
            _ => String::new(),
        };

        let sql = format!(
            "SELECT {EPISODE_SELECT_COLS} {joins} {where_clause} {order_clause} {limit_clause}"
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Erreur préparation query_episodes : {e}"))?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            bind_values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let items = stmt
            .query_map(params_refs.as_slice(), row_to_episode)
            .map_err(|e| format!("Erreur exécution query_episodes : {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Erreur lecture épisode : {e}"))?;

        Ok(items)
    }

    /// Insère ou met à jour un lot d'épisodes pour un abonnement donné.
    /// Retourne la liste des IDs nouvellement insérés (utile pour auto-queue / auto-download).
    pub fn upsert_episodes(
        &self,
        feed_id: i64,
        inputs: &[UpsertEpisodeInput],
    ) -> Result<Vec<i64>, String> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Erreur ouverture transaction upsert_episodes : {e}"))?;

        let mut new_ids = Vec::new();
        for item in inputs {
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM episodes WHERE feed_id = ?1 AND guid = ?2",
                    params![feed_id, item.guid],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| format!("Erreur lookup épisode guid : {e}"))?;

            if let Some(ep_id) = existing {
                tx.execute(
                    "
                    UPDATE episodes SET
                        title = ?1,
                        pub_date = CASE WHEN ?2 > 0 THEN ?2 ELSE pub_date END,
                        description = COALESCE(?3, description),
                        audio_url = CASE WHEN ?4 != '' THEN ?4 ELSE audio_url END,
                        mime_type = COALESCE(?5, mime_type),
                        file_size = CASE WHEN ?6 > 0 THEN ?6 ELSE file_size END,
                        duration_ms = CASE WHEN ?7 > 0 THEN ?7 ELSE duration_ms END,
                        transcript_url = COALESCE(?8, transcript_url),
                        transcript_type = COALESCE(?9, transcript_type),
                        transcript_data = COALESCE(?10, transcript_data),
                        local_file_path = COALESCE(?11, local_file_path)
                    WHERE id = ?12
                    ",
                    params![
                        item.title,
                        item.pub_date,
                        item.description,
                        item.audio_url,
                        item.mime_type,
                        item.file_size,
                        item.duration_ms,
                        item.transcript_url,
                        item.transcript_type,
                        item.transcript_data,
                        item.local_file_path,
                        ep_id,
                    ],
                )
                .map_err(|e| format!("Erreur update épisode : {e}"))?;
            } else {
                let inbox_state = if item.auto_mark_played {
                    0
                } else if item.put_in_inbox {
                    1
                } else {
                    0
                };
                tx.execute(
                    "
                    INSERT INTO episodes (
                        feed_id, guid, title, pub_date, description, audio_url,
                        mime_type, local_file_path, file_size, duration_ms,
                        is_played, inbox_state, transcript_url, transcript_type, transcript_data
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                    ",
                    params![
                        feed_id,
                        item.guid,
                        item.title,
                        item.pub_date,
                        item.description,
                        item.audio_url,
                        item.mime_type,
                        item.local_file_path,
                        item.file_size,
                        item.duration_ms,
                        if item.auto_mark_played { 1 } else { 0 },
                        inbox_state,
                        item.transcript_url,
                        item.transcript_type,
                        item.transcript_data,
                    ],
                )
                .map_err(|e| format!("Erreur insert épisode : {e}"))?;
                let id = tx.last_insert_rowid();
                if !item.auto_mark_played {
                    new_ids.push(id);
                }
            }
        }

        tx.commit()
            .map_err(|e| format!("Erreur commit upsert_episodes : {e}"))?;
        Ok(new_ids)
    }

    /// Marque intelligemment un ensemble d'épisodes comme Lu (`played = true`) ou Non lu (`played = false`).
    ///
    /// Lorsque `played = true` :
    /// - Retire automatiquement les épisodes de l'Inbox et de la File d'attente.
    /// - Met `played_at` à l'instant présent et positionne `position_ms = duration_ms`.
    /// - Enregistre immédiatement une action `PLAY` complète (`position == total`) dans `sync_queue`
    ///   pour qu'AntennaPod mobile marque l'épisode comme terminé.
    /// - Identifie les fichiers locaux éligibles à l'auto-suppression (tout en protégeant les Favoris
    ///   si `protect_favorites = true`).
    ///
    /// Lorsque `played = false` :
    /// - Réinitialise `is_played = 0` et `position_ms = 0`.
    /// - Enregistre une action `NEW` dans `sync_queue` pour AntennaPod mobile.
    pub fn mark_episodes_played(
        &self,
        episode_ids: &[i64],
        played: bool,
        emit_sync: bool,
        auto_delete_global: bool,
        protect_favorites: bool,
    ) -> Result<MarkPlayedResult, String> {
        if episode_ids.is_empty() {
            return Ok(MarkPlayedResult {
                affected_ids: Vec::new(),
                files_to_delete: Vec::new(),
            });
        }

        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Erreur transaction mark_episodes_played : {e}"))?;

        let now_ts = chrono::Utc::now().timestamp();
        let now_iso = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let mut affected_ids = Vec::new();
        let mut files_to_delete = Vec::new();

        for &ep_id in episode_ids {
            let meta: Option<(String, String, String, i64, bool, bool, Option<String>, bool, bool)> = tx
                .query_row(
                    "
                    SELECT e.audio_url, f.feed_url, e.guid, e.duration_ms,
                           e.is_favorite, f.auto_delete, e.local_file_path, f.is_local_folder,
                           e.is_played
                    FROM episodes e
                    INNER JOIN feeds f ON f.id = e.feed_id
                    WHERE e.id = ?1
                    ",
                    [ep_id],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get::<_, i64>(4)? != 0,
                            r.get::<_, i64>(5)? != 0,
                            r.get(6)?,
                            r.get::<_, i64>(7)? != 0,
                            r.get::<_, i64>(8)? != 0,
                        ))
                    },
                )
                .optional()
                .map_err(|e| format!("Erreur lecture métadonnées épisode {ep_id} : {e}"))?;

            let Some((
                audio_url,
                feed_url,
                guid,
                duration_ms,
                is_favorite,
                feed_auto_delete,
                local_file_path,
                is_local_folder,
                _was_played,
            )) = meta
            else {
                continue;
            };

            if played {
                tx.execute(
                    "UPDATE episodes
                     SET is_played = 1,
                         inbox_state = 0,
                         played_at = ?1,
                         position_ms = CASE WHEN duration_ms > 0 THEN duration_ms ELSE position_ms END
                     WHERE id = ?2",
                    params![now_ts, ep_id],
                )
                .map_err(|e| format!("Erreur mark played {ep_id} : {e}"))?;

                // Retire de la file d'attente
                tx.execute("DELETE FROM queue WHERE episode_id = ?1", [ep_id])
                    .map_err(|e| format!("Erreur retrait file {ep_id} : {e}"))?;

                // Émet une action PLAY à 100 % compatible AntennaPod (hors dossiers locaux virtuels)
                if emit_sync && !is_local_folder {
                    let total_sec = (duration_ms / 1000).max(1);
                    tx.execute(
                        "INSERT INTO sync_queue
                         (episode_url, feed_url, guid, action, timestamp, started_sec, position_sec, total_sec, synced)
                         VALUES (?1, ?2, ?3, 'PLAY', ?4, 0, ?5, ?5, 0)",
                        params![audio_url, feed_url, guid, now_iso, total_sec],
                    )
                    .map_err(|e| format!("Erreur insertion sync PLAY : {e}"))?;
                }

                // Auto-suppression sécurisée (jamais sur un dossier local virtuel, protection des Favoris)
                let should_delete = (auto_delete_global || feed_auto_delete)
                    && !is_local_folder
                    && !(protect_favorites && is_favorite);
                if should_delete {
                    if let Some(path) = local_file_path.filter(|p| !p.is_empty()) {
                        files_to_delete.push(path);
                        tx.execute(
                            "UPDATE episodes SET local_file_path = NULL WHERE id = ?1",
                            [ep_id],
                        )
                        .map_err(|e| format!("Erreur clear local_file_path : {e}"))?;
                    }
                }
            } else {
                tx.execute(
                    "UPDATE episodes SET is_played = 0, position_ms = 0 WHERE id = ?1",
                    [ep_id],
                )
                .map_err(|e| format!("Erreur mark unplayed {ep_id} : {e}"))?;

                if emit_sync && !is_local_folder {
                    tx.execute(
                        "INSERT INTO sync_queue
                         (episode_url, feed_url, guid, action, timestamp, synced)
                         VALUES (?1, ?2, ?3, 'NEW', ?4, 0)",
                        params![audio_url, feed_url, guid, now_iso],
                    )
                    .map_err(|e| format!("Erreur insertion sync NEW : {e}"))?;
                }
            }

            affected_ids.push(ep_id);
        }

        // Renumérote la file d'attente proprement après retraits
        if played {
            let mut stmt = tx
                .prepare("SELECT episode_id FROM queue ORDER BY position ASC")
                .map_err(|e| format!("Erreur renumérotation file : {e}"))?;
            let remaining: Vec<i64> = stmt
                .query_map([], |r| r.get(0))
                .map_err(|e| format!("Erreur lecture file : {e}"))?
                .flatten()
                .collect();
            drop(stmt);
            tx.execute("DELETE FROM queue", [])
                .map_err(|e| format!("Erreur clear queue temp : {e}"))?;
            for (idx, eid) in remaining.into_iter().enumerate() {
                tx.execute(
                    "INSERT INTO queue (position, episode_id) VALUES (?1, ?2)",
                    params![idx as i64, eid],
                )
                .map_err(|e| format!("Erreur réinsertion queue : {e}"))?;
            }
        }

        tx.commit()
            .map_err(|e| format!("Erreur commit mark_episodes_played : {e}"))?;

        Ok(MarkPlayedResult {
            affected_ids,
            files_to_delete,
        })
    }

    /// Récupère tous les IDs d'épisodes non lus d'un abonnement (pour "Tout marquer comme lu").
    pub fn get_unplayed_ids_for_feed(&self, feed_id: i64) -> Result<Vec<i64>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT id FROM episodes WHERE feed_id = ?1 AND is_played = 0")
            .map_err(|e| format!("Erreur get_unplayed_ids_for_feed : {e}"))?;
        let ids = stmt
            .query_map([feed_id], |r| r.get(0))
            .map_err(|e| format!("Erreur query unplayed feed : {e}"))?
            .flatten()
            .collect();
        Ok(ids)
    }

    /// Récupère tous les IDs d'épisodes non lus d'un tag/dossier (pour "Tout marquer comme lu").
    pub fn get_unplayed_ids_for_tag(&self, tag_id: i64) -> Result<Vec<i64>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT e.id
                 FROM episodes e
                 INNER JOIN feed_tags ft ON ft.feed_id = e.feed_id
                 WHERE ft.tag_id = ?1 AND e.is_played = 0",
            )
            .map_err(|e| format!("Erreur get_unplayed_ids_for_tag : {e}"))?;
        let ids = stmt
            .query_map([tag_id], |r| r.get(0))
            .map_err(|e| format!("Erreur query unplayed tag : {e}"))?
            .flatten()
            .collect();
        Ok(ids)
    }

    /// Bascule l'état Favori d'un ou plusieurs épisodes.
    pub fn set_episodes_favorite(&self, episode_ids: &[i64], favorite: bool) -> Result<(), String> {
        let conn = self.lock()?;
        for &id in episode_ids {
            conn.execute(
                "UPDATE episodes SET is_favorite = ?1 WHERE id = ?2",
                params![if favorite { 1 } else { 0 }, id],
            )
            .map_err(|e| format!("Erreur set_episodes_favorite : {e}"))?;
        }
        Ok(())
    }

    /// Met à jour l'état Boîte de réception (0 = retiré, 1 = dans Inbox, 2 = ignoré).
    pub fn set_inbox_state(&self, episode_ids: &[i64], state: i64) -> Result<(), String> {
        let conn = self.lock()?;
        for &id in episode_ids {
            conn.execute(
                "UPDATE episodes SET inbox_state = ?1 WHERE id = ?2",
                params![state, id],
            )
            .map_err(|e| format!("Erreur set_inbox_state : {e}"))?;
        }
        Ok(())
    }

    /// Retire de la Boîte de réception tous les épisodes appartenant à un podcast donné.
    pub fn dismiss_feed_from_inbox(&self, feed_id: i64) -> Result<usize, String> {
        let conn = self.lock()?;
        let affected = conn
            .execute(
                "UPDATE episodes SET inbox_state = 2 WHERE feed_id = ?1 AND inbox_state = 1",
                [feed_id],
            )
            .map_err(|e| format!("Erreur dismiss_feed_from_inbox : {e}"))?;
        Ok(affected)
    }

    /// Vide intégralement la Boîte de réception (sans marquer les épisodes comme lus).
    pub fn dismiss_all_inbox(&self) -> Result<usize, String> {
        let conn = self.lock()?;
        let affected = conn
            .execute("UPDATE episodes SET inbox_state = 2 WHERE inbox_state = 1", [])
            .map_err(|e| format!("Erreur dismiss_all_inbox : {e}"))?;
        Ok(affected)
    }

    /// Met à jour la position exacte en ms (et la durée si découverte par le décodeur).
    pub fn update_playback_position(
        &self,
        episode_id: i64,
        position_ms: i64,
        duration_ms: Option<i64>,
    ) -> Result<(), String> {
        let conn = self.lock()?;
        let now = chrono::Utc::now().timestamp();
        if let Some(dur) = duration_ms.filter(|&d| d > 0) {
            conn.execute(
                "UPDATE episodes SET position_ms = ?1, duration_ms = ?2, played_at = ?3 WHERE id = ?4",
                params![position_ms, dur, now, episode_id],
            )
            .map_err(|e| format!("Erreur update_playback_position : {e}"))?;
        } else {
            conn.execute(
                "UPDATE episodes SET position_ms = ?1, played_at = ?2 WHERE id = ?3",
                params![position_ms, now, episode_id],
            )
            .map_err(|e| format!("Erreur update_playback_position : {e}"))?;
        }
        Ok(())
    }

    /// Enregistre le chemin local d'un fichier audio téléchargé (ou le supprime avec `None`).
    pub fn set_local_file_path(
        &self,
        episode_id: i64,
        path: Option<&str>,
        file_size: Option<i64>,
    ) -> Result<(), String> {
        let conn = self.lock()?;
        if let Some(sz) = file_size {
            conn.execute(
                "UPDATE episodes SET local_file_path = ?1, file_size = ?2 WHERE id = ?3",
                params![path, sz, episode_id],
            )
            .map_err(|e| format!("Erreur set_local_file_path : {e}"))?;
        } else {
            conn.execute(
                "UPDATE episodes SET local_file_path = ?1 WHERE id = ?2",
                params![path, episode_id],
            )
            .map_err(|e| format!("Erreur set_local_file_path : {e}"))?;
        }
        Ok(())
    }

    /// Met à jour le texte de transcription d'un épisode (indexé aussitôt par le trigger FTS5).
    pub fn set_transcript_data(&self, episode_id: i64, transcript_data: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE episodes SET transcript_data = ?1 WHERE id = ?2",
            params![transcript_data, episode_id],
        )
        .map_err(|e| format!("Erreur set_transcript_data : {e}"))?;
        Ok(())
    }

    /// Calcule la santé du stockage hors-ligne par podcast et globalement.
    pub fn get_storage_health(
        &self,
        dir_media: &Path,
        dir_covers: &Path,
        dir_runtime: &Path,
    ) -> Result<StorageHealth, String> {
        let conn = self.lock()?;
        let downloaded_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE local_file_path IS NOT NULL AND local_file_path != ''",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let played_downloaded_count: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM episodes e
                 INNER JOIN feeds f ON f.id = e.feed_id
                 WHERE e.local_file_path IS NOT NULL AND e.local_file_path != ''
                   AND e.is_played = 1 AND f.is_local_folder = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mut stmt = conn
            .prepare(
                "SELECT f.id, f.title, COALESCE(SUM(e.file_size), 0), COUNT(e.id)
                 FROM episodes e
                 INNER JOIN feeds f ON f.id = e.feed_id
                 WHERE e.local_file_path IS NOT NULL AND e.local_file_path != ''
                   AND f.is_local_folder = 0
                 GROUP BY f.id, f.title
                 ORDER BY SUM(e.file_size) DESC",
            )
            .map_err(|e| format!("Erreur storage health by_feed : {e}"))?;

        let by_feed = stmt
            .query_map([], |r| {
                Ok(FeedStorageUsage {
                    feed_id: r.get(0)?,
                    feed_title: r.get(1)?,
                    bytes: r.get::<_, i64>(2)?.max(0) as u64,
                    episode_count: r.get(3)?,
                })
            })
            .map_err(|e| format!("Erreur query by_feed : {e}"))?
            .flatten()
            .collect();

        let db_size_bytes = self
            .db_path()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(StorageHealth {
            total_media_bytes: crate::paths::PortablePaths::directory_size_bytes(dir_media),
            total_covers_bytes: crate::paths::PortablePaths::directory_size_bytes(dir_covers),
            total_runtime_bytes: crate::paths::PortablePaths::directory_size_bytes(dir_runtime),
            db_size_bytes,
            downloaded_count,
            played_downloaded_count,
            by_feed,
        })
    }

    /// Purge en 1 clic tous les fichiers locaux téléchargés d'épisodes déjà marqués comme lus
    /// (protège les favoris si demandé et ne touche jamais aux dossiers locaux virtuels).
    pub fn purge_played_downloads(&self, protect_favorites: bool) -> Result<(usize, u64), String> {
        let conn = self.lock()?;
        let sql = if protect_favorites {
            "SELECT e.id, e.local_file_path
             FROM episodes e
             INNER JOIN feeds f ON f.id = e.feed_id
             WHERE e.is_played = 1 AND e.is_favorite = 0 AND f.is_local_folder = 0
               AND e.local_file_path IS NOT NULL AND e.local_file_path != ''"
        } else {
            "SELECT e.id, e.local_file_path
             FROM episodes e
             INNER JOIN feeds f ON f.id = e.feed_id
             WHERE e.is_played = 1 AND f.is_local_folder = 0
               AND e.local_file_path IS NOT NULL AND e.local_file_path != ''"
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| format!("Erreur purge_played_downloads : {e}"))?;
        let targets: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| format!("Erreur query purge : {e}"))?
            .flatten()
            .collect();
        drop(stmt);

        let mut deleted_count = 0usize;
        let mut freed_bytes = 0u64;

        for (ep_id, path_str) in targets {
            let p = Path::new(&path_str);
            if p.exists() {
                if let Ok(meta) = std::fs::metadata(p) {
                    freed_bytes += meta.len();
                }
                let _ = std::fs::remove_file(p);
            }
            let _ = conn.execute(
                "UPDATE episodes SET local_file_path = NULL WHERE id = ?1",
                [ep_id],
            );
            deleted_count += 1;
        }

        Ok((deleted_count, freed_bytes))
    }

    /// Supprime les fichiers locaux téléchargés des épisodes présents dans la file d'attente
    /// (laisse les épisodes dans la file pour lecture streaming si besoin).
    pub fn purge_queue_downloads(&self, protect_favorites: bool) -> Result<(usize, u64), String> {
        self.purge_downloads_where(
            "e.id IN (SELECT episode_id FROM queue)",
            protect_favorites,
        )
    }

    /// Supprime les fichiers locaux téléchargés qui ne sont PAS dans la file d'attente.
    pub fn purge_non_queue_downloads(&self, protect_favorites: bool) -> Result<(usize, u64), String> {
        self.purge_downloads_where(
            "e.id NOT IN (SELECT episode_id FROM queue)",
            protect_favorites,
        )
    }

    fn purge_downloads_where(
        &self,
        extra_clause: &str,
        protect_favorites: bool,
    ) -> Result<(usize, u64), String> {
        let conn = self.lock()?;
        let fav_clause = if protect_favorites {
            "AND e.is_favorite = 0"
        } else {
            ""
        };
        let sql = format!(
            "SELECT e.id, e.local_file_path
             FROM episodes e
             INNER JOIN feeds f ON f.id = e.feed_id
             WHERE f.is_local_folder = 0
               AND e.local_file_path IS NOT NULL AND e.local_file_path != ''
               {fav_clause}
               AND ({extra_clause})"
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Erreur purge_downloads_where : {e}"))?;
        let targets: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| format!("Erreur query purge : {e}"))?
            .flatten()
            .collect();
        drop(stmt);

        let mut deleted_count = 0usize;
        let mut freed_bytes = 0u64;
        for (ep_id, path_str) in targets {
            let p = Path::new(&path_str);
            if p.exists() {
                if let Ok(meta) = std::fs::metadata(p) {
                    freed_bytes += meta.len();
                }
                let _ = std::fs::remove_file(p);
            }
            let _ = conn.execute(
                "UPDATE episodes SET local_file_path = NULL WHERE id = ?1",
                [ep_id],
            );
            deleted_count += 1;
        }
        Ok((deleted_count, freed_bytes))
    }
}

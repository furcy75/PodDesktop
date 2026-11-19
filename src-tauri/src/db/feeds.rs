//! Opérations SQLite sur les abonnements (`feeds`), tags (`tags`, `feed_tags`)
//! et dossiers locaux virtuels.

use crate::db::Database;
use crate::models::{Feed, Tag};
use rusqlite::{params, OptionalExtension};

/// Paramètres de mise à jour des préférences spécifiques d'un abonnement.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FeedSettingsUpdate {
    pub feed_id: i64,
    pub playback_speed: Option<f32>,
    pub auto_download: bool,
    pub auto_delete: bool,
    pub keep_updated: bool,
    pub max_kept_episodes: Option<i64>,
    pub include_keywords: Option<String>,
    pub exclude_keywords: Option<String>,
    pub username: Option<String>,
    /// Mot de passe chiffré (`Some("")` pour effacer, `None` pour ne pas modifier).
    pub password_enc: Option<String>,
}

/// Données nécessaires pour insérer ou mettre à jour un abonnement.
#[derive(Debug, Clone)]
pub struct UpsertFeedInput {
    pub title: String,
    pub feed_url: String,
    pub image_url: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub is_local_folder: bool,
    pub local_folder_path: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub funding_url: Option<String>,
    pub username: Option<String>,
    pub password_enc: Option<String>,
}

impl Database {
    /// Liste tous les abonnements avec leur compteur d'épisodes non lus et leurs tags associés.
    pub fn list_feeds(&self) -> Result<Vec<Feed>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "
                SELECT
                    f.id, f.title, f.feed_url, f.image_url, f.local_cover_path,
                    f.description, f.author, f.language, f.is_local_folder,
                    f.local_folder_path, f.last_update, f.etag, f.last_modified,
                    f.playback_speed, f.auto_download, f.auto_delete, f.keep_updated,
                    f.max_kept_episodes, f.include_keywords, f.exclude_keywords,
                    f.funding_url, f.username,
                    (CASE WHEN f.password_enc IS NOT NULL AND f.password_enc != '' THEN 1 ELSE 0 END) AS has_password,
                    (SELECT COUNT(*) FROM episodes e WHERE e.feed_id = f.id AND e.is_played = 0) AS unread_count
                FROM feeds f
                ORDER BY LOWER(f.title) ASC
                ",
            )
            .map_err(|e| format!("Erreur préparation list_feeds : {e}"))?;

        let mut feeds: Vec<Feed> = stmt
            .query_map([], |row| {
                Ok(Feed {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    feed_url: row.get(2)?,
                    image_url: row.get(3)?,
                    local_cover_path: row.get(4)?,
                    description: row.get(5)?,
                    author: row.get(6)?,
                    language: row.get(7)?,
                    is_local_folder: row.get::<_, i64>(8)? != 0,
                    local_folder_path: row.get(9)?,
                    last_update: row.get(10)?,
                    etag: row.get(11)?,
                    last_modified: row.get(12)?,
                    playback_speed: row.get(13)?,
                    auto_download: row.get::<_, i64>(14)? != 0,
                    auto_delete: row.get::<_, i64>(15)? != 0,
                    keep_updated: row.get::<_, i64>(16)? != 0,
                    max_kept_episodes: row.get(17)?,
                    include_keywords: row.get(18)?,
                    exclude_keywords: row.get(19)?,
                    funding_url: row.get(20)?,
                    username: row.get(21)?,
                    has_password: row.get::<_, i64>(22)? != 0,
                    unread_count: row.get(23)?,
                    tag_ids: Vec::new(),
                })
            })
            .map_err(|e| format!("Erreur exécution list_feeds : {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Erreur lecture ligne feed : {e}"))?;

        // Charge les associations tags -> feeds en une seule requête
        let mut tag_stmt = conn
            .prepare("SELECT feed_id, tag_id FROM feed_tags")
            .map_err(|e| format!("Erreur lecture feed_tags : {e}"))?;
        let pairs = tag_stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| format!("Erreur query feed_tags : {e}"))?;

        for pair in pairs.flatten() {
            if let Some(feed) = feeds.iter_mut().find(|f| f.id == pair.0) {
                feed.tag_ids.push(pair.1);
            }
        }

        Ok(feeds)
    }

    /// Récupère un abonnement par son identifiant.
    pub fn get_feed_by_id(&self, feed_id: i64) -> Result<Option<Feed>, String> {
        let all = self.list_feeds()?;
        Ok(all.into_iter().find(|f| f.id == feed_id))
    }

    /// Récupère les identifiants HTTP Basic (username, password_enc) d'un flux privé.
    pub fn get_feed_credentials(&self, feed_id: i64) -> Result<(Option<String>, Option<String>), String> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT username, password_enc FROM feeds WHERE id = ?1",
            [feed_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("Erreur lecture identifiants flux : {e}"))
        .map(|opt| opt.unwrap_or((None, None)))
    }

    /// Insère un nouvel abonnement ou met à jour ses métadonnées s'il existe déjà (`feed_url`).
    /// Retourne `(feed_id, is_new)`.
    pub fn upsert_feed(&self, input: &UpsertFeedInput) -> Result<(i64, bool), String> {
        let conn = self.lock()?;
        let now = chrono::Utc::now().timestamp();

        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM feeds WHERE feed_url = ?1",
                [&input.feed_url],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("Erreur recherche feed existant : {e}"))?;

        if let Some(id) = existing_id {
            conn.execute(
                "
                UPDATE feeds SET
                    title = CASE WHEN ?1 != '' THEN ?1 ELSE title END,
                    image_url = COALESCE(?2, image_url),
                    description = COALESCE(?3, description),
                    author = COALESCE(?4, author),
                    language = COALESCE(?5, language),
                    last_update = ?6,
                    etag = COALESCE(?7, etag),
                    last_modified = COALESCE(?8, last_modified),
                    funding_url = COALESCE(?9, funding_url)
                WHERE id = ?10
                ",
                params![
                    input.title,
                    input.image_url,
                    input.description,
                    input.author,
                    input.language,
                    now,
                    input.etag,
                    input.last_modified,
                    input.funding_url,
                    id,
                ],
            )
            .map_err(|e| format!("Erreur mise à jour abonnement : {e}"))?;
            Ok((id, false))
        } else {
            conn.execute(
                "
                INSERT INTO feeds (
                    title, feed_url, image_url, description, author, language,
                    is_local_folder, local_folder_path, last_update, etag, last_modified,
                    funding_url, username, password_enc
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                ",
                params![
                    input.title,
                    input.feed_url,
                    input.image_url,
                    input.description,
                    input.author,
                    input.language,
                    if input.is_local_folder { 1 } else { 0 },
                    input.local_folder_path,
                    now,
                    input.etag,
                    input.last_modified,
                    input.funding_url,
                    input.username,
                    input.password_enc,
                ],
            )
            .map_err(|e| format!("Erreur insertion abonnement : {e}"))?;
            Ok((conn.last_insert_rowid(), true))
        }
    }

    /// Met à jour les réglages spécifiques d'un abonnement (vitesse, filtres, rétention, auth).
    pub fn update_feed_settings(&self, upd: &FeedSettingsUpdate) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "
            UPDATE feeds SET
                playback_speed = ?1,
                auto_download = ?2,
                auto_delete = ?3,
                keep_updated = ?4,
                max_kept_episodes = ?5,
                include_keywords = ?6,
                exclude_keywords = ?7,
                username = ?8
            WHERE id = ?9
            ",
            params![
                upd.playback_speed,
                if upd.auto_download { 1 } else { 0 },
                if upd.auto_delete { 1 } else { 0 },
                if upd.keep_updated { 1 } else { 0 },
                upd.max_kept_episodes,
                upd.include_keywords,
                upd.exclude_keywords,
                upd.username,
                upd.feed_id,
            ],
        )
        .map_err(|e| format!("Erreur mise à jour paramètres flux : {e}"))?;

        if let Some(ref pwd) = upd.password_enc {
            conn.execute(
                "UPDATE feeds SET password_enc = ?1 WHERE id = ?2",
                params![if pwd.is_empty() { None } else { Some(pwd.as_str()) }, upd.feed_id],
            )
            .map_err(|e| format!("Erreur mise à jour mot de passe flux : {e}"))?;
        }
        Ok(())
    }

    /// Met à jour le chemin local de la pochette mise en cache pour un flux.
    pub fn set_feed_local_cover(&self, feed_id: i64, local_path: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE feeds SET local_cover_path = ?1 WHERE id = ?2",
            params![local_path, feed_id],
        )
        .map_err(|e| format!("Erreur sauvegarde pochette locale : {e}"))?;
        Ok(())
    }

    /// Supprime un abonnement (et en cascade ses épisodes/chapitres/file). Retourne son URL pour la synchro.
    pub fn delete_feed(&self, feed_id: i64) -> Result<Option<String>, String> {
        let conn = self.lock()?;
        let feed_url: Option<String> = conn
            .query_row(
                "SELECT feed_url FROM feeds WHERE id = ?1 AND is_local_folder = 0",
                [feed_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("Erreur lecture feed_url avant suppression : {e}"))?;

        conn.execute("DELETE FROM feeds WHERE id = ?1", [feed_id])
            .map_err(|e| format!("Erreur suppression abonnement : {e}"))?;
        Ok(feed_url)
    }

    /// Supprime intégralement le catalogue distant local (`feeds` non locaux, épisodes, file,
    /// historique de synchro en attente) tout en préservant les réglages (`settings`), les tags
    /// et les dossiers locaux virtuels (`is_local_folder = 1`), puis remet `sync_last_timestamp` à 0.
    /// Retourne `(nb_abonnements_supprimes, chemins_fichiers_locaux_a_effacer)`.
    pub fn wipe_remote_catalog_for_resync(&self) -> Result<(usize, Vec<String>), String> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Erreur transaction wipe_remote_catalog_for_resync : {e}"))?;

        let files_to_delete: Vec<String> = {
            let mut stmt = tx
                .prepare(
                    "SELECT e.local_file_path
                     FROM episodes e
                     INNER JOIN feeds f ON f.id = e.feed_id
                     WHERE f.is_local_folder = 0
                       AND e.local_file_path IS NOT NULL
                       AND e.local_file_path != ''",
                )
                .map_err(|e| format!("Erreur lecture fichiers locaux avant wipe : {e}"))?;
            let rows: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| format!("Erreur map fichiers locaux : {e}"))?
                .flatten()
                .collect();
            rows
        };

        // Supprime les flux RSS distants (cascade sur episodes, chapters, bookmarks, queue, feed_tags)
        let deleted_feeds = tx
            .execute("DELETE FROM feeds WHERE is_local_folder = 0", [])
            .map_err(|e| format!("Erreur suppression flux distants : {e}"))?;

        // Vide totalement sync_queue pour éviter de renvoyer un état obsolète vers le serveur
        tx.execute("DELETE FROM sync_queue", [])
            .map_err(|e| format!("Erreur vidage sync_queue : {e}"))?;

        // Remet le curseur de synchro à 0
        tx.execute(
            "INSERT INTO settings (key, value) VALUES ('sync_last_timestamp', '0')
             ON CONFLICT(key) DO UPDATE SET value = '0'",
            [],
        )
        .map_err(|e| format!("Erreur reset sync_last_timestamp : {e}"))?;

        tx.commit()
            .map_err(|e| format!("Erreur commit wipe_remote_catalog_for_resync : {e}"))?;

        Ok((deleted_feeds, files_to_delete))
    }

    // -------------------------------------------------------------------------
    // Gestion des Tags / Dossiers personnalisés
    // -------------------------------------------------------------------------

    /// Liste tous les tags avec le nombre d'abonnements et d'épisodes non lus.
    pub fn list_tags(&self) -> Result<Vec<Tag>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "
                SELECT
                    t.id, t.name, t.color,
                    (SELECT COUNT(*) FROM feed_tags ft WHERE ft.tag_id = t.id) AS feed_count,
                    (SELECT COUNT(*)
                     FROM episodes e
                     INNER JOIN feed_tags ft ON ft.feed_id = e.feed_id
                     WHERE ft.tag_id = t.id AND e.is_played = 0) AS unread_count
                FROM tags t
                ORDER BY LOWER(t.name) ASC
                ",
            )
            .map_err(|e| format!("Erreur list_tags : {e}"))?;

        let tags = stmt
            .query_map([], |r| {
                Ok(Tag {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    color: r.get(2)?,
                    feed_count: r.get(3)?,
                    unread_count: r.get(4)?,
                })
            })
            .map_err(|e| format!("Erreur query list_tags : {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Erreur row tag : {e}"))?;
        Ok(tags)
    }

    /// Crée ou retourne l'ID d'un tag par son nom.
    pub fn upsert_tag(&self, name: &str, color: Option<&str>) -> Result<i64, String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Le nom du dossier/tag ne peut pas être vide".to_string());
        }
        let conn = self.lock()?;
        if let Some(id) = conn
            .query_row("SELECT id FROM tags WHERE LOWER(name) = LOWER(?1)", [trimmed], |r| r.get::<_, i64>(0))
            .optional()
            .map_err(|e| format!("Erreur recherche tag : {e}"))?
        {
            if let Some(c) = color {
                let _ = conn.execute("UPDATE tags SET color = ?1 WHERE id = ?2", params![c, id]);
            }
            return Ok(id);
        }

        let chosen_color = color.unwrap_or("#6366f1");
        conn.execute(
            "INSERT INTO tags (name, color) VALUES (?1, ?2)",
            params![trimmed, chosen_color],
        )
        .map_err(|e| format!("Erreur création tag : {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    /// Renomme ou change la couleur d'un tag existant.
    pub fn update_tag(&self, tag_id: i64, name: &str, color: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE tags SET name = ?1, color = ?2 WHERE id = ?3",
            params![name.trim(), color, tag_id],
        )
        .map_err(|e| format!("Erreur mise à jour tag : {e}"))?;
        Ok(())
    }

    /// Supprime un tag (ne supprime pas les abonnements associés).
    pub fn delete_tag(&self, tag_id: i64) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM tags WHERE id = ?1", [tag_id])
            .map_err(|e| format!("Erreur suppression tag : {e}"))?;
        Ok(())
    }

    /// Remplace la liste des tags assignés à un abonnement.
    pub fn set_feed_tags(&self, feed_id: i64, tag_ids: &[i64]) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM feed_tags WHERE feed_id = ?1", [feed_id])
            .map_err(|e| format!("Erreur nettoyage feed_tags : {e}"))?;
        for &tid in tag_ids {
            conn.execute(
                "INSERT OR IGNORE INTO feed_tags (feed_id, tag_id) VALUES (?1, ?2)",
                params![feed_id, tid],
            )
            .map_err(|e| format!("Erreur assignation tag : {e}"))?;
        }
        Ok(())
    }
}

/// Évalue les règles de filtrage d'inclusion/exclusion par mots-clés d'un flux.
/// Retourne `true` si l'épisode doit être conservé comme non lu/inbox, `false` s'il doit être ignoré/marqué lu.
pub fn matches_feed_keywords(
    title: &str,
    include_keywords: Option<&str>,
    exclude_keywords: Option<&str>,
) -> bool {
    let title_lower = title.to_lowercase();

    // Si des mots-clés d'exclusion sont définis, tout titre contenant l'un d'eux est exclu.
    if let Some(exc) = exclude_keywords {
        let tokens: Vec<String> = exc
            .split([',', ';', '\n'])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        if !tokens.is_empty() && tokens.iter().any(|kw| title_lower.contains(kw)) {
            return false;
        }
    }

    // Si des mots-clés d'inclusion sont définis, le titre doit contenir au moins l'un d'eux.
    if let Some(inc) = include_keywords {
        let tokens: Vec<String> = inc
            .split([',', ';', '\n'])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        if !tokens.is_empty() && !tokens.iter().any(|kw| title_lower.contains(kw)) {
            return false;
        }
    }

    true
}

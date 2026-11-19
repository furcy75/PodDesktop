//! Opérations SQLite sur les tables annexes :
//! `chapters`, `bookmarks`, `playback_stats`, `sync_queue` et `settings`.

use crate::db::Database;
use crate::models::{
    AppSettings, Bookmark, Chapter, FeedStat, ListeningStatsSummary, MonthlyStat, SyncQueueItem,
};
use crate::paths::PortablePaths;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

/// Entrée de chapitre à enregistrer pour un épisode.
#[derive(Debug, Clone)]
pub struct NewChapter {
    pub start_time_ms: i64,
    pub title: String,
    pub url: Option<String>,
    pub image_url: Option<String>,
}

/// Clés de `settings` contenant des secrets chiffrés (exclues par défaut des exports JSON).
const SECRET_SETTING_KEYS: &[&str] = &["sync_secret_enc", "podcast_index_secret_enc"];

impl Database {
    // -------------------------------------------------------------------------
    // Chapitres (`chapters`)
    // -------------------------------------------------------------------------

    /// Remplace les chapitres d'un épisode (ID3 / Vorbis / Podcasting 2.0).
    pub fn replace_chapters(&self, episode_id: i64, chapters: &[NewChapter]) -> Result<(), String> {
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Erreur transaction replace_chapters : {e}"))?;
        tx.execute("DELETE FROM chapters WHERE episode_id = ?1", [episode_id])
            .map_err(|e| format!("Erreur suppression anciens chapitres : {e}"))?;
        for ch in chapters {
            tx.execute(
                "INSERT INTO chapters (episode_id, start_time_ms, title, url, image_url)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![episode_id, ch.start_time_ms, ch.title, ch.url, ch.image_url],
            )
            .map_err(|e| format!("Erreur insertion chapitre : {e}"))?;
        }
        tx.commit()
            .map_err(|e| format!("Erreur commit replace_chapters : {e}"))?;
        Ok(())
    }

    /// Liste les chapitres d'un épisode triés chronologiquement.
    pub fn list_chapters(&self, episode_id: i64) -> Result<Vec<Chapter>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, episode_id, start_time_ms, title, url, image_url
                 FROM chapters
                 WHERE episode_id = ?1
                 ORDER BY start_time_ms ASC",
            )
            .map_err(|e| format!("Erreur list_chapters : {e}"))?;
        let items = stmt
            .query_map([episode_id], |r| {
                Ok(Chapter {
                    id: r.get(0)?,
                    episode_id: r.get(1)?,
                    start_time_ms: r.get(2)?,
                    title: r.get(3)?,
                    url: r.get(4)?,
                    image_url: r.get(5)?,
                })
            })
            .map_err(|e| format!("Erreur query chapters : {e}"))?
            .flatten()
            .collect();
        Ok(items)
    }

    // -------------------------------------------------------------------------
    // Signets personnels horodatés (`bookmarks`)
    // -------------------------------------------------------------------------

    /// Ajoute un signet personnel horodaté (raccourci clavier `B`).
    pub fn add_bookmark(
        &self,
        episode_id: i64,
        position_ms: i64,
        note: &str,
    ) -> Result<Bookmark, String> {
        let conn = self.lock()?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO bookmarks (episode_id, position_ms, note, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![episode_id, position_ms, note.trim(), now],
        )
        .map_err(|e| format!("Erreur add_bookmark : {e}"))?;
        let id = conn.last_insert_rowid();
        Ok(Bookmark {
            id,
            episode_id,
            position_ms,
            note: note.trim().to_string(),
            created_at: now,
        })
    }

    /// Liste les signets d'un épisode par ordre chronologique.
    pub fn list_bookmarks(&self, episode_id: i64) -> Result<Vec<Bookmark>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, episode_id, position_ms, note, created_at
                 FROM bookmarks
                 WHERE episode_id = ?1
                 ORDER BY position_ms ASC",
            )
            .map_err(|e| format!("Erreur list_bookmarks : {e}"))?;
        let items = stmt
            .query_map([episode_id], |r| {
                Ok(Bookmark {
                    id: r.get(0)?,
                    episode_id: r.get(1)?,
                    position_ms: r.get(2)?,
                    note: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })
            .map_err(|e| format!("Erreur query bookmarks : {e}"))?
            .flatten()
            .collect();
        Ok(items)
    }

    /// Supprime un signet personnel.
    pub fn delete_bookmark(&self, bookmark_id: i64) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM bookmarks WHERE id = ?1", [bookmark_id])
            .map_err(|e| format!("Erreur delete_bookmark : {e}"))?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Statistiques d'écoute (`playback_stats`)
    // -------------------------------------------------------------------------

    /// Enregistre un incrément de temps d'écoute (appelé périodiquement pendant la lecture active).
    /// `listened_ms` = temps réel passé à écouter, `saved_ms` = temps économisé grâce à la vitesse > 1.0x.
    pub fn record_listening_delta(
        &self,
        episode_id: i64,
        feed_id: i64,
        listened_ms: i64,
        saved_ms: i64,
    ) -> Result<(), String> {
        if listened_ms <= 0 {
            return Ok(());
        }
        let conn = self.lock()?;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // Agrège dans la ligne existante du jour pour éviter l'explosion du nombre de lignes
        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM playback_stats WHERE episode_id = ?1 AND date_day = ?2",
                params![episode_id, today],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("Erreur lookup playback_stats : {e}"))?;

        if let Some(id) = existing_id {
            conn.execute(
                "UPDATE playback_stats
                 SET listened_ms = listened_ms + ?1, saved_ms = saved_ms + ?2
                 WHERE id = ?3",
                params![listened_ms, saved_ms.max(0), id],
            )
            .map_err(|e| format!("Erreur update playback_stats : {e}"))?;
        } else {
            conn.execute(
                "INSERT INTO playback_stats (episode_id, feed_id, date_day, listened_ms, saved_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![episode_id, feed_id, today, listened_ms, saved_ms.max(0)],
            )
            .map_err(|e| format!("Erreur insert playback_stats : {e}"))?;
        }
        Ok(())
    }

    /// Calcule le résumé complet pour la vue Statistiques (global, par mois, par podcast, temps économisé).
    pub fn get_listening_stats(&self) -> Result<ListeningStatsSummary, String> {
        let conn = self.lock()?;
        let (total_listened_ms, total_saved_ms): (i64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(listened_ms), 0), COALESCE(SUM(saved_ms), 0) FROM playback_stats",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0));

        let completed_episodes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE is_played = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mut month_stmt = conn
            .prepare(
                "SELECT SUBSTR(date_day, 1, 7) AS month, SUM(listened_ms)
                 FROM playback_stats
                 GROUP BY month
                 ORDER BY month DESC
                 LIMIT 12",
            )
            .map_err(|e| format!("Erreur stats by_month : {e}"))?;
        let by_month = month_stmt
            .query_map([], |r| {
                Ok(MonthlyStat {
                    month: r.get(0)?,
                    listened_ms: r.get(1)?,
                })
            })
            .map_err(|e| format!("Erreur query by_month : {e}"))?
            .flatten()
            .collect();

        let mut feed_stmt = conn
            .prepare(
                "SELECT f.id, f.title, COALESCE(SUM(ps.listened_ms), 0) AS total_ms,
                        (SELECT COUNT(*) FROM episodes e WHERE e.feed_id = f.id AND e.is_played = 1) AS completed
                 FROM feeds f
                 LEFT JOIN playback_stats ps ON ps.feed_id = f.id
                 GROUP BY f.id, f.title
                 HAVING total_ms > 0 OR completed > 0
                 ORDER BY total_ms DESC, completed DESC
                 LIMIT 25",
            )
            .map_err(|e| format!("Erreur stats by_feed : {e}"))?;
        let by_feed = feed_stmt
            .query_map([], |r| {
                Ok(FeedStat {
                    feed_id: r.get(0)?,
                    feed_title: r.get(1)?,
                    listened_ms: r.get(2)?,
                    completed_count: r.get(3)?,
                })
            })
            .map_err(|e| format!("Erreur query by_feed : {e}"))?
            .flatten()
            .collect();

        Ok(ListeningStatsSummary {
            total_listened_ms,
            total_saved_ms,
            completed_episodes,
            by_month,
            by_feed,
        })
    }

    // -------------------------------------------------------------------------
    // File de synchronisation AntennaPod (`sync_queue`)
    // -------------------------------------------------------------------------

    /// Empile une action dans `sync_queue` (PLAY, NEW, DOWNLOAD, DELETE, SUBSCRIBE, UNSUBSCRIBE).
    pub fn enqueue_sync_action(
        &self,
        episode_url: &str,
        feed_url: &str,
        guid: Option<&str>,
        action: &str,
        started_sec: Option<i64>,
        position_sec: Option<i64>,
        total_sec: Option<i64>,
    ) -> Result<(), String> {
        let conn = self.lock()?;
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "INSERT INTO sync_queue
             (episode_url, feed_url, guid, action, timestamp, started_sec, position_sec, total_sec, synced)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            params![
                episode_url,
                feed_url,
                guid,
                action,
                ts,
                started_sec,
                position_sec,
                total_sec,
            ],
        )
        .map_err(|e| format!("Erreur enqueue_sync_action : {e}"))?;
        Ok(())
    }

    /// Récupère toutes les actions en attente de synchronisation (`synced = 0`).
    pub fn get_pending_sync_actions(&self) -> Result<Vec<SyncQueueItem>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, episode_url, feed_url, guid, action, timestamp,
                        started_sec, position_sec, total_sec, synced
                 FROM sync_queue
                 WHERE synced = 0
                 ORDER BY id ASC",
            )
            .map_err(|e| format!("Erreur get_pending_sync_actions : {e}"))?;
        let items = stmt
            .query_map([], |r| {
                Ok(SyncQueueItem {
                    id: r.get(0)?,
                    episode_url: r.get(1)?,
                    feed_url: r.get(2)?,
                    guid: r.get(3)?,
                    action: r.get(4)?,
                    timestamp: r.get(5)?,
                    started_sec: r.get(6)?,
                    position_sec: r.get(7)?,
                    total_sec: r.get(8)?,
                    synced: r.get::<_, i64>(9)? != 0,
                })
            })
            .map_err(|e| format!("Erreur query sync_queue : {e}"))?
            .flatten()
            .collect();
        Ok(items)
    }

    /// Marque les entrées `sync_queue` spécifiées comme synchronisées (`synced = 1`).
    pub fn mark_sync_actions_done(&self, ids: &[i64]) -> Result<(), String> {
        let conn = self.lock()?;
        for &id in ids {
            conn.execute("UPDATE sync_queue SET synced = 1 WHERE id = ?1", [id])
                .map_err(|e| format!("Erreur mark_sync_actions_done : {e}"))?;
        }
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Préférences globales (`settings`)
    // -------------------------------------------------------------------------

    /// Lit une clé brute dans la table `settings`.
    pub fn get_setting_raw(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.lock()?;
        conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(|e| format!("Erreur lecture setting {key} : {e}"))
    }

    /// Écrit une clé brute dans la table `settings`.
    pub fn set_setting_raw(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| format!("Erreur écriture setting {key} : {e}"))?;
        Ok(())
    }

    /// Charge l'ensemble structuré des préférences (`AppSettings`), complété par les chemins portables.
    pub fn load_app_settings(&self, paths: &PortablePaths) -> Result<AppSettings, String> {
        let mut s = AppSettings::default();

        // Hydrate les chemins actuels depuis PortablePaths
        let fmt = |p: &std::path::Path| {
            if paths.prefer_relative_paths {
                crate::paths::make_relative_if_possible(&paths.exe_dir, p)
            } else {
                p.to_string_lossy().to_string()
            }
        };
        s.dir_data = fmt(&paths.dir_data);
        s.dir_media = fmt(&paths.dir_media);
        s.dir_covers = fmt(&paths.dir_covers);
        s.dir_exports = fmt(&paths.dir_exports);
        s.prefer_relative_paths = paths.prefer_relative_paths;

        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM settings")
            .map_err(|e| format!("Erreur load_app_settings : {e}"))?;
        let pairs: HashMap<String, String> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("Erreur map settings : {e}"))?
            .flatten()
            .collect();

        let get_bool = |k: &str, def: bool| -> bool {
            pairs.get(k).map(|v| v == "true" || v == "1").unwrap_or(def)
        };
        let get_str = |k: &str, def: &str| -> String {
            pairs.get(k).cloned().unwrap_or_else(|| def.to_string())
        };

        s.language = get_str("language", &s.language);
        s.stream_without_downloading =
            get_bool("stream_without_downloading", s.stream_without_downloading);
        s.smart_rewind_enabled = get_bool("smart_rewind_enabled", s.smart_rewind_enabled);
        s.smart_rewind_sec = pairs
            .get("smart_rewind_sec")
            .and_then(|v| v.parse().ok())
            .unwrap_or(s.smart_rewind_sec);
        s.default_speed = pairs
            .get("default_speed")
            .and_then(|v| v.parse().ok())
            .unwrap_or(s.default_speed);
        s.skip_silence_enabled = get_bool("skip_silence_enabled", s.skip_silence_enabled);
        s.skip_silence_threshold_db = pairs
            .get("skip_silence_threshold_db")
            .and_then(|v| v.parse().ok())
            .unwrap_or(s.skip_silence_threshold_db);
        s.normalize_loudness = get_bool("normalize_loudness", s.normalize_loudness);
        s.voice_boost = get_bool("voice_boost", s.voice_boost);
        s.end_of_episode = get_str("end_of_episode", &s.end_of_episode);

        s.queue_insert_manual = get_str("queue_insert_manual", &s.queue_insert_manual);
        s.queue_insert_auto = get_str("queue_insert_auto", &s.queue_insert_auto);
        s.queue_locked = get_bool("queue_locked", s.queue_locked);
        s.auto_download_enabled = get_bool("auto_download_enabled", s.auto_download_enabled);
        s.auto_download_on_queue = get_bool("auto_download_on_queue", s.auto_download_on_queue);
        s.global_cache_limit_mb = pairs
            .get("global_cache_limit_mb")
            .and_then(|v| v.parse().ok())
            .unwrap_or(s.global_cache_limit_mb);
        s.default_max_kept_episodes = pairs
            .get("default_max_kept_episodes")
            .and_then(|v| v.parse().ok())
            .unwrap_or(s.default_max_kept_episodes);
        s.auto_delete_played = get_bool("auto_delete_played", s.auto_delete_played);
        s.protect_favorites_from_delete =
            get_bool("protect_favorites_from_delete", s.protect_favorites_from_delete);

        s.theme = get_str("theme", &s.theme);
        s.density = get_str("density", &s.density);
        s.minimize_to_tray = get_bool("minimize_to_tray", s.minimize_to_tray);
        s.close_to_tray = get_bool("close_to_tray", s.close_to_tray);
        s.taskbar_show_play_pause = get_bool("taskbar_show_play_pause", s.taskbar_show_play_pause);
        s.taskbar_show_next = get_bool("taskbar_show_next", s.taskbar_show_next);
        s.taskbar_show_mark_played =
            get_bool("taskbar_show_mark_played", s.taskbar_show_mark_played);
        if let Some(sc_json) = pairs.get("shortcuts") {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(sc_json) {
                for (k, v) in parsed {
                    s.shortcuts.insert(k, v);
                }
            }
        }

        s.sync_provider = get_str("sync_provider", &s.sync_provider);
        s.sync_server_url = get_str("sync_server_url", &s.sync_server_url);
        s.sync_username = get_str("sync_username", &s.sync_username);
        s.sync_has_secret = pairs
            .get("sync_secret_enc")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        s.sync_device_name = get_str("sync_device_name", &s.sync_device_name);
        s.sync_device_id = get_str("sync_device_id", &s.sync_device_id);
        s.sync_episode_actions = get_bool("sync_episode_actions", s.sync_episode_actions);
        s.sync_subscriptions = get_bool("sync_subscriptions", s.sync_subscriptions);
        s.sync_download_delete = get_bool("sync_download_delete", s.sync_download_delete);
        s.sync_interval_min = pairs
            .get("sync_interval_min")
            .and_then(|v| v.parse().ok())
            .unwrap_or(s.sync_interval_min);
        s.sync_last_timestamp = pairs
            .get("sync_last_timestamp")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        s.podcast_index_key = get_str("podcast_index_key", &s.podcast_index_key);
        s.podcast_index_has_secret = pairs
            .get("podcast_index_secret_enc")
            .map(|v| !v.is_empty())
            .unwrap_or(false);

        Ok(s)
    }

    /// Sauvegarde les préférences non secrètes dans SQLite (application instantanée).
    pub fn save_app_settings(&self, s: &AppSettings) -> Result<(), String> {
        let sc_json = serde_json::to_string(&s.shortcuts).unwrap_or_else(|_| "{}".into());
        let entries: Vec<(&str, String)> = vec![
            ("language", s.language.clone()),
            (
                "stream_without_downloading",
                s.stream_without_downloading.to_string(),
            ),
            ("smart_rewind_enabled", s.smart_rewind_enabled.to_string()),
            ("smart_rewind_sec", s.smart_rewind_sec.to_string()),
            ("default_speed", format!("{:.2}", s.default_speed)),
            ("skip_silence_enabled", s.skip_silence_enabled.to_string()),
            (
                "skip_silence_threshold_db",
                format!("{:.1}", s.skip_silence_threshold_db),
            ),
            ("normalize_loudness", s.normalize_loudness.to_string()),
            ("voice_boost", s.voice_boost.to_string()),
            ("end_of_episode", s.end_of_episode.clone()),
            ("queue_insert_manual", s.queue_insert_manual.clone()),
            ("queue_insert_auto", s.queue_insert_auto.clone()),
            ("queue_locked", s.queue_locked.to_string()),
            ("auto_download_enabled", s.auto_download_enabled.to_string()),
            ("auto_download_on_queue", s.auto_download_on_queue.to_string()),
            ("global_cache_limit_mb", s.global_cache_limit_mb.to_string()),
            (
                "default_max_kept_episodes",
                s.default_max_kept_episodes.to_string(),
            ),
            ("auto_delete_played", s.auto_delete_played.to_string()),
            (
                "protect_favorites_from_delete",
                s.protect_favorites_from_delete.to_string(),
            ),
            ("theme", s.theme.clone()),
            ("density", s.density.clone()),
            ("minimize_to_tray", s.minimize_to_tray.to_string()),
            ("close_to_tray", s.close_to_tray.to_string()),
            (
                "taskbar_show_play_pause",
                s.taskbar_show_play_pause.to_string(),
            ),
            ("taskbar_show_next", s.taskbar_show_next.to_string()),
            (
                "taskbar_show_mark_played",
                s.taskbar_show_mark_played.to_string(),
            ),
            ("shortcuts", sc_json),
            ("sync_provider", s.sync_provider.clone()),
            ("sync_server_url", s.sync_server_url.clone()),
            ("sync_username", s.sync_username.clone()),
            ("sync_device_name", s.sync_device_name.clone()),
            ("sync_device_id", s.sync_device_id.clone()),
            ("sync_episode_actions", s.sync_episode_actions.to_string()),
            ("sync_subscriptions", s.sync_subscriptions.to_string()),
            ("sync_download_delete", s.sync_download_delete.to_string()),
            ("sync_interval_min", s.sync_interval_min.to_string()),
            ("podcast_index_key", s.podcast_index_key.clone()),
        ];

        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Erreur transaction save_app_settings : {e}"))?;
        for (k, v) in entries {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![k, v],
            )
            .map_err(|e| format!("Erreur save setting {k} : {e}"))?;
        }
        tx.commit()
            .map_err(|e| format!("Erreur commit save_app_settings : {e}"))?;
        Ok(())
    }

    /// Exporte toutes les préférences en JSON, en excluant par défaut tous les secrets chiffrés.
    pub fn export_settings_json(&self, include_secrets: bool) -> Result<String, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM settings ORDER BY key ASC")
            .map_err(|e| format!("Erreur export_settings_json : {e}"))?;
        let mut map = serde_json::Map::new();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("Erreur map export settings : {e}"))?;

        for row in rows.flatten() {
            if !include_secrets && SECRET_SETTING_KEYS.contains(&row.0.as_str()) {
                continue;
            }
            map.insert(row.0, serde_json::Value::String(row.1));
        }

        serde_json::to_string_pretty(&serde_json::Value::Object(map))
            .map_err(|e| format!("Erreur sérialisation JSON préférences : {e}"))
    }

    /// Importe des préférences depuis un JSON (ignore les clés secrètes pour la sécurité).
    pub fn import_settings_json(&self, json: &str) -> Result<usize, String> {
        let parsed: HashMap<String, String> = serde_json::from_str(json)
            .map_err(|e| format!("Fichier JSON de préférences invalide : {e}"))?;
        let mut count = 0usize;
        for (k, v) in parsed {
            if SECRET_SETTING_KEYS.contains(&k.as_str()) {
                continue;
            }
            self.set_setting_raw(&k, &v)?;
            count += 1;
        }
        Ok(count)
    }
}

//! Couche d'accès à la base de données SQLite (`poddesktop.db`).
//!
//! Utilise `rusqlite` compilé statiquement en mode WAL (`PRAGMA journal_mode=WAL;`)
//! avec l'extension FTS5 active pour garantir une recherche plein texte instantanée
//! même au-delà de 50 000 épisodes.

pub mod episodes;
pub mod extras;
pub mod feeds;
pub mod queue;

#[cfg(test)]
mod tests;

use crate::models::SidebarCounts;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Gestionnaire thread-safe de la connexion SQLite locale.
pub struct Database {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl Database {
    /// Ouvre (ou crée) la base SQLite portable au chemin spécifié en mode WAL.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Impossible de créer le dossier DB : {e}"))?;
        }

        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(|e| format!("Erreur ouverture SQLite {:?} : {e}", path))?;

        let db = Self {
            conn: Mutex::new(conn),
            path: Some(path.to_path_buf()),
        };
        db.configure_and_migrate()?;
        Ok(db)
    }

    /// Crée une base SQLite en mémoire (utilisé par la suite de tests unitaires).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Erreur SQLite en mémoire : {e}"))?;
        let db = Self {
            conn: Mutex::new(conn),
            path: None,
        };
        db.configure_and_migrate()?;
        Ok(db)
    }

    /// Applique les PRAGMA de performance/intégrité et exécute le schéma idempotent.
    fn configure_and_migrate(&self) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA temp_store = MEMORY;
            PRAGMA cache_size = -16000;
            ",
        )
        .map_err(|e| format!("Erreur configuration PRAGMA SQLite : {e}"))?;

        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| format!("Erreur initialisation schéma SQLite : {e}"))?;

        Ok(())
    }

    /// Verrouille la connexion SQLite de façon sécurisée.
    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.conn
            .lock()
            .map_err(|_| "Verrou SQLite corrompu (poisoned mutex)".to_string())
    }

    /// Retourne le chemin sur disque de la base actuelle (si fichier).
    pub fn db_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Compacte la base (`VACUUM`) et reconstruit intégralement l'index plein texte FTS5.
    pub fn vacuum_and_rebuild_fts(&self) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute_batch(
            "
            INSERT INTO episodes_fts(episodes_fts) VALUES('rebuild');
            PRAGMA wal_checkpoint(TRUNCATE);
            VACUUM;
            ",
        )
        .map_err(|e| format!("Erreur lors de la maintenance SQLite : {e}"))?;
        Ok(())
    }

    /// Crée une sauvegarde cohérente à chaud de `poddesktop.db` vers `dest`.
    pub fn backup_to(&self, dest: &Path) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Impossible de créer le dossier de sauvegarde : {e}"))?;
        }
        let conn = self.lock()?;
        // Flush du journal WAL avant copie via l'API native SQLite VACUUM INTO
        let dest_str = dest
            .to_str()
            .ok_or_else(|| "Chemin de destination invalide".to_string())?;
        if dest.exists() {
            let _ = std::fs::remove_file(dest);
        }
        conn.execute("VACUUM INTO ?1", [dest_str])
            .map_err(|e| format!("Échec sauvegarde SQLite : {e}"))?;
        Ok(())
    }

    /// Restaure une sauvegarde SQLite en vérifiant d'abord son intégrité, puis recharge la connexion.
    pub fn restore_from(&self, src: &Path) -> Result<(), String> {
        let target = self
            .path
            .clone()
            .ok_or_else(|| "Restauration impossible sur une base en mémoire".to_string())?;
        if !src.exists() {
            return Err("Le fichier de sauvegarde spécifié est introuvable".to_string());
        }

        // 1. Validation de l'intégrité du fichier source avant d'écraser quoi que ce soit
        {
            let check_conn = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| format!("Fichier de sauvegarde SQLite invalide : {e}"))?;
            let integrity: String = check_conn
                .query_row("PRAGMA quick_check;", [], |r| r.get(0))
                .map_err(|e| format!("Vérification d'intégrité échouée : {e}"))?;
            if integrity.to_lowercase() != "ok" {
                return Err(format!("Base de sauvegarde corrompue : {integrity}"));
            }
        }

        // 2. Remplacement propre via une nouvelle connexion
        let mut guard = self.lock()?;
        let _ = guard.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        // Ferme temporairement en remplaçant par une connexion mémoire pour libérer les verrous Windows
        let mem_conn = Connection::open_in_memory()
            .map_err(|e| format!("Erreur interne restauration : {e}"))?;
        let old_conn = std::mem::replace(&mut *guard, mem_conn);
        drop(old_conn);

        // Supprime les anciens fichiers -wal / -shm
        let _ = std::fs::remove_file(format!("{}-wal", target.display()));
        let _ = std::fs::remove_file(format!("{}-shm", target.display()));
        std::fs::copy(src, &target)
            .map_err(|e| format!("Erreur copie fichier de sauvegarde : {e}"))?;

        // Réouverture de la base restaurée
        let reopened = Connection::open_with_flags(
            &target,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(|e| format!("Erreur réouverture base restaurée : {e}"))?;
        *guard = reopened;
        drop(guard);

        self.configure_and_migrate()?;
        Ok(())
    }

    /// Calcule les compteurs en temps réel affichés dans la barre latérale gauche.
    pub fn get_sidebar_counts(&self) -> Result<SidebarCounts, String> {
        let conn = self.lock()?;
        let queue: i64 = conn
            .query_row("SELECT COUNT(*) FROM queue", [], |r| r.get(0))
            .unwrap_or(0);
        let inbox: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE inbox_state = 1 AND is_played = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let all_unplayed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE is_played = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let downloads: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE local_file_path IS NOT NULL AND local_file_path != ''",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let favorites: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE is_favorite = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let history: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM episodes WHERE played_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        Ok(SidebarCounts {
            queue,
            inbox,
            all_unplayed,
            downloads,
            favorites,
            history,
        })
    }
}

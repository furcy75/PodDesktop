//! Chiffrement local AES-256-GCM des identifiants sensibles.
//!
//! Protège les tokens Nextcloud, mots de passe gpodder et identifiants HTTP Basic
//! des flux RSS privés stockés dans `poddesktop.db`.
//!
//! La clé symétrique de 256 bits est générée aléatoirement lors du premier démarrage
//! et stockée dans `<dir_data>/.keystore` (strictement portable avec le dossier data,
//! zéro dépendance au trousseau système ou registre Windows).

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::RngCore;
use std::fs;
use std::path::{Path, PathBuf};

const ENC_PREFIX: &str = "enc:v1:";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// Gestionnaire de chiffrement local AES-256-GCM portable.
#[derive(Clone)]
pub struct LocalCrypto {
    key: [u8; KEY_LEN],
}

impl LocalCrypto {
    /// Charge la clé locale depuis `<dir_data>/.keystore` ou en génère une nouvelle.
    pub fn load_or_create(dir_data: &Path) -> Result<Self, String> {
        let key_file: PathBuf = dir_data.join(".keystore");
        let key = if key_file.exists() {
            let raw = fs::read_to_string(&key_file)
                .map_err(|e| format!("Erreur lecture clé locale : {e}"))?;
            let decoded = B64
                .decode(raw.trim())
                .map_err(|e| format!("Clé locale corrompue : {e}"))?;
            if decoded.len() != KEY_LEN {
                return Err("Longueur de clé locale invalide".to_string());
            }
            let mut arr = [0u8; KEY_LEN];
            arr.copy_from_slice(&decoded);
            arr
        } else {
            let mut arr = [0u8; KEY_LEN];
            rand::thread_rng().fill_bytes(&mut arr);
            let encoded = B64.encode(arr);
            fs::write(&key_file, encoded)
                .map_err(|e| format!("Erreur création clé locale : {e}"))?;
            // Sur Unix, restreint les permissions à l'utilisateur courant (0600)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600));
            }
            arr
        };

        Ok(Self { key })
    }

    /// Constructeur direct pour les tests unitaires.
    #[cfg(test)]
    pub fn from_raw_key(key: [u8; KEY_LEN]) -> Self {
        Self { key }
    }

    /// Chiffre une chaîne UTF-8 en `enc:v1:<base64(nonce_12o || ciphertext)>`.
    /// Retourne une chaîne vide si l'entrée est vide.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, String> {
        if plaintext.is_empty() {
            return Ok(String::new());
        }
        // Évite un double chiffrement si déjà préfixé
        if plaintext.starts_with(ENC_PREFIX) {
            return Ok(plaintext.to_string());
        }

        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| format!("Initialisation AES-256-GCM impossible : {e}"))?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("Échec chiffrement AES-256-GCM : {e}"))?;

        let mut payload = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        payload.extend_from_slice(&nonce_bytes);
        payload.extend_from_slice(&ciphertext);

        Ok(format!("{ENC_PREFIX}{}", B64.encode(payload)))
    }

    /// Déchiffre une valeur chiffrée `enc:v1:...`.
    /// Si la chaîne n'est pas préfixée (ex: valeur héritée ou vide), elle est renvoyée telle quelle.
    pub fn decrypt(&self, stored: &str) -> Result<String, String> {
        if stored.is_empty() {
            return Ok(String::new());
        }
        let Some(encoded) = stored.strip_prefix(ENC_PREFIX) else {
            return Ok(stored.to_string());
        };

        let raw = B64
            .decode(encoded)
            .map_err(|e| format!("Données chiffrées base64 invalides : {e}"))?;
        if raw.len() <= NONCE_LEN {
            return Err("Payload AES-256-GCM tronqué".to_string());
        }

        let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| format!("Initialisation AES-256-GCM impossible : {e}"))?;
        let nonce = Nonce::from_slice(nonce_bytes);

        let plain_bytes = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| "Échec du déchiffrement (clé invalide ou données altérées)".to_string())?;

        String::from_utf8(plain_bytes)
            .map_err(|e| format!("Texte déchiffré UTF-8 invalide : {e}"))
    }
}

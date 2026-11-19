# PodDesktop — Cahier des Charges Exhaustif (v4.2 — Architecture Rust Audio, Sync Mobile AntennaPod & Ergonomie Desktop)

Client desktop natif, ultra-léger (**< 12 Mo**), instantané (**< 0,3 s**) et **100 % portable** (Windows & Linux), conçu comme compagnon PC d'AntennaPod avec une ergonomie pensée exclusivement pour le bureau (souris, clic droit, multi-sélection, raccourcis clavier, palette de commandes), un moteur audio natif Rust, une base SQLite locale FTS5, une barre de lecture intégrée à l'OS et une synchronisation bidirectionnelle 100 % compatible avec AntennaPod mobile.

---

## 1. Objectifs & Contraintes de Performance

* **Poids cible du binaire :** `< 12 Mo` (sans bundle lourd ni runtime embarqué).
* **Temps de démarrage :** `< 300 ms` à froid.
* **Empreinte mémoire (RAM) :** `< 60 Mo` en lecture active (garanti par le déport du décodage audio hors de la WebView).
* **Portabilité stricte (Zéro trace système) :** Aucune installation requise, aucune écriture dans `%APPDATA%`, `%LOCALAPPDATA%`, `~/.config`, `~/.cache` ou le registre Windows.
  * Tous les chemins sont résolus relativement au dossier de l'exécutable (`std::env::current_exe()`).
  * Dès l'initialisation de `main.rs` (avant le lancement de Tauri), les variables d'environnement `WEBVIEW2_USER_DATA_FOLDER` (Windows) et `XDG_CACHE_HOME` / `XDG_DATA_HOME` (Linux) sont explicitement forcées vers `./data/.runtime/` pour contenir les caches de la WebView.
* **Multiplateforme :** Binaire autonome Windows (`.exe` MSVC avec chargeur WebView2 lié statiquement) et Linux (ELF dynamique léger / AppImage).

---

## 2. Architecture Technique (Tauri v2 + Moteur Audio Rust + Frontend Vanilla)

### 2.1 Stack Minimale & Optimisée

* **Backend & Moteur Audio (Rust) :**
  * **Tauri v2 :** Utilise la WebView native du système (WebView2 sous Windows, WebKitGTK sous Linux).
  * **Moteur Audio Natif (`symphonia` + `rodio` + `cpal` + `ebur128`) :** Remplace totalement l'audio navigateur. Assure un décodage pur Rust (`< 5 Mo` de RAM), un positionnement exact à la milliseconde sur les fichiers MP3 VBR (sans décalage avec les chapitres/transcriptions), le maintien de la tonalité (`pitch-preserving time-stretch`, vitesse `0.50x` à `3.00x`), la suppression dynamique des silences (*Skip Silence*), la normalisation EBU R128, l'amplification vocale (*Voice Boost*) et le fondu de sortie (*fade-out*), sans aucune restriction CORS sur les fichiers locaux.
  * **`rusqlite` (bundled) :** SQLite compilé statiquement en mode WAL (`PRAGMA journal_mode=WAL;`) avec extension **FTS5** activée pour la recherche plein texte instantanée.
  * **`reqwest` (`rustls-tls`) + `tokio` :** Client HTTP asynchrone léger (flux RSS avec cache conditionnel `ETag` / `Last-Modified`, téléchargements parallèles avec reprise, protocoles gpodder / Nextcloud GPodder Sync, recherche d'annuaires Podcast Index / iTunes).
  * **`quick-xml` :** Parsing ultra-rapide RSS 2.0, Atom, iTunes, OPML et balises Podcasting 2.0 (chapitres JSON, transcriptions multiples, financement `<podcast:funding>`).
  * **`souvlaki` + `tray-icon` + API Win32/MPRIS2 :** Contrôles multimédias natifs OS (SMTC + boutons intégrés dans l'aperçu de la barre des tâches Windows, MPRIS2 D-Bus Linux) et icône System Tray.
* **Frontend (UI Desktop pure sans framework lourd) :**
  * **HTML5 + CSS3 (Grid/Flexbox) + TypeScript / Vanilla JS (ES2022) :** Piloté par les événements temps réel émis par le backend Rust (toutes les 250 ms pour la position de lecture).
  * **Virtual Scrolling natif :** Rendu dom-recyclé sur la liste centrale permettant d'afficher instantanément plus de 50 000 épisodes sans latence ni surcharge mémoire.

### 2.2 Optimisations de Compilation (`Cargo.toml`)

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

### 2.3 Arborescence Portable

* `/PodDesktop/PodDesktop.exe` (Windows) & `/PodDesktop/PodDesktop` ou `.AppImage` (Linux)
* `/PodDesktop/bootstrap.json` : Chemin relatif ou absolu vers la base SQLite (défaut : `./data/poddesktop.db`).
* `/PodDesktop/data/` : Dossier unique de données (déplaçable via Paramètres) contenant `poddesktop.db`, `covers/`, `media/`, `exports/` et `.runtime/` (cache WebView isolé).

---

## 3. Ergonomie 100 % Desktop & Contrôle Barre des Tâches

L'interface abandonne totalement les gestes mobiles au profit d'une ergonomie desktop dense, rapide et pilotable au clavier comme à la souris :

### 3.1 Disposition Multi-Panneaux & Modes d'Affichage

1. **Barre latérale gauche (Navigation) :** Accès direct en 1 clic à *File d'attente*, *Boîte de réception (Inbox)*, *Tous les épisodes*, *Téléchargements*, *Favoris*, *Historique*, *Découvrir (Recherche annuaire)*, *Statistiques*, et l'arborescence des *Tags / Dossiers d'abonnements*.
2. **Panneau central (Liste dense virtualisée) :** Affichage tabulaire / liste compacte avec colonnes triables (*Pochette, Titre, Podcast, Date, Durée, Taille, Statut*).
   * **En-tête contextuel d'abonnement :** Lorsqu'un podcast est sélectionné, affichage d'un bandeau supérieur dédié intégrant le bouton d'action rapide **"Tout marquer comme lu"** (avec confirmation si > 10 épisodes non lus), le bouton d'actualisation du flux et les réglages spécifiques du podcast.
3. **Panneau droit (Inspecteur / Détails) :** Affiche instantanément au clic simple les *Notes d'émission (Show Notes)*, les *Chapitres*, les *Signets personnels*, les *Transcriptions synchronisées*, le bouton de soutien créateur (`<podcast:funding>`) et les *Liens* de l'épisode sélectionné sans quitter la liste.
4. **Barre de lecture persistante en bas :** Toujours visible, avec barre de progression large cliquable au pixel près, curseur de volume à la molette, sélecteur de vitesse, bouton **Marquer comme lu**, bouton **Favori**, minuterie de sommeil et accès direct à la file d'attente.
5. **Mode Mini-Lecteur Flottant (Always-on-top) :** Bascule en 1 clic vers une barre compacte flottante (44 px de haut) ancrable en haut ou en bas de l'écran pour contrôler la lecture tout en travaillant dans d'autres logiciels.

### 3.2 Intégration Native dans la Barre des Tâches (Windows & Linux)

* **Barre des tâches Windows (Thumbnail Toolbar & SMTC) :**
  * Boutons d'action intégrés directement dans la miniature d'aperçu de la barre des tâches Windows : **Lecture / Pause**, **Épisode suivant**, **Marquer comme lu** (passant automatiquement à l'épisode suivant de la file).
  * Barre de progression intégrée directement sur l'icône de l'application dans la barre des tâches (`ITaskbarList3` : verte en lecture, jaune en pause, cyan pendant un téléchargement actif).
  * Panneau multimédia Windows (SMTC) complet (pochette, titre, podcast, barre de temps).
* **Barre système Linux (MPRIS2 D-Bus) :**
  * Contrôles complets (*Lecture/Pause, Suivant, Précédent, Seek, Pochette, Métadonnées*) intégrés nativement dans la barre supérieure / panneau système de GNOME, KDE Plasma, XFCE et Cinnamon.
* **System Tray & Touches Multimédias :** Réduction dans la zone de notification en arrière-plan, menu contextuel complet au clic droit sur l'icône tray (Lecture/Pause, Suivant, Marquer comme lu, Vitesse, Quitter) et support natif des touches média clavier/casque.

### 3.3 Interactions Souris & Raccourcis Clavier

* **Palette de Commandes (`Ctrl + K`) :** Accès instantané au clavier à n'importe quel podcast, épisode, réglage ou action (*"Tout marquer comme lu"*, *"Passer en 1.5x"*, *"Importer OPML"*).
* **Menu contextuel complet au clic droit (`contextmenu`) :** Sur chaque épisode, podcast ou élément de la file (*Lire maintenant, Lire ensuite, Ajouter en haut/bas de la file, Télécharger, Marquer lu / non lu, Tout marquer comme lu [sur un podcast/tag], Ajouter/Retirer des Favoris, Copier le lien avec horodatage, Supprimer le fichier local, Ouvrir le dossier sur le disque*).
* **Actions au survol (`:hover`) :** Boutons d'action rapide (*Play, +File, Télécharger, Marquer lu, Étoile Favori*) apparaissant au passage de la souris sur une ligne.
* **Sélection multiple (`Clic`, `Ctrl + Clic`, `Shift + Clic`, `Ctrl + A`) :** Barre d'actions groupées permettant de télécharger, marquer comme lu, mettre en favori ou ajouter à la file des dizaines d'épisodes simultanément.
* **Drag & Drop précis :** Réorganisation de la file d'attente à la souris avec poignée dédiée et ligne d'insertion visuelle.
* **Molette de la souris :** Ajuste finement le volume ou la vitesse de lecture au survol des contrôles.
* **Raccourcis Clavier :**
  * `Espace` : Lecture / Pause
  * `[` / `]` : Diminuer / Augmenter la vitesse de `0.05x`
  * `M` : Marquer l'épisode comme Lu / Non lu
  * `Shift + M` : Marquer tout le podcast affiché comme Lu
  * `B` : Poser un signet personnel horodaté avec note rapide
  * `F` : Basculer Favori
  * `Suppr` : Retirer de la file d'attente
  * `Ctrl + K` : Ouvrir la Palette de Commandes
  * `Ctrl + F` : Recherche instantanée (FTS5)
  * `F5` : Rafraîchir les flux

---

## 4. Base de Données SQLite (`poddesktop.db`)

| Table | Colonnes principales | Rôle |
| :--- | :--- | :--- |
| **`feeds`** | `id`, `title`, `feed_url`, `image_url`, `description`, `author`, `language`, `is_local_folder`, `local_folder_path`, `last_update`, `etag`, `last_modified`, `playback_speed`, `auto_download`, `auto_delete`, `keep_updated`, `max_kept_episodes`, `include_keywords`, `exclude_keywords`, `funding_url`, `username`, `password_enc` | Abonnements RSS (y compris flux privés et dossiers locaux virtuels), cache HTTP conditionnel, règles de filtrage/rétention et vitesse mémorisée par flux. |
| **`tags`** | `id`, `name`, `color` | Catégories / Dossiers personnalisés pour classer les abonnements. |
| **`feed_tags`** | `feed_id`, `tag_id` | Association N-N entre podcasts et tags. |
| **`episodes`** | `id`, `feed_id`, `guid`, `title`, `pub_date`, `description`, `audio_url`, `mime_type`, `local_file_path`, `file_size`, `duration_ms`, `position_ms`, `is_played`, `is_favorite`, `inbox_state`, `played_at`, `transcript_data` | Catalogue complet, position exacte en ms, état Lu/Favori/Inbox, date d'écoute et transcriptions indexées. |
| **`episodes_fts`** | `title`, `description`, `transcript_data` *(Table virtuelle FTS5)* | Indexation plein texte ultra-rapide synchronisée par triggers SQLite. |
| **`queue`** | `position`, `episode_id` | File d'attente ordonnée (`position = 0` = épisode en cours). |
| **`chapters`** | `id`, `episode_id`, `start_time_ms`, `title`, `url`, `image_url` | Chapitres ID3/Vorbis ou Podcasting 2.0. |
| **`bookmarks`** | `id`, `episode_id`, `position_ms`, `note`, `created_at` | Signets personnels horodatés ajoutés par l'utilisateur. |
| **`playback_stats`** | `id`, `episode_id`, `feed_id`, `date_day`, `listened_ms` | Historique détaillé du temps d'écoute par jour et par podcast. |
| **`sync_queue`** | `id`, `episode_url`, `feed_url`, `guid`, `action`, `timestamp`, `started_sec`, `position_sec`, `total_sec`, `synced` | Journal des actions en attente de synchronisation vers AntennaPod mobile. |
| **`settings`** | `key`, `value` | Préférences globales, stratégies d'insertion, chemins de dossiers et identifiants chiffrés. |

---

## 5. Spécifications Fonctionnelles

### 5.1 Toutes les Vues & Collections

1. **File d'attente (Queue) :** Gestion de la liste de lecture, verrouillage de l'ordre, durée totale restante calculée selon la vitesse, taille totale sur disque.
2. **Boîte de réception (Inbox) :** Nouveaux épisodes arrivés lors de l'actualisation des flux, en attente de décision (*Ajouter à la file, Télécharger, Marquer lu, Ignorer*).
3. **Favoris (Favorites) :** Vue dédiée regroupant tous les épisodes marqués d'une étoile (`is_favorite = 1`), filtrable par podcast ou tag, avec protection contre la suppression automatique du fichier audio téléchargé.
4. **Téléchargements & Santé du stockage (Downloads) :** Liste des épisodes stockés hors-ligne, espace disque utilisé par podcast, file d'attente des téléchargements en cours (avec barre de progression, pause/reprise) et outil de purge en 1 clic des fichiers locaux d'épisodes déjà lus.
5. **Tous les épisodes (Episodes) :** Catalogue global virtualisé avec filtres combinables (*Non lus, En cours, Téléchargés, Favoris, Avec chapitres*) et tris multiples (*Date, Durée, Titre, Podcast*).
6. **Historique de lecture (Playback History) :** Liste chronologique des épisodes écoutés (`played_at`) permettant de retrouver facilement un épisode terminé.
7. **Abonnements, Tags & Dossiers Locaux :** Grille ou liste des podcasts avec compteur d'épisodes non lus, bouton rapide **"Tout marquer comme lu"** par podcast ou par dossier/tag, support des flux RSS privés (HTTP Basic Auth) et création d'abonnements virtuels pointant vers un **dossier local** (livres audio, cours MP3/M4B).
8. **Découvrir (Recherche d'Annuaires & URL) :** Recherche intégrée via les API **Podcast Index** et **iTunes Search**, ajout par URL RSS directe (ou lien Apple Podcasts résolu automatiquement) et mode aperçu (*Preview*) avant abonnement.
9. **Statistiques d'écoute :** Temps total écouté (global, par mois, par podcast), nombre d'épisodes terminés et temps économisé grâce à la vitesse accélérée.

### 5.2 Gestion de la File & Automatisation

* **Stratégies d'insertion dans la file (Queue Insertion Rules) :**
  * Paramétrable séparément pour les **ajouts manuels** et les **ajouts automatiques (nouveaux épisodes)** :
    1. **En bas de la file** (à la suite de tous les épisodes en attente).
    2. **En haut de la file** (avant l'épisode en cours, devient immédiatement le prochain lu).
    3. **Après l'épisode en cours** (s'insère en position `1`, sans interrompre la lecture active).
    4. **Tri intelligent** (insertion automatique classée par date de publication croissante ou décroissante).
* **Marquage comme lu intelligent :**
  * Bouton **"Tout marquer comme lu"** sur chaque fiche podcast, dans le menu contextuel d'un tag/dossier et dans la barre d'actions groupées.
  * Marquer un épisode comme lu le retire automatiquement de la *Boîte de réception (Inbox)* et de la *File d'attente*, émet immédiatement un événement de synchronisation `PLAY` complet (`position = total`) pour AntennaPod mobile et déclenche le nettoyage automatique du fichier local si activé.
* **Téléchargement automatique & Limite de cache :** Télécharge automatiquement les nouveaux épisodes ajoutés à la file ou détectés dans les flux sélectionnés, avec limite par podcast (`max_kept_episodes`) et limite globale de cache hors-ligne.
* **Nettoyage automatique (Auto-Delete) :** Supprime le fichier audio local une fois l'épisode marqué comme lu/terminé (sauf si l'épisode est en Favori, option configurable).
* **Filtres d'inclusion/exclusion par flux :** Ignorer ou marquer automatiquement comme lus les épisodes dont le titre correspond à des mots-clés définis (`include_keywords` / `exclude_keywords`).

### 5.3 Lecteur Audio Natif & Podcasting 2.0

* **Lecture & Vitesse :** Barre de progression interactive précise à la milliseconde (y compris sur MP3 VBR), reprise automatique légère après pause (*Smart Rewind* de 3 s configurable), vitesse de `0.50x` à `3.00x` par paliers de `0.05x` (mémorisable par podcast), correction de tonalité (`pitch-preserving`), suppression dynamique des silences (*Skip Silence*), normalisation EBU R128 et amplification vocale (*Voice Boost*).
* **Comportement en fin d'épisode :** Choix entre *Passer à l'épisode suivant de la file*, *Mettre en pause à la fin* ou *Répéter l'épisode*.
* **Chapitrage, Signets & Show Notes :** Liste des chapitres avec miniature et saut direct au clic ; prise de signets personnels horodatés (`B`) ; notes d'émission HTML nettoyées avec timestamps cliquables (`12:34`), partage avec horodatage (`#t=`) et ouverture des liens dans le navigateur externe.
* **Transcriptions (Podcasting 2.0) :** Affichage, recherche textuelle instantanée et défilement synchronisé avec la lecture pour les formats SRT, VTT et JSON.
* **Minuterie de sommeil (Sleep Timer) :** Arrêt après *X* minutes ou à la fin de l'épisode en cours avec baisse progressive du volume (*fade-out* sur les 15 dernières secondes).

---

## 6. Synchronisation Bidirectionnelle PC ↔ Mobile (100 % Compatible AntennaPod)

Pour garantir que toute action effectuée sur **PodDesktop** (abonnement, désabonnement, progression de lecture, marquage comme lu/non lu, suppression) se répercute fidèlement sur **AntennaPod Android** (et inversement) :

### 6.1 Support Multi-Fournisseurs & Protocoles Natifs
* **Nextcloud GPodder Sync (`gpodder-sync` API) :** Implémentation native de l'authentification OAuth2 / Login Flow v2 de Nextcloud et des endpoints `/index.php/apps/gpoddersync/subscriptions` et `/episode_action` (serveur recommandé par le projet AntennaPod pour sa fiabilité).
* **Serveurs gpodder API v2 :** Compatible avec `gpodder.net` ainsi que les serveurs autonomes légers (`oPodSync`, `goPodder`, `Micro-GPodder`, `PodFetch`).

### 6.2 Règles Strictes de Compatibilité & Répercussion Mobile
1. **Normalisation stricte des URLs et identifiants :** Comparaison canonique des `feed_url` et `audio_url` (gestion des redirections HTTP 301/302 de régies publicitaires dynamiques type Megaphone/Acast/Podtrac en résolvant l'URL canonique d'origine du flux RSS) afin qu'AntennaPod mobile associe toujours l'action PC au bon épisode.
2. **Mapping exact des actions d'épisodes (`episode_action`) :**
   * *Écoute / Pause sur PC :* Envoi d'une action `PLAY` avec `started` (position de départ en sec), `position` (position actuelle en sec), `total` (durée totale en sec) et `timestamp` UTC ISO-8601 (`YYYY-MM-DDTHH:MM:SSZ`). Sur mobile, AntennaPod déplace automatiquement le curseur de lecture à cette seconde exacte.
   * *Bouton "Marquer comme lu" / "Tout marquer comme lu" sur PC :* Envoi d'une action `PLAY` où `position == total` (ou `started = 0, position = duration_sec, total = duration_sec`). AntennaPod mobile interprète ce ratio `100 %` comme un épisode terminé et le marque automatiquement comme **Lu** en le retirant de sa file/inbox.
   * *Marquer comme non lu sur PC :* Envoi d'une action `NEW` avec horodatage UTC courant pour réinitialiser l'état sur mobile.
   * *Téléchargement / Suppression locale sur PC :* Envoi optionnel des actions `DOWNLOAD` / `DELETE` (activable dans les paramètres de synchro pour éviter de déclencher des téléchargements non désirés sur forfait mobile).
3. **Déclencheurs intelligents (Push & Pull) :**
   * Synchronisation automatique au démarrage, à la mise en pause, au clic sur **"Marquer comme lu"**, lors d'un nouvel abonnement, et à la fermeture de l'application (flush de `sync_queue` avant extinction).
   * Résolution de conflits par horodatage UTC le plus récent (`last-write-wins`) et gestion de la liaison d'appareils (`device_sync` sous gpodder v2) pour fusionner l'historique PC et Mobile.

### 6.3 Sécurité & Import/Export
* **Protection des identifiants :** Chiffrement local AES-256-GCM des tokens Nextcloud, mots de passe gpodder et flux RSS privés dans `poddesktop.db` ; exclusion par défaut lors de l'export JSON.
* **Import / Export complet :** Import/Export OPML (abonnements + tags), export des Favoris (HTML/Markdown/JSON), et sauvegarde/restauration complète de `poddesktop.db`.

---

## 7. Paramétrages de l'Application

L'interface des paramètres adopte un **panneau unifié à défilement continu couplé à un menu latéral d'ancrage rapide (6 sections)** :

1. **Stockage & Dossiers :** Sélection graphique (*Parcourir...*) de `dir_data`, `dir_media`, `dir_covers`, `dir_exports`, bascule *Chemin relatif (portable)* / *Chemin absolu*, gestion des dossiers locaux virtuels, nettoyage du cache WebView et purge des fichiers lus.
2. **Lecture & Audio :** Activation du *Smart Rewind* après pause, vitesse de lecture par défaut, sensibilité de la suppression des silences (*Skip Silence*), normalisation EBU R128, amplification vocale (*Voice Boost*), comportement en fin d'épisode.
3. **File & Automatisation :** Stratégie d'insertion dans la file (ajouts manuels vs ajouts auto : *Haut, Bas, Après l'épisode en cours, Tri intelligent*), règles de téléchargement automatique, limite d'épisodes conservés par podcast et limite globale de cache, suppression auto après marquage comme lu (avec protection des Favoris).
4. **Interface, Barre des Tâches & Raccourcis :** Thème (*Clair / Sombre / Système*), densité d'affichage des listes, boutons affichés dans la miniature de barre des tâches Windows, personnalisation des raccourcis clavier, comportement de réduction dans le System Tray.
5. **Synchronisation Mobile (AntennaPod / Nextcloud / gpodder) :** Fournisseur (*Nextcloud GPodder Sync / gpodder.net / Serveur custom*), authentification OAuth2 ou identifiants chiffrés, nom de l'appareil PC, options de synchronisation (`PLAY`, `NEW`, `SUBSCRIPTIONS`, `DOWNLOAD/DELETE`), fréquence auto et bouton **Tester & Synchroniser maintenant**.
6. **Import / Export & Base de Données :** Import/Export OPML, export des Favoris, export/import des préférences JSON (sans secrets par défaut), sauvegarde/restauration et compactage (`VACUUM` + reconstruction index FTS5) de `poddesktop.db`.

*(Les réglages locaux d'interface, d'audio et d'automatisation s'appliquent et s'enregistrent instantanément ; seuls les paramètres réseau de synchronisation disposent d'un bouton de validation et de test).*

---

## 8. Développement & Compilation

* **Prérequis développement :** Rust (stable), Node.js (pour l'outillage de build Tauri uniquement), bibliothèques système WebKitGTK (Linux) ou WebView2 Runtime (Windows 10/11).
* **Commandes :**
  * `npm install` : Installation des dépendances CLI Tauri.
  * `npm run dev` : Lancement en mode développement avec rechargement rapide.
  * `npm run build:linux` : Compilation du binaire portable Linux (`dist/PodDesktop` & `dist/PodDesktop.AppImage`).
  * `npm run build:win` : Compilation du binaire portable Windows (`dist/PodDesktop.exe` MSVC avec chargeur WebView2 lié statiquement).

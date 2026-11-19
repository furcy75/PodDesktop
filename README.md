<div align="center">

# 🎙️ PodDesktop

**Client desktop natif, ultra-léger, 100 % portable et multilingue — Compagnon PC d'AntennaPod**

[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-0ea5e9?style=flat-square)](#installation--compilation)
[![Stack](https://img.shields.io/badge/Stack-Tauri%20v2%20%7C%20Rust%20%7C%20TypeScript-6366f1?style=flat-square)](#architecture)
[![Binary Size](https://img.shields.io/badge/Binary-~7.4%20Mo-10b981?style=flat-square)](#performances--empreinte)
[![Portable](https://img.shields.io/badge/Portable-Zero%20System%20Trace-f59e0b?style=flat-square)](#arborescence-portable)
[![Languages](https://img.shields.io/badge/i18n-FR%20%7C%20EN%20%7C%20DE%20%7C%20ES-8b5cf6?style=flat-square)](#internationalisation-i18n)

[Fonctionnalités](#-fonctionnalités-clés) • [Performances](#-performances--empreinte) • [Installation & Compilation](#-installation--compilation) • [Synchro AntennaPod](#-synchronisation-antennapod) • [Raccourcis](#-raccourcis-clavier) • [Cahier des charges](docs/CAHIER_DES_CHARGES.md)

</div>

---

## À propos

**PodDesktop** est un gestionnaire et lecteur de podcasts natif conçu exclusivement pour l'ergonomie de bureau (**Windows** et **Linux**) afin de prolonger votre expérience **AntennaPod Android** sur PC.

Construit sur **Tauri v2**, un **moteur audio pur Rust** (`symphonia` + `cpal` + DSP temps réel), **SQLite WAL + FTS5** et une interface **Vanilla TypeScript / CSS3** sans framework lourd, PodDesktop tient dans un exécutable autonome de **~7,4 Mo**, consomme **~45 Mo de RAM** et ne laisse **aucune trace** sur le système hôte.

---

## ✨ Fonctionnalités clés

### 🎧 Moteur Audio Natif Rust & Podcasting 2.0
* **Décodage pur Rust (`symphonia` + `cpal`) :** MP3 (CBR & VBR avec seek précis à la milliseconde), AAC / M4A / M4B, OGG / Vorbis, FLAC et WAV — sans balise `<audio>` navigateur ni restriction CORS.
* **Streaming réseau progressif sans écriture disque :** option activable/désactivable (*activée par défaut sur PC*) lisant par blocs HTTP `Range` de 256 Ko en mémoire (2 Mo max de RAM, démarrage instantané, seek précis, 0 octet stocké sur disque).
* **Traitement DSP temps réel :**
  * Vitesse de `0.50x` à `3.00x` par paliers de `0.05x` avec **conservation de tonalité (WSOLA)** et mémorisation par podcast.
  * **Skip Silence** dynamique (détection RMS dBFS configurable).
  * **Normalisation EBU R128** (`-16 LUFS` via `ebur128`) et **Voice Boost** (passe-haut 100 Hz + EQ présence vocale + limiteur doux).
  * **Smart Rewind** après pause et **Minuterie de sommeil** (X minutes ou fin d'épisode avec fondu progressif de 15 s).
* **Podcasting 2.0 :** chapitres (JSON / Podlove / embarqués), transcriptions synchronisées (SRT / VTT / JSON avec recherche plein texte), signets personnels horodatés (`B`), notes d'émission HTML nettoyées avec timestamps cliquables (`12:34`), partage `#t=` et soutien créateur (`<podcast:funding>`).

### 🖥️ Ergonomie 100 % Desktop & Intégration OS
* **Disposition 3 panneaux :** Barre latérale (9 vues + Tags/Dossiers + Abonnements), Liste dense **Virtual Scrolling** (> 50 000 épisodes sans latence) et Inspecteur droit instantané.
* **Barre de lecture accessible & Mode Mini-Lecteur 44 px :** contrôles dédiés Vitesse (curseurs + `−`/`+` + paliers + molette) et Volume (muet + curseur + `%` + molette), bascule en 1 clic vers une barre flottante *Always-on-top* ancrable en haut ou en bas de l'écran.
* **Productivité bureau :** Palette de commandes (`Ctrl+K`), recherche plein texte instantanée **SQLite FTS5** (`Ctrl+F`), clic droit contextuel complet, actions au survol, multi-sélection (`Clic`, `Ctrl+Clic`, `Shift+Clic`, `Ctrl+A`), Drag & Drop de la file d'attente et raccourcis personnalisables.
* **Intégration native Windows & Linux :**
  * **Windows :** `ITaskbarList3` (barre de progression verte/jaune/cyan sur l'icône + 3 boutons miniatures Lecture/Pause, Suivant, Marquer comme lu), panneau multimédia **SMTC** et **System Tray** multilingue.
  * **Linux :** contrôles natifs **MPRIS2 D-Bus** (GNOME, KDE Plasma, XFCE, Cinnamon) et **System Tray**.

### 🔄 Synchronisation Bidirectionnelle AntennaPod
* Compatible nativement avec **Nextcloud GPodder Sync** (avec **Login Flow v2** en 1 clic), **oPodSync**, **gpodder.net**, **goPodder**, **Micro-GPodder** et **PodFetch**.
* Découverte multi-appareils (`/api/2/devices/`), gestion des cookies de session `sessionid` + repli Basic Auth, normalisation canonique d'URL multi-régies (Podtrac, Chartable, OP3, Megaphone, Spotify...), rejeu chronologique complet (`DOWNLOAD`, `PLAY`, `DELETE`, `NEW`) et bouton de réinitialisation forcée depuis le serveur.
* Chiffrement local **AES-256-GCM** de tous les identifiants sensibles (tokens, mots de passe, flux RSS privés HTTP Basic).

### 🌍 Internationalisation (i18n)
* **4 langues intégrées dès l'origine :** Français (`fr`), English (`en`), Deutsch (`de`), Español (`es`).
* Détection automatique de la langue du système + **sélecteur rapide par drapeaux** dans la barre supérieure et dans les Paramètres (changement à chaud de toute l'interface, du System Tray et de la barre des tâches).

---

## 📊 Performances & Empreinte

| Métrique | Objectif | Mesuré (Windows MSVC Release) |
| :--- | :--- | :--- |
| **Poids du binaire autonome** | `< 12 Mo` | **`~7,4 Mo`** (`dist/PodDesktop.exe`) |
| **Empreinte RAM active** | `< 60 Mo` | **`~45 Mo`** |
| **Bundle Frontend (gzip)** | Ultra-léger | **`~50 Ko`** (HTML + CSS + TS + 4 langues) |
| **Écritures hors dossier portable** | `0` | **`0`** (`%APPDATA%` / `~/.config` intacts) |

---

## 📁 Arborescence Portable

PodDesktop fonctionne sans installateur (disque local, NAS ou clé USB). Tous les chemins sont résolus par rapport à l'exécutable et la WebView est isolée dans `./data/.runtime/` **avant** le démarrage de Tauri (`WEBVIEW2_USER_DATA_FOLDER` sous Windows, `XDG_*` sous Linux) :

```text
PodDesktop/
├── PodDesktop.exe          # Exécutable portable Windows (ou PodDesktop / .AppImage sous Linux)
├── bootstrap.json          # Chemin relatif ou absolu vers la base SQLite (./data/poddesktop.db)
└── data/                   # Dossier unique de données (déplaçable depuis Paramètres)
    ├── poddesktop.db       # Base SQLite (mode WAL + index plein texte FTS5)
    ├── .keystore           # Clé locale AES-256-GCM pour chiffrer les identifiants
    ├── covers/             # Cache local des pochettes
    ├── media/              # Épisodes téléchargés hors-ligne
    ├── exports/            # Exports OPML, Favoris (HTML/MD/JSON), sauvegardes DB
    └── .runtime/           # Cache isolé WebView2 / WebKitGTK
```

---

## 🏗️ Architecture

```text
PodDesktop/
├── src/                        # Frontend Vanilla TypeScript (ES2022) + CSS3
│   ├── i18n/                   # Moteur i18n + dictionnaires fr.json, en.json, de.json, es.json
│   ├── styles/                 # Design system (tokens Clair/Sombre, layout 3 panneaux, composants)
│   └── ui/                     # Sidebar, VirtualList, EpisodeList, Inspector, PlayerBar, Palette, Settings
├── src-tauri/src/              # Backend natif Rust (Tauri v2)
│   ├── main.rs                 # Isolation WebView2 / XDG avant init Tauri
│   ├── paths.rs & crypto.rs    # Résolution portable bootstrap.json & chiffrement AES-256-GCM
│   ├── db/                     # SQLite WAL + FTS5 (11 tables, triggers, file d'attente, maintenance)
│   ├── audio/                  # Décodeur Symphonia (fichier + HTTP Range), DSP WSOLA/R128/VoiceBoost, cpal
│   ├── rss/                    # Parser quick-xml RSS/Atom/Podcasting 2.0, dossiers locaux, OPML, découverte
│   ├── download.rs             # Téléchargements parallèles avec reprise HTTP Range & limites de cache
│   ├── sync/                   # Nextcloud GPodder Sync (Login Flow v2) + gpodder API v2 / oPodSync
│   └── os_integration/         # System Tray, SMTC/MPRIS2 (souvlaki), Win32 ITaskbarList3
├── scripts/                    # Scripts de packaging portable Windows & Linux
└── docs/CAHIER_DES_CHARGES.md  # Cahier des charges fonctionnel & technique complet (v4.2)
```

---

## 🚀 Installation & Compilation

### Prérequis
* **Rust** (toolchain stable)
* **Node.js** `>= 18` (uniquement pour l'outillage Vite / CLI Tauri)
* **Windows 10/11 :** Visual Studio C++ Build Tools (MSVC) et Microsoft Edge WebView2 Runtime (préinstallé sous Windows 10/11).
* **Linux (Debian / Ubuntu) :**
  ```bash
  sudo apt update && sudo apt install -y \
    libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev libasound2-dev libdbus-1-dev pkg-config build-essential
  ```

### Commandes

```bash
# 1. Installer les dépendances de build frontend
npm install

# 2. Lancer en mode développement (rechargement rapide)
npm run dev

# 3. Vérifier les types TypeScript & exécuter la suite de tests Rust (18 tests)
npm run typecheck
npm run test:rust

# 4. Compiler le binaire portable Windows -> dist/PodDesktop.exe
npm run build:win

# 5. Compiler le binaire portable Linux -> dist/PodDesktop & dist/PodDesktop.AppImage
npm run build:linux
```

---

## 🔄 Synchronisation AntennaPod

1. Ouvrez **Paramètres → 5. Synchronisation Mobile**.
2. Choisissez votre serveur :
   * **Nextcloud GPodder Sync (Recommandé) :** saisissez l'URL de votre instance Nextcloud puis cliquez sur **Connexion rapide Nextcloud (Login Flow v2)**.
   * **gpodder API v2 (`oPodSync`, `gpodder.net`, `goPodder`, `PodFetch`) :** saisissez l'URL du serveur, votre nom d'utilisateur et votre mot de passe (chiffré en AES-256-GCM).
3. Cliquez sur **Valider, Tester & Synchroniser maintenant** (ou **Vider la base locale & Forcer la synchro serveur** pour reconstruire intégralement la base PC depuis votre téléphone).

> **Note sur le protocole AntennaPod :** le protocole gpodder synchronise les abonnements, les épisodes lus/en cours (`PLAY`), les épisodes téléchargés/en attente (`DOWNLOAD`) et `NEW`/`DELETE`. AntennaPod ne transmet pas les Favoris (étoiles) via le protocole gpodder.

---

## ⌨️ Raccourcis clavier

*(Tous personnalisables depuis **Paramètres → 4. Interface & Raccourcis**)*

| Raccourci | Action |
| :--- | :--- |
| `Espace` | Lecture / Pause |
| `[` / `]` | Diminuer / Augmenter la vitesse de `0.05x` |
| `M` | Marquer l'épisode sélectionné (ou en cours) comme Lu / Non lu |
| `Shift + M` | Marquer tout le podcast affiché comme Lu |
| `B` | Poser un signet personnel horodaté avec note rapide |
| `F` | Ajouter / Retirer des Favoris |
| `Suppr` | Retirer de la file d'attente (ou de la boîte de réception) |
| `Ctrl + K` | Ouvrir la Palette de Commandes |
| `Ctrl + F` | Recherche instantanée plein texte (FTS5) |
| `Ctrl + A` | Sélectionner tous les épisodes de la vue |
| `F5` | Actualiser tous les flux RSS |

---

## 🌐 Ajouter une nouvelle langue

1. Dupliquez [`src/i18n/en.json`](src/i18n/en.json) vers `src/i18n/<code_iso>.json` et traduisez les clés.
2. Enregistrez la langue dans [`src/i18n/index.ts`](src/i18n/index.ts), ajoutez son drapeau SVG dans [`src/ui/icons.ts`](src/ui/icons.ts) et ses 7 libellés System Tray dans [`src-tauri/src/i18n_backend.rs`](src-tauri/src/i18n_backend.rs).

---

## 📄 Documentation

Le cahier des charges exhaustif d'origine (v4.2) est archivé dans [docs/CAHIER_DES_CHARGES.md](docs/CAHIER_DES_CHARGES.md).

//! Contrôleur principal du moteur audio natif (`symphonia` + `cpal` + DSP).
//!
//! Architecture :
//! - Un thread dédié (`poddesktop-audio`) pilote le décodage Symphonia, le pipeline DSP
//!   (WSOLA, Skip Silence, EBU R128, Voice Boost, Sleep Timer fade-out) et alimente
//!   un tampon circulaire partagé avec le flux temps réel `cpal`.
//! - Aucun verrouillage bloquant ni allocation mémoire dans le callback audio `cpal`.
//! - Émet l'instantané `AudioStateSnapshot` toutes les 250 ms vers le backend/frontend,
//!   et notifie proprement la fin d'épisode pour l'enchaînement automatique de la file.

pub mod decoder;
pub mod dsp;

use crate::audio::decoder::SymphoniaDecoder;
use crate::audio::dsp::{AudioDspPipeline, DspConfig};
use crate::models::AudioStateSnapshot;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Source audio supportée : fichier local sur disque ou streaming HTTP progressif sans téléchargement.
#[derive(Debug, Clone)]
pub enum AudioSource {
    LocalFile(PathBuf),
    HttpStream {
        url: String,
        username: Option<String>,
        password: Option<String>,
        http: reqwest::Client,
        runtime: tokio::runtime::Handle,
    },
}

/// Métadonnées d'une piste à charger dans le moteur audio.
#[derive(Debug, Clone)]
pub struct PlayRequest {
    pub episode_id: i64,
    pub feed_id: i64,
    pub title: String,
    pub feed_title: String,
    pub cover_url: Option<String>,
    pub source: AudioSource,
    pub start_position_ms: i64,
    pub known_duration_ms: i64,
    pub speed: f32,
}

/// Commandes envoyées de façon asynchrone au thread audio.
#[derive(Debug)]
enum AudioCmd {
    LoadAndPlay(PlayRequest),
    Pause,
    Resume { smart_rewind_ms: i64 },
    Stop,
    SeekMs(i64),
    UpdateDsp(DspConfig),
    SetSleepTimer {
        minutes: Option<u32>,
        end_of_episode: bool,
    },
}

/// Événements remontés par le moteur audio vers `lib.rs` (persistance DB, OS SMTC/MPRIS2, UI).
#[derive(Debug, Clone)]
pub enum AudioEvent {
    /// État périodique toutes les 250 ms + chaque changement d'état.
    StateChanged(AudioStateSnapshot),
    /// Incrément d'écoute réel + temps économisé (pour `playback_stats`).
    ListeningDelta {
        episode_id: i64,
        feed_id: i64,
        listened_ms: i64,
        saved_ms: i64,
    },
    /// Fin naturelle de l'épisode (pour passer au suivant, répéter ou mettre en pause).
    EpisodeEnded { episode_id: i64 },
}

/// Poignée thread-safe vers le moteur audio natif.
pub struct AudioEngine {
    tx: Sender<AudioCmd>,
    snapshot: Arc<Mutex<AudioStateSnapshot>>,
    dsp_cfg: Arc<Mutex<DspConfig>>,
}

impl AudioEngine {
    /// Démarre le thread audio natif et connecte le callback d'événements.
    pub fn spawn<F>(on_event: F) -> Self
    where
        F: Fn(AudioEvent) + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel::<AudioCmd>();
        let snapshot = Arc::new(Mutex::new(AudioStateSnapshot::default()));
        let dsp_cfg = Arc::new(Mutex::new(DspConfig::default()));

        let snap_clone = Arc::clone(&snapshot);
        let cb = Arc::new(on_event);

        thread::Builder::new()
            .name("poddesktop-audio".into())
            .spawn(move || audio_worker_loop(rx, snap_clone, cb))
            .expect("Impossible de démarrer le thread audio natif");

        Self {
            tx,
            snapshot,
            dsp_cfg,
        }
    }

    /// Retourne l'instantané courant du lecteur.
    pub fn snapshot(&self) -> AudioStateSnapshot {
        self.snapshot
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn load_and_play(&self, req: PlayRequest) {
        if let Ok(mut cfg) = self.dsp_cfg.lock() {
            cfg.speed = req.speed.clamp(0.50, 3.00);
        }
        let _ = self.tx.send(AudioCmd::LoadAndPlay(req));
    }

    pub fn pause(&self) {
        let _ = self.tx.send(AudioCmd::Pause);
    }

    pub fn resume(&self, smart_rewind_ms: i64) {
        let _ = self.tx.send(AudioCmd::Resume { smart_rewind_ms });
    }

    pub fn stop(&self) {
        let _ = self.tx.send(AudioCmd::Stop);
    }

    pub fn seek_ms(&self, ms: i64) {
        let _ = self.tx.send(AudioCmd::SeekMs(ms));
    }

    pub fn update_dsp<F: FnOnce(&mut DspConfig)>(&self, updater: F) {
        if let Ok(mut cfg) = self.dsp_cfg.lock() {
            updater(&mut cfg);
            cfg.speed = cfg.speed.clamp(0.50, 3.00);
            cfg.volume = cfg.volume.clamp(0.0, 2.0);
            let _ = self.tx.send(AudioCmd::UpdateDsp(*cfg));
        }
    }

    pub fn set_sleep_timer(&self, minutes: Option<u32>, end_of_episode: bool) {
        let _ = self.tx.send(AudioCmd::SetSleepTimer {
            minutes,
            end_of_episode,
        });
    }
}

/// État interne du tampon circulaire partagé entre le thread DSP et le callback `cpal`.
struct SharedRing {
    samples: VecDeque<f32>,
    paused: bool,
}

/// Construit et démarre le flux de sortie `cpal` adapté au `sample_rate` / `channels` décodés.
/// Si le périphérique audio utilise un taux différent, un rééchantillonneur linéaire léger
/// est appliqué directement dans le callback pour une compatibilité 100 % matérielle Windows/Linux.
fn build_cpal_stream(
    src_sample_rate: u32,
    src_channels: usize,
    ring: Arc<Mutex<SharedRing>>,
) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "Aucun périphérique de sortie audio détecté".to_string())?;

    let default_cfg = device
        .default_output_config()
        .map_err(|e| format!("Impossible de lire la configuration audio : {e}"))?;

    let out_channels = default_cfg.channels() as usize;
    let out_rate = default_cfg.sample_rate().0;
    let stream_cfg: cpal::StreamConfig = default_cfg.into();

    let ratio = src_sample_rate as f64 / out_rate as f64;
    let mut frac_pos: f64 = 0.0;
    let mut last_frame = [0.0f32; 2];
    let mut next_frame = [0.0f32; 2];
    let mut initialized = false;

    let stream = device
        .build_output_stream(
            &stream_cfg,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // try_lock évite tout blocage temps réel dans le thread matériel audio
                let Ok(mut guard) = ring.try_lock() else {
                    data.fill(0.0);
                    return;
                };
                if guard.paused {
                    data.fill(0.0);
                    return;
                }

                let pop_frame = |q: &mut VecDeque<f32>| -> Option<[f32; 2]> {
                    if q.len() < src_channels {
                        return None;
                    }
                    let l = q.pop_front().unwrap_or(0.0);
                    let r = if src_channels >= 2 {
                        q.pop_front().unwrap_or(l)
                    } else {
                        l
                    };
                    Some([l, r])
                };

                if !initialized {
                    if let Some(f0) = pop_frame(&mut guard.samples) {
                        last_frame = f0;
                        next_frame = pop_frame(&mut guard.samples).unwrap_or(f0);
                        initialized = true;
                    } else {
                        data.fill(0.0);
                        return;
                    }
                }

                for frame_out in data.chunks_mut(out_channels) {
                    while frac_pos >= 1.0 {
                        frac_pos -= 1.0;
                        last_frame = next_frame;
                        next_frame = pop_frame(&mut guard.samples).unwrap_or([0.0, 0.0]);
                    }
                    let t = frac_pos as f32;
                    let l = last_frame[0] + (next_frame[0] - last_frame[0]) * t;
                    let r = last_frame[1] + (next_frame[1] - last_frame[1]) * t;
                    frac_pos += ratio;

                    if out_channels == 1 {
                        frame_out[0] = (l + r) * 0.5;
                    } else {
                        frame_out[0] = l;
                        frame_out[1] = r;
                        for extra in frame_out.iter_mut().skip(2) {
                            *extra = 0.0;
                        }
                    }
                }
            },
            |err| {
                log::error!("Erreur flux de sortie cpal : {err}");
            },
            None,
        )
        .map_err(|e| format!("Échec ouverture flux audio : {e}"))?;

    stream
        .play()
        .map_err(|e| format!("Échec démarrage flux audio : {e}"))?;

    Ok(stream)
}

/// Boucle principale du thread audio dédié (`poddesktop-audio`).
fn audio_worker_loop(
    rx: Receiver<AudioCmd>,
    shared_snapshot: Arc<Mutex<AudioStateSnapshot>>,
    on_event: Arc<dyn Fn(AudioEvent) + Send + Sync>,
) {
    let mut decoder: Option<SymphoniaDecoder> = None;
    let mut dsp: Option<AudioDspPipeline> = None;
    let mut _stream: Option<cpal::Stream> = None;
    let ring = Arc::new(Mutex::new(SharedRing {
        samples: VecDeque::with_capacity(96_000),
        paused: true,
    }));

    let mut cfg = DspConfig::default();
    let mut state = AudioStateSnapshot::default();
    let mut current_feed_id: i64 = 0;
    let mut sleep_deadline: Option<Instant> = None;
    let mut sleep_end_of_ep = false;
    let mut paused_at: Option<Instant> = None;

    let mut raw_buf: Vec<f32> = Vec::with_capacity(8192);
    let mut dsp_buf: Vec<f32> = Vec::with_capacity(8192);
    let mut last_emit = Instant::now();
    let mut last_stat_tick = Instant::now();
    let mut eof_reached = false;

    let publish_state = |s: &AudioStateSnapshot,
                         shared: &Arc<Mutex<AudioStateSnapshot>>,
                         cb: &Arc<dyn Fn(AudioEvent) + Send + Sync>| {
        if let Ok(mut guard) = shared.lock() {
            *guard = s.clone();
        }
        cb(AudioEvent::StateChanged(s.clone()));
    };

    loop {
        // 1. Dépile toutes les commandes en attente sans bloquer si la lecture est active
        let wait_timeout = if state.is_playing {
            Duration::from_millis(8)
        } else {
            Duration::from_millis(60)
        };

        let first_cmd = rx.recv_timeout(wait_timeout).ok();
        let mut cmds = Vec::new();
        if let Some(c) = first_cmd {
            cmds.push(c);
        }
        while let Ok(c) = rx.try_recv() {
            cmds.push(c);
        }

        let mut state_dirty = false;

        for cmd in cmds {
            match cmd {
                AudioCmd::LoadAndPlay(req) => {
                    // Arrête proprement le flux précédent
                    if let Ok(mut r) = ring.lock() {
                        r.paused = true;
                        r.samples.clear();
                    }
                    _stream = None;
                    decoder = None;
                    dsp = None;
                    eof_reached = false;
                    paused_at = None;
                    cfg.fade_gain = 1.0;
                    cfg.speed = req.speed.clamp(0.50, 3.00);

                    state.episode_id = Some(req.episode_id);
                    current_feed_id = req.feed_id;
                    state.title = req.title;
                    state.feed_title = req.feed_title;
                    state.cover_url = req.cover_url;
                    state.is_loading = true;
                    state.is_playing = false;
                    state.error = None;
                    state.speed = cfg.speed;
                    publish_state(&state, &shared_snapshot, &on_event);

                    let opened = match req.source {
                        AudioSource::LocalFile(ref path) => SymphoniaDecoder::open_file(path),
                        AudioSource::HttpStream {
                            ref url,
                            ref username,
                            ref password,
                            ref http,
                            ref runtime,
                        } => SymphoniaDecoder::open_http_stream(
                            url,
                            http.clone(),
                            runtime.clone(),
                            username.clone(),
                            password.clone(),
                        ),
                    };

                    match opened {
                        Ok(mut dec) => {
                            let sr = dec.info.sample_rate;
                            let ch = dec.info.channels;
                            state.duration_ms = if dec.info.duration_ms > 0 {
                                dec.info.duration_ms
                            } else {
                                req.known_duration_ms
                            };

                            if req.start_position_ms > 500 {
                                let _ = dec.seek_ms(req.start_position_ms);
                                state.position_ms = req.start_position_ms;
                            } else {
                                state.position_ms = 0;
                            }

                            match build_cpal_stream(sr, ch, Arc::clone(&ring)) {
                                Ok(s) => {
                                    _stream = Some(s);
                                    dsp = Some(AudioDspPipeline::new(sr, ch));
                                    decoder = Some(dec);
                                    if let Ok(mut r) = ring.lock() {
                                        r.paused = false;
                                    }
                                    state.is_loading = false;
                                    state.is_playing = true;
                                    last_stat_tick = Instant::now();
                                }
                                Err(e) => {
                                    state.is_loading = false;
                                    state.is_playing = false;
                                    state.error = Some(e);
                                }
                            }
                        }
                        Err(e) => {
                            state.is_loading = false;
                            state.is_playing = false;
                            state.error = Some(e);
                        }
                    }
                    state_dirty = true;
                }
                AudioCmd::Pause => {
                    if state.is_playing {
                        if let Ok(mut r) = ring.lock() {
                            r.paused = true;
                        }
                        state.is_playing = false;
                        paused_at = Some(Instant::now());
                        state_dirty = true;
                    }
                }
                AudioCmd::Resume { smart_rewind_ms } => {
                    if decoder.is_some() && !state.is_playing {
                        // Smart Rewind configurable si mis en pause depuis au moins 2 secondes
                        let should_rewind = paused_at
                            .map(|t| t.elapsed() >= Duration::from_secs(2))
                            .unwrap_or(false);
                        if should_rewind && smart_rewind_ms > 0 {
                            let target = (state.position_ms - smart_rewind_ms).max(0);
                            if let Some(ref mut dec) = decoder {
                                if let Ok(actual) = dec.seek_ms(target) {
                                    state.position_ms = actual;
                                }
                            }
                            if let Some(ref mut pipeline) = dsp {
                                pipeline.reset();
                            }
                            if let Ok(mut r) = ring.lock() {
                                r.samples.clear();
                            }
                        }
                        cfg.fade_gain = 1.0;
                        if let Ok(mut r) = ring.lock() {
                            r.paused = false;
                        }
                        state.is_playing = true;
                        paused_at = None;
                        last_stat_tick = Instant::now();
                        state_dirty = true;
                    }
                }
                AudioCmd::Stop => {
                    if let Ok(mut r) = ring.lock() {
                        r.paused = true;
                        r.samples.clear();
                    }
                    _stream = None;
                    decoder = None;
                    dsp = None;
                    state.is_playing = false;
                    state.is_loading = false;
                    state.episode_id = None;
                    state.position_ms = 0;
                    state.duration_ms = 0;
                    sleep_deadline = None;
                    sleep_end_of_ep = false;
                    state.sleep_timer_remaining_sec = None;
                    state.sleep_at_end_of_episode = false;
                    state_dirty = true;
                }
                AudioCmd::SeekMs(target_ms) => {
                    if let Some(ref mut dec) = decoder {
                        if let Ok(actual) = dec.seek_ms(target_ms) {
                            state.position_ms = actual;
                            eof_reached = false;
                            if let Some(ref mut pipeline) = dsp {
                                pipeline.reset();
                            }
                            if let Ok(mut r) = ring.lock() {
                                r.samples.clear();
                            }
                            state_dirty = true;
                        }
                    }
                }
                AudioCmd::UpdateDsp(new_cfg) => {
                    cfg.speed = new_cfg.speed;
                    cfg.volume = new_cfg.volume;
                    cfg.skip_silence = new_cfg.skip_silence;
                    cfg.skip_silence_threshold_db = new_cfg.skip_silence_threshold_db;
                    cfg.normalize_loudness = new_cfg.normalize_loudness;
                    cfg.voice_boost = new_cfg.voice_boost;
                    state.speed = cfg.speed;
                    state.volume = cfg.volume;
                    state.skip_silence = cfg.skip_silence;
                    state.voice_boost = cfg.voice_boost;
                    state.normalize_loudness = cfg.normalize_loudness;
                    state_dirty = true;
                }
                AudioCmd::SetSleepTimer {
                    minutes,
                    end_of_episode,
                } => {
                    sleep_end_of_ep = end_of_episode;
                    sleep_deadline = minutes
                        .filter(|&m| m > 0)
                        .map(|m| Instant::now() + Duration::from_secs(m as u64 * 60));
                    cfg.fade_gain = 1.0;
                    state.sleep_at_end_of_episode = end_of_episode;
                    state.sleep_timer_remaining_sec = minutes.filter(|&m| m > 0).map(|m| m as u64 * 60);
                    state_dirty = true;
                }
            }
        }

        // 2. Gestion de la minuterie de sommeil avec fondu progressif sur les 15 dernières secondes
        if let Some(deadline) = sleep_deadline {
            let now = Instant::now();
            if now >= deadline {
                sleep_deadline = None;
                state.sleep_timer_remaining_sec = None;
                cfg.fade_gain = 1.0;
                if state.is_playing {
                    if let Ok(mut r) = ring.lock() {
                        r.paused = true;
                    }
                    state.is_playing = false;
                    paused_at = Some(now);
                    state_dirty = true;
                }
            } else {
                let rem = deadline.duration_since(now);
                let rem_secs = rem.as_secs_f32();
                state.sleep_timer_remaining_sec = Some(rem.as_secs());
                if rem_secs <= 15.0 {
                    // Fondu doux sur les 15 dernières secondes
                    cfg.fade_gain = (rem_secs / 15.0).clamp(0.0, 1.0);
                } else {
                    cfg.fade_gain = 1.0;
                }
            }
        }

        // 3. Alimentation du tampon circulaire si lecture active
        if state.is_playing {
            let buffered_samples = ring.lock().map(|r| r.samples.len()).unwrap_or(0);
            // Garde ~250 ms d'avance audio (très faible latence sur volume/vitesse/seek)
            if buffered_samples < 24_000 && !eof_reached {
                if let (Some(ref mut dec), Some(ref mut pipeline)) = (&mut decoder, &mut dsp) {
                    match dec.next_packet_f32(&mut raw_buf) {
                        Ok(Some(pos_ms)) => {
                            state.position_ms = pos_ms;
                            if state.duration_ms < pos_ms {
                                state.duration_ms = pos_ms;
                            }
                            dsp_buf.clear();
                            pipeline.process_block(&raw_buf, &cfg, &mut dsp_buf);
                            if let Ok(mut r) = ring.lock() {
                                r.samples.extend(dsp_buf.iter().copied());
                            }
                        }
                        Ok(None) => {
                            eof_reached = true;
                        }
                        Err(e) => {
                            state.error = Some(e);
                            state.is_playing = false;
                            if let Ok(mut r) = ring.lock() {
                                r.paused = true;
                            }
                            state_dirty = true;
                        }
                    }
                }
            }

            // Fin de l'épisode une fois le tampon vidé
            if eof_reached && buffered_samples < 256 {
                if let Ok(mut r) = ring.lock() {
                    r.paused = true;
                }
                state.is_playing = false;
                eof_reached = false;
                let ended_ep = state.episode_id;

                if sleep_end_of_ep {
                    // Minuterie "Fin de l'épisode" atteinte : pause sans enchaîner
                    sleep_end_of_ep = false;
                    state.sleep_at_end_of_episode = false;
                    state_dirty = true;
                } else if let Some(ep_id) = ended_ep {
                    publish_state(&state, &shared_snapshot, &on_event);
                    on_event(AudioEvent::EpisodeEnded { episode_id: ep_id });
                }
            }

            // Comptabilisation des statistiques d'écoute toutes les 5 secondes
            if last_stat_tick.elapsed() >= Duration::from_secs(5) {
                let elapsed_ms = last_stat_tick.elapsed().as_millis() as i64;
                last_stat_tick = Instant::now();
                if let Some(ep_id) = state.episode_id {
                    let content_ms = (elapsed_ms as f32 * cfg.speed) as i64;
                    let saved_ms = (content_ms - elapsed_ms).max(0);
                    on_event(AudioEvent::ListeningDelta {
                        episode_id: ep_id,
                        feed_id: current_feed_id,
                        listened_ms: elapsed_ms,
                        saved_ms,
                    });
                }
            }
        }

        // 4. Émission temps réel toutes les 250 ms (conforme §2.1) ou sur changement immédiat
        if state_dirty || (state.is_playing && last_emit.elapsed() >= Duration::from_millis(250)) {
            publish_state(&state, &shared_snapshot, &on_event);
            last_emit = Instant::now();
        }
    }
}

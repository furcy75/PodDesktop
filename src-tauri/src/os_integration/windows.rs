//! Intégration native de la barre des tâches Windows (`ITaskbarList3`).
//!
//! Implémente exactement la section 3.2 du Cahier des Charges v4.2 :
//! - **Progression sur l'icône barre des tâches :**
//!   - Verte (`TBPF_NORMAL`) pendant la lecture active
//!   - Jaune (`TBPF_PAUSED`) en pause
//!   - Cyan / Indéterminée (`TBPF_INDETERMINATE`) pendant un téléchargement actif
//! - **Thumbnail Toolbar (boutons dans l'aperçu miniature) :**
//!   - Lecture / Pause (icône vectorielle 16x16 générée nativement via `CreateIcon`)
//!   - Épisode suivant
//!   - Marquer comme lu (passe automatiquement à l'épisode suivant)
//!   - Visibilité et infobulles multilingues configurables dynamiquement.

use crate::i18n_backend::{native_strings, BackendLang};
use crate::os_integration::TaskbarProgressMode;
use std::sync::{Arc, OnceLock};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, TBPF_INDETERMINATE,
    TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED, THBF_ENABLED, THBF_HIDDEN, THB_FLAGS, THB_ICON,
    THB_TOOLTIP, THUMBBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIcon, DestroyIcon, HICON, WM_COMMAND};

const CMD_PLAY_PAUSE: u32 = 41_001;
const CMD_NEXT: u32 = 41_002;
const CMD_MARK_PLAYED: u32 = 41_003;
const SUBCLASS_ID: usize = 0x504F_4444; // "PODD"

/// Boutons de la Thumbnail Toolbar Windows.
#[derive(Debug, Clone, Copy)]
pub enum ThumbButton {
    PlayPause,
    Next,
    MarkPlayed,
}

static THUMB_CALLBACK: OnceLock<Arc<dyn Fn(ThumbButton) + Send + Sync>> = OnceLock::new();

/// Contrôleur Win32 `ITaskbarList3` (thread-safe via `HWND` brut + COM).
pub struct WinTaskbarController {
    hwnd: isize,
    buttons_added: bool,
    last_mode: TaskbarProgressMode,
    icon_play: isize,
    icon_next: isize,
    icon_check: isize,
}

unsafe impl Send for WinTaskbarController {}

impl Drop for WinTaskbarController {
    fn drop(&mut self) {
        unsafe {
            for raw in [self.icon_play, self.icon_next, self.icon_check] {
                if raw != 0 {
                    let _ = DestroyIcon(HICON(raw as *mut std::ffi::c_void));
                }
            }
        }
    }
}

impl WinTaskbarController {
    /// Attache le contrôleur `ITaskbarList3` et les boutons miniatures à la fenêtre principale.
    pub fn attach<F>(hwnd_raw: isize, lang: BackendLang, on_button: F) -> Result<Self, String>
    where
        F: Fn(ThumbButton) + Send + Sync + 'static,
    {
        let _ = THUMB_CALLBACK.set(Arc::new(on_button));

        unsafe {
            let hwnd = HWND(hwnd_raw as *mut std::ffi::c_void);
            let _ = SetWindowSubclass(hwnd, Some(taskbar_subclass_proc), SUBCLASS_ID, 0);
        }

        let icon_play = create_glyph_hicon(GLYPH_PLAY);
        let icon_next = create_glyph_hicon(GLYPH_NEXT);
        let icon_check = create_glyph_hicon(GLYPH_CHECK);

        let mut ctrl = Self {
            hwnd: hwnd_raw,
            buttons_added: false,
            last_mode: TaskbarProgressMode::None,
            icon_play,
            icon_next,
            icon_check,
        };
        ctrl.configure_buttons(lang, true, true, true);
        Ok(ctrl)
    }

    /// Met à jour la couleur et le pourcentage de la barre de progression de l'icône Windows.
    pub fn set_progress(&mut self, mode: TaskbarProgressMode, completed: u64, total: u64) {
        unsafe {
            let Some(taskbar) = create_taskbar_list() else {
                return;
            };
            let hwnd = HWND(self.hwnd as *mut std::ffi::c_void);

            if mode != self.last_mode {
                let flag = match mode {
                    TaskbarProgressMode::None => TBPF_NOPROGRESS,
                    TaskbarProgressMode::Playing => TBPF_NORMAL,
                    TaskbarProgressMode::Paused => TBPF_PAUSED,
                    TaskbarProgressMode::Downloading => TBPF_INDETERMINATE,
                };
                let _ = taskbar.SetProgressState(hwnd, flag);
                self.last_mode = mode;
            }

            if matches!(mode, TaskbarProgressMode::Playing | TaskbarProgressMode::Paused) && total > 0
            {
                let _ = taskbar.SetProgressValue(hwnd, completed.min(total), total);
            }
        }
    }

    /// Crée ou met à jour les 3 boutons de la miniature de barre des tâches Windows.
    pub fn configure_buttons(
        &mut self,
        lang: BackendLang,
        show_play_pause: bool,
        show_next: bool,
        show_mark_played: bool,
    ) {
        let s = native_strings(lang);
        let buttons = [
            make_thumb_button(CMD_PLAY_PAUSE, s.play_pause, self.icon_play, show_play_pause),
            make_thumb_button(CMD_NEXT, s.next_episode, self.icon_next, show_next),
            make_thumb_button(CMD_MARK_PLAYED, s.mark_played, self.icon_check, show_mark_played),
        ];

        unsafe {
            let Some(taskbar) = create_taskbar_list() else {
                return;
            };
            let hwnd = HWND(self.hwnd as *mut std::ffi::c_void);
            if !self.buttons_added {
                if taskbar.ThumbBarAddButtons(hwnd, &buttons).is_ok() {
                    self.buttons_added = true;
                }
            } else {
                let _ = taskbar.ThumbBarUpdateButtons(hwnd, &buttons);
            }
        }
    }
}

fn make_thumb_button(id: u32, tooltip: &str, hicon_raw: isize, visible: bool) -> THUMBBUTTON {
    let mut tip = [0u16; 260];
    for (dst, src) in tip.iter_mut().zip(tooltip.encode_utf16().take(259)) {
        *dst = src;
    }
    let has_icon = hicon_raw != 0;
    THUMBBUTTON {
        dwMask: if has_icon {
            THB_FLAGS | THB_TOOLTIP | THB_ICON
        } else {
            THB_FLAGS | THB_TOOLTIP
        },
        iId: id,
        iBitmap: 0,
        hIcon: HICON(hicon_raw as *mut std::ffi::c_void),
        szTip: tip,
        dwFlags: if visible { THBF_ENABLED } else { THBF_HIDDEN },
    }
}

/// Glyphes 16x16 monochromes nets (blanc sur fond transparent) pour les boutons miniatures Windows.
const GLYPH_PLAY: [&str; 16] = [
    "................",
    "................",
    "....##..........",
    "....####........",
    "....######......",
    "....########....",
    "....##########..",
    "....###########.",
    "....###########.",
    "....##########..",
    "....########....",
    "....######......",
    "....####........",
    "....##..........",
    "................",
    "................",
];

const GLYPH_NEXT: [&str; 16] = [
    "................",
    "................",
    "..##.......##...",
    "..####.....##...",
    "..######...##...",
    "..########.##...",
    "..###########...",
    "..###########...",
    "..###########...",
    "..###########...",
    "..########.##...",
    "..######...##...",
    "..####.....##...",
    "..##.......##...",
    "................",
    "................",
];

const GLYPH_CHECK: [&str; 16] = [
    "................",
    "................",
    ".............##.",
    "............###.",
    "...........###..",
    "..........###...",
    ".........###....",
    "..##....###.....",
    "..###..###......",
    "...######.......",
    "....####........",
    ".....##.........",
    "................",
    "................",
    "................",
    "................",
];

/// Construit une icône Win32 16x16 32-bits RGBA nette via `CreateIcon`.
fn create_glyph_hicon(rows: [&str; 16]) -> isize {
    let mut and_mask = [0xFFu8; 32]; // 16 lignes x 2 octets (1 = transparent, 0 = opaque)
    let mut xor_bits = [0u8; 16 * 16 * 4]; // BGRA 32-bits

    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().take(16).enumerate() {
            if ch == '#' {
                let byte_idx = y * 2 + (x / 8);
                let bit_idx = 7 - (x % 8);
                and_mask[byte_idx] &= !(1 << bit_idx);

                let px = (y * 16 + x) * 4;
                xor_bits[px] = 0xFF;     // B
                xor_bits[px + 1] = 0xFF; // G
                xor_bits[px + 2] = 0xFF; // R
                xor_bits[px + 3] = 0xFF; // A
            }
        }
    }

    unsafe {
        CreateIcon(
            None,
            16,
            16,
            1,
            32,
            and_mask.as_ptr(),
            xor_bits.as_ptr(),
        )
        .map(|h| h.0 as isize)
        .unwrap_or(0)
    }
}

unsafe fn create_taskbar_list() -> Option<ITaskbarList3> {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let tbl: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()?;
    let _ = tbl.HrInit();
    Some(tbl)
}

/// Procédure de sous-classement légère capturant uniquement les clics `WM_COMMAND`
/// des 3 boutons miniatures de la barre des tâches Windows.
unsafe extern "system" fn taskbar_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid_subclass: usize,
    _dw_ref_data: usize,
) -> LRESULT {
    if msg == WM_COMMAND {
        let cmd_id = (wparam.0 & 0xFFFF) as u32;
        if let Some(cb) = THUMB_CALLBACK.get() {
            match cmd_id {
                CMD_PLAY_PAUSE => {
                    cb(ThumbButton::PlayPause);
                    return LRESULT(0);
                }
                CMD_NEXT => {
                    cb(ThumbButton::Next);
                    return LRESULT(0);
                }
                CMD_MARK_PLAYED => {
                    cb(ThumbButton::MarkPlayed);
                    return LRESULT(0);
                }
                _ => {}
            }
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

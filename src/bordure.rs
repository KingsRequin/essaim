//! Bordure de la fenêtre au premier plan : elle prend la couleur d'Essaim et s'illumine sur
//! chaque kick (Windows 11, `DWMWA_BORDER_COLOR`, possible sur la fenêtre d'un autre processus).
//!
//! Pas de hook : la fenêtre au premier plan est lue à chaque image (`GetForegroundWindow`). Une
//! seule fenêtre colorée à la fois ; l'ancienne retrouve la couleur du système dès qu'elle perd
//! le premier plan. `DwmSetWindowAttribute` n'est appelé que si la couleur (quantifiée) change,
//! 60 fois par seconde au plus. La fenêtre colorée est notée dans `bordure.hwnd` : si Essaim est
//! tué, le lancement suivant (ou `--restaurer-bordure`) lui rend sa couleur. Le témoin n'est
//! réécrit que pour une nouvelle fenêtre colorée, et effacé seulement à la pause, au plein écran
//! et à la fermeture : entre deux fenêtres, il peut désigner une fenêtre déjà rendue (la lui
//! rendre encore est sans effet).

use crate::config::{Action, Bordure, Declencheur, Evenement};
use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GWL_EXSTYLE, GWL_STYLE,
    WS_CAPTION, WS_CHILD, WS_EX_TOOLWINDOW, WS_THICKFRAME,
};

/// Couleur de bordure choisie par le système.
const COLOR_DEFAULT: u32 = 0xFFFF_FFFF;
/// Durée de l'illumination d'un kick (montée immédiate, retombée adoucie).
const PULSE: f32 = 0.35;
/// Pas de quantification de chaque composante (une bordure fine ne montre pas mieux).
const STEP: u8 = 4;
/// Intervalle minimal entre deux changements de couleur (≈ 60 par seconde).
const MIN_INTERVAL: Duration = Duration::from_millis(16);
/// Nouvel examen de la fenêtre au premier plan (passage en plein écran sans changer de fenêtre).
const RECHECK: Duration = Duration::from_millis(500);

/// Fenêtre colorée en ce moment (pour le gestionnaire de plantage).
static COLORED: AtomicIsize = AtomicIsize::new(0);

/// Illumination d'un kick, `s` secondes après lui : 1 tout de suite, 0 après `PULSE`.
pub fn pulse(s: f32) -> f32 {
    if !(0.0..PULSE).contains(&s) {
        return 0.0;
    }
    let x = 1.0 - s / PULSE;
    x * x
}

/// Luminosité de la bordure (0 = noire, 1 = couleur de l'essaim) : `repos` au calme, montée
/// vers 1 sur un déclencheur, d'autant plus que sa `force` est grande.
pub fn level(cfg: &Bordure, force: f32, pulse: f32) -> f32 {
    let repos = cfg.repos.clamp(0.0, 1.0);
    (repos + (1.0 - repos) * force.clamp(0.0, 1.0) * pulse).clamp(0.0, 1.0)
}

/// COLORREF (0x00BBGGRR) de la couleur `rgb` à la luminosité `level`, quantifié.
pub fn colorref(rgb: (u8, u8, u8), level: f32) -> u32 {
    let q = |v: u8| {
        let v = (v as f32 * level.clamp(0.0, 1.0)).round() as u16;
        ((v + STEP as u16 / 2) / STEP as u16 * STEP as u16).min(255) as u32
    };
    q(rgb.0) | q(rgb.1) << 8 | q(rgb.2) << 16
}

/// La fenêtre (classe, styles) a une bordure à colorer : fenêtre principale à cadre, ni outil,
/// ni morceau du shell, ni fenêtre d'Essaim.
pub fn eligible(class: &str, style: u32, ex_style: u32) -> bool {
    // bureau, barre des tâches, calques et fenêtres techniques d'Essaim (la fenêtre des réglages,
    // elle, est une fenêtre comme les autres : elle se colore)
    if matches!(class, "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW" | "EssaimOverlay" | "EssaimTray" | "EssaimStream") {
        return false;
    }
    if style & WS_CHILD.0 != 0 || ex_style & WS_EX_TOOLWINDOW.0 != 0 {
        return false;
    }
    style & WS_THICKFRAME.0 != 0 || style & WS_CAPTION.0 == WS_CAPTION.0
}

fn set_color(hwnd: HWND, c: u32) -> windows::core::Result<()> {
    unsafe { DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &c as *const u32 as *const c_void, 4) }
}

fn marker() -> std::path::PathBuf {
    crate::config::dir().join("bordure.hwnd")
}

/// Rend sa couleur système à la fenêtre colorée (celle de cette instance, ou celle notée par
/// une instance tuée). Sans effet si aucune.
pub fn restore() {
    let mut list = vec![COLORED.swap(0, Ordering::Relaxed)];
    if let Ok(s) = std::fs::read_to_string(marker()) {
        list.extend(s.trim().parse::<isize>());
        let _ = std::fs::remove_file(marker());
    }
    list.sort_unstable();
    list.dedup();
    for h in list.into_iter().filter(|&h| h != 0) {
        let _ = set_color(HWND(h as *mut c_void), COLOR_DEFAULT);
    }
}

/// Fenêtre au premier plan si sa bordure peut être colorée.
fn target(fg: HWND) -> bool {
    unsafe {
        let mut buf = [0u16; 64];
        let n = GetClassNameW(fg, &mut buf) as usize;
        let class = String::from_utf16_lossy(&buf[..n]);
        let style = GetWindowLongPtrW(fg, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(fg, GWL_EXSTYLE) as u32;
        eligible(&class, style, ex) && !crate::win::covers_monitor(fg)
    }
}

#[derive(Default)]
pub struct Border {
    /// Fenêtre colorée et couleur qu'elle a reçue.
    colored: Option<(isize, u32)>,
    /// Dernière fenêtre au premier plan examinée, et si elle se colore.
    fg: Option<(isize, bool)>,
    checked: Option<Instant>,
    sent: Option<Instant>,
    /// Dernière illumination (déclencheur « bordure ») et sa force.
    kick_at: Option<Instant>,
    force: f32,
    /// Dernier déclenchement de chaque déclencheur (délai minimal), tirage de la chance.
    derniers: Vec<Instant>,
    tirage: u64,
    /// Fenêtres qui refusent la couleur (administrateur) : notées une fois au journal.
    refused: HashSet<isize>,
    /// Fenêtre notée dans le témoin `bordure.hwnd`.
    marque: Option<isize>,
}

impl Border {
    /// Un événement (kick, clic…) : ses déclencheurs « bordure » l'illuminent, chacun après son
    /// délai minimal et avec sa chance ; rien en mode sans flashs (`doux`).
    pub fn reagir(&mut self, ev: Evenement, liste: &[Declencheur], doux: bool, now: Instant) {
        if doux {
            return;
        }
        if self.derniers.len() != liste.len() {
            self.derniers.resize(liste.len(), now.checked_sub(Duration::from_secs(3600)).unwrap_or(now));
        }
        for (i, d) in liste.iter().enumerate() {
            if d.quand != ev || d.faire != Action::Bordure || (d.delai > 0.0 && (now - self.derniers[i]).as_secs_f32() < d.delai) {
                continue;
            }
            if d.probabilite < 1.0 {
                // xorshift : chance tirée sans dépendre de l'essaim
                self.tirage ^= (self.tirage << 13) | 1;
                self.tirage ^= self.tirage >> 7;
                self.tirage ^= self.tirage << 17;
                if (self.tirage >> 40) as f32 / (1u64 << 24) as f32 >= d.probabilite {
                    continue;
                }
            }
            self.derniers[i] = now;
            self.kick_at = Some(now);
            self.force = d.force;
        }
    }

    /// Une image : `on` = bordure voulue (active, ni pause ni plein écran) ; `rgb` = couleur de
    /// l'essaim.
    pub fn tick(&mut self, on: bool, cfg: &Bordure, rgb: (u8, u8, u8), now: Instant) {
        self.tick_for(on, cfg, rgb, now, || unsafe { GetForegroundWindow() });
    }

    /// `tick`, avec la fenêtre au premier plan donnée par `foreground` (lue seulement si utile).
    fn tick_for(&mut self, on: bool, cfg: &Bordure, rgb: (u8, u8, u8), now: Instant, foreground: impl FnOnce() -> HWND) {
        if !on || !cfg.active {
            self.reset();
            return;
        }
        if self.sent.is_some_and(|s| now - s < MIN_INTERVAL) {
            return;
        }
        let fg = foreground();
        let id = fg.0 as isize;
        let stale = self.checked.is_none_or(|c| now - c >= RECHECK);
        let ok = match self.fg {
            Some((h, ok)) if h == id && !stale => ok,
            _ => {
                let ok = !fg.is_invalid() && !self.refused.contains(&id) && target(fg);
                self.fg = Some((id, ok));
                self.checked = Some(now);
                ok
            }
        };
        if self.colored.is_some_and(|(h, _)| h != id || !ok) {
            self.release();
        }
        if !ok {
            return;
        }
        let p = self.kick_at.map_or(0.0, |k| pulse((now - k).as_secs_f32()));
        let c = colorref(rgb, level(cfg, self.force, p));
        if self.colored == Some((id, c)) {
            return;
        }
        self.sent = Some(now);
        match set_color(fg, c) {
            Ok(()) => {
                if self.colored.is_none() {
                    COLORED.store(id, Ordering::Relaxed);
                    if self.marque != Some(id) {
                        let _ = std::fs::write(marker(), id.to_string());
                        self.marque = Some(id);
                    }
                }
                self.colored = Some((id, c));
            }
            Err(e) => {
                if self.refused.len() > 256 {
                    self.refused.clear();
                }
                if self.refused.insert(id) {
                    log::info!("bordure : fenêtre {id:#x} refuse la couleur ({e})");
                }
                self.fg = Some((id, false));
            }
        }
    }

    /// Rend sa couleur système à la fenêtre colorée et efface le témoin (pause, plein écran,
    /// fermeture).
    pub fn reset(&mut self) {
        self.release();
        if self.marque.take().is_some() {
            let _ = std::fs::remove_file(marker());
        }
    }

    /// Rend sa couleur système à la fenêtre colorée (elle perd le premier plan) ; le témoin reste.
    fn release(&mut self) {
        if let Some((h, _)) = self.colored.take() {
            let _ = set_color(HWND(h as *mut c_void), COLOR_DEFAULT);
            COLORED.store(0, Ordering::Relaxed);
        }
    }
}

impl Drop for Border {
    fn drop(&mut self) {
        self.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_kick_monte_d_un_coup_puis_retombe_en_douceur() {
        assert_eq!(pulse(0.0), 1.0);
        assert!(pulse(0.05) > 0.7);
        let mut prev = 1.0;
        for i in 1..40 {
            let p = pulse(i as f32 * 0.01);
            assert!(p <= prev, "pas monotone à {i} cs");
            prev = p;
        }
        assert!(pulse(0.2) > 0.0 && pulse(0.2) < 0.3);
        assert_eq!(pulse(PULSE), 0.0);
        assert_eq!(pulse(5.0), 0.0);
        assert_eq!(pulse(-0.1), 0.0);
    }

    #[test]
    fn luminosite_repos_et_force() {
        let cfg = Bordure { active: true, repos: 0.5 };
        assert!((level(&cfg, 1.0, 0.0) - 0.5).abs() < 1e-6);
        assert!((level(&cfg, 1.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((level(&cfg, 0.4, 1.0) - 0.7).abs() < 1e-6);
        assert!((level(&cfg, 0.0, 1.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn illuminee_par_ses_declencheurs() {
        use crate::config::{Action, Declencheur, Evenement};
        let maintenant = Instant::now();
        let liste = [
            Declencheur { force: 0.6, ..Declencheur::new(Evenement::Caisse, Action::Bordure) },
            Declencheur::new(Evenement::Kick, Action::Eclat),
        ];
        let mut b = Border::default();
        b.reagir(Evenement::Kick, &liste, false, maintenant);
        assert!(b.kick_at.is_none(), "le kick n'a pas de déclencheur « bordure » ici");
        b.reagir(Evenement::Caisse, &liste, true, maintenant);
        assert!(b.kick_at.is_none(), "rien en mode sans flashs");
        b.reagir(Evenement::Caisse, &liste, false, maintenant);
        assert_eq!((b.kick_at, b.force), (Some(maintenant), 0.6));
    }

    #[test]
    fn couleur_quantifiee_au_format_colorref() {
        // rouge pur → 0x000000FF ; jamais la valeur « couleur du système »
        assert_eq!(colorref((255, 0, 0), 1.0), 0x0000_00FF);
        assert_eq!(colorref((0, 0, 255), 1.0), 0x00FF_0000);
        assert_eq!(colorref((255, 255, 255), 1.0), 0x00FF_FFFF);
        // de petites variations ne changent pas la couleur envoyée
        assert_eq!(colorref((100, 48, 200), 0.5), colorref((101, 49, 199), 0.5));
        // composantes multiples du pas (ou 255)
        let c = colorref((123, 77, 201), 0.83);
        for v in [c & 0xFF, c >> 8 & 0xFF, c >> 16 & 0xFF] {
            assert!(v % STEP as u32 == 0 || v == 255);
        }
        assert_eq!(colorref((200, 200, 200), 0.0), 0);
    }

    /// Pixel du bord haut de la fenêtre, au milieu (0xRRGGBB), capturé à l'écran.
    fn edge_pixel(h: HWND) -> u32 {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
        use windows::Win32::Graphics::Gdi::*;
        unsafe {
            let mut r = RECT::default();
            DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut c_void, 16).unwrap();
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, 1, 1);
            let old = SelectObject(mem, bmp.into());
            BitBlt(mem, 0, 0, 1, 1, Some(screen), (r.left + r.right) / 2, r.top, ROP_CODE(0x40CC_0020)).unwrap();
            let p = GetPixel(mem, 0, 0).0;
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            (p & 0xFF) << 16 | (p & 0xFF00) | (p >> 16 & 0xFF)
        }
    }

    /// Essai réel sur la fenêtre d'un autre processus (le Bloc-notes, ouvert à la main) :
    /// `cargo test --release bordure_reelle -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bordure_reelle_sur_le_bloc_notes() {
        use windows::core::w;
        use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
        let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let pad = unsafe { FindWindowW(w!("Notepad"), None) }.expect("ouvrir un Bloc-notes");
        assert!(target(pad), "le Bloc-notes doit être colorable");
        let wait = || std::thread::sleep(Duration::from_millis(60));
        let before = edge_pixel(pad);
        let cfg = Bordure::default();
        let rgb = (40, 200, 255);
        let mut b = Border::default();
        let t0 = Instant::now();
        b.tick_for(true, &cfg, rgb, t0, || pad);
        assert!(marker().exists());
        wait();
        let rest = edge_pixel(pad);
        // kick : illumination puis retombée
        let tk = Instant::now();
        b.reagir(crate::config::Evenement::Kick, &crate::config::Declencheurs::default().liste, false, tk);
        let mut seen = Vec::new();
        for i in 0..9 {
            b.tick_for(true, &cfg, rgb, Instant::now(), || pad);
            wait();
            seen.push(((Instant::now() - tk).as_millis(), b.colored.unwrap().1, edge_pixel(pad)));
            if i == 0 {
                assert_eq!(b.colored.unwrap().1, colorref(rgb, 1.0));
            }
        }
        // coût d'un changement de couleur (DwmSetWindowAttribute vers un autre processus)
        let n = 300;
        let tc = Instant::now();
        for i in 0..n {
            b.sent = None;
            b.tick_for(true, &cfg, (40, 200, (i % 2 * 100) as u8), tk + Duration::from_secs(5), || pad);
        }
        println!("changement de couleur : {:.1} µs", tc.elapsed().as_secs_f64() * 1e6 / n as f64);
        // la fenêtre perd le premier plan : couleur du système rendue (le témoin reste)
        b.tick_for(true, &cfg, rgb, tk + Duration::from_secs(6), HWND::default);
        assert!(b.colored.is_none());
        b.reset();
        assert!(!marker().exists());
        wait();
        let after = edge_pixel(pad);
        println!("avant {before:06X}, repos {rest:06X} (envoyé {:06X}), après {after:06X}", colorref(rgb, 0.5));
        for (ms, c, px) in &seen {
            println!("  kick +{ms} ms : envoyé {c:06X}, écran {px:06X}");
        }
        let blue = |p: u32| p & 0xFF;
        let red = |p: u32| p >> 16;
        assert!(blue(rest) > red(rest) + 20, "repos pas teinté : {rest:06X}");
        assert!(blue(seen[0].2) > blue(rest), "pas d'illumination : {:06X} ≤ {rest:06X}", seen[0].2);
        assert!(blue(seen[8].2) < blue(seen[0].2), "pas de retombée");
        assert_eq!(after, before, "couleur du système non rendue");
    }

    #[test]
    fn seules_les_fenetres_a_cadre_se_colorent() {
        use windows::Win32::UI::WindowsAndMessaging::{WS_EX_LAYERED, WS_EX_TOPMOST, WS_POPUP};
        let fen = WS_CAPTION.0 | WS_THICKFRAME.0;
        assert!(eligible("Notepad", fen, 0));
        assert!(eligible("Chrome_WidgetWin_1", WS_THICKFRAME.0, 0));
        assert!(eligible("#32770", WS_CAPTION.0, 0)); // boîte de dialogue
        assert!(!eligible("Notepad", fen, WS_EX_TOOLWINDOW.0));
        assert!(!eligible("Popup", 0, 0)); // sans cadre
        // barre et menu d'Horizon : popups superposés sans cadre, outils ou non
        assert!(!eligible("HorizonBar", WS_POPUP.0, WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0));
        assert!(!eligible("HorizonMenu", WS_POPUP.0, WS_EX_LAYERED.0 | WS_EX_TOPMOST.0));
        // la fenêtre des réglages d'Essaim se colore ; ses calques et sa fenêtre de stream, non
        assert!(eligible("EssaimReglages", fen, 0));
        for essaim in ["EssaimOverlay", "EssaimTray", "EssaimStream"] {
            assert!(!eligible(essaim, fen, 0), "{essaim}");
        }
        assert!(!eligible("Enfant", fen | WS_CHILD.0, 0));
        for shell in ["Shell_TrayWnd", "Shell_SecondaryTrayWnd", "Progman", "WorkerW", "EssaimOverlay", "EssaimTray"] {
            assert!(!eligible(shell, fen, 0), "{shell}");
        }
    }
}

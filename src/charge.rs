//! Charge mesurée de l'essaim (temps de calcul par image et cadence), publiée chaque seconde par
//! la boucle d'affichage et lue par la fenêtre des réglages.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;

static MS: AtomicU32 = AtomicU32::new(0);
static IPS: AtomicU32 = AtomicU32::new(0);
static PAUSE: AtomicBool = AtomicBool::new(false);
static CAPTURE: AtomicU8 = AtomicU8::new(0);
static VEILLE: AtomicBool = AtomicBool::new(false);
static RAISON: Mutex<String> = Mutex::new(String::new());

/// Pause demandée par l'utilisateur (menu, Horizon, fenêtre des réglages).
pub fn publier_pause(p: bool) {
    PAUSE.store(p, Ordering::Relaxed);
}

pub fn pause() -> bool {
    PAUSE.load(Ordering::Relaxed)
}

/// États de la capture du son, dans l'ordre de leur code.
const CAPTURES: [&str; 3] = ["active", "coupee", "erreur"];

/// Capture du son : "active", "coupee" (pause, plein écran, son désactivé) ou "erreur".
pub fn publier_capture(etat: &str) {
    let code = CAPTURES.iter().position(|&c| c == etat).unwrap_or(2);
    CAPTURE.store(code as u8, Ordering::Relaxed);
}

pub fn capture() -> &'static str {
    CAPTURES.get(CAPTURE.load(Ordering::Relaxed) as usize).copied().unwrap_or("erreur")
}

/// Veille profonde : souris immobile, ni son ni clic depuis `[rendu] veille_apres`.
pub fn publier_veille(v: bool) {
    VEILLE.store(v, Ordering::Relaxed);
}

pub fn veille() -> bool {
    VEILLE.load(Ordering::Relaxed)
}

/// Ce qui retient l'essaim, en clair (« Plein écran », « Ne pas déranger », « Batterie : 30 i/s »…) ;
/// vide si rien (`pauses::raison`).
pub fn publier_raison(r: &str) {
    if let Ok(mut x) = RAISON.lock() {
        r.clone_into(&mut x);
    }
}

pub fn raison() -> String {
    RAISON.lock().map(|r| r.clone()).unwrap_or_default()
}

/// Temps de calcul moyen d'une image (ms, sans l'attente de l'écran) et images par seconde.
pub fn publier(ms_par_image: f32, images_par_s: f32) {
    MS.store(ms_par_image.to_bits(), Ordering::Relaxed);
    IPS.store(images_par_s.to_bits(), Ordering::Relaxed);
}

pub fn lire() -> (f32, f32) {
    (f32::from_bits(MS.load(Ordering::Relaxed)), f32::from_bits(IPS.load(Ordering::Relaxed)))
}

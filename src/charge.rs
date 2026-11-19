//! Charge mesurée de l'essaim (temps de calcul par image et cadence), publiée chaque seconde par
//! la boucle d'affichage et lue par la fenêtre des réglages.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

static MS: AtomicU32 = AtomicU32::new(0);
static IPS: AtomicU32 = AtomicU32::new(0);
static PAUSE: AtomicBool = AtomicBool::new(false);
static CAPTURE: AtomicU8 = AtomicU8::new(0);
static VEILLE: AtomicBool = AtomicBool::new(false);

/// États de la capture du son, dans l'ordre de `CAPTURE`.
const CAPTURES: [&str; 3] = ["active", "coupee", "erreur"];

/// État de la capture du son : `"active"`, `"coupee"` (réglage `[audio] actif`) ou `"erreur"`.
/// Une autre valeur compte comme une erreur.
// appelée par la boucle principale (main.rs, équipe cœur) ; en attendant, « active »
#[allow(dead_code)]
pub fn publier_capture(etat: &str) {
    let i = CAPTURES.iter().position(|c| *c == etat).unwrap_or(2);
    CAPTURE.store(i as u8, Ordering::Relaxed);
}

pub fn capture() -> &'static str {
    CAPTURES.get(CAPTURE.load(Ordering::Relaxed) as usize).copied().unwrap_or("erreur")
}

/// Veille plein écran (jeu, vidéo sur l'écran de la souris).
#[allow(dead_code)] // appelée par main.rs (équipe cœur)
pub fn publier_veille(v: bool) {
    VEILLE.store(v, Ordering::Relaxed);
}

pub fn veille() -> bool {
    VEILLE.load(Ordering::Relaxed)
}

/// Pause demandée par l'utilisateur (menu, Horizon, fenêtre des réglages).
pub fn publier_pause(p: bool) {
    PAUSE.store(p, Ordering::Relaxed);
}

pub fn pause() -> bool {
    PAUSE.load(Ordering::Relaxed)
}

/// Temps de calcul moyen d'une image (ms, sans l'attente de l'écran) et images par seconde.
pub fn publier(ms_par_image: f32, images_par_s: f32) {
    MS.store(ms_par_image.to_bits(), Ordering::Relaxed);
    IPS.store(images_par_s.to_bits(), Ordering::Relaxed);
}

pub fn lire() -> (f32, f32) {
    (f32::from_bits(MS.load(Ordering::Relaxed)), f32::from_bits(IPS.load(Ordering::Relaxed)))
}

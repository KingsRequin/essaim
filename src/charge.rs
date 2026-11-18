//! Charge mesurée de l'essaim (temps de calcul par image et cadence), publiée chaque seconde par
//! la boucle d'affichage et lue par la fenêtre des réglages.

use std::sync::atomic::{AtomicU32, Ordering};

static MS: AtomicU32 = AtomicU32::new(0);
static IPS: AtomicU32 = AtomicU32::new(0);

/// Temps de calcul moyen d'une image (ms, sans l'attente de l'écran) et images par seconde.
pub fn publier(ms_par_image: f32, images_par_s: f32) {
    MS.store(ms_par_image.to_bits(), Ordering::Relaxed);
    IPS.store(images_par_s.to_bits(), Ordering::Relaxed);
}

pub fn lire() -> (f32, f32) {
    (f32::from_bits(MS.load(Ordering::Relaxed)), f32::from_bits(IPS.load(Ordering::Relaxed)))
}

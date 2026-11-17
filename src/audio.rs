//! Son de Windows → atomiques lus par la boucle d'affichage. Capture (WASAPI loopback) et
//! détection du kick viennent de la bibliothèque partagée `audio-commun` ; les résultats sont
//! publiés après chaque paquet reçu (~10 ms), sans attendre l'image suivante.

use audio_commun::capture::{self, Capture, Sink};
use audio_commun::kick::Analyzer;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct AudioShared {
    kicks: AtomicU32,
    bass: AtomicU32,
    pitch: AtomicU32,
    sensitivity: AtomicU32,
    min_gap: AtomicU32,
}

#[derive(Clone, Copy, Default)]
pub struct AudioView {
    pub kicks: u32,
    pub bass: f32,
    pub pitch: f32,
}

impl AudioShared {
    pub fn view(&self) -> AudioView {
        AudioView {
            kicks: self.kicks.load(Ordering::Relaxed),
            bass: f32::from_bits(self.bass.load(Ordering::Relaxed)),
            pitch: f32::from_bits(self.pitch.load(Ordering::Relaxed)),
        }
    }
    pub fn set_sensitivity(&self, s: f32) {
        self.sensitivity.store(s.to_bits(), Ordering::Relaxed);
    }
    pub fn set_min_gap(&self, s: f32) {
        self.min_gap.store(s.to_bits(), Ordering::Relaxed);
    }
    fn publish(&self, a: &Analyzer) {
        self.kicks.store(a.kicks, Ordering::Relaxed);
        self.bass.store(a.bass.to_bits(), Ordering::Relaxed);
        self.pitch.store(a.pitch.to_bits(), Ordering::Relaxed);
    }
}

/// Capture en cours ; l'arrêter = la `drop`.
pub type AudioThread = Capture;

pub fn start(shared: Arc<AudioShared>) -> std::io::Result<AudioThread> {
    capture::start("essaim-audio", Kick { shared, an: None })
}

struct Kick {
    shared: Arc<AudioShared>,
    an: Option<Analyzer>,
}

impl Sink for Kick {
    fn open(&mut self, sample_rate: u32) {
        self.an = Some(Analyzer::new(sample_rate));
    }

    fn samples(&mut self, mono: &[f32]) {
        if let Some(an) = &mut self.an {
            // Réglages lus à chaque paquet : un changement de config agit tout de suite.
            an.sensitivity = f32::from_bits(self.shared.sensitivity.load(Ordering::Relaxed));
            let gap = f32::from_bits(self.shared.min_gap.load(Ordering::Relaxed));
            if gap > 0.0 {
                an.min_gap_s = f64::from(gap);
            }
            an.push(mono);
        }
    }

    fn silence(&mut self, frames: usize) {
        if let Some(an) = &mut self.an {
            an.push_silence(frames);
        }
    }

    fn tick(&mut self, _now: Instant) -> Duration {
        if let Some(an) = &self.an {
            self.shared.publish(an);
        }
        Duration::from_millis(10)
    }

    fn lost(&mut self) {
        self.shared.bass.store(0f32.to_bits(), Ordering::Relaxed);
    }
}

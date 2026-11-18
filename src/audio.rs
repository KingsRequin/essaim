//! Son de Windows → atomiques lus par la boucle d'affichage. Capture (WASAPI loopback) et
//! détection du kick viennent de la bibliothèque partagée `audio-commun` ; les attaques des
//! aigus (charleston, cymbales) sont repérées ici (`Aigus`). Les résultats sont publiés après
//! chaque paquet reçu (~10 ms), sans attendre l'image suivante.

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
    aigus: AtomicU32,
    brillance: AtomicU32,
}

#[derive(Clone, Copy, Default)]
pub struct AudioView {
    pub kicks: u32,
    pub bass: f32,
    pub pitch: f32,
    /// Attaques d'aigus depuis le démarrage (charleston, cymbales).
    pub aigus: u32,
    /// Part d'aigus du moment, rapportée à son pic récent (0 à 1).
    pub brillance: f32,
}

impl AudioShared {
    pub fn view(&self) -> AudioView {
        AudioView {
            kicks: self.kicks.load(Ordering::Relaxed),
            bass: f32::from_bits(self.bass.load(Ordering::Relaxed)),
            pitch: f32::from_bits(self.pitch.load(Ordering::Relaxed)),
            aigus: self.aigus.load(Ordering::Relaxed),
            brillance: f32::from_bits(self.brillance.load(Ordering::Relaxed)),
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
    capture::start("essaim-audio", Kick { shared, an: None, aigus: None })
}

struct Kick {
    shared: Arc<AudioShared>,
    an: Option<Analyzer>,
    aigus: Option<Aigus>,
}

/// Attaques des aigus : deux passe-haut (5 kHz, 12 dB/octave), puis une enveloppe rapide (~2 ms)
/// comparée à une lente (~200 ms). Une attaque = la rapide passe trois fois au-dessus de la
/// lente, assez fort, au moins 70 ms après la précédente.
pub struct Aigus {
    a: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    rapide: f32,
    lent: f32,
    pic: f32,
    k_rapide: f32,
    k_lent: f32,
    k_pic: f32,
    attente: u32,
    refractaire: u32,
    pub compte: u32,
}

impl Aigus {
    pub fn new(fs: u32) -> Self {
        let fs = fs.max(8000) as f32;
        let rc = 1.0 / (std::f32::consts::TAU * 5000.0);
        let k = |s: f32| 1.0 - (-1.0 / (s * fs)).exp();
        Self {
            a: rc / (rc + 1.0 / fs),
            x1: 0.0,
            y1: 0.0,
            x2: 0.0,
            y2: 0.0,
            rapide: 0.0,
            lent: 0.0,
            pic: 1e-4,
            k_rapide: k(0.002),
            k_lent: k(0.2),
            k_pic: k(10.0),
            attente: 0,
            refractaire: (0.07 * fs) as u32,
            compte: 0,
        }
    }

    pub fn push(&mut self, mono: &[f32]) {
        for &x in mono {
            // deux passe-haut du premier ordre en série
            self.y1 = self.a * (self.y1 + x - self.x1);
            self.x1 = x;
            self.y2 = self.a * (self.y2 + self.y1 - self.x2);
            self.x2 = self.y1;
            let e = self.y2.abs();
            self.rapide += (e - self.rapide) * self.k_rapide;
            self.lent += (e - self.lent) * self.k_lent;
            self.pic = self.pic.max(self.lent) + (self.lent - self.pic) * self.k_pic;
            if self.attente > 0 {
                self.attente -= 1;
            } else if self.rapide > 3.0 * self.lent && self.rapide > 0.004 {
                self.compte = self.compte.wrapping_add(1);
                self.attente = self.refractaire;
            }
        }
    }

    pub fn brillance(&self) -> f32 {
        (self.lent / self.pic.max(1e-4)).clamp(0.0, 1.0)
    }
}

impl Sink for Kick {
    fn open(&mut self, sample_rate: u32) {
        self.an = Some(Analyzer::new(sample_rate));
        self.aigus = Some(Aigus::new(sample_rate));
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
        if let Some(a) = &mut self.aigus {
            a.push(mono);
            self.shared.aigus.store(a.compte, Ordering::Relaxed);
            self.shared.brillance.store(a.brillance().to_bits(), Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::Aigus;

    const FS: u32 = 48000;

    fn bruit(graine: &mut u64) -> f32 {
        *graine ^= *graine << 13;
        *graine ^= *graine >> 7;
        *graine ^= *graine << 17;
        (*graine >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }

    #[test]
    fn charleston_trouve_basse_et_kick_ignores() {
        // charleston : 30 ms de bruit toutes les 250 ms, pendant 4 s → 16 attaques
        let mut g = 0x9e37_79b9_7f4a_7c15u64;
        let mut a = Aigus::new(FS);
        let n = FS as usize * 4;
        let son: Vec<f32> = (0..n)
            .map(|i| {
                let t = i % (FS as usize / 4);
                // basse continue à 55 Hz en dessous
                let basse = 0.6 * (std::f32::consts::TAU * 55.0 * i as f32 / FS as f32).sin();
                basse + if t < FS as usize * 3 / 100 { 0.25 * bruit(&mut g) } else { 0.0 }
            })
            .collect();
        a.push(&son);
        assert!((14..=17).contains(&a.compte), "{} attaques", a.compte);

        // basse seule et kick (balayage grave 120 → 45 Hz) : aucune attaque d'aigus
        let mut a = Aigus::new(FS);
        let mut phase = 0f32;
        let son: Vec<f32> = (0..n)
            .map(|i| {
                let t = (i % (FS as usize / 2)) as f32 / FS as f32;
                let f = 45.0 + 75.0 * (-t * 30.0).exp();
                phase += std::f32::consts::TAU * f / FS as f32;
                // attaque de 3 ms, comme un vrai kick (un saut instantané serait un clic large bande)
                0.9 * (t / 0.003).min(1.0) * (-t * 8.0).exp() * phase.sin() + 0.4 * (std::f32::consts::TAU * 50.0 * i as f32 / FS as f32).sin()
            })
            .collect();
        a.push(&son);
        assert_eq!(a.compte, 0);
    }
}

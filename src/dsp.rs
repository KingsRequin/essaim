//! Analyse du son : kick (attaques dans le grave), niveau de basse et hauteur de la note de basse.
//!
//! Méthode (voir README, section « Analyse du son ») :
//! - kick : grave isolé par 3 passe-bas biquad à 90 Hz (~36 dB/octave, pas de FFT : sa fenêtre
//!   étale la caisse claire dans le grave). Une attaque = énergie grave au-dessus de sa moyenne
//!   lente (~350 ms), pour ne pas confondre avec une ligne de basse tenue. Garde anti-caisse
//!   claire : énergie grave ≥ 0,3 × le pic récent (120 ms) de l'énergie 120–1000 Hz.
//! - basse : niveau du grave en dB, rapporté au pic récent du morceau (plage fixe de 20 dB).
//! - hauteur : autocorrélation du grave sous-échantillonné à ~1 kHz, entre 25 et 83 Hz.
//!
//! Tout est causal et tourne échantillon par échantillon ; les décisions sont prises tous les
//! `HOP_S` (5 ms).

use std::collections::VecDeque;

const HOP_S: f64 = 0.005;
const KICK_WIN_S: f64 = 0.0107; // ~512 échantillons à 48 kHz
const PITCH_RATE: f64 = 1000.0;
const PITCH_N: usize = 48; // fenêtre d'autocorrélation : 48 ms
const LAG_MIN: usize = 12; // 83 Hz
const LAG_MAX: usize = 40; // 25 Hz

#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn new(sr: f64, f0: f64, high: bool) -> Self {
        // Formules RBJ, Q = 0,707 (Butterworth).
        let w = std::f64::consts::TAU * f0 / sr;
        let (c, a) = (w.cos(), w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2));
        let a0 = 1.0 + a;
        let (b0, b1, b2) = if high {
            ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0)
        } else {
            ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0)
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - a) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
    pub fn lowpass(sr: f64, f0: f64) -> Self {
        Self::new(sr, f0, false)
    }
    pub fn highpass(sr: f64, f0: f64) -> Self {
        Self::new(sr, f0, true)
    }
    #[inline]
    pub fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Énergie moyenne glissante (moyenne des carrés sur une fenêtre fixe).
struct MeanSquare {
    ring: Vec<f64>,
    pos: usize,
    sum: f64,
    since_resync: usize,
}

impl MeanSquare {
    fn new(len: usize) -> Self {
        Self { ring: vec![0.0; len.max(1)], pos: 0, sum: 0.0, since_resync: 0 }
    }
    #[inline]
    fn push(&mut self, x: f64) {
        let sq = x * x;
        self.sum += sq - self.ring[self.pos];
        self.ring[self.pos] = sq;
        self.pos = (self.pos + 1) % self.ring.len();
        self.since_resync += 1;
        if self.since_resync >= 1 << 16 {
            // recalcul exact de temps en temps : pas de dérive de la somme glissante
            self.sum = self.ring.iter().sum();
            self.since_resync = 0;
        }
    }
    fn value(&self) -> f64 {
        (self.sum / self.ring.len() as f64).max(0.0)
    }
}

pub struct Analyzer {
    low: [Biquad; 3],
    mid: [Biquad; 3],
    low_e: MeanSquare,
    mid_e: MeanSquare,
    hop: usize,
    hop_count: usize,
    dt: f64,
    pub t: f64,
    // kick
    slow: f64,
    exc_pk: f64,
    low_pk: f64,
    mid_hist: VecDeque<(f64, f64)>,
    armed: bool,
    last_kick: f64,
    /// Sensibilité du kick, 1 (peu sensible) à 10 (très sensible).
    pub sensitivity: f32,
    /// Nombre total de kicks détectés.
    pub kicks: u32,
    // basse
    peak_db: f64,
    bass_target: f64,
    /// Niveau de basse lissé, 0..1.
    pub bass: f32,
    // hauteur
    dec: usize,
    dec_count: usize,
    dec_rate: f64,
    pitch_buf: VecDeque<f64>,
    /// Hauteur de la dernière note de basse fiable, en Hz (0 = jamais mesurée).
    pub pitch: f32,
}

impl Analyzer {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate as f64;
        let dec = (sr / PITCH_RATE).round().max(1.0) as usize;
        let hop = (sr * HOP_S).round().max(1.0) as usize;
        Self {
            low: [Biquad::lowpass(sr, 90.0); 3],
            mid: [Biquad::highpass(sr, 120.0), Biquad::highpass(sr, 120.0), Biquad::lowpass(sr, 1000.0)],
            low_e: MeanSquare::new((sr * KICK_WIN_S).round() as usize),
            mid_e: MeanSquare::new((sr * KICK_WIN_S).round() as usize),
            hop,
            hop_count: 0,
            dt: hop as f64 / sr,
            t: 0.0,
            slow: 0.0,
            exc_pk: 1e-9,
            low_pk: 0.0,
            mid_hist: VecDeque::new(),
            armed: true,
            last_kick: -9.0,
            sensitivity: 5.0,
            kicks: 0,
            peak_db: -60.0,
            bass_target: 0.0,
            bass: 0.0,
            dec,
            dec_count: 0,
            dec_rate: sr / dec as f64,
            pitch_buf: VecDeque::with_capacity(PITCH_N + LAG_MAX + 2),
            pitch: 0.0,
        }
    }

    /// Ajoute des échantillons mono.
    pub fn push(&mut self, samples: &[f32]) {
        for &s in samples {
            self.sample(s as f64);
        }
    }

    /// Ajoute `n` échantillons de silence (Windows n'envoie rien quand rien ne joue).
    pub fn push_silence(&mut self, n: usize) {
        for _ in 0..n {
            self.sample(0.0);
        }
    }

    #[inline]
    fn sample(&mut self, x: f64) {
        let l = self.low.iter_mut().fold(x, |v, f| f.run(v));
        let m = self.mid.iter_mut().fold(x, |v, f| f.run(v));
        self.low_e.push(l);
        self.mid_e.push(m);
        self.dec_count += 1;
        if self.dec_count >= self.dec {
            self.dec_count = 0;
            if self.pitch_buf.len() == PITCH_N + LAG_MAX + 1 {
                self.pitch_buf.pop_front();
            }
            self.pitch_buf.push_back(l);
        }
        self.hop_count += 1;
        if self.hop_count >= self.hop {
            self.hop_count = 0;
            self.t += self.dt;
            self.step();
        }
    }

    fn step(&mut self) {
        let (t, dt) = (self.t, self.dt);
        let e = self.low_e.value();
        let mi = self.mid_e.value();

        // --- kick ---
        self.mid_hist.push_back((t, mi));
        while self.mid_hist.front().is_some_and(|q| t - q.0 > 0.12) {
            self.mid_hist.pop_front();
        }
        let mid_max = self.mid_hist.iter().fold(0.0f64, |a, q| a.max(q.1));
        let exc = (e - self.slow).max(0.0);
        self.slow += (e - self.slow) * (dt / 0.35).min(1.0);
        self.exc_pk = exc.max(self.exc_pk * (-dt / 3.0).exp());
        self.low_pk = e.max(self.low_pk * (-dt / 4.0).exp());
        let onset = exc / self.exc_pk;
        let sens = self.sensitivity.clamp(1.0, 10.0) as f64;
        let thr = 0.6 - (sens - 1.0) * (0.3 / 9.0);
        if onset < thr * 0.35 {
            self.armed = true;
        }
        if self.armed && onset > thr && t - self.last_kick > 0.12 {
            let loud = e > self.low_pk * 0.0158 && e > 1e-7;
            let guard = e >= 0.3 * mid_max;
            if loud && guard {
                self.kicks = self.kicks.wrapping_add(1);
                self.last_kick = t;
                self.armed = false;
            }
        }

        // --- niveau de basse ---
        let db = 10.0 * (e + 1e-12).log10();
        self.peak_db = db.max(self.peak_db - dt * 1.5);
        self.bass_target = if db < -70.0 {
            0.0
        } else {
            (1.0 - (self.peak_db - db) / 20.0).clamp(0.0, 1.0).powf(1.6)
        };
        let rate = if self.bass_target > self.bass as f64 { 40.0 } else { 8.0 };
        self.bass += ((self.bass_target - self.bass as f64) * (dt * rate).min(1.0)) as f32;

        // --- hauteur (hors des kicks, qui occupent la même zone) ---
        if self.bass_target > 0.1 && t - self.last_kick > 0.15 {
            if let Some(f) = self.estimate_pitch() {
                self.pitch = f as f32;
            }
        }
    }

    /// Autocorrélation normalisée ; garde la plus petite période presque aussi bonne que la
    /// meilleure (évite de répondre une octave trop bas).
    fn estimate_pitch(&self) -> Option<f64> {
        if self.pitch_buf.len() < PITCH_N + LAG_MAX + 1 {
            return None;
        }
        let (a, b) = self.pitch_buf.as_slices();
        let buf: Vec<f64> = a.iter().chain(b).copied().collect();
        let x = &buf[buf.len() - PITCH_N - LAG_MAX - 1..];
        let base = &x[LAG_MAX + 1..]; // les PITCH_N échantillons les plus récents
        let e0: f64 = base.iter().map(|v| v * v).sum();
        if e0 < 1e-12 {
            return None;
        }
        let mut r = [0.0f64; LAG_MAX + 2];
        for (lag, rl) in r.iter_mut().enumerate().skip(LAG_MIN - 1) {
            let start = LAG_MAX + 1 - lag;
            let past = &x[start..start + PITCH_N];
            let (mut num, mut e1) = (0.0, 0.0);
            for (p, q) in base.iter().zip(past) {
                num += p * q;
                e1 += q * q;
            }
            *rl = if e1 > 0.0 { num / (e0 * e1).sqrt() } else { 0.0 };
        }
        let best = (LAG_MIN..=LAG_MAX).map(|l| r[l]).fold(f64::MIN, f64::max);
        if best < 0.6 {
            return None;
        }
        let lag = (LAG_MIN..=LAG_MAX).find(|&l| r[l] >= best * 0.92 && r[l] >= r[l - 1] && r[l] >= r[l + 1])?;
        let (ym, y0, yp) = (r[lag - 1], r[lag], r[lag + 1]);
        let den = ym - 2.0 * y0 + yp;
        let off = if den < 0.0 { 0.5 * (ym - yp) / den } else { 0.0 };
        Some(self.dec_rate / (lag as f64 + off.clamp(-0.5, 0.5)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48000;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0
        }
    }

    /// 8 s à 120 BPM : caisse claire (corps à `snare_hz`) sur 2 et 4, charleston en croches,
    /// kicks sur chaque temps à partir de 4 s, basse tenue optionnelle (attaque douce).
    fn render(snare_hz: f64, bass: bool) -> Vec<f32> {
        let n = SR as usize * 8;
        let mut rng = Rng(1);
        let mut ph = 0.0f64;
        (0..n)
            .map(|i| {
                let t = i as f64 / SR as f64;
                let (beat, bar) = (t % 0.5, t % 1.0);
                let mut v = rng.next() * 0.01;
                let ts = (bar + 0.5) % 1.0;
                if ts < 0.2 {
                    v += (rng.next() * 0.5 + (std::f64::consts::TAU * snare_hz * ts).sin() * 0.6) * (-ts * 25.0).exp();
                }
                let th = t % 0.25;
                if th < 0.05 {
                    v += rng.next() * 0.15 * (-th * 80.0).exp();
                }
                if t >= 4.0 && beat < 0.25 {
                    let phase = 50.0 * beat + 60.0 * (1.0 - (-beat * 30.0).exp()) / 30.0;
                    v += (std::f64::consts::TAU * phase).sin() * 0.9 * (-beat * 12.0).exp();
                }
                if bass {
                    let tn = t % 0.75;
                    let f = [55.0, 41.0, 49.0, 62.0][(t / 0.75) as usize % 4];
                    ph += std::f64::consts::TAU * f / SR as f64;
                    v += ph.sin() * 0.3 * (tn / 0.05).min(1.0) * (-tn * 1.2).exp();
                }
                v as f32
            })
            .collect()
    }

    /// Instants (s) où le compteur de kicks augmente.
    fn kick_times(sig: &[f32], sens: f32) -> Vec<f64> {
        let mut a = Analyzer::new(SR);
        a.sensitivity = sens;
        let mut out = Vec::new();
        for chunk in sig.chunks(480) {
            let before = a.kicks;
            a.push(chunk);
            if a.kicks != before {
                out.push(a.t);
            }
        }
        out
    }

    #[test]
    fn caisse_claire_seule_ne_donne_aucun_kick() {
        for snare in [140.0, 190.0, 230.0] {
            for sens in [1.0, 5.0, 10.0] {
                let sig = render(snare, false);
                let early: Vec<_> = kick_times(&sig, sens).into_iter().filter(|&t| t < 4.0).collect();
                assert!(early.is_empty(), "caisse {snare} Hz, sens {sens} : faux kicks {early:?}");
            }
        }
    }

    #[test]
    fn kicks_trouves_vite_avec_ou_sans_basse() {
        let expected = [4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5];
        for bass in [false, true] {
            for sens in [1.0, 5.0, 10.0] {
                let hits = kick_times(&render(190.0, bass), sens);
                let mut delays = Vec::new();
                for k in expected {
                    if let Some(h) = hits.iter().find(|&&h| h >= k && h < k + 0.1) {
                        delays.push(h - k);
                    }
                }
                assert!(delays.len() >= 7, "basse {bass}, sens {sens} : {}/8 kicks ({hits:?})", delays.len());
                let extra = hits.iter().filter(|&&h| h >= 4.0).count() - delays.len();
                assert!(extra <= 1, "basse {bass}, sens {sens} : {extra} kicks en trop ({hits:?})");
                delays.sort_by(f64::total_cmp);
                let median = delays[delays.len() / 2];
                assert!(median < 0.03, "basse {bass}, sens {sens} : délai médian {:.0} ms", median * 1000.0);
            }
        }
    }

    #[test]
    fn hauteur_de_la_note_de_basse() {
        for f in [30.0f64, 41.0, 55.0, 73.0] {
            let mut a = Analyzer::new(SR);
            let sig: Vec<f32> = (0..SR as usize)
                .map(|i| ((std::f64::consts::TAU * f * i as f64 / SR as f64).sin() * 0.3) as f32)
                .collect();
            a.push(&sig);
            assert!((a.pitch as f64 - f).abs() < f * 0.04, "{f} Hz mesuré {:.1} Hz", a.pitch);
        }
    }

    #[test]
    fn niveau_de_basse_monte_et_retombe() {
        let mut a = Analyzer::new(SR);
        a.push_silence(SR as usize / 2);
        assert!(a.bass < 0.01);
        let tone: Vec<f32> = (0..SR as usize / 2)
            .map(|i| ((std::f64::consts::TAU * 45.0 * i as f64 / SR as f64).sin() * 0.3) as f32)
            .collect();
        a.push(&tone);
        assert!(a.bass > 0.8, "basse {}", a.bass);
        a.push_silence(SR as usize);
        assert!(a.bass < 0.05, "basse après silence {}", a.bass);
    }
}

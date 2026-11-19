//! Son de Windows → atomiques lus par la boucle d'affichage. Capture (WASAPI loopback) et
//! détection du kick viennent de la bibliothèque partagée `audio-commun` ; les attaques des
//! aigus (charleston, cymbales) sont repérées ici (`Aigus`). Les résultats sont publiés après
//! chaque paquet reçu (~10 ms), sans attendre l'image suivante.

use audio_commun::capture::{self, Capture, Sink};
use audio_commun::kick::Analyzer;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    /// Flux perdu (erreur, sortie changée) et pas encore rouvert.
    perdu: AtomicBool,
    /// Bandes voulues pour l'anneau de spectre (0 = aucune : rien n'est calculé).
    bandes_voulues: AtomicU32,
    /// Niveau de chaque bande, 0 à 1 (bits d'un f32), pour l'anneau de spectre.
    bandes: [AtomicU32; BANDES_MAX],
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
    /// Dernières valeurs publiées ; une valeur non finie (ne devrait pas arriver) vaut 0.
    pub fn view(&self) -> AudioView {
        let f = |a: &AtomicU32| fini(f32::from_bits(a.load(Ordering::Relaxed)));
        AudioView {
            kicks: self.kicks.load(Ordering::Relaxed),
            bass: f(&self.bass),
            pitch: f(&self.pitch),
            aigus: self.aigus.load(Ordering::Relaxed),
            brillance: f(&self.brillance),
        }
    }
    pub fn set_sensitivity(&self, s: f32) {
        self.sensitivity.store(s.to_bits(), Ordering::Relaxed);
    }
    pub fn set_min_gap(&self, s: f32) {
        self.min_gap.store(s.to_bits(), Ordering::Relaxed);
    }
    /// Capture arrêtée : niveaux à zéro (sinon basse, hauteur et brillance resteraient figées et
    /// la musique semblerait jouer). Les compteurs gardent leur valeur : ils ne reculent jamais.
    pub fn reset(&self) {
        for a in [&self.bass, &self.pitch, &self.brillance] {
            a.store(0f32.to_bits(), Ordering::Relaxed);
        }
        self.effacer_bandes();
        self.perdu.store(false, Ordering::Relaxed);
    }
    /// Nombre de bandes à calculer pour l'anneau de spectre (0 = aucune, coût nul ; sinon 8 à 32).
    pub fn set_bandes(&self, n: u32) {
        self.bandes_voulues.store(n, Ordering::Relaxed);
    }
    /// Niveaux des bandes (0 à 1) copiés dans `sortie` ; renvoie le nombre de bandes voulues.
    pub fn bandes(&self, sortie: &mut [f32; BANDES_MAX]) -> usize {
        let n = (self.bandes_voulues.load(Ordering::Relaxed) as usize).min(BANDES_MAX);
        for (s, a) in sortie.iter_mut().zip(&self.bandes).take(n) {
            *s = fini(f32::from_bits(a.load(Ordering::Relaxed))).clamp(0.0, 1.0);
        }
        n
    }
    fn effacer_bandes(&self) {
        for a in &self.bandes {
            a.store(0f32.to_bits(), Ordering::Relaxed);
        }
    }
    /// Le flux est perdu et la capture réessaie (pas de sortie son, erreur de Windows).
    pub fn perdu(&self) -> bool {
        self.perdu.load(Ordering::Relaxed)
    }
}

fn fini(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Capture en cours ; l'arrêter = la `drop`.
pub type AudioThread = Capture;

pub fn start(shared: Arc<AudioShared>) -> std::io::Result<AudioThread> {
    capture::start("essaim-audio", Kick { shared, an: None, aigus: None, bandes: None, base: (0, 0), propre: Vec::new() })
}

struct Kick {
    shared: Arc<AudioShared>,
    an: Option<Analyzer>,
    aigus: Option<Aigus>,
    /// Bandes de l'anneau de spectre (aucun filtre tant que l'effet est coupé).
    bandes: Option<Bandes>,
    /// Compteurs (kicks, aigus) déjà publiés à l'ouverture du flux : l'analyse rouverte repart
    /// de 0, les compteurs publiés continuent (sinon l'affichage verrait un saut, donc de faux
    /// kicks).
    base: (u32, u32),
    /// Copie des échantillons, valeurs non finies mises à 0.
    propre: Vec<f32>,
}

impl Kick {
    fn publier(&self) {
        let sh = &self.shared;
        if let Some(an) = &self.an {
            sh.kicks.store(self.base.0.wrapping_add(an.kicks), Ordering::Relaxed);
            sh.bass.store(fini(an.bass).to_bits(), Ordering::Relaxed);
            sh.pitch.store(fini(an.pitch).to_bits(), Ordering::Relaxed);
        }
    }

    fn publier_bandes(&self) {
        if let Some(b) = &self.bandes {
            for (a, v) in self.shared.bandes.iter().zip(b.niveaux()) {
                a.store(fini(v).to_bits(), Ordering::Relaxed);
            }
        }
    }

    fn publier_aigus(&self) {
        if let Some(a) = &self.aigus {
            self.shared.aigus.store(self.base.1.wrapping_add(a.compte), Ordering::Relaxed);
            self.shared.brillance.store(fini(a.brillance()).to_bits(), Ordering::Relaxed);
        }
    }
}

/// Échantillons utilisables tels quels, ou copiés dans `propre` avec les valeurs non finies
/// (NaN, infini : pilote défaillant) mises à 0 : un seul NaN empoisonnerait tous les filtres.
fn assainir<'a>(mono: &'a [f32], propre: &'a mut Vec<f32>) -> &'a [f32] {
    if mono.iter().all(|x| x.is_finite()) {
        return mono;
    }
    propre.clear();
    propre.extend(mono.iter().map(|&x| fini(x)));
    propre
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
            self.step(x);
        }
    }

    /// `n` échantillons de silence : les enveloppes retombent, la brillance avec.
    pub fn push_silence(&mut self, n: usize) {
        for _ in 0..n {
            self.step(0.0);
        }
    }

    #[inline]
    fn step(&mut self, x: f32) {
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

    pub fn brillance(&self) -> f32 {
        (self.lent / self.pic.max(1e-4)).clamp(0.0, 1.0)
    }
}

/// Bandes de l'anneau de spectre, au plus.
pub const BANDES_MAX: usize = 32;
/// Centres de la première et de la dernière bande (Hz).
const BANDE_BASSE: f32 = 40.0;
const BANDE_HAUTE: f32 = 12_000.0;
/// Plage affichée sous le pic récent (dB) : une bande 36 dB sous la plus forte est à zéro.
const BANDES_PLAGE_DB: f32 = 36.0;

/// Passe-bande du second ordre (RBJ, gain 1 au centre), forme transposée II en f32.
#[derive(Clone, Copy)]
struct PasseBande {
    b0: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl PasseBande {
    fn new(fs: f32, f0: f32, q: f32) -> Self {
        let w0 = std::f32::consts::TAU * f0 / fs;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self { b0: alpha / a0, a1: -2.0 * w0.cos() / a0, a2: (1.0 - alpha) / a0, z1: 0.0, z2: 0.0 }
    }

    #[inline]
    fn step(&mut self, x: f32) -> f32 {
        // b1 = 0, b2 = -b0
        let y = self.b0 * x + self.z1;
        self.z1 = -self.a1 * y + self.z2;
        self.z2 = -self.b0 * x - self.a2 * y;
        y
    }
}

/// Énergie par bande de fréquences pour l'anneau de spectre, sans FFT : `n` passe-bande (8 à
/// 32) à centres répartis en échelle logarithmique de 40 Hz à 12 kHz, chacun aussi large que
/// l'écart entre deux centres (bandes jointives) et fait de deux biquads en série (12 dB par
/// octave hors de la bande : un seul laissait une sinusoïde allumer la moitié de l'anneau), puis
/// une enveloppe par bande (attaque ~5 ms, relâche ~120 ms). Niveaux rapportés au pic récent de toutes les bandes, en dB (plage de
/// 36 dB). Aucun filtre tant que l'effet est coupé.
pub struct Bandes {
    fs: f32,
    filtres: Vec<[PasseBande; 2]>,
    env: Vec<f32>,
    k_attaque: f32,
    k_relache: f32,
    /// Pic récent (toutes bandes), retombe en ~8 s.
    pic: f32,
    k_pic: f32,
}

impl Bandes {
    pub fn new(fs: u32) -> Self {
        let fs = fs.max(8000) as f32;
        let k = |s: f32| 1.0 - (-1.0 / (s * fs)).exp();
        Self { fs, filtres: Vec::new(), env: Vec::new(), k_attaque: k(0.005), k_relache: k(0.12), pic: 1e-4, k_pic: k(8.0) }
    }

    /// Centre de la bande `i` sur `n` (Hz).
    pub fn centre(i: usize, n: usize) -> f32 {
        if n < 2 {
            return BANDE_BASSE;
        }
        BANDE_BASSE * (BANDE_HAUTE / BANDE_BASSE).powf(i as f32 / (n - 1) as f32)
    }

    /// Règle le nombre de bandes (0 = aucune ; sinon ramené entre 8 et 32). Filtres refaits
    /// seulement si le nombre change.
    pub fn regler(&mut self, n: usize) {
        let n = if n == 0 { 0 } else { n.clamp(8, BANDES_MAX) };
        if n == self.filtres.len() {
            return;
        }
        // largeur d'une bande = écart entre deux centres (en octaves)
        let oct = if n > 1 { (BANDE_HAUTE / BANDE_BASSE).log2() / (n - 1) as f32 } else { 1.0 };
        let r = 2f32.powf(oct);
        // deux cellules en série : chacune plus large (√(√2 − 1) ≈ 0,64), pour que l'ensemble
        // perde 3 dB à la frontière entre deux bandes
        let q = r.sqrt() / (r - 1.0) * 0.6436;
        self.filtres = (0..n)
            .map(|i| {
                let f = PasseBande::new(self.fs, Self::centre(i, n).min(0.45 * self.fs), q);
                [f, f]
            })
            .collect();
        self.env = vec![0.0; n];
    }

    pub fn actives(&self) -> bool {
        !self.filtres.is_empty()
    }

    pub fn push(&mut self, mono: &[f32]) {
        let (ka, kr) = (self.k_attaque, self.k_relache);
        // bande par bande : chaque filtre reste dans les registres pendant tout le paquet
        for (f, e) in self.filtres.iter_mut().zip(self.env.iter_mut()) {
            let ([mut f1, mut f2], mut en) = (*f, *e);
            for &x in mono {
                let y = f2.step(f1.step(x)).abs();
                en += (y - en) * if y > en { ka } else { kr };
            }
            (*f, *e) = ([f1, f2], en);
        }
        // pic : suit la bande la plus forte, retombe lentement
        let m = self.env.iter().copied().fold(0.0f32, f32::max);
        let k = 1.0 - (1.0 - self.k_pic).powi(mono.len().min(i32::MAX as usize) as i32);
        self.pic = (self.pic + (m - self.pic) * k).max(m).max(1e-4);
    }

    /// `n` échantillons de silence : les enveloppes retombent d'un coup (calcul direct).
    pub fn push_silence(&mut self, n: usize) {
        let k = (1.0 - self.k_relache).powi(n.min(i32::MAX as usize) as i32);
        for e in &mut self.env {
            *e *= k;
        }
        if n > 0 {
            for f in self.filtres.iter_mut().flatten() {
                (f.z1, f.z2) = (0.0, 0.0);
            }
        }
    }

    /// Niveau de chaque bande, 0 à 1 (dB sous le pic récent, plage de 36 dB).
    pub fn niveaux(&self) -> impl Iterator<Item = f32> + '_ {
        let pic = self.pic;
        self.env.iter().map(move |&e| (1.0 + 20.0 * (e.max(1e-9) / pic).log10() / BANDES_PLAGE_DB).clamp(0.0, 1.0))
    }
}

impl Sink for Kick {
    fn open(&mut self, sample_rate: u32) {
        // flux rouvert : les compteurs publiés continuent depuis leur valeur actuelle
        self.base = (self.shared.kicks.load(Ordering::Relaxed), self.shared.aigus.load(Ordering::Relaxed));
        self.an = Some(Analyzer::new(sample_rate));
        self.aigus = Some(Aigus::new(sample_rate));
        self.bandes = Some(Bandes::new(sample_rate));
        self.shared.perdu.store(false, Ordering::Relaxed);
    }

    fn samples(&mut self, mono: &[f32]) {
        let mono = assainir(mono, &mut self.propre);
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
        }
        // bandes du spectre : seulement si l'effet est voulu (sinon aucun filtre, coût nul)
        let voulues = self.shared.bandes_voulues.load(Ordering::Relaxed) as usize;
        let (mut publier, mut effacer) = (false, false);
        if let Some(b) = &mut self.bandes {
            let avant = b.actives();
            b.regler(voulues);
            if b.actives() {
                b.push(mono);
                publier = true;
            } else {
                effacer = avant;
            }
        }
        self.publier_aigus();
        if publier {
            self.publier_bandes();
        }
        if effacer {
            self.shared.effacer_bandes();
        }
    }

    fn silence(&mut self, frames: usize) {
        if let Some(an) = &mut self.an {
            an.push_silence(frames);
        }
        if let Some(a) = &mut self.aigus {
            a.push_silence(frames);
        }
        self.publier_aigus();
        if let Some(b) = self.bandes.as_mut().filter(|b| b.actives()) {
            b.push_silence(frames);
            self.publier_bandes();
        }
    }

    fn tick(&mut self, _now: Instant) -> Duration {
        self.publier();
        Duration::from_millis(10)
    }

    fn lost(&mut self) {
        let sh = &self.shared;
        sh.bass.store(0f32.to_bits(), Ordering::Relaxed);
        sh.brillance.store(0f32.to_bits(), Ordering::Relaxed);
        sh.effacer_bandes();
        sh.perdu.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{assainir, Aigus, AudioShared, Bandes, Kick, BANDES_MAX};
    use audio_commun::capture::Sink;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    #[test]
    fn la_brillance_retombe_en_silence() {
        let mut g = 0x1234_5678_9abc_def1u64;
        let mut a = Aigus::new(FS);
        let son: Vec<f32> = (0..FS as usize).map(|_| 0.3 * bruit(&mut g)).collect();
        a.push(&son);
        assert!(a.brillance() > 0.5, "{}", a.brillance());
        a.push_silence(FS as usize);
        assert!(a.brillance() < 0.05, "{}", a.brillance());
    }

    #[test]
    fn echantillons_non_finis_mis_a_zero() {
        let mut propre = Vec::new();
        let sain = [0.1f32, -0.2];
        assert_eq!(assainir(&sain, &mut propre).as_ptr(), sain.as_ptr());
        let abime = [0.5f32, f32::NAN, f32::INFINITY, -0.5];
        assert_eq!(assainir(&abime, &mut propre), &[0.5, 0.0, 0.0, -0.5]);
        // passés dans la vraie analyse, les niveaux restent finis
        let shared = Arc::new(AudioShared::default());
        let mut k = Kick { shared: shared.clone(), an: None, aigus: None, bandes: None, base: (0, 0), propre: Vec::new() };
        k.open(FS);
        let son: Vec<f32> = (0..4800).map(|i| if i % 7 == 0 { f32::NAN } else { (i as f32 * 0.05).sin() }).collect();
        k.samples(&son);
        k.tick(std::time::Instant::now());
        let v = shared.view();
        assert!(v.bass.is_finite() && v.pitch.is_finite() && v.brillance.is_finite());
    }

    #[test]
    fn les_compteurs_ne_reculent_pas_a_la_reouverture() {
        let shared = Arc::new(AudioShared::default());
        let mut k = Kick { shared: shared.clone(), an: None, aigus: None, bandes: None, base: (0, 0), propre: Vec::new() };
        k.open(FS);
        // l'analyse avait compté 7 kicks et 3 attaques d'aigus avant la perte du flux
        k.an.as_mut().unwrap().kicks = 7;
        k.aigus.as_mut().unwrap().compte = 3;
        k.tick(std::time::Instant::now());
        k.samples(&[0.0; 16]);
        assert_eq!((shared.view().kicks, shared.view().aigus), (7, 3));
        k.lost();
        assert!(shared.perdu());
        k.open(FS);
        assert!(!shared.perdu());
        k.tick(std::time::Instant::now());
        k.samples(&[0.0; 16]);
        assert_eq!((shared.view().kicks, shared.view().aigus), (7, 3));
        // capture arrêtée : niveaux à zéro, compteurs gardés
        shared.bass.store(0.7f32.to_bits(), Ordering::Relaxed);
        shared.reset();
        assert_eq!((shared.view().bass, shared.view().kicks), (0.0, 7));
    }

    const FS: u32 = 48000;

    fn sinus(f: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| 0.5 * (std::f32::consts::TAU * f * i as f32 / FS as f32).sin()).collect()
    }

    #[test]
    fn une_sinusoide_excite_la_bonne_bande() {
        for n in [8usize, 16, 32] {
            for f in [60.0f32, 1000.0, 6000.0] {
                let mut b = Bandes::new(FS);
                b.regler(n);
                b.push(&sinus(f, FS as usize / 2));
                // bande la plus forte = celle dont le centre est le plus proche (en octaves)
                let ecart = |i: usize| (Bandes::centre(i, n) / f).log2().abs();
                let forte = (0..n).max_by(|&i, &j| b.env[i].total_cmp(&b.env[j])).unwrap();
                let proche = (0..n).min_by(|&i, &j| ecart(i).total_cmp(&ecart(j))).unwrap();
                assert_eq!(forte, proche, "{n} bandes, {f} Hz : bande {forte} ({} Hz)", Bandes::centre(forte, n));
                // à l'écran : la bonne bande pleine, les bandes lointaines éteintes
                let niveaux: Vec<f32> = b.niveaux().collect();
                assert!(niveaux[forte] > 0.95, "{niveaux:?}");
                for (i, v) in niveaux.iter().enumerate() {
                    if ecart(i) > 2.5 {
                        assert!(*v < 0.4, "{n} bandes, {f} Hz : bande {i} à {v}");
                    }
                }
            }
        }
    }

    #[test]
    fn bandes_coupees_sans_calcul_et_remises_a_zero() {
        let shared = Arc::new(AudioShared::default());
        let mut k = Kick { shared: shared.clone(), an: None, aigus: None, bandes: None, base: (0, 0), propre: Vec::new() };
        k.open(FS);
        let mut sortie = [0f32; BANDES_MAX];
        // effet coupé : aucun filtre, rien de publié
        k.samples(&sinus(1000.0, 4800));
        assert!(!k.bandes.as_ref().unwrap().actives());
        assert_eq!(shared.bandes(&mut sortie), 0);
        // effet voulu : 12 bandes, la sinusoïde à 1 kHz se voit
        shared.set_bandes(12);
        k.samples(&sinus(1000.0, 9600));
        assert_eq!(shared.bandes(&mut sortie), 12);
        assert!(sortie[..12].iter().any(|&v| v > 0.9), "{sortie:?}");
        // silence : tout retombe
        k.silence(FS as usize);
        shared.bandes(&mut sortie);
        assert!(sortie[..12].iter().all(|&v| v < 0.05), "{sortie:?}");
        // effet coupé de nouveau : filtres retirés, niveaux à zéro
        k.samples(&sinus(1000.0, 9600));
        shared.set_bandes(0);
        k.samples(&sinus(1000.0, 480));
        assert!(!k.bandes.as_ref().unwrap().actives());
        shared.set_bandes(12);
        shared.bandes(&mut sortie);
        assert!(sortie[..12].iter().all(|&v| v == 0.0), "{sortie:?}");
    }

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

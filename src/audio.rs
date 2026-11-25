//! Son de Windows → atomiques lus par la boucle d'affichage. Capture (WASAPI loopback) et
//! détection du kick viennent de la bibliothèque partagée `audio-commun` ; les attaques des
//! aigus (charleston, cymbales) sont repérées ici (`Aigus`). Le tempo est tiré des instants des
//! kicks (`audio_commun::pulsation`, sans FFT), avec la phase du temps et le relais (musique sans
//! kick depuis 2 s alors qu'un tempo stable était connu). Les résultats sont publiés après chaque paquet reçu
//! (~10 ms), sans attendre l'image suivante.
//!
//! Seulement si un effet les demande (`set_musique`) : caisses claires (reconnues par
//! l'analyse du kick d'`audio-commun`, sur les mêmes attaques), montée et drops (`Montee`). Les niveaux des deux canaux donnent l'équilibre
//! gauche-droite (`balance`).
//!
//! Le kick reçoit le son à sa fréquence d'origine ; aigus, bandes du spectre et montée le
//! reçoivent ramené à 48 kHz au plus (`Reduction`) : à 96 ou 192 kHz, ils coûteraient deux à
//! quatre fois plus pour des fréquences que personne n'entend.
//!
//! Flux rouvert à la même fréquence après moins de 3 s (erreur, sortie changée, fin du repli des
//! applis) : l'analyse continue, tempo compris. Chaque capture a sa génération dans `AudioShared` :
//! celle qu'une nouvelle capture ou l'arrêt a remplacée n'y écrit plus rien.

use audio_commun::capture::{self, Capture, Ecoute, Sink};
use audio_commun::kick::Analyzer;
use audio_commun::pulsation::Pulsation;
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
    /// Tempo stable des kicks (f32, 0 = inconnu) et position dans le temps (0 à 1).
    bpm: AtomicU32,
    phase: AtomicU32,
    /// La musique joue sans kick depuis 2 s alors qu'un tempo stable était connu.
    relais: AtomicBool,
    /// Flux perdu (erreur) et pas encore rouvert.
    perdu: AtomicBool,
    /// Écoute par application refusée par Windows : toute la sortie par défaut est écoutée.
    repli: AtomicBool,
    /// Bandes voulues pour l'anneau de spectre (0 = aucune : rien n'est calculé).
    bandes_voulues: AtomicU32,
    /// Niveau de chaque bande, 0 à 1 (bits d'un f32), pour l'anneau de spectre.
    bandes: [AtomicU32; BANDES_MAX],
    /// Niveau absolu de la bande la plus forte (bits d'un f32) : y a-t-il du son ?
    bandes_crete: AtomicU32,
    /// Analyse musicale voulue (caisse claire, montée, drop) ; sinon rien n'est calculé.
    musique_voulue: AtomicBool,
    /// Caisses claires et drops (compteurs), tension de la montée (0 à 1), équilibre
    /// gauche-droite (−1 à 1).
    caisses: AtomicU32,
    drops: AtomicU32,
    tension: AtomicU32,
    balance: AtomicU32,
    /// Capture qui a le droit d'écrire ici : changée à chaque nouvelle capture et à l'arrêt. Le
    /// fil d'une capture lâchée encore en vie (arrêt borné à 300 ms, fil détaché) n'écrit plus
    /// rien.
    generation: AtomicU32,
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
    /// Tempo des kicks, quand il est stable.
    pub bpm: Option<f32>,
    /// Position dans le temps en cours : 0 sur le temps, → 1 juste avant le suivant.
    pub phase: f32,
    /// Relais du tempo : la musique joue sans kick depuis 2 s (break, couplet sans grosse
    /// caisse) alors qu'un tempo stable était connu.
    pub relais: bool,
    /// Caisses claires (et claps) depuis le démarrage, y compris celles frappées avec un kick.
    pub caisses: u32,
    /// Drops depuis le démarrage : retour brutal de l'énergie, ou premier kick après une montée.
    pub drops: u32,
    /// Montée en cours (roulement de caisse claire qui s'accélère, son qui s'éclaircit), 0 à 1.
    pub tension: f32,
    /// Côté qui joue le plus fort : −1 tout à gauche, 0 au centre, 1 tout à droite.
    pub balance: f32,
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
            bpm: Some(f(&self.bpm)).filter(|&b| b > 0.0),
            phase: f(&self.phase),
            relais: self.relais.load(Ordering::Relaxed),
            caisses: self.caisses.load(Ordering::Relaxed),
            drops: self.drops.load(Ordering::Relaxed),
            tension: f(&self.tension).clamp(0.0, 1.0),
            balance: f(&self.balance).clamp(-1.0, 1.0),
        }
    }
    pub fn set_sensitivity(&self, s: f32) {
        self.sensitivity.store(s.to_bits(), Ordering::Relaxed);
    }
    pub fn set_min_gap(&self, s: f32) {
        self.min_gap.store(s.to_bits(), Ordering::Relaxed);
    }
    /// Capture arrêtée (lâchée) : niveaux à zéro (sinon basse, hauteur et brillance resteraient
    /// figées et la musique semblerait jouer). Les compteurs gardent leur valeur : ils ne reculent
    /// jamais. Le fil de la capture lâchée, s'il tourne encore, n'écrit plus rien.
    pub fn reset(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        for a in [&self.bass, &self.pitch, &self.brillance, &self.bpm, &self.phase, &self.tension, &self.balance] {
            a.store(0f32.to_bits(), Ordering::Relaxed);
        }
        self.relais.store(false, Ordering::Relaxed);
        self.effacer_bandes();
        self.perdu.store(false, Ordering::Relaxed);
        self.repli.store(false, Ordering::Relaxed);
    }
    /// Analyse musicale voulue (un effet lit les caisses claires, la montée ou les drops).
    pub fn set_musique(&self, voulue: bool) {
        self.musique_voulue.store(voulue, Ordering::Relaxed);
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
    /// Niveau absolu de la bande la plus forte (0 en silence ou sans capture).
    pub fn bandes_crete(&self) -> f32 {
        fini(f32::from_bits(self.bandes_crete.load(Ordering::Relaxed))).max(0.0)
    }
    #[cfg(test)]
    pub fn set_bandes_crete(&self, v: f32) {
        self.bandes_crete.store(v.to_bits(), Ordering::Relaxed);
    }
    fn effacer_bandes(&self) {
        for a in &self.bandes {
            a.store(0f32.to_bits(), Ordering::Relaxed);
        }
        self.bandes_crete.store(0f32.to_bits(), Ordering::Relaxed);
    }
    /// Le flux est perdu et la capture réessaie (pas de sortie son, erreur de Windows).
    pub fn perdu(&self) -> bool {
        self.perdu.load(Ordering::Relaxed)
    }
    /// Seules certaines applis devaient être écoutées, mais Windows refuse la capture par
    /// application : toute la sortie par défaut est écoutée à la place (réessayé toutes les 30 s,
    /// sauf si cette version de Windows ne la connaît pas).
    pub fn repli_applis(&self) -> bool {
        self.repli.load(Ordering::Relaxed)
    }
    /// Nouvelle capture : elle seule écrira désormais (numéro rendu), plus de repli annoncé.
    fn nouvelle_capture(&self) -> u32 {
        let g = self.generation.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        self.repli.store(false, Ordering::Relaxed);
        g
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

/// Lance la capture : de la sortie `sortie` (identifiant Windows ; vide = sortie par défaut), ou
/// des seules applications `applis` (noms d'exécutables) si la liste n'est pas vide. Une capture
/// précédente sur le même `shared` n'y écrit plus rien, même si son fil tourne encore.
pub fn start(shared: Arc<AudioShared>, sortie: &str, applis: &[String]) -> std::io::Result<AudioThread> {
    capture::start_ecoute("essaim-audio", Ecoute::depuis_reglages(sortie, applis), Kick::new(shared))
}

/// Sous ce niveau crête (−50 dBFS), un paquet compte comme silencieux pour le tempo.
const AUDIBLE: f32 = 0.003;
/// Flux rouvert à la même fréquence après une coupure plus courte que ça (erreur, sortie
/// changée, fin du repli des applis) : l'analyse continue, la coupure comptée comme du silence.
/// Au-delà, le tempo serait de toute façon oublié (3 s de silence).
const REPRISE_MAX: Duration = Duration::from_secs(3);

struct Kick {
    shared: Arc<AudioShared>,
    /// Génération de la capture de ce `Sink` (`AudioShared::generation`) : plus rien n'est
    /// écrit quand ce n'est plus la sienne.
    generation: u32,
    /// Dernier réveil du fil (`tick`) : durée d'une coupure du flux.
    vu: Option<Instant>,
    an: Option<Analyzer>,
    aigus: Option<Aigus>,
    tempo: Pulsation,
    /// Bandes de l'anneau de spectre (aucun filtre tant que l'effet est coupé).
    bandes: Option<Bandes>,
    /// Analyse musicale (caisse claire, montée, drop), seulement si un effet la demande.
    musique: Option<Musique>,
    /// Fréquence du flux (celle que voit le kick).
    fs: u32,
    /// Son ramené à 48 kHz au plus pour les aigus, les bandes et la montée, et son tampon.
    reduction: Reduction,
    reduit: Vec<f32>,
    /// Équilibre gauche-droite, et durée du dernier paquet (s) pour le lissage.
    balance: Balance,
    duree_paquet: f32,
    /// Compteurs (kicks, aigus) déjà publiés à l'ouverture du flux : l'analyse rouverte repart
    /// de 0, les compteurs publiés continuent (sinon l'affichage verrait un saut, donc de faux
    /// kicks).
    base: (u32, u32),
    /// Copie des échantillons, valeurs non finies mises à 0.
    propre: Vec<f32>,
}

impl Kick {
    /// `Sink` d'une nouvelle capture : celui d'avant sur ce `shared` n'y écrit plus.
    fn new(shared: Arc<AudioShared>) -> Self {
        Self {
            generation: shared.nouvelle_capture(),
            shared,
            vu: None,
            an: None,
            aigus: None,
            tempo: Pulsation::new(),
            bandes: None,
            musique: None,
            fs: 48000,
            reduction: Reduction::new(48000),
            reduit: Vec::new(),
            balance: Balance::default(),
            duree_paquet: 0.01,
            base: (0, 0),
            propre: Vec::new(),
        }
    }

    /// Cette capture est-elle toujours celle qui écrit ?
    fn courante(&self) -> bool {
        self.shared.generation.load(Ordering::Relaxed) == self.generation
    }

    /// Compteurs de l'analyse du kick.
    fn coups(&self) -> Option<Coups> {
        self.an.as_ref().map(Coups::de)
    }

    /// Tempo : avancé et publié à chaque paquet (tempo, phase, relais).
    fn suivre_tempo(&mut self, audible: bool) {
        let Some(an) = &self.an else { return };
        let t = an.t;
        self.tempo.avancer(t, audible);
        let sh = &self.shared;
        sh.bpm.store(self.tempo.bpm().unwrap_or(0.0).to_bits(), Ordering::Relaxed);
        sh.phase.store(self.tempo.phase(t).unwrap_or(0.0).to_bits(), Ordering::Relaxed);
        sh.relais.store(self.tempo.relais(t), Ordering::Relaxed);
    }

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
            self.shared.bandes_crete.store(fini(b.crete()).to_bits(), Ordering::Relaxed);
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

/// Fréquence la plus haute que voient les aigus, les bandes et la montée.
const FS_ANALYSE_MAX: u32 = 48_000;
/// Facteurs de qualité des trois cellules d'un passe-bas de Butterworth d'ordre 6.
const Q_BUTTERWORTH_6: [f32; 3] = [0.517_638_1, std::f32::consts::FRAC_1_SQRT_2, 1.931_851_7];
/// Coupure du passe-bas avant la réduction, en part de la fréquence d'arrivée (19,2 kHz à 48 kHz).
const REDUCTION_COUPURE: f32 = 0.4;

/// Passe-bas du second ordre (RBJ), forme transposée II en f32.
#[derive(Clone, Copy)]
struct PasseBas {
    b0: f32,
    b1: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl PasseBas {
    fn new(fs: f32, f0: f32, q: f32) -> Self {
        let w0 = std::f32::consts::TAU * f0 / fs;
        let (c, alpha) = (w0.cos(), w0.sin() / (2.0 * q));
        let a0 = 1.0 + alpha;
        Self { b0: (1.0 - c) / 2.0 / a0, b1: (1.0 - c) / a0, a1: -2.0 * c / a0, a2: (1.0 - alpha) / a0, z1: 0.0, z2: 0.0 }
    }

    #[inline]
    fn step(&mut self, x: f32) -> f32 {
        // b2 = b0
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b0 * x - self.a2 * y;
        y
    }
}

/// Son ramené à 48 kHz au plus : au-dessus (88,2, 96, 192 kHz…), passe-bas de Butterworth
/// d'ordre 6 à 0,4 × la fréquence d'arrivée (contre le repliement), puis un échantillon gardé
/// sur `facteur` (le plus petit entier qui ramène sous 48 kHz). Jusqu'à 48 kHz, le son passe tel
/// quel (aucun calcul).
struct Reduction {
    facteur: usize,
    fs_sortie: u32,
    filtres: [PasseBas; 3],
    /// Échantillons reçus depuis le dernier gardé.
    phase: usize,
}

impl Reduction {
    fn new(fs: u32) -> Self {
        let fs = fs.max(1);
        let facteur = fs.div_ceil(FS_ANALYSE_MAX).max(1) as usize;
        let fs_sortie = fs / facteur as u32;
        let coupure = REDUCTION_COUPURE * fs_sortie as f32;
        Self { facteur, fs_sortie, filtres: Q_BUTTERWORTH_6.map(|q| PasseBas::new(fs as f32, coupure, q)), phase: 0 }
    }

    /// Fréquence du son rendu.
    fn fs_sortie(&self) -> u32 {
        self.fs_sortie
    }

    /// `mono` ramené à `fs_sortie` : tel quel jusqu'à 48 kHz, sinon filtré et réduit dans `tampon`.
    fn reduire<'a>(&mut self, mono: &'a [f32], tampon: &'a mut Vec<f32>) -> &'a [f32] {
        if self.facteur == 1 {
            return mono;
        }
        tampon.clear();
        let [mut f1, mut f2, mut f3] = self.filtres;
        for &x in mono {
            let y = f3.step(f2.step(f1.step(x)));
            self.phase += 1;
            if self.phase >= self.facteur {
                self.phase = 0;
                tampon.push(y);
            }
        }
        self.filtres = [f1, f2, f3];
        tampon
    }

    /// `n` échantillons de silence à l'arrivée : combien à la sortie. Les filtres repartent de zéro.
    fn silence(&mut self, n: usize) -> usize {
        if self.facteur == 1 {
            return n;
        }
        if n > 0 {
            for f in &mut self.filtres {
                (f.z1, f.z2) = (0.0, 0.0);
            }
        }
        let total = self.phase + n;
        self.phase = total % self.facteur;
        total / self.facteur
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

    /// Niveau absolu de la bande la plus forte (enveloppe, pas rapportée au pic).
    pub fn crete(&self) -> f32 {
        self.env.iter().copied().fold(0.0, f32::max)
    }

    /// Niveau de chaque bande, 0 à 1 (dB sous le pic récent, plage de 36 dB).
    pub fn niveaux(&self) -> impl Iterator<Item = f32> + '_ {
        let pic = self.pic;
        self.env.iter().map(move |&e| (1.0 + 20.0 * (e.max(1e-9) / pic).log10() / BANDES_PLAGE_DB).clamp(0.0, 1.0))
    }
}

// Chaque méthode commence par `courante` : une capture remplacée ou arrêtée n'écrit plus rien.
impl Sink for Kick {
    fn open(&mut self, sample_rate: u32) {
        if !self.courante() {
            return;
        }
        self.shared.perdu.store(false, Ordering::Relaxed);
        // même fréquence, courte coupure : l'analyse continue (tempo gardé), la coupure passée
        // comme du silence pour que son horloge suive le temps réel
        if self.an.is_some() && sample_rate == self.fs {
            if let Some(coupure) = self.vu.map(|t| t.elapsed()).filter(|&d| d <= REPRISE_MAX) {
                let n = (coupure.as_secs_f64() * f64::from(sample_rate)) as usize;
                let paquet = (sample_rate as usize / 100).max(1);
                for k in (0..n).step_by(paquet) {
                    self.silence(paquet.min(n - k));
                }
                self.vu = Some(Instant::now());
                return;
            }
        }
        // flux rouvert : les compteurs publiés continuent depuis leur valeur actuelle
        self.base = (self.shared.kicks.load(Ordering::Relaxed), self.shared.aigus.load(Ordering::Relaxed));
        // le kick à la fréquence du flux, le reste à 48 kHz au plus
        self.an = Some(Analyzer::new(sample_rate));
        self.reduction = Reduction::new(sample_rate);
        let fs_analyse = self.reduction.fs_sortie();
        self.aigus = Some(Aigus::new(fs_analyse));
        // nouvelle horloge (l'analyse repart de 0) : tempo à retrouver
        self.tempo = Pulsation::new();
        self.bandes = Some(Bandes::new(fs_analyse));
        // analyse musicale refaite à la nouvelle fréquence ; ses compteurs publiés continuent
        self.fs = sample_rate;
        self.musique = None;
        self.vu = Some(Instant::now());
    }

    fn samples(&mut self, mono: &[f32]) {
        if !self.courante() {
            return;
        }
        let mono = assainir(mono, &mut self.propre);
        if let Some(an) = &mut self.an {
            // Réglages lus à chaque paquet : un changement de config agit tout de suite.
            an.sensitivity = f32::from_bits(self.shared.sensitivity.load(Ordering::Relaxed));
            let gap = f32::from_bits(self.shared.min_gap.load(Ordering::Relaxed));
            if gap > 0.0 {
                an.min_gap_s = f64::from(gap);
            }
            let avant = an.kicks;
            an.push(mono);
            if an.kicks != avant {
                self.tempo.kick(an.t);
            }
        }
        let audible = mono.iter().any(|x| x.abs() > AUDIBLE);
        self.duree_paquet = mono.len() as f32 / self.fs.max(1) as f32;
        // aigus, montée et bandes : son ramené à 48 kHz au plus (tel quel jusque-là)
        let fs_analyse = self.reduction.fs_sortie();
        let bas = self.reduction.reduire(mono, &mut self.reduit);
        if let Some(a) = &mut self.aigus {
            a.push(bas);
        }
        // champ par champ : `mono` et `bas` empruntent encore `self.propre` et `self.reduit`
        let coups = self.an.as_ref().map(Coups::de);
        ecouter_musique(&self.shared, &mut self.musique, fs_analyse, Some(bas), bas.len(), coups);
        // bandes du spectre : seulement si l'effet est voulu (sinon aucun filtre, coût nul)
        let voulues = self.shared.bandes_voulues.load(Ordering::Relaxed) as usize;
        let (mut publier, mut effacer) = (false, false);
        if let Some(b) = &mut self.bandes {
            let avant = b.actives();
            b.regler(voulues);
            if b.actives() {
                b.push(bas);
                publier = true;
            } else {
                effacer = avant;
            }
        }
        // après le dernier usage de `mono` et `bas` (ils empruntent les tampons)
        self.suivre_tempo(audible);
        self.publier_aigus();
        if publier {
            self.publier_bandes();
        }
        if effacer {
            self.shared.effacer_bandes();
        }
    }

    fn silence(&mut self, frames: usize) {
        if !self.courante() {
            return;
        }
        if let Some(an) = &mut self.an {
            an.push_silence(frames);
        }
        // même silence, compté à la fréquence des aigus, de la montée et des bandes
        let reduits = self.reduction.silence(frames);
        if let Some(a) = &mut self.aigus {
            a.push_silence(reduits);
        }
        self.suivre_tempo(false);
        self.publier_aigus();
        let coups = self.coups();
        ecouter_musique(&self.shared, &mut self.musique, self.reduction.fs_sortie(), None, reduits, coups);
        self.balance.silence(frames as f32 / self.fs.max(1) as f32);
        self.shared.balance.store(self.balance.valeur.to_bits(), Ordering::Relaxed);
        if let Some(b) = self.bandes.as_mut().filter(|b| b.actives()) {
            b.push_silence(reduits);
            self.publier_bandes();
        }
    }

    fn tick(&mut self, now: Instant) -> Duration {
        if self.courante() {
            self.vu = Some(now);
            self.publier();
        }
        Duration::from_millis(10)
    }

    fn lost(&mut self) {
        if !self.courante() {
            return;
        }
        let sh = &self.shared;
        sh.bass.store(0f32.to_bits(), Ordering::Relaxed);
        sh.brillance.store(0f32.to_bits(), Ordering::Relaxed);
        sh.bpm.store(0f32.to_bits(), Ordering::Relaxed);
        sh.relais.store(false, Ordering::Relaxed);
        sh.tension.store(0f32.to_bits(), Ordering::Relaxed);
        sh.balance.store(0f32.to_bits(), Ordering::Relaxed);
        sh.effacer_bandes();
        sh.perdu.store(true, Ordering::Relaxed);
    }

    fn stereo(&mut self, gauche: f32, droite: f32) {
        if !self.courante() {
            return;
        }
        self.balance.paquet(gauche, droite, self.duree_paquet);
        self.shared.balance.store(self.balance.valeur.to_bits(), Ordering::Relaxed);
    }

    fn repli(&mut self, actif: bool) {
        if self.courante() {
            self.shared.repli.store(actif, Ordering::Relaxed);
        }
    }
}

/// Équilibre gauche-droite : −1 tout à gauche, 0 au centre, 1 tout à droite, lissé sur ~0,25 s.
/// Les paquets presque muets (les blancs entre deux coups) ne le changent pas ; après 0,5 s de
/// silence, il revient doucement au centre.
#[derive(Default)]
struct Balance {
    valeur: f32,
    /// Durée passée sans son (s).
    muet: f32,
}

impl Balance {
    /// Un paquet de `duree` secondes aux niveaux efficaces `gauche` et `droite`.
    fn paquet(&mut self, gauche: f32, droite: f32, duree: f32) {
        let somme = gauche + droite;
        if somme.is_finite() && somme > AUDIBLE {
            self.muet = 0.0;
            let vise = ((droite - gauche) / somme).clamp(-1.0, 1.0);
            self.valeur += (vise - self.valeur) * (1.0 - (-duree.max(0.0) / 0.25).exp());
        } else {
            self.silence(duree);
        }
        self.valeur = fini(self.valeur).clamp(-1.0, 1.0);
    }

    /// `duree` secondes de silence.
    fn silence(&mut self, duree: f32) {
        self.muet += duree.max(0.0);
        if self.muet >= 0.5 {
            self.valeur = fini(self.valeur * (-duree.max(0.0) / 0.5).exp());
        }
    }
}

/// Compteurs de l'analyse du kick (`audio_commun::kick`) après un paquet.
#[derive(Clone, Copy)]
struct Coups {
    kicks: u32,
    caisses: u32,
}

impl Coups {
    fn de(an: &Analyzer) -> Self {
        Self { kicks: an.kicks, caisses: an.caisses }
    }
}

/// Analyse musicale de `n` échantillons (`mono`, ou du silence) quand un effet la demande ;
/// caisses claires, drops et montée publiés. `coups` : compteurs de l'analyse du kick. Analyse
/// retirée (coût nul) quand plus aucun effet ne la demande.
fn ecouter_musique(sh: &AudioShared, musique: &mut Option<Musique>, fs: u32, mono: Option<&[f32]>, n: usize, coups: Option<Coups>) {
    if !sh.musique_voulue.load(Ordering::Relaxed) {
        if musique.take().is_some() {
            sh.tension.store(0f32.to_bits(), Ordering::Relaxed);
        }
        return;
    }
    let m = musique.get_or_insert_with(|| Musique::new(fs, sh.caisses.load(Ordering::Relaxed), sh.drops.load(Ordering::Relaxed)));
    m.ecouter(mono, n, coups);
    sh.caisses.store(m.caisses, Ordering::Relaxed);
    sh.drops.store(m.drops, Ordering::Relaxed);
    sh.tension.store(fini(m.montee.tension).to_bits(), Ordering::Relaxed);
}

/// Sous ce niveau efficace (−60 dBFS), un paquet compte comme silencieux pour la montée.
const MUSIQUE_SILENCE: f32 = 0.001;

/// Caisses claires, drops et montée.
struct Musique {
    eclat: Eclat,
    /// Horloge propre (s, d'après les échantillons reçus).
    t: f64,
    fs: f64,
    /// Compteurs de l'analyse du kick déjà vus.
    vus: Option<Coups>,
    /// Compteurs publiés (continuent d'une analyse à l'autre).
    caisses: u32,
    drops: u32,
    montee: Montee,
}

impl Musique {
    fn new(fs: u32, caisses: u32, drops: u32) -> Self {
        let fs = fs.max(8000);
        Self { eclat: Eclat::new(fs), t: 0.0, fs: f64::from(fs), vus: None, caisses, drops, montee: Montee::default() }
    }

    /// `n` échantillons (`mono`, ou du silence). `coups` : compteurs de l'analyse du kick après
    /// ce paquet ; kicks et caisses claires reconnus dans le paquet sont notés à sa fin.
    fn ecouter(&mut self, mono: Option<&[f32]>, n: usize, coups: Option<Coups>) {
        // paquet vide (moins d'un échantillon après réduction) : aucune durée, rien à juger (la
        // moyenne du niveau diviserait 0 par 0)
        if n == 0 {
            return;
        }
        let mut carres = 0f64;
        for i in 0..n {
            let x = mono.map_or(0.0, |m| m[i]);
            carres += f64::from(x * x);
            self.eclat.step(x);
        }
        self.t += n as f64 / self.fs;
        if let Some(c) = coups {
            if let Some(vus) = self.vus {
                // au plus quelques coups par paquet de 10 ms ; la borne protège d'un compteur remis à 0
                for _ in 0..c.caisses.wrapping_sub(vus.caisses).min(8) {
                    self.caisses = self.caisses.wrapping_add(1);
                    self.montee.caisse();
                }
                if c.kicks != vus.kicks && self.montee.kick(self.t) {
                    self.drops = self.drops.wrapping_add(1);
                }
            }
            self.vus = Some(c);
        }
        let rms = (carres / n.max(1) as f64).sqrt() as f32;
        if self.montee.avancer(self.t, n as f32 / self.fs as f32, self.eclat.brillance(), rms) {
            self.drops = self.drops.wrapping_add(1);
        }
    }
}

/// Brillance du son pour la montée : part de la bande 8–12 kHz (deux passe-bande en série
/// centrés sur 10 kHz) dans le son entier, niveaux lissés sur ~0,3 s.
struct Eclat {
    aigu: [PasseBande; 2],
    haut: f32,
    tout: f32,
    k_doux: f32,
}

impl Eclat {
    fn new(fs: u32) -> Self {
        let fs = fs.max(8000) as f32;
        let aigu = PasseBande::new(fs, 10_000.0f32.min(0.4 * fs), 1.6);
        Self { aigu: [aigu, aigu], haut: 0.0, tout: 0.0, k_doux: 1.0 - (-1.0 / (0.3 * fs)).exp() }
    }

    #[inline]
    fn step(&mut self, x: f32) {
        let [a0, a1] = &mut self.aigu;
        let a = a1.step(a0.step(x)).abs();
        self.haut += (a - self.haut) * self.k_doux;
        self.tout += (x.abs() - self.tout) * self.k_doux;
    }

    /// Part de l'aigu dans le son, 0 (−40 dB) à 1 (autant que le son entier).
    fn brillance(&self) -> f32 {
        if self.tout < 1e-6 {
            return 0.0;
        }
        fini(1.0 + 20.0 * (self.haut.max(1e-9) / self.tout).log10() / 40.0).clamp(0.0, 1.0)
    }
}

/// Montée (build-up) et drop.
///
/// - `tension` (0 à 1) : roulement de caisse claire bien plus serré que d'habitude, et son qui
///   s'éclaircit (filtre qui s'ouvre) ; monte en ~0,6 s, retombe en ~1,5 s.
/// - Drop : le premier kick après une montée (au moins 1,2 s sans kick, tension passée au-dessus
///   de 0,45), ou le retour brutal de l'énergie après un passage calme (niveau des 0,5 dernières
///   secondes face à celui des 12 dernières, comme `spectre` d'`audio-commun`). Au plus un toutes
///   les 8 s ; la tension retombe alors à 0.
struct Montee {
    /// Caisses claires par seconde, sur ~1 s et sur ~12 s.
    rythme_court: f32,
    rythme_long: f32,
    /// Brillance (0 à 1), sur ~0,5 s et sur ~8 s ; amorcée par la première valeur entendue.
    brillance_court: f32,
    brillance_long: f32,
    amorcee: bool,
    tension: f32,
    /// Tension la plus haute depuis le dernier kick (un drop arrive souvent après une courte
    /// retombée : silence, cassure).
    tension_max: f32,
    dernier_kick: f64,
    dernier_drop: f64,
    /// Niveau (dB) sur ~0,5 s et sur ~12 s, durée de son entendue depuis le dernier silence, et
    /// temps passé depuis le dernier passage calme.
    niveau_court: f32,
    niveau_long: f32,
    entendu: f32,
    depuis_calme: f32,
    /// Puissance du son lissée sur ~85 ms, et durée passée sous le seuil de silence (s).
    puissance: f32,
    muet: f32,
}

/// Écart minimal entre deux drops (s).
const DROP_ECART: f64 = 8.0;
/// Intensité (0,5 = niveau habituel) sous laquelle un passage est calme, au-dessus de laquelle
/// l'énergie est revenue ; fenêtre entre les deux (s) ; écart de niveau qui fait varier
/// l'intensité de 0,5 (dB).
const CALME: f32 = 0.32;
const FORT: f32 = 0.66;
const CALME_FENETRE: f32 = 4.0;
const INTENSITE_DB: f32 = 8.0;

impl Default for Montee {
    fn default() -> Self {
        Self {
            rythme_court: 0.0,
            rythme_long: 0.0,
            brillance_court: 0.0,
            brillance_long: 0.0,
            amorcee: false,
            tension: 0.0,
            tension_max: 0.0,
            dernier_kick: f64::NEG_INFINITY,
            dernier_drop: f64::NEG_INFINITY,
            niveau_court: -70.0,
            niveau_long: -70.0,
            entendu: 0.0,
            depuis_calme: CALME_FENETRE,
            puissance: 0.0,
            muet: 0.0,
        }
    }
}

impl Montee {
    /// Une caisse claire (comptée dans les deux rythmes, en caisses par seconde).
    fn caisse(&mut self) {
        self.rythme_court += 1.0;
        self.rythme_long += 1.0 / 12.0;
    }

    /// Un drop à l'instant `t`, s'il est assez loin du précédent : la tension retombe.
    fn drop_permis(&mut self, t: f64) -> bool {
        if t - self.dernier_drop < DROP_ECART {
            return false;
        }
        self.dernier_drop = t;
        (self.tension, self.tension_max) = (0.0, 0.0);
        true
    }

    /// Un kick à l'instant `t` ; vrai si c'est le drop d'une montée.
    fn kick(&mut self, t: f64) -> bool {
        let apres_montee = t - self.dernier_kick >= 1.2 && self.tension_max > 0.45;
        self.dernier_kick = t;
        let drop = apres_montee && self.drop_permis(t);
        self.tension_max = 0.0;
        drop
    }

    /// `dt` secondes de son, de brillance et de niveau efficace donnés ; vrai sur un drop
    /// d'énergie (retour brutal après un passage calme).
    fn avancer(&mut self, t: f64, dt: f32, brillance: f32, rms: f32) -> bool {
        let k = |tau: f32| 1.0 - (-dt.max(0.0) / tau).exp();
        self.rythme_court *= 1.0 - k(1.0);
        self.rythme_long *= 1.0 - k(12.0);
        // silence : sous le seuil depuis 250 ms (un paquet muet entre deux coups n'en est pas un)
        self.muet = if !rms.is_finite() || rms < MUSIQUE_SILENCE { self.muet + dt } else { 0.0 };
        let silence = self.muet >= 0.25;
        let vise = if silence {
            self.amorcee = false;
            0.0
        } else {
            let b = if brillance.is_finite() { brillance.clamp(0.0, 1.0) } else { 0.5 };
            if !self.amorcee {
                (self.brillance_court, self.brillance_long, self.amorcee) = (b, b, true);
            }
            self.brillance_court += (b - self.brillance_court) * k(0.5);
            // amorçage : la première seconde de son, la moyenne longue suit la courte
            self.brillance_long = if self.entendu < 1.0 { self.brillance_court } else { self.brillance_long + (b - self.brillance_long) * k(8.0) };
            // accélération : bien plus de caisses claires que d'habitude (au moins 1,5 par seconde)
            let acceleration = ((self.rythme_court - self.rythme_long.max(1.5)) / 5.0).clamp(0.0, 1.0);
            let eclaircie = ((self.brillance_court - self.brillance_long) / 0.12).clamp(0.0, 1.0);
            (0.75 * acceleration + 0.45 * eclaircie).clamp(0.0, 1.0)
        };
        let tau = if vise > self.tension { 0.6 } else { 1.5 };
        self.tension = fini(self.tension + (vise - self.tension) * k(tau)).clamp(0.0, 1.0);
        self.tension_max = self.tension_max.max(self.tension);
        self.energie(t, dt, rms, silence, &k)
    }

    /// Intensité du passage (niveau court face au niveau long) ; vrai sur un drop d'énergie.
    fn energie(&mut self, t: f64, dt: f32, rms: f32, silence: bool, k: &impl Fn(f32) -> f32) -> bool {
        self.depuis_calme += dt;
        if silence {
            // le morceau suivant aura son propre niveau de référence
            self.entendu = 0.0;
            return false;
        }
        // puissance lissée sur ~85 ms (une fenêtre d'analyse), puis en dB
        if self.entendu == 0.0 {
            self.puissance = rms * rms;
        }
        self.puissance += (rms * rms - self.puissance) * k(0.085);
        let db = (10.0 * self.puissance.max(1e-12).log10()).max(-70.0);
        if self.entendu == 0.0 {
            (self.niveau_court, self.niveau_long) = (db, db);
        }
        self.niveau_court += (db - self.niveau_court) * k(0.5);
        self.entendu += dt;
        // amorçage : moyenne de tout ce qui a été entendu pendant les 12 premières secondes
        // (une moyenne glissante partie d'un mauvais début en garderait la trace longtemps)
        let poids = if self.entendu < 12.0 { dt / self.entendu } else { k(12.0) };
        self.niveau_long += (db - self.niveau_long) * poids;
        let intensite = (0.5 + (self.niveau_court - self.niveau_long) / (2.0 * INTENSITE_DB)).clamp(0.0, 1.0);
        if intensite < CALME {
            self.depuis_calme = 0.0;
        } else if intensite > FORT && self.depuis_calme < CALME_FENETRE && self.drop_permis(t) {
            self.depuis_calme = CALME_FENETRE;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{assainir, Aigus, AudioShared, Balance, Bandes, Kick, Musique, Reduction, BANDES_MAX};
    use audio_commun::capture::Sink;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    /// Batterie synthétique : kicks (sinus qui descend de 110 à 50 Hz), caisses claires (bruit
    /// et ton à 190 Hz, ~80 ms) et charleston en croches à 120 BPM, plus un bruit aigu dont le
    /// niveau suit `eclat(t)` (filtre qui s'ouvre pendant une montée).
    fn batterie(secs: f64, kicks: &[f64], caisses: &[f64], eclat: impl Fn(f64) -> f32) -> Vec<f32> {
        let mut g = 0x5851_f42d_4c95_7f2du64;
        let n = (secs * f64::from(FS)) as usize;
        // charleston et bruit de la montée : bruit dérivé deux fois (presque tout au-dessus de
        // 6 kHz, comme de vraies cymbales)
        let mut aigu: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(FS);
                let th = t % 0.25;
                let mut v = eclat(t) * 0.1 * bruit(&mut g);
                if th < 0.03 {
                    v += 0.08 * bruit(&mut g) * (-th * 120.0).exp() as f32;
                }
                v
            })
            .collect();
        for _ in 0..2 {
            for i in (1..aigu.len()).rev() {
                aigu[i] = 0.5 * (aigu[i] - aigu[i - 1]);
            }
        }
        let mut son: Vec<f32> = aigu.iter().map(|&a| a + 0.004 * bruit(&mut g)).collect();
        for &k in kicks {
            let debut = (k * f64::from(FS)) as usize;
            for (j, v) in son.iter_mut().skip(debut).take(FS as usize / 4).enumerate() {
                let b = j as f64 / f64::from(FS);
                let phase = 50.0 * b + 60.0 * (1.0 - (-b * 30.0).exp()) / 30.0;
                *v += ((std::f64::consts::TAU * phase).sin() * 0.9 * (b / 0.003).min(1.0) * (-b * 12.0).exp()) as f32;
            }
        }
        for &c in caisses {
            let debut = (c * f64::from(FS)) as usize;
            for (j, v) in son.iter_mut().skip(debut).take(FS as usize / 8).enumerate() {
                let b = j as f64 / f64::from(FS);
                let env = (b / 0.002).min(1.0) * (-b * 30.0).exp();
                *v += (env * (0.45 * f64::from(bruit(&mut g)) + 0.25 * (std::f64::consts::TAU * 190.0 * b).sin())) as f32;
            }
        }
        son
    }

    /// Kick avec l'analyse musicale voulue.
    fn kick_musique() -> (Arc<AudioShared>, Kick) {
        let shared = Arc::new(AudioShared::default());
        shared.set_musique(true);
        let mut k = Kick::new(shared.clone());
        k.open(FS);
        (shared, k)
    }

    /// Temps d'une mesure à 120 BPM à partir de `debut` jusqu'à `fin` : kicks sur chaque temps,
    /// caisses claires sur les temps 2 et 4.
    fn groove(debut: f64, fin: f64) -> (Vec<f64>, Vec<f64>) {
        let temps: Vec<f64> = (0..).map(|i| debut + i as f64 * 0.5).take_while(|&t| t < fin).collect();
        let caisses = temps.iter().enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, &t)| t).collect();
        (temps, caisses)
    }

    #[test]
    fn caisses_claires_trouvees_sans_celles_des_kicks() {
        let (shared, mut k) = kick_musique();
        let (kicks, caisses) = groove(0.3, 10.3);
        let v = ecouter(&mut k, &shared, &batterie(10.5, &kicks, &caisses, |_| 0.0));
        let attendu = caisses.len() as i64;
        assert!((i64::from(v.caisses) - attendu).abs() <= 2, "{} caisses claires pour {attendu} ({} kicks)", v.caisses, v.kicks);
        // kicks seuls : aucune caisse claire
        let (shared, mut k) = kick_musique();
        let v = ecouter(&mut k, &shared, &batterie(8.0, &groove(0.3, 7.8).0, &[], |_| 0.0));
        assert!(v.kicks >= 10, "{} kicks", v.kicks);
        assert!(v.caisses <= 1, "{} caisses claires sur des kicks seuls", v.caisses);
        // analyse non voulue : rien n'est compté
        let shared = Arc::new(AudioShared::default());
        let mut k = Kick::new(shared.clone());
        k.open(FS);
        let v = ecouter(&mut k, &shared, &batterie(4.0, &kicks, &caisses, |_| 0.0));
        assert_eq!((v.caisses, v.drops, v.tension), (0, 0, 0.0));
        assert!(k.musique.is_none());
    }

    #[test]
    fn groove_regulier_sans_montee_ni_drop() {
        let (shared, mut k) = kick_musique();
        let (kicks, caisses) = groove(0.3, 16.0);
        let son = batterie(16.0, &kicks, &caisses, |_| 0.0);
        let mut pire = 0f32;
        for paquet in son.chunks(FS as usize / 100) {
            k.samples(paquet);
            k.tick(std::time::Instant::now());
            pire = pire.max(shared.view().tension);
        }
        assert!(pire < 0.3, "tension {pire} sur un groove régulier");
        assert_eq!(shared.view().drops, 0);
    }

    #[test]
    fn montee_puis_drop() {
        let (shared, mut k) = kick_musique();
        // 8 s de groove, 6 s de montée sans kick (roulement de 4 à 16 caisses par seconde, son
        // qui s'éclaircit), puis le drop : le groove reprend
        let (mut kicks, mut caisses) = groove(0.3, 8.0);
        let mut t = 8.0;
        while t < 14.0 {
            caisses.push(t);
            t += 1.0 / (4.0 + 12.0 * (t - 8.0) / 6.0);
        }
        let (k2, c2) = groove(14.0, 18.0);
        kicks.extend(k2);
        caisses.extend(c2);
        let son = batterie(18.0, &kicks, &caisses, |t| if (8.0..14.0).contains(&t) { ((t - 8.0) / 6.0) as f32 } else { 0.0 });
        let (mut tension_montee, mut drops_avant) = (0f32, 0);
        for (i, paquet) in son.chunks(FS as usize / 100).enumerate() {
            k.samples(paquet);
            k.tick(std::time::Instant::now());
            let t = i as f64 * 0.01;
            if t < 13.95 {
                drops_avant = shared.view().drops;
            }
            if (12.5..13.95).contains(&t) {
                tension_montee = tension_montee.max(shared.view().tension);
            }
        }
        let v = shared.view();
        assert!(tension_montee > 0.5, "tension pendant la montée : {tension_montee}");
        assert_eq!(drops_avant, 0, "drop avant la fin de la montée");
        assert_eq!(v.drops, 1, "un drop à la reprise du groove");
        assert!(v.tension < 0.3, "tension après le drop : {}", v.tension);
    }

    /// Essai réel sur la sortie son par défaut : jouer un morceau pendant ces 24 s
    /// (`cargo test --release musique_reelle -- --ignored --nocapture`). Note chaque seconde
    /// kicks, caisses claires, drops, tension et équilibre gauche-droite.
    #[test]
    #[ignore]
    fn musique_reelle() {
        let shared = Arc::new(AudioShared::default());
        shared.set_musique(true);
        let _capture = super::start(shared.clone(), "", &[]).expect("capture");
        let mut tension_max = 0f32;
        for s in 0..240 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let v = shared.view();
            tension_max = tension_max.max(v.tension);
            if s % 10 == 9 {
                println!(
                    "{:>2} s : kicks {:>3}, caisses {:>3}, drops {}, tension {:.2} (max {:.2}), balance {:+.2}",
                    s / 10 + 1,
                    v.kicks,
                    v.caisses,
                    v.drops,
                    v.tension,
                    tension_max,
                    v.balance
                );
                tension_max = 0.0;
            }
        }
    }

    #[test]
    fn drop_d_energie_apres_un_passage_calme() {
        // 12 s de groove, 3 s de passage calme (le même, 16 dB plus bas), puis le groove revient
        // d'un coup : un drop, au retour seulement
        let (shared, mut k) = kick_musique();
        let (kicks, caisses) = groove(0.3, 12.0);
        let mut son = batterie(12.0, &kicks, &caisses, |_| 0.0);
        let (kicks_calmes, caisses_calmes) = groove(0.0, 3.0);
        son.extend(batterie(3.0, &kicks_calmes, &caisses_calmes, |_| 0.0).iter().map(|x| x * 0.15));
        let avant = ecouter(&mut k, &shared, &son);
        assert_eq!(avant.drops, 0);
        let (kicks, caisses) = groove(0.0, 4.0);
        let v = ecouter(&mut k, &shared, &batterie(4.0, &kicks, &caisses, |_| 0.0));
        assert_eq!(v.drops, 1);
    }

    #[test]
    fn balance_suit_le_cote_le_plus_fort() {
        let mut b = Balance::default();
        // coups tout à droite séparés de blancs (paquets muets) : l'équilibre va à droite (le
        // lissage ne compte que le temps où le son s'entend)
        for i in 0..500 {
            if i % 5 == 0 {
                b.paquet(0.0, 0.3, 0.01);
            } else {
                b.paquet(0.0, 0.0, 0.01);
            }
        }
        assert!(b.valeur > 0.85, "tout à droite : {}", b.valeur);
        for _ in 0..100 {
            b.paquet(0.3, 0.1, 0.01);
        }
        assert!((b.valeur + 0.5).abs() < 0.05, "plutôt à gauche : {}", b.valeur);
        // silence : rien pendant 0,5 s, puis retour au centre
        for _ in 0..40 {
            b.silence(0.01);
        }
        assert!((b.valeur + 0.5).abs() < 0.05, "{}", b.valeur);
        for _ in 0..300 {
            b.silence(0.01);
        }
        assert!(b.valeur.abs() < 0.01, "{}", b.valeur);
        b.paquet(f32::NAN, 0.1, 0.01);
        assert!(b.valeur.is_finite());
    }

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
        let mut k = Kick::new(shared.clone());
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
        let mut k = Kick::new(shared.clone());
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

    /// Morceau synthétique : charleston en croches (au tempo `bpm`) sur un léger bruit, et un
    /// kick (sinus qui descend de 110 à 50 Hz) à chaque instant de `kicks` (secondes).
    fn morceau(secs: f64, bpm: f64, kicks: &[f64]) -> Vec<f32> {
        let mut g = 0x2545_f491_4f6c_dd1du64;
        let n = (secs * f64::from(FS)) as usize;
        let croche = 30.0 / bpm;
        let mut son: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(FS);
                let th = t % croche;
                let mut v = 0.01 * bruit(&mut g);
                if th < 0.05 {
                    v += 0.12 * bruit(&mut g) * (-th * 80.0).exp() as f32;
                }
                v
            })
            .collect();
        for &k in kicks {
            let debut = (k * f64::from(FS)) as usize;
            for (j, v) in son.iter_mut().skip(debut).take(FS as usize / 4).enumerate() {
                let b = j as f64 / f64::from(FS);
                let phase = 50.0 * b + 60.0 * (1.0 - (-b * 30.0).exp()) / 30.0;
                *v += ((std::f64::consts::TAU * phase).sin() * 0.9 * (-b * 12.0).exp()) as f32;
            }
        }
        son
    }

    /// Passe `son` dans la vraie chaîne (paquets de 10 ms, comme WASAPI) et rend la vue finale.
    fn ecouter(k: &mut Kick, shared: &AudioShared, son: &[f32]) -> super::AudioView {
        for paquet in son.chunks(FS as usize / 100) {
            k.samples(paquet);
            k.tick(std::time::Instant::now());
        }
        shared.view()
    }

    #[test]
    fn tempo_trouve_sur_des_kicks_reguliers() {
        for bpm in [120.0, 128.0, 90.0] {
            let shared = Arc::new(AudioShared::default());
            let mut k = Kick::new(shared.clone());
            k.open(FS);
            let kicks: Vec<f64> = (0..).map(|i| 0.3 + i as f64 * 60.0 / bpm).take_while(|&t| t < 11.5).collect();
            let v = ecouter(&mut k, &shared, &morceau(12.0, bpm, &kicks));
            let trouve = v.bpm.unwrap_or_else(|| panic!("{bpm} BPM : aucun tempo ({} kicks)", v.kicks));
            assert!((f64::from(trouve) - bpm).abs() <= 1.0, "{bpm} BPM → {trouve}");
            assert!(!v.relais, "kicks en cours : pas de relais");
            // break : la musique continue sans kick → relais au même tempo
            let v = ecouter(&mut k, &shared, &morceau(4.0, bpm, &[]));
            assert!(v.relais && v.bpm == Some(trouve), "{bpm} BPM : relais {} tempo {:?}", v.relais, v.bpm);
            // silence (paquets de 10 ms) : plus de relais après 0,3 s, tempo oublié après 3 s
            for _ in 0..50 {
                k.silence(FS as usize / 100);
            }
            assert!(!shared.view().relais);
            for _ in 0..300 {
                k.silence(FS as usize / 100);
            }
            assert_eq!(shared.view().bpm, None);
        }
    }

    #[test]
    fn aucun_tempo_sur_des_kicks_irreguliers_ni_en_silence() {
        let shared = Arc::new(AudioShared::default());
        let mut k = Kick::new(shared.clone());
        k.open(FS);
        let mut g = 0x9e37_79b9_7f4a_7c15u64;
        let mut kicks = Vec::new();
        let mut t = 0.3;
        while t < 15.0 {
            kicks.push(t);
            t += 0.3 + 0.35 * (f64::from(bruit(&mut g)) + 1.0);
        }
        for paquet in morceau(15.5, 120.0, &kicks).chunks(FS as usize / 100) {
            k.samples(paquet);
            k.tick(std::time::Instant::now());
            assert_eq!(shared.view().bpm, None, "faux tempo sur des kicks irréguliers");
        }
        assert!(shared.view().kicks >= 10, "{} kicks", shared.view().kicks);
        // silence seul
        let shared = Arc::new(AudioShared::default());
        let mut k = Kick::new(shared.clone());
        k.open(FS);
        for _ in 0..200 {
            k.samples(&[0.0; 480]);
            k.silence(480);
        }
        let v = shared.view();
        assert_eq!((v.bpm, v.relais), (None, false));
    }

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
        let mut k = Kick::new(shared.clone());
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

    #[test]
    fn son_tel_quel_jusqu_a_48_khz() {
        for fs in [44_100, 48_000, 32_000] {
            let mut r = Reduction::new(fs);
            assert_eq!(r.fs_sortie(), fs);
            let son = sinus(1000.0, 480);
            let mut tampon = Vec::new();
            // aucun calcul : la même tranche revient
            assert_eq!(r.reduire(&son, &mut tampon).as_ptr(), son.as_ptr());
            assert_eq!(r.silence(37), 37);
        }
    }

    /// Amplitude crête de la seconde moitié de `x` (filtres établis).
    fn crete(x: &[f32]) -> f32 {
        x[x.len() / 2..].iter().fold(0.0, |c, v| c.max(v.abs()))
    }

    #[test]
    fn son_ramene_a_48_khz_au_dessus() {
        for (fs, sortie) in [(96_000u32, 48_000u32), (88_200, 44_100), (192_000, 48_000), (176_400, 44_100)] {
            let sin = |f: f32| -> Vec<f32> {
                (0..fs as usize / 5).map(|i| 0.5 * (std::f32::consts::TAU * f * i as f32 / fs as f32).sin()).collect()
            };
            let mut r = Reduction::new(fs);
            assert_eq!(r.fs_sortie(), sortie);
            let mut tampon = Vec::new();
            // 1 kHz passe intact, un échantillon gardé sur fs / sortie
            let bas = r.reduire(&sin(1000.0), &mut tampon).to_vec();
            assert_eq!(bas.len(), sortie as usize / 5);
            assert!((crete(&bas) - 0.5).abs() < 0.01, "{fs} Hz : 1 kHz à {}", crete(&bas));
            // au-delà de la moitié de la sortie : coupé, ne revient pas replié dans les aigus
            let mut r = Reduction::new(fs);
            let haut = r.reduire(&sin(0.85 * sortie as f32), &mut tampon).to_vec();
            assert!(crete(&haut) < 0.02, "{fs} Hz : {} Hz replié à {}", 0.85 * sortie as f32, crete(&haut));
        }
        // silence : compté à la fréquence de sortie, sans perdre les restes d'un paquet à l'autre
        let mut r = Reduction::new(96_000);
        let mut tampon = Vec::new();
        assert_eq!(r.reduire(&[0.1; 3], &mut tampon).len(), 1);
        assert_eq!(r.silence(3), 2);
        assert_eq!(r.silence(1), 0);
        assert_eq!(r.silence(1), 1);
    }

    #[test]
    fn charleston_et_bandes_a_96_khz_comme_a_48() {
        // la même batterie à 48 et 96 kHz : autant d'attaques d'aigus, la même bande la plus forte
        let mut resultats = Vec::new();
        for fs in [48_000u32, 96_000] {
            let mut g = 0x9e37_79b9_7f4a_7c15u64;
            let n = fs as usize * 4;
            let son: Vec<f32> = (0..n)
                .map(|i| {
                    let basse = 0.6 * (std::f32::consts::TAU * 55.0 * i as f32 / fs as f32).sin();
                    basse + if i % (fs as usize / 4) < fs as usize * 3 / 100 { 0.25 * bruit(&mut g) } else { 0.0 }
                })
                .collect();
            let shared = Arc::new(AudioShared::default());
            shared.set_musique(true);
            shared.set_bandes(16);
            let mut k = Kick::new(shared.clone());
            k.open(fs);
            for paquet in son.chunks(fs as usize / 100 + 1) {
                k.samples(paquet);
                k.tick(std::time::Instant::now());
            }
            k.silence(7);
            let mut sortie = [0f32; BANDES_MAX];
            shared.bandes(&mut sortie);
            let forte = (0..16).max_by(|&i, &j| sortie[i].total_cmp(&sortie[j])).unwrap();
            let v = shared.view();
            assert!(v.tension.is_finite() && v.brillance.is_finite());
            resultats.push((v.aigus, forte));
        }
        let [(a48, b48), (a96, b96)] = resultats[..] else { unreachable!() };
        assert!((14..=17).contains(&a48) && (14..=17).contains(&a96), "{a48} et {a96} attaques");
        assert_eq!(b48, b96, "bande la plus forte");
    }

    #[test]
    fn paquet_vide_sans_division_par_zero() {
        let mut m = Musique::new(FS, 0, 0);
        m.ecouter(Some(&[]), 0, None);
        m.ecouter(None, 0, None);
        assert!(m.montee.niveau_long.is_finite() && m.montee.niveau_court.is_finite());
        assert_eq!(m.t, 0.0);
        // la suite reste juste
        m.ecouter(Some(&sinus(1000.0, 480)), 480, None);
        assert!(m.montee.niveau_long.is_finite() && m.montee.niveau_long > -70.0);
    }

    #[test]
    fn repli_des_applis_publie_puis_efface() {
        let shared = Arc::new(AudioShared::default());
        let mut k = Kick::new(shared.clone());
        assert!(!shared.repli_applis());
        k.repli(true);
        assert!(shared.repli_applis());
        k.repli(false);
        assert!(!shared.repli_applis());
        // capture arrêtée : plus de repli annoncé
        k.repli(true);
        shared.reset();
        assert!(!shared.repli_applis());
        // nouvelle capture : le repli resté levé par la précédente est effacé
        let mut k = Kick::new(shared.clone());
        k.repli(true);
        let _nouvelle = Kick::new(shared.clone());
        assert!(!shared.repli_applis());
    }

    #[test]
    fn une_capture_remplacee_n_ecrit_plus_rien() {
        let shared = Arc::new(AudioShared::default());
        let mut ancienne = Kick::new(shared.clone());
        ancienne.open(FS);
        ancienne.an.as_mut().unwrap().kicks = 5;
        ancienne.tick(std::time::Instant::now());
        assert_eq!(shared.view().kicks, 5);
        // nouvelle capture (fil de l'ancienne détaché, encore en vie)
        let mut nouvelle = Kick::new(shared.clone());
        nouvelle.open(FS);
        ancienne.an.as_mut().unwrap().kicks = 9;
        ancienne.tick(std::time::Instant::now());
        ancienne.samples(&sinus(1000.0, 4800));
        ancienne.stereo(0.0, 0.5);
        ancienne.lost();
        ancienne.repli(true);
        let v = shared.view();
        assert_eq!((v.kicks, v.balance, v.brillance), (5, 0.0, 0.0));
        assert!(!shared.perdu() && !shared.repli_applis());
        // la nouvelle écrit, et ses compteurs continuent ceux d'avant
        nouvelle.an.as_mut().unwrap().kicks = 2;
        nouvelle.tick(std::time::Instant::now());
        assert_eq!(shared.view().kicks, 7);
        nouvelle.lost();
        assert!(shared.perdu());
        // arrêt : plus rien de la nouvelle non plus
        shared.reset();
        nouvelle.lost();
        nouvelle.repli(true);
        assert!(!shared.perdu() && !shared.repli_applis());
    }

    #[test]
    fn flux_rouvert_a_la_meme_frequence_garde_le_tempo() {
        let shared = Arc::new(AudioShared::default());
        let mut k = Kick::new(shared.clone());
        k.open(FS);
        let kicks: Vec<f64> = (0..).map(|i| 0.3 + i as f64 * 0.5).take_while(|&t| t < 11.5).collect();
        let tempo = ecouter(&mut k, &shared, &morceau(12.0, 120.0, &kicks)).bpm.expect("tempo");
        // coupure brève (erreur), rouvert à la même fréquence : tempo et horloge continuent
        let t_avant = k.an.as_ref().unwrap().t;
        k.lost();
        std::thread::sleep(std::time::Duration::from_millis(200));
        k.open(FS);
        assert!(!shared.perdu());
        let t_apres = k.an.as_ref().unwrap().t;
        assert!(t_apres - t_avant >= 0.19, "coupure comptée comme silence : {t_avant} → {t_apres}");
        k.samples(&[0.0; 480]);
        assert_eq!(shared.view().bpm, Some(tempo));
        // autre fréquence : analyse refaite, tempo à retrouver
        k.open(44_100);
        k.samples(&[0.0; 441]);
        assert_eq!(shared.view().bpm, None);
        assert_eq!(k.an.as_ref().unwrap().kicks, 0);
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

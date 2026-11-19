//! Simulation et dessin : le point, les deux comètes, l'essaim de satellites et les fissures.
//!
//! Règles (« Essaim durable ») : chaque kick inverse le sens de rotation et ajoute des
//! satellites ; la quantité de basse règle la vitesse ; plus la note de basse est grave, plus
//! les orbites s'éloignent. Les traînées sont gardées en coordonnées écran : quand la souris
//! bouge, elles s'étirent derrière elle (la fenêtre s'agrandit le temps du mouvement).
//!
//! Au-delà du kick (`[musique]`) : pendant une montée l'essaim se resserre et accélère, puis
//! explose au drop ; un carré s'ouvre sur chaque caisse claire ; les têtes pulsent sur les
//! contretemps ; l'essaim penche vers le côté qui joue le plus fort. Tout est reproduit dans
//! `ui/apercu.js`.

use crate::audio::AudioView;
use crate::config::{Config, Forme};
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use crate::canvas::Canvas;

/// Marge autour de tout ce qui est dessiné, décalage maximal du centre lissé, et étendue
/// maximale de la fenêtre (une traînée plus longue que ça est coupée).
const MARGIN: f32 = 12.0;
const CENTER_LAG_MAX: f32 = 30.0;
const MAX_EXTENT: f32 = 2400.0;
const SAT_R_MAX: f32 = 1.9;
const SAT_TRAIL: f64 = 0.25;
const TRAIL_STEP: f64 = 1.0 / 90.0;
/// Finesse des traînées (angle maximal en radians, écart maximal en px entre deux points) :
/// les comètes, larges et longues, sont lissées de près ; les satellites, fins et courts, moins.
const FINESSE_COMETES: (f32, f32) = (0.045, 4.0);
const FINESSE_SATELLITES: (f32, f32) = (0.1, 8.0);

/// Pas intermédiaires pour un déplacement d'angle `da` sur un rayon de `r` px, pour que la
/// traînée reste une courbe (et non une ligne brisée) quand l'essaim tourne vite.
fn sous_pas(da: f32, r: f32, (angle, ecart): (f32, f32)) -> usize {
    (da.abs() / angle).max(da.abs() * r / ecart).ceil().clamp(1.0, 24.0) as usize
}

/// Petit générateur pseudo-aléatoire (xorshift), suffisant pour des effets visuels.
pub struct Rng(u64);
impl Rng {
    pub fn seeded() -> Self {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        Self(t | 1)
    }
    pub fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + self.f() * (b - a)
    }
}

/// Teinte de base imposée par la synchro avec l'éclairage (bits d'un f32 ; NaN = aucune).
static SYNC_HUE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x7FC0_0000);

/// Décalage de teinte (degrés) suivant la musique (`[couleur] musique`), lissé par la boucle.
static DECALAGE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_decalage(d: f32) {
    if !d.is_finite() {
        return; // un NaN y resterait pour toujours (le lissage repart de la valeur stockée)
    }
    DECALAGE.store(d.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

pub fn decalage() -> f32 {
    f32::from_bits(DECALAGE.load(std::sync::atomic::Ordering::Relaxed))
}

/// Décalage visé : jusqu'à 160° quand la basse est forte et la musique brillante.
pub fn decalage_vise(au: &AudioView) -> f32 {
    160.0 * (0.6 * au.bass.clamp(0.0, 1.0) + 0.4 * au.brillance.clamp(0.0, 1.0))
}

/// Trajet dessiné à la main, rééchantillonné à pas égaux (vide : cercle).
static TRACE: std::sync::RwLock<Vec<(f32, f32)>> = std::sync::RwLock::new(Vec::new());

/// Remplace le trajet dessiné : points rééchantillonnés à 256 pas égaux le long de la courbe
/// fermée, centrés, et mis à l'échelle (rayon le plus grand : 1,2).
pub fn set_trace(points: &[[f32; 2]]) {
    let pts = reechantillonner(points);
    if let Ok(mut t) = TRACE.write() {
        *t = pts;
    }
}

fn reechantillonner(points: &[[f32; 2]]) -> Vec<(f32, f32)> {
    if points.len() < 3 {
        return Vec::new();
    }
    let p: Vec<(f32, f32)> = points.iter().filter(|q| q[0].is_finite() && q[1].is_finite()).take(2000).map(|q| (q[0], q[1])).collect();
    let n = p.len();
    if n < 3 {
        return Vec::new();
    }
    let long = |i: usize| {
        let (a, b) = (p[i], p[(i + 1) % n]);
        ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
    };
    let total: f32 = (0..n).map(long).sum();
    if total <= 1e-6 {
        return Vec::new();
    }
    let (mut out, mut i, mut fait) = (Vec::with_capacity(256), 0usize, 0f32);
    for k in 0..256 {
        let cible = total * k as f32 / 256.0;
        while i < n - 1 && fait + long(i) < cible {
            fait += long(i);
            i += 1;
        }
        let f = if long(i) > 0.0 { ((cible - fait) / long(i)).clamp(0.0, 1.0) } else { 0.0 };
        let (a, b) = (p[i], p[(i + 1) % n]);
        out.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
    }
    let (cx, cy) = out.iter().fold((0.0, 0.0), |s, q| (s.0 + q.0 / 256.0, s.1 + q.1 / 256.0));
    let r = out.iter().map(|q| ((q.0 - cx).powi(2) + (q.1 - cy).powi(2)).sqrt()).fold(0.0f32, f32::max).max(1e-6);
    out.iter().map(|q| ((q.0 - cx) / r * 1.2, (q.1 - cy) / r * 1.2)).collect()
}

fn trace(a: f32) -> (f32, f32) {
    let Ok(t) = TRACE.read() else { return (a.cos(), a.sin()) };
    if t.len() < 3 {
        return (a.cos(), a.sin());
    }
    let n = t.len() as f32;
    let u = (a / TAU * n).rem_euclid(n);
    let (i, f) = (u.floor() as usize, u.fract());
    let (p, q) = (t[i % t.len()], t[(i + 1) % t.len()]);
    (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f)
}

/// Teinte de base imposée en ce moment (application, Prisme), en degrés.
pub fn sync_hue() -> Option<f32> {
    Some(f32::from_bits(SYNC_HUE.load(std::sync::atomic::Ordering::Relaxed))).filter(|h| h.is_finite())
}

/// Impose (ou retire) la teinte de base, en degrés.
pub fn set_sync_hue(h: Option<f32>) {
    SYNC_HUE.store(h.unwrap_or(f32::NAN).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Teinte « Prisme » : `k` = 0 en queue de traînée, 1 en tête. Base : celle de l'éclairage si
/// la synchro en fournit une, sinon `teinte` qui tourne avec le temps (fixe à vitesse nulle).
pub fn prisme(cfg: &Config, t: f64, k: f32) -> (u8, u8, u8) {
    prisme_decale(cfg, t, k, 0.0)
}

/// Teinte `j` de la palette (`[couleur] palette`, 1 à 3 teintes) : celle de `prisme`, décalée de
/// `j` fois `ecart_palette` degrés (les comètes et satellites se partagent la palette).
pub fn prisme_palette(cfg: &Config, t: f64, k: f32, j: usize) -> (u8, u8, u8) {
    let n = cfg.couleur.palette.clamp(1, 3) as usize;
    prisme_decale(cfg, t, k, (j % n) as f32 * cfg.couleur.ecart_palette)
}

fn prisme_decale(cfg: &Config, t: f64, k: f32, decale: f32) -> (u8, u8, u8) {
    let synced = f32::from_bits(SYNC_HUE.load(std::sync::atomic::Ordering::Relaxed));
    let base = if synced.is_finite() { synced } else { (cfg.couleur.teinte + t as f32 * cfg.couleur.vitesse_teinte).rem_euclid(360.0) };
    let base = if cfg.couleur.musique { base + decalage() } else { base };
    let base = base + if decale.is_finite() { decale } else { 0.0 };
    let tail = base + cfg.couleur.ecart_teinte;
    let mut dh = base - tail;
    if dh > 180.0 {
        dh -= 360.0;
    }
    if dh < -180.0 {
        dh += 360.0;
    }
    hsl((tail + dh * k).rem_euclid(360.0), 1.0, 0.66)
}

fn hsl(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (to(r), to(g), to(b))
}

/// Points de traînée : (instant, x, y) en coordonnées écran.
type Trail = VecDeque<(f64, f32, f32)>;

/// Ajoute la position courante. Le dernier point suit la tête à chaque image ; un nouveau point
/// fixe n'est ajouté que tous les `TRAIL_STEP` ou tous les `gap` pixels, mesurés depuis
/// l'avant-dernier (le dernier, lui, est sans cesse rafraîchi) : peu de points au ralenti, assez
/// pour une courbe lisse à grande vitesse.
fn push_trail(tr: &mut Trail, t: f64, x: f32, y: f32, life: f64, gap: f32) {
    let n = tr.len();
    let proche = |p: &(f64, f32, f32)| t - p.0 < TRAIL_STEP && (p.1 - x).powi(2) + (p.2 - y).powi(2) < gap * gap;
    if n >= 2 && proche(&tr[n - 2]) {
        tr[n - 1] = (t, x, y);
    } else {
        tr.push_back((t, x, y));
    }
    while tr.front().is_some_and(|p| t - p.0 > life) {
        tr.pop_front();
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Genre {
    /// Étincelle (aigus, éclats de clic) : point vif qui ralentit et s'éteint.
    Etincelle,
    /// Onde de choc d'un clic : anneau qui s'élargit (`taille` = rayon final).
    Onde,
    /// Poussière du sillage de la souris.
    Poussiere,
    /// Caisse claire : carré (posé sur la pointe) qui s'ouvre sous le pointeur.
    Carre,
    /// Clic droit : anneau qui se referme sur le point du clic.
    Implosion,
}

#[derive(Clone, Copy)]
struct Particule {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    ne: f64,
    vie: f32,
    taille: f32,
    genre: Genre,
}

impl Particule {
    /// Demi-côté du carré qui contient ce qui est dessiné.
    fn portee(&self) -> f32 {
        match self.genre {
            Genre::Onde | Genre::Carre | Genre::Implosion => self.taille + 4.0,
            _ => self.taille * 2.5,
        }
    }
}

/// Une particule qui traverse un bord de la fenêtre au premier plan (`murs` : gauche, haut,
/// droite, bas) entre (x0, y0) et sa position rebondit dessus, en perdant un peu de vitesse.
fn rebondir(p: &mut Particule, x0: f32, y0: f32, (l, h, r, b): (f32, f32, f32, f32)) {
    const REBOND: f32 = 0.7;
    for bord in [l, r] {
        if (x0 - bord) * (p.x - bord) < 0.0 && (h..=b).contains(&p.y) {
            p.x = 2.0 * bord - p.x;
            p.vx = -p.vx * REBOND;
        }
    }
    for bord in [h, b] {
        if (y0 - bord) * (p.y - bord) < 0.0 && (l..=r).contains(&p.x) {
            p.y = 2.0 * bord - p.y;
            p.vy = -p.vy * REBOND;
        }
    }
}

/// Liens de la constellation (`[essaim] constellation`) : paires de têtes `(x, y)` plus proches
/// que `portee`, avec leur force (1 tout près, 0 à la portée) ; au plus `LIENS_MAX`.
fn liens(tetes: &[(f32, f32)], portee: f32, sortie: &mut Vec<(usize, usize, f32)>) {
    sortie.clear();
    if !portee.is_finite() || portee <= 0.0 {
        return;
    }
    for i in 0..tetes.len() {
        for j in i + 1..tetes.len() {
            let d = ((tetes[i].0 - tetes[j].0).powi(2) + (tetes[i].1 - tetes[j].1).powi(2)).sqrt();
            if d < portee {
                sortie.push((i, j, 1.0 - d / portee));
                if sortie.len() >= LIENS_MAX {
                    return;
                }
            }
        }
    }
}

/// Liens de la constellation dessinés au plus, et leur portée (en rayons de l'orbite).
const LIENS_MAX: usize = 160;
const LIENS_PORTEE: f32 = 1.1;
/// Carré d'une caisse claire : demi-diagonale (px à 100 %) et durée (s).
const CARRE_CAISSE: (f32, f32) = (30.0, 0.32);
/// Drop : onde (px à 100 %, durée en s) et étincelles projetées.
const ONDE_DROP: (f32, f32) = (130.0, 0.9);
const ETINCELLES_DROP: usize = 28;
/// Mode sans flashs : au plus un éclat tous les `ECART_FLASH` secondes (trois par seconde), à
/// la moitié de sa force.
const ECART_FLASH: f64 = 1.0 / 3.0;

/// Particules vivantes au plus (au-delà, les plus vieilles s'en vont).
const MAX_PARTICULES: usize = 400;

struct Sat {
    a: f32,
    r: f32,
    mul: f32,
    born: f64,
    trail: Trail,
    /// Teinte de la palette (donnée à la naissance : elle ne change pas quand d'autres meurent).
    couleur: usize,
}

/// Point du trajet des comètes à l'angle `a`, pour un rayon de 1 (y vers le bas).
pub fn chemin(forme: Forme, a: f32) -> (f32, f32) {
    // polygone de k sommets, premier sommet à l'angle `depart`, rayons alternés `r` et `r2`
    // (égaux : régulier), parcouru à vitesse constante
    let poly_depuis = |k: f32, r: f32, r2: f32, depart: f32| {
        let u = (a / TAU * k).rem_euclid(k);
        let (j, f) = (u.floor(), u.fract());
        let v = |j: f32| {
            let th = depart + j * TAU / k;
            let rr = if j as i32 % 2 == 0 { r } else { r2 };
            (th.cos() * rr, th.sin() * rr)
        };
        let (p, q) = (v(j), v(j + 1.0));
        (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f)
    };
    let poly = |k: f32, r: f32, r2: f32| poly_depuis(k, r, r2, -PI / 2.0);
    match forme {
        Forme::Cercle => (a.cos(), a.sin()),
        Forme::Triangle => poly(3.0, 1.2, 1.2),
        Forme::Losange => poly(4.0, 1.15, 1.15),
        Forme::Hexagone => poly(6.0, 1.05, 1.05),
        Forme::Huit => (1.2 * a.cos(), 0.6 * (2.0 * a).sin()),
        Forme::Etoile => poly(10.0, 1.25, 0.55),
        Forme::Fleur => {
            let r = 0.95 + 0.3 * (5.0 * a).cos();
            (a.cos() * r, a.sin() * r)
        }
        // carré posé à plat (le losange est sur la pointe)
        Forme::Carre => poly_depuis(4.0, 1.25, 1.25, -3.0 * PI / 4.0),
        Forme::Pentagone => poly(5.0, 1.1, 1.1),
        Forme::Ellipse => (1.3 * a.cos(), 0.7 * a.sin()),
        // cœur classique (16 sin³, 13 cos − 5 cos 2 − 2 cos 3 − cos 4), pointe en bas
        Forme::Coeur => {
            let (s, c) = a.sin_cos();
            let y = 13.0 * c - 5.0 * (2.0 * a).cos() - 2.0 * (3.0 * a).cos() - (4.0 * a).cos();
            (16.0 * s * s * s / 15.0, -(y + 2.5) / 15.0)
        }
        Forme::Trefle => {
            let r = 0.75 + 0.45 * (3.0 * a).cos();
            (a.cos() * r, a.sin() * r)
        }
        Forme::Lissajous => (0.9 * (3.0 * a).cos(), 0.85 * (2.0 * a).sin()),
        // astroïde : étoile à quatre branches courbes
        Forme::Astroide => (1.3 * a.cos().powi(3), 1.3 * a.sin().powi(3)),
        Forme::Trace => trace(a),
    }
}

/// Éclat de la pulsation au tempo pendant un relais (break sans kick) : `FORCE_PULSATION` sur
/// le temps, retombée rapide ensuite (comme un petit kick). Reproduit dans `ui/apercu.js`.
const FORCE_PULSATION: f32 = 0.35;
fn pulsation_tempo(phase: f32) -> f32 {
    FORCE_PULSATION * (1.0 - phase.clamp(0.0, 1.0)).powi(4)
}

/// Nouveaux événements depuis le compteur `vu` (kicks, aigus), au plus 4 par image. Un compteur
/// qui recule (analyse rouverte) n'en compte aucun, au lieu d'un saut énorme.
fn nouveaux(vu: Option<u32>, compteur: u32) -> u32 {
    let d = vu.map_or(0, |v| compteur.wrapping_sub(v));
    if d > u32::MAX / 2 {
        0
    } else {
        d.min(4)
    }
}

/// Pas de simulation le plus long : au-delà (cadence basse, machine chargée), l'image est
/// simulée en plusieurs pas, sinon l'essaim ralentirait sous 20 images par seconde.
pub const PAS_MAX: f32 = 0.05;
/// Écart le plus long rattrapé (retour de veille, de pause) : au-delà, le temps perdu est oublié.
const RATTRAPAGE_MAX: f32 = 0.25;

/// Rayon (relatif) et vitesse (relative) de l'anneau `k` : les anneaux extérieurs, plus
/// larges, tournent moins vite.
fn anneau(k: usize) -> (f32, f32) {
    (1.0 + 0.55 * k as f32, 1.0 / (1.0 + 0.45 * k as f32))
}

pub struct Swarm {
    rng: Rng,
    dir: f32,
    spd: f32,
    far: f32,
    /// Angle de chaque anneau.
    angles: Vec<f32>,
    /// Rotation du trajet lui-même.
    rot: f32,
    /// Traînées des comètes, anneau par anneau.
    trails: Vec<Trail>,
    sats: Vec<Sat>,
    /// Satellites permanents (`[essaim] fixes`).
    fixes: Vec<Sat>,
    kicks_seen: Option<u32>,
    aigus_vus: Option<u32>,
    caisses_vues: Option<u32>,
    drops_vus: Option<u32>,
    /// Montée suivie (0 à 1), lissée.
    tension: f32,
    /// Pulsation du dernier contretemps (1 → 0), et phase du temps à l'image précédente.
    contre: f32,
    phase_avant: Option<f32>,
    /// Vitesse du centre de l'orbite (px/s), pour l'inertie.
    vitesse_centre: (f32, f32),
    /// Teinte de la palette du prochain satellite.
    prochaine_couleur: usize,
    /// Bords de la fenêtre au premier plan (gauche, haut, droite, bas) : les particules y
    /// rebondissent (`[scintillement] rebonds`).
    pub murs: Option<(f32, f32, f32, f32)>,
    /// Mode sans flashs (photosensibilité), et instant du dernier éclat.
    pub doux: bool,
    dernier_flash: f64,
    particules: Vec<Particule>,
    dernier_curseur: Option<(f32, f32)>,
    center: Option<(f32, f32)>,
    pitch: f32,
    /// Éclat du dernier kick, 1 → 0.
    pub env: f32,
    /// Têtes des traînées (étincelles des aigus), tampon gardé d'une image à l'autre.
    tetes: Vec<(f32, f32)>,
    /// Tampons du dessin, gardés d'une image à l'autre (le dessin ne prend que `&self`).
    tampons: std::cell::RefCell<Tampons>,
}

/// Tête d'une traînée à dessiner.
struct Head {
    x: f32,
    y: f32,
    s: f32,
    al: f32,
    front: bool,
    rgb: (u8, u8, u8),
}

#[derive(Default)]
struct Tampons {
    heads: Vec<Head>,
    xy: Vec<(f32, f32)>,
    /// Constellation : positions des têtes et liens.
    points: Vec<(f32, f32)>,
    liens: Vec<(usize, usize, f32)>,
}

impl Swarm {
    pub fn new() -> Self {
        Self {
            rng: Rng::seeded(),
            dir: 1.0,
            spd: 3.5,
            far: 1.0,
            angles: Vec::new(),
            rot: 0.0,
            trails: Vec::new(),
            sats: Vec::new(),
            fixes: Vec::new(),
            aigus_vus: None,
            caisses_vues: None,
            drops_vus: None,
            tension: 0.0,
            contre: 0.0,
            phase_avant: None,
            vitesse_centre: (0.0, 0.0),
            prochaine_couleur: 0,
            murs: None,
            doux: false,
            dernier_flash: f64::NEG_INFINITY,
            particules: Vec::new(),
            dernier_curseur: None,
            kicks_seen: None,
            center: None,
            pitch: 50.0,
            env: 0.0,
            tetes: Vec::new(),
            tampons: Default::default(),
        }
    }

    /// Rectangle écran (x0, y0, x1, y1) qui contient tout ce qui sera dessiné : traînées, têtes
    /// et éclat du kick (de taille `eclat`). Borné à `MAX_EXTENT` autour du pointeur.
    pub fn bounds(&self, cursor: (f32, f32), scale: f32, eclat: f32) -> (f32, f32, f32, f32) {
        // anneau de l'éclat : jusqu'à 7,5 fois sa taille, plus son épaisseur
        let ring = ((eclat * 7.5 + 1.5) * scale - (2.4 * 2.6 + 3.5 * 2.6 + MARGIN) * scale).max(0.0);
        let mut b = (cursor.0 - ring, cursor.1 - ring, cursor.0 + ring, cursor.1 + ring);
        for p in &self.particules {
            let e = p.portee();
            b = (b.0.min(p.x - e), b.1.min(p.y - e), b.2.max(p.x + e), b.3.max(p.y + e));
        }
        let pts = self.trails.iter().chain(self.sats.iter().chain(&self.fixes).map(|s| &s.trail)).flat_map(|tr| tr.iter());
        for &(_, x, y) in pts {
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
        // halo des comètes (le plus large) + marge. Pas de rognage : tout ce qui est dessiné doit
        // être dans ce rectangle, sinon ce qui en sort n'est jamais effacé (les points trop
        // loin du pointeur sont retirés des traînées par `update`).
        let pad = (2.4 * 2.6 + 3.5 * 2.6 + MARGIN) * scale;
        (b.0 - pad, b.1 - pad, b.2 + pad, b.3 + pad)
    }

    /// Avance la simulation jusqu'à `t`, `dt` secondes après l'image précédente : en plusieurs
    /// pas d'au plus `PAS_MAX` si besoin, le pointeur glissant de sa position précédente à
    /// `cursor` (le sillage voit ainsi la vraie vitesse).
    pub fn avance(&mut self, t: f64, dt: f32, cursor: (f32, f32), au: AudioView, cfg: &Config, scale: f32) {
        let dt = if dt.is_finite() { dt.clamp(0.0005, RATTRAPAGE_MAX) } else { 0.0005 };
        let n = (dt / PAS_MAX).ceil().max(1.0) as usize;
        let pas = dt / n as f32;
        let depart = self.dernier_curseur.unwrap_or(cursor);
        for i in 1..=n {
            let f = i as f32 / n as f32;
            let c = (depart.0 + (cursor.0 - depart.0) * f, depart.1 + (cursor.1 - depart.1) * f);
            self.update(t - (dt * (1.0 - f)) as f64, pas, c, au, cfg, scale);
        }
    }

    pub fn update(&mut self, t: f64, dt: f32, cursor: (f32, f32), au: AudioView, cfg: &Config, scale: f32) {
        // centre de l'orbite : suit le pointeur, décalé vers le côté qui joue le plus fort
        // (`[musique] stereo`), de près ou en ressort souple (`[orbite] inertie`), retard borné
        let penche = au.balance.clamp(-1.0, 1.0) * cfg.musique.stereo.clamp(0.0, 1.0) * cfg.orbite.rayon * 1.6 * scale;
        let cible = (cursor.0 + if penche.is_finite() { penche } else { 0.0 }, cursor.1);
        let (mut cx, mut cy) = self.center.unwrap_or(cible);
        let (c0, far0) = ((cx, cy), self.far);
        let inertie = cfg.orbite.inertie.clamp(0.0, 1.0);
        if inertie > 0.0 {
            // ressort amorti : pulsation de 14 à 4,9 rad/s, amortissement 0,45 (dépasse un peu)
            let w = 14.0 * (1.0 - 0.65 * inertie);
            let (vx, vy) = self.vitesse_centre;
            let vx = vx + (w * w * (cible.0 - cx) - 0.9 * w * vx) * dt;
            let vy = vy + (w * w * (cible.1 - cy) - 0.9 * w * vy) * dt;
            self.vitesse_centre = if vx.is_finite() && vy.is_finite() { (vx, vy) } else { (0.0, 0.0) };
            cx += self.vitesse_centre.0 * dt;
            cy += self.vitesse_centre.1 * dt;
        } else {
            self.vitesse_centre = (0.0, 0.0);
            let k = 1.0 - (-dt * 14.0).exp();
            cx += (cible.0 - cx) * k;
            cy += (cible.1 - cy) * k;
        }
        let (ox, oy) = (cx - cible.0, cy - cible.1);
        let d = (ox * ox + oy * oy).sqrt();
        let lim = CENTER_LAG_MAX * scale * (1.0 + 5.0 * inertie);
        if d > lim {
            cx = cible.0 + ox / d * lim;
            cy = cible.1 + oy / d * lim;
            // en butée : la vitesse qui éloignait le centre est perdue
            let (ux, uy) = (ox / d, oy / d);
            let (vx, vy) = self.vitesse_centre;
            let fuite = (vx * ux + vy * uy).max(0.0);
            self.vitesse_centre = (vx - fuite * ux, vy - fuite * uy);
        }
        if !(cx.is_finite() && cy.is_finite()) {
            (cx, cy) = cible;
            self.vitesse_centre = (0.0, 0.0);
        }
        self.center = Some((cx, cy));

        // kicks : sens inversé + satellites
        let new = nouveaux(self.kicks_seen, au.kicks);
        self.kicks_seen = Some(au.kicks);
        let e = &cfg.essaim;
        for _ in 0..new {
            self.dir = -self.dir;
            self.eclater(t);
            let (lo, hi) = (e.par_kick_min.min(e.par_kick_max), e.par_kick_max.max(e.par_kick_min));
            let n = self.rng.range(lo as f32, hi as f32 + 0.999) as u32;
            self.ajouter_satellites(n as usize, t, e.maximum);
        }
        self.env *= (-dt * 9.0).exp();
        // relais du tempo (break sans kick) : l'éclat pulse doucement sur chaque temps
        if cfg.audio.pulsation_tempo && au.relais {
            self.env = self.env.max(pulsation_tempo(au.phase));
        }
        // caisse claire : un carré s'ouvre sous le pointeur
        let caisses = nouveaux(self.caisses_vues, au.caisses);
        self.caisses_vues = Some(au.caisses);
        if cfg.musique.caisse_claire && caisses > 0 {
            let (taille, vie) = CARRE_CAISSE;
            self.particules.push(Particule { x: cursor.0, y: cursor.1, vx: 0.0, vy: 0.0, ne: t, vie, taille: taille * scale, genre: Genre::Carre });
        }
        // montée : suivie en douceur ; drop : l'essaim explose
        let drops = nouveaux(self.drops_vus, au.drops);
        self.drops_vus = Some(au.drops);
        let vise = if cfg.musique.montee { au.tension.clamp(0.0, 1.0) } else { 0.0 };
        self.tension += (vise - self.tension) * (dt * 6.0).min(1.0);
        if cfg.musique.montee && drops > 0 {
            self.exploser(cursor.0, cursor.1, t, cfg, scale);
        }
        // contretemps (milieu du temps, tempo connu) : les têtes pulsent
        self.contre *= (-dt * 10.0).exp();
        let phase = au.bpm.map(|_| au.phase);
        if let (true, Some(p), Some(avant)) = (cfg.musique.contretemps, phase, self.phase_avant) {
            if avant < 0.5 && p >= 0.5 && p - avant < 0.4 {
                self.contre = 1.0;
            }
        }
        self.phase_avant = phase;

        // aigus : étincelles près des têtes (ou sur l'orbite s'il n'y en a pas)
        let nouveaux = nouveaux(self.aigus_vus, au.aigus);
        self.aigus_vus = Some(au.aigus);
        if cfg.scintillement.actif && nouveaux > 0 {
            let mut tetes = std::mem::take(&mut self.tetes);
            tetes.clear();
            tetes.extend(self.trails.iter().chain(self.sats.iter().chain(&self.fixes).map(|s| &s.trail)).filter_map(|tr| tr.back().map(|p| (p.1, p.2))));
            let quantite = cfg.scintillement.quantite.clamp(1.0, 10.0).round() as u32;
            for _ in 0..(nouveaux * quantite).min(MAX_PARTICULES as u32) {
                let (x, y) = if tetes.is_empty() {
                    let a = self.rng.range(0.0, TAU);
                    (cx + a.cos() * cfg.orbite.rayon * scale, cy + a.sin() * cfg.orbite.rayon * scale)
                } else {
                    tetes[(self.rng.f() * tetes.len() as f32) as usize % tetes.len()]
                };
                let (a, v) = (self.rng.range(0.0, TAU), self.rng.range(30.0, 90.0) * scale);
                let vie = self.rng.range(0.3, 0.55);
                let taille = self.rng.range(1.1, 2.1) * scale;
                self.particules.push(Particule { x, y, vx: a.cos() * v, vy: a.sin() * v, ne: t, vie, taille, genre: Genre::Etincelle });
            }
            self.tetes = tetes;
        }
        // sillage : poussière le long du trajet de la souris quand elle file vite
        if let (true, Some(avant)) = (cfg.sillage.actif, self.dernier_curseur) {
            let (dx, dy) = (cursor.0 - avant.0, cursor.1 - avant.1);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist / dt.max(1e-3) > 700.0 * scale && dist < 600.0 * scale {
                let n = ((cfg.sillage.densite.clamp(1.0, 10.0) * dist / (60.0 * scale)).round() as usize).min(14);
                for i in 0..n {
                    let f = (i as f32 + self.rng.f()) / n.max(1) as f32;
                    let (a, v) = (self.rng.range(0.0, TAU), self.rng.range(5.0, 30.0) * scale);
                    let vie = self.rng.range(0.35, 0.7);
                    let taille = self.rng.range(0.7, 1.7) * scale;
                    let (x, y) = (avant.0 + dx * f, avant.1 + dy * f);
                    self.particules.push(Particule { x, y, vx: a.cos() * v - dx / dt * 0.05, vy: a.sin() * v - dy / dt * 0.05, ne: t, vie, taille, genre: Genre::Poussiere });
                }
            }
        }
        self.dernier_curseur = Some(cursor);
        // particules : avancent en ralentissant (en rebondissant sur les bords de la fenêtre au
        // premier plan), puis s'éteignent
        let frein = (-dt * 3.0).exp();
        let murs = self.murs.filter(|_| cfg.scintillement.rebonds);
        for p in &mut self.particules {
            let (x0, y0) = (p.x, p.y);
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            if let (Some(m), Genre::Etincelle | Genre::Poussiere) = (murs, p.genre) {
                rebondir(p, x0, y0, m);
            }
            p.vx *= frein;
            p.vy *= frein;
        }
        self.particules.retain(|p| t - p.ne < p.vie as f64);
        if self.particules.len() > MAX_PARTICULES {
            let trop = self.particules.len() - MAX_PARTICULES;
            self.particules.drain(..trop);
        }

        // basse : vitesse ; note grave : éloignement (lissages courts, ~100 ms et ~60 ms)
        if au.pitch > 0.0 {
            self.pitch += (au.pitch - self.pitch) * (dt * 25.0).min(1.0);
        }
        let depth = ((80.0 - self.pitch) / 50.0).clamp(0.0, 1.0);
        let o = &cfg.orbite;
        // montée : plus vite et plus serré ; contretemps : les orbites s'écartent un instant
        let tension = self.tension.clamp(0.0, 1.0);
        self.spd += ((o.vitesse_base + o.vitesse_basse * au.bass) * (1.0 + 1.2 * tension) - self.spd) * (dt * 10.0).min(1.0);
        let presence = (au.bass * 2.5).clamp(0.0, 1.0);
        let vise = (1.0 + o.eloignement_grave * depth * presence) * (1.0 - 0.45 * tension) * (1.0 + 0.06 * self.contre);
        self.far += (vise - self.far) * (dt * 16.0).min(1.0);
        // une valeur non finie (son abîmé) figerait l'essaim pour toujours : retour au repos
        if !self.spd.is_finite() {
            self.spd = o.vitesse_base;
        }
        if !self.far.is_finite() {
            self.far = 1.0;
        }
        if !self.pitch.is_finite() {
            self.pitch = 50.0;
        }

        let w = self.dir * self.spd;
        // comètes : `nombre` par anneau, réparties sur le trajet
        let (anneaux, nombre) = (o.anneaux.max(1) as usize, o.nombre.max(1) as usize);
        self.angles.resize(anneaux, 0.0);
        self.trails.resize_with(anneaux * nombre, Trail::new);
        let life = o.trainee.max(0.01) as f64;
        // position sur le trajet à la fraction `f` de l'image (centre et rayon interpolés) : les
        // points intermédiaires suivent la vraie forme, coins compris
        let (rayon, rayon0) = (o.rayon * scale * self.far, o.rayon * scale * far0);
        let drot = o.rotation * TAU / 60.0 * dt;
        let (rot0, drot) = (if self.rot.is_finite() { self.rot } else { 0.0 }, if drot.is_finite() { drot } else { 0.0 });
        self.rot = (rot0 + drot).rem_euclid(TAU);
        let point = |forme, a: f32, rel: f32, f: f32| {
            let (x, y) = chemin(forme, a);
            let (sin, cos) = (rot0 + drot * f).sin_cos();
            let (x, y) = (x * cos - y * sin, x * sin + y * cos);
            let rr = (rayon0 + (rayon - rayon0) * f) * rel;
            (c0.0 + (cx - c0.0) * f + x * rr, c0.1 + (cy - c0.1) * f + y * rr)
        };
        for (k, angle) in self.angles.iter_mut().enumerate() {
            let (rk, wk) = anneau(k);
            let (a0, da) = (*angle, w * wk * dt);
            *angle = (a0 + da).rem_euclid(TAU);
            let n = sous_pas(da, rayon * rk * 1.2, FINESSE_COMETES);
            for i in 0..nombre {
                let decale = i as f32 * TAU / nombre as f32;
                for j in 1..=n {
                    let f = j as f32 / n as f32;
                    let (x, y) = point(o.forme, a0 + da * f + decale, rk, f);
                    push_trail(&mut self.trails[k * nombre + i], t - dt as f64 * (1.0 - f) as f64, x, y, life, FINESSE_COMETES.1);
                }
            }
        }
        let dur = e.duree.max(0.1) as f64;
        self.sats.retain(|s| t - s.born < dur);
        // satellites permanents : répartis une fois pour toutes (nombre d'or), toujours les mêmes
        if self.fixes.len() != e.fixes as usize {
            self.fixes = (0..e.fixes)
                .map(|j| {
                    let u = (j as f32 * 0.618_034).fract();
                    Sat { a: j as f32 * 2.399_963, r: 0.6 + u * (SAT_R_MAX - 0.6), mul: 0.6 + 0.8 * (1.0 - u), born: f64::INFINITY, trail: Trail::new(), couleur: j as usize }
                })
                .collect();
        }
        // satellites : sur le trajet des comètes, ou le leur, à des distances variées
        let forme = e.forme.suivie(o.forme);
        for s in self.sats.iter_mut().chain(self.fixes.iter_mut()) {
            let (a0, da) = (s.a, w * s.mul * dt);
            s.a = (a0 + da).rem_euclid(TAU);
            let n = sous_pas(da, rayon * s.r * 1.2, FINESSE_SATELLITES);
            for j in 1..=n {
                let f = j as f32 / n as f32;
                let (x, y) = point(forme, a0 + da * f, s.r, f);
                push_trail(&mut s.trail, t - dt as f64 * (1.0 - f) as f64, x, y, SAT_TRAIL, FINESSE_SATELLITES.1);
            }
        }
        // saut du pointeur (autre écran…) : la traînée laissée loin derrière est abandonnée
        let half = MAX_EXTENT / 2.0;
        let near = |&(_, x, y): &(f64, f32, f32)| (x - cursor.0).abs() <= half && (y - cursor.1).abs() <= half;
        for tr in self.trails.iter_mut().chain(self.sats.iter_mut().chain(self.fixes.iter_mut()).map(|s| &mut s.trail)) {
            tr.retain(near);
        }
        self.particules.retain(|p| near(&(0.0, p.x, p.y)));
    }

    /// Effets d'un clic en (x, y) : onde de choc et éclats (`[clic]`).
    pub fn clic(&mut self, x: f32, y: f32, t: f64, cfg: &Config, scale: f32) {
        let c = &cfg.clic;
        let t_clic = c.taille.clamp(0.3, 3.0);
        if c.onde {
            let taille = 46.0 * t_clic * scale;
            self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie: 0.55, taille, genre: Genre::Onde });
        }
        if c.eclats {
            for _ in 0..16 {
                let (a, v) = (self.rng.range(0.0, TAU), self.rng.range(160.0, 380.0) * t_clic * scale);
                let vie = self.rng.range(0.45, 0.8);
                let taille = self.rng.range(1.2, 2.4) * scale;
                self.particules.push(Particule { x, y, vx: a.cos() * v, vy: a.sin() * v, ne: t, vie, taille, genre: Genre::Etincelle });
            }
        }
    }

    /// Clic droit en (x, y) (`[clic] droit`) : un anneau qui se referme et des étincelles
    /// aspirées vers le point du clic (l'inverse de l'onde du clic principal).
    pub fn clic_droit(&mut self, x: f32, y: f32, t: f64, cfg: &Config, scale: f32) {
        let r = 52.0 * cfg.clic.taille.clamp(0.3, 3.0) * scale;
        self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie: 0.45, taille: r, genre: Genre::Implosion });
        for i in 0..12 {
            let a = i as f32 * TAU / 12.0 + self.rng.range(-0.2, 0.2);
            let (cos, sin) = (a.cos(), a.sin());
            let v = self.rng.range(2.2, 3.0) * r;
            let taille = self.rng.range(1.0, 1.8) * scale;
            self.particules.push(Particule { x: x + cos * r, y: y + sin * r, vx: -cos * v, vy: -sin * v, ne: t, vie: 0.4, taille, genre: Genre::Etincelle });
        }
    }

    /// Éclat du kick (et des rafales) : plein, ou en mode sans flashs, à moitié et au plus trois
    /// fois par seconde.
    fn eclater(&mut self, t: f64) {
        if !self.doux {
            self.env = 1.0;
        } else if t - self.dernier_flash >= ECART_FLASH {
            self.dernier_flash = t;
            self.env = self.env.max(0.5);
        }
    }

    /// Ajoute `n` satellites nés à `t`, dans la limite de `maximum`.
    fn ajouter_satellites(&mut self, n: usize, t: f64, maximum: usize) {
        for _ in 0..n {
            if self.sats.len() >= maximum {
                break;
            }
            let (a, r, mul) = (self.rng.range(0.0, TAU), self.rng.range(0.75, SAT_R_MAX), self.rng.range(0.7, 1.3));
            self.sats.push(Sat { a, r, mul, born: t, trail: Trail::new(), couleur: self.prochaine_couleur });
            self.prochaine_couleur = self.prochaine_couleur.wrapping_add(1);
        }
    }

    /// Drop (`[musique] montee`) : l'éclat d'un kick, deux fois les satellites d'un kick fort,
    /// une grande onde et des étincelles projetées depuis le centre de l'orbite.
    fn exploser(&mut self, x: f32, y: f32, t: f64, cfg: &Config, scale: f32) {
        let e = &cfg.essaim;
        self.eclater(t);
        self.ajouter_satellites(2 * e.par_kick_min.max(e.par_kick_max).max(2) as usize, t, e.maximum);
        let (taille, vie) = ONDE_DROP;
        self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie, taille: taille * scale, genre: Genre::Onde });
        let (cx, cy) = self.center.unwrap_or((x, y));
        for i in 0..ETINCELLES_DROP {
            let a = i as f32 * TAU / ETINCELLES_DROP as f32 + self.rng.range(-0.1, 0.1);
            let v = self.rng.range(220.0, 420.0) * scale;
            let (vie, taille) = (self.rng.range(0.5, 0.9), self.rng.range(1.2, 2.2) * scale);
            self.particules.push(Particule { x: cx, y: cy, vx: a.cos() * v, vy: a.sin() * v, ne: t, vie, taille, genre: Genre::Etincelle });
        }
    }

    /// Dessine (coordonnées écran). Le pointeur lui-même est celui de Windows, affiché
    /// par-dessus : seul son éclat au kick est dessiné ici, autour de `cursor`.
    pub fn draw(&self, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32, cursor: (f32, f32)) {
        if self.center.is_none() {
            return;
        }
        let glow = 0.4 + 0.6 * self.env;
        let o = &cfg.orbite;
        let dur = cfg.essaim.duree.max(0.1) as f64;

        let mut tampons = self.tampons.borrow_mut();
        let Tampons { heads, xy, points, liens: paires } = &mut *tampons;
        heads.clear();
        // contretemps : les têtes grossissent un instant
        let pulse = 1.0 + 0.5 * self.contre;

        // satellites (traînées sans halo, pour rester léger) ; les permanents restent à 0,8
        for s in self.sats.iter().chain(&self.fixes) {
            let k = if s.born.is_finite() { (1.0 - (t - s.born) / dur) as f32 } else { 0.8 };
            let al = (k * 1.5).min(1.0);
            draw_trail(cv, xy, &s.trail, t, SAT_TRAIL, 2.6 * k * scale, al, false, cfg, s.couleur);
            if let Some(&(_, x, y)) = s.trail.back() {
                let rgb = prisme_palette(cfg, t, 1.0, s.couleur);
                heads.push(Head { x, y, s: (0.6 + 1.6 * k) * scale * pulse, al, front: y > self.center.map_or(0.0, |c| c.1), rgb });
            }
        }
        // les comètes (devant : sous le centre de l'orbite, comme vues de biais) ; chacune prend
        // une teinte de la palette
        if o.cometes {
            let cy = self.center.map_or(0.0, |c| c.1);
            for (i, tr) in self.trails.iter().enumerate() {
                draw_trail(cv, xy, tr, t, o.trainee.max(0.01) as f64, 3.5 * scale, 1.0, true, cfg, i);
                if let Some(&(_, x, y)) = tr.back() {
                    heads.push(Head { x, y, s: 2.4 * scale * pulse, al: 1.0, front: y > cy, rgb: prisme_palette(cfg, t, 1.0, i) });
                }
            }
        }
        let head_rgb = prisme(cfg, t, 1.0);
        // constellation : fines lignes entre les têtes proches, sous les têtes
        if cfg.essaim.constellation {
            points.clear();
            points.extend(heads.iter().map(|h| (h.x, h.y)));
            liens(points, LIENS_PORTEE * o.rayon * scale * self.far.max(0.2), paires);
            let rgb = prisme(cfg, t, 0.85);
            for &(i, j, f) in paires.iter() {
                let al = (0.25 + 0.5 * f) * heads[i].al.min(heads[j].al);
                cv.line((heads[i].x, heads[i].y), (heads[j].x, heads[j].y), 1.2 * scale, rgb, al, false);
            }
        }
        let draw_head = |cv: &mut dyn Canvas, h: &Head| {
            // lueur : deux disques translucides (un dégradé radial coûte bien plus cher sur processeur)
            cv.circle(h.x, h.y, h.s * 2.6, h.rgb, 0.12 * h.al * glow);
            cv.circle(h.x, h.y, h.s * 1.6, h.rgb, 0.3 * h.al * glow);
            cv.circle(h.x, h.y, h.s, h.rgb, h.al);
            cv.circle(h.x, h.y, h.s * 0.45, (255, 255, 255), h.al);
        };
        for h in heads.iter().filter(|h| !h.front) {
            draw_head(cv, h);
        }
        // mode sans flashs : ni halo blanc, ni anneau ou carré trop vifs
        let vif = if self.doux { 0.45 } else { 1.0 };
        if self.env > 0.02 && cfg.eclat.taille > 0.0 {
            // éclat du kick : halo blanc et anneau coloré qui s'élargit sous le pointeur
            let (px, py) = cursor;
            let r = cfg.eclat.taille * scale;
            if !self.doux {
                cv.circle(px, py, r * (2.0 + 2.5 * self.env), (255, 255, 255), 0.35 * self.env);
            }
            cv.ring(px, py, r * (2.5 + 5.0 * (1.0 - self.env)), 1.5 * scale, head_rgb, 0.8 * self.env * vif);
        }
        // particules : étincelles, ondes de choc, poussière, carrés des caisses claires, implosions
        for p in &self.particules {
            let k = ((t - p.ne) as f32 / p.vie).clamp(0.0, 1.0);
            match p.genre {
                Genre::Etincelle => {
                    cv.circle(p.x, p.y, p.taille * 2.2, head_rgb, 0.25 * (1.0 - k));
                    cv.circle(p.x, p.y, p.taille * (1.0 - 0.5 * k), (255, 255, 255), 1.0 - k);
                }
                Genre::Onde => {
                    let r = p.taille * (1.0 - (1.0 - k).powi(3));
                    cv.ring(p.x, p.y, r.max(1.0), (2.5 * (1.0 - k) + 0.6) * scale, head_rgb, 0.85 * (1.0 - k) * vif);
                }
                Genre::Poussiere => {
                    cv.circle(p.x, p.y, p.taille * (1.0 - 0.5 * k), prisme(cfg, t, 0.5), 0.7 * (1.0 - k));
                }
                Genre::Carre => {
                    // carré posé sur la pointe, qui s'ouvre vite puis s'efface
                    let r = p.taille * (0.35 + 0.65 * (1.0 - (1.0 - k).powi(2)));
                    let sommets = [(p.x, p.y - r), (p.x + r, p.y), (p.x, p.y + r), (p.x - r, p.y), (p.x, p.y - r)];
                    cv.polyline(&sommets, (2.2 * (1.0 - k) + 0.8) * scale, head_rgb, 0.8 * (1.0 - k) * vif, true);
                }
                Genre::Implosion => {
                    // anneau qui se referme : grand au clic, nul à la fin
                    let r = p.taille * (1.0 - k).powi(2);
                    cv.ring(p.x, p.y, r.max(1.0), (0.8 + 1.8 * k) * scale, head_rgb, 0.8 * (1.0 - k * k) * vif);
                }
            }
        }
        for h in heads.iter().filter(|h| h.front) {
            draw_head(cv, h);
        }
    }
}

/// Segments de traînée regroupés par morceaux : chaque morceau est un seul tracé (couleur,
/// épaisseur et opacité de son milieu). Sur processeur, bien moins coûteux qu'un tracé par
/// segment, à l'œil identique ; le lissage y coûte ~3,5× plus cher par tracé, il est donc
/// réservé aux comètes.
const TRAIL_CHUNK: usize = 4;

/// Dessine une traînée dans la teinte `j` de la palette.
#[allow(clippy::too_many_arguments)]
fn draw_trail(cv: &mut dyn Canvas, xy: &mut Vec<(f32, f32)>, tr: &Trail, t: f64, life: f64, width: f32, al: f32, halo: bool, cfg: &Config, j: usize) {
    if tr.len() < 2 {
        return;
    }
    // fraîcheur du point `i` : 1 en tête, 0 en bout de vie
    let q = |i: usize| (1.0 - (t - tr[i].0) / life).max(0.0) as f32;
    xy.clear();
    xy.extend(tr.iter().map(|&(_, x, y)| (x, y)));
    if halo {
        cv.polyline(xy, (0.8 + width * 0.7) * 2.6, prisme_palette(cfg, t, 0.7, j), 0.12 * al, false);
    }
    // les comètes (avec halo) sont lissées ; les satellites non, en deux morceaux (tête vive,
    // queue pâle) : c'est le poste le plus coûteux sur processeur
    let smooth = halo;
    let chunk = if smooth { TRAIL_CHUNK } else { (xy.len() - 1).div_ceil(2).max(1) };
    let mut i = 0;
    while i + 1 < xy.len() {
        let end = (i + chunk).min(xy.len() - 1);
        let k = (q(i) + q(end)) * 0.5;
        cv.polyline(&xy[i..=end], 0.8 + k * width, prisme_palette(cfg, t, k, j), k * 0.9 * al, smooth);
        i = end;
    }
}

/// Fissure : segments (x1, y1, x2, y2, épaisseur) centrés sur le point de clic.
pub struct Crack {
    pub x: f32,
    pub y: f32,
    segs: Vec<[f32; 5]>,
    /// Demi-largeur du carré qui contient la fissure.
    pub ext: f32,
    pub born: f64,
    /// Passes fixes (trait sombre et reflet) déjà tracées, pour les moteurs qui recopient des
    /// images (processeur) : faites à la première image, recopiées ensuite.
    fixe: std::cell::RefCell<Option<tiny_skia::Pixmap>>,
    /// Branches de la fissure (suites de segments bout à bout), en coordonnées écran : l'éclat,
    /// de largeur constante, est tracé branche par branche plutôt que segment par segment.
    branches: Vec<Vec<(f32, f32)>>,
}

impl Crack {
    pub fn new(x: f32, y: f32, t: f64, size: f32, rng: &mut Rng) -> Self {
        let mut segs = Vec::new();
        let rays = 3 + (rng.f() * 4.0) as usize;
        let off = rng.range(0.0, TAU);
        let mut stack = Vec::new();
        for i in 0..rays {
            let ang = off + i as f32 / rays as f32 * TAU + rng.range(-0.35, 0.35);
            stack.push((0.0f32, 0.0f32, ang, rng.range(18.0, 44.0) * size, rng.range(1.6, 2.6) * size.sqrt(), 0u8));
        }
        while let Some((mut px, mut py, mut a, len, wd, depth)) = stack.pop() {
            let mut left = len;
            while left > 0.0 {
                let st = rng.range(4.0, 9.0) * size.sqrt();
                a += (rng.f() - 0.5) * 0.75;
                let (nx, ny) = (px + a.cos() * st, py + a.sin() * st);
                segs.push([px, py, nx, ny, wd * (left / len) + 0.35]);
                px = nx;
                py = ny;
                left -= st;
                if depth < 2 && rng.f() < 0.1 {
                    let side = if rng.f() < 0.5 { -1.0 } else { 1.0 };
                    stack.push((px, py, a + side * rng.range(0.5, 1.1), left * rng.range(0.4, 0.7), wd * 0.6, depth + 1));
                }
            }
        }
        let ext = segs.iter().fold(8.0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()).max(s[2].abs()).max(s[3].abs())) + 6.0;
        let mut branches: Vec<Vec<(f32, f32)>> = Vec::new();
        for s in &segs {
            let (a, b) = ((x + s[0], y + s[1]), (x + s[2], y + s[3]));
            match branches.last_mut() {
                Some(br) if br.last() == Some(&a) => br.push(b),
                _ => branches.push(vec![a, b]),
            }
        }
        Self { x, y, segs, ext, born: t, fixe: Default::default(), branches }
    }

    /// Rectangle écran (gauche, haut, droite, bas) qui contient la fissure.
    pub fn rect(&self) -> (i32, i32, i32, i32) {
        let e = self.ext.ceil() as i32 + 1;
        (self.x as i32 - e, self.y as i32 - e, self.x as i32 + e, self.y as i32 + e)
    }

    /// Dessine la fissure (coordonnées écran) ; `glow` 0..1 = éclat coloré, `opacity` = fondu.
    /// Sur processeur, les deux passes fixes sont recopiées d'une image faite une fois (tracer
    /// chaque segment à chaque image coûtait plusieurs millisecondes pour 12 fissures) ; seul
    /// l'éclat, qui change de couleur et d'intensité, est retracé.
    pub fn draw(&self, cv: &mut dyn Canvas, glow: f32, rgb: (u8, u8, u8), opacity: f32) {
        if cv.images() {
            let (l, t, r, b) = self.rect();
            let mut fixe = self.fixe.borrow_mut();
            if fixe.is_none() {
                *fixe = tiny_skia::Pixmap::new((r - l) as u32, (b - t) as u32).map(|mut pm| {
                    self.passes_fixes(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (l as f32, t as f32) }, 1.0);
                    pm
                });
            }
            match fixe.as_ref() {
                Some(pm) => {
                    cv.image(pm, l, t, opacity);
                }
                None => self.passes_fixes(cv, opacity),
            }
        } else {
            self.passes_fixes(cv, opacity);
        }
        if glow > 0.02 {
            for br in &self.branches {
                cv.polyline(br, 3.5, rgb, glow * 0.3 * opacity, true);
            }
            for br in &self.branches {
                cv.polyline(br, 1.2, rgb, glow * opacity, true);
            }
        }
    }

    /// Trait sombre puis reflet décalé.
    fn passes_fixes(&self, cv: &mut dyn Canvas, opacity: f32) {
        let (x, y) = (self.x, self.y);
        for s in &self.segs {
            cv.line((x + s[0], y + s[1]), (x + s[2], y + s[3]), s[4], (2, 3, 8), 0.92 * opacity, true);
        }
        for s in &self.segs {
            let w = (s[4] * 0.35).max(0.5);
            cv.line((x + s[0] + 0.8, y + s[1] + 0.8), (x + s[2] + 0.8, y + s[3] + 0.8), w, (170, 185, 250), 0.25 * opacity, true);
        }
    }
}

/// Taille de l'onde d'un changement de piste (pixels à 100 %), et sa durée (s).
const ONDE_PISTE: (f32, f32) = (90.0, 0.8);

impl Swarm {
    /// Rafale d'un changement de piste (`[musique] rafale_piste`), faite d'effets existants :
    /// l'éclat d'un kick, deux fois les satellites d'un kick fort (dans la limite
    /// `[essaim] maximum`) et une grande onde de choc en (x, y), sans changer de sens.
    pub fn rafale(&mut self, x: f32, y: f32, t: f64, cfg: &Config, scale: f32) {
        let e = &cfg.essaim;
        self.eclater(t);
        self.ajouter_satellites(2 * e.par_kick_min.max(e.par_kick_max).max(2) as usize, t, e.maximum);
        let (taille, vie) = ONDE_PISTE;
        self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie, taille: taille * scale, genre: Genre::Onde });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    const C: (f32, f32) = (300.0, 300.0);

    /// Distance moyenne des têtes des comètes au centre de l'orbite.
    fn ecart_moyen(s: &Swarm) -> f32 {
        let c = s.center.unwrap();
        let d: Vec<f32> = s.trails.iter().filter_map(|tr| tr.back()).map(|p| ((p.1 - c.0).powi(2) + (p.2 - c.1).powi(2)).sqrt()).collect();
        d.iter().sum::<f32>() / d.len().max(1) as f32
    }

    #[test]
    fn caisse_claire_ouvre_un_carre() {
        let mut cfg = Config::default();
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, AudioView::default(), &cfg, 1.0);
        s.update(0.016, 0.016, C, AudioView { caisses: 1, ..Default::default() }, &cfg, 1.0);
        assert_eq!(s.particules.iter().filter(|p| p.genre == Genre::Carre).count(), 1);
        assert_eq!(s.sats.len(), 0, "une caisse claire n'est pas un kick");
        // le carré reste dans le rectangle redessiné
        let (x0, _, x1, _) = s.bounds(C, 1.0, cfg.eclat.taille);
        assert!(x0 <= C.0 - CARRE_CAISSE.0 && x1 >= C.0 + CARRE_CAISSE.0);
        // effet coupé : rien
        cfg.musique.caisse_claire = false;
        s.update(0.032, 0.016, C, AudioView { caisses: 2, ..Default::default() }, &cfg, 1.0);
        assert_eq!(s.particules.iter().filter(|p| p.genre == Genre::Carre).count(), 1);
    }

    #[test]
    fn montee_resserre_puis_le_drop_explose() {
        let mut cfg = Config::default();
        let au = AudioView { bass: 0.4, pitch: 50.0, ..Default::default() };
        let tourner = |s: &mut Swarm, au: AudioView, cfg: &Config, t0: f64| {
            for i in 0..120 {
                s.update(t0 + i as f64 / 60.0, 1.0 / 60.0, C, au, cfg, 1.0);
            }
        };
        let (mut calme, mut tendu) = (Swarm::new(), Swarm::new());
        tourner(&mut calme, au, &cfg, 0.0);
        tourner(&mut tendu, AudioView { tension: 1.0, ..au }, &cfg, 0.0);
        assert!(ecart_moyen(&tendu) < 0.7 * ecart_moyen(&calme), "{} contre {}", ecart_moyen(&tendu), ecart_moyen(&calme));
        assert!(tendu.spd > 1.8 * calme.spd, "{} contre {}", tendu.spd, calme.spd);
        // drop : éclat, satellites, grande onde et étincelles
        tendu.update(2.0, 0.016, C, AudioView { tension: 1.0, drops: 1, ..au }, &cfg, 1.0);
        assert_eq!(tendu.env, 1.0);
        assert!(tendu.sats.len() >= 6, "{} satellites", tendu.sats.len());
        assert!(tendu.particules.iter().any(|p| p.genre == Genre::Onde && p.taille >= ONDE_DROP.0));
        assert!(tendu.particules.iter().filter(|p| p.genre == Genre::Etincelle).count() >= ETINCELLES_DROP);
        // effet coupé : ni resserrement ni explosion
        cfg.musique.montee = false;
        let mut coupe = Swarm::new();
        tourner(&mut coupe, AudioView { tension: 1.0, ..au }, &cfg, 0.0);
        coupe.update(2.0, 0.016, C, AudioView { tension: 1.0, drops: 1, ..au }, &cfg, 1.0);
        assert!((ecart_moyen(&coupe) - ecart_moyen(&calme)).abs() < 0.05 * ecart_moyen(&calme));
        assert!(coupe.sats.is_empty());
    }

    #[test]
    fn contretemps_pulse_au_milieu_du_temps() {
        let mut cfg = Config::default();
        cfg.musique.contretemps = true;
        let mut s = Swarm::new();
        let au = |phase: f32| AudioView { bpm: Some(120.0), phase, ..Default::default() };
        s.update(0.0, 0.016, C, au(0.4), &cfg, 1.0);
        assert_eq!(s.contre, 0.0);
        s.update(0.016, 0.016, C, au(0.55), &cfg, 1.0);
        assert!(s.contre > 0.8, "{}", s.contre);
        // passage du temps (0,9 → 0,1) : pas un contretemps ; sans tempo connu : rien
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, au(0.9), &cfg, 1.0);
        s.update(0.016, 0.016, C, au(0.1), &cfg, 1.0);
        s.update(0.032, 0.016, C, AudioView { phase: 0.6, ..Default::default() }, &cfg, 1.0);
        assert_eq!(s.contre, 0.0);
    }

    #[test]
    fn stereo_penche_l_essaim_du_cote_le_plus_fort() {
        let mut cfg = Config::default();
        let au = AudioView { balance: 1.0, ..Default::default() };
        let mut s = Swarm::new();
        for i in 0..60 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, au, &cfg, 1.0);
        }
        assert!((s.center.unwrap().0 - C.0).abs() < 0.5, "réglage à 0 : l'essaim reste centré");
        cfg.musique.stereo = 1.0;
        for i in 60..120 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, au, &cfg, 1.0);
        }
        let d = s.center.unwrap().0 - C.0;
        assert!(d > 0.8 * cfg.orbite.rayon, "décalage {d}");
        assert!(d <= CENTER_LAG_MAX + 1.6 * cfg.orbite.rayon);
    }

    #[test]
    fn inertie_fait_trainer_puis_rattraper_le_centre() {
        let mut cfg = Config::default();
        let suivre = |cfg: &Config| {
            let mut s = Swarm::new();
            s.update(0.0, 1.0 / 60.0, (0.0, 0.0), AudioView::default(), cfg, 1.0);
            // le pointeur file vers la droite à 1500 px/s pendant 0,3 s
            let mut ecart: f32 = 0.0;
            for i in 1..=18 {
                let x = i as f32 * 25.0;
                s.update(i as f64 / 60.0, 1.0 / 60.0, (x, 0.0), AudioView::default(), cfg, 1.0);
                ecart = ecart.max(x - s.center.unwrap().0);
            }
            // puis s'arrête : 3 s plus tard, le centre est revenu sur lui
            for i in 19..200 {
                s.update(i as f64 / 60.0, 1.0 / 60.0, (450.0, 0.0), AudioView::default(), cfg, 1.0);
            }
            (ecart, (s.center.unwrap().0 - 450.0).abs())
        };
        let (pres, fin_pres) = suivre(&cfg);
        cfg.orbite.inertie = 1.0;
        let (souple, fin_souple) = suivre(&cfg);
        assert!(souple > 2.0 * pres, "retard {souple} contre {pres}");
        assert!(souple <= CENTER_LAG_MAX * 6.0 + 1.0);
        assert!(fin_pres < 0.5 && fin_souple < 1.0, "{fin_pres} {fin_souple}");
    }

    #[test]
    fn particules_rebondissent_sur_la_fenetre_active() {
        let mut p = Particule { x: 105.0, y: 50.0, vx: -300.0, vy: 0.0, ne: 0.0, vie: 1.0, taille: 1.0, genre: Genre::Etincelle };
        let (x0, y0) = (p.x, p.y);
        p.x += p.vx * 0.02;
        rebondir(&mut p, x0, y0, (100.0, 0.0, 400.0, 300.0));
        assert!(p.x > 100.0 && p.vx > 0.0, "{} {}", p.x, p.vx);
        // hors de la hauteur de la fenêtre : pas de rebond
        let mut q = Particule { y: 400.0, ..p };
        q.vx = -300.0;
        q.x = 105.0;
        q.x += q.vx * 0.02;
        rebondir(&mut q, 105.0, 400.0, (100.0, 0.0, 400.0, 300.0));
        assert!(q.x < 100.0 && q.vx < 0.0);
        // dans l'essaim : les étincelles des aigus rebondissent, si le réglage le veut
        let mut cfg = Config::default();
        let mut s = Swarm::new();
        s.murs = Some((C.0 - 5.0, 0.0, 2000.0, 2000.0));
        s.update(0.0, 0.016, C, AudioView::default(), &cfg, 1.0);
        s.particules.push(Particule { x: C.0, y: C.1, vx: -400.0, vy: 0.0, ne: 0.0, vie: 1.0, taille: 1.0, genre: Genre::Etincelle });
        s.update(0.02, 0.02, C, AudioView::default(), &cfg, 1.0);
        assert!(s.particules.last().unwrap().x > C.0 - 5.0);
        cfg.scintillement.rebonds = false;
        s.particules.push(Particule { x: C.0, y: C.1, vx: -400.0, vy: 0.0, ne: 0.02, vie: 1.0, taille: 1.0, genre: Genre::Etincelle });
        s.update(0.04, 0.02, C, AudioView::default(), &cfg, 1.0);
        assert!(s.particules.last().unwrap().x < C.0 - 5.0);
    }

    #[test]
    fn palette_partage_les_teintes() {
        let mut cfg = Config::default();
        (cfg.couleur.vitesse_teinte, cfg.couleur.synchro_prisme) = (0.0, false);
        // une seule teinte : toutes pareilles
        assert_eq!(prisme_palette(&cfg, 0.0, 1.0, 0), prisme_palette(&cfg, 0.0, 1.0, 1));
        cfg.couleur.palette = 3;
        let t: Vec<_> = (0..4).map(|j| prisme_palette(&cfg, 0.0, 1.0, j)).collect();
        assert_ne!(t[0], t[1]);
        assert_ne!(t[1], t[2]);
        assert_eq!(t[0], t[3], "la palette revient à sa première teinte");
        assert_eq!(t[0], prisme(&cfg, 0.0, 1.0));
    }

    #[test]
    fn constellation_relie_les_tetes_proches() {
        let mut l = Vec::new();
        liens(&[(0.0, 0.0), (10.0, 0.0), (100.0, 0.0)], 20.0, &mut l);
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].0, l[0].1), (0, 1));
        assert!((l[0].2 - 0.5).abs() < 1e-6);
        // au plus LIENS_MAX liens, même serrés
        let serres: Vec<(f32, f32)> = (0..64).map(|i| (i as f32 * 0.1, 0.0)).collect();
        liens(&serres, 50.0, &mut l);
        assert_eq!(l.len(), LIENS_MAX);
        liens(&serres, f32::NAN, &mut l);
        assert!(l.is_empty());
    }

    #[test]
    fn clic_droit_se_referme() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, AudioView::default(), &cfg, 1.0);
        s.clic_droit(C.0, C.1, 0.016, &cfg, 1.0);
        assert_eq!(s.particules.iter().filter(|p| p.genre == Genre::Implosion).count(), 1);
        // les étincelles partent du cercle et vont vers le centre
        for p in s.particules.iter().filter(|p| p.genre == Genre::Etincelle) {
            let (dx, dy) = (C.0 - p.x, C.1 - p.y);
            assert!(dx * p.vx + dy * p.vy > 0.0);
        }
        let (x0, y0, x1, y1) = s.bounds(C, 1.0, cfg.eclat.taille);
        assert!(x0 <= C.0 - 52.0 && y0 <= C.1 - 52.0 && x1 >= C.0 + 52.0 && y1 >= C.1 + 52.0);
    }

    #[test]
    fn sans_flashs_au_plus_trois_eclats_par_seconde() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        s.doux = true;
        let mut eclats = 0;
        // 8 kicks par seconde pendant une seconde
        for i in 0..60 {
            let au = AudioView { kicks: i / 7, ..Default::default() };
            let avant = s.env;
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, au, &cfg, 1.0);
            if s.env > avant + 0.1 {
                eclats += 1;
            }
            assert!(s.env <= 0.5 + 1e-6, "éclat trop fort : {}", s.env);
        }
        assert!((2..=3).contains(&eclats), "{eclats} éclats");
    }

    #[test]
    fn rafale_de_changement_de_piste() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        s.update(0.0, 0.016, (100.0, 100.0), AudioView::default(), &cfg, 1.0);
        let dir = s.dir;
        s.rafale(100.0, 100.0, 0.016, &cfg, 1.0);
        // pas de changement de sens ; 2 × 3 satellites, l'éclat et une onde
        assert_eq!((s.dir, s.sats.len(), s.env), (dir, 6, 1.0));
        assert_eq!(s.particules.iter().filter(|p| p.genre == Genre::Onde).count(), 1);
        // dans la limite du maximum
        let mut cfg = cfg;
        cfg.essaim.maximum = 7;
        s.rafale(100.0, 100.0, 0.02, &cfg, 1.0);
        assert_eq!(s.sats.len(), 7);
        // l'onde reste dans le rectangle redessiné
        s.update(0.03, 0.01, (100.0, 100.0), AudioView::default(), &cfg, 1.0);
        let (x0, y0, x1, y1) = s.bounds((100.0, 100.0), 1.0, cfg.eclat.taille);
        assert!(x0 <= 100.0 - ONDE_PISTE.0 && x1 >= 100.0 + ONDE_PISTE.0 && y0 <= 100.0 - ONDE_PISTE.0 && y1 >= 100.0 + ONDE_PISTE.0);
    }

    #[test]
    fn kick_inverse_le_sens_et_ajoute_des_satellites() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        let mut au = AudioView::default();
        s.update(0.0, 0.016, (100.0, 100.0), au, &cfg, 1.0);
        let dir = s.dir;
        au.kicks = 1;
        s.update(0.016, 0.016, (100.0, 100.0), au, &cfg, 1.0);
        assert_eq!(s.dir, -dir);
        assert!((2..=3).contains(&s.sats.len()));
        // les satellites disparaissent après leur durée de vie
        s.update(10.0, 0.016, (100.0, 100.0), au, &cfg, 1.0);
        assert!(s.sats.is_empty());
    }

    #[test]
    fn relais_du_tempo_pulse_doucement_sans_toucher_au_sens() {
        let mut cfg = Config::default();
        let mut s = Swarm::new();
        let au = AudioView { relais: true, phase: 0.0, bpm: Some(128.0), ..Default::default() };
        s.update(0.0, 0.016, (100.0, 100.0), AudioView::default(), &cfg, 1.0);
        let dir = s.dir;
        s.update(0.016, 0.016, (100.0, 100.0), au, &cfg, 1.0);
        assert!((s.env - FORCE_PULSATION).abs() < 1e-4, "sur le temps : {}", s.env);
        assert_eq!((s.dir, s.sats.len()), (dir, 0), "un temps relais n'est pas un kick");
        // milieu du temps : presque retombé
        s.env = 0.0;
        s.update(0.032, 0.016, (100.0, 100.0), AudioView { phase: 0.5, ..au }, &cfg, 1.0);
        assert!(s.env < 0.03, "{}", s.env);
        // réglage coupé : rien
        cfg.audio.pulsation_tempo = false;
        s.env = 0.0;
        s.update(0.048, 0.016, (100.0, 100.0), au, &cfg, 1.0);
        assert_eq!(s.env, 0.0);
    }

    #[test]
    fn compteur_qui_recule_aucun_nouveau() {
        assert_eq!(nouveaux(None, 12), 0);
        assert_eq!(nouveaux(Some(3), 5), 2);
        assert_eq!(nouveaux(Some(3), 50), 4);
        // analyse rouverte : le compteur repart de 0
        assert_eq!(nouveaux(Some(57), 0), 0);
        // tour complet du compteur
        assert_eq!(nouveaux(Some(u32::MAX), 1), 2);
        let cfg = Config::default();
        let mut s = Swarm::new();
        let au = AudioView { kicks: 40, ..Default::default() };
        s.update(0.0, 0.016, (100.0, 100.0), au, &cfg, 1.0);
        let dir = s.dir;
        s.update(0.016, 0.016, (100.0, 100.0), AudioView { kicks: 0, ..au }, &cfg, 1.0);
        assert_eq!((s.dir, s.sats.len()), (dir, 0), "faux kicks");
    }

    #[test]
    fn la_vitesse_ne_depend_pas_de_la_cadence() {
        // une seconde à 10 images/s ou à 60 : l'essaim a tourné pareil (avant : bridé à 0,05 s
        // par image, il faisait moitié moins de chemin à 10 images/s)
        let cfg = Config::default();
        let au = AudioView { bass: 0.6, pitch: 40.0, ..Default::default() };
        let angle = |ips: u32| {
            let mut s = Swarm::new();
            let dt = 1.0 / ips as f32;
            let mut total = 0.0f32;
            s.avance(0.0, dt, (300.0, 300.0), au, &cfg, 1.0);
            for i in 1..=ips {
                let a0 = s.angles[0];
                s.avance(i as f64 * dt as f64, dt, (300.0, 300.0), au, &cfg, 1.0);
                total += (s.angles[0] - a0 + 3.0 * PI).rem_euclid(TAU) - PI;
            }
            total
        };
        let (lent, vif) = (angle(10), angle(60));
        assert!((lent - vif).abs() < 0.05 * vif.abs(), "10 i/s : {lent} rad, 60 i/s : {vif} rad");
        // retour de pause : un écart de plusieurs secondes n'est pas rattrapé
        let mut s = Swarm::new();
        s.avance(0.0, 0.016, (0.0, 0.0), au, &cfg, 1.0);
        s.avance(30.0, 30.0, (0.0, 0.0), au, &cfg, 1.0);
        assert!(s.angles[0].is_finite());
    }

    #[test]
    fn un_son_non_fini_ne_fige_pas_l_essaim() {
        let mut cfg = Config::default();
        cfg.orbite.rotation = 20.0;
        let mut s = Swarm::new();
        let nan = AudioView { bass: f32::NAN, pitch: f32::NAN, brillance: f32::INFINITY, ..Default::default() };
        for i in 0..10 {
            s.update(i as f64 * 0.016, 0.016, (100.0, 100.0), nan, &cfg, 1.0);
        }
        s.update(0.2, 0.016, (100.0, 100.0), AudioView::default(), &cfg, 1.0);
        assert!(s.spd.is_finite() && s.far.is_finite() && s.rot.is_finite() && s.angles.iter().all(|a| a.is_finite()));
        assert!(s.trails[0].back().is_some_and(|p| p.1.is_finite() && p.2.is_finite()));
        let d = decalage();
        set_decalage(f32::NAN);
        assert_eq!(decalage(), d);
    }

    #[test]
    fn dessins_successifs_identiques_tampons_gardes() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        let au = AudioView { kicks: 0, bass: 0.8, pitch: 40.0, ..Default::default() };
        for i in 0..100 {
            s.update(i as f64 * 0.007, 0.007, (300.0, 300.0), AudioView { kicks: i / 25, ..au }, &cfg, 1.0);
        }
        let (a, _) = render(&s, (300.0, 300.0), 0.7, &cfg, 1.0);
        let cap = (s.tampons.borrow().heads.capacity(), s.tampons.borrow().xy.capacity());
        assert!(cap.0 > 0 && cap.1 > 0);
        let (b, _) = render(&s, (300.0, 300.0), 0.7, &cfg, 1.0);
        assert!(a.data() == b.data(), "le second dessin diffère");
        assert_eq!(cap, (s.tampons.borrow().heads.capacity(), s.tampons.borrow().xy.capacity()));
    }

    #[test]
    fn basse_grave_eloigne_et_accelere() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        let quiet = AudioView { kicks: 0, bass: 0.0, pitch: 0.0, ..Default::default() };
        let deep = AudioView { kicks: 0, bass: 1.0, pitch: 30.0, ..Default::default() };
        for i in 0..60 {
            s.update(i as f64 * 0.016, 0.016, (0.0, 0.0), quiet, &cfg, 1.0);
        }
        let (far0, spd0) = (s.far, s.spd);
        for i in 60..90 {
            s.update(i as f64 * 0.016, 0.016, (0.0, 0.0), deep, &cfg, 1.0);
        }
        assert!(s.far > far0 + 1.0, "éloignement {} → {}", far0, s.far);
        assert!(s.spd > spd0 + 8.0, "vitesse {} → {}", spd0, s.spd);
    }

    #[test]
    fn la_trainee_se_dessine_a_haute_cadence() {
        // 144 images/s pendant 0,5 s : la traînée doit compter plusieurs points et laisser
        // de la couleur loin de la tête.
        let cfg = Config::default();
        let mut s = Swarm::new();
        let dt = 1.0 / 144.0;
        for i in 0..72 {
            s.update(i as f64 * dt as f64, dt, (100.0, 100.0), AudioView::default(), &cfg, 1.0);
        }
        assert!(s.trails[0].len() >= 10, "{} points", s.trails[0].len());
        let (pm, _) = render(&s, (100.0, 100.0), 71.0 * dt as f64, &cfg, 1.0);
        let painted = pm.data().chunks_exact(4).filter(|p| p[3] > 40).count();
        assert!(painted > 150, "{painted} pixels colorés");
    }

    /// Dessine comme la boucle principale : image ajustée au rectangle de `bounds`.
    fn render(s: &Swarm, cursor: (f32, f32), t: f64, cfg: &Config, scale: f32) -> (Pixmap, (f32, f32)) {
        let (x0, y0, x1, y1) = s.bounds(cursor, scale, cfg.eclat.taille);
        let origin = (x0.floor(), y0.floor());
        let mut pm = Pixmap::new((x1 - origin.0).ceil() as u32, (y1 - origin.1).ceil() as u32).unwrap();
        s.draw(&mut crate::canvas::PixCanvas { pm: &mut pm, origin }, t, cfg, scale, cursor);
        (pm, origin)
    }

    #[test]
    fn la_trainee_s_etire_derriere_la_souris() {
        // la souris traverse 500 px en 0,15 s : les traînées restent en arrière, la fenêtre
        // s'agrandit pour les contenir, et de la couleur est dessinée loin derrière le pointeur
        let cfg = Config::default();
        let mut s = Swarm::new();
        let dt = 1.0 / 144.0;
        for i in 0..40 {
            s.update(i as f64 * dt as f64, dt, (100.0, 300.0), AudioView::default(), &cfg, 1.0);
        }
        let mut t = 40.0 * dt as f64;
        for i in 0..22 {
            let x = 100.0 + 500.0 * (i as f32 / 21.0);
            s.update(t, dt, (x, 300.0), AudioView::default(), &cfg, 1.0);
            t += dt as f64;
        }
        let cursor = (600.0, 300.0);
        let (x0, _, x1, _) = s.bounds(cursor, 1.0, cfg.eclat.taille);
        assert!(x1 - x0 > 300.0, "fenêtre de {} px", x1 - x0);
        let (pm, origin) = render(&s, cursor, t, &cfg, 1.0);
        // colonne de pixels 250 px derrière le pointeur
        let col = (cursor.0 - 250.0 - origin.0) as u32;
        let painted = (0..pm.height()).filter(|&y| pm.pixel(col, y).is_some_and(|p| p.alpha() > 30)).count();
        assert!(painted > 0, "aucune traînée 250 px derrière le pointeur");
    }

    /// Mesure du coût d'une image avec un essaim plein (`cargo test --release -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn mesure_cout_image() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        let dt = 1.0 / 144.0;
        let mut au = AudioView { kicks: 0, bass: 0.8, pitch: 40.0, ..Default::default() };
        for i in 0..1000 {
            if i % 30 == 0 {
                au.kicks += 1;
            }
            s.update(i as f64 * dt as f64, dt, (300.0, 300.0), au, &cfg, 1.5);
        }
        let n = 500;
        let t0 = std::time::Instant::now();
        let mut size = 0;
        for _ in 0..n {
            let (pm, _) = render(&s, (300.0, 300.0), 1000.0 * dt as f64, &cfg, 1.5);
            size = pm.width();
        }
        let per = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
        eprintln!("{} satellites, image {size} px de large : {per:.3} ms par image", s.sats.len());
    }

    /// Coût du dessin de 12 fissures (le maximum) sur processeur :
    /// `cargo test --release mesure_fissures -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn mesure_fissures() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let cracks: Vec<Crack> = (0..12).map(|i| Crack::new(200.0 + 60.0 * i as f32, 300.0, 0.0, 1.5, &mut rng)).collect();
        let segs: usize = cracks.iter().map(|c| c.segs.len()).sum();
        let mut pm = Pixmap::new(1200, 600).unwrap();
        for glow in [0.0f32, 1.0] {
            let n = 200;
            let t0 = std::time::Instant::now();
            for _ in 0..n {
                let cv = &mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) };
                for c in &cracks {
                    c.draw(cv, glow, (120, 160, 255), 1.0);
                }
            }
            eprintln!("12 fissures ({segs} segments), éclat {glow} : {:.3} ms par image", t0.elapsed().as_secs_f64() * 1000.0 / n as f64);
        }
    }

    #[test]
    fn fissure_recopiee_comme_tracee() {
        // l'image des passes fixes, recopiée, donne le même dessin que le tracé direct
        struct SansImage<'a>(crate::canvas::PixCanvas<'a>);
        impl Canvas for SansImage<'_> {
            fn line(&mut self, a: (f32, f32), b: (f32, f32), w: f32, c: (u8, u8, u8), al: f32, sm: bool) {
                self.0.line(a, b, w, c, al, sm);
            }
            fn polyline(&mut self, p: &[(f32, f32)], w: f32, c: (u8, u8, u8), al: f32, sm: bool) {
                self.0.polyline(p, w, c, al, sm);
            }
            fn circle(&mut self, x: f32, y: f32, r: f32, c: (u8, u8, u8), al: f32) {
                self.0.circle(x, y, r, c, al);
            }
            fn ring(&mut self, x: f32, y: f32, r: f32, w: f32, c: (u8, u8, u8), al: f32) {
                self.0.ring(x, y, r, w, c, al);
            }
            fn voile(&mut self, z: (f32, f32, f32, f32), x: f32, y: f32, r: f32, f: f32, c: (u8, u8, u8), al: f32) {
                self.0.voile(z, x, y, r, f, c, al);
            }
        }
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let c = Crack::new(300.0, 200.0, 0.0, 1.5, &mut rng);
        let points: usize = c.branches.iter().map(Vec::len).sum();
        assert_eq!(points, c.segs.len() + c.branches.len(), "branches incomplètes");
        let (mut a, mut b) = (Pixmap::new(600, 400).unwrap(), Pixmap::new(600, 400).unwrap());
        for _ in 0..2 {
            a.fill(tiny_skia::Color::TRANSPARENT);
            c.draw(&mut crate::canvas::PixCanvas { pm: &mut a, origin: (10.0, 20.0) }, 0.0, (255, 0, 0), 1.0);
        }
        c.draw(&mut SansImage(crate::canvas::PixCanvas { pm: &mut b, origin: (10.0, 20.0) }), 0.0, (255, 0, 0), 1.0);
        let ecart = a.data().iter().zip(b.data()).map(|(p, q)| p.abs_diff(*q) as u32).max().unwrap();
        let peint = a.data().chunks_exact(4).filter(|p| p[3] > 100).count();
        assert!(peint > 100 && ecart <= 40, "{peint} pixels, écart {ecart}");
    }

    #[test]
    fn tout_tient_dans_le_rectangle() {
        // orbite la plus large (basse très grave) : têtes et traînées dans le rectangle
        let cfg = Config::default();
        let mut s = Swarm::new();
        let au = AudioView { kicks: 0, bass: 1.0, pitch: 30.0, ..Default::default() };
        for i in 0..200 {
            let au = AudioView { kicks: (i / 20) as u32, ..au };
            s.update(i as f64 * 0.007, 0.007, (500.0, 500.0), au, &cfg, 1.5);
        }
        let (x0, y0, x1, y1) = s.bounds((500.0, 500.0), 1.5, cfg.eclat.taille);
        for tr in s.trails.iter().chain(s.sats.iter().chain(&s.fixes).map(|q| &q.trail)) {
            for &(_, x, y) in tr {
                assert!(x > x0 && x < x1 && y > y0 && y < y1, "point ({x}, {y}) hors du rectangle");
            }
        }
    }

    #[test]
    fn formes_anneaux_et_permanents_tiennent_dans_le_rectangle() {
        use crate::config::{Forme, FormeSatellites};
        for (forme, sat) in [
            (Forme::Cercle, FormeSatellites::Cometes),
            (Forme::Triangle, FormeSatellites::Cometes),
            (Forme::Losange, FormeSatellites::Huit),
            (Forme::Hexagone, FormeSatellites::Triangle),
            (Forme::Huit, FormeSatellites::Cometes),
            (Forme::Etoile, FormeSatellites::Fleur),
            (Forme::Fleur, FormeSatellites::Cometes),
            (Forme::Coeur, FormeSatellites::Lissajous),
            (Forme::Astroide, FormeSatellites::Carre),
        ] {
            let mut cfg = Config::default();
            (cfg.orbite.forme, cfg.orbite.nombre, cfg.orbite.anneaux, cfg.essaim.fixes, cfg.essaim.forme) = (forme, 8, 4, 32, sat);
            cfg.orbite.rotation = 45.0;
            let mut s = Swarm::new();
            let au = AudioView { kicks: 0, bass: 1.0, pitch: 30.0, ..Default::default() };
            for i in 0..200 {
                s.update(i as f64 * 0.007, 0.007, (500.0, 500.0), au, &cfg, 1.5);
            }
            assert_eq!((s.trails.len(), s.fixes.len()), (32, 32), "{forme:?}");
            let (x0, y0, x1, y1) = s.bounds((500.0, 500.0), 1.5, cfg.eclat.taille);
            for tr in s.trails.iter().chain(s.fixes.iter().map(|q| &q.trail)) {
                for &(_, x, y) in tr {
                    assert!(x > x0 && x < x1 && y > y0 && y < y1, "{forme:?} : ({x}, {y}) hors du rectangle");
                }
            }
        }
    }

    #[test]
    fn les_satellites_suivent_la_forme_des_cometes_par_defaut() {
        use crate::config::{Forme, FormeSatellites};
        for (sat, attendue) in [(FormeSatellites::Cometes, Forme::Triangle), (FormeSatellites::Huit, Forme::Huit)] {
            let mut cfg = Config::default();
            (cfg.orbite.forme, cfg.essaim.fixes, cfg.essaim.forme) = (Forme::Triangle, 6, sat);
            let mut s = Swarm::new();
            for i in 0..20 {
                s.update(i as f64 * 0.01, 0.01, (300.0, 300.0), AudioView::default(), &cfg, 1.0);
            }
            for q in &s.fixes {
                let &(_, x, y) = q.trail.back().unwrap();
                let rr = cfg.orbite.rayon * q.r * s.far;
                let (ex, ey) = chemin(attendue, q.a);
                assert!((x - 300.0 - ex * rr).abs() < 0.01 && (y - 300.0 - ey * rr).abs() < 0.01, "{sat:?} : ({x}, {y})");
            }
        }
    }

        #[test]
    fn la_trainee_reste_lisse_a_grande_vitesse() {
        // basse au maximum, 60 images/s : l'essaim fait ~3 tours par seconde ; entre deux points
        // de traînée, l'angle vu du centre doit rester petit (pas de ligne brisée)
        let cfg = Config::default();
        let mut s = Swarm::new();
        let au = AudioView { kicks: 0, bass: 1.0, pitch: 30.0, ..Default::default() };
        for i in 0..120 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, (400.0, 400.0), au, &cfg, 1.0);
        }
        assert!(s.spd > 12.0, "vitesse {}", s.spd);
        let tr = &s.trails[0];
        assert!(tr.len() > 30, "{} points", tr.len());
        let angle = |p: &(f64, f32, f32)| (p.2 - 400.0).atan2(p.1 - 400.0);
        for w in tr.iter().collect::<Vec<_>>().windows(2) {
            let d = (angle(w[1]) - angle(w[0]) + 3.0 * PI).rem_euclid(TAU) - PI;
            assert!(d.abs() < 0.08, "écart de {d} rad entre deux points");
        }
    }

        #[test]
    fn particules_dans_le_rectangle_et_eteintes() {
        let mut cfg = Config::default();
        (cfg.clic.onde, cfg.clic.eclats, cfg.sillage.actif) = (true, true, true);
        let mut s = Swarm::new();
        let mut au = AudioView::default();
        let dt = 1.0 / 144.0;
        for i in 0..200 {
            au.aigus = i / 10;
            // souris qui file : 1200 px/s
            let cursor = (300.0 + i as f32 * 8.0, 400.0);
            s.update(i as f64 * dt as f64, dt, cursor, au, &cfg, 1.0);
            if i == 100 {
                s.clic(cursor.0, cursor.1, i as f64 * dt as f64, &cfg, 1.0);
            }
            let (x0, y0, x1, y1) = s.bounds(cursor, 1.0, cfg.eclat.taille);
            for p in &s.particules {
                let e = p.portee();
                assert!(p.x - e >= x0 && p.x + e <= x1 && p.y - e >= y0 && p.y + e <= y1, "particule hors du rectangle");
            }
        }
        assert!(s.particules.iter().any(|p| p.genre == Genre::Poussiere) || s.particules.len() <= MAX_PARTICULES);
        // une seconde plus tard, sans rien de nouveau, tout s'est éteint
        for i in 200..400 {
            s.update(i as f64 * dt as f64, dt, (1900.0, 400.0), AudioView { aigus: 19, ..au }, &cfg, 1.0);
        }
        assert!(s.particules.is_empty(), "{} particules encore là", s.particules.len());
    }

    #[test]
    fn trace_reechantillonne_et_borne() {
        // un carré dessiné grossièrement, décentré
        let pts = [[10.0, 10.0], [30.0, 10.0], [30.0, 30.0], [10.0, 30.0]];
        let r = reechantillonner(&pts);
        assert_eq!(r.len(), 256);
        let max = r.iter().map(|p| (p.0 * p.0 + p.1 * p.1).sqrt()).fold(0.0f32, f32::max);
        assert!((max - 1.2).abs() < 1e-3, "{max}");
        assert!(reechantillonner(&[[0.0, 0.0], [1.0, 1.0]]).is_empty());
    }

    #[test]
    fn les_trajets_sont_continus() {
        use crate::config::Forme;
        for forme in [
            Forme::Cercle, Forme::Triangle, Forme::Losange, Forme::Hexagone, Forme::Huit, Forme::Etoile, Forme::Fleur,
            Forme::Carre, Forme::Pentagone, Forme::Ellipse, Forme::Coeur, Forme::Trefle, Forme::Lissajous, Forme::Astroide,
        ] {
            let mut p = chemin(forme, 0.0);
            // chaque trajet tient dans le rayon prévu par les bornes du dessin (1,3)
            for i in 0..720 {
                let (x, y) = chemin(forme, i as f32 * TAU / 720.0);
                assert!((x * x + y * y).sqrt() <= 1.31, "{forme:?} : ({x}, {y}) trop loin");
            }
            for i in 1..=720 {
                let q = chemin(forme, i as f32 * TAU / 720.0);
                let d = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
                assert!(d < 0.05, "{forme:?} : saut de {d} à l'angle {i}/720");
                p = q;
            }
        }
    }

    #[test]
    fn saut_d_ecran_la_zone_contient_tout_et_reste_bornee() {
        // le pointeur saute de 3000 px (autre écran) : la vieille traînée est abandonnée, la
        // zone contient tout ce qui reste et ne dépasse pas l'étendue maximale
        let cfg = Config::default();
        let mut s = Swarm::new();
        let dt = 1.0 / 144.0;
        for i in 0..100 {
            s.update(i as f64 * dt as f64, dt, (500.0, 500.0), AudioView::default(), &cfg, 1.0);
        }
        let far = (3500.0, -900.0);
        s.update(100.0 * dt as f64, dt, far, AudioView::default(), &cfg, 1.5);
        let (x0, y0, x1, y1) = s.bounds(far, 1.5, cfg.eclat.taille);
        assert!(x1 - x0 <= MAX_EXTENT + 100.0 && y1 - y0 <= MAX_EXTENT + 100.0, "zone {x0},{y0} → {x1},{y1}");
        for tr in s.trails.iter().chain(s.sats.iter().chain(&s.fixes).map(|q| &q.trail)) {
            for &(_, x, y) in tr {
                assert!(x > x0 && x < x1 && y > y0 && y < y1, "point ({x}, {y}) hors de la zone");
            }
        }
    }
}

//! Simulation et dessin : le point, les deux comètes, l'essaim de satellites et les fissures.
//!
//! Règles (« Essaim durable ») : chaque kick inverse le sens de rotation et ajoute des
//! satellites ; la quantité de basse règle la vitesse ; plus la note de basse est grave, plus
//! les orbites s'éloignent. Les traînées sont gardées en coordonnées écran : quand la souris
//! bouge, elles s'étirent derrière elle (la fenêtre s'agrandit le temps du mouvement).
//!
//! Au-delà du kick (`[musique]`) : pendant une montée l'essaim se resserre et accélère, puis
//! explose au drop ; un carré s'ouvre sur chaque caisse claire ; les têtes pulsent sur les
//! contretemps ; l'essaim penche vers le côté qui joue le plus fort. Les modulations
//! (`modulation.rs`) font agir une source du son sur la rotation, la vitesse, le rayon, la
//! taille ou la teinte des comètes ou des satellites. Tout est reproduit dans `ui/apercu.js`.

use crate::audio::AudioView;
use crate::config::{Action, Config, Declencheur, Evenement, Forme};
use crate::modulation::{self, Effet, Sources};
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use std::sync::atomic::{AtomicU32, Ordering};
use crate::canvas::Canvas;

/// Marge autour de tout ce qui est dessiné, décalage maximal du centre lissé, et étendue
/// maximale de la fenêtre : un point de traînée à plus de `MAX_EXTENT / 2` du pointeur (laissé
/// derrière par un saut du pointeur) est retiré. Le trajet lui-même est ramené en deçà
/// (`borne_etendue`).
const MARGIN: f32 = 12.0;
const CENTER_LAG_MAX: f32 = 30.0;
const MAX_EXTENT: f32 = 2400.0;
/// Distance au centre (en rayons) des satellites les plus écartés, et part qu'ils dépassent au plus
/// en mode ressorts (nés au centre, ils rejoignent leur place avec un dépassement de ~37 %).
const SAT_R_MAX: f32 = 1.9;
const SAT_DEPASSEMENT: f32 = 1.4;
/// Point le plus loin du centre du trajet `forme` de rayon 1 (`chemin`) : sommet d'un polygone,
/// bout d'un pétale, mesuré pour le cœur et la courbe de Lissajous (arrondi au-dessus), 1,2 pour
/// des contours (`mettre_a_l_echelle`). Vérifié par `les_trajets_sont_continus`.
fn etendue_forme(forme: Forme) -> f32 {
    match forme {
        Forme::Cercle => 1.0,
        Forme::Hexagone => 1.05,
        Forme::Pentagone => 1.1,
        Forme::Losange => 1.15,
        Forme::Lissajous => 1.19,
        Forme::Triangle | Forme::Huit | Forme::Trefle | Forme::Trace | Forme::Icone => 1.2,
        Forme::Coeur => 1.22,
        Forme::Etoile | Forme::Fleur | Forme::Carre => 1.25,
        Forme::Ellipse | Forme::Astroide => 1.3,
    }
}
const SAT_TRAIL: f64 = 0.25;
const TRAIL_STEP: f64 = 1.0 / 90.0;
/// Finesse des traînées (angle maximal en radians, écart maximal en px entre deux points) :
/// les comètes, larges et longues, sont lissées de près ; les satellites, fins et courts, moins.
const FINESSE_COMETES: (f32, f32) = (0.045, 4.0);
const FINESSE_SATELLITES: (f32, f32) = (0.1, 8.0);

/// Redressement du trajet : quand sa rotation (réglage plus modulations) retombe sous
/// `REDRESSE.0` tours/min, il revient à l'endroit (angle 0, par le plus court) à `REDRESSE.1`
/// par seconde, en douceur : une modulation retirée, la retombée d'un drop ou le silence ne le
/// laissent pas de travers.
const REDRESSE: (f32, f32) = (2.0, 3.0);

/// Pas de rotation du trajet pendant `dt` : celui de la vitesse `tours_min`, plus le
/// redressement vers l'angle 0 d'autant plus fort que la rotation est lente.
fn pas_rotation(rot: f32, tours_min: f32, dt: f32) -> f32 {
    let ecart = (PI - rot).rem_euclid(TAU) - PI;
    let poids = (1.0 - tours_min.abs() / REDRESSE.0).clamp(0.0, 1.0);
    tours_min * TAU / 60.0 * dt + ecart * (1.0 - (-REDRESSE.1 * dt).exp()) * poids
}

/// Pas intermédiaires pour un déplacement d'angle `da` sur un rayon de `r` px, pour que la
/// traînée reste une courbe (et non une ligne brisée) quand l'essaim tourne vite.
fn sous_pas(da: f32, r: f32, (angle, ecart): (f32, f32)) -> usize {
    (da.abs() / angle).max(da.abs() * r / ecart).ceil().clamp(1.0, 24.0) as usize
}

/// Ressorts (`[ressorts]`, actions `rebond`, `poussee`, `frein`) : élan donné au rayon par un
/// rebond de force 1 (rayons par seconde) ; vitesse ajoutée par une poussée de force 1
/// (radians/s) et sa retombée (par seconde) ; part de la vitesse retirée par un coup de frein et
/// sa retombée (par seconde) ; tremblement du rayon pendant une montée (part du rayon, rad/s).
const REBOND: f32 = 6.67;
const REBOND_MAX: f32 = 3.0 * REBOND;
const POUSSEE: (f32, f32) = (5.0, 4.0);
const FREIN: (f32, f32) = (0.8, 9.0);
const TREMBLE: (f32, f64) = (0.053, 46.0);
/// Satellites du mode ressorts : pulsation de leur ressort (tirée entre les deux, rad/s) et son
/// amortissement ; durée de leur entrée (avec dépassement) et de leur sortie (s) ; part de leur
/// taille perdue en fin de vie ; part de leur traînée gardée (sans inversion du sens, les traînées
/// restent déployées : plus courtes, elles ne masquent pas plus que celles d'avant).
const SAT_RESSORT: (f32, f32, f32) = (10.0, 16.0, 0.3);
const SAT_ENTREE_SORTIE: (f32, f32) = (0.3, 0.35);
const SAT_FONTE: f32 = 0.6;
const SAT_TRAIL_RESSORTS: f64 = 0.68;
/// Tête étirée du mode ressorts : temps de trajet qu'elle couvre (s) et longueur au plus (px à 100 %).
const ETIREMENT: (f32, f32) = (0.02, 8.7);
/// Éclat du mode ressorts : rayon de l'anneau (en tailles d'éclat) et son épaisseur de départ
/// (px à 100 %).
const ANNEAU_RESSORTS: (f32, f32) = (5.0, 4.5);
/// Rubans (`[rubans]`) : profondeur de la vrille à fond, son défilement (rad/s), le saut que lui
/// donne chaque caisse claire (rad) ; segments de traînée par morceau tracé.
const VRILLE: (f32, f64, f32) = (0.58, 2.4, 1.4);
const RUBAN_MORCEAU: usize = 2;

/// Avance de `dt` un ressort amorti : `x` son écart à la cible, `v` sa vitesse, `w` sa pulsation
/// (rad/s), `z` son amortissement. En petits pas (w × pas ≤ 0,25) : stable à toute cadence.
pub(crate) fn ressort(x: &mut f32, v: &mut f32, w: f32, z: f32, dt: f32) {
    let n = (w * dt / 0.25).ceil().clamp(1.0, 16.0);
    let h = dt / n;
    for _ in 0..n as usize {
        *v += (-w * w * *x - 2.0 * z * w * *v) * h;
        *x += *v * h;
    }
    if !(x.is_finite() && v.is_finite()) {
        (*x, *v) = (0.0, 0.0);
    }
}

/// Sortie avec dépassement, de 0 à 1 : dépasse 1 d'un dixième avant d'y revenir.
pub(crate) fn depasse(k: f32) -> f32 {
    let (c, u) = (1.70158f32, k.clamp(0.0, 1.0) - 1.0);
    1.0 + (c + 1.0) * u * u * u + c * u * u
}

/// Petit générateur pseudo-aléatoire (xorshift), suffisant pour des effets visuels.
pub struct Rng(u64);
impl Rng {
    /// Suite fixée (essais : même suite à chaque fois).
    #[cfg(test)]
    pub fn graine(g: u64) -> Self {
        Self(g | 1)
    }
    /// Suite tirée de l'heure ; dans les essais, toujours la même (un échec se rejoue).
    #[cfg(not(test))]
    pub fn seeded() -> Self {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        Self(t | 1)
    }
    #[cfg(test)]
    pub fn seeded() -> Self {
        Self::graine(0x2545_f491_4f6c_dd1d)
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

/// Décalage de teinte (degrés) de tout ce qu'Essaim dessine : modulations de la teinte « sur
/// tout » (`Swarm::teinte_globale`), passé par la boucle principale.
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


/// Contours du trajet « image » (`[orbite] contours`, ou le trajet dessiné d'avant) et de
/// l'icône de l'application au premier plan, mis à l'échelle ensemble (vide : cercle).
type Contours = std::sync::RwLock<Vec<Contour>>;
static TRACES: Contours = std::sync::RwLock::new(Vec::new());
static ICONE: Contours = std::sync::RwLock::new(Vec::new());

/// Un contour fermé, parcouru à vitesse constante : ses sommets (coins gardés) et la longueur
/// parcourue jusqu'à chacun.
#[derive(Clone, Debug)]
struct Contour {
    points: Vec<(f32, f32)>,
    cumul: Vec<f32>,
    total: f32,
}

impl Contour {
    fn new(points: Vec<(f32, f32)>) -> Option<Self> {
        let n = points.len();
        if n < 3 {
            return None;
        }
        let mut cumul = Vec::with_capacity(n);
        let mut total = 0.0f32;
        for i in 0..n {
            cumul.push(total);
            let (a, b) = (points[i], points[(i + 1) % n]);
            total += ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        }
        (total > 1e-6).then_some(Self { points, cumul, total })
    }

    /// Point à la fraction `u` (0 à 1) du tour.
    fn point(&self, u: f32) -> (f32, f32) {
        let s = u.rem_euclid(1.0) * self.total;
        let i = self.cumul.partition_point(|&c| c <= s).saturating_sub(1);
        let (a, b) = (self.points[i], self.points[(i + 1) % self.points.len()]);
        let long = self.cumul.get(i + 1).copied().unwrap_or(self.total) - self.cumul[i];
        let f = if long > 1e-9 { ((s - self.cumul[i]) / long).clamp(0.0, 1.0) } else { 0.0 };
        (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
    }
}

/// Version des contours du trajet « image » et de ceux de l'icône : comptée à chaque vrai
/// changement. Le trajet change alors de forme : les traînées qui le suivent repartent de zéro
/// (`Swarm::update`), sinon elles relieraient l'ancien contour au nouveau.
static VERSION_TRACES: AtomicU32 = AtomicU32::new(0);
static VERSION_ICONE: AtomicU32 = AtomicU32::new(0);

/// Remplace les contours de `cible` ; sa version n'avance que s'ils ont vraiment changé (les
/// contours de l'image sont redonnés à chaque relecture des réglages).
fn remplacer_contours(cible: &Contours, version: &AtomicU32, contours: &[Vec<[f32; 2]>]) {
    let neufs = mettre_a_l_echelle(contours);
    if let Ok(mut t) = cible.write() {
        let pareils = t.len() == neufs.len() && t.iter().zip(&neufs).all(|(a, b)| a.points == b.points);
        if !pareils {
            *t = neufs;
            version.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Remplace les contours du trajet « image » (`Orbite::chemins`).
pub fn set_traces(contours: &[Vec<[f32; 2]>]) {
    remplacer_contours(&TRACES, &VERSION_TRACES, contours);
}

/// Remplace les contours de l'icône de l'application au premier plan.
pub fn set_icone(contours: &[Vec<[f32; 2]>]) {
    remplacer_contours(&ICONE, &VERSION_ICONE, contours);
}

/// Version des contours que suit la forme (0 pour une forme calculée).
fn version_contours(forme: Forme) -> u32 {
    match forme {
        Forme::Trace => VERSION_TRACES.load(Ordering::Relaxed),
        Forme::Icone => VERSION_ICONE.load(Ordering::Relaxed),
        _ => 0,
    }
}

/// Contours (`CONTOURS_MAX` au plus, 2000 points chacun) centrés sur le milieu de l'ensemble et
/// mis à la même échelle (point le plus loin du centre : 1,2), pour garder leur place les uns
/// par rapport aux autres (les yeux et la bouche d'un visage, les lettres d'un mot). Un seul
/// contour : centré sur lui-même, comme le trajet dessiné d'avant.
fn mettre_a_l_echelle(contours: &[Vec<[f32; 2]>]) -> Vec<Contour> {
    let tous: Vec<Vec<(f32, f32)>> = contours
        .iter()
        .take(crate::contours::CONTOURS_MAX)
        .map(|c| c.iter().filter(|q| q[0].is_finite() && q[1].is_finite()).take(2000).map(|q| (q[0], q[1])).collect::<Vec<_>>())
        .filter(|c| c.len() >= 3)
        .collect();
    // milieu : celui du cadre qui les englobe (un contour très détaillé d'un côté ne le tire pas)
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for q in tous.iter().flatten() {
        (x0, y0, x1, y1) = (x0.min(q.0), y0.min(q.1), x1.max(q.0), y1.max(q.1));
    }
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let r = tous.iter().flatten().map(|q| ((q.0 - cx).powi(2) + (q.1 - cy).powi(2)).sqrt()).fold(0.0f32, f32::max).max(1e-6);
    tous.into_iter().filter_map(|c| Contour::new(c.into_iter().map(|q| ((q.0 - cx) / r * 1.2, (q.1 - cy) / r * 1.2)).collect())).collect()
}

/// Contours de l'icône de l'application au premier plan, déjà mis à l'échelle (pour l'aperçu des
/// réglages).
pub fn contours_icone() -> Vec<Vec<[f32; 2]>> {
    ICONE.read().map(|t| t.iter().map(|c| c.points.iter().map(|p| [p.0, p.1]).collect()).collect()).unwrap_or_default()
}

/// Contours de la forme (image ou icône) ; aucun pour les formes calculées.
fn contours_de(forme: Forme) -> Option<&'static Contours> {
    match forme {
        Forme::Trace => Some(&TRACES),
        Forme::Icone => Some(&ICONE),
        _ => None,
    }
}

/// Nombre de contours de la forme (1 pour une forme calculée ou sans contour).
pub fn nombre_contours(forme: Forme) -> usize {
    contours_de(forme).and_then(|c| c.read().ok().map(|t| t.len())).unwrap_or(0).max(1)
}

/// Point du contour `i` de la forme à l'angle `a` (le contour est parcouru à vitesse constante) ;
/// cercle s'il n'y en a pas.
fn sur_contour(forme: Forme, i: usize, a: f32) -> (f32, f32) {
    let Some(Ok(t)) = contours_de(forme).map(|c| c.read()) else { return (a.cos(), a.sin()) };
    if t.is_empty() {
        return (a.cos(), a.sin());
    }
    t[i % t.len()].point(a / TAU)
}

/// Longueur parcourue sur le trajet par radian d'angle (le rayon d'un cercle de même tour), pour
/// découper chaque image en assez de pas : 1,2 pour les formes calculées, plus pour un contour
/// long et tourmenté.
fn longueur_par_radian(forme: Forme, i: usize) -> f32 {
    let Some(Ok(t)) = contours_de(forme).map(|c| c.read()) else { return 1.2 };
    t.get(i % t.len().max(1)).map_or(1.2, |c| (c.total / TAU).max(1.2))
}

/// Teinte de base imposée en ce moment (application, Prisme), en degrés.
pub fn sync_hue() -> Option<f32> {
    Some(f32::from_bits(SYNC_HUE.load(std::sync::atomic::Ordering::Relaxed))).filter(|h| h.is_finite())
}

/// Impose (ou retire) la teinte de base, en degrés.
pub fn set_sync_hue(h: Option<f32>) {
    SYNC_HUE.store(h.unwrap_or(f32::NAN).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Teinte propre de l'essaim à l'instant `t` (sans teinte imposée), en degrés : `teinte` qui
/// tourne avec le temps (fixe à vitesse nulle). Calculée et repliée en f64 (en f32, après des
/// jours, elle irait par crans).
pub fn teinte_propre(cfg: &Config, t: f64) -> f32 {
    (cfg.couleur.teinte as f64 + t * cfg.couleur.vitesse_teinte as f64).rem_euclid(360.0) as f32
}

/// Teinte « Prisme » : `k` = 0 en queue de traînée, 1 en tête. Base : celle de l'éclairage si
/// la synchro en fournit une, sinon `teinte` qui tourne avec le temps (fixe à vitesse nulle).
pub fn prisme(cfg: &Config, t: f64, k: f32) -> (u8, u8, u8) {
    prisme_decale(cfg, t, k, 0.0)
}

/// Teinte `j` de la palette (`[couleur] palette`, 1 à 3 teintes) : celle de `prisme`, décalée de
/// `j` fois `ecart_palette` degrés (les comètes et satellites se partagent la palette).
/// `decale` : degrés ajoutés (modulation de la teinte).
fn prisme_palette(cfg: &Config, t: f64, k: f32, j: usize, decale: f32) -> (u8, u8, u8) {
    let n = cfg.couleur.palette.clamp(1, 3) as usize;
    prisme_decale(cfg, t, k, (j % n) as f32 * cfg.couleur.ecart_palette + decale)
}

/// Couleur de l'essaim (comme `prisme`), décalée de `decale` degrés.
pub(crate) fn prisme_decale(cfg: &Config, t: f64, k: f32, decale: f32) -> (u8, u8, u8) {
    let base = sync_hue().unwrap_or_else(|| teinte_propre(cfg, t));
    let base = base + decalage();
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
///
/// Au-delà de `VIE_FINE` (traînée allongée par une modulation), les points sont espacés d'autant
/// (en temps et en distance) : une traînée de 12 s ne compte pas plus de points qu'une de 3 s.
/// `max` (sa part de `POINTS_TRAINEES`, `TRAIL_POINTS_MAX` au plus) borne le reste (pointeur qui
/// file très vite) : les points les plus vieux s'en vont.
fn push_trail(tr: &mut Trail, t: f64, x: f32, y: f32, life: f64, gap: f32, max: usize) {
    let n = tr.len();
    let k = (life / VIE_FINE).max(1.0);
    let (pas, gap) = (TRAIL_STEP * k, gap * k as f32);
    let proche = |p: &(f64, f32, f32)| t - p.0 < pas && (p.1 - x).powi(2) + (p.2 - y).powi(2) < gap * gap;
    if n >= 2 && proche(&tr[n - 2]) {
        tr[n - 1] = (t, x, y);
    } else {
        tr.push_back((t, x, y));
    }
    while tr.front().is_some_and(|p| t - p.0 > life) {
        tr.pop_front();
    }
    let max = max.max(2);
    if tr.len() > max {
        let trop = tr.len() - max;
        tr.drain(..trop);
    }
}

/// Vie de traînée jusqu'à laquelle les points gardent leur finesse (la plus longue du réglage
/// `[orbite] trainee`), et points d'une traînée au plus.
const VIE_FINE: f64 = 3.0;
const TRAIL_POINTS_MAX: usize = 4000;
/// Points de toutes les traînées réunies au plus, partagés également entre elles (jusqu'à 32
/// comètes, 64 satellites et 32 permanents : 468 chacune au pire, au-delà d'une traînée
/// ordinaire : un point tous les 1/90 s, ~25 pour un satellite, 27 à 270 pour une comète de 0,3 à
/// 3 s, plus quand le pointeur file).
const POINTS_TRAINEES: usize = 60_000;
/// Tours de son trajet qu'une traînée couvre au plus : au-delà, elle repasse sur elle-même
/// (traînée de 15 s à grande vitesse) sans rien montrer de plus, et coûte autant de points. Pas un
/// seul : des presets fournis (Intense, Fantôme, Typhon…) font 1,7 tour sur le son d'essai, et
/// près de 4 basse à fond, où le bout de la traînée, pâle, repasse sur la tête ; cinq n'en
/// touchent aucun.
const TOURS_TRAINEE: f64 = 5.0;

/// Part de chaque traînée dans `POINTS_TRAINEES` (`TRAIL_POINTS_MAX` au plus), à parts égales
/// entre toutes celles que le réglage permet : `cometes`, `satellites` (le maximum, pas ceux qui
/// vivent en ce moment) et `fixes`.
fn part_trainee(cometes: usize, satellites: usize, fixes: usize) -> usize {
    (POINTS_TRAINEES / (cometes + satellites + fixes).max(1)).min(TRAIL_POINTS_MAX)
}

/// Vie utile d'une traînée de vie `vie` (s) qui tourne à `w` radians/s (lissée) : au plus
/// `TOURS_TRAINEE` tours de son trajet.
fn vie_utile(vie: f64, w: f32) -> f64 {
    if w.is_finite() && w > 1e-3 { vie.min(TOURS_TRAINEE * std::f64::consts::TAU / w as f64) } else { vie }
}

/// Facteur du rayon et décalage (en rayons) d'un groupe, ramenés ensemble (la forme garde ses
/// proportions) pour que son point le plus loin du centre — `portee` (en rayons : étendue de la
/// forme × anneau extérieur ou satellite le plus écarté) × `rayon` (px) × facteur, plus le
/// décalage — reste à moins de `dispo` px.
fn borne_etendue(facteur: f32, decalage: f32, rayon: f32, portee: f32, dispo: f32) -> (f32, f32) {
    let total = decalage.abs() * rayon + portee * rayon * facteur;
    if total.is_finite() && total > dispo && total > 0.0 {
        let k = dispo / total;
        (facteur * k, decalage * k)
    } else {
        (facteur, decalage)
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
/// la moitié de sa force (aussi pour les éclats de l'habillage : `loin.rs`).
pub const ECART_FLASH: f64 = 1.0 / 3.0;
/// Halo blanc de l'éclat du kick : rayon du vide laissé sous la pointe (px à 100 %).
const HALO_VIDE: f32 = 6.0;

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
    /// Mode ressorts : sa distance (relative) au bout de son ressort et sa vitesse, la pulsation de
    /// ce ressort. Né au centre, il rejoint `r` avec de l'élan.
    rr: f32,
    rv: f32,
    w: f32,
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
        Forme::Trace | Forme::Icone => sur_contour(forme, 0, a),
    }
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

/// Événements du son depuis la lecture précédente : kicks, caisses claires, drops, attaques
/// des aigus (au plus 4 chacun), et, quand le tempo est régulier, passage d'un temps (pendant un
/// break ou non) et d'un contretemps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Coups {
    pub kicks: u32,
    pub caisses: u32,
    pub drops: u32,
    pub aigus: u32,
    pub temps: bool,
    pub pause: bool,
    pub contretemps: bool,
}

impl Coups {
    /// Les événements, dans l'ordre où leurs déclencheurs agissent (un par kick, un par attaque
    /// des aigus).
    pub fn evenements(&self) -> impl Iterator<Item = Evenement> + '_ {
        let n = |e: Evenement, k: u32| std::iter::repeat_n(e, k as usize);
        let si = |e: Evenement, oui: bool| std::iter::repeat_n(e, oui as usize);
        n(Evenement::Kick, self.kicks)
            .chain(si(Evenement::Temps, self.temps))
            .chain(si(Evenement::Break, self.temps && self.pause))
            .chain(si(Evenement::Caisse, self.caisses > 0))
            .chain(si(Evenement::Drop, self.drops > 0))
            .chain(si(Evenement::Contretemps, self.contretemps))
            .chain(n(Evenement::Charleston, self.aigus))
    }
}

/// Écoute des compteurs de l'analyse du son (`AudioView`) : ce qui est arrivé depuis la lecture
/// précédente. L'essaim et la bordure ont chacun la leur.
#[derive(Clone, Debug, Default)]
pub struct Ecoute {
    kicks: Option<u32>,
    caisses: Option<u32>,
    drops: Option<u32>,
    aigus: Option<u32>,
    phase: Option<f32>,
}

impl Ecoute {
    pub fn lire(&mut self, au: &AudioView) -> Coups {
        let lu = |vu: &mut Option<u32>, n: u32| {
            let d = nouveaux(*vu, n);
            *vu = Some(n);
            d
        };
        let (kicks, caisses, drops, aigus) = (lu(&mut self.kicks, au.kicks), lu(&mut self.caisses, au.caisses), lu(&mut self.drops, au.drops), lu(&mut self.aigus, au.aigus));
        let phase = au.bpm.map(|_| au.phase);
        let (temps, contretemps) = match (phase, self.phase) {
            (Some(p), Some(avant)) => (p < avant - 0.5, avant < 0.5 && p >= 0.5 && p - avant < 0.4),
            _ => (false, false),
        };
        self.phase = phase;
        Coups { kicks, caisses, drops, aigus, temps, pause: au.relais, contretemps }
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
    /// Sens de rotation, et instant de sa dernière inversion (une par image au plus).
    dir: f32,
    inverse_a: f64,
    /// Vitesse du trajet des comètes et de celui des satellites (radians/s), lissée vers la
    /// vitesse de base accélérée (`acceleration` des modulations).
    spd: [f32; 2],
    /// Angle de chaque anneau.
    angles: Vec<f32>,
    /// Rotation du trajet lui-même : celui des comètes, celui des satellites.
    rot: f32,
    rot_sats: f32,
    /// Sources des modulations (enveloppes des coups, montée suivie), valeur lissée de chaque
    /// modulation, effets sur les comètes (`[0]`) et les satellites (`[1]`) à cette image et à la
    /// précédente, teinte ajoutée à tout ce qu'Essaim dessine.
    sources: Sources,
    valeurs: Vec<f32>,
    mods: [Effet; 2],
    mods_avant: [Effet; 2],
    /// Effets des modulations sur chaque figure (`figures.rs` s'en sert).
    mods_figures: [Effet; modulation::NB_FIGURES],
    teinte: f32,
    /// Dernier déclenchement de chaque déclencheur (délai minimal), et la liste des déclencheurs
    /// (événement, action) à qui ils appartiennent : une liste changée les remet à zéro.
    derniers: Vec<f64>,
    cles_declencheurs: Vec<(Evenement, Action)>,
    /// Part fractionnaire des grains du sillage, reportée à l'image suivante.
    reste_sillage: f32,
    /// Fissures demandées par les déclencheurs (x, y, force), que la boucle principale ouvre.
    pub fissures: Vec<(f32, f32, f32)>,
    /// Traînées des comètes, anneau par anneau.
    trails: Vec<Trail>,
    sats: Vec<Sat>,
    /// Satellites permanents (`[essaim] fixes`).
    fixes: Vec<Sat>,
    /// Événements du son (kicks, caisses claires, temps…).
    ecoute: Ecoute,
    /// Vitesse du centre de l'orbite (px/s), pour l'inertie.
    vitesse_centre: (f32, f32),
    /// Ressort du rayon de chaque groupe : écart au rayon visé et sa vitesse (rayons, rayons/s) ;
    /// rayon visé (modulations) et facteur du rayon à la fin de l'image précédente.
    rebond: [(f32, f32); 2],
    cibles: [f32; 2],
    facteurs: [f32; 2],
    /// Poussée de la rotation (radians/s) et coup de frein (0 à 1), qui retombent.
    pousse: f32,
    frein: f32,
    /// Vrille des rubans : saut visé (un cran par caisse claire) et saut lissé (radians).
    vrille: (f32, f32),
    /// Teinte de la palette du prochain satellite.
    prochaine_couleur: usize,
    /// Bords de la fenêtre au premier plan (gauche, haut, droite, bas) : les particules y
    /// rebondissent (`[particules] rebonds`).
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
    /// Vitesse angulaire des comètes et des satellites (radians/s, sans le sens), lissée (~0,5 s) :
    /// la vie utile des traînées en dépend (`vie_utile`) sans sauter à chaque kick.
    w_lisse: [f32; 2],
    /// Vie utile des traînées des comètes, anneau par anneau (s).
    vies: Vec<f64>,
    /// Forme du trajet des comètes et de celui des satellites à l'image d'avant, avec la version
    /// de leurs contours (image, icône de l'application : `version_contours`) : changée, les
    /// traînées du groupe repartent de zéro (elles suivraient l'ancien trajet jusqu'à leur fin).
    /// Un rayon, une quantité, une couleur changés n'en font pas partie : le trajet glisse.
    formes: Option<[(Forme, u32); 2]>,
    /// Modulations (source, cible, groupe), dans l'ordre : la liste changée (ajout, retrait,
    /// déplacement), chaque valeur lissée appartiendrait à une autre ; elles repartent de zéro.
    cles_modulations: Vec<(crate::config::Source, crate::config::Cible, crate::config::Groupe)>,
    /// Tampons du dessin, gardés d'une image à l'autre (le dessin ne prend que `&self`).
    tampons: std::cell::RefCell<Tampons>,
}

/// Trajet d'un groupe pendant une image : rotation au début et son avance, rayon et décalage
/// (px) au début et à la fin.
struct Pose {
    rot0: f32,
    drot: f32,
    rayon0: f32,
    rayon: f32,
    dx0: f32,
    dx: f32,
}

/// Tête d'une traînée à dessiner.
struct Head {
    x: f32,
    y: f32,
    s: f32,
    al: f32,
    /// Lueur (modulation de la luminosité).
    lueur: f32,
    front: bool,
    rgb: (u8, u8, u8),
    /// Mode ressorts : de la tête au bout de son étirement, à l'opposé de sa vitesse (px).
    queue: (f32, f32),
    /// Tête non dessinée (ruban) : elle compte encore pour la constellation.
    cachee: bool,
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
    /// Essaim au hasard fixé (essais : même suite de tirages à chaque fois).
    #[cfg(test)]
    pub fn avec_graine(g: u64) -> Self {
        Self { rng: Rng::graine(g), ..Self::new() }
    }

    pub fn new() -> Self {
        Self {
            rng: Rng::seeded(),
            dir: 1.0,
            inverse_a: f64::NEG_INFINITY,
            spd: [3.5; 2],
            angles: Vec::new(),
            rot: 0.0,
            rot_sats: 0.0,
            sources: Sources::default(),
            valeurs: Vec::new(),
            mods: [Effet::default(); 2],
            mods_avant: [Effet::default(); 2],
            mods_figures: [Effet::default(); modulation::NB_FIGURES],
            teinte: 0.0,
            derniers: Vec::new(),
            cles_declencheurs: Vec::new(),
            reste_sillage: 0.0,
            fissures: Vec::new(),
            trails: Vec::new(),
            sats: Vec::new(),
            fixes: Vec::new(),
            ecoute: Ecoute::default(),
            vitesse_centre: (0.0, 0.0),
            rebond: [(0.0, 0.0); 2],
            cibles: [1.0; 2],
            facteurs: [1.0; 2],
            pousse: 0.0,
            frein: 0.0,
            vrille: (0.0, 0.0),
            prochaine_couleur: 0,
            murs: None,
            doux: false,
            dernier_flash: f64::NEG_INFINITY,
            particules: Vec::new(),
            dernier_curseur: None,
            center: None,
            pitch: 50.0,
            env: 0.0,
            tetes: Vec::new(),
            w_lisse: [0.0; 2],
            vies: Vec::new(),
            formes: None,
            cles_modulations: Vec::new(),
            tampons: Default::default(),
        }
    }

    /// Rien n'est simulé à cette image (frappe, pause, plein écran) : les coups joués entre-temps
    /// sont lus et jetés, sinon ils partiraient tous à la reprise (jusqu'à 4 kicks et un drop
    /// d'un coup) ; le pointeur sera repris où il sera, sans glisser depuis l'ancienne position.
    pub fn oublier_coups(&mut self, au: &AudioView) {
        self.ecoute.lire(au);
        self.dernier_curseur = None;
    }

    /// Preset appliqué : toutes les traînées repartent de zéro. Sans cela, celles qui suivaient
    /// l'ancien trajet (autre rayon, autre décalage, autres anneaux) seraient reliées au nouveau
    /// par un trait. Une glissière qu'on déplace n'y passe pas : le trajet y glisse.
    pub fn vider_trainees(&mut self) {
        self.trails.iter_mut().for_each(Trail::clear);
        self.sats.iter_mut().chain(self.fixes.iter_mut()).for_each(|s| s.trail.clear());
    }

    /// Effets des modulations sur chaque figure, dans l'ordre de `config::FIGURES`.
    pub fn effets_figures(&self) -> &[Effet; modulation::NB_FIGURES] {
        &self.mods_figures
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
        // halo des comètes (le plus large, grossi par une modulation de la taille) + marge. Pas de
        // rognage : tout ce qui est dessiné doit être dans ce rectangle, sinon ce qui en sort
        // n'est jamais effacé (les points trop loin du pointeur sont retirés des traînées par
        // `update`).
        let taille = self.mods.iter().map(|m| m.taille).fold(1.0f32, f32::max);
        let pad = ((2.4 * 2.6 + 3.5 * 2.6) * taille + MARGIN) * scale;
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
        // centre de l'orbite : suit le pointeur, de près ou en ressort souple (`[orbite] inertie`),
        // retard borné ; le décalage (stéréo…) passe par les modulations
        let cible = cursor;
        let (mut cx, mut cy) = self.center.unwrap_or(cible);
        let c0 = (cx, cy);
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

        // événements du son : kicks, caisses claires, drops, attaques des aigus, temps et
        // contretemps (tempo connu) ; chacun fait agir ses déclencheurs (`[declencheurs]`)
        let Coups { kicks, caisses, drops, aigus, temps, pause, contretemps: contre } = self.ecoute.lire(&au);
        for _ in 0..kicks {
            self.declencher(Evenement::Kick, cursor, t, cfg, scale);
        }
        self.env *= (-dt * 9.0).exp();
        if temps {
            self.declencher(Evenement::Temps, cursor, t, cfg, scale);
            if pause {
                self.declencher(Evenement::Break, cursor, t, cfg, scale);
            }
        }
        if caisses > 0 {
            self.declencher(Evenement::Caisse, cursor, t, cfg, scale);
        }
        if drops > 0 {
            self.declencher(Evenement::Drop, cursor, t, cfg, scale);
        }
        if contre {
            self.declencher(Evenement::Contretemps, cursor, t, cfg, scale);
        }
        for _ in 0..aigus {
            self.declencher(Evenement::Charleston, cursor, t, cfg, scale);
        }
        // sillage : poussière le long du trajet de la souris quand elle file vite
        // (le nombre de grains de l'image garde sa part fractionnaire pour l'image suivante : à
        // haute cadence, chaque image n'en voit qu'une fraction, qui doit s'accumuler)
        let mut vite = false;
        if let (true, Some(avant)) = (cfg.sillage.actif, self.dernier_curseur) {
            let (dx, dy) = (cursor.0 - avant.0, cursor.1 - avant.1);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist / dt.max(1e-3) > 700.0 * scale && dist < 600.0 * scale {
                vite = true;
                self.reste_sillage += cfg.sillage.densite.clamp(1.0, 10.0) * dist / (60.0 * scale);
                let n = (self.reste_sillage.floor().max(0.0) as usize).min(14);
                // au plus 14 grains par image : au-delà, le surplus est perdu (comme avant)
                self.reste_sillage = (self.reste_sillage - n as f32).min(1.0);
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
        if !vite || !self.reste_sillage.is_finite() {
            self.reste_sillage = 0.0;
        }
        self.dernier_curseur = Some(cursor);
        // particules : avancent en ralentissant (en rebondissant sur les bords de la fenêtre au
        // premier plan), puis s'éteignent
        let frein = (-dt * 3.0).exp();
        let murs = self.murs.filter(|_| cfg.particules.rebonds);
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

        // note de la basse (lissée ~40 ms) ; vitesse du trajet. Ce que la basse, la note grave, la
        // montée… y changent passe par les modulations, juste après.
        if au.pitch > 0.0 {
            self.pitch += (au.pitch - self.pitch) * (dt * 25.0).min(1.0);
        }
        let depth = ((80.0 - self.pitch) / 50.0).clamp(0.0, 1.0);
        let presence = (au.bass * 2.5).clamp(0.0, 1.0);
        let o = &cfg.orbite;
        // une valeur non finie (son abîmé) figerait l'essaim pour toujours : retour au repos
        if !self.pitch.is_finite() {
            self.pitch = 50.0;
        }
        // forme d'un trajet changée (autre forme, autre icône au premier plan, autre image) : les
        // traînées de ce groupe suivraient l'ancien trajet jusqu'à leur fin ; elles repartent de
        // zéro. Un rayon qu'on glisse, une modulation ajoutée (teinte, opacité…) les laissent : le
        // trajet glisse. Un preset appliqué les vide aussi (`vider_trainees`, appelé par la boucle).
        let formes = [o.forme, cfg.essaim.forme.suivie(o.forme)].map(|f| (f, version_contours(f)));
        let avant = self.formes.replace(formes);
        if avant.map(|f| f[0]) != Some(formes[0]) {
            self.trails.iter_mut().for_each(Trail::clear);
        }
        if avant.map(|f| f[1]) != Some(formes[1]) {
            self.sats.iter_mut().chain(self.fixes.iter_mut()).for_each(|s| s.trail.clear());
        }
        // modulation ajoutée, retirée ou déplacée : chaque valeur lissée appartiendrait à une
        // autre ; elles repartent de zéro (comparées sur place, sans rien allouer)
        let liste = &cfg.modulations.liste;
        if !self.cles_modulations.iter().copied().eq(liste.iter().map(|m| (m.source, m.cible, m.sur))) {
            self.cles_modulations.clear();
            self.cles_modulations.extend(liste.iter().map(|m| (m.source, m.cible, m.sur)));
            self.valeurs.clear();
        }
        // modulations : valeur visée de chacune (source, seuil, à l'envers), lissée à son rythme
        self.sources.avancer(dt, [kicks, caisses, aigus, drops, contre as u32], au.tension);
        self.valeurs.resize(liste.len(), 0.0);
        for (m, v) in liste.iter().zip(self.valeurs.iter_mut()) {
            let vise = modulation::visee(m, self.sources.valeur(m.source, &au, depth, presence));
            *v += (vise - *v) * modulation::part_lissage(m.lissage, dt);
            if !v.is_finite() {
                *v = 0.0;
            }
        }
        let effets = modulation::effets(liste, &self.valeurs);
        self.mods_avant = self.mods;
        (self.mods, self.teinte, self.mods_figures) = (effets.groupes, effets.teinte, effets.figures);
        // vitesse du trajet de chaque groupe, accélérée (montée…), lissée (~100 ms)
        for (v, m) in self.spd.iter_mut().zip(&self.mods) {
            *v += (o.vitesse_base * m.acceleration - *v) * (dt * 10.0).min(1.0);
            if !v.is_finite() {
                *v = o.vitesse_base;
            }
        }
        // ressorts : la poussée et le frein retombent ; le rayon de chaque groupe est son rayon
        // modulé plus l'écart d'un ressort, que les rebonds lancent. En mode ressorts, tout le
        // rayon passe par ce ressort : il suit sa cible (modulations, tremblement de la montée)
        // avec de l'élan. Sans rebond ni mode ressorts, l'écart reste nul.
        let re = &cfg.ressorts;
        self.pousse *= (-dt * POUSSEE.1).exp();
        self.frein *= (-dt * FREIN.1).exp();
        let (raideur, amorti) = (re.raideur.clamp(6.0, 30.0), re.amorti.clamp(0.1, 1.0));
        let facteurs_avant = self.facteurs;
        for g in 0..2 {
            let (mut ecart, mut v) = self.rebond[g];
            let mut cible = self.mods[g].rayon;
            if re.actif {
                cible += TREMBLE.0 * au.tension.clamp(0.0, 1.0) * (t * TREMBLE.1).sin() as f32;
                ecart += self.cibles[g] - cible;
            }
            ressort(&mut ecart, &mut v, raideur, amorti, dt);
            self.rebond[g] = (ecart, v);
            self.cibles[g] = cible;
            self.facteurs[g] = (cible + ecart).max(0.05);
        }
        // plusieurs contours (une image, une icône, un texte) : un anneau par contour, à sa place
        let contours_cometes = nombre_contours(o.forme);
        let anneaux = if contours_cometes > 1 { contours_cometes } else { o.anneaux.max(1) as usize };
        // étendue bornée : le point le plus loin de chaque groupe (anneau extérieur, satellite le
        // plus écarté, avec son dépassement en mode ressorts), décalage et retard du centre
        // compris, reste à moins de `MAX_EXTENT / 2` du pointeur, au-delà de quoi les points sont
        // retirés (un grand rayon modulé sur un grand écran les y jetait tous : l'essaim
        // disparaissait). Le rayon et le décalage sont ramenés ensemble.
        let rayon = o.rayon * scale;
        let portees = [
            etendue_forme(o.forme) * if contours_cometes > 1 { 1.0 } else { anneau(anneaux - 1).0 },
            etendue_forme(cfg.essaim.forme.suivie(o.forme)) * SAT_R_MAX * if re.actif { SAT_DEPASSEMENT } else { 1.0 },
        ];
        let dispo = (MAX_EXTENT / 2.0 - MARGIN * scale - lim).max(0.0);
        for (g, &portee) in portees.iter().enumerate() {
            (self.facteurs[g], self.mods[g].decalage) = borne_etendue(self.facteurs[g], self.mods[g].decalage, rayon, portee, dispo);
        }
        // vrille des rubans : un cran de plus à chaque caisse claire, rejoint en douceur
        if caisses > 0 && cfg.rubans.actif {
            self.vrille.0 += VRILLE.2;
        }
        self.vrille.1 += (self.vrille.0 - self.vrille.1) * (1.0 - (-dt * 12.0).exp());
        if self.vrille.1 > TAU {
            self.vrille = (self.vrille.0 - TAU, self.vrille.1 - TAU);
        }

        // comètes : `nombre` par anneau, réparties sur le trajet
        let e = &cfg.essaim;
        let nombre = o.nombre.max(1) as usize;
        self.angles.resize(anneaux, 0.0);
        // anneaux ou comètes par anneau changés : chaque traînée appartiendrait à une autre
        // comète (et sauterait d'une place à l'autre) ; elles repartent de zéro
        if self.trails.len() != anneaux * nombre {
            self.trails.clear();
            self.trails.resize_with(anneaux * nombre, Trail::new);
        }
        let life = o.trainee.max(0.01) as f64 * self.mods[0].trainee as f64;
        // une pose par groupe : rotation du trajet (`rotation_sur` : comètes, satellites ou tout,
        // plus les modulations), rayon et décalage au début et à la fin de l'image ; les points
        // intermédiaires suivent la vraie forme, coins compris
        let pose = |g: usize, rot: &mut f32, mods: &[Effet; 2], avant: &[Effet; 2]| {
            let base = if o.rotation_sur.inclut(g == 1) { o.rotation } else { 0.0 };
            let rot0 = if rot.is_finite() { *rot } else { 0.0 };
            let drot = pas_rotation(rot0, modulation::borne_rotation(base + mods[g].rotation), dt);
            let drot = if drot.is_finite() { drot } else { 0.0 };
            *rot = (rot0 + drot).rem_euclid(TAU);
            let (dx0, dx) = (avant[g].decalage * rayon, mods[g].decalage * rayon);
            Pose { rot0, drot, rayon0: rayon * facteurs_avant[g], rayon: rayon * self.facteurs[g], dx0, dx }
        };
        let poses = [pose(0, &mut self.rot, &self.mods, &self.mods_avant), pose(1, &mut self.rot_sats, &self.mods, &self.mods_avant)];
        let point = |p: &Pose, forme, contour: usize, a: f32, rel: f32, f: f32| {
            let (x, y) = if contour == 0 { chemin(forme, a) } else { sur_contour(forme, contour, a) };
            let (sin, cos) = (p.rot0 + p.drot * f).sin_cos();
            let (x, y) = (x * cos - y * sin, x * sin + y * cos);
            let rr = (p.rayon0 + (p.rayon - p.rayon0) * f) * rel;
            let dx = p.dx0 + (p.dx - p.dx0) * f;
            (c0.0 + (cx - c0.0) * f + dx + x * rr, c0.1 + (cy - c0.1) * f + y * rr)
        };
        let dur = e.duree.max(0.1) as f64;
        self.sats.retain(|s| t - s.born < dur);
        // satellites permanents : répartis une fois pour toutes (nombre d'or), toujours les mêmes
        if self.fixes.len() != e.fixes as usize {
            self.fixes = (0..e.fixes)
                .map(|j| {
                    let u = (j as f32 * 0.618_034).fract();
                    let r = 0.6 + u * (SAT_R_MAX - 0.6);
                    Sat { a: j as f32 * 2.399_963, r, mul: 0.6 + 0.8 * (1.0 - u), born: f64::INFINITY, trail: Trail::new(), couleur: j as usize, rr: r, rv: 0.0, w: SAT_RESSORT.0 }
                })
                .collect();
        }
        // part de chaque traînée dans le budget de points commun, comptée sur les nombres maximaux
        // du réglage (stables) : une salve de satellites ne raccourcit pas les comètes
        let part = part_trainee(anneaux * nombre, e.maximum.max(self.sats.len()), self.fixes.len());
        // vitesse : celle du trajet plus celle des modulations (accélérée aussi), jamais négative ;
        // lissée, elle borne la vie utile des traînées (`TOURS_TRAINEE` tours de trajet)
        let vitesse = |g: usize| self.dir * self.vitesse(g);
        let (w_cometes, w_sats) = (vitesse(0), vitesse(1));
        for (w, v) in self.w_lisse.iter_mut().zip([w_cometes.abs(), w_sats.abs()]) {
            *w += (v - *w) * (1.0 - (-dt * 2.0).exp());
            if !w.is_finite() {
                *w = v;
            }
        }
        self.vies.resize(anneaux, life);
        for (k, angle) in self.angles.iter_mut().enumerate() {
            let (rk, wk) = if contours_cometes > 1 { (1.0, 1.0) } else { anneau(k) };
            let contour = k % contours_cometes;
            let (a0, da) = (*angle, w_cometes * wk * dt);
            *angle = (a0 + da).rem_euclid(TAU);
            let vie = vie_utile(life, self.w_lisse[0] * wk);
            self.vies[k] = vie;
            let n = sous_pas(da, poses[0].rayon * rk * longueur_par_radian(o.forme, contour), FINESSE_COMETES);
            for i in 0..nombre {
                let decale = i as f32 * TAU / nombre as f32;
                for j in 1..=n {
                    let f = j as f32 / n as f32;
                    let (x, y) = point(&poses[0], o.forme, contour, a0 + da * f + decale, rk, f);
                    push_trail(&mut self.trails[k * nombre + i], t - dt as f64 * (1.0 - f) as f64, x, y, vie, FINESSE_COMETES.1, part);
                }
            }
        }
        // satellites : sur le trajet des comètes, ou le leur, à des distances variées
        let forme = e.forme.suivie(o.forme);
        let vie_sats = SAT_TRAIL * self.mods[1].trainee as f64 * if re.actif { SAT_TRAIL_RESSORTS } else { 1.0 };
        // plusieurs contours : chaque satellite suit l'un d'eux, de près
        let contours_sats = nombre_contours(forme);
        for s in self.sats.iter_mut().chain(self.fixes.iter_mut()) {
            // mode ressorts : le satellite est au bout de son propre ressort, un peu en retard
            if re.actif {
                let mut ecart = s.rr - s.r;
                ressort(&mut ecart, &mut s.rv, s.w, SAT_RESSORT.2, dt);
                // dépassement borné (il ne va pas au-delà de ~37 %) : l'étendue reste celle prévue
                s.rr = (s.r + ecart).clamp(0.0, SAT_R_MAX * SAT_DEPASSEMENT);
            } else {
                (s.rr, s.rv) = (s.r, 0.0);
            }
            let (contour, rel) = if contours_sats > 1 { (s.couleur % contours_sats, 1.0 + (s.r - 1.3) * 0.25) } else { (0, s.rr) };
            let (a0, da) = (s.a, w_sats * s.mul * dt);
            s.a = (a0 + da).rem_euclid(TAU);
            let vie = vie_utile(vie_sats, self.w_lisse[1] * s.mul);
            let n = sous_pas(da, poses[1].rayon * rel * longueur_par_radian(forme, contour), FINESSE_SATELLITES);
            for j in 1..=n {
                let f = j as f32 / n as f32;
                let (x, y) = point(&poses[1], forme, contour, a0 + da * f, rel, f);
                push_trail(&mut s.trail, t - dt as f64 * (1.0 - f) as f64, x, y, vie, FINESSE_SATELLITES.1, part);
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

    /// Un événement de la souris ou du morceau (clic, clic droit, changement de piste) en
    /// (x, y) : ses déclencheurs agissent tout de suite.
    pub fn evenement(&mut self, ev: Evenement, x: f32, y: f32, t: f64, cfg: &Config, scale: f32) {
        self.declencher(ev, (x, y), t, cfg, scale);
    }

    /// Fait agir les déclencheurs de `ev`, dans l'ordre de la liste, au point `p` : chacun après
    /// son délai minimal, avec sa chance.
    fn declencher(&mut self, ev: Evenement, p: (f32, f32), t: f64, cfg: &Config, scale: f32) {
        let liste = &cfg.declencheurs.liste;
        // délais rattachés à chaque déclencheur, pas à sa place : une liste changée (ajout,
        // retrait, réordonnée, autre preset) les remet tous à zéro
        if !self.cles_declencheurs.iter().copied().eq(liste.iter().map(|d| (d.quand, d.faire))) {
            self.cles_declencheurs.clear();
            self.cles_declencheurs.extend(liste.iter().map(|d| (d.quand, d.faire)));
            self.derniers.clear();
            self.derniers.resize(liste.len(), f64::NEG_INFINITY);
        }
        for (i, d) in liste.iter().enumerate() {
            if d.quand != ev || (d.delai > 0.0 && t - self.derniers[i] < d.delai as f64) {
                continue;
            }
            // tirage seulement s'il y a une chance de ne rien faire (la suite reste la même sinon)
            if d.probabilite < 1.0 && self.rng.f() >= d.probabilite {
                continue;
            }
            self.derniers[i] = t;
            self.agir(d, p, t, cfg, scale);
        }
    }

    /// Une action d'un déclencheur au point (x, y). La bordure suit ses déclencheurs elle-même
    /// (`bordure.rs`) ; une fissure est demandée à la boucle principale (`fissures`).
    fn agir(&mut self, d: &Declencheur, (x, y): (f32, f32), t: f64, cfg: &Config, scale: f32) {
        let force = d.force.clamp(0.0, 3.0);
        let taille = cfg.particules.taille.clamp(0.3, 3.0) * force;
        let (lo, hi) = (d.nombre.min(d.nombre_max), d.nombre.max(d.nombre_max));
        // nombre tiré entre les deux bornes (aucun tirage si elles sont égales)
        let nombre = |rng: &mut Rng| if lo == hi { lo } else { rng.range(lo as f32, hi as f32 + 0.999) as u32 };
        match d.faire {
            Action::Eclat => self.eclater(t, force.min(1.0)),
            Action::Satellites => {
                // tirage toujours fait : la suite du kick reste celle d'avant les déclencheurs
                let n = self.rng.range(lo as f32, hi as f32 + 0.999) as usize;
                self.ajouter_satellites(n, t, cfg);
            }
            // une seule inversion par image : deux (deux kicks, kick et caisse claire…)
            // s'annuleraient
            Action::Inversion => {
                if self.inverse_a != t {
                    self.dir = -self.dir;
                    self.inverse_a = t;
                }
            }
            Action::Rebond => {
                // élan borné à celui d'un rebond de force 3 : plusieurs rebonds dans la même
                // image (kicks en rafale, kick et caisse claire) ne s'additionnent pas sans fin
                for r in &mut self.rebond {
                    r.1 = (r.1 + REBOND * force).clamp(-REBOND_MAX, REBOND_MAX);
                }
            }
            Action::Poussee => self.pousse += POUSSEE.0 * force,
            Action::Frein => self.frein = self.frein.max(force.min(1.0)),
            Action::Bordure | Action::Aucune => {}
            Action::Onde => {
                self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie: 0.55, taille: 46.0 * taille * scale, genre: Genre::Onde });
            }
            Action::Eclats => {
                for _ in 0..16 {
                    let (a, v) = (self.rng.range(0.0, TAU), self.rng.range(160.0, 380.0) * taille * scale);
                    let vie = self.rng.range(0.45, 0.8);
                    let s = self.rng.range(1.2, 2.4) * scale;
                    self.particules.push(Particule { x, y, vx: a.cos() * v, vy: a.sin() * v, ne: t, vie, taille: s, genre: Genre::Etincelle });
                }
            }
            Action::Fissure => {
                if self.fissures.len() < 8 {
                    self.fissures.push((x, y, force));
                }
            }
            Action::Implosion => {
                // un anneau qui se referme et des étincelles aspirées vers le point
                let r = 52.0 * taille * scale;
                self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie: 0.45, taille: r, genre: Genre::Implosion });
                for i in 0..12 {
                    let a = i as f32 * TAU / 12.0 + self.rng.range(-0.2, 0.2);
                    let (cos, sin) = (a.cos(), a.sin());
                    let v = self.rng.range(2.2, 3.0) * r;
                    let s = self.rng.range(1.0, 1.8) * scale;
                    self.particules.push(Particule { x: x + cos * r, y: y + sin * r, vx: -cos * v, vy: -sin * v, ne: t, vie: 0.4, taille: s, genre: Genre::Etincelle });
                }
            }
            Action::Carre => {
                let (c, vie) = CARRE_CAISSE;
                self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie, taille: c * force * scale, genre: Genre::Carre });
            }
            Action::Etincelles => {
                // près des têtes (ou sur l'orbite s'il n'y en a pas)
                let n = nombre(&mut self.rng);
                let (cx, cy) = self.center.unwrap_or((x, y));
                let mut tetes = std::mem::take(&mut self.tetes);
                tetes.clear();
                tetes.extend(self.trails.iter().chain(self.sats.iter().chain(&self.fixes).map(|s| &s.trail)).filter_map(|tr| tr.back().map(|p| (p.1, p.2))));
                for _ in 0..n.min(MAX_PARTICULES as u32) {
                    let (px, py) = if tetes.is_empty() {
                        let a = self.rng.range(0.0, TAU);
                        (cx + a.cos() * cfg.orbite.rayon * scale, cy + a.sin() * cfg.orbite.rayon * scale)
                    } else {
                        tetes[(self.rng.f() * tetes.len() as f32) as usize % tetes.len()]
                    };
                    let (a, v) = (self.rng.range(0.0, TAU), self.rng.range(30.0, 90.0) * scale * force.max(0.2));
                    let vie = self.rng.range(0.3, 0.55);
                    let s = self.rng.range(1.1, 2.1) * scale;
                    self.particules.push(Particule { x: px, y: py, vx: a.cos() * v, vy: a.sin() * v, ne: t, vie, taille: s, genre: Genre::Etincelle });
                }
                self.tetes = tetes;
            }
            Action::Explosion => {
                // l'éclat d'un kick, des satellites, une grande onde sous le pointeur et une
                // gerbe d'étincelles depuis le centre de l'orbite
                self.eclater(t, 1.0);
                let n = nombre(&mut self.rng) as usize;
                self.ajouter_satellites(n, t, cfg);
                let (o, vie) = ONDE_DROP;
                self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie, taille: o * force * scale, genre: Genre::Onde });
                let (cx, cy) = self.center.unwrap_or((x, y));
                for i in 0..ETINCELLES_DROP {
                    let a = i as f32 * TAU / ETINCELLES_DROP as f32 + self.rng.range(-0.1, 0.1);
                    let v = self.rng.range(220.0, 420.0) * scale;
                    let (vie, s) = (self.rng.range(0.5, 0.9), self.rng.range(1.2, 2.2) * scale);
                    self.particules.push(Particule { x: cx, y: cy, vx: a.cos() * v, vy: a.sin() * v, ne: t, vie, taille: s, genre: Genre::Etincelle });
                }
            }
            Action::Rafale => {
                // l'éclat d'un kick, des satellites et une grande onde
                self.eclater(t, 1.0);
                let n = nombre(&mut self.rng) as usize;
                self.ajouter_satellites(n, t, cfg);
                let (o, vie) = ONDE_PISTE;
                self.particules.push(Particule { x, y, vx: 0.0, vy: 0.0, ne: t, vie, taille: o * force * scale, genre: Genre::Onde });
            }
        }
    }

    /// Éclat (kick, rafale, drop) de force `force` (0 à 1) : plein, ou en mode sans flashs, à
    /// moitié et au plus trois fois par seconde.
    fn eclater(&mut self, t: f64, force: f32) {
        if force <= 0.0 {
            return;
        }
        if !self.doux {
            self.env = self.env.max(force);
        } else if t - self.dernier_flash >= ECART_FLASH {
            self.dernier_flash = t;
            self.env = self.env.max(0.5 * force);
        }
    }

    /// Ajoute `n` satellites nés à `t`, dans la limite de `[essaim] maximum`. En mode ressorts,
    /// chacun naît au centre, au bout d'un ressort à lui (tiré seulement dans ce mode : la suite
    /// des tirages des autres presets reste la même).
    fn ajouter_satellites(&mut self, n: usize, t: f64, cfg: &Config) {
        for _ in 0..n {
            if self.sats.len() >= cfg.essaim.maximum {
                break;
            }
            let (a, r, mul) = (self.rng.range(0.0, TAU), self.rng.range(0.75, SAT_R_MAX), self.rng.range(0.7, 1.3));
            let (rr, w) = if cfg.ressorts.actif { (0.0, self.rng.range(SAT_RESSORT.0, SAT_RESSORT.1)) } else { (r, SAT_RESSORT.0) };
            self.sats.push(Sat { a, r, mul, born: t, trail: Trail::new(), couleur: self.prochaine_couleur, rr, rv: 0.0, w });
            self.prochaine_couleur = self.prochaine_couleur.wrapping_add(1);
        }
    }

    /// Vitesse sur le trajet du groupe `g` (0 : comètes, 1 : satellites), en radians/s, sans le
    /// sens : celle du trajet plus celle des modulations, accélérée, et la poussée, jamais
    /// négative ni au-delà de `VITESSE_MAX` ; le coup de frein en retire une part.
    fn vitesse(&self, g: usize) -> f32 {
        let v = self.spd[g] + self.mods[g].vitesse * self.mods[g].acceleration + self.pousse;
        let v = if v.is_finite() { v.clamp(0.0, modulation::VITESSE_MAX) } else { 0.0 };
        v * (1.0 - FREIN.0 * self.frein)
    }

    /// Teinte ajoutée à tout ce qu'Essaim dessine (modulations de la teinte « sur tout »), en
    /// degrés : la boucle principale la passe à `set_decalage`.
    pub fn teinte_globale(&self) -> f32 {
        self.teinte
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

        // satellites (traînées sans halo, pour rester léger) ; les permanents restent à 0,8
        // modulations : taille (têtes et traînées), teinte, lueur, traînée et opacité de chaque groupe
        let [mc, ms] = self.mods;
        let re = &cfg.ressorts;
        // mode ressorts : têtes étirées dans leur élan, satellites qui entrent en dépassant leur
        // taille et sortent en fondant, traînées de satellites plus courtes
        let queue = |tr: &Trail| if re.actif { etirement(tr, scale, re.etirement) } else { (0.0, 0.0) };
        let vie_sats = SAT_TRAIL * ms.trainee as f64 * if re.actif { SAT_TRAIL_RESSORTS } else { 1.0 };
        for s in self.sats.iter().chain(&self.fixes) {
            let k = match (s.born.is_finite(), re.actif) {
                (false, _) => 0.8,
                (true, false) => (1.0 - (t - s.born) / dur) as f32,
                (true, true) => {
                    let (age, reste) = ((t - s.born) as f32, (dur - (t - s.born)) as f32);
                    let entree = if age < SAT_ENTREE_SORTIE.0 { depasse(age / SAT_ENTREE_SORTIE.0) } else { 1.0 };
                    let sortie = (reste / SAT_ENTREE_SORTIE.1).clamp(0.0, 1.0).powi(2);
                    (1.0 - SAT_FONTE * age / dur as f32) * entree * sortie
                }
            };
            let al = (k * 1.5).min(1.0) * ms.opacite;
            let vie = vie_utile(vie_sats, self.w_lisse[1] * s.mul);
            draw_trail(cv, xy, &s.trail, t, vie, 2.6 * k * scale * ms.taille, al, false, cfg, (s.couleur, ms.teinte));
            if let Some(&(_, x, y)) = s.trail.back() {
                let rgb = prisme_palette(cfg, t, 1.0, s.couleur, ms.teinte);
                let front = y > self.center.map_or(0.0, |c| c.1);
                heads.push(Head { x, y, s: (0.6 + 1.6 * k) * scale * ms.taille, al, lueur: ms.luminosite, front, rgb, queue: queue(&s.trail), cachee: false });
            }
        }
        // les comètes (devant : sous le centre de l'orbite, comme vues de biais) ; chacune prend
        // une teinte de la palette. En rubans : une traînée large, vrillée, sans tête
        if o.cometes {
            let cy = self.center.map_or(0.0, |c| c.1);
            let vie_reglee = o.trainee.max(0.01) as f64 * mc.trainee as f64;
            let nombre = o.nombre.max(1) as usize;
            let ru = &cfg.rubans;
            let phase = (-VRILLE.1 * t).rem_euclid(TAU as f64) as f32 + self.vrille.1;
            for (i, tr) in self.trails.iter().enumerate() {
                // vie utile de son anneau (`vie_utile`), comme dans `update`
                let vie = self.vies.get(i / nombre).copied().unwrap_or(vie_reglee);
                if ru.actif {
                    let vrille = (VRILLE.0 * ru.vrille.clamp(0.0, 1.0), phase + 2.0 * i as f32);
                    draw_ruban(cv, xy, tr, t, vie, (ru.largeur.clamp(1.0, 14.0) * scale * mc.taille, 0.6 * scale), mc.opacite, cfg, (i, mc.teinte), vrille);
                } else {
                    draw_trail(cv, xy, tr, t, vie, 3.5 * scale * mc.taille, mc.opacite, true, cfg, (i, mc.teinte));
                }
                if let Some(&(_, x, y)) = tr.back() {
                    let rgb = prisme_palette(cfg, t, 1.0, i, mc.teinte);
                    heads.push(Head { x, y, s: 2.4 * scale * mc.taille, al: mc.opacite, lueur: mc.luminosite, front: y > cy, rgb, queue: queue(tr), cachee: ru.actif });
                }
            }
        }
        let head_rgb = prisme(cfg, t, 1.0);
        // constellation : fines lignes entre les têtes proches, sous les têtes
        if cfg.essaim.constellation {
            points.clear();
            points.extend(heads.iter().map(|h| (h.x, h.y)));
            liens(points, LIENS_PORTEE * o.rayon * scale * self.mods[0].rayon.max(0.2), paires);
            let rgb = prisme(cfg, t, 0.85);
            for &(i, j, f) in paires.iter() {
                let al = (0.25 + 0.5 * f) * heads[i].al.min(heads[j].al);
                cv.line((heads[i].x, heads[i].y), (heads[j].x, heads[j].y), 1.2 * scale, rgb, al, false);
            }
        }
        let draw_head = |cv: &mut dyn Canvas, h: &Head| {
            if h.queue.0.abs() + h.queue.1.abs() > 0.5 {
                // tête étirée : les mêmes quatre couches, en traits aux bouts ronds
                let (a, b) = ((h.x, h.y), (h.x + h.queue.0, h.y + h.queue.1));
                cv.line(a, b, h.s * 5.2, h.rgb, 0.12 * h.al * glow * h.lueur, true);
                cv.line(a, b, h.s * 3.2, h.rgb, 0.3 * h.al * glow * h.lueur, true);
                cv.line(a, b, h.s * 2.0, h.rgb, h.al, true);
                cv.line(a, (h.x + h.queue.0 * 0.55, h.y + h.queue.1 * 0.55), h.s * 0.9, (255, 255, 255), h.al, true);
                return;
            }
            // lueur : deux disques translucides (un dégradé radial coûte bien plus cher sur processeur)
            cv.circle(h.x, h.y, h.s * 2.6, h.rgb, 0.12 * h.al * glow * h.lueur);
            cv.circle(h.x, h.y, h.s * 1.6, h.rgb, 0.3 * h.al * glow * h.lueur);
            cv.circle(h.x, h.y, h.s, h.rgb, h.al);
            cv.circle(h.x, h.y, h.s * 0.45, (255, 255, 255), h.al);
        };
        for h in heads.iter().filter(|h| !h.front && !h.cachee) {
            draw_head(cv, h);
        }
        // mode sans flashs : ni halo blanc, ni anneau ou carré trop vifs
        let vif = if self.doux { 0.45 } else { 1.0 };
        if self.env > 0.02 && cfg.eclat.taille > 0.0 {
            let (px, py) = cursor;
            let r = cfg.eclat.taille * scale;
            if re.actif {
                // mode ressorts : un anneau épais qui sort en dépassant puis s'affine, sans halo
                // blanc ; son avancée se lit sur l'éclat, qui retombe de 1 à 0,02 en 0,42 s
                let k = (-self.env.min(1.0).ln() / 3.78).clamp(0.0, 1.0);
                let epais = (ANNEAU_RESSORTS.1 * (1.0 - k).powi(2) + 0.6) * scale;
                cv.ring(px, py, (r * ANNEAU_RESSORTS.0 * depasse(k)).max(1.0), epais, head_rgb, 0.9 * (1.0 - k) * vif);
            } else {
                // éclat du kick : halo blanc et anneau coloré qui s'élargit sous le pointeur ; le
                // halo laisse un vide sous la pointe (le texte qu'elle vise n'est pas voilé)
                if !self.doux {
                    let (dehors, vide) = (r * (2.0 + 2.5 * self.env), HALO_VIDE * scale);
                    if dehors > vide {
                        cv.ring(px, py, (dehors + vide) / 2.0, dehors - vide, (255, 255, 255), 0.35 * self.env);
                    }
                }
                cv.ring(px, py, r * (2.5 + 5.0 * (1.0 - self.env)), 1.5 * scale, head_rgb, 0.8 * self.env * vif);
            }
        }
        // particules : étincelles, ondes de choc, poussière, carrés des caisses claires, implosions
        for p in &self.particules {
            let k = ((t - p.ne) as f32 / p.vie).clamp(0.0, 1.0);
            match p.genre {
                Genre::Etincelle => {
                    cv.circle(p.x, p.y, p.taille * 2.2, head_rgb, 0.25 * (1.0 - k));
                    // cœur blanc, atténué en mode sans flashs (une salve d'aigus en ferait des éclairs)
                    cv.circle(p.x, p.y, p.taille * (1.0 - 0.5 * k), (255, 255, 255), (1.0 - k) * vif);
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
        for h in heads.iter().filter(|h| h.front && !h.cachee) {
            draw_head(cv, h);
        }
    }
}

/// Étirement d'une tête (mode ressorts) : vecteur de la tête au bout de son étirement, à
/// l'opposé de sa vitesse, long du trajet de `ETIREMENT.0` s (borné), fois `force`.
fn etirement(tr: &Trail, scale: f32, force: f32) -> (f32, f32) {
    let n = tr.len();
    if n < 2 || force <= 0.0 {
        return (0.0, 0.0);
    }
    let (a, b) = (tr[n - 2], tr[n - 1]);
    let (dx, dy, dt) = (b.1 - a.1, b.2 - a.2, (b.0 - a.0) as f32);
    let d = (dx * dx + dy * dy).sqrt();
    if dt < 1e-4 || d < 1e-3 {
        return (0.0, 0.0);
    }
    let l = (d / dt * ETIREMENT.0).min(ETIREMENT.1 * scale) * force.min(2.0);
    (-dx / d * l, -dy / d * l)
}

/// Dessine une traînée en ruban, dans la teinte `j` de la palette décalée de `decale` degrés :
/// large au milieu, affinée aux deux bouts (`largeur` : au plus large et au plus fin, px), sa
/// largeur ondulant le long de la traînée (`vrille` : profondeur de 0 à 1, phase). Par morceaux de
/// `RUBAN_MORCEAU` segments, chacun de la largeur et de la couleur de son milieu.
#[allow(clippy::too_many_arguments)]
fn draw_ruban(cv: &mut dyn Canvas, xy: &mut Vec<(f32, f32)>, tr: &Trail, t: f64, life: f64, largeur: (f32, f32), al: f32, cfg: &Config, (j, decale): (usize, f32), vrille: (f32, f32)) {
    if tr.len() < 2 {
        return;
    }
    let q = |i: usize| (1.0 - (t - tr[i].0) / life).clamp(0.0, 1.0) as f32;
    xy.clear();
    xy.extend(tr.iter().map(|&(_, x, y)| (x, y)));
    let mut i = 0;
    while i + 1 < xy.len() {
        let end = (i + RUBAN_MORCEAU).min(xy.len() - 1);
        let k = (q(i) + q(end)) * 0.5;
        let profil = (PI * k.powf(0.75)).sin().max(0.0).powf(0.8);
        let onde = 0.5 + 0.5 * (5.5 * k + vrille.1).sin();
        let w = (largeur.0 * profil * (1.0 - vrille.0 * (1.0 - onde))).max(largeur.1);
        cv.trainee(&xy[i..=end], w, prisme_palette(cfg, t, k, j, decale), al, true);
        i = end;
    }
}

/// Segments de traînée regroupés par morceaux : chaque morceau est un seul tracé (couleur,
/// épaisseur et opacité de son milieu). Sur processeur, bien moins coûteux qu'un tracé par
/// segment, à l'œil identique ; le lissage y coûte ~3,5× plus cher par tracé, il est donc
/// réservé aux comètes.
const TRAIL_CHUNK: usize = 4;

/// Dessine une traînée dans la teinte `j` de la palette, décalée de `decale` degrés
/// (`teinte = (j, decale)`).
#[allow(clippy::too_many_arguments)]
fn draw_trail(cv: &mut dyn Canvas, xy: &mut Vec<(f32, f32)>, tr: &Trail, t: f64, life: f64, width: f32, al: f32, halo: bool, cfg: &Config, (j, decale): (usize, f32)) {
    if tr.len() < 2 {
        return;
    }
    // fraîcheur du point `i` : 1 en tête, 0 en bout de vie
    let q = |i: usize| (1.0 - (t - tr[i].0) / life).max(0.0) as f32;
    xy.clear();
    xy.extend(tr.iter().map(|&(_, x, y)| (x, y)));
    if halo {
        cv.trainee(xy, (0.8 + width * 0.7) * 2.6, prisme_palette(cfg, t, 0.7, j, decale), 0.12 * al, false);
    }
    // les comètes (avec halo) sont lissées ; les satellites non, en deux morceaux (tête vive,
    // queue pâle) : c'est le poste le plus coûteux sur processeur
    let smooth = halo;
    let chunk = if smooth { TRAIL_CHUNK } else { (xy.len() - 1).div_ceil(2).max(1) };
    let mut i = 0;
    while i + 1 < xy.len() {
        let end = (i + chunk).min(xy.len() - 1);
        let k = (q(i) + q(end)) * 0.5;
        cv.trainee(&xy[i..=end], 0.8 + k * width, prisme_palette(cfg, t, k, j, decale), k * 0.9 * al, smooth);
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
    /// Échelle de l'écran : largeurs de l'éclat et du reflet, décalage du reflet (1 par défaut,
    /// `a_l_echelle`).
    echelle: f32,
}

impl Crack {
    /// La même fissure, ses traits fixes (éclat, reflet) à l'échelle `scale` de l'écran ; sa zone
    /// (`rect`) suit.
    pub fn a_l_echelle(mut self, scale: f32) -> Self {
        let scale = if scale.is_finite() { scale.max(0.1) } else { 1.0 };
        self.ext += MARGE_FISSURE * ((scale - self.echelle).max(0.0));
        self.echelle = scale;
        *self.fixe.borrow_mut() = None;
        self
    }

    pub fn new(x: f32, y: f32, t: f64, size: f32, rng: &mut Rng) -> Self {
        let mut segs = Vec::new();
        let rays = 3 + (rng.f() * 4.0) as usize;
        let off = rng.range(0.0, TAU);
        let mut stack = Vec::new();
        for i in 0..rays {
            let ang = off + i as f32 / rays as f32 * TAU + rng.range(-0.35, 0.35);
            stack.push((0.0f32, 0.0f32, ang, rng.range(FISSURE_RAYON.0, FISSURE_RAYON.1) * size, rng.range(1.6, 2.6) * size.sqrt(), 0u8));
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
        let ext = segs.iter().fold(8.0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()).max(s[2].abs()).max(s[3].abs())) + MARGE_FISSURE;
        let mut branches: Vec<Vec<(f32, f32)>> = Vec::new();
        for s in &segs {
            let (a, b) = ((x + s[0], y + s[1]), (x + s[2], y + s[3]));
            match branches.last_mut() {
                Some(br) if br.last() == Some(&a) => br.push(b),
                _ => branches.push(vec![a, b]),
            }
        }
        Self { x, y, segs, ext, born: t, fixe: Default::default(), branches, echelle: 1.0 }
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
            let e = self.echelle;
            for br in &self.branches {
                cv.polyline(br, 3.5 * e, rgb, glow * 0.3 * opacity, true);
            }
            for br in &self.branches {
                cv.polyline(br, 1.2 * e, rgb, glow * opacity, true);
            }
        }
    }

    /// Trait sombre puis reflet décalé.
    fn passes_fixes(&self, cv: &mut dyn Canvas, opacity: f32) {
        let (x, y, e) = (self.x, self.y, self.echelle);
        for s in &self.segs {
            cv.line((x + s[0], y + s[1]), (x + s[2], y + s[3]), s[4], (2, 3, 8), 0.92 * opacity, true);
        }
        let d = 0.8 * e;
        for s in &self.segs {
            let w = (s[4] * 0.35).max(0.5 * e);
            cv.line((x + s[0] + d, y + s[1] + d), (x + s[2] + d, y + s[3] + d), w, (170, 185, 250), 0.25 * opacity, true);
        }
    }
}

/// Marge autour de la fissure, à 100 % (moitié de l'éclat, reflet décalé, lissage).
const MARGE_FISSURE: f32 = 6.0;
/// Longueur d'un rayon de fissure, en tailles (tirée entre les deux), et portée la plus longue
/// d'une fissure (px à 100 %) : une branche ne va jamais plus loin que son rayon (plus un pas).
const FISSURE_RAYON: (f32, f32) = (18.0, 44.0);
const FISSURE_PORTEE_MAX: f32 = 400.0;

/// Taille d'une fissure (`Crack::new`) : `[fissures] taille` × la force de son déclencheur, bornée
/// pour que ses rayons ne dépassent pas `FISSURE_PORTEE_MAX` (taille 6 et force 3 la portaient à
/// ~800 px et plus), × l'échelle de l'écran.
pub fn taille_fissure(taille: f32, force: f32, scale: f32) -> f32 {
    let t = taille * force;
    let t = if t.is_finite() { t.clamp(0.0, FISSURE_PORTEE_MAX / FISSURE_RAYON.1) } else { 0.0 };
    t * scale
}

/// Taille de l'onde d'un changement de piste (pixels à 100 %), et sa durée (s).
const ONDE_PISTE: (f32, f32) = (90.0, 0.8);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Cible, Groupe, Modulation, Source};
    use tiny_skia::Pixmap;

    const C: (f32, f32) = (300.0, 300.0);

    /// Son d'essai, 8 s : kicks sur chaque temps (120 BPM), caisse claire un temps sur deux,
    /// charleston aux contretemps, basse qui retombe, note qui ondule, stéréo qui balance ;
    /// montée de 3 à 5 s (pas de kick, roulement, tension), drop à 5 s ; break de 6,5 à 7,5 s.
    pub(super) fn son_essai(t: f64) -> AudioView {
        let temps = 0.5;
        let k = (t / temps).floor();
        let phase = ((t - k * temps) / temps) as f32;
        let montee = (3.0..5.0).contains(&t);
        let pause = (6.5..7.5).contains(&t);
        let kicks = if t < 3.0 { k } else if t < 5.0 { 6.0 } else if t < 6.5 { 6.0 + (k - 9.0) } else if t < 7.5 { 9.0 } else { 9.0 + (k - 14.0) };
        let roulement = if montee { (4.0 * (t - 3.0) + 3.0 * (t - 3.0).powi(2)).floor() } else if t >= 5.0 { 20.0 } else { 0.0 };
        AudioView {
            kicks: kicks.max(0.0) as u32,
            bass: if montee || pause { 0.2 } else { 0.35 + 0.5 * (-phase * 6.0).exp() },
            pitch: 42.0 + 6.0 * (t as f32 * 0.7).sin(),
            aigus: ((t + 0.25) / temps).floor() as u32,
            brillance: if montee { 0.35 + 0.2 * (t - 3.0) as f32 } else { 0.35 },
            bpm: Some(120.0),
            phase,
            relais: pause,
            caisses: (k / 2.0).floor() as u32 + roulement as u32,
            drops: if t >= 5.0 { 1 } else { 0 },
            tension: if montee { ((t - 3.0) / 2.0 * 1.2).min(1.0) as f32 } else { 0.0 },
            balance: (t as f32 * 0.8).sin() * 0.8,
        }
    }

    /// Mesures d'un preset sur le son d'essai (graine fixée), une toutes les 10 images : sens de
    /// rotation, satellites, particules, distance moyenne des têtes des comètes et des satellites
    /// au centre, éclat. Clic à 1 s, clic droit à 2 s, changement de piste à 2,5 s.
    pub(super) fn mesures(cfg: &Config, mut evenement: impl FnMut(&mut Swarm, usize, f64)) -> Vec<[f32; 6]> {
        let mut s = Swarm::new();
        s.rng = Rng::graine(7);
        let mut out = Vec::new();
        for i in 0..480 {
            let t = i as f64 / 60.0;
            evenement(&mut s, i, t);
            s.avance(t, 1.0 / 60.0, C, son_essai(t), cfg, 1.0);
            if i % 10 == 9 {
                let c = s.center.unwrap();
                let moyenne = |tr: &mut dyn Iterator<Item = &Trail>| {
                    let d: Vec<f32> = tr.filter_map(|tr| tr.back()).map(|p| ((p.1 - c.0).powi(2) + (p.2 - c.1).powi(2)).sqrt()).collect();
                    d.iter().sum::<f32>() / d.len().max(1) as f32
                };
                let (dc, ds) = (moyenne(&mut s.trails.iter()), moyenne(&mut s.sats.iter().chain(&s.fixes).map(|q| &q.trail)));
                out.push([s.dir, s.sats.len() as f32, s.particules.len() as f32, dc, ds, s.env]);
            }
        }
        out
    }

    /// Les presets fournis d'avant (`prereglages::presets_d_avant`), réglages d'avant convertis en
    /// modulations et déclencheurs, se comportent comme avant : les mesures de l'ancien moteur (`tests/donnees/rendu-presets.json`, Essaim 0.4) sur le même
    /// son d'essai. Même sens de rotation, mêmes satellites et particules à chaque mesure ;
    /// distance des têtes au centre (8 % près ; 0,1 % en médiane) et éclat, en moyenne sur chaque
    /// seconde (les réactions sont lissées un peu autrement, le pouls du break est un coup sur
    /// chaque temps).
    #[test]
    fn presets_comme_avant() {
        let avant: std::collections::HashMap<String, Vec<[f32; 6]>> = serde_json::from_str(include_str!("../tests/donnees/rendu-presets.json")).unwrap();
        let mut ecarts = Vec::new();
        for p in crate::prereglages::presets_d_avant() {
            let a = &avant[&p.name];
            let m = mesures(&p.cfg, |s, i, t| match i {
                60 => s.evenement(Evenement::Clic, C.0, C.1, t, &p.cfg, 1.0),
                120 => s.evenement(Evenement::ClicDroit, C.0, C.1, t, &p.cfg, 1.0),
                150 => s.evenement(Evenement::Piste, C.0, C.1, t, &p.cfg, 1.0),
                _ => {}
            });
            assert_eq!(m.len(), a.len());
            for (k, (x, y)) in m.iter().zip(a).enumerate() {
                assert_eq!((x[0], x[1], x[2]), (y[0], y[1], y[2]), "{} : sens, satellites, particules à la mesure {k}", p.name);
            }
            // moyennes par seconde (6 mesures)
            for (k, (x, y)) in m.chunks(6).zip(a.chunks(6)).enumerate() {
                let moy = |c: &[[f32; 6]], i: usize| c.iter().map(|v| v[i]).sum::<f32>() / c.len() as f32;
                for (i, nom, tolerance) in [(3, "comètes", 0.08), (4, "satellites", 0.08)] {
                    let (n, v) = (moy(x, i), moy(y, i));
                    let e = (n - v).abs() / v.abs().max(1.0);
                    ecarts.push(e);
                    assert!(e <= tolerance, "{} : distance des {nom}, seconde {k} : {n} contre {v}", p.name);
                }
                let (n, v) = (moy(x, 5), moy(y, 5));
                assert!((n - v).abs() <= 0.05, "{} : éclat, seconde {k} : {n} contre {v}", p.name);
            }
        }
        ecarts.sort_by(f32::total_cmp);
        println!("écart médian {:.4}, pire {:.4}", ecarts[ecarts.len() / 2], ecarts[ecarts.len() - 1]);
    }

    /// Retire les déclencheurs qui font `faire`.
    fn sans(cfg: &mut Config, faire: Action) {
        cfg.declencheurs.liste.retain(|d| d.faire != faire);
    }

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
        // déclencheur retiré : rien
        sans(&mut cfg, Action::Carre);
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
        // par les modulations de base : vitesse du trajet + basse + montée
        let vitesse = |s: &Swarm| s.vitesse(0);
        assert!(vitesse(&tendu) > 1.8 * vitesse(&calme), "{} contre {}", vitesse(&tendu), vitesse(&calme));
        // drop : éclat, satellites, grande onde et étincelles
        tendu.update(2.0, 0.016, C, AudioView { tension: 1.0, drops: 1, ..au }, &cfg, 1.0);
        assert_eq!(tendu.env, 1.0);
        assert!(tendu.sats.len() >= 6, "{} satellites", tendu.sats.len());
        assert!(tendu.particules.iter().any(|p| p.genre == Genre::Onde && p.taille >= ONDE_DROP.0));
        assert!(tendu.particules.iter().filter(|p| p.genre == Genre::Etincelle).count() >= ETINCELLES_DROP);
        // modulations de la montée et déclencheur du drop retirés : ni resserrement ni explosion
        cfg.modulations.liste.retain(|m| m.source != crate::config::Source::Montee);
        sans(&mut cfg, Action::Explosion);
        let mut coupe = Swarm::new();
        tourner(&mut coupe, AudioView { tension: 1.0, ..au }, &cfg, 0.0);
        coupe.update(2.0, 0.016, C, AudioView { tension: 1.0, drops: 1, ..au }, &cfg, 1.0);
        assert!((ecart_moyen(&coupe) - ecart_moyen(&calme)).abs() < 0.05 * ecart_moyen(&calme));
        assert!(coupe.sats.is_empty());
    }

    #[test]
    fn contretemps_pulse_au_milieu_du_temps() {
        // la source « contretemps » des modulations : un coup au milieu de chaque temps
        let cfg = Config::default();
        let mut s = Swarm::new();
        let au = |phase: f32| AudioView { bpm: Some(120.0), phase, ..Default::default() };
        s.update(0.0, 0.016, C, au(0.4), &cfg, 1.0);
        assert_eq!(s.sources.contretemps, 0.0);
        s.update(0.016, 0.016, C, au(0.55), &cfg, 1.0);
        assert!(s.sources.contretemps > 0.8, "{}", s.sources.contretemps);
        // passage du temps (0,9 → 0,1) : pas un contretemps ; sans tempo connu : rien
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, au(0.9), &cfg, 1.0);
        s.update(0.016, 0.016, C, au(0.1), &cfg, 1.0);
        s.update(0.032, 0.016, C, AudioView { phase: 0.6, ..Default::default() }, &cfg, 1.0);
        assert_eq!(s.sources.contretemps, 0.0);
    }

    #[test]
    fn stereo_penche_l_essaim_du_cote_le_plus_fort() {
        let mut cfg = Config::default();
        let au = AudioView { balance: 1.0, ..Default::default() };
        let mut s = Swarm::new();
        for i in 0..60 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, au, &cfg, 1.0);
        }
        // milieu des têtes des comètes (deux comètes opposées : le centre du trajet)
        let milieu = |s: &Swarm| s.trails.iter().filter_map(|tr| tr.back()).map(|p| p.1).sum::<f32>() / s.trails.len() as f32;
        assert!((milieu(&s) - C.0).abs() < 0.5, "sans modulation : l'essaim reste centré");
        use crate::config::{Cible, Groupe, Modulation, Source};
        cfg.modulations.liste.push(Modulation { source: Source::Stereo, cible: Cible::Decalage, sur: Groupe::Tout, quantite: 1.0, ..Default::default() });
        for i in 60..120 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, au, &cfg, 1.0);
        }
        let d = milieu(&s) - C.0;
        assert!(d > 0.8 * cfg.orbite.rayon && d <= 1.7 * cfg.orbite.rayon, "décalage {d}");
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
        cfg.particules.rebonds = false;
        s.particules.push(Particule { x: C.0, y: C.1, vx: -400.0, vy: 0.0, ne: 0.02, vie: 1.0, taille: 1.0, genre: Genre::Etincelle });
        s.update(0.04, 0.02, C, AudioView::default(), &cfg, 1.0);
        assert!(s.particules.last().unwrap().x < C.0 - 5.0);
    }

    #[test]
    fn palette_partage_les_teintes() {
        let mut cfg = Config::default();
        (cfg.couleur.vitesse_teinte, cfg.couleur.synchro_prisme) = (0.0, false);
        // une seule teinte : toutes pareilles
        assert_eq!(prisme_palette(&cfg, 0.0, 1.0, 0, 0.0), prisme_palette(&cfg, 0.0, 1.0, 1, 0.0));
        cfg.couleur.palette = 3;
        let t: Vec<_> = (0..4).map(|j| prisme_palette(&cfg, 0.0, 1.0, j, 0.0)).collect();
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
        s.evenement(Evenement::ClicDroit, C.0, C.1, 0.016, &cfg, 1.0);
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
        s.evenement(Evenement::Piste, 100.0, 100.0, 0.016, &cfg, 1.0);
        // pas de changement de sens ; 2 × 3 satellites, l'éclat et une onde
        assert_eq!((s.dir, s.sats.len(), s.env), (dir, 6, 1.0));
        assert_eq!(s.particules.iter().filter(|p| p.genre == Genre::Onde).count(), 1);
        // dans la limite du maximum
        let mut cfg = cfg;
        cfg.essaim.maximum = 7;
        s.evenement(Evenement::Piste, 100.0, 100.0, 0.02, &cfg, 1.0);
        assert_eq!(s.sats.len(), 7);
        // l'onde reste dans le rectangle redessiné
        s.update(0.03, 0.01, (100.0, 100.0), AudioView::default(), &cfg, 1.0);
        let (x0, y0, x1, y1) = s.bounds((100.0, 100.0), 1.0, cfg.eclat.taille);
        assert!(x0 <= 100.0 - ONDE_PISTE.0 && x1 >= 100.0 + ONDE_PISTE.0 && y0 <= 100.0 - ONDE_PISTE.0 && y1 >= 100.0 + ONDE_PISTE.0);
    }

    #[test]
    fn rotation_du_trajet_sur_les_satellites_seulement() {
        let mut cfg = Config::default();
        cfg.orbite.rotation = 30.0;
        cfg.orbite.rotation_sur = crate::config::Groupe::Satellites;
        let mut s = Swarm::new();
        for i in 0..60 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, AudioView::default(), &cfg, 1.0);
        }
        // une seconde à 30 tr/min : un demi-tour pour les satellites, rien pour les comètes
        assert_eq!(s.rot, 0.0);
        assert!((s.rot_sats - PI).abs() < 0.01, "{}", s.rot_sats);
    }

    #[test]
    fn trajet_redresse_quand_la_rotation_retombe() {
        use crate::config::{Cible, Groupe, Modulation, Source};
        let mut cfg = Config::default();
        let mut s = Swarm::new();
        // de travers (une modulation de rotation vient d'être retirée) : revient à l'endroit,
        // par le plus court (5,5 rad : en avançant, pas en reculant de presque un tour)
        (s.rot, s.rot_sats) = (2.0, 5.5);
        let mut sens = 0.0;
        for i in 0..120 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, AudioView::default(), &cfg, 1.0);
            if i == 0 {
                sens = s.rot_sats - 5.5;
            }
        }
        let a_l_endroit = |r: f32| r.min(TAU - r) < 0.01;
        assert!(a_l_endroit(s.rot) && a_l_endroit(s.rot_sats), "{} {}", s.rot, s.rot_sats);
        assert!(sens > 0.0, "par le plus court : {sens}");
        // une vraie rotation (réglage) n'est pas freinée : 30 tr/min, un demi-tour par seconde
        cfg.orbite.rotation = 30.0;
        let mut s = Swarm::new();
        for i in 0..60 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, AudioView::default(), &cfg, 1.0);
        }
        assert!((s.rot - PI).abs() < 0.01, "{}", s.rot);
        // modulation de rotation qui retombe (après le drop) : le trajet revient droit
        cfg.orbite.rotation = 0.0;
        cfg.modulations.liste = vec![Modulation { source: Source::Drop, cible: Cible::Rotation, sur: Groupe::Tout, quantite: 1.0, ..Default::default() }];
        let mut s = Swarm::new();
        let au = AudioView { drops: 1, ..Default::default() };
        for i in 0..60 * 20 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, au, &cfg, 1.0);
        }
        assert!(s.mods[0].rotation.abs() < 0.1, "le drop est retombé : {}", s.mods[0].rotation);
        assert!(a_l_endroit(s.rot), "{}", s.rot);
    }

    #[test]
    fn drop_module_la_rotation_des_satellites() {
        use crate::config::{Cible, Groupe, Modulation, Source};
        let mut cfg = Config::default();
        cfg.modulations.liste = vec![Modulation { source: Source::Drop, cible: Cible::Rotation, sur: Groupe::Satellites, quantite: 1.0, ..Default::default() }];
        let mut s = Swarm::new();
        let mut au = AudioView::default();
        for i in 0..30 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, au, &cfg, 1.0);
        }
        assert_eq!((s.rot, s.rot_sats), (0.0, 0.0), "rien avant le drop");
        au.drops = 1;
        for i in 30..90 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, au, &cfg, 1.0);
        }
        // après le drop, le trajet des satellites tourne (jusqu'à 90 tr/min), pas celui des comètes
        assert_eq!(s.rot, 0.0);
        assert!(s.rot_sats > 1.0, "{}", s.rot_sats);
        assert!(s.mods[1].rotation > 10.0 && s.mods[0].rotation == 0.0);
    }

    #[test]
    fn basse_module_rayon_et_taille_qui_tiennent_dans_le_rectangle() {
        use crate::config::{Cible, Groupe, Modulation, Source};
        let mut cfg = Config::default();
        cfg.modulations.liste = vec![
            Modulation { source: Source::Basse, cible: Cible::Rayon, sur: Groupe::Cometes, quantite: 0.25, ..Default::default() },
            Modulation { source: Source::Basse, cible: Cible::Taille, sur: Groupe::Tout, quantite: 1.0, ..Default::default() },
        ];
        let (mut calme, mut module) = (Swarm::new(), Swarm::new());
        let au = AudioView { bass: 1.0, pitch: 60.0, ..Default::default() };
        let mut sans = Config::default();
        sans.modulations.liste.clear();
        for i in 0..120 {
            let t = i as f64 / 60.0;
            calme.update(t, 1.0 / 60.0, C, au, &sans, 1.0);
            module.update(t, 1.0 / 60.0, C, au, &cfg, 1.0);
        }
        assert!(ecart_moyen(&module) > 1.8 * ecart_moyen(&calme), "{} contre {}", ecart_moyen(&module), ecart_moyen(&calme));
        assert!((module.mods[0].taille - 2.5).abs() < 0.01);
        // têtes grossies : le dessin tient dans le rectangle (rien en dehors ne serait effacé)
        let (pm, _) = render(&module, C, 2.0, &cfg, 1.0);
        let bord = |x: u32, y: u32| pm.pixel(x, y).map_or(0, |p| p.alpha());
        let (w, h) = (pm.width(), pm.height());
        assert!((0..w).all(|x| bord(x, 0) == 0 && bord(x, h - 1) == 0) && (0..h).all(|y| bord(0, y) == 0 && bord(w - 1, y) == 0));
    }

    #[test]
    fn kick_sans_inversion_du_sens() {
        let mut cfg = Config::default();
        sans(&mut cfg, Action::Inversion);
        let mut s = Swarm::new();
        let mut au = AudioView::default();
        s.update(0.0, 0.016, C, au, &cfg, 1.0);
        let dir = s.dir;
        au.kicks = 1;
        s.update(0.016, 0.016, C, au, &cfg, 1.0);
        assert_eq!(s.dir, dir);
        assert!(!s.sats.is_empty(), "les satellites arrivent quand même");
    }

    #[test]
    fn une_inversion_par_image_et_vitesses_bornees() {
        // deux inversions à la même image (deux déclencheurs, ou deux kicks) : le sens change une fois
        let mut cfg = Config::default();
        cfg.declencheurs.liste = vec![Declencheur::new(Evenement::Clic, Action::Inversion); 2];
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, AudioView::default(), &cfg, 1.0);
        s.evenement(Evenement::Clic, C.0, C.1, 0.016, &cfg, 1.0);
        assert_eq!(s.dir, -1.0);
        s.evenement(Evenement::Clic, C.0, C.1, 0.032, &cfg, 1.0);
        assert_eq!(s.dir, 1.0, "à l'image suivante, de nouveau");
        cfg.declencheurs.liste = vec![Declencheur::new(Evenement::Kick, Action::Inversion)];
        s.update(0.048, 0.016, C, AudioView { kicks: 2, ..Default::default() }, &cfg, 1.0);
        assert_eq!(s.dir, -1.0, "deux kicks dans l'image");
        // modulations à fond qui s'empilent : vitesse et rotation bornées
        use crate::config::{Cible, Groupe, Modulation, Source};
        let m = Modulation { source: Source::Basse, cible: Cible::Vitesse, sur: Groupe::Tout, quantite: 1.0, lissage: 0.0, ..Default::default() };
        cfg.modulations.liste = vec![m; 4];
        cfg.modulations.liste.extend([Modulation { cible: Cible::Rotation, ..m }; 4]);
        cfg.orbite.rotation = 30.0;
        let au = AudioView { bass: 1.0, ..Default::default() };
        for i in 0..30 {
            s.update(0.1 + i as f64 * 0.016, 0.016, C, au, &cfg, 1.0);
        }
        assert!((s.vitesse(0) - modulation::VITESSE_MAX).abs() < 1e-3, "{}", s.vitesse(0));
        // 180 tr/min : 3 tours par seconde, soit 0,30 rad par image de 16 ms
        let avant = s.rot;
        s.update(0.6, 0.016, C, au, &cfg, 1.0);
        let pas = (s.rot - avant).rem_euclid(TAU);
        assert!((pas - 180.0 / 60.0 * TAU * 0.016).abs() < 0.01, "{pas}");
    }

    #[test]
    fn declencheurs_delai_chance_et_fissure_au_drop() {
        let ondes = |s: &Swarm| s.particules.iter().filter(|p| p.genre == Genre::Onde).count();
        // kick → onde, une fois par seconde au plus : trois kicks en une demi-seconde, une onde
        let mut cfg = Config::default();
        cfg.declencheurs.liste = vec![Declencheur { delai: 1.0, ..Declencheur::new(Evenement::Kick, Action::Onde) }];
        let mut s = Swarm::new();
        for i in 0..30u32 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, AudioView { kicks: i / 10, ..Default::default() }, &cfg, 1.0);
        }
        assert_eq!(ondes(&s), 1);
        // chance nulle : jamais ; chance pleine : à chaque fois
        cfg.declencheurs.liste = vec![Declencheur { probabilite: 0.0, ..Declencheur::new(Evenement::Clic, Action::Onde) }];
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, AudioView::default(), &cfg, 1.0);
        for i in 0..20 {
            s.evenement(Evenement::Clic, C.0, C.1, i as f64 * 0.01, &cfg, 1.0);
        }
        assert_eq!(ondes(&s), 0);
        cfg.declencheurs.liste[0].probabilite = 1.0;
        s.evenement(Evenement::Clic, C.0, C.1, 0.3, &cfg, 1.0);
        assert_eq!(ondes(&s), 1);
        // drop → fissure : demandée à la boucle principale, au pointeur, avec sa force
        cfg.declencheurs.liste = vec![Declencheur { force: 2.0, ..Declencheur::new(Evenement::Drop, Action::Fissure) }];
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, AudioView::default(), &cfg, 1.0);
        s.update(0.016, 0.016, C, AudioView { drops: 1, ..Default::default() }, &cfg, 1.0);
        assert_eq!(s.fissures, vec![(C.0, C.1, 2.0)]);
    }

    #[test]
    fn teinte_sur_tout_et_opacite() {
        use crate::config::{Cible, Groupe, Modulation, Source};
        let mut cfg = Config::default();
        let m = |source, cible, sur, quantite| Modulation { source, cible, sur, quantite, lissage: 0.0, ..Default::default() };
        cfg.modulations.liste = vec![m(Source::Basse, Cible::Teinte, Groupe::Tout, 0.5), m(Source::Basse, Cible::Opacite, Groupe::Satellites, -0.5)];
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, AudioView { bass: 1.0, ..Default::default() }, &cfg, 1.0);
        assert_eq!(s.teinte_globale(), 90.0);
        assert_eq!((s.mods[0].teinte, s.mods[0].opacite, s.mods[1].opacite), (0.0, 1.0, 0.5));
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
        // déclencheur de base : « break → éclat », force 0,35, sur chaque temps
        let au = AudioView { relais: true, phase: 0.0, bpm: Some(128.0), ..Default::default() };
        s.update(0.0, 0.016, (100.0, 100.0), AudioView { phase: 0.9, ..au }, &cfg, 1.0);
        let dir = s.dir;
        s.update(0.016, 0.016, (100.0, 100.0), au, &cfg, 1.0);
        assert!((s.env - 0.35).abs() < 1e-4, "sur le temps : {}", s.env);
        assert_eq!((s.dir, s.sats.len()), (dir, 0), "un temps relais n'est pas un kick");
        // milieu du temps : presque retombé
        s.env = 0.0;
        s.update(0.032, 0.016, (100.0, 100.0), AudioView { phase: 0.5, ..au }, &cfg, 1.0);
        assert!(s.env < 0.03, "{}", s.env);
        // déclencheur retiré : rien sur le temps suivant
        cfg.declencheurs.liste.retain(|d| d.quand != Evenement::Break);
        s.env = 0.0;
        s.update(0.048, 0.016, (100.0, 100.0), AudioView { phase: 0.9, ..au }, &cfg, 1.0);
        s.update(0.064, 0.016, (100.0, 100.0), au, &cfg, 1.0);
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
        assert!(s.vitesse(0).is_finite() && s.mods[0].rayon.is_finite() && s.rot.is_finite() && s.angles.iter().all(|a| a.is_finite()));
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
        // par les modulations de base : rayon et vitesse effectifs des comètes
        let effectifs = |s: &Swarm| (s.mods[0].rayon, s.vitesse(0));
        let (far0, spd0) = effectifs(&s);
        for i in 60..90 {
            s.update(i as f64 * 0.016, 0.016, (0.0, 0.0), deep, &cfg, 1.0);
        }
        let (far, spd) = effectifs(&s);
        assert!(far > far0 + 1.0, "éloignement {far0} → {far}");
        assert!(spd > spd0 + 8.0, "vitesse {spd0} → {spd}");
        // sans les modulations, la basse ne change plus rien
        let mut sans = Config::default();
        sans.modulations.liste.clear();
        let mut t = Swarm::new();
        for i in 0..90 {
            t.update(i as f64 * 0.016, 0.016, (0.0, 0.0), deep, &sans, 1.0);
        }
        assert!((effectifs(&t).0 - 1.0).abs() < 0.01 && (effectifs(&t).1 - sans.orbite.vitesse_base).abs() < 0.05, "{:?}", effectifs(&t));
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
    fn eclat_du_kick_laisse_la_pointe_libre() {
        // juste après un kick : le halo blanc entoure la pointe sans la voiler
        let mut cfg = Config::default();
        cfg.eclat.taille = 4.0;
        let mut s = Swarm::new();
        s.update(0.0, 1.0 / 60.0, C, AudioView::default(), &cfg, 1.0);
        s.env = 1.0;
        let (pm, o) = render(&s, C, 0.0, &cfg, 1.0);
        let alpha = |x: f32, y: f32| pm.pixel((x - o.0) as u32, (y - o.1) as u32).unwrap().alpha();
        assert_eq!(alpha(C.0, C.1), 0, "rien sous la pointe");
        assert_eq!(alpha(C.0 + 3.0, C.1 + 3.0), 0);
        // halo jusqu'à 4,5 fois la taille de l'éclat (18 px) : lumière entre le vide et ce bord
        assert!(alpha(C.0 + (HALO_VIDE + 18.0) / 2.0, C.1) > 0, "même lumière autour");
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

        // cas extrême : 32 comètes, 64 satellites, 32 permanents, traînées de 15 s, vitesse 60,
        // pointeur qui tourne vite ; simulation et dessin (processeur, puis carte graphique)
        let mut cfg = Config::default();
        (cfg.orbite.nombre, cfg.orbite.anneaux, cfg.orbite.trainee, cfg.orbite.vitesse_base) = (8, 4, 3.0, 60.0);
        (cfg.essaim.maximum, cfg.essaim.fixes, cfg.essaim.duree) = (64, 32, 30.0);
        cfg.modulations.liste = vec![Modulation { source: Source::Basse, cible: Cible::Trainee, sur: Groupe::Tout, quantite: 1.0, ..Default::default() }];
        cfg.declencheurs.liste = vec![Declencheur { nombre: 8, nombre_max: 8, ..Declencheur::new(Evenement::Kick, Action::Satellites) }];
        let mut s = Swarm::new();
        let mut au = AudioView { bass: 1.0, ..Default::default() };
        let curseur = |i: usize| {
            let a = i as f32 * dt * 6.0;
            (900.0 + 600.0 * a.cos(), 600.0 + 400.0 * a.sin())
        };
        let (chauffe, n) = (144 * 8, 200);
        let mut b = crate::backend::Backend::new(true);
        let mons = [(1isize, (0, 0, 1920, 1080))];
        let (mut cpu, mut gpu, mut simu) = (0.0, 0.0, 0.0);
        for i in 0..chauffe + n {
            au.kicks += (i % 20 == 0) as u32;
            let (t, c) = (i as f64 * dt as f64, curseur(i));
            let t0 = std::time::Instant::now();
            s.update(t, dt, c, au, &cfg, 1.5);
            let t1 = std::time::Instant::now();
            if i < chauffe {
                continue;
            }
            let _ = render(&s, c, t, &cfg, 1.5);
            let t2 = std::time::Instant::now();
            let (x0, y0, x1, y1) = s.bounds(c, 1.5, cfg.eclat.taille);
            b.frame(&mons, (x0 as i32, y0 as i32, x1 as i32 + 1, y1 as i32 + 1), &[], (c.0 as i32, c.1 as i32), &|cv, _| s.draw(cv, t, &cfg, 1.5, c));
            simu += (t1 - t0).as_secs_f64();
            cpu += (t2 - t1).as_secs_f64();
            gpu += t2.elapsed().as_secs_f64();
        }
        let points: usize = s.trails.iter().chain(s.sats.iter().chain(&s.fixes).map(|q| &q.trail)).map(|tr| tr.len()).sum();
        let ms = |x: f64| x * 1000.0 / n as f64;
        eprintln!(
            "extrême ({} traînées, {points} points) : simulation {:.3} ms, dessin processeur {:.3} ms, carte graphique {:.3} ms par image",
            s.trails.len() + s.sats.len() + s.fixes.len(),
            ms(simu),
            ms(cpu),
            ms(gpu)
        );
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
            fn vignette(&mut self, img: &Pixmap, id: u64, x: f32, y: f32, al: f32) {
                self.0.vignette(img, id, x, y, al);
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

    /// Planche d'un preset fourni sur le son d'essai, pour juger son allure à l'œil : six images
    /// (après un kick, en plein groove, pendant la montée, au drop, après le drop, au break) à
    /// 200 %, sur fond sombre. Le pointeur décrit un petit cercle (les figures qui suivent son
    /// mouvement se voient) ; les bandes de fréquences sont simulées.
    /// `ESSAIM_PLANCHE=dossier ESSAIM_PRESET=Rubans cargo test --release exporter_planche -- --ignored`
    #[test]
    #[ignore]
    fn exporter_planche() {
        let dossier = std::env::var("ESSAIM_PLANCHE").expect("ESSAIM_PLANCHE : dossier de sortie");
        let nom = std::env::var("ESSAIM_PRESET").expect("ESSAIM_PRESET : nom du preset");
        let p = crate::prereglages::builtins().into_iter().find(|p| p.name == nom).expect("preset inconnu");
        let (cote, scale, instants) = (300u32, 2.0f32, [1.03, 2.3, 4.6, 5.05, 5.6, 7.0]);
        let mut planche = Pixmap::new(cote * instants.len() as u32, cote).unwrap();
        planche.fill(tiny_skia::Color::from_rgba8(17, 17, 17, 255));
        let mut s = Swarm::new();
        s.rng = Rng::graine(7);
        // les petites figures aussi (`figures.rs`), avec leur propre écoute des coups
        let (mut figures, mut ecoute) = (crate::figures::Figures::new(), Ecoute::default());
        let (dt, mut t, mut k) = (1.0 / 120.0, 0.0f64, 0);
        while k < instants.len() {
            let au = son_essai(t);
            let c = (C.0 + 40.0 * (t as f32 * 9.0).cos(), C.1 + 40.0 * (t as f32 * 9.0).sin());
            let cloche = |x: f32, milieu: f32, large: f32| (-((x - milieu) / large).powi(2)).exp();
            let bandes: Vec<f32> = (0..16)
                .map(|i| {
                    let x = i as f32 / 15.0;
                    (au.bass * cloche(x, 0.1, 0.2) + 0.4 * (0.6 + 0.4 * (t as f32 * 2.3 + i as f32 * 1.7).sin()) * cloche(x, 0.5, 0.22) + au.brillance * cloche(x, 0.88, 0.14)).clamp(0.0, 1.0)
                })
                .collect();
            s.avance(t, dt, c, au, &p.cfg, scale);
            figures.avance(t, dt, c, scale, &crate::figures::Son { au: &au, coups: &ecoute.lire(&au), bandes: &bandes }, &p.cfg);
            if t >= instants[k] {
                let origin = (C.0 - cote as f32 / 2.0 - (k as u32 * cote) as f32, C.1 - cote as f32 / 2.0);
                let cv = &mut crate::canvas::PixCanvas { pm: &mut planche, origin };
                figures.dessiner(cv, t, &p.cfg, scale);
                s.draw(cv, t, &p.cfg, scale, c);
                // le pointeur : un point blanc
                cv.circle(c.0, c.1, 2.0 * scale, (255, 255, 255), 0.9);
                k += 1;
            }
            t += dt as f64;
        }
        planche.save_png(std::path::Path::new(&dossier).join(format!("{nom}.png"))).unwrap();
    }

    /// Distance de la tête de la première comète au centre de l'orbite.
    fn rayon_tete(s: &Swarm) -> f32 {
        let (c, p) = (s.center.unwrap(), *s.trails[0].back().unwrap());
        ((p.1 - c.0).powi(2) + (p.2 - c.1).powi(2)).sqrt()
    }

    #[test]
    fn rebond_l_orbite_part_depasse_et_revient() {
        use crate::config::{Action, Declencheur};
        // sans le mode ressorts : seul le déclencheur « rebond » fait bouger le rayon
        let mut cfg = Config::default();
        cfg.modulations.liste.clear();
        cfg.declencheurs.liste = vec![Declencheur::new(Evenement::Kick, Action::Rebond)];
        let repos = cfg.orbite.rayon;
        for dt in [1.0 / 144.0, 1.0 / 60.0, PAS_MAX] {
            let mut s = Swarm::new();
            let (mut t, mut haut, mut bas) = (0.0f64, 0.0f32, f32::MAX);
            for i in 0..(2.5 / dt) as usize {
                let au = AudioView { kicks: (t >= 0.5) as u32, ..Default::default() };
                s.update(t, dt, C, au, &cfg, 1.0);
                if i > 0 && t < 0.5 {
                    assert!((rayon_tete(&s) - repos).abs() < 0.01, "avant le kick : {}", rayon_tete(&s));
                }
                if t >= 0.5 {
                    (haut, bas) = (haut.max(rayon_tete(&s)), bas.min(rayon_tete(&s)));
                }
                t += dt as f64;
            }
            // parti de plus de 15 %, repassé sous le rayon de repos (dépassement), revenu à 1 % près
            assert!(haut > repos * 1.15 && haut < repos * 1.6, "pas de {dt} : sommet {haut}");
            assert!(bas < repos * 0.99, "pas de {dt} : creux {bas}");
            assert!((rayon_tete(&s) - repos).abs() < repos * 0.01, "pas de {dt} : fin {}", rayon_tete(&s));
        }
        // ressort le plus raide et le moins amorti, à 20 images par seconde : il reste borné
        (cfg.ressorts.raideur, cfg.ressorts.amorti, cfg.ressorts.actif) = (30.0, 0.1, true);
        let mut s = Swarm::new();
        for i in 0..200 {
            let au = AudioView { kicks: i / 10, tension: 1.0, ..Default::default() };
            s.update(i as f64 * PAS_MAX as f64, PAS_MAX, C, au, &cfg, 1.0);
            assert!(rayon_tete(&s).is_finite() && rayon_tete(&s) < repos * 6.0, "image {i} : {}", rayon_tete(&s));
        }
    }

    #[test]
    fn poussee_et_frein_agissent_sur_la_vitesse_puis_retombent() {
        use crate::config::{Action, Declencheur};
        let mut cfg = Config::default();
        cfg.modulations.liste.clear();
        cfg.declencheurs.liste = vec![Declencheur::new(Evenement::Kick, Action::Poussee), Declencheur::new(Evenement::Caisse, Action::Frein)];
        let mut s = Swarm::new();
        let dt = 1.0 / 120.0;
        let image = |s: &mut Swarm, i: usize, au: AudioView| s.update(i as f64 * dt as f64, dt, C, au, &cfg, 1.0);
        for i in 0..60 {
            image(&mut s, i, AudioView::default());
        }
        let base = s.vitesse(0);
        assert!((base - cfg.orbite.vitesse_base).abs() < 0.05, "{base}");
        // kick : la rotation prend de la vitesse, sans changer de sens, puis retombe
        image(&mut s, 60, AudioView { kicks: 1, ..Default::default() });
        assert!(s.vitesse(0) > base + 4.0 && s.dir == 1.0, "{}", s.vitesse(0));
        for i in 61..300 {
            image(&mut s, i, AudioView { kicks: 1, ..Default::default() });
        }
        assert!((s.vitesse(0) - base).abs() < 0.05, "{}", s.vitesse(0));
        // caisse claire : coup de frein (la vitesse tombe à moins du tiers), puis elle repart
        image(&mut s, 300, AudioView { kicks: 1, caisses: 1, ..Default::default() });
        assert!(s.vitesse(0) < base * 0.33 && s.vitesse(1) < base * 0.33, "{}", s.vitesse(0));
        for i in 301..450 {
            image(&mut s, i, AudioView { kicks: 1, caisses: 1, ..Default::default() });
        }
        assert!((s.vitesse(0) - base).abs() < 0.05, "{}", s.vitesse(0));
    }

    #[test]
    fn mode_ressorts_satellites_nes_au_centre_tetes_etirees_et_anneau() {
        let p = crate::prereglages::builtins().into_iter().find(|p| p.name == "Ressorts").unwrap();
        let cfg = &p.cfg;
        let mut s = Swarm::new();
        s.rng = Rng::graine(3);
        let dt = 1.0 / 120.0;
        let mut t = 0.0f64;
        let mut avance = |s: &mut Swarm, n: usize, kicks: u32| {
            for _ in 0..n {
                s.update(t, dt, C, AudioView { kicks, bass: 0.3, ..Default::default() }, cfg, 1.0);
                t += dt as f64;
            }
            t
        };
        avance(&mut s, 60, 0);
        // kick : des satellites naissent au centre, puis rejoignent leur orbite
        avance(&mut s, 2, 1);
        assert!(!s.sats.is_empty() && s.sats.iter().all(|q| q.rr < 0.3 * q.r), "nés loin du centre");
        let t = avance(&mut s, 120, 1);
        assert!(s.sats.iter().all(|q| (q.rr - q.r).abs() < 0.05 * q.r), "pas arrivés sur leur orbite");
        // les têtes sont étirées à l'opposé de leur vitesse, sans dépasser la longueur prévue
        let q = etirement(&s.trails[0], 1.0, cfg.ressorts.etirement);
        let long = (q.0 * q.0 + q.1 * q.1).sqrt();
        assert!(long > 1.0 && long <= ETIREMENT.1 + 1e-3, "étirement {long}");
        let (a, b) = (s.trails[0][s.trails[0].len() - 2], *s.trails[0].back().unwrap());
        assert!(q.0 * (b.1 - a.1) + q.1 * (b.2 - a.2) < 0.0, "étirée dans le sens de la marche");
        // le sens n'a pas changé (aucune inversion dans ce preset)
        assert_eq!(s.dir, 1.0);
        // l'éclat : un anneau, sans halo blanc sous le pointeur ; tout tient dans le rectangle
        s.env = 0.6;
        let (pm, origin) = render(&s, C, t, cfg, 1.0);
        let centre = pm.pixel((C.0 - origin.0) as u32, (C.1 - origin.1) as u32).unwrap();
        assert!(centre.alpha() < 30, "halo sous le pointeur : {centre:?}");
        let (w, h) = (pm.width(), pm.height());
        let bord = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((0..h).flat_map(|y| [(0, y), (w - 1, y)])).filter(|&(x, y)| pm.pixel(x, y).unwrap().alpha() > 0).count();
        assert_eq!(bord, 0, "dessin au bord du rectangle");
        assert!(pm.data().chunks_exact(4).filter(|p| p[3] > 40).count() > 150);
    }

    #[test]
    fn rubans_sans_tete_et_dans_le_rectangle() {
        let p = crate::prereglages::builtins().into_iter().find(|p| p.name == "Rubans").unwrap();
        let mut cfg = p.cfg.clone();
        // le ruban le plus large, gonflé par la basse et le kick
        cfg.rubans.largeur = 14.0;
        let mut s = Swarm::new();
        let dt = 1.0 / 120.0;
        for i in 0..360 {
            let au = AudioView { kicks: (i / 60) as u32, caisses: (i / 120) as u32, bass: 1.0, ..Default::default() };
            s.update(i as f64 * dt as f64, dt, C, au, &cfg, 1.5);
        }
        assert!(s.vrille.0 > 0.0, "la caisse claire n'a pas fait avancer la vrille");
        let t = 359.0 * dt as f64;
        let (pm, _) = render(&s, C, t, &cfg, 1.5);
        let (w, h) = (pm.width(), pm.height());
        let bord = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((0..h).flat_map(|y| [(0, y), (w - 1, y)])).filter(|&(x, y)| pm.pixel(x, y).unwrap().alpha() > 0).count();
        assert_eq!(bord, 0, "ruban au bord du rectangle");
        // un ruban large est bien là, et aucune tête blanche
        let peints = pm.data().chunks_exact(4).filter(|p| p[3] > 200).count();
        let blancs = pm.data().chunks_exact(4).filter(|p| p[3] > 200 && p[0] > 240 && p[1] > 240 && p[2] > 240).count();
        assert!(peints > 800 && blancs == 0, "{peints} pixels pleins, {blancs} blancs");
        // sans rubans : les mêmes réglages redonnent des têtes au cœur blanc
        cfg.rubans.actif = false;
        let (pm, _) = render(&s, C, t, &cfg, 1.5);
        assert!(pm.data().chunks_exact(4).any(|p| p[3] > 200 && p[0] > 240 && p[1] > 240 && p[2] > 240));
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
                let rr = cfg.orbite.rayon * q.r * s.mods[1].rayon;
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
        assert!(s.vitesse(0) > 12.0, "vitesse {}", s.vitesse(0));
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
        cfg.declencheurs.liste.push(Declencheur::new(Evenement::Clic, Action::Eclats));
        cfg.sillage.actif = true;
        let mut s = Swarm::new();
        let mut au = AudioView::default();
        let dt = 1.0 / 144.0;
        for i in 0..200 {
            au.aigus = i / 10;
            // souris qui file : 1200 px/s
            let cursor = (300.0 + i as f32 * 8.0, 400.0);
            s.update(i as f64 * dt as f64, dt, cursor, au, &cfg, 1.0);
            if i == 100 {
                s.evenement(Evenement::Clic, cursor.0, cursor.1, i as f64 * dt as f64, &cfg, 1.0);
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
    fn contours_mis_a_l_echelle_et_parcourus_coins_compris() {
        // un carré décentré : centré, point le plus loin à 1,2, coins atteints exactement
        let pts = vec![[10.0, 10.0], [30.0, 10.0], [30.0, 30.0], [10.0, 30.0]];
        let c = &mettre_a_l_echelle(&[pts])[0];
        let max = c.points.iter().map(|p| (p.0 * p.0 + p.1 * p.1).sqrt()).fold(0.0f32, f32::max);
        assert!((max - 1.2).abs() < 1e-3, "{max}");
        let k = 1.2 / 2f32.sqrt();
        for (u, coin) in [(0.0, (-k, -k)), (0.25, (k, -k)), (0.5, (k, k)), (0.75, (-k, k)), (1.0, (-k, -k))] {
            let p = c.point(u);
            assert!((p.0 - coin.0).abs() < 1e-4 && (p.1 - coin.1).abs() < 1e-4, "{u} : {p:?}");
        }
        // milieu d'un côté : sur le côté, pas en dedans
        assert!((c.point(0.125).1 + k).abs() < 1e-4);
        assert!(mettre_a_l_echelle(&[vec![[0.0, 0.0], [1.0, 1.0]]]).is_empty());
        // deux contours : mis à l'échelle ensemble, chacun garde sa place (gauche, droite)
        let carre = |x: f32| vec![[x, 0.0], [x + 10.0, 0.0], [x + 10.0, 10.0], [x, 10.0]];
        let deux = mettre_a_l_echelle(&[carre(0.0), carre(40.0)]);
        let milieu = |c: &Contour| c.points.iter().map(|p| p.0).sum::<f32>() / c.points.len() as f32;
        assert!(milieu(&deux[0]) < -0.5 && milieu(&deux[1]) > 0.5, "{} {}", milieu(&deux[0]), milieu(&deux[1]));
    }

    #[test]
    fn version_des_contours_seulement_quand_ils_changent() {
        // contours à part (ceux de l'icône sont partagés avec les autres essais, Caméléon compris)
        let (cible, version): (Contours, AtomicU32) = (std::sync::RwLock::new(Vec::new()), AtomicU32::new(0));
        let carre = |c: f32| vec![vec![[0.0, 0.0], [c, 0.0], [c, c], [0.0, c]]];
        remplacer_contours(&cible, &version, &carre(10.0));
        assert_eq!(version.load(Ordering::Relaxed), 1);
        // les mêmes (relecture des réglages, même icône) : rien ne change, les traînées restent
        remplacer_contours(&cible, &version, &carre(10.0));
        assert_eq!(version.load(Ordering::Relaxed), 1);
        // un autre contour (Alt+Tab vers une autre application) : nouvelle version
        remplacer_contours(&cible, &version, &[vec![[0.0, 0.0], [10.0, 0.0], [5.0, 8.0]]]);
        assert_eq!(version.load(Ordering::Relaxed), 2);
        assert_eq!(version_contours(Forme::Cercle), 0, "une forme calculée n'a pas de version");
    }

    #[test]
    fn preset_applique_vide_les_trainees() {
        let cfg = Config::default();
        let mut s = Swarm::avec_graine(3);
        for i in 0..30 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, AudioView::default(), &cfg, 1.0);
        }
        assert!(s.trails.iter().any(|t| !t.is_empty()));
        s.vider_trainees();
        assert!(s.trails.iter().all(VecDeque::is_empty));
        assert!(s.sats.iter().chain(&s.fixes).all(|q| q.trail.is_empty()));
    }

    #[test]
    fn un_anneau_par_contour_d_une_image() {
        use crate::config::Forme;
        // trois disques côte à côte (un mot de trois lettres) : trois anneaux, chacun sur le sien,
        // quel que soit le réglage « anneaux »
        let disque = |cx: f32| (0..64).map(|i| { let a = i as f32 * TAU / 64.0; [cx + 10.0 * a.cos(), 10.0 * a.sin()] }).collect::<Vec<_>>();
        let mut cfg = Config::default();
        (cfg.orbite.forme, cfg.orbite.anneaux, cfg.orbite.nombre) = (Forme::Trace, 1, 1);
        cfg.orbite.contours = vec![disque(-40.0), disque(0.0), disque(40.0)];
        set_traces(&cfg.orbite.chemins());
        assert_eq!(nombre_contours(Forme::Trace), 3);
        let mut s = Swarm::new();
        for i in 0..60 {
            s.update(i as f64 / 60.0, 1.0 / 60.0, C, AudioView::default(), &cfg, 1.0);
        }
        assert_eq!(s.trails.len(), 3);
        let x = |k: usize| s.trails[k].back().unwrap().1 - C.0;
        assert!(x(0) < -5.0 && x(1).abs() < x(0).abs() && x(2) > 5.0, "gauche, milieu, droite : {} {} {}", x(0), x(1), x(2));
        set_traces(&[]);
        assert_eq!(nombre_contours(Forme::Trace), 1);
    }

    #[test]
    fn les_trajets_sont_continus() {
        use crate::config::Forme;
        for forme in [
            Forme::Cercle, Forme::Triangle, Forme::Losange, Forme::Hexagone, Forme::Huit, Forme::Etoile, Forme::Fleur,
            Forme::Carre, Forme::Pentagone, Forme::Ellipse, Forme::Coeur, Forme::Trefle, Forme::Lissajous, Forme::Astroide,
        ] {
            let mut p = chemin(forme, 0.0);
            // chaque trajet tient dans son étendue (`etendue_forme`), qui ne le surestime pas
            let loin = (0..7200).map(|i| chemin(forme, i as f32 * TAU / 7200.0)).map(|(x, y)| x.hypot(y)).fold(0.0f32, f32::max);
            assert!(loin <= etendue_forme(forme) + 1e-4, "{forme:?} : {loin} au-delà de {}", etendue_forme(forme));
            assert!(loin >= etendue_forme(forme) - 0.01, "{forme:?} : {loin}, étendue {} trop large", etendue_forme(forme));
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

    #[test]
    fn coups_oublies_pendant_la_pause_aucune_rafale_a_la_reprise() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, AudioView::default(), &cfg, 1.0);
        let dir = s.dir;
        // pause (frappe, plein écran) : 6 kicks et un drop joués, lus et jetés ; le pointeur a bougé
        let pendant = AudioView { kicks: 6, drops: 1, ..Default::default() };
        s.oublier_coups(&pendant);
        assert_eq!(s.dernier_curseur, None);
        // reprise : rien ne part, et l'essaim ne glisse pas depuis l'ancienne place (pas de sillage)
        s.avance(5.0, 0.016, (C.0 + 800.0, C.1), pendant, &cfg, 1.0);
        assert_eq!((s.dir, s.sats.len(), s.env), (dir, 0, 0.0));
        assert!(s.particules.is_empty(), "{} particules", s.particules.len());
    }

    #[test]
    fn le_sillage_ne_depend_pas_de_la_cadence() {
        // la souris file à 1500 px/s pendant 0,3 s : à 240 images/s, chaque image ne voit qu'une
        // fraction de grain (avant : arrondie à 0, aucun sillage) ; autant de grains qu'à 60
        let mut cfg = Config::default();
        (cfg.sillage.actif, cfg.sillage.densite) = (true, 3.0);
        let grains = |ips: u32| {
            let mut s = Swarm::avec_graine(3);
            let dt = 1.0 / ips as f32;
            for i in 0..=(ips * 3 / 10) {
                let t = i as f64 * dt as f64;
                s.update(t, dt, (100.0 + 1500.0 * t as f32, 300.0), AudioView::default(), &cfg, 1.0);
            }
            s.particules.iter().filter(|p| p.genre == Genre::Poussiere).count() as f32
        };
        let (lent, vif) = (grains(60), grains(240));
        // 3 grains par 60 px sur 450 px : 22,5
        assert!(vif > 15.0 && (vif - lent).abs() <= 0.2 * lent, "60 i/s : {lent} grains, 240 i/s : {vif}");
        // à l'arrêt, le reste est oublié
        let mut s = Swarm::new();
        s.update(0.0, 0.004, C, AudioView::default(), &cfg, 1.0);
        s.update(0.004, 0.004, (C.0 + 6.0, C.1), AudioView::default(), &cfg, 1.0);
        assert!(s.reste_sillage > 0.0);
        s.update(0.008, 0.004, (C.0 + 6.0, C.1), AudioView::default(), &cfg, 1.0);
        assert_eq!(s.reste_sillage, 0.0);
    }

    #[test]
    fn nombre_de_cometes_change_les_trainees_repartent_de_zero() {
        let mut cfg = Config::default();
        cfg.orbite.nombre = 3;
        let mut s = Swarm::new();
        let dt = 1.0 / 60.0;
        for i in 0..30 {
            s.update(i as f64 * dt as f64, dt, C, AudioView::default(), &cfg, 1.0);
        }
        assert!(s.trails.iter().all(|tr| tr.len() > 3));
        // deux comètes au lieu de trois : aucune ne garde la traînée d'une autre
        cfg.orbite.nombre = 2;
        let t = 30.0 * dt as f64;
        s.update(t, dt, C, AudioView::default(), &cfg, 1.0);
        assert_eq!(s.trails.len(), 2);
        for tr in &s.trails {
            assert!(tr.iter().all(|p| p.0 >= t - dt as f64 - 1e-9), "point d'avant le changement gardé");
        }
    }

    #[test]
    fn delais_des_declencheurs_remis_a_zero_quand_la_liste_change() {
        let mut cfg = Config::default();
        cfg.declencheurs.liste = vec![Declencheur { delai: 5.0, ..Declencheur::new(Evenement::Clic, Action::Onde) }];
        let mut s = Swarm::new();
        s.update(0.0, 0.016, C, AudioView::default(), &cfg, 1.0);
        s.evenement(Evenement::Clic, C.0, C.1, 0.1, &cfg, 1.0);
        assert_eq!(s.particules.len(), 1);
        // même liste : le délai tient
        s.evenement(Evenement::Clic, C.0, C.1, 0.2, &cfg, 1.0);
        assert_eq!(s.particules.len(), 1);
        // autre déclencheur à la même place : il n'hérite pas du délai de l'ancien
        cfg.declencheurs.liste = vec![Declencheur { delai: 5.0, ..Declencheur::new(Evenement::Clic, Action::Eclats) }];
        s.evenement(Evenement::Clic, C.0, C.1, 0.3, &cfg, 1.0);
        assert_eq!(s.particules.iter().filter(|p| p.genre == Genre::Etincelle).count(), 16);
        // une force changée n'est pas une autre liste : le délai tient
        cfg.declencheurs.liste[0].force = 2.0;
        s.evenement(Evenement::Clic, C.0, C.1, 0.4, &cfg, 1.0);
        assert_eq!(s.particules.iter().filter(|p| p.genre == Genre::Etincelle).count(), 16);
    }

    #[test]
    fn longue_trainee_points_espaces() {
        // 12 s de traînée (modulation de la traînée) : points espacés d'autant
        let points = |vie: f64, px_par_s: f64, images: usize| {
            let mut tr = Trail::new();
            for i in 0..images {
                let t = i as f64 / 144.0;
                push_trail(&mut tr, t, (t * px_par_s) as f32, 0.0, vie, FINESSE_COMETES.1, TRAIL_POINTS_MAX);
            }
            tr.len()
        };
        // lente (144 px/s) : un point tous les `TRAIL_STEP` jusqu'à 3 s, quatre fois moins à 12 s
        let (courte, longue) = (points(3.0, 144.0, 14 * 144), points(12.0, 144.0, 14 * 144));
        assert!(longue as f32 <= 1.2 * courte as f32, "3 s : {courte} points, 12 s : {longue}");
        // jusqu'à 3 s, rien ne change : au moins 90 points par seconde
        assert!(courte >= 3 * 90, "{courte} points pour 3 s");
        // très rapide et très longue : jamais plus de `TRAIL_POINTS_MAX`
        assert_eq!(points(100.0, 20000.0, 6000), TRAIL_POINTS_MAX);
    }

    #[test]
    fn fissure_a_l_echelle_dans_sa_zone() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let c = Crack::new(300.0, 300.0, 0.0, 1.5 * 2.0, &mut rng).a_l_echelle(2.0);
        let (l, t, r, b) = c.rect();
        let mut pm = Pixmap::new(600, 600).unwrap();
        c.draw(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, 1.0, (120, 160, 255), 1.0);
        let mut peints = 0;
        for (j, p) in pm.data().chunks_exact(4).enumerate() {
            if p[3] > 0 {
                peints += 1;
                let (x, y) = ((j % 600) as i32, (j / 600) as i32);
                assert!(x >= l && x < r && y >= t && y < b, "pixel ({x}, {y}) hors de {l},{t} → {r},{b}");
            }
        }
        assert!(peints > 200);
        assert_eq!(c.echelle, 2.0);
    }

    #[test]
    fn les_presets_fournis_gardent_leurs_trainees() {
        // la borne de `TOURS_TRAINEE` ne mord que sur les cas extrêmes : aucun preset fourni n'a sa
        // traînée raccourcie, ni sur le son d'essai, ni ensuite basse à fond
        for p in crate::prereglages::builtins() {
            let mut s = Swarm::new();
            s.rng = Rng::graine(7);
            for i in 0..960 {
                let t = i as f64 / 60.0;
                let mut au = son_essai(t % 8.0);
                if i >= 480 {
                    (au.bass, au.kicks) = (1.0, au.kicks + 100);
                }
                s.avance(t, 1.0 / 60.0, C, au, &p.cfg, 1.0);
                let vie = p.cfg.orbite.trainee.max(0.01) as f64 * s.mods[0].trainee as f64;
                let vie_sats = SAT_TRAIL * s.mods[1].trainee as f64 * if p.cfg.ressorts.actif { SAT_TRAIL_RESSORTS } else { 1.0 };
                assert!(s.vies.iter().all(|&v| v == vie), "{} : comètes raccourcies à {t:.2} s", p.name);
                assert!(s.sats.iter().chain(&s.fixes).all(|q| vie_utile(vie_sats, s.w_lisse[1] * q.mul) == vie_sats), "{} : satellites raccourcis à {t:.2} s", p.name);
            }
        }
    }

    #[test]
    fn fissure_bornee_quelle_que_soit_la_force() {
        // taille 6 et force 3 : 18 tailles, des rayons de ~800 px ; bornée à `FISSURE_PORTEE_MAX`
        assert_eq!(taille_fissure(1.4, 1.0, 1.0), 1.4);
        assert_eq!(taille_fissure(6.0, 3.0, 2.0), 2.0 * FISSURE_PORTEE_MAX / FISSURE_RAYON.1);
        assert_eq!(taille_fissure(f32::NAN, 1.0, 1.0), 0.0);
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        for scale in [1.0f32, 2.0] {
            for _ in 0..20 {
                let c = Crack::new(0.0, 0.0, 0.0, taille_fissure(6.0, 3.0, scale), &mut rng);
                assert!(c.ext <= (FISSURE_PORTEE_MAX + 40.0) * scale, "fissure de {} px à {scale}", c.ext);
            }
        }
    }

    #[test]
    fn grand_rayon_module_l_essaim_reste_entier() {
        // rayon 200 à 200 %, quatre anneaux, rayon modulé × 5, inertie à fond, décalage : sans
        // borne, l'anneau extérieur passait à ~5 000 px et toutes ses comètes étaient retirées
        let mut cfg = Config::default();
        (cfg.orbite.rayon, cfg.orbite.anneaux, cfg.orbite.inertie) = (200.0, 4, 1.0);
        cfg.essaim.fixes = 8;
        cfg.ressorts.actif = true;
        cfg.modulations.liste = vec![
            Modulation { source: Source::Basse, cible: Cible::Rayon, sur: Groupe::Tout, quantite: 1.0, ..Default::default() },
            Modulation { source: Source::Basse, cible: Cible::Decalage, sur: Groupe::Tout, quantite: 1.0, ..Default::default() },
        ];
        cfg.declencheurs.liste = vec![Declencheur { force: 3.0, ..Declencheur::new(Evenement::Kick, Action::Rebond) }];
        let mut s = Swarm::new();
        let dt = 1.0 / 60.0;
        let mut au = AudioView { bass: 1.0, ..Default::default() };
        for i in 0..240 {
            // quatre kicks par image : leurs rebonds ne s'additionnent pas sans fin
            au.kicks += 4;
            let cursor = (C.0 + 300.0 * (i as f32 * 0.05).sin(), C.1);
            s.update(i as f64 * dt as f64, dt, cursor, au, &cfg, 2.0);
            for tr in s.trails.iter().chain(s.fixes.iter().map(|q| &q.trail)) {
                let p = tr.back().expect("une tête a disparu");
                let d = (p.1 - cursor.0).abs().max((p.2 - cursor.1).abs());
                assert!(d <= MAX_EXTENT / 2.0, "tête à {d} px du pointeur");
            }
        }
        // l'essaim reste grand : borné, pas écrasé
        let c = s.center.unwrap();
        let loin = s.trails.iter().filter_map(|tr| tr.back()).map(|p| (p.1 - c.0).hypot(p.2 - c.1)).fold(0.0f32, f32::max);
        assert!(loin > 400.0, "{loin}");
        // cinq rebonds de force 3 dans la même image : l'élan d'un seul
        let d = Declencheur { force: 3.0, ..Declencheur::new(Evenement::Clic, Action::Rebond) };
        s.rebond = [(0.0, 0.0); 2];
        for _ in 0..5 {
            s.agir(&d, C, 0.0, &cfg, 1.0);
        }
        assert_eq!(s.rebond.map(|r| r.1), [REBOND_MAX; 2]);
    }

    #[test]
    fn etendue_bornee_garde_les_proportions() {
        // rien à borner : inchangé
        assert_eq!(borne_etendue(2.0, 0.5, 20.0, 1.0, 1000.0), (2.0, 0.5));
        // trop grand : ramené juste à `dispo`, rayon et décalage d'autant
        let (f, d) = borne_etendue(5.0, 1.0, 200.0, 2.65, 1000.0);
        assert!((d.abs() * 200.0 + 2.65 * 200.0 * f - 1000.0).abs() < 0.1);
        assert!((f / d - 5.0).abs() < 1e-4);
    }

    #[test]
    fn trainees_au_plus_cinq_tours_et_budget_commun() {
        // tout au maximum : 32 comètes, 64 satellites, 32 permanents, traînées de 15 s, vitesse 60,
        // pointeur qui file
        let mut cfg = Config::default();
        (cfg.orbite.nombre, cfg.orbite.anneaux, cfg.orbite.trainee, cfg.orbite.vitesse_base) = (8, 4, 3.0, 60.0);
        (cfg.essaim.maximum, cfg.essaim.fixes, cfg.essaim.duree) = (64, 32, 30.0);
        cfg.modulations.liste = vec![Modulation { source: Source::Basse, cible: Cible::Trainee, sur: Groupe::Tout, quantite: 1.0, ..Default::default() }];
        cfg.declencheurs.liste = vec![Declencheur { nombre: 8, nombre_max: 8, ..Declencheur::new(Evenement::Kick, Action::Satellites) }];
        let mut s = Swarm::new();
        let dt = 1.0 / 144.0;
        let mut au = AudioView { bass: 1.0, ..Default::default() };
        for i in 0..(144 * 8) {
            au.kicks += (i % 20 == 0) as u32;
            let a = i as f32 * dt * 6.0;
            s.update(i as f64 * dt as f64, dt, (900.0 + 600.0 * a.cos(), 600.0 + 400.0 * a.sin()), au, &cfg, 1.5);
        }
        assert_eq!((s.trails.len(), s.sats.len(), s.fixes.len()), (32, 64, 32));
        let total: usize = s.trails.iter().chain(s.sats.iter().chain(&s.fixes).map(|q| &q.trail)).map(|tr| tr.len()).sum();
        assert!(total <= POINTS_TRAINEES + 128, "{total} points");
        // une comète de l'anneau intérieur tourne à ~60 rad/s : sa traînée couvre cinq tours
        // (~0,52 s), pas 15 s
        let vie = s.vies[0];
        assert!(vie < 0.6, "vie utile {vie} s");
        let tr = &s.trails[0];
        assert!(tr.back().unwrap().0 - tr.front().unwrap().0 <= vie + 1e-6);
    }

    #[test]
    fn trajet_change_trainees_et_modulations_repartent_de_zero() {
        let mut cfg = Config::default();
        let mut s = Swarm::new();
        let dt = 1.0 / 60.0;
        let au = AudioView { bass: 0.8, ..Default::default() };
        let mut t = 0.0;
        let mut avancer = |s: &mut Swarm, cfg: &Config, n: usize| {
            for _ in 0..n {
                s.update(t, dt, C, au, cfg, 1.0);
                t += dt as f64;
            }
        };
        avancer(&mut s, &cfg, 30);
        let longueur = |s: &Swarm| s.trails.iter().map(|tr| tr.len()).max().unwrap();
        assert!(longueur(&s) > 5);
        // une teinte, une quantité changées : rien ne repart
        cfg.couleur.teinte = 200.0;
        cfg.modulations.liste[0].quantite *= 0.5;
        avancer(&mut s, &cfg, 1);
        assert!(longueur(&s) > 5);
        let lissee = s.valeurs[0];
        assert!(lissee != 0.0);
        // autre forme : les traînées repartent de zéro (il ne reste que les points de cette image)
        let duree = |s: &Swarm| s.trails.iter().map(|tr| tr.back().unwrap().0 - tr.front().unwrap().0).fold(0.0f64, f64::max);
        cfg.orbite.forme = Forme::Etoile;
        avancer(&mut s, &cfg, 1);
        assert!(duree(&s) <= dt as f64 + 1e-9, "{}", duree(&s));
        avancer(&mut s, &cfg, 30);
        // rayon glissé cran par cran : les traînées restent, les valeurs lissées aussi
        // (valeurs lissées marquées : remises à zéro, elles en seraient loin après un pas)
        s.valeurs.iter_mut().for_each(|v| *v = 123.0);
        for i in 0..10 {
            cfg.orbite.rayon += 1.0;
            avancer(&mut s, &cfg, 1);
            assert!(longueur(&s) > 5, "{}", longueur(&s));
            if i == 0 {
                // celles qui se lissent assez pour qu'on voie d'où elles partent
                let lentes = cfg.modulations.liste.iter().zip(&s.valeurs).filter(|(m, _)| m.lissage > 0.2);
                assert!(lentes.clone().count() > 0 && lentes.clone().all(|(_, &v)| v > 50.0), "{:?}", s.valeurs);
            }
        }
        // une modulation de teinte ajoutée : les traînées restent, les valeurs lissées repartent
        // de zéro (la liste a changé)
        cfg.modulations.liste.push(Modulation { source: Source::Basse, cible: Cible::Teinte, sur: Groupe::Tout, quantite: 1.0, ..Default::default() });
        s.valeurs.iter_mut().for_each(|v| *v = 123.0);
        avancer(&mut s, &cfg, 1);
        assert!(longueur(&s) > 5);
        assert!(s.valeurs.iter().all(|&v| v != 123.0));
        // une modulation retirée en tête : les valeurs lissées repartent de zéro (la suivante
        // n'hérite pas de la valeur de celle d'avant)
        avancer(&mut s, &cfg, 30);
        cfg.modulations.liste.remove(0);
        s.valeurs.iter_mut().for_each(|v| *v = 123.0);
        avancer(&mut s, &cfg, 1);
        assert!(s.valeurs.iter().all(|&v| v != 123.0));
    }

    #[test]
    fn forme_des_satellites_changee_seules_leurs_trainees_repartent() {
        let mut cfg = Config::default();
        cfg.essaim.fixes = 4;
        let mut s = Swarm::new();
        let dt = 1.0 / 60.0;
        for i in 0..30 {
            s.update(i as f64 * dt as f64, dt, C, AudioView::default(), &cfg, 1.0);
        }
        cfg.essaim.forme = crate::config::FormeSatellites::Carre;
        assert_ne!(cfg.essaim.forme.suivie(cfg.orbite.forme), cfg.orbite.forme);
        s.update(30.0 * dt as f64, dt, C, AudioView::default(), &cfg, 1.0);
        assert!(s.trails.iter().all(|tr| tr.len() > 3), "comètes vidées");
        assert!(s.fixes.iter().all(|q| q.trail.back().unwrap().0 - q.trail.front().unwrap().0 <= dt as f64 + 1e-9), "satellites gardés");
    }

    #[test]
    fn part_des_trainees_stable_pendant_une_salve() {
        // 8 comètes, 64 satellites au plus, 8 permanents : la part ne dépend pas de ceux qui vivent
        assert_eq!(part_trainee(8, 64, 8), POINTS_TRAINEES / 80);
        assert_eq!(part_trainee(0, 0, 0), TRAIL_POINTS_MAX);
        let mut cfg = Config::default();
        (cfg.orbite.nombre, cfg.essaim.maximum) = (2, 64);
        let mut s = Swarm::new();
        let dt = 1.0 / 60.0;
        s.update(0.0, dt, C, AudioView::default(), &cfg, 1.0);
        let avant = part_trainee(s.trails.len(), cfg.essaim.maximum.max(s.sats.len()), s.fixes.len());
        s.ajouter_satellites(40, 0.0, &cfg);
        let apres = part_trainee(s.trails.len(), cfg.essaim.maximum.max(s.sats.len()), s.fixes.len());
        assert_eq!(avant, apres);
    }
}

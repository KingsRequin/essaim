//! Petites figures autour du pointeur, faites de lignes fines et de points qui laissent voir
//! l'écran : arcs (`[arcs]`), quatre temps (`[quatre_temps]`) et anneau suiveur (`[anneau]`) ici ;
//! orbe, ligne de voix, trame, trait de pinceau et pendule dans `figures/plus.rs`, gouttes en
//! contour dans `figures/gouttes.rs`.
//! `Effets` les possède, leur passe le son et ajoute leur zone à celle du pointeur.
//!
//! Ce sont des alternatives à l'essaim : chacune a son allure (`config::Allure` : échelle,
//! opacité, couleur, rotation, place) et ses modulations (`modulation::Effets::figures`, dont
//! les cibles propres : ouverture des arcs, perles des gouttes…). Ses réactions aux coups du son
//! sont des gestes (`Gestes`) que lui envoient les déclencheurs (`battement`, `accent`,
//! `scintillement`, `apogee`) ; la basse et la montée, elle les entend elle-même (`Partage`).
//! Elle se dessine à travers sa pose (`Pose`, `Posee`).
//!
//! Tout ce qui bouge passe par des ressorts bornés (`Ressort::vers`) : la zone de `rect` contient
//! donc toujours le dessin. `ui/apercu.js` reproduit ces figures (mêmes formules) : les modifier
//! ici oblige à les reporter là.

use crate::audio::AudioView;
use crate::canvas::{Canvas, Rgb};
use crate::config::{Allure, Config, CouleurFigure};
use crate::layer::IRect;
use crate::modulation::{self, Effet, NB_FIGURES};
use crate::render::{prisme_decale, ressort, Coups};
use std::cell::RefCell;
use std::f32::consts::{PI, SQRT_2, TAU};

mod gouttes;
mod plus;

/// Ce que les figures écoutent pendant une image : le son, ses coups depuis l'image précédente
/// (pour le tempo et le compte des temps) et les bandes de fréquences lissées (vide si personne
/// ne les demande).
pub struct Son<'a> {
    pub au: &'a AudioView,
    pub coups: &'a Coups,
    pub bandes: &'a [f32],
}

/// Gestes : battement, accent, scintillement, apogée (rang dans `config::GESTES`).
const BATTEMENT: usize = 0;
const ACCENT: usize = 1;
const SCINTILLEMENT: usize = 2;
const APOGEE: usize = 3;
/// Gestes d'une même sorte reçus en une image au plus (comme les coups, `render::nouveaux`).
const GESTES_MAX: u32 = 4;

/// Gestes reçus par une figure depuis l'image précédente (déclencheurs `battement`, `accent`,
/// `scintillement`, `apogee`) : combien de chaque, et leur force cumulée (1 : le geste de
/// toujours, 2 : le double).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gestes {
    n: [u32; 4],
    force: [f32; 4],
}

impl Gestes {
    /// Un geste `g` de force `force` de plus (au plus `GESTES_MAX` par image).
    pub fn ajouter(&mut self, g: usize, force: f32) {
        if let (Some(n), Some(f)) = (self.n.get_mut(g), self.force.get_mut(g)) {
            if *n < GESTES_MAX && force.is_finite() {
                *n += 1;
                *f += force.clamp(0.0, 2.0);
            }
        }
    }

    /// Nombre de gestes `g` reçus.
    fn n(&self, g: usize) -> u32 {
        self.n[g]
    }

    /// Force cumulée des gestes `g` (plusieurs battements dans l'image s'ajoutent).
    fn total(&self, g: usize) -> f32 {
        self.force[g]
    }

    /// Force d'un geste `g` (moyenne de ceux reçus), 0 sans geste.
    fn fort(&self, g: usize) -> f32 {
        if self.n[g] > 0 { self.force[g] / self.n[g] as f32 } else { 0.0 }
    }
}

/// Retombée (par seconde) des enveloppes du battement, de l'accent et du scintillement.
const RETOMBEE_GESTES: [f32; 3] = [7.0, 8.0, 10.0];

/// Ce qu'une figure entend pendant une image : son centre, l'horloge des rotations lentes, le
/// niveau de la basse et la montée (communs), et l'enveloppe de ses propres battements, accents et
/// scintillements (1 au geste, puis retombée).
#[derive(Clone, Copy)]
struct Partage {
    centre: (f32, f32),
    horloge: f32,
    basse: f32,
    montee: f32,
    battement: f32,
    accent: f32,
    scintillement: f32,
}

/// Couleur d'une figure d'allure `a`, de sa queue (`k` = 0) à sa tête (1) : celle de l'essaim
/// (`prisme_decale`, sa teinte plus `teinte` degrés, à la luminosité de l'essaim), une couleur
/// fixe (teinte, saturation et luminosité à elle, la queue `degrade` degrés plus loin) ou du
/// blanc ; `decale` : degrés ajoutés par les modulations de la teinte.
pub(crate) fn couleur_figure(cfg: &Config, a: &Allure, t: f64, k: f32, decale: f32) -> Rgb {
    let decale = if decale.is_finite() { decale } else { 0.0 };
    match a.couleur {
        CouleurFigure::Essaim => prisme_decale(cfg, t, k, a.teinte + decale),
        CouleurFigure::Fixe => {
            let (s, l) = (if a.saturation.is_finite() { a.saturation.clamp(0.0, 1.0) } else { 1.0 }, if a.luminosite.is_finite() { a.luminosite.clamp(0.0, 1.0) } else { crate::config::LUMINOSITE });
            crate::render::hsl((a.teinte_fixe + decale + a.degrade * (1.0 - k.clamp(0.0, 1.0))).rem_euclid(360.0), s, l)
        }
        CouleurFigure::Blanc => (255, 255, 255),
    }
}

/// Numéros des figures, dans l'ordre de `config::FIGURES`.
const ARCS: usize = 0;
const QUATRE_TEMPS: usize = 1;
const ANNEAU: usize = 2;
const ORBE: usize = 3;
const VOIX: usize = 4;
const TRAME: usize = 5;
const TRAIT: usize = 6;
const PENDULE: usize = 7;
const GOUTTES: usize = 8;
/// Ordre du dessin (les premières dessous).
const ORDRE: [usize; NB_FIGURES] = [GOUTTES, TRAME, ORBE, VOIX, TRAIT, PENDULE, ARCS, QUATRE_TEMPS, ANNEAU];

/// Le trait suit le trajet du pointeur et le pendule y est accroché : ils ne tournent, ne
/// grandissent ni ne se déplacent (seuls leur épaisseur, leur opacité et leur teinte changent).
fn geometrique(i: usize) -> bool {
    i != TRAIT && i != PENDULE
}

/// La figure peut-elle tourner autour du pointeur ? Les quatre temps et la ligne de voix sont
/// posés sous la flèche (tournés, ils passeraient dessus), la trame est fixe à l'écran : seuls
/// les arcs, l'anneau, l'orbe et les gouttes tournent.
fn tourne(i: usize) -> bool {
    matches!(i, ARCS | ANNEAU | ORBE | GOUTTES)
}

/// Part de la vitesse des modulations prise par la rotation d'une figure (à fond : 40 rad/s pour
/// l'essaim, 4 pour une figure) ; décalage d'une figure par « rayon » de décalage (px à 100 %).
const FIG_VITESSE: f32 = 0.1;
const FIG_DECALAGE: f32 = 30.0;
/// Échelle d'une figure, modulations comprises, jamais sous celle que permet son allure seule
/// (`echelle` 0,5) : plus petite, elle se ramasserait sur le pointeur ; ni au-delà de 2,5 (allure
/// à 2 et rayon modulé jusqu'à × 5 la portaient à 10 : la figure couvrait l'écran).
const GEO_MIN: f32 = 0.5;
const GEO_MAX: f32 = 2.5;
/// Trait plus fin que ça (px) : rien n'est dessiné (tiny-skia trace 1 px pour une largeur nulle).
const TRAIT_MIN: f32 = 0.05;

/// Pose d'une figure à cette image : autour de `centre`, tournée de `angle`, agrandie de `geo`
/// (positions et rayons), traits épaissis de `traits`, déplacée de `decalage` (px écran), opacité
/// multipliée par `alpha` (peut dépasser 1 : une lueur ravive ce qui est pâle).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pose {
    centre: (f32, f32),
    angle: f32,
    geo: f32,
    traits: f32,
    decalage: (f32, f32),
    alpha: f32,
}

impl Default for Pose {
    fn default() -> Self {
        Self { centre: (0.0, 0.0), angle: 0.0, geo: 1.0, traits: 1.0, decalage: (0.0, 0.0), alpha: 1.0 }
    }
}

impl Pose {
    /// La pose ne change rien : la figure est dessinée directement (au bit près comme avant).
    fn neutre(&self) -> bool {
        self.angle == 0.0 && self.geo == 1.0 && self.traits == 1.0 && self.decalage == (0.0, 0.0) && self.alpha == 1.0
    }

    fn point(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let (c, (sin, cos)) = (self.centre, self.angle.sin_cos());
        let (dx, dy) = ((x - c.0) * self.geo, (y - c.1) * self.geo);
        (c.0 + dx * cos - dy * sin + self.decalage.0, c.1 + dx * sin + dy * cos + self.decalage.1)
    }

    /// Zone d'une figure contenue dans le carré de demi-côté `e` px autour de `centre`, une fois
    /// posée ; `epaisseur` : son trait le plus large (px), qu'un trait épaissi de `traits` élargit
    /// (le reste de la figure, lui, ne grandit pas avec les traits).
    fn autour(&self, centre: (f32, f32), e: f32, epaisseur: f32) -> IRect {
        let tourne = if self.angle != 0.0 { SQRT_2 } else { 1.0 };
        let demi = e * self.geo + epaisseur * (self.traits - 1.0).max(0.0);
        autour((centre.0 + self.decalage.0, centre.1 + self.decalage.1), demi * tourne)
    }
}

/// Dessin à travers la pose d'une figure.
struct Posee<'a> {
    cv: &'a mut dyn Canvas,
    pose: Pose,
    pts: &'a mut Vec<(f32, f32)>,
}

impl Posee<'_> {
    fn alpha(&self, a: f32) -> f32 {
        (a * self.pose.alpha).min(1.0)
    }

    /// Largeur d'un trait une fois posé ; `None` s'il est trop fin pour être vu.
    fn largeur(&self, width: f32) -> Option<f32> {
        let w = width * self.pose.traits;
        (w >= TRAIT_MIN).then_some(w)
    }
}

impl Canvas for Posee<'_> {
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32, smooth: bool) {
        let (Some(w), al) = (self.largeur(width), self.alpha(alpha)) else { return };
        self.cv.line(self.pose.point(a), self.pose.point(b), w, rgb, al, smooth);
    }

    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, rgb: Rgb, alpha: f32, smooth: bool) {
        let (Some(w), al) = (self.largeur(width), self.alpha(alpha)) else { return };
        self.pts.clear();
        self.pts.extend(pts.iter().map(|&p| self.pose.point(p)));
        self.cv.polyline(self.pts, w, rgb, al, smooth);
    }

    fn circle(&mut self, x: f32, y: f32, r: f32, rgb: Rgb, alpha: f32) {
        let ((x, y), al) = (self.pose.point((x, y)), self.alpha(alpha));
        self.cv.circle(x, y, r * self.pose.geo, rgb, al);
    }

    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, rgb: Rgb, alpha: f32) {
        let (Some(w), al) = (self.largeur(width), self.alpha(alpha)) else { return };
        let (x, y) = self.pose.point((x, y));
        self.cv.ring(x, y, r * self.pose.geo, w, rgb, al);
    }

    fn bande(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32) {
        let (Some(w), al) = (self.largeur(width), self.alpha(alpha)) else { return };
        self.cv.bande(self.pose.point(a), self.pose.point(b), w, rgb, al);
    }

    fn voile(&mut self, zone: (f32, f32, f32, f32), x: f32, y: f32, r: f32, flou: f32, rgb: Rgb, alpha: f32) {
        let al = self.alpha(alpha);
        self.cv.voile(zone, x, y, r, flou, rgb, al);
    }

    fn vignette(&mut self, img: &tiny_skia::Pixmap, id: u64, x: f32, y: f32, alpha: f32) {
        let al = self.alpha(alpha);
        self.cv.vignette(img, id, x, y, al);
    }
}

/// Centre des figures : au bout d'un ressort accroché au pointeur (pulsation en rad/s,
/// amortissement), jamais à plus de `RETARD_MAX` px (à 100 %) de lui.
const SUIVI: (f32, f32) = (14.0, 0.55);
const RETARD_MAX: f32 = 40.0;
/// Suivi de la montée (par seconde).
const RETOMBEE_MONTEE: f32 = 6.0;
/// Présence du tempo : durée de son arrivée et de son effacement (s).
const TEMPO_FONDU: (f32, f32) = (0.15, 0.8);
/// Place des figures posées sous le pointeur (quatre temps, voix), depuis la pointe de la flèche
/// (px à 100 %, pointeur de taille normale).
const SOUS_LE_POINTEUR: (f32, f32) = (7.0, 27.0);

/// `SOUS_LE_POINTEUR`, écarté d'autant que Windows agrandit le pointeur (Accessibilité :
/// `souris::echelle_pointeur`) : la figure reste sous la flèche, pas dessus.
pub(crate) fn sous_le_pointeur() -> (f32, f32) {
    let k = crate::souris::echelle_pointeur();
    (SOUS_LE_POINTEUR.0 * k, SOUS_LE_POINTEUR.1 * k)
}

/// Arcs : cran dont le grand arc avance à chaque kick (rad), élan que le kick donne à son rayon
/// (px/s), recul du petit arc sur la caisse claire (rad/s), bornes du bond (px).
const ARC_CRAN: f32 = 0.6;
const ARC_BOND: f32 = 90.0;
const ARC_RECUL: f32 = 13.0;
const ARC_BORNES: (f32, f32) = (-4.0, 8.0);
/// Rayon et épaisseur du petit arc, par rapport au grand.
const ARC_PETIT: (f32, f32) = (0.675, 0.76);
/// Ouverture des arcs (rad) : au repos, puis ce que le niveau y ajoute à fond.
const ARC_GRAND_OUVERTURE: (f32, f32) = (0.14, 5.5);
const ARC_PETIT_OUVERTURE: (f32, f32) = (0.12, 4.8);

/// Quatre temps : bornes du saut (px, vers le haut en négatif), de la taille et du tiret d'un point.
const SAUT_BORNES: (f32, f32) = (-12.0, 6.0);
const TAILLE_BORNES: (f32, f32) = (0.3, 1.8);
const TIRET_BORNES: (f32, f32) = (-1.0, 5.0);
/// Grossissement du point du temps en cours, opacité des autres, écart entre deux sauts de la
/// vague du drop (s).
const POINT_ACTIF: f32 = 1.45;
const POINT_ETEINT: f32 = 0.4;
const VAGUE: f64 = 0.06;
/// Écart des quatre temps (et orbite des perles des gouttes) modulé, au plus (`ecart_module`) ;
/// grossissement des points des quatre temps par la modulation de la taille, au plus.
const ECART_MAX: f32 = 2.0;
const TAILLE_POINTS_MAX: f32 = 2.5;

/// Anneau : bornes de son échelle, durée d'un écho (s) et ce dont il s'élargit, échos gardés au
/// plus, vitesse qui donne l'étirement le plus grand (px/s à 100 % ; `[anneau] etirement`). Il
/// s'étire aussi avec son retard sur le pointeur (`Figures::etirement`) : au retard le plus
/// grand, l'étirement le plus grand.
const ANNEAU_BORNES: (f32, f32) = (0.2, 1.7);
const ANNEAU_ECHO: (f64, f32) = (0.42, 1.2);
const ANNEAU_ECHOS_MAX: usize = 6;
const ANNEAU_VITESSE: f32 = 900.0;
/// Étirement le plus grand permis par les réglages (`config::limites_connues`) : la zone en tient
/// compte.
const ETIREMENT_MAX: f32 = 0.7;

/// Cloche de Gauss, 1 en 0.
pub(crate) fn gauss(x: f32) -> f32 {
    (-x * x).exp()
}

/// Couleur entre `a` (k = 0) et `b` (k = 1).
pub(crate) fn melange(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k.clamp(0.0, 1.0)).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// Ressort amorti dont la position reste dans des bornes (la zone dessinée en dépend).
#[derive(Clone, Copy, Default)]
pub(crate) struct Ressort {
    pub(crate) x: f32,
    pub(crate) v: f32,
}

impl Ressort {
    pub(crate) fn en(x: f32) -> Self {
        Self { x, v: 0.0 }
    }

    /// Avance de `dt` vers `cible` (pulsation `w`, amortissement `z`) ; en butée, l'élan est perdu.
    pub(crate) fn vers(&mut self, cible: f32, (w, z): (f32, f32), dt: f32, (lo, hi): (f32, f32)) {
        let mut ecart = self.x - cible;
        ressort(&mut ecart, &mut self.v, w, z, dt);
        let x = cible + ecart;
        self.x = x.clamp(lo, hi);
        if self.x != x {
            self.v = 0.0;
        }
    }
}

#[derive(Default)]
struct Arcs {
    /// Angle du grand arc (rejoint `cran` avec de l'élan), bond de son rayon, recul du petit arc.
    tour: Ressort,
    cran: f32,
    bond: Ressort,
    recul: Ressort,
    /// Niveaux affichés (0 à 1).
    grave: f32,
    aigu: f32,
}

#[derive(Clone, Copy)]
struct Point {
    saut: Ressort,
    taille: Ressort,
    tiret: Ressort,
    /// Attend son tour dans la vague du drop.
    arme: bool,
}

struct QuatreTemps {
    pts: [Point; 4],
    actif: Option<usize>,
    /// Début et force de la vague du drop.
    vague: f64,
    force: f32,
}

impl QuatreTemps {
    fn new() -> Self {
        let point = Point { saut: Ressort::default(), taille: Ressort::en(1.0), tiret: Ressort::default(), arme: false };
        Self { pts: [point; 4], actif: None, vague: f64::NEG_INFINITY, force: 1.0 }
    }
}

struct Anneau {
    echelle: Ressort,
    echos: Vec<f64>,
}

impl Anneau {
    fn new() -> Self {
        Self { echelle: Ressort::en(1.0), echos: Vec::new() }
    }
}

pub struct Figures {
    /// Centre des figures et sa vitesse (px, px/s) ; `None` tant qu'aucune figure n'est active.
    centre: Option<(f32, f32)>,
    vitesse: (f32, f32),
    /// Écart du centre au pointeur (px), et retard au plus (px) à cette image.
    retard: (f32, f32),
    retard_max: f32,
    /// Horloge des rotations lentes (s, repliée).
    horloge: f32,
    /// Gestes reçus pour la prochaine image (déclencheurs, `recevoir`), et enveloppes du
    /// battement, de l'accent et du scintillement de chaque figure (1 au geste, puis retombée).
    gestes: [Gestes; NB_FIGURES],
    enveloppes: [[f32; 3]; NB_FIGURES],
    /// Montée suivie.
    montee: f32,
    /// Niveau de la basse, lissé (0 à 1).
    basse: f32,
    /// Temps comptés depuis le dernier drop, place dans la mesure de quatre temps (0 à 1) et
    /// présence du tempo (0 à 1).
    compte: u32,
    mesure: f32,
    tempo: f32,
    arcs: Arcs,
    temps: QuatreTemps,
    anneau: Anneau,
    /// Orbe, ligne de voix, trame, trait de pinceau et pendule.
    plus: plus::Plus,
    gouttes: gouttes::Gouttes,
    /// Effets des modulations sur chaque figure (donnés avant chaque image), angle de chacune,
    /// sa pose et sa teinte (degrés ajoutés) à cette image.
    mods: [Effet; NB_FIGURES],
    angles: [f32; NB_FIGURES],
    poses: [Pose; NB_FIGURES],
    teintes: [f32; NB_FIGURES],
    /// Figures allumées à l'image précédente : une figure qui s'allume repart de son état de départ.
    allumees: [bool; NB_FIGURES],
    /// Points d'une ligne à tracer, gardés d'une image à l'autre (le dessin ne prend que `&self`),
    /// et ceux de cette ligne une fois posée.
    tampon: RefCell<Vec<(f32, f32)>>,
    tampon_pose: RefCell<Vec<(f32, f32)>>,
}

impl Figures {
    pub fn new() -> Self {
        Self {
            centre: None,
            vitesse: (0.0, 0.0),
            retard: (0.0, 0.0),
            retard_max: RETARD_MAX,
            horloge: 0.0,
            gestes: Default::default(),
            enveloppes: [[0.0; 3]; NB_FIGURES],
            montee: 0.0,
            basse: 0.0,
            compte: 0,
            mesure: 0.0,
            tempo: 0.0,
            arcs: Arcs::default(),
            temps: QuatreTemps::new(),
            anneau: Anneau::new(),
            plus: plus::Plus::new(),
            gouttes: gouttes::Gouttes::new(),
            mods: [Effet::default(); NB_FIGURES],
            angles: [0.0; NB_FIGURES],
            poses: [Pose::default(); NB_FIGURES],
            teintes: [0.0; NB_FIGURES],
            allumees: [false; NB_FIGURES],
            tampon: Default::default(),
            tampon_pose: Default::default(),
        }
    }

    /// Effets des modulations sur chaque figure, pour les images qui suivent.
    pub fn moduler(&mut self, mods: &[Effet; NB_FIGURES]) {
        self.mods = *mods;
    }

    /// Rien n'est simulé à cette image : les gestes reçus en attendant sont jetés (pas de salve à
    /// la reprise).
    pub fn oublier_gestes(&mut self) {
        self.gestes = Default::default();
    }

    /// Gestes des déclencheurs (`render::Swarm::prendre_gestes`), faits à l'image suivante ;
    /// ceux d'avant qui n'ont pas encore été faits s'y ajoutent.
    pub fn recevoir(&mut self, gestes: &[Gestes; NB_FIGURES]) {
        for (a, b) in self.gestes.iter_mut().zip(gestes) {
            for g in 0..4 {
                for _ in 0..b.n[g] {
                    a.ajouter(g, b.fort(g));
                }
            }
        }
    }

    /// Une figure est-elle active ?
    pub fn actives(cfg: &Config) -> bool {
        cfg.arcs.actif || cfg.quatre_temps.actif || cfg.anneau.actif || cfg.gouttes.actif || plus::Plus::actives(cfg)
    }

    /// Une figure active a-t-elle besoin des bandes de fréquences ?
    pub fn veut_les_bandes(cfg: &Config) -> bool {
        cfg.orbe.actif
    }

    /// Avance d'une image : le centre suit le pointeur, les gestes reçus (`recevoir`) poussent les ressorts.
    pub fn avance(&mut self, t: f64, dt: f32, cursor: (f32, f32), scale: f32, son: &Son, cfg: &Config) {
        let (au, coups) = (son.au, son.coups);
        if !Self::actives(cfg) {
            // toutes éteintes : au rallumage, le centre repart du pointeur, sans élan ni retard
            (self.centre, self.vitesse, self.retard) = (None, (0.0, 0.0), (0.0, 0.0));
            self.allumees = [false; NB_FIGURES];
            self.plus.oublier();
            self.gouttes.oublier();
            return;
        }
        // une figure qui s'allume repart de son état de départ (rien de ce qu'elle faisait
        // avant d'être éteinte ne revient)
        for i in 0..NB_FIGURES {
            let allumee = cfg.figure(i).0;
            if allumee && !self.allumees[i] {
                self.reinitialiser(i);
            }
            self.allumees[i] = allumee;
        }
        // centre : au bout d'un ressort accroché au pointeur, retard borné
        let avant = self.centre;
        let (cx, cy) = self.centre.unwrap_or(cursor);
        let (mut ex, mut ey) = (cx - cursor.0, cy - cursor.1);
        ressort(&mut ex, &mut self.vitesse.0, SUIVI.0, SUIVI.1, dt);
        ressort(&mut ey, &mut self.vitesse.1, SUIVI.0, SUIVI.1, dt);
        let (d, lim) = ((ex * ex + ey * ey).sqrt(), RETARD_MAX * scale);
        if d > lim {
            (ex, ey) = (ex / d * lim, ey / d * lim);
            self.vitesse = (0.0, 0.0);
        }
        self.centre = Some((cursor.0 + ex, cursor.1 + ey));
        (self.retard, self.retard_max) = ((ex, ey), lim);
        self.horloge = (self.horloge + dt).rem_euclid(TAU * 40.0);
        // ce que les figures écoutent : la basse et la montée (communes), leurs gestes
        let gestes = std::mem::take(&mut self.gestes);
        for (env, g) in self.enveloppes.iter_mut().zip(&gestes) {
            for (k, e) in env.iter_mut().enumerate() {
                let retombee = *e * (-dt * RETOMBEE_GESTES[k]).exp();
                *e = if g.n(k) > 0 { retombee.max(g.fort(k).min(1.0)) } else { retombee };
                if !e.is_finite() {
                    *e = 0.0;
                }
            }
        }
        self.montee += (au.tension.clamp(0.0, 1.0) - self.montee) * (dt * RETOMBEE_MONTEE).min(1.0);
        let basse = au.bass.clamp(0.0, 1.0);
        self.basse += (basse - self.basse) * (1.0 - (-dt * 10.0).exp());
        // tempo : les temps se comptent depuis le dernier drop ; sans tempo régulier, ce qui le
        // montre s'efface
        if coups.temps {
            self.compte = self.compte.wrapping_add(1);
        }
        if coups.drops > 0 {
            self.compte = 0;
        }
        self.tempo = if au.bpm.is_some() { (self.tempo + dt / TEMPO_FONDU.0).min(1.0) } else { (self.tempo - dt / TEMPO_FONDU.1).max(0.0) };
        if au.bpm.is_some() {
            self.mesure = ((self.compte % 4) as f32 + au.phase.clamp(0.0, 1.0)) / 4.0;
        }
        self.poser(dt, scale, cfg);
        let ecoutes: [(Partage, Gestes); NB_FIGURES] = std::array::from_fn(|i| (self.partage(i, cursor), gestes[i]));
        self.plus.avance(&ecoutes, &self.mods, &self.poses, t, dt, cursor, scale, son, cfg);
        // les gouttes traînent derrière le centre quand il bouge (px écran depuis l'image d'avant)
        let centre = self.centre.unwrap_or(cursor);
        let bouge = avant.map_or((0.0, 0.0), |a| (centre.0 - a.0, centre.1 - a.1));
        let (p, g) = &ecoutes[GOUTTES];
        self.gouttes.avance(p, g, &self.mods[GOUTTES], bouge, t, dt, scale, cfg);

        if cfg.arcs.actif {
            let ((p, g), a) = (&ecoutes[ARCS], &mut self.arcs);
            if g.n(BATTEMENT) > 0 {
                a.cran += ARC_CRAN * g.total(BATTEMENT);
                a.bond.v += ARC_BOND * g.total(BATTEMENT);
            }
            a.recul.v -= ARC_RECUL * g.fort(ACCENT);
            // l'angle reste petit : le cran et l'arc reculent ensemble d'un tour
            while a.cran > TAU {
                a.cran -= TAU;
                a.tour.x -= TAU;
            }
            a.tour.vers(a.cran, (14.0, 0.4), dt, (-TAU, 2.0 * TAU));
            a.bond.vers(0.0, (18.0, 0.3), dt, ARC_BORNES);
            a.recul.vers(0.0, (12.0, 0.35), dt, (-PI, PI));
            let aigu = (0.9 * p.scintillement + 0.8 * p.montee + 0.45 * p.accent).min(1.0);
            a.grave = p.basse;
            a.aigu += (aigu - a.aigu) * (1.0 - (-dt * if aigu > a.aigu { 30.0 } else { 6.0 }).exp());
        }

        if cfg.quatre_temps.actif {
            let (g, q) = (&ecoutes[QUATRE_TEMPS].1, &mut self.temps);
            let actif = au.bpm.map(|_| (self.compte % 4) as usize);
            if actif != q.actif {
                q.actif = actif;
                if let Some(i) = actif {
                    q.pts[i].taille.v += 9.0;
                }
            }
            if let Some(p) = q.actif.map(|i| &mut q.pts[i]) {
                p.saut.v -= 150.0 * g.fort(BATTEMENT);
                p.tiret.v += 60.0 * g.fort(ACCENT);
            }
            if g.n(APOGEE) > 0 {
                (q.vague, q.force) = (t, g.fort(APOGEE));
                q.pts.iter_mut().for_each(|p| p.arme = true);
            }
            for (i, p) in q.pts.iter_mut().enumerate() {
                if p.arme && t >= q.vague + i as f64 * VAGUE {
                    p.saut.v -= 230.0 * q.force;
                    p.arme = false;
                }
                p.saut.vers(0.0, (20.0, 0.3), dt, SAUT_BORNES);
                p.taille.vers(1.0, (18.0, 0.35), dt, TAILLE_BORNES);
                p.tiret.vers(0.0, (16.0, 0.4), dt, TIRET_BORNES);
            }
        }

        if cfg.anneau.actif {
            let ((p, g), a) = (&ecoutes[ANNEAU], &mut self.anneau);
            if g.n(APOGEE) > 0 {
                a.echelle.v += 10.0 * g.fort(APOGEE);
                a.echos.push(t);
            }
            a.echelle.vers(1.0 - 0.42 * p.montee, (18.0, 0.28), dt, ANNEAU_BORNES);
            a.echos.retain(|&ne| t - ne < ANNEAU_ECHO.0);
            if a.echos.len() > ANNEAU_ECHOS_MAX {
                let trop = a.echos.len() - ANNEAU_ECHOS_MAX;
                a.echos.drain(..trop);
            }
        }
    }

    /// État propre de la figure `i` remis à celui de départ (elle vient de s'allumer).
    fn reinitialiser(&mut self, i: usize) {
        self.angles[i] = 0.0;
        self.enveloppes[i] = [0.0; 3];
        match i {
            ARCS => self.arcs = Arcs::default(),
            QUATRE_TEMPS => self.temps = QuatreTemps::new(),
            ANNEAU => self.anneau = Anneau::new(),
            GOUTTES => self.gouttes.reinitialiser(),
            _ => self.plus.reinitialiser(i),
        }
    }

    /// Ce qu'entend la figure `i` à cette image (son centre : `cursor` avant la première).
    fn partage(&self, i: usize, cursor: (f32, f32)) -> Partage {
        let [battement, accent, scintillement] = self.enveloppes[i];
        Partage { centre: self.centre.unwrap_or(cursor), horloge: self.horloge, basse: self.basse, montee: self.montee, battement, accent, scintillement }
    }

    /// Pose de chaque figure à cette image : son allure (`[…] echelle`, `rotation`…) et ses
    /// modulations. Une figure qui tourne (`tourne`) tourne de sa rotation propre, plus celle des
    /// modulations, le tout multiplié par leur accélération. Pour la trame, l'échelle agrandit la
    /// loupe et la taille des traits ses points (`dessiner` : elle ne passe pas par `Posee`).
    fn poser(&mut self, dt: f32, scale: f32, cfg: &Config) {
        let centre = self.centre.unwrap_or_default();
        for i in 0..NB_FIGURES {
            let (actif, a) = cfg.figure(i);
            let m = self.mods[i];
            if !actif {
                (self.angles[i], self.poses[i], self.teintes[i]) = (0.0, Pose::default(), 0.0);
                continue;
            }
            let geo = geometrique(i);
            if tourne(i) {
                // rotation totale bornée comme celle du trajet de l'essaim
                let w = ((a.rotation + m.rotation) * TAU / 60.0 + m.vitesse * FIG_VITESSE) * m.acceleration;
                let w = modulation::borne_rotation(w * 60.0 / TAU) * TAU / 60.0;
                if w != 0.0 {
                    self.angles[i] = (self.angles[i] + w * dt).rem_euclid(TAU);
                }
            } else {
                self.angles[i] = 0.0;
            }
            self.poses[i] = Pose {
                centre,
                angle: self.angles[i],
                geo: if geo { (a.echelle * m.rayon).clamp(GEO_MIN, GEO_MAX) } else { 1.0 },
                traits: m.taille,
                decalage: if geo { ((a.decalage_x + m.decalage * FIG_DECALAGE) * scale, a.decalage_y * scale) } else { (0.0, 0.0) },
                alpha: a.opacite * m.opacite * m.luminosite,
            };
            self.teintes[i] = m.teinte;
        }
    }

    /// Étirement de l'anneau ou de l'orbe à cette image (0 à `max`) et sa direction à l'écran
    /// (cosinus, sinus) : le plus grand de celui de leur vitesse et de celui de leur retard sur
    /// le pointeur. Le retard compte aussi : arrivé au retard le plus grand, le centre ne bouge
    /// plus que de la vitesse du pointeur, qui l'entraîne au bout de son ressort, et l'anneau
    /// resterait rond.
    fn etirement(&self, scale: f32, max: f32) -> (f32, f32, f32) {
        let max = max.clamp(0.0, ETIREMENT_MAX);
        let v = self.vitesse.0.hypot(self.vitesse.1);
        let d = self.retard.0.hypot(self.retard.1);
        let par_vitesse = (v / (ANNEAU_VITESSE * scale)).min(1.0) * max;
        let par_retard = (d / self.retard_max.max(1.0)).min(1.0) * max;
        if par_retard > par_vitesse && d > 0.5 {
            (par_retard, -self.retard.0 / d, -self.retard.1 / d)
        } else if v > 1.0 {
            (par_vitesse, self.vitesse.0 / v, self.vitesse.1 / v)
        } else {
            (0.0, 1.0, 0.0)
        }
    }

    /// `etirement` de la figure `i`, sa direction ramenée dans le repère de la figure : la pose la
    /// tourne de son angle, la direction est donc tournée d'autant dans l'autre sens, pour que
    /// l'anneau reste étiré vers le pointeur même quand il tourne.
    fn etirement_pose(&self, i: usize, scale: f32, max: f32) -> (f32, f32, f32) {
        let (e, cos, sin) = self.etirement(scale, max);
        let (s, c) = (-self.poses[i].angle).sin_cos();
        (e, cos * c - sin * s, cos * s + sin * c)
    }

    /// Demi-côté (px à 100 %) du carré autour du centre qui contient la figure `i` (sauf le trait,
    /// le pendule et la trame, qui ont leur propre zone).
    fn etendue(&self, i: usize, cfg: &Config) -> f32 {
        match i {
            ARCS => cfg.arcs.rayon + ARC_BORNES.1 + cfg.arcs.epaisseur,
            QUATRE_TEMPS => {
                let q = &cfg.quatre_temps;
                let r = q.taille * TAILLE_BORNES.1 * POINT_ACTIF;
                let sous = sous_le_pointeur();
                let large = sous.0 + 1.5 * q.ecart * ECART_MAX + TIRET_BORNES.1 * 1.1 + r + 1.0;
                let haut = sous.1 + SAUT_BORNES.1.max(-SAUT_BORNES.0) + r;
                large.max(haut)
            }
            ANNEAU => cfg.anneau.rayon * ANNEAU_BORNES.1 * (1.0 + ANNEAU_ECHO.1) + 4.0 * cfg.anneau.epaisseur + 4.0,
            GOUTTES => self.gouttes.etendue(cfg),
            _ => self.plus.etendue(i, cfg),
        }
    }

    /// Zone écran qui contient les figures (autour de leur centre, ou de `cursor` avant la
    /// première image) ; `None` si aucune n'est active.
    pub fn rect(&self, cursor: (f32, f32), scale: f32, cfg: &Config) -> Option<IRect> {
        let centre = self.centre.unwrap_or(cursor);
        let mut zone = None;
        for i in 0..NB_FIGURES {
            if !cfg.figure(i).0 {
                continue;
            }
            let pose = &self.poses[i];
            let r = match i {
                TRAIT => self.plus.rect_trait(scale * pose.traits.max(1.0), cfg),
                PENDULE => self.plus.rect_pendule(cursor, scale, pose.traits, cfg),
                TRAME => Some(autour((centre.0 + pose.decalage.0, centre.1 + pose.decalage.1), self.plus.etendue_trame(cfg, pose.traits) * scale)),
                _ => Some(pose.autour(centre, self.etendue(i, cfg) * scale, trait_max(i, cfg) * scale)),
            };
            zone = crate::layer::union(zone, r);
        }
        zone
    }

    /// Dessine les figures actives autour de leur centre, chacune dans sa pose et sa couleur.
    pub fn dessiner(&self, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32) {
        let Some(centre) = self.centre else { return };
        let (mut pts, mut pts_pose) = (self.tampon.borrow_mut(), self.tampon_pose.borrow_mut());
        for i in ORDRE {
            let (actif, a) = cfg.figure(i);
            if !actif {
                continue;
            }
            // deux couleurs : celle de la tête et celle de la queue (de l'essaim, fixes ou blanches)
            let teinte = self.teintes[i];
            let couleurs = (couleur_figure(cfg, a, t, 1.0, teinte), couleur_figure(cfg, a, t, 0.0, teinte));
            let p = self.partage(i, centre);
            // la trame applique elle-même sa pose (loupe, points, place) : elle reste fixe à l'écran
            let pose = if i == TRAME { Pose { centre, alpha: self.poses[i].alpha, ..Pose::default() } } else { self.poses[i] };
            if pose.neutre() {
                self.dessiner_figure(i, &p, cv, &mut pts, t, cfg, scale, couleurs, teinte);
            } else {
                let mut posee = Posee { cv: &mut *cv, pose, pts: &mut pts_pose };
                self.dessiner_figure(i, &p, &mut posee, &mut pts, t, cfg, scale, couleurs, teinte);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dessiner_figure(&self, i: usize, p: &Partage, cv: &mut dyn Canvas, pts: &mut Vec<(f32, f32)>, t: f64, cfg: &Config, scale: f32, (tete, queue): (Rgb, Rgb), teinte: f32) {
        let (cx, cy) = p.centre;
        let propres = &self.mods[i].propres;
        match i {
            GOUTTES => self.gouttes.dessiner(p, cv, cfg, scale, (tete, queue)),
            TRAME => self.plus.dessiner_trame(p, cv, t, cfg, scale, tete, &self.poses[TRAME]),
            ORBE => self.plus.dessiner_orbe(p, cv, pts, t, cfg, scale, (tete, queue), self.etirement_pose(ORBE, scale, cfg.orbe.etirement)),
            VOIX => self.plus.dessiner_voix(p, cv, pts, cfg, scale, (tete, queue), propres.relief),
            TRAIT => self.plus.dessiner_trait(p, cv, t, cfg, scale, teinte),
            PENDULE => self.plus.dessiner_pendule(p, cv, pts, cfg, scale, (tete, queue), self.poses[PENDULE].traits),
            ARCS => {
                let (a, c) = (&self.arcs, &cfg.arcs);
                let mut arc = |r: f32, depart: f32, ouverture: f32, largeur: f32, rgb| {
                    let r = r.max(2.0);
                    let n = ((ouverture * r / (3.0 * scale)).ceil() as usize).clamp(2, 96);
                    pts.clear();
                    pts.extend((0..=n).map(|i| {
                        let (sin, cos) = (depart + ouverture * i as f32 / n as f32).sin_cos();
                        (cx + cos * r, cy + sin * r)
                    }));
                    cv.polyline(pts, largeur, rgb, 1.0, true);
                };
                // la modulation de l'ouverture ouvre (ou ferme) les deux arcs
                let grand = ARC_GRAND_OUVERTURE.0 + ARC_GRAND_OUVERTURE.1 * (a.grave + propres.ouverture).clamp(0.0, 1.0);
                let petit = ARC_PETIT_OUVERTURE.0 + ARC_PETIT_OUVERTURE.1 * (a.aigu + propres.ouverture).clamp(0.0, 1.0);
                arc((c.rayon + a.bond.x) * scale, -PI / 2.0 + a.tour.x + self.horloge * 0.25, grand, c.epaisseur * scale, tete);
                arc((c.rayon * ARC_PETIT.0 + a.bond.x * 0.5) * scale, PI / 2.0 + a.recul.x - self.horloge * 0.4, petit, c.epaisseur * ARC_PETIT.1 * scale, queue);
            }
            QUATRE_TEMPS => {
                if self.tempo <= 0.01 {
                    return;
                }
                let (q, c) = (&self.temps, &cfg.quatre_temps);
                // la pose met le rayon d'un point à l'échelle `geo` et la largeur d'un trait à
                // l'échelle `traits` : le tiret, un point allongé, garde l'échelle du point ; la
                // modulation de la taille grossit les points
                let pose = &self.poses[QUATRE_TEMPS];
                let tiret = if pose.traits > 1e-3 { pose.geo / pose.traits } else { 0.0 };
                let sous = sous_le_pointeur();
                let (ax, ay) = (cx + sous.0 * scale, cy + sous.1 * scale);
                let ecart = c.ecart * ecart_module(propres.ecart) * scale * (1.0 - 0.35 * p.montee);
                for (i, pt) in q.pts.iter().enumerate() {
                    let actif = q.actif == Some(i);
                    let x = ax + (i as f32 - 1.5) * ecart + (self.horloge * 40.0 + i as f32 * 2.0).sin() * 0.8 * p.montee * scale;
                    let y = ay + pt.saut.x * scale;
                    let r = c.taille * scale * pt.taille.x * pose.traits.min(TAILLE_POINTS_MAX) * if actif { POINT_ACTIF } else { 1.0 };
                    let l = pt.tiret.x.max(0.0) * 1.1 * scale;
                    let al = self.tempo * if actif { 1.0 } else { POINT_ETEINT };
                    if l > 0.3 && tiret > 0.0 {
                        cv.line((x - l, y), (x + l, y), 2.0 * r * tiret, tete, al, true);
                    } else {
                        cv.circle(x, y, r, tete, al);
                    }
                }
            }
            _ => {
                let (a, c) = (&self.anneau, &cfg.anneau);
                let r = c.rayon * scale * a.echelle.x;
                for &ne in &a.echos {
                    let k = ((t - ne) / ANNEAU_ECHO.0) as f32;
                    if (0.0..1.0).contains(&k) {
                        cv.ring(cx, cy, r * (1.0 + ANNEAU_ECHO.1 * (1.0 - (1.0 - k).powi(3))), 1.2 * scale, queue, 0.7 * (1.0 - k));
                    }
                }
                // étiré dans le sens où il file ; carré un instant sur l'accent, ou par la
                // modulation de la forme (superellipse : un cercle à l'exposant 2, presque un
                // carré à 8)
                let (e, cos, sin) = self.etirement_pose(ANNEAU, scale, c.etirement);
                let carre = propres.forme.clamp(0.0, 1.0);
                let (ra, rb, puissance) = (r * (1.0 + e), r * (1.0 - e), 2.0 / (2.0 + 6.0 * carre));
                let point = |angle: f32| {
                    let (st, ct) = angle.sin_cos();
                    let (x, y) = (ra * ct.signum() * ct.abs().powf(puissance), rb * st.signum() * st.abs().powf(puissance));
                    (cx + x * cos - y * sin, cy + x * sin + y * cos)
                };
                pts.clear();
                pts.extend((0..=48).map(|i| point(i as f32 / 48.0 * TAU)));
                // le trait s'épaissit avec la basse et la montée (au plus trois fois l'épaisseur réglée)
                cv.polyline(pts, c.epaisseur * (1.0 + 1.07 * p.basse + 0.86 * p.montee) * scale, tete, 1.0, true);
                // le point du tempo : un tour par mesure de quatre temps, plus gros au scintillement
                if self.tempo > 0.01 {
                    let (x, y) = point(-PI / 2.0 + self.mesure * TAU);
                    cv.circle(x, y, (1.9 + 1.9 * p.scintillement) * scale, queue, self.tempo);
                }
            }
        }
    }
}

/// Facteur de l'écart des quatre temps (ou de l'orbite des perles des gouttes) pour une part
/// modulée `ecart` : de × 0,3 à × `ECART_MAX`.
pub(crate) fn ecart_module(ecart: f32) -> f32 {
    if ecart.is_finite() { (1.0 + ecart).clamp(0.3, ECART_MAX) } else { 1.0 }
}

/// Carré écran de demi-côté `e` px (plus 2 px de marge) autour de `centre`.
fn autour((cx, cy): (f32, f32), e: f32) -> IRect {
    let e = e + 2.0;
    ((cx - e).floor() as i32, (cy - e).floor() as i32, (cx + e).ceil() as i32, (cy + e).ceil() as i32)
}

/// Trait le plus large que la figure `i` épaissit avec ses `traits` (px à 100 %) : les arcs, le
/// contour de l'anneau (jusqu'à trois fois son épaisseur avec la basse et la montée), de l'orbe
/// (ou de ses échos), de la voix et des gouttes ; le point le plus gros des quatre temps (la
/// modulation de la taille les grossit, à l'échelle de la figure). La trame a sa propre zone.
fn trait_max(i: usize, cfg: &Config) -> f32 {
    match i {
        QUATRE_TEMPS => cfg.quatre_temps.taille * TAILLE_BORNES.1 * POINT_ACTIF * GEO_MAX,
        ARCS => cfg.arcs.epaisseur,
        ANNEAU => (3.0 * cfg.anneau.epaisseur).max(1.2),
        ORBE => cfg.orbe.epaisseur.max(1.7),
        VOIX => cfg.voix.epaisseur,
        GOUTTES => cfg.gouttes.epaisseur,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    /// Avance d'une image, sans bandes de fréquences.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn avancer(f: &mut Figures, t: f64, dt: f32, cursor: (f32, f32), scale: f32, au: &AudioView, coups: &Coups, cfg: &Config) {
        f.recevoir(&gestes_de(coups, 1.0));
        f.avance(t, dt, cursor, scale, &Son { au, coups, bandes: &[] }, cfg);
    }

    /// Gestes des réactions de toujours pour ces coups (un battement par kick, un accent à la
    /// caisse claire, un scintillement par attaque des aigus, une apogée au drop), de force
    /// `force`, pour toutes les figures.
    pub(super) fn gestes_de(c: &Coups, force: f32) -> [Gestes; NB_FIGURES] {
        let mut g = Gestes::default();
        for _ in 0..c.kicks {
            g.ajouter(BATTEMENT, force);
        }
        for _ in 0..c.aigus {
            g.ajouter(SCINTILLEMENT, force);
        }
        if c.caisses > 0 {
            g.ajouter(ACCENT, force);
        }
        if c.drops > 0 {
            g.ajouter(APOGEE, force);
        }
        [g; NB_FIGURES]
    }

    /// Son d'essai : 120 BPM, kick sur chaque temps, caisse claire un temps sur deux, charleston
    /// aux contretemps, basse forte ; drop à 2 s.
    fn son(t: f64) -> (AudioView, Coups) {
        let (k, avant) = ((t / 0.5).floor(), ((t - 1.0 / 120.0) / 0.5).floor());
        let temps = k != avant;
        let au = AudioView { bass: 0.9, bpm: Some(120.0), phase: ((t / 0.5) - k) as f32, tension: if (1.0..2.0).contains(&t) { 1.0 } else { 0.0 }, ..Default::default() };
        let demi = ((t + 0.25) / 0.5).floor() != ((t + 0.25 - 1.0 / 120.0) / 0.5).floor();
        let drop = (2.0..2.0 + 1.0 / 120.0).contains(&t);
        (au, Coups { kicks: temps as u32, caisses: (temps && k as i64 % 2 == 1) as u32, aigus: demi as u32, drops: drop as u32, temps, ..Default::default() })
    }

    fn toutes() -> Config {
        let mut cfg = Config::default();
        (cfg.arcs.actif, cfg.quatre_temps.actif, cfg.anneau.actif) = (true, true, true);
        (cfg.orbe.actif, cfg.voix.actif, cfg.trame.actif, cfg.pinceau.actif, cfg.pendule.actif) = (true, true, true, true, true);
        cfg.gouttes.actif = true;
        cfg
    }

    #[test]
    fn eteintes_par_defaut_aucune_zone_aucun_dessin() {
        let cfg = Config::default();
        let mut f = Figures::new();
        let (au, coups) = son(0.0);
        avancer(&mut f, 0.0, 1.0 / 60.0, (300.0, 300.0), 1.0, &au, &coups, &cfg);
        assert_eq!(f.rect((300.0, 300.0), 1.0, &cfg), None);
        let mut pm = Pixmap::new(600, 600).unwrap();
        f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, 0.0, &cfg, 1.0);
        assert!(pm.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn les_figures_tiennent_dans_leur_zone() {
        // réglages les plus grands, musique chargée, pointeur qui file : à chaque image, aucun
        // pixel peint hors de `rect`
        let mut cfg = toutes();
        (cfg.arcs.rayon, cfg.arcs.epaisseur) = (80.0, 6.0);
        (cfg.quatre_temps.taille, cfg.quatre_temps.ecart) = (5.0, 20.0);
        (cfg.anneau.rayon, cfg.anneau.epaisseur) = (60.0, 6.0);
        (cfg.orbe.rayon, cfg.orbe.relief, cfg.orbe.epaisseur) = (60.0, 30.0, 5.0);
        (cfg.voix.largeur, cfg.voix.hauteur, cfg.voix.epaisseur) = (120.0, 20.0, 5.0);
        (cfg.trame.rayon, cfg.trame.pas, cfg.trame.taille) = (120.0, 6.0, 3.0);
        (cfg.pinceau.largeur, cfg.pinceau.duree) = (12.0, 1.0);
        (cfg.pendule.longueur, cfg.pendule.taille) = (60.0, 6.0);
        tiennent_dans_leur_zone(&cfg, &[Effet::default(); NB_FIGURES]);
        // allure poussée au bout de ses bornes (sauf l'échelle, déjà au plus grand avec ces
        // réglages) et modulations à fond : la zone suit toujours
        for i in 0..NB_FIGURES {
            let a = allure_mut(&mut cfg, i);
            (a.echelle, a.rotation, a.decalage_x, a.decalage_y, a.opacite, a.teinte) = (1.4, -30.0, 80.0, -80.0, 0.5, 120.0);
        }
        (cfg.anneau.etirement, cfg.orbe.etirement, cfg.gouttes.perles, cfg.gouttes.souplesse, cfg.gouttes.vitesse) = (0.7, 0.7, 8, 1.0, 3.0);
        let propres = modulation::Propres { ouverture: 1.0, ecart: 1.0, forme: 1.0, relief: 1.0, duree: 1.0, nombre: 1.0, noyau: 1.0 };
        let fort = Effet { rotation: 90.0, vitesse: 40.0, decalage: 1.6, rayon: 1.5, taille: 3.0, luminosite: 3.0, propres, ..Effet::default() };
        tiennent_dans_leur_zone(&cfg, &[fort; NB_FIGURES]);
    }

    fn allure_mut(cfg: &mut Config, i: usize) -> &mut Allure {
        match i {
            ARCS => &mut cfg.arcs.allure,
            QUATRE_TEMPS => &mut cfg.quatre_temps.allure,
            ANNEAU => &mut cfg.anneau.allure,
            ORBE => &mut cfg.orbe.allure,
            VOIX => &mut cfg.voix.allure,
            TRAME => &mut cfg.trame.allure,
            TRAIT => &mut cfg.pinceau.allure,
            PENDULE => &mut cfg.pendule.allure,
            _ => &mut cfg.gouttes.allure,
        }
    }

    /// Musique chargée, pointeur qui file : à chaque image, aucun pixel peint hors de `rect`.
    fn tiennent_dans_leur_zone(cfg: &Config, mods: &[Effet; NB_FIGURES]) {
        let cfg = cfg.clone();
        for (scale, dt) in [(1.0f32, 1.0 / 120.0), (1.5, 1.0 / 20.0)] {
            let mut f = Figures::new();
            f.moduler(mods);
            let (mut t, mut peints) = (0.0f64, 0usize);
            for i in 0..(4.0 / dt) as usize {
                let cursor = (700.0 + 300.0 * (t as f32 * 5.0).sin(), 700.0 + 120.0 * (t as f32 * 3.0).cos());
                let (au, mut coups) = son(t);
                coups.kicks *= 4; // quatre kicks dans la même image : les ressorts restent bornés
                f.recevoir(&gestes_de(&coups, 2.0)); // gestes au plus fort
                f.avance(t, dt, cursor, scale, &Son { au: &au, coups: &coups, bandes: &[1.0; 16] }, &cfg);
                if i % 7 == 0 {
                    let (x0, y0, x1, y1) = f.rect(cursor, scale, &cfg).unwrap();
                    let mut pm = Pixmap::new(1600, 1600).unwrap();
                    f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, &cfg, scale);
                    for (j, p) in pm.data().as_chunks::<4>().0.iter().enumerate() {
                        if p[3] > 0 {
                            peints += 1;
                            let (x, y) = ((j % 1600) as i32, (j / 1600) as i32);
                            assert!(x >= x0 && x < x1 && y >= y0 && y < y1, "t = {t:.2} : pixel ({x}, {y}) hors de {x0},{y0} → {x1},{y1}");
                        }
                    }
                }
                t += dt as f64;
            }
            assert!(peints > 5000, "{peints} pixels peints");
        }
    }

    #[test]
    fn figure_reduite_bornee_et_trait_nul_invisible() {
        // échelle au plus petit et rayon modulé à 10 % : la figure garde la moitié de sa taille
        let mut cfg = Config::default();
        (cfg.arcs.actif, cfg.arcs.allure.echelle) = (true, 0.5);
        let mut f = Figures::new();
        f.moduler(&[Effet { rayon: 0.1, ..Effet::default() }; NB_FIGURES]);
        avancer(&mut f, 0.0, 1.0 / 60.0, (300.0, 300.0), 1.0, &AudioView::default(), &Coups::default(), &cfg);
        assert_eq!(f.poses[ARCS].geo, GEO_MIN);
        // à l'échelle 1 sans modulation, rien ne change
        cfg.arcs.allure.echelle = 1.0;
        f.moduler(&[Effet::default(); NB_FIGURES]);
        avancer(&mut f, 1.0 / 60.0, 1.0 / 60.0, (300.0, 300.0), 1.0, &AudioView::default(), &Coups::default(), &cfg);
        assert_eq!(f.poses[ARCS].geo, 1.0);
        // traits à épaisseur nulle : rien de dessiné (tiny-skia tracerait 1 px)
        let mut pm = Pixmap::new(100, 100).unwrap();
        let (mut pts, mut cv) = (Vec::new(), crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) });
        let mut posee = Posee { cv: &mut cv, pose: Pose { centre: (50.0, 50.0), traits: 0.0, ..Pose::default() }, pts: &mut pts };
        posee.line((10.0, 10.0), (90.0, 90.0), 2.0, (255, 255, 255), 1.0, true);
        posee.polyline(&[(10.0, 50.0), (90.0, 50.0)], 2.0, (255, 255, 255), 1.0, true);
        posee.ring(50.0, 50.0, 20.0, 2.0, (255, 255, 255), 1.0);
        posee.bande((10.0, 20.0), (90.0, 20.0), 2.0, (255, 255, 255), 1.0);
        assert!(pm.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn seuls_les_gestes_font_reagir_et_l_allure_se_voit() {
        let mut cfg = Config::default();
        cfg.arcs.actif = true;
        let mut f = Figures::new();
        let kick = Coups { kicks: 1, ..Default::default() };
        // le kick seul ne fait rien : seuls les gestes (déclencheurs) font bouger les arcs
        for i in 0..30 {
            f.avance(i as f64 / 60.0, 1.0 / 60.0, (300.0, 300.0), 1.0, &Son { au: &AudioView::default(), coups: &kick, bandes: &[] }, &cfg);
        }
        assert_eq!((f.arcs.cran, f.arcs.bond.x), (0.0, 0.0), "le kick fait encore bouger les arcs");
        // geste double : le bond part deux fois plus fort
        let un = |f: &mut Figures, force: f32| {
            f.recevoir(&gestes_de(&kick, force));
            f.avance(0.0, 1.0 / 60.0, (300.0, 300.0), 1.0, &Son { au: &AudioView::default(), coups: &Coups::default(), bandes: &[] }, &cfg);
        };
        let (mut g, mut h) = (Figures::new(), Figures::new());
        un(&mut g, 2.0);
        un(&mut h, 1.0);
        assert!((g.arcs.cran - 2.0 * h.arcs.cran).abs() < 1e-6 && g.arcs.bond.x > h.arcs.bond.x);
        // décalée de 60 px à droite, à moitié transparente : le dessin a bougé et pâli
        let image = |f: &Figures, cfg: &Config| {
            let mut pm = Pixmap::new(600, 600).unwrap();
            f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, 0.0, cfg, 1.0);
            pm
        };
        let droite = |pm: &Pixmap| pm.data().as_chunks::<4>().0.iter().enumerate().filter(|(j, p)| p[3] > 0 && j % 600 > 340).count();
        let opaque = |pm: &Pixmap| pm.data().as_chunks::<4>().0.iter().map(|p| p[3]).max().unwrap_or(0);
        let avant = image(&h, &cfg);
        (cfg.arcs.allure.decalage_x, cfg.arcs.allure.opacite) = (60.0, 0.5);
        avancer(&mut h, 1.0 / 60.0, 1.0 / 60.0, (300.0, 300.0), 1.0, &AudioView::default(), &Coups::default(), &cfg);
        let apres = image(&h, &cfg);
        assert!(droite(&avant) == 0 && droite(&apres) > 20, "{} → {}", droite(&avant), droite(&apres));
        assert!(opaque(&apres) < 160 && opaque(&avant) > 200, "{} → {}", opaque(&avant), opaque(&apres));
        // une modulation de la teinte change la couleur, pas la place
        cfg.arcs.allure = Allure::default();
        h.moduler(&[Effet { teinte: 180.0, ..Effet::default() }; NB_FIGURES]);
        avancer(&mut h, 2.0 / 60.0, 1.0 / 60.0, (300.0, 300.0), 1.0, &AudioView::default(), &Coups::default(), &cfg);
        assert!(h.poses[ARCS].neutre() && h.teintes[ARCS] == 180.0);
    }

    #[test]
    fn l_anneau_reste_etire_quand_le_pointeur_file_loin() {
        let mut cfg = Config::default();
        cfg.anneau.actif = true;
        let mut f = Figures::new();
        let (au, coups) = (AudioView::default(), Coups::default());
        // le pointeur file vers la droite, vite : le centre est au bout de son retard, sa vitesse
        // remise à zéro à chaque image ; l'anneau doit rester étiré vers le pointeur
        for i in 0..120 {
            avancer(&mut f, i as f64 / 120.0, 1.0 / 120.0, (100.0 + 40.0 * i as f32, 300.0), 1.0, &au, &coups, &cfg);
        }
        let (e, cos, sin) = f.etirement(1.0, cfg.anneau.etirement);
        assert!(e > 0.8 * cfg.anneau.etirement && cos > 0.99 && sin.abs() < 0.05, "étirement {e}, direction ({cos}, {sin})");
        // le pointeur s'arrête : l'anneau revient rond
        for i in 120..360 {
            avancer(&mut f, i as f64 / 120.0, 1.0 / 120.0, (100.0 + 40.0 * 119.0, 300.0), 1.0, &au, &coups, &cfg);
        }
        assert!(f.etirement(1.0, cfg.anneau.etirement).0 < 0.02, "{}", f.etirement(1.0, cfg.anneau.etirement).0);
    }

    #[test]
    fn le_centre_suit_le_pointeur_sans_trop_trainer() {
        let cfg = toutes();
        let mut f = Figures::new();
        let (au, coups) = (AudioView::default(), Coups::default());
        avancer(&mut f, 0.0, 1.0 / 60.0, (100.0, 100.0), 1.0, &au, &coups, &cfg);
        // saut du pointeur sur un autre écran : le centre reste à portée, puis le rejoint
        avancer(&mut f, 1.0 / 60.0, 1.0 / 60.0, (3000.0, 900.0), 1.0, &au, &coups, &cfg);
        let c = f.centre.unwrap();
        assert!(((c.0 - 3000.0).powi(2) + (c.1 - 900.0).powi(2)).sqrt() <= RETARD_MAX + 0.01, "{c:?}");
        for i in 2..120 {
            avancer(&mut f, i as f64 / 60.0, 1.0 / 60.0, (3000.0, 900.0), 1.0, &au, &coups, &cfg);
        }
        let c = f.centre.unwrap();
        assert!((c.0 - 3000.0).abs() < 0.5 && (c.1 - 900.0).abs() < 0.5, "{c:?}");
    }

    #[test]
    fn arcs_suivent_la_basse_et_avancent_au_kick() {
        let mut cfg = Config::default();
        cfg.arcs.actif = true;
        let mut f = Figures::new();
        let dt = 1.0 / 120.0;
        let image = |f: &mut Figures, i: usize, bass: f32, kicks: u32| {
            avancer(f, i as f64 * dt as f64, dt, (300.0, 300.0), 1.0, &AudioView { bass, ..Default::default() }, &Coups { kicks, ..Default::default() }, &cfg)
        };
        for i in 0..120 {
            image(&mut f, i, 0.0, 0);
        }
        assert!(f.arcs.grave < 0.01 && f.arcs.tour.x.abs() < 1e-3);
        // la basse ouvre le grand arc ; un kick le fait avancer d'un cran, avec un bond de son rayon
        image(&mut f, 120, 1.0, 1);
        assert!(f.arcs.bond.v > 0.0);
        for i in 121..360 {
            image(&mut f, i, 1.0, 0);
        }
        assert!(f.arcs.grave > 0.95 && (f.arcs.tour.x - ARC_CRAN).abs() < 0.02 && f.arcs.bond.x.abs() < 0.05, "{} {} {}", f.arcs.grave, f.arcs.tour.x, f.arcs.bond.x);
    }

    #[test]
    fn quatre_temps_comptent_depuis_le_drop_et_s_effacent_sans_tempo() {
        let mut cfg = Config::default();
        cfg.quatre_temps.actif = true;
        let mut f = Figures::new();
        let dt = 1.0 / 120.0;
        let mut t = 0.0f64;
        let mut actifs = Vec::new();
        while t < 4.0 {
            let (au, coups) = son(t);
            avancer(&mut f, t, dt, (300.0, 300.0), 1.0, &au, &coups, &cfg);
            if actifs.last() != Some(&f.temps.actif) {
                actifs.push(f.temps.actif);
            }
            t += dt as f64;
        }
        // un point après l'autre ; au drop (2 s), le compte repart du premier
        let suite: Vec<usize> = actifs.iter().flatten().copied().collect();
        assert!(suite.windows(2).all(|w| w[1] == (w[0] + 1) % 4 || w[1] == 0), "{suite:?}");
        assert!(suite.len() >= 7 && f.tempo == 1.0, "{suite:?}");
        // plus de tempo : les points s'effacent, plus rien n'est dessiné
        for i in 0..150 {
            avancer(&mut f, t + i as f64 * dt as f64, dt, (300.0, 300.0), 1.0, &AudioView::default(), &Coups::default(), &cfg);
        }
        assert_eq!((f.tempo, f.temps.actif), (0.0, None));
        let mut pm = Pixmap::new(600, 600).unwrap();
        f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, &cfg, 1.0);
        assert!(pm.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn figures_remises_a_zero_au_rallumage() {
        let mut cfg = Config::default();
        (cfg.arcs.actif, cfg.anneau.actif) = (true, true);
        let mut f = Figures::new();
        let (au, kick) = (AudioView::default(), Coups { kicks: 1, ..Default::default() });
        // les arcs ont avancé d'un cran, le pointeur file : le centre a de l'élan et du retard
        for i in 0..10 {
            avancer(&mut f, i as f64 / 60.0, 1.0 / 60.0, (100.0 + 30.0 * i as f32, 300.0), 1.0, &au, &kick, &cfg);
        }
        assert!(f.arcs.cran > 0.0 && f.retard.0.abs() > 1.0);
        // arcs seuls éteints puis rallumés : ils repartent de zéro (l'anneau, resté allumé, non)
        cfg.arcs.actif = false;
        avancer(&mut f, 10.0 / 60.0, 1.0 / 60.0, (400.0, 300.0), 1.0, &au, &Coups::default(), &cfg);
        cfg.arcs.actif = true;
        avancer(&mut f, 11.0 / 60.0, 1.0 / 60.0, (400.0, 300.0), 1.0, &au, &Coups::default(), &cfg);
        assert_eq!((f.arcs.cran, f.arcs.bond.v), (0.0, 0.0));
        assert!(f.centre.is_some() && f.retard.0.abs() > 0.0, "le centre, partagé, n'est pas remis à zéro");
        // toutes éteintes : plus ni élan ni retard ; au rallumage, le centre part du pointeur
        let eteint = Config::default();
        avancer(&mut f, 12.0 / 60.0, 1.0 / 60.0, (400.0, 300.0), 1.0, &au, &Coups::default(), &eteint);
        assert_eq!((f.centre, f.vitesse, f.retard), (None, (0.0, 0.0), (0.0, 0.0)));
        avancer(&mut f, 13.0 / 60.0, 1.0 / 60.0, (800.0, 300.0), 1.0, &au, &Coups::default(), &cfg);
        assert_eq!(f.centre, Some((800.0, 300.0)));
        assert_eq!(f.etirement(1.0, cfg.anneau.etirement).0, 0.0, "l'anneau rallumé ne doit pas être étiré");
    }

    #[test]
    fn le_tiret_garde_la_taille_du_point() {
        // pose agrandie (rayon ×2) et traits affinés (×0,5) : le tiret d'un point doit garder la
        // largeur du diamètre des autres points, pas celle d'un trait
        #[derive(Default)]
        struct Releve {
            lignes: Vec<f32>,
            cercles: Vec<f32>,
        }
        impl Canvas for Releve {
            fn line(&mut self, _: (f32, f32), _: (f32, f32), w: f32, _: Rgb, _: f32, _: bool) {
                self.lignes.push(w);
            }
            fn polyline(&mut self, _: &[(f32, f32)], _: f32, _: Rgb, _: f32, _: bool) {}
            fn circle(&mut self, _: f32, _: f32, r: f32, _: Rgb, _: f32) {
                self.cercles.push(r);
            }
            fn ring(&mut self, _: f32, _: f32, _: f32, _: f32, _: Rgb, _: f32) {}
            fn voile(&mut self, _: (f32, f32, f32, f32), _: f32, _: f32, _: f32, _: f32, _: Rgb, _: f32) {}
            fn vignette(&mut self, _: &Pixmap, _: u64, _: f32, _: f32, _: f32) {}
        }
        let mut cfg = Config::default();
        cfg.quatre_temps.actif = true;
        let mut f = Figures::new();
        f.moduler(&[Effet { rayon: 2.0, taille: 0.5, ..Effet::default() }; NB_FIGURES]);
        let au = AudioView { bpm: Some(120.0), ..Default::default() };
        for i in 0..60 {
            avancer(&mut f, i as f64 / 60.0, 1.0 / 60.0, (300.0, 300.0), 1.0, &au, &Coups::default(), &cfg);
        }
        let pose = f.poses[QUATRE_TEMPS];
        assert!((pose.geo - 2.0).abs() < 1e-6 && (pose.traits - 0.5).abs() < 1e-6, "{pose:?}");
        // tous les points au repos et pareils ; le premier en tiret
        f.temps.actif = None;
        for p in &mut f.temps.pts {
            (p.taille, p.tiret, p.saut) = (Ressort::en(1.0), Ressort::default(), Ressort::default());
        }
        f.temps.pts[0].tiret = Ressort::en(3.0);
        let mut r = Releve::default();
        f.dessiner(&mut r, 1.0, &cfg, 1.0);
        assert_eq!((r.lignes.len(), r.cercles.len()), (1, 3));
        assert!((r.lignes[0] - 2.0 * r.cercles[0]).abs() < 1e-4, "tiret de {} px, points de {} px de rayon", r.lignes[0], r.cercles[0]);
    }

    #[test]
    fn anneau_laisse_le_pointeur_libre() {
        let mut cfg = Config::default();
        cfg.anneau.actif = true;
        let mut f = Figures::new();
        let dt = 1.0 / 120.0;
        let mut t = 0.0f64;
        while t < 3.0 {
            let (au, coups) = son(t);
            avancer(&mut f, t, dt, (300.0, 300.0), 1.0, &au, &coups, &cfg);
            t += dt as f64;
        }
        let mut pm = Pixmap::new(600, 600).unwrap();
        f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, &cfg, 1.0);
        // un anneau est dessiné, mais rien sous le pointeur
        assert!(pm.data().as_chunks::<4>().0.iter().filter(|p| p[3] > 100).count() > 60);
        for (dx, dy) in [(0i32, 0i32), (2, 2), (-2, 1)] {
            assert_eq!(pm.pixel((300 + dx) as u32, (300 + dy) as u32).unwrap().alpha(), 0);
        }
    }

    #[test]
    fn les_declencheurs_envoient_leurs_gestes_aux_figures_visees() {
        use crate::config::{Action, Declencheur, Evenement, Groupe};
        let mut cfg = toutes();
        cfg.declencheurs.liste = vec![
            Declencheur { sur: Groupe::Pendule, ..Declencheur::new(Evenement::Clic, Action::Accent) },
            Declencheur::new(Evenement::Clic, Action::Battement),
        ];
        let mut s = crate::render::Swarm::avec_graine(1);
        s.evenement(Evenement::Clic, 300.0, 300.0, 0.0, &cfg, 1.0);
        let g = s.prendre_gestes();
        // l'accent visé : le pendule seul ; le battement pour toutes : chaque figure qui le fait
        for (i, g) in g.iter().enumerate() {
            assert_eq!(g.n(ACCENT), (i == PENDULE) as u32, "{}", crate::config::FIGURES[i]);
            assert_eq!(g.n(BATTEMENT), crate::config::gestes_de_figure(i).contains(&Action::Battement) as u32);
        }
        // pris une fois : la liste repart de zéro
        assert!(s.prendre_gestes().iter().all(|g| *g == Gestes::default()));
    }

    #[test]
    fn l_anneau_qui_tourne_reste_etire_vers_le_pointeur() {
        // l'anneau tourne vite ; le pointeur file vers la droite : à chaque image, l'anneau dessiné
        // est plus large que haut (étiré à l'horizontale, quel que soit son angle)
        let mut cfg = Config::default();
        (cfg.anneau.actif, cfg.anneau.allure.rotation) = (true, 30.0);
        let mut f = Figures::new();
        let (au, coups) = (AudioView::default(), Coups::default());
        for i in 0..240 {
            let cursor = (100.0 + 6.0 * i as f32, 300.0);
            avancer(&mut f, i as f64 / 120.0, 1.0 / 120.0, cursor, 1.0, &au, &coups, &cfg);
            if i >= 60 && i % 20 == 0 {
                let mut pm = Pixmap::new(1800, 600).unwrap();
                f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, i as f64 / 120.0, &cfg, 1.0);
                let (mut x0, mut x1, mut y0, mut y1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
                for (j, p) in pm.data().as_chunks::<4>().0.iter().enumerate() {
                    if p[3] > 80 {
                        let (x, y) = ((j % 1800) as i32, (j / 1800) as i32);
                        (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
                    }
                }
                assert!(x1 - x0 > (y1 - y0) * 3 / 2, "angle {} : {} × {} px", f.poses[ANNEAU].angle, x1 - x0, y1 - y0);
            }
        }
    }
}

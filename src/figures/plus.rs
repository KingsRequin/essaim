//! Cinq petites figures de plus : orbe (`[orbe]`), ligne de voix (`[voix]`), trame (`[trame]`),
//! trait de pinceau (`[pinceau]`) et pendule (`[pendule]`). `Figures` les possède et leur passe ce
//! qu'elle partage (`Partage`). Les trois premières tournent autour du centre des figures ; le
//! trait suit le trajet du pointeur et le pendule y est accroché.
//!
//! `ui/apercu.js` les reproduit (mêmes formules) : les modifier ici oblige à les reporter là.

use super::{autour, gauss, melange, Elan, Partage, Ressort, Son, ANNEAU_ETIRE, ORBE, PENDULE, SOUS_LE_POINTEUR, TRAIT, TRAME, VOIX};
use crate::modulation::NB_FIGURES;
use crate::canvas::{Canvas, Rgb};
use crate::config::Config;
use crate::layer::IRect;
use crate::render::prisme_decale;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};

/// Orbe : points de la ligne, bornes de son souffle (px à 100 %), durée d'un écho (s) et ce dont
/// il s'élargit, élan que le kick donne au souffle (px/s).
const ORBE_POINTS: usize = 96;
const ORBE_SOUFFLE: (f32, f32) = (-6.0, 12.0);
const ORBE_ECHO: (f64, f32) = (0.45, 0.32);
const ORBE_ELAN: f32 = 110.0;

/// Ligne de voix : points de la ligne et morceaux tracés (un par couleur), silence attendu avant
/// qu'elle s'efface (s), durée de son retour et de son effacement (s), bornes de sa largeur
/// (relative), part de la hauteur des ondulations moyennes et fines.
const VOIX_POINTS: usize = 40;
const VOIX_MORCEAUX: usize = 8;
const VOIX_ATTENTE: f32 = 1.5;
const VOIX_FONDU: (f32, f32) = (0.15, 0.8);
const VOIX_LARGE: (f32, f32) = (0.5, 1.6);
const VOIX_ONDES: (f32, f32) = (0.62, 0.4);

/// Trame : vide sous le pointeur (rayons où les points commencent puis sont pleins, px à 100 %),
/// vitesse et épaisseur du front d'une vague (px/s, px), force cumulée des vagues au plus,
/// déplacement d'un point par unité de force (px), loupe au plus (en rayons), vagues gardées.
const TRAME_VIDE: (f32, f32) = (7.0, 16.0);
const TRAME_VAGUE: (f32, f32) = (220.0, 10.0);
const TRAME_ONDE_MAX: f32 = 2.5;
const TRAME_DEPLACE: f32 = 3.0;
const TRAME_LOUPE_MAX: f32 = 1.5;
const TRAME_VAGUES_MAX: usize = 12;

/// Trait de pinceau : vitesse du pointeur où le trait commence, et ce qu'il faut de plus pour sa
/// pleine largeur (px/s à 100 %) ; perle d'un kick (gain, durée de sa glissade en s, largeur au
/// plus) ; un point du trajet plus loin que ça du pointeur est oublié (saut d'écran).
const PINCEAU_VITESSE: (f32, f32) = (120.0, 700.0);
const PINCEAU_PERLE: (f32, f32, f32) = (1.4, 0.4, 2.4);
const PINCEAU_LOIN: f32 = 1200.0;

/// Pendule : gravité (px/s² à 100 %), frottement (par seconde), saut du kick et pichenette de la
/// caisse claire (px/s à 100 %).
const PENDULE_GRAVITE: f32 = 900.0;
const PENDULE_FREIN: f32 = 1.8;
const PENDULE_SAUT: (f32, f32) = (250.0, 200.0);

fn lisse(a: f32, b: f32, x: f32) -> f32 {
    let k = ((x - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// Rayons de la ligne de l'orbe (px à 100 %), un par point.
type Rayons = [f32; ORBE_POINTS];
/// Échos de l'orbe gardés au plus (les plus vieux s'en vont), scintillements de la trame au plus.
const ORBE_ECHOS_MAX: usize = 8;
const TRAME_ECLAIRS_MAX: usize = 48;

struct Orbe {
    souffle: Ressort,
    /// Rayons de la ligne, recalculés à chaque image (`None` : orbe éteinte).
    contour: Option<Rayons>,
    /// Échos : naissance, force, rayons de la ligne à ce moment-là (sans allocation : le
    /// tableau garde sa capacité d'une image à l'autre).
    echos: Vec<(f64, f32, Rayons)>,
}

impl Orbe {
    fn new() -> Self {
        Self { souffle: Ressort::default(), contour: None, echos: Vec::new() }
    }
}

struct Voix {
    /// Niveaux des trois ondulations (graves, médiums, aigus), 0 à 1.
    niveaux: [f32; 3],
    large: Ressort,
    silence: f32,
    visible: f32,
}

impl Voix {
    /// Silencieuse au départ : la ligne n'apparaît qu'avec le son.
    fn new() -> Self {
        Self { niveaux: [0.0; 3], large: Ressort::en(1.0), silence: VOIX_ATTENTE, visible: 0.0 }
    }
}

struct Vague {
    x: f32,
    y: f32,
    ne: f64,
    force: f32,
    vie: f32,
    losange: bool,
}

struct Trame {
    /// Rayon de la loupe (px à 100 %) : part de 0, s'ouvre à l'activation.
    loupe: Ressort,
    vagues: Vec<Vague>,
    /// Points qui scintillent : case de la grille (colonne, ligne) et instant.
    eclairs: Vec<(i32, i32, f64)>,
    /// Scintillement de chaque case dessinée, rempli avant de la parcourir (gardé d'une image à
    /// l'autre : le dessin ne prend que `&self`).
    table: RefCell<Vec<f32>>,
}

impl Trame {
    fn new() -> Self {
        Self { loupe: Ressort::default(), vagues: Vec::new(), eclairs: Vec::new(), table: RefCell::default() }
    }
}

struct Pinceau {
    /// Trajet récent du pointeur : (instant, x, y) en coordonnées écran. Un point n'est ajouté
    /// que si le pointeur a bougé (`PINCEAU_PAS`) : la souris est lue moins souvent que l'écran
    /// ne se rafraîchit, et deux points au même endroit feraient un segment immobile (sauté, le
    /// trait se trouait et sa largeur clignotait).
    trajet: VecDeque<(f64, f32, f32)>,
    /// Instants des kicks dont la perle glisse encore.
    perles: Vec<f64>,
    vie: Ressort,
}

impl Pinceau {
    fn new() -> Self {
        Self { trajet: VecDeque::new(), perles: Vec::new(), vie: Ressort::default() }
    }
}

/// Déplacement du pointeur (px écran) qui ajoute un point au trajet du pinceau.
const PINCEAU_PAS: f32 = 0.5;

struct Pendule {
    /// Poids (écran) et sa vitesse (px/s) ; `None` avant la première image.
    poids: Option<(f32, f32)>,
    vitesse: (f32, f32),
    /// Longueur du fil (px à 100 %), point d'accroche et mou du fil (px écran).
    fil: Ressort,
    ancre: (f32, f32),
    mou: f32,
    cote: f32,
}

impl Pendule {
    fn new() -> Self {
        Self { poids: None, vitesse: (0.0, 0.0), fil: Ressort::default(), ancre: (0.0, 0.0), mou: 0.0, cote: 1.0 }
    }
}

pub(super) struct Plus {
    /// Enveloppe du kick (1 au coup, puis retombée) et générateur des tirages.
    kick: f32,
    graine: u32,
    orbe: Orbe,
    voix: Voix,
    trame: Trame,
    pinceau: Pinceau,
    pendule: Pendule,
}

impl Plus {
    pub(super) fn new() -> Self {
        Self {
            kick: 0.0,
            graine: 0x9e37_79b9,
            orbe: Orbe::new(),
            voix: Voix::new(),
            trame: Trame::new(),
            pinceau: Pinceau::new(),
            pendule: Pendule::new(),
        }
    }

    pub(super) fn actives(cfg: &Config) -> bool {
        cfg.orbe.actif || cfg.voix.actif || cfg.trame.actif || cfg.pinceau.actif || cfg.pendule.actif
    }

    /// Plus aucune figure : ce qui tient à la place du pointeur est oublié.
    pub(super) fn oublier(&mut self) {
        self.pinceau.trajet.clear();
        self.pendule.poids = None;
        self.trame.vagues.clear();
        self.trame.loupe = Ressort::default();
        self.orbe.contour = None;
        self.orbe.echos.clear();
    }

    /// La figure `i` vient d'être allumée : elle repart de son état de départ (rien de ce qu'elle
    /// faisait avant d'être éteinte ne revient).
    pub(super) fn reinitialiser(&mut self, i: usize) {
        match i {
            ORBE => {
                // l'état repart de zéro, les tampons gardent leur capacité
                self.orbe.souffle = Ressort::default();
                self.orbe.contour = None;
                self.orbe.echos.clear();
            }
            VOIX => self.voix = Voix::new(),
            TRAME => {
                self.trame.loupe = Ressort::default();
                self.trame.vagues.clear();
                self.trame.eclairs.clear();
            }
            TRAIT => self.pinceau = Pinceau::new(),
            PENDULE => self.pendule = Pendule::new(),
            _ => {}
        }
    }

    /// Tirage entre 0 et 1 (xorshift), pour les scintillements et les sauts.
    fn hasard(&mut self) -> f32 {
        self.graine ^= self.graine << 13;
        self.graine ^= self.graine >> 17;
        self.graine ^= self.graine << 5;
        (self.graine >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Rayons de l'orbe (px à 100 %) : le rayon au repos, plus le relief des bandes de fréquences
    /// (graves en haut, aigus en bas, les deux moitiés en miroir), une ondulation lente qui suit
    /// la basse et un frisson pendant une montée.
    fn rayons(&self, p: &Partage, bandes: &[f32], cfg: &Config) -> Rayons {
        let (c, n) = (&cfg.orbe, bandes.len());
        let fond = c.rayon * (1.0 - 0.35 * p.montee) + self.orbe.souffle.x;
        std::array::from_fn(|i| {
            let th = i as f32 / ORBE_POINTS as f32 * TAU;
            let bande = if n >= 2 {
                let x = (1.0 - (th / PI - 1.0).abs()) * (n - 1) as f32;
                let j = (x.floor() as usize).min(n - 2);
                let f = (1.0 - ((x - j as f32) * PI).cos()) / 2.0;
                (bandes[j] + (bandes[j + 1] - bandes[j]) * f).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let lente = (0.4 + 0.7 * p.basse) * (1.3 * (3.0 * th + p.horloge * 0.9).sin() + 0.9 * (5.0 * th - p.horloge * 1.3).sin());
            (fond + c.relief * bande + lente + 1.4 * p.montee * (14.0 * th + p.horloge * 22.0).sin()).max(2.0)
        })
    }

    /// Avance d'une image ; `ecoutes` : ce que chaque figure entend (à la force de ses réactions).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn avance(&mut self, ecoutes: &[(Partage, Elan); NB_FIGURES], t: f64, dt: f32, cursor: (f32, f32), scale: f32, son: &Son, cfg: &Config) {
        if !Self::actives(cfg) {
            self.oublier();
            return;
        }
        self.kick = if son.coups.kicks > 0 { 1.0 } else { self.kick * (-dt * 7.0).exp() };

        if cfg.orbe.actif {
            let (p, el) = &ecoutes[ORBE];
            let echo = |s: &Self, retard: f64, force: f32| (t + retard, force, s.rayons(p, son.bandes, cfg));
            if el.kick > 0.0 {
                self.orbe.souffle.v += ORBE_ELAN * el.kick;
                let e = echo(self, 0.03, el.kick.min(1.5));
                self.orbe.echos.push(e);
            }
            if el.drop > 0.0 {
                self.orbe.souffle.v += 2.0 * ORBE_ELAN * el.drop;
                for retard in [0.12, 0.24] {
                    let e = echo(self, retard, (1.5 * el.drop).min(1.5));
                    self.orbe.echos.push(e);
                }
            }
            self.orbe.souffle.vers(0.0, (17.0, 0.3), dt, ORBE_SOUFFLE);
            self.orbe.echos.retain(|e| t - e.0 < ORBE_ECHO.0);
            if self.orbe.echos.len() > ORBE_ECHOS_MAX {
                let trop = self.orbe.echos.len() - ORBE_ECHOS_MAX;
                self.orbe.echos.drain(..trop);
            }
            self.orbe.contour = Some(self.rayons(p, son.bandes, cfg));
        } else {
            self.orbe.contour = None;
            self.orbe.echos.clear();
        }

        if cfg.voix.actif {
            let ((p, el), v) = (&ecoutes[VOIX], &mut self.voix);
            let cibles = [0.3 * p.basse + 0.85 * self.kick * el.fk, if p.basse > 0.02 { 0.15 } else { 0.0 } + 0.9 * p.caisse, 0.9 * p.charleston + 0.6 * p.montee];
            for (n, c) in v.niveaux.iter_mut().zip(cibles) {
                let c = c.min(1.0);
                *n += (c - *n) * (1.0 - (-dt * if c > *n { 30.0 } else { 9.0 }).exp());
            }
            v.large.v += 5.0 * el.drop;
            v.large.vers(1.0, (12.0, 0.35), dt, VOIX_LARGE);
            // sans son, la ligne s'efface ; elle revient dès qu'il reprend
            v.silence = if v.niveaux.iter().sum::<f32>() < 0.03 { v.silence + dt } else { 0.0 };
            v.visible = if v.silence >= VOIX_ATTENTE { (v.visible - dt / VOIX_FONDU.1).max(0.0) } else { (v.visible + dt / VOIX_FONDU.0).min(1.0) };
        } else {
            (self.voix.silence, self.voix.visible) = (VOIX_ATTENTE, 0.0);
        }

        if cfg.trame.actif {
            let ((p, el), c) = (&ecoutes[TRAME], &cfg.trame);
            let ((cx, cy), pas) = (p.centre, c.pas * scale);
            if el.aigus > 0.0 {
                // quelques points scintillent autour du pointeur
                for _ in 0..(4.0 * el.aigus).round() as usize {
                    let (a, d) = (self.hasard() * TAU, (14.0 + 28.0 * self.hasard()) * scale);
                    self.trame.eclairs.push((((cx + a.cos() * d) / pas).round() as i32, ((cy + a.sin() * d) / pas).round() as i32, t));
                }
            }
            let tr = &mut self.trame;
            if el.kick > 0.0 {
                tr.vagues.push(Vague { x: cx, y: cy, ne: t, force: el.kick, vie: 0.4, losange: false });
                tr.loupe.v += 1.96 * c.rayon * el.kick;
            }
            if el.caisse > 0.0 {
                tr.vagues.push(Vague { x: cx, y: cy, ne: t, force: 0.6 * el.caisse, vie: 0.34, losange: true });
            }
            if el.drop > 0.0 {
                tr.vagues.push(Vague { x: cx, y: cy, ne: t + 0.05, force: 1.8 * el.drop, vie: 0.6, losange: false });
            }
            tr.vagues.retain(|v| t - v.ne < v.vie as f64);
            if tr.vagues.len() > TRAME_VAGUES_MAX {
                let trop = tr.vagues.len() - TRAME_VAGUES_MAX;
                tr.vagues.drain(..trop);
            }
            tr.eclairs.retain(|e| t - e.2 < 0.4);
            if tr.eclairs.len() > TRAME_ECLAIRS_MAX {
                let trop = tr.eclairs.len() - TRAME_ECLAIRS_MAX;
                tr.eclairs.drain(..trop);
            }
            tr.loupe.vers(c.rayon * (1.0 + 0.3 * p.basse), (15.0, 0.4), dt, (0.0, c.rayon * TRAME_LOUPE_MAX));
        } else {
            self.trame.vagues.clear();
            self.trame.loupe = Ressort::default();
        }

        if cfg.pinceau.actif {
            let (el, pi, c) = (&ecoutes[TRAIT].1, &mut self.pinceau, &cfg.pinceau);
            if el.kick > 0.0 && pi.perles.len() < 6 {
                pi.perles.push(t);
            }
            if pi.vie.x <= 0.0 {
                pi.vie = Ressort::en(c.duree);
            }
            pi.vie.v += 5.2 * c.duree * el.drop;
            pi.vie.vers(c.duree, (9.0, 0.45), dt, (c.duree * 0.5, c.duree * 2.2));
            pi.perles.retain(|&ne| t - ne < 0.5);
            let immobile = pi.trajet.back().is_some_and(|q| (q.1 - cursor.0).hypot(q.2 - cursor.1) < PINCEAU_PAS);
            if !immobile {
                pi.trajet.push_back((t, cursor.0, cursor.1));
            }
            let (vie, loin) = (pi.vie.x as f64, PINCEAU_LOIN * scale);
            pi.trajet.retain(|q| t - q.0 <= vie && (q.1 - cursor.0).abs() <= loin && (q.2 - cursor.1).abs() <= loin);
        } else {
            self.pinceau.trajet.clear();
        }

        if cfg.pendule.actif && dt > 1e-5 {
            let ((p, el), c) = (&ecoutes[PENDULE], &cfg.pendule);
            let saut = (self.hasard() * 2.0 - 1.0) * 70.0 * scale;
            let pe = &mut self.pendule;
            if pe.fil.x <= 0.0 {
                pe.fil = Ressort::en(c.longueur);
            }
            pe.fil.v += 7.3 * c.longueur * el.drop;
            pe.fil.vers(c.longueur * (1.0 + 0.32 * p.basse - 0.6 * p.montee), (12.0, 0.4), dt, (4.0, c.longueur * 2.0));
            let l = pe.fil.x * scale;
            let (mut qx, mut qy) = pe.poids.unwrap_or((cursor.0, cursor.1 + l));
            if el.kick > 0.0 {
                pe.vitesse.1 -= PENDULE_SAUT.0 * scale * el.kick;
                pe.vitesse.0 += saut * el.kick;
            }
            if el.caisse > 0.0 {
                pe.cote = -pe.cote;
                pe.vitesse.0 += PENDULE_SAUT.1 * pe.cote * scale * el.caisse;
            }
            if el.drop > 0.0 {
                pe.vitesse.1 -= 1.5 * PENDULE_SAUT.0 * scale * el.drop;
            }
            // gravité et frottement, puis le fil : il ne s'étire pas, mais peut se détendre
            let (avant, frein) = ((qx, qy), (-dt * PENDULE_FREIN).exp());
            pe.vitesse = (pe.vitesse.0 * frein, (pe.vitesse.1 + PENDULE_GRAVITE * scale * dt) * frein);
            (qx, qy) = (qx + pe.vitesse.0 * dt, qy + pe.vitesse.1 * dt);
            let (dx, dy) = (qx - cursor.0, qy - cursor.1);
            let d = (dx * dx + dy * dy).sqrt();
            if d > l {
                (qx, qy) = (cursor.0 + dx / d * l, cursor.1 + dy / d * l);
            }
            // la vitesse est celle du déplacement réel : le fil tendu entraîne le poids
            pe.vitesse = ((qx - avant.0) / dt, (qy - avant.1) / dt);
            if !(qx.is_finite() && qy.is_finite() && pe.vitesse.0.is_finite() && pe.vitesse.1.is_finite()) {
                (qx, qy, pe.vitesse) = (cursor.0, cursor.1 + l, (0.0, 0.0));
            }
            (pe.poids, pe.ancre, pe.mou) = (Some((qx, qy)), cursor, (l - d.min(l)).max(0.0));
        } else if !cfg.pendule.actif {
            self.pendule.poids = None;
        }
    }

    /// Demi-côté (px à 100 %) du carré autour du centre qui contient l'orbe, la ligne de voix ou la
    /// trame.
    pub(super) fn etendue(&self, i: usize, cfg: &Config) -> f32 {
        match i {
            ORBE => {
                let c = &cfg.orbe;
                // étirée comme l'anneau, de `ANNEAU_ETIRE.0` au plus
                (c.rayon + c.relief + ORBE_SOUFFLE.1 + 4.0) * (1.0 + ORBE_ECHO.1 * 1.5) * (1.0 + ANNEAU_ETIRE.0) + c.epaisseur + 1.0
            }
            VOIX => {
                let c = &cfg.voix;
                let large = SOUS_LE_POINTEUR.0 + c.largeur * VOIX_LARGE.1 / 2.0;
                let haut = SOUS_LE_POINTEUR.1 + c.hauteur * (1.4 + 1.0 + VOIX_ONDES.0 + VOIX_ONDES.1);
                large.max(haut) + c.epaisseur + 1.0
            }
            _ => {
                let c = &cfg.trame;
                let point = (1.9 + 2.0 * TRAME_ONDE_MAX + 1.5) * c.taille;
                self.trame.loupe.x.max(4.0) * 1.25 + TRAME_DEPLACE * TRAME_ONDE_MAX + point + 1.0
            }
        }
    }

    /// Zone du pendule, autour de son point d'accroche (traits épaissis de `traits`).
    pub(super) fn rect_pendule(&self, cursor: (f32, f32), scale: f32, traits: f32, cfg: &Config) -> Option<IRect> {
        let c = &cfg.pendule;
        let fil = if self.pendule.fil.x > 0.0 { self.pendule.fil.x } else { c.longueur };
        let ancre = if self.pendule.poids.is_some() { self.pendule.ancre } else { cursor };
        Some(autour(ancre, (fil * 1.15 + c.taille + 2.8) * scale * traits.max(1.0)))
    }

    /// Zone du trait de pinceau, le long du trajet du pointeur ; `None` sans trajet.
    pub(super) fn rect_trait(&self, scale: f32, cfg: &Config) -> Option<IRect> {
        if self.pinceau.trajet.is_empty() {
            return None;
        }
        let demi = (0.6 + 1.2 * cfg.pinceau.largeur) * PINCEAU_PERLE.2 / 2.0 * scale + 2.0;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(_, x, y) in &self.pinceau.trajet {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        Some(((x0 - demi).floor() as i32, (y0 - demi).floor() as i32, (x1 + demi).ceil() as i32, (y1 + demi).ceil() as i32))
    }

    pub(super) fn dessiner_trame(&self, p: &Partage, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32, tete: Rgb) {
        let (cx, cy) = p.centre;
        let (tr, c) = (&self.trame, &cfg.trame);
        let (pas, l) = (c.pas * scale, tr.loupe.x.max(4.0) * scale);
        let marge = l * 1.25 + 1.0;
        let cases = |m: f32| (((m - marge) / pas).floor() as i32, ((m + marge) / pas).ceil() as i32);
        let ((i0, i1), (j0, j1)) = (cases(cx), cases(cy));
        // scintillement de chaque case, posé une fois dans une table (au lieu de chercher dans
        // tous les scintillements à chaque case)
        let largeur = (i1 - i0 + 1).max(0) as usize;
        let mut table = tr.table.borrow_mut();
        table.clear();
        table.resize(largeur * (j1 - j0 + 1).max(0) as usize, 0.0);
        for e in &tr.eclairs {
            if (i0..=i1).contains(&e.0) && (j0..=j1).contains(&e.1) {
                let case = &mut table[(e.1 - j0) as usize * largeur + (e.0 - i0) as usize];
                *case = case.max((-(t - e.2) as f32 * 12.0).exp());
            }
        }
        for j in j0..=j1 {
            for i in i0..=i1 {
                let (mut x, mut y) = (i as f32 * pas, j as f32 * pas);
                let (dx, dy) = (x - cx, y - cy);
                let d = (dx * dx + dy * dy).sqrt();
                // le vide sous le pointeur, puis la loupe qui s'estompe vers son bord
                let vide = lisse(TRAME_VIDE.0 * scale, TRAME_VIDE.1 * scale, d);
                let dedans = (1.0 - lisse(l * 0.35, l, d)) * vide;
                let large = (1.0 - lisse(l * 0.3, l * 1.25, d)) * vide;
                if large <= 0.01 {
                    continue;
                }
                let (mut onde, mut px, mut py) = (0.0f32, 0.0f32, 0.0f32);
                for v in &tr.vagues {
                    let age = (t - v.ne) as f32;
                    if age < 0.0 {
                        continue;
                    }
                    let (vx, vy) = (x - v.x, y - v.y);
                    let droit = (vx * vx + vy * vy).sqrt();
                    let dist = if v.losange { (vx.abs() + vy.abs()) / 1.25 } else { droit };
                    let f = gauss((dist - TRAME_VAGUE.0 * scale * age) / (TRAME_VAGUE.1 * scale)) * v.force * (1.0 - age / v.vie).max(0.0);
                    onde += f;
                    if droit > 1e-3 {
                        px += vx / droit * f;
                        py += vy / droit * f;
                    }
                }
                let onde = onde.min(TRAME_ONDE_MAX);
                let pousse = (px * px + py * py).sqrt();
                if pousse > TRAME_ONDE_MAX {
                    (px, py) = (px / pousse * TRAME_ONDE_MAX, py / pousse * TRAME_ONDE_MAX);
                }
                let eclair = table[(j - j0) as usize * largeur + (i - i0) as usize];
                x += px * TRAME_DEPLACE * scale - dx * 0.3 * p.montee * dedans;
                y += py * TRAME_DEPLACE * scale - dy * 0.3 * p.montee * dedans;
                let r = (dedans * (1.0 + 0.9 * p.basse) + 2.0 * onde * large + 1.5 * eclair * large) * c.taille * scale;
                if r >= 0.25 {
                    cv.circle(x, y, r, tete, (0.3 + 0.35 * r / scale).min(1.0));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Orbe, étirée comme l'anneau (`etire` : étirement, cosinus et sinus de sa direction).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn dessiner_orbe(&self, p: &Partage, cv: &mut dyn Canvas, pts: &mut Vec<(f32, f32)>, t: f64, cfg: &Config, scale: f32, (tete, queue): (Rgb, Rgb), (e, ec, es): (f32, f32, f32)) {
        let Some(contour) = &self.orbe.contour else { return };
        let ((cx, cy), c) = (p.centre, &cfg.orbe);
        let mut tracer = |rayons: &[f32], echelle: f32, largeur: f32, rgb: Rgb, al: f32| {
            pts.clear();
            pts.extend((0..=ORBE_POINTS).map(|i| {
                let (sin, cos) = (-PI / 2.0 + i as f32 / ORBE_POINTS as f32 * TAU).sin_cos();
                let r = rayons[i % ORBE_POINTS] * echelle * scale;
                let (x, y) = (cos * r, sin * r);
                if e == 0.0 {
                    return (cx + x, cy + y);
                }
                // étirée le long de (ec, es), resserrée en travers
                let (a, b) = ((x * ec + y * es) * (1.0 + e), (-x * es + y * ec) * (1.0 - e));
                (cx + a * ec - b * es, cy + a * es + b * ec)
            }));
            cv.polyline(pts, largeur, rgb, al, true);
        };
        for (ne, force, rayons) in &self.orbe.echos {
            let k = ((t - ne) / ORBE_ECHO.0) as f32;
            if (0.0..1.0).contains(&k) {
                tracer(rayons, 1.0 + ORBE_ECHO.1 * force * (1.0 - (1.0 - k).powi(3)), (1.3 * (1.0 - k) + 0.4) * scale, melange(tete, queue, k), 0.55 * (1.0 - k));
            }
        }
        tracer(contour, 1.0, c.epaisseur * scale, tete, 1.0);
    }

    pub(super) fn dessiner_voix(&self, p: &Partage, cv: &mut dyn Canvas, pts: &mut Vec<(f32, f32)>, cfg: &Config, scale: f32, (tete, queue): (Rgb, Rgb)) {
        if self.voix.visible <= 0.01 {
            return;
        }
        let ((cx, cy), v, c) = (p.centre, &self.voix, &cfg.voix);
        let (w, h) = (c.largeur * scale * v.large.x, c.hauteur * scale);
        let (ax, ay) = (cx + SOUS_LE_POINTEUR.0 * scale, cy + (SOUS_LE_POINTEUR.1 + 1.4 * c.hauteur) * scale);
        let [grave, medium, aigu] = v.niveaux;
        let point = |i: usize| {
            let u = i as f32 / VOIX_POINTS as f32;
            let fenetre = (PI * u).sin().powi(2);
            let onde = grave * (TAU * 1.5 * u + p.horloge * 5.0).sin() + VOIX_ONDES.0 * medium * (TAU * 3.0 * u - p.horloge * 8.0).sin() + VOIX_ONDES.1 * aigu * (TAU * 6.5 * u + p.horloge * 13.0).sin();
            (ax + (u - 0.5) * w, ay + fenetre * h * onde)
        };
        let al = v.visible * (0.5 + 0.5 * (grave + medium + aigu).min(1.0));
        let par = VOIX_POINTS / VOIX_MORCEAUX;
        for m in 0..VOIX_MORCEAUX {
            pts.clear();
            pts.extend((m * par..=(m + 1) * par).map(point));
            cv.polyline(pts, c.epaisseur * scale, melange(tete, queue, (m as f32 + 0.5) / VOIX_MORCEAUX as f32), al, true);
        }
    }

    /// Trait de pinceau ; `teinte` : degrés ajoutés à la couleur de l'essaim, dont il prend le
    /// dégradé de la tête à la queue.
    pub(super) fn dessiner_trait(&self, p: &Partage, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32, teinte: f32) {
        if self.pinceau.trajet.len() < 2 {
            return;
        }
        let (pi, c) = (&self.pinceau, &cfg.pinceau);
        let vie = pi.vie.x.max(0.05) as f64;
        for (a, b) in pi.trajet.iter().zip(pi.trajet.iter().skip(1)) {
            let duree = ((b.0 - a.0) as f32).max(1e-3);
            let v = ((b.1 - a.1).powi(2) + (b.2 - a.2).powi(2)).sqrt() / duree / scale;
            let vif = ((v - PINCEAU_VITESSE.0) / PINCEAU_VITESSE.1).clamp(0.0, 1.0).powf(0.8);
            if vif < 0.02 {
                continue;
            }
            let k = (1.0 - (t - b.0) / vie).clamp(0.0, 1.0) as f32;
            let profil = (PI * k.powf(0.6)).sin().max(0.0).powf(0.7);
            let perle = pi.perles.iter().map(|&ne| PINCEAU_PERLE.0 * gauss((k - (1.0 - (t - ne) as f32 / PINCEAU_PERLE.1)) / 0.1)).sum::<f32>();
            let largeur = (0.6 + c.largeur * vif * (0.6 + 0.6 * p.basse)) * profil * (1.0 + perle).min(PINCEAU_PERLE.2) * scale;
            if largeur >= 0.4 {
                cv.line((a.1, a.2), (b.1, b.2), largeur, prisme_decale(cfg, t, k, teinte), 1.0, true);
            }
        }
    }

    pub(super) fn dessiner_pendule(&self, cv: &mut dyn Canvas, pts: &mut Vec<(f32, f32)>, cfg: &Config, scale: f32, (tete, queue): (Rgb, Rgb)) {
        let Some((qx, qy)) = self.pendule.poids else { return };
        let (pe, c) = (&self.pendule, &cfg.pendule);
        let (ax, ay) = pe.ancre;
        // le fil : droit quand il est tendu, il pend quand le poids est remonté
        let (mx, my) = ((ax + qx) / 2.0, (ay + qy) / 2.0 + pe.mou * 0.6);
        pts.clear();
        pts.extend((0..=8).map(|i| {
            let u = i as f32 / 8.0;
            let (a, b, d) = ((1.0 - u).powi(2), 2.0 * u * (1.0 - u), u * u);
            (a * ax + b * mx + d * qx, a * ay + b * my + d * qy)
        }));
        cv.polyline(pts, scale, queue, 0.75, true);
        cv.circle(qx, qy, (c.taille + 1.8 * self.kick * c.allure.reaction_kick) * scale, tete, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::avancer;
    use super::super::Figures;
    use super::*;
    use crate::audio::AudioView;
    use crate::render::Coups;
    use tiny_skia::Pixmap;

    const C: (f32, f32) = (300.0, 300.0);

    fn peints(f: &Figures, t: f64, cfg: &Config) -> (Pixmap, usize) {
        let mut pm = Pixmap::new(600, 600).unwrap();
        f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, cfg, 1.0);
        let n = pm.data().chunks_exact(4).filter(|p| p[3] > 60).count();
        (pm, n)
    }

    #[test]
    fn orbe_suit_les_bandes_et_laisse_le_pointeur_libre() {
        let mut cfg = Config::default();
        cfg.orbe.actif = true;
        let mut f = Figures::new();
        let (au, coups) = (AudioView::default(), Coups::default());
        // sans bandes : un cercle au rayon réglé ; avec les graves à fond : le haut de la ligne monte
        let mut bandes = [0.0f32; 16];
        for i in 0..120 {
            f.avance(i as f64 / 120.0, 1.0 / 120.0, C, 1.0, &Son { au: &au, coups: &coups, bandes: &bandes }, &cfg);
        }
        let plat = f.plus.orbe.contour.unwrap();
        assert!(plat.iter().all(|r| (r - cfg.orbe.rayon).abs() < 2.0), "{plat:?}");
        bandes[0] = 1.0;
        f.avance(1.0, 1.0 / 120.0, C, 1.0, &Son { au: &au, coups: &coups, bandes: &bandes }, &cfg);
        let contour = f.plus.orbe.contour.unwrap();
        let (haut, bas) = (contour[0], contour[ORBE_POINTS / 2]);
        assert!(haut > cfg.orbe.rayon + cfg.orbe.relief - 2.0 && bas < cfg.orbe.rayon + 2.0, "haut {haut}, bas {bas}");
        let (pm, n) = peints(&f, 1.0, &cfg);
        assert!(n > 100 && pm.pixel(300, 300).unwrap().alpha() == 0, "{n} pixels");
        // un kick détache un écho, qui s'efface en moins d'une demi-seconde
        f.avance(1.01, 1.0 / 120.0, C, 1.0, &Son { au: &au, coups: &Coups { kicks: 1, ..Default::default() }, bandes: &bandes }, &cfg);
        assert_eq!(f.plus.orbe.echos.len(), 1);
        for i in 0..70 {
            f.avance(1.02 + i as f64 / 120.0, 1.0 / 120.0, C, 1.0, &Son { au: &au, coups: &coups, bandes: &bandes }, &cfg);
        }
        assert!(f.plus.orbe.echos.is_empty());
    }

    #[test]
    fn la_ligne_de_voix_s_efface_sans_son_et_revient() {
        let mut cfg = Config::default();
        cfg.voix.actif = true;
        let mut f = Figures::new();
        let dt = 1.0 / 60.0;
        let mut t = 0.0f64;
        let mut joue = |f: &mut Figures, secondes: f32, bass: f32, kicks: u32| {
            for i in 0..(secondes * 60.0) as usize {
                avancer(f, t, dt, C, 1.0, &AudioView { bass, ..Default::default() }, &Coups { kicks: (kicks > 0 && i % 30 == 0) as u32, ..Default::default() }, &cfg);
                t += dt as f64;
            }
            t
        };
        // musique : la ligne est là, sous le pointeur (rien au-dessus de sa pointe)
        let t1 = joue(&mut f, 1.0, 0.8, 1);
        let (pm, n) = peints(&f, t1, &cfg);
        assert!(f.plus.voix.visible == 1.0 && n > 40, "{n} pixels");
        assert!((0..600 * 300).all(|j| pm.data()[j * 4 + 3] == 0), "la ligne passe sur le pointeur");
        // silence : elle reste un moment, puis s'efface ; le son la ramène aussitôt
        let t2 = joue(&mut f, 4.0, 0.0, 0);
        assert_eq!((f.plus.voix.visible, peints(&f, t2, &cfg).1), (0.0, 0));
        joue(&mut f, 0.3, 0.8, 1);
        assert_eq!(f.plus.voix.visible, 1.0);
    }

    #[test]
    fn la_trame_laisse_un_vide_sous_le_pointeur_et_ses_vagues_passent() {
        let mut cfg = Config::default();
        cfg.trame.actif = true;
        let mut f = Figures::new();
        let dt = 1.0 / 120.0;
        for i in 0..240 {
            avancer(&mut f, i as f64 * dt as f64, dt, C, 1.0, &AudioView { bass: 0.6, ..Default::default() }, &Coups::default(), &cfg);
        }
        // la loupe s'est ouverte : des points autour du pointeur, aucun dessus
        let (pm, repos) = peints(&f, 2.0, &cfg);
        assert!(f.plus.trame.loupe.x > cfg.trame.rayon && repos > 20, "loupe {}, {repos} pixels", f.plus.trame.loupe.x);
        for (dx, dy) in [(0i32, 0i32), (4, 0), (0, -5), (-3, 3)] {
            assert_eq!(pm.pixel((300 + dx) as u32, (300 + dy) as u32).unwrap().alpha(), 0);
        }
        // un kick : une vague grossit les points sur son passage, puis s'efface
        avancer(&mut f, 2.0, dt, C, 1.0, &AudioView { bass: 0.6, ..Default::default() }, &Coups { kicks: 1, ..Default::default() }, &cfg);
        for i in 1..12 {
            avancer(&mut f, 2.0 + i as f64 * dt as f64, dt, C, 1.0, &AudioView { bass: 0.6, ..Default::default() }, &Coups::default(), &cfg);
        }
        assert!(peints(&f, 2.0 + 11.0 * dt as f64, &cfg).1 > repos * 2);
        for i in 12..80 {
            avancer(&mut f, 2.0 + i as f64 * dt as f64, dt, C, 1.0, &AudioView { bass: 0.6, ..Default::default() }, &Coups::default(), &cfg);
        }
        assert!(f.plus.trame.vagues.is_empty());
    }

    #[test]
    fn le_trait_ne_se_voit_que_quand_la_souris_file() {
        let mut cfg = Config::default();
        cfg.pinceau.actif = true;
        let mut f = Figures::new();
        let dt = 1.0 / 120.0;
        let (au, coups) = (AudioView::default(), Coups::default());
        // au repos, puis en mouvement lent (60 px/s) : rien
        for i in 0..120 {
            avancer(&mut f, i as f64 * dt as f64, dt, (100.0 + i as f32 * 0.5, 300.0), 1.0, &au, &coups, &cfg);
        }
        assert_eq!(peints(&f, 1.0, &cfg).1, 0);
        // la souris file (1200 px/s) : un trait derrière elle, dans la zone
        for i in 0..36 {
            avancer(&mut f, 1.0 + i as f64 * dt as f64, dt, (160.0 + i as f32 * 10.0, 300.0), 1.0, &au, &coups, &cfg);
        }
        let t = 1.0 + 35.0 * dt as f64;
        let (pm, n) = peints(&f, t, &cfg);
        assert!(n > 300, "{n} pixels");
        let (x0, y0, x1, y1) = f.rect((510.0, 300.0), 1.0, &cfg).unwrap();
        for (j, p) in pm.data().chunks_exact(4).enumerate() {
            let (x, y) = ((j % 600) as i32, (j / 600) as i32);
            assert!(p[3] == 0 || (x >= x0 && x < x1 && y >= y0 && y < y1), "pixel ({x}, {y}) hors de la zone");
        }
        // elle s'arrête : le trait s'efface en moins d'une seconde
        for i in 0..120 {
            avancer(&mut f, t + (i + 1) as f64 * dt as f64, dt, (510.0, 300.0), 1.0, &au, &coups, &cfg);
        }
        assert_eq!(peints(&f, t + 1.0, &cfg).1, 0);
    }

    #[test]
    fn le_trait_reste_plein_quand_la_souris_est_lue_moins_souvent_que_l_ecran() {
        // souris lue à 125 Hz, écran à 240 Hz : le pointeur ne bouge pas à certaines images ;
        // aucun segment immobile dans le trajet, et le trait garde sa largeur d'une image à l'autre
        let mut cfg = Config::default();
        cfg.pinceau.actif = true;
        let mut f = Figures::new();
        let dt = 1.0 / 240.0;
        let (au, coups) = (AudioView::default(), Coups::default());
        let mut largeurs = Vec::new();
        for i in 0..96 {
            let t = i as f64 * dt as f64;
            // position lue au dernier rapport de la souris (toutes les 8 ms), à 1200 px/s
            let lu = (t * 125.0).floor() / 125.0;
            avancer(&mut f, t, dt, (100.0 + 1200.0 * lu as f32, 300.0), 1.0, &au, &coups, &cfg);
            if i >= 48 {
                largeurs.push(peints(&f, t, &cfg).1 as f32);
            }
        }
        let tr = &f.plus.pinceau.trajet;
        assert!(tr.iter().zip(tr.iter().skip(1)).all(|(a, b)| (b.1 - a.1).hypot(b.2 - a.2) >= PINCEAU_PAS), "segment immobile dans le trajet");
        // le trait s'allonge : d'une image à l'autre, il ne perd jamais de pixels peints
        assert!(largeurs[0] > 300.0 && largeurs.windows(2).all(|w| w[1] >= 0.95 * w[0]), "pixels peints : {largeurs:?}");
    }

    #[test]
    fn le_pendule_reste_au_bout_de_son_fil() {
        let mut cfg = Config::default();
        cfg.pendule.actif = true;
        for dt in [1.0f32 / 144.0, 1.0 / 20.0] {
            let mut f = Figures::new();
            let mut t = 0.0f64;
            for i in 0..(6.0 / dt) as usize {
                // le pointeur fait des allers-retours rapides, avec des kicks, des caisses claires et un drop
                let cursor = (300.0 + 200.0 * (t as f32 * 6.0).sin(), 300.0 + 80.0 * (t as f32 * 4.0).cos());
                let coups = Coups { kicks: (i % 9 == 0) as u32, caisses: (i % 14 == 0) as u32, drops: (i == 40) as u32, ..Default::default() };
                avancer(&mut f, t, dt, cursor, 1.0, &AudioView { bass: 1.0, ..Default::default() }, &coups, &cfg);
                let (q, pe) = (f.plus.pendule.poids.unwrap(), &f.plus.pendule);
                let d = ((q.0 - cursor.0).powi(2) + (q.1 - cursor.1).powi(2)).sqrt();
                assert!(d <= pe.fil.x + 0.01 && pe.fil.x <= cfg.pendule.longueur * 2.0, "pas de {dt} : poids à {d} px, fil de {}", pe.fil.x);
                t += dt as f64;
            }
            // au calme, il pend droit sous le pointeur
            for _ in 0..(4.0 / dt) as usize {
                avancer(&mut f, t, dt, C, 1.0, &AudioView::default(), &Coups::default(), &cfg);
                t += dt as f64;
            }
            let q = f.plus.pendule.poids.unwrap();
            assert!((q.0 - C.0).abs() < 1.5 && (q.1 - C.1 - cfg.pendule.longueur).abs() < 1.5, "pas de {dt} : {q:?}");
        }
    }
}

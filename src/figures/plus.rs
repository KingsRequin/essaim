//! Cinq petites figures de plus : orbe (`[orbe]`), ligne de voix (`[voix]`), trame (`[trame]`),
//! trait de pinceau (`[pinceau]`) et pendule (`[pendule]`). `Figures` les possède et leur passe ce
//! qu'elle partage (`Partage`). Les trois premières tournent autour du centre des figures ; le
//! trait suit le trajet du pointeur et le pendule y est accroché.
//!
//! `ui/apercu.js` les reproduit (mêmes formules) : les modifier ici oblige à les reporter là.

use super::{autour, Partage, Ressort, Son, SOUS_LE_POINTEUR};
use crate::canvas::{Canvas, Rgb};
use crate::config::Config;
use crate::layer::IRect;
use crate::render::prisme;
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

fn gauss(x: f32) -> f32 {
    (-x * x).exp()
}

fn melange(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k.clamp(0.0, 1.0)).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

struct Orbe {
    souffle: Ressort,
    /// Rayons de la ligne (px à 100 %), recalculés à chaque image.
    contour: Vec<f32>,
    /// Échos : naissance, force, rayons de la ligne à ce moment-là.
    echos: Vec<(f64, f32, Vec<f32>)>,
}

struct Voix {
    /// Niveaux des trois ondulations (graves, médiums, aigus), 0 à 1.
    niveaux: [f32; 3],
    large: Ressort,
    silence: f32,
    visible: f32,
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
}

struct Pinceau {
    /// Trajet récent du pointeur : (instant, x, y) en coordonnées écran.
    trajet: VecDeque<(f64, f32, f32)>,
    /// Instants des kicks dont la perle glisse encore.
    perles: Vec<f64>,
    vie: Ressort,
}

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
            orbe: Orbe { souffle: Ressort::default(), contour: Vec::new(), echos: Vec::new() },
            voix: Voix { niveaux: [0.0; 3], large: Ressort::en(1.0), silence: 0.0, visible: 0.0 },
            trame: Trame { loupe: Ressort::default(), vagues: Vec::new(), eclairs: Vec::new() },
            pinceau: Pinceau { trajet: VecDeque::new(), perles: Vec::new(), vie: Ressort::default() },
            pendule: Pendule { poids: None, vitesse: (0.0, 0.0), fil: Ressort::default(), ancre: (0.0, 0.0), mou: 0.0, cote: 1.0 },
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
        self.orbe.contour.clear();
        self.orbe.echos.clear();
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
    fn rayons(&self, p: &Partage, bandes: &[f32], cfg: &Config) -> Vec<f32> {
        let (c, n) = (&cfg.orbe, bandes.len());
        let fond = c.rayon * (1.0 - 0.35 * p.montee) + self.orbe.souffle.x;
        (0..ORBE_POINTS)
            .map(|i| {
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
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn avance(&mut self, p: &Partage, t: f64, dt: f32, cursor: (f32, f32), scale: f32, son: &Son, cfg: &Config) {
        if !Self::actives(cfg) {
            self.oublier();
            return;
        }
        let (kicks, caisse, drop, aigus) = (son.coups.kicks, son.coups.caisses > 0, son.coups.drops > 0, son.coups.aigus > 0);
        self.kick = if kicks > 0 { 1.0 } else { self.kick * (-dt * 7.0).exp() };
        let (cx, cy) = p.centre;

        if cfg.orbe.actif {
            let echo = |s: &Self, retard: f64, force: f32| (t + retard, force, s.rayons(p, son.bandes, cfg));
            for _ in 0..kicks.min(1) {
                self.orbe.souffle.v += ORBE_ELAN;
                let e = echo(self, 0.03, 1.0);
                self.orbe.echos.push(e);
            }
            if drop {
                self.orbe.souffle.v += 2.0 * ORBE_ELAN;
                for retard in [0.12, 0.24] {
                    let e = echo(self, retard, 1.5);
                    self.orbe.echos.push(e);
                }
            }
            self.orbe.souffle.vers(0.0, (17.0, 0.3), dt, ORBE_SOUFFLE);
            self.orbe.echos.retain(|e| t - e.0 < ORBE_ECHO.0);
            self.orbe.contour = self.rayons(p, son.bandes, cfg);
        } else {
            self.orbe.contour.clear();
            self.orbe.echos.clear();
        }

        if cfg.voix.actif {
            let v = &mut self.voix;
            let cibles = [0.3 * p.basse + 0.85 * self.kick, if p.basse > 0.02 { 0.15 } else { 0.0 } + 0.9 * p.caisse, 0.9 * p.charleston + 0.6 * p.montee];
            for (n, c) in v.niveaux.iter_mut().zip(cibles) {
                let c = c.min(1.0);
                *n += (c - *n) * (1.0 - (-dt * if c > *n { 30.0 } else { 9.0 }).exp());
            }
            if drop {
                v.large.v += 5.0;
            }
            v.large.vers(1.0, (12.0, 0.35), dt, VOIX_LARGE);
            // sans son, la ligne s'efface ; elle revient dès qu'il reprend
            v.silence = if v.niveaux.iter().sum::<f32>() < 0.03 { v.silence + dt } else { 0.0 };
            v.visible = if v.silence >= VOIX_ATTENTE { (v.visible - dt / VOIX_FONDU.1).max(0.0) } else { (v.visible + dt / VOIX_FONDU.0).min(1.0) };
        }

        if cfg.trame.actif {
            let c = &cfg.trame;
            let pas = c.pas * scale;
            if aigus {
                // quelques points scintillent autour du pointeur
                for _ in 0..4 {
                    let (a, d) = (self.hasard() * TAU, (14.0 + 28.0 * self.hasard()) * scale);
                    self.trame.eclairs.push((((cx + a.cos() * d) / pas).round() as i32, ((cy + a.sin() * d) / pas).round() as i32, t));
                }
            }
            let tr = &mut self.trame;
            for _ in 0..kicks.min(1) {
                tr.vagues.push(Vague { x: cx, y: cy, ne: t, force: 1.0, vie: 0.4, losange: false });
                tr.loupe.v += 1.96 * c.rayon;
            }
            if caisse {
                tr.vagues.push(Vague { x: cx, y: cy, ne: t, force: 0.6, vie: 0.34, losange: true });
            }
            if drop {
                tr.vagues.push(Vague { x: cx, y: cy, ne: t + 0.05, force: 1.8, vie: 0.6, losange: false });
            }
            tr.vagues.retain(|v| t - v.ne < v.vie as f64);
            if tr.vagues.len() > TRAME_VAGUES_MAX {
                let trop = tr.vagues.len() - TRAME_VAGUES_MAX;
                tr.vagues.drain(..trop);
            }
            tr.eclairs.retain(|e| t - e.2 < 0.4);
            tr.loupe.vers(c.rayon * (1.0 + 0.3 * p.basse), (15.0, 0.4), dt, (0.0, c.rayon * TRAME_LOUPE_MAX));
        } else {
            self.trame.vagues.clear();
            self.trame.loupe = Ressort::default();
        }

        if cfg.pinceau.actif {
            let (pi, c) = (&mut self.pinceau, &cfg.pinceau);
            if kicks > 0 && pi.perles.len() < 6 {
                pi.perles.push(t);
            }
            if pi.vie.x <= 0.0 {
                pi.vie = Ressort::en(c.duree);
            }
            if drop {
                pi.vie.v += 5.2 * c.duree;
            }
            pi.vie.vers(c.duree, (9.0, 0.45), dt, (c.duree * 0.5, c.duree * 2.2));
            pi.perles.retain(|&ne| t - ne < 0.5);
            pi.trajet.push_back((t, cursor.0, cursor.1));
            let (vie, loin) = (pi.vie.x as f64, PINCEAU_LOIN * scale);
            pi.trajet.retain(|q| t - q.0 <= vie && (q.1 - cursor.0).abs() <= loin && (q.2 - cursor.1).abs() <= loin);
        } else {
            self.pinceau.trajet.clear();
        }

        if cfg.pendule.actif && dt > 1e-5 {
            let c = &cfg.pendule;
            let saut = (self.hasard() * 2.0 - 1.0) * 70.0 * scale;
            let pe = &mut self.pendule;
            if pe.fil.x <= 0.0 {
                pe.fil = Ressort::en(c.longueur);
            }
            if drop {
                pe.fil.v += 7.3 * c.longueur;
            }
            pe.fil.vers(c.longueur * (1.0 + 0.32 * p.basse - 0.6 * p.montee), (12.0, 0.4), dt, (4.0, c.longueur * 2.0));
            let l = pe.fil.x * scale;
            let (mut qx, mut qy) = pe.poids.unwrap_or((cursor.0, cursor.1 + l));
            if kicks > 0 {
                pe.vitesse.1 -= PENDULE_SAUT.0 * scale;
                pe.vitesse.0 += saut;
            }
            if caisse {
                pe.cote = -pe.cote;
                pe.vitesse.0 += PENDULE_SAUT.1 * pe.cote * scale;
            }
            if drop {
                pe.vitesse.1 -= 1.5 * PENDULE_SAUT.0 * scale;
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

    /// Zone écran des figures actives : autour de `centre` (orbe, voix, trame), autour du pointeur
    /// (pendule) et le long du trajet du pointeur (trait).
    pub(super) fn rect(&self, centre: (f32, f32), cursor: (f32, f32), scale: f32, cfg: &Config) -> Option<IRect> {
        let mut e = 0.0f32;
        if cfg.orbe.actif {
            let c = &cfg.orbe;
            e = e.max((c.rayon + c.relief + ORBE_SOUFFLE.1 + 4.0) * (1.0 + ORBE_ECHO.1 * 1.5) + c.epaisseur + 1.0);
        }
        if cfg.voix.actif {
            let c = &cfg.voix;
            let large = SOUS_LE_POINTEUR.0 + c.largeur * VOIX_LARGE.1 / 2.0;
            let haut = SOUS_LE_POINTEUR.1 + c.hauteur * (1.4 + 1.0 + VOIX_ONDES.0 + VOIX_ONDES.1);
            e = e.max(large.max(haut) + c.epaisseur + 1.0);
        }
        if cfg.trame.actif {
            let c = &cfg.trame;
            let point = (1.9 + 2.0 * TRAME_ONDE_MAX + 1.5) * c.taille;
            e = e.max(self.trame.loupe.x.max(4.0) * 1.25 + TRAME_DEPLACE * TRAME_ONDE_MAX + point + 1.0);
        }
        let mut zone = (e > 0.0).then(|| autour(centre, e * scale));
        if cfg.pendule.actif {
            let c = &cfg.pendule;
            let fil = if self.pendule.fil.x > 0.0 { self.pendule.fil.x } else { c.longueur };
            let ancre = if self.pendule.poids.is_some() { self.pendule.ancre } else { cursor };
            zone = crate::layer::union(zone, Some(autour(ancre, (fil * 1.15 + c.taille + 2.8) * scale)));
        }
        if cfg.pinceau.actif && !self.pinceau.trajet.is_empty() {
            let demi = (0.6 + 1.2 * cfg.pinceau.largeur) * PINCEAU_PERLE.2 / 2.0 * scale + 2.0;
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for &(_, x, y) in &self.pinceau.trajet {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
            zone = crate::layer::union(zone, Some(((x0 - demi).floor() as i32, (y0 - demi).floor() as i32, (x1 + demi).ceil() as i32, (y1 + demi).ceil() as i32)));
        }
        zone
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dessiner(&self, p: &Partage, cv: &mut dyn Canvas, pts: &mut Vec<(f32, f32)>, t: f64, cfg: &Config, scale: f32, (tete, queue): (Rgb, Rgb)) {
        let (cx, cy) = p.centre;

        if cfg.trame.actif {
            let (tr, c) = (&self.trame, &cfg.trame);
            let (pas, l) = (c.pas * scale, tr.loupe.x.max(4.0) * scale);
            let marge = l * 1.25 + 1.0;
            let cases = |m: f32| (((m - marge) / pas).floor() as i32, ((m + marge) / pas).ceil() as i32);
            let ((i0, i1), (j0, j1)) = (cases(cx), cases(cy));
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
                    let eclair = tr.eclairs.iter().filter(|e| e.0 == i && e.1 == j).map(|e| (-(t - e.2) as f32 * 12.0).exp()).fold(0.0f32, f32::max);
                    x += px * TRAME_DEPLACE * scale - dx * 0.3 * p.montee * dedans;
                    y += py * TRAME_DEPLACE * scale - dy * 0.3 * p.montee * dedans;
                    let r = (dedans * (1.0 + 0.9 * p.basse) + 2.0 * onde * large + 1.5 * eclair * large) * c.taille * scale;
                    if r >= 0.25 {
                        cv.circle(x, y, r, tete, (0.3 + 0.35 * r / scale).min(1.0));
                    }
                }
            }
        }

        if cfg.orbe.actif && self.orbe.contour.len() == ORBE_POINTS {
            let c = &cfg.orbe;
            let mut tracer = |rayons: &[f32], echelle: f32, largeur: f32, rgb: Rgb, al: f32| {
                pts.clear();
                pts.extend((0..=ORBE_POINTS).map(|i| {
                    let (sin, cos) = (-PI / 2.0 + i as f32 / ORBE_POINTS as f32 * TAU).sin_cos();
                    let r = rayons[i % ORBE_POINTS] * echelle * scale;
                    (cx + cos * r, cy + sin * r)
                }));
                cv.polyline(pts, largeur, rgb, al, true);
            };
            for (ne, force, rayons) in &self.orbe.echos {
                let k = ((t - ne) / ORBE_ECHO.0) as f32;
                if (0.0..1.0).contains(&k) && rayons.len() == ORBE_POINTS {
                    tracer(rayons, 1.0 + ORBE_ECHO.1 * force * (1.0 - (1.0 - k).powi(3)), (1.3 * (1.0 - k) + 0.4) * scale, melange(tete, queue, k), 0.55 * (1.0 - k));
                }
            }
            tracer(&self.orbe.contour, 1.0, c.epaisseur * scale, tete, 1.0);
        }

        if cfg.voix.actif && self.voix.visible > 0.01 {
            let (v, c) = (&self.voix, &cfg.voix);
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

        if cfg.pinceau.actif && self.pinceau.trajet.len() >= 2 {
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
                    cv.line((a.1, a.2), (b.1, b.2), largeur, prisme(cfg, t, k), 1.0, true);
                }
            }
        }

        if let (true, Some((qx, qy))) = (cfg.pendule.actif, self.pendule.poids) {
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
            cv.circle(qx, qy, (c.taille + 1.8 * self.kick) * scale, tete, 1.0);
        }
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
        let plat = f.plus.orbe.contour.clone();
        assert!(plat.iter().all(|r| (r - cfg.orbe.rayon).abs() < 2.0), "{plat:?}");
        bandes[0] = 1.0;
        f.avance(1.0, 1.0 / 120.0, C, 1.0, &Son { au: &au, coups: &coups, bandes: &bandes }, &cfg);
        let (haut, bas) = (f.plus.orbe.contour[0], f.plus.orbe.contour[ORBE_POINTS / 2]);
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

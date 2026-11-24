//! Petites figures autour du pointeur, faites de lignes fines et de points qui laissent voir
//! l'écran : arcs (`[arcs]`), quatre temps (`[quatre_temps]`) et anneau suiveur (`[anneau]`) ici ;
//! orbe, ligne de voix, trame, trait de pinceau et pendule dans `figures/plus.rs`, gouttes en
//! contour dans `figures/gouttes.rs`.
//! `Effets` les possède, leur passe le son et ajoute leur zone à celle du pointeur.
//!
//! Tout ce qui bouge passe par des ressorts bornés (`Ressort::vers`) : la zone de `rect` contient
//! donc toujours le dessin. `ui/apercu.js` reproduit ces figures (mêmes formules) : les modifier
//! ici oblige à les reporter là.

use crate::audio::AudioView;
use crate::canvas::{Canvas, Rgb};
use crate::config::Config;
use crate::layer::IRect;
use crate::render::{prisme, ressort, Coups};
use std::f32::consts::{PI, TAU};

mod gouttes;
mod plus;

/// Ce que les figures écoutent pendant une image : le son, ses coups depuis l'image précédente
/// et les bandes de fréquences lissées (vide si personne ne les demande).
pub struct Son<'a> {
    pub au: &'a AudioView,
    pub coups: &'a Coups,
    pub bandes: &'a [f32],
}

/// Ce que les figures partagent pendant une image : leur centre, l'horloge des rotations lentes,
/// le niveau de la basse, la montée, les enveloppes du charleston et de la caisse claire.
#[derive(Clone, Copy)]
struct Partage {
    centre: (f32, f32),
    horloge: f32,
    basse: f32,
    montee: f32,
    charleston: f32,
    caisse: f32,
}

/// Centre des figures : au bout d'un ressort accroché au pointeur (pulsation en rad/s,
/// amortissement), jamais à plus de `RETARD_MAX` px (à 100 %) de lui.
const SUIVI: (f32, f32) = (14.0, 0.55);
const RETARD_MAX: f32 = 40.0;
/// Retombée des enveloppes du charleston et de la caisse claire (par seconde), suivi de la montée.
const RETOMBEE: (f32, f32, f32) = (10.0, 8.0, 6.0);
/// Présence du tempo : durée de son arrivée et de son effacement (s).
const TEMPO_FONDU: (f32, f32) = (0.15, 0.8);
/// Place des figures posées sous le pointeur (quatre temps), depuis la pointe de la flèche (px à 100 %).
const SOUS_LE_POINTEUR: (f32, f32) = (7.0, 27.0);

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

/// Anneau : bornes de son échelle, durée d'un écho (s) et ce dont il s'élargit, étirement au plus
/// et vitesse qui le donne (px/s à 100 %).
const ANNEAU_BORNES: (f32, f32) = (0.2, 1.7);
const ANNEAU_ECHO: (f64, f32) = (0.42, 1.2);
const ANNEAU_ETIRE: (f32, f32) = (0.45, 900.0);

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
    /// Niveaux affichés (0 à 1) et arcs fermés par un drop (1 → 0).
    grave: f32,
    aigu: f32,
    plein: f32,
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
    vague: f64,
}

struct Anneau {
    echelle: Ressort,
    carre: f32,
    echos: Vec<f64>,
}

pub struct Figures {
    /// Centre des figures et sa vitesse (px, px/s) ; `None` tant qu'aucune figure n'est active.
    centre: Option<(f32, f32)>,
    vitesse: (f32, f32),
    /// Horloge des rotations lentes (s, repliée).
    horloge: f32,
    /// Enveloppes du charleston et de la caisse claire (1 au coup, puis retombée), montée suivie.
    charleston: f32,
    caisse: f32,
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
    /// Points d'une ligne à tracer, gardés d'une image à l'autre (le dessin ne prend que `&self`).
    tampon: std::cell::RefCell<Vec<(f32, f32)>>,
}

impl Figures {
    pub fn new() -> Self {
        let point = Point { saut: Ressort::default(), taille: Ressort::en(1.0), tiret: Ressort::default(), arme: false };
        Self {
            centre: None,
            vitesse: (0.0, 0.0),
            horloge: 0.0,
            charleston: 0.0,
            caisse: 0.0,
            montee: 0.0,
            basse: 0.0,
            compte: 0,
            mesure: 0.0,
            tempo: 0.0,
            arcs: Arcs::default(),
            temps: QuatreTemps { pts: [point; 4], actif: None, vague: f64::NEG_INFINITY },
            anneau: Anneau { echelle: Ressort::en(1.0), carre: 0.0, echos: Vec::new() },
            plus: plus::Plus::new(),
            gouttes: gouttes::Gouttes::new(),
            tampon: Default::default(),
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

    /// Avance d'une image : le centre suit le pointeur, les coups du son poussent les ressorts.
    pub fn avance(&mut self, t: f64, dt: f32, cursor: (f32, f32), scale: f32, son: &Son, cfg: &Config) {
        let (au, coups) = (son.au, son.coups);
        if !Self::actives(cfg) {
            self.centre = None;
            self.plus.oublier();
            self.gouttes.oublier();
            return;
        }
        // centre : au bout d'un ressort accroché au pointeur, retard borné
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
        self.horloge = (self.horloge + dt).rem_euclid(TAU * 40.0);
        // ce que les figures écoutent
        let retombe = |v: f32, coup: bool, taux: f32| if coup { 1.0 } else { v * (-dt * taux).exp() };
        self.charleston = retombe(self.charleston, coups.aigus > 0, RETOMBEE.0);
        self.caisse = retombe(self.caisse, coups.caisses > 0, RETOMBEE.1);
        self.montee += (au.tension.clamp(0.0, 1.0) - self.montee) * (dt * RETOMBEE.2).min(1.0);
        let basse = au.bass.clamp(0.0, 1.0);
        self.basse += (basse - self.basse) * (1.0 - (-dt * 10.0).exp());
        let (kicks, caisse, drop) = (coups.kicks, coups.caisses > 0, coups.drops > 0);
        // tempo : les temps se comptent depuis le dernier drop ; sans tempo régulier, ce qui le
        // montre s'efface
        if coups.temps {
            self.compte = self.compte.wrapping_add(1);
        }
        if drop {
            self.compte = 0;
        }
        self.tempo = if au.bpm.is_some() { (self.tempo + dt / TEMPO_FONDU.0).min(1.0) } else { (self.tempo - dt / TEMPO_FONDU.1).max(0.0) };
        if au.bpm.is_some() {
            self.mesure = ((self.compte % 4) as f32 + au.phase.clamp(0.0, 1.0)) / 4.0;
        }
        let partage = self.partage(cursor);
        self.plus.avance(&partage, t, dt, cursor, scale, son, cfg);
        self.gouttes.avance(&partage, t, dt, son, cfg);

        if cfg.arcs.actif {
            let a = &mut self.arcs;
            for _ in 0..kicks {
                a.cran += ARC_CRAN;
                a.bond.v += ARC_BOND;
            }
            if caisse {
                a.recul.v -= ARC_RECUL;
            }
            if drop {
                a.plein = 1.0;
            }
            a.plein *= (-dt * 1.6).exp();
            // l'angle reste petit : le cran et l'arc reculent ensemble d'un tour
            if a.cran > TAU {
                a.cran -= TAU;
                a.tour.x -= TAU;
            }
            a.tour.vers(a.cran, (14.0, 0.4), dt, (-TAU, 2.0 * TAU));
            a.bond.vers(0.0, (18.0, 0.3), dt, ARC_BORNES);
            a.recul.vers(0.0, (12.0, 0.35), dt, (-PI, PI));
            let aigu = (0.9 * self.charleston + 0.8 * self.montee + 0.45 * self.caisse).min(1.0).max(a.plein);
            a.grave = self.basse.max(a.plein);
            a.aigu += (aigu - a.aigu) * (1.0 - (-dt * if aigu > a.aigu { 30.0 } else { 6.0 }).exp());
        }

        if cfg.quatre_temps.actif {
            let q = &mut self.temps;
            let actif = au.bpm.map(|_| (self.compte % 4) as usize);
            if actif != q.actif {
                q.actif = actif;
                if let Some(i) = actif {
                    q.pts[i].taille.v += 9.0;
                }
            }
            if let Some(p) = q.actif.map(|i| &mut q.pts[i]) {
                p.saut.v -= 150.0 * kicks.min(1) as f32;
                if caisse {
                    p.tiret.v += 60.0;
                }
            }
            if drop {
                q.vague = t;
                q.pts.iter_mut().for_each(|p| p.arme = true);
            }
            for (i, p) in q.pts.iter_mut().enumerate() {
                if p.arme && t >= q.vague + i as f64 * VAGUE {
                    p.saut.v -= 230.0;
                    p.arme = false;
                }
                p.saut.vers(0.0, (20.0, 0.3), dt, SAUT_BORNES);
                p.taille.vers(1.0, (18.0, 0.35), dt, TAILLE_BORNES);
                p.tiret.vers(0.0, (16.0, 0.4), dt, TIRET_BORNES);
            }
        }

        if cfg.anneau.actif {
            let a = &mut self.anneau;
            a.echelle.v += 5.5 * kicks as f32;
            if caisse {
                a.carre = 1.0;
            }
            if drop {
                a.echelle.v += 10.0;
                a.echos.push(t);
            }
            a.carre *= (-dt * 6.0).exp();
            a.echelle.vers(1.0 - 0.42 * self.montee, (18.0, 0.28), dt, ANNEAU_BORNES);
            a.echos.retain(|&ne| t - ne < ANNEAU_ECHO.0);
        }
    }

    /// Ce que les figures partagent à cette image (leur centre : `cursor` avant la première).
    fn partage(&self, cursor: (f32, f32)) -> Partage {
        Partage { centre: self.centre.unwrap_or(cursor), horloge: self.horloge, basse: self.basse, montee: self.montee, charleston: self.charleston, caisse: self.caisse }
    }

    /// Zone écran qui contient les figures (autour de leur centre, ou de `cursor` avant la
    /// première image) ; `None` si aucune n'est active.
    pub fn rect(&self, cursor: (f32, f32), scale: f32, cfg: &Config) -> Option<IRect> {
        let mut e = 0.0f32;
        if cfg.arcs.actif {
            e = e.max(cfg.arcs.rayon + ARC_BORNES.1 + cfg.arcs.epaisseur);
        }
        if cfg.quatre_temps.actif {
            let q = &cfg.quatre_temps;
            let r = q.taille * TAILLE_BORNES.1 * POINT_ACTIF;
            let large = SOUS_LE_POINTEUR.0 + 1.5 * q.ecart + TIRET_BORNES.1 * 1.1 + r + 1.0;
            let haut = SOUS_LE_POINTEUR.1 + SAUT_BORNES.1.max(-SAUT_BORNES.0) + r;
            e = e.max(large).max(haut);
        }
        if cfg.anneau.actif {
            let a = &cfg.anneau;
            e = e.max(a.rayon * ANNEAU_BORNES.1 * (1.0 + ANNEAU_ECHO.1) + 4.0 * a.epaisseur + 4.0);
        }
        let zone = (e > 0.0).then(|| autour(self.centre.unwrap_or(cursor), e * scale));
        let centre = self.centre.unwrap_or(cursor);
        let zone = crate::layer::union(zone, self.gouttes.rect(centre, scale, cfg));
        crate::layer::union(zone, self.plus.rect(centre, cursor, scale, cfg))
    }

    /// Dessine les figures actives autour de leur centre.
    pub fn dessiner(&self, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32) {
        let Some((cx, cy)) = self.centre else { return };
        // deux couleurs : celle des têtes de l'essaim et celle des queues de traînée
        let (tete, queue) = (prisme(cfg, t, 1.0), prisme(cfg, t, 0.0));
        let mut pts = self.tampon.borrow_mut();
        self.gouttes.dessiner(&self.partage((cx, cy)), cv, cfg, scale, (tete, queue));
        self.plus.dessiner(&self.partage((cx, cy)), cv, &mut pts, t, cfg, scale, (tete, queue));

        if cfg.arcs.actif {
            let (a, c) = (&self.arcs, &cfg.arcs);
            let mut arc = |r: f32, depart: f32, ouverture: f32, largeur: f32, rgb| {
                let r = r.max(2.0);
                let n = ((ouverture * r / (3.0 * scale)).ceil() as usize).clamp(2, 96);
                pts.clear();
                pts.extend((0..=n).map(|i| {
                    let (sin, cos) = (depart + ouverture * i as f32 / n as f32).sin_cos();
                    (cx + cos * r, cy + sin * r)
                }));
                cv.polyline(&pts, largeur, rgb, 1.0, true);
            };
            let grand = ARC_GRAND_OUVERTURE.0 + ARC_GRAND_OUVERTURE.1 * a.grave.clamp(0.0, 1.0);
            let petit = ARC_PETIT_OUVERTURE.0 + ARC_PETIT_OUVERTURE.1 * a.aigu.clamp(0.0, 1.0);
            arc((c.rayon + a.bond.x) * scale, -PI / 2.0 + a.tour.x + self.horloge * 0.25, grand, c.epaisseur * scale, tete);
            arc((c.rayon * ARC_PETIT.0 + a.bond.x * 0.5) * scale, PI / 2.0 + a.recul.x - self.horloge * 0.4, petit, c.epaisseur * ARC_PETIT.1 * scale, queue);
        }

        if cfg.quatre_temps.actif && self.tempo > 0.01 {
            let (q, c) = (&self.temps, &cfg.quatre_temps);
            let (ax, ay) = (cx + SOUS_LE_POINTEUR.0 * scale, cy + SOUS_LE_POINTEUR.1 * scale);
            let ecart = c.ecart * scale * (1.0 - 0.35 * self.montee);
            for (i, p) in q.pts.iter().enumerate() {
                let actif = q.actif == Some(i);
                let x = ax + (i as f32 - 1.5) * ecart + (self.horloge * 40.0 + i as f32 * 2.0).sin() * 0.8 * self.montee * scale;
                let y = ay + p.saut.x * scale;
                let r = c.taille * scale * p.taille.x * if actif { POINT_ACTIF } else { 1.0 };
                let l = p.tiret.x.max(0.0) * 1.1 * scale;
                let al = self.tempo * if actif { 1.0 } else { POINT_ETEINT };
                if l > 0.3 {
                    cv.line((x - l, y), (x + l, y), 2.0 * r, tete, al, true);
                } else {
                    cv.circle(x, y, r, tete, al);
                }
            }
        }

        if cfg.anneau.actif {
            let (a, c) = (&self.anneau, &cfg.anneau);
            let r = c.rayon * scale * a.echelle.x;
            for &ne in &a.echos {
                let k = ((t - ne) / ANNEAU_ECHO.0) as f32;
                if (0.0..1.0).contains(&k) {
                    cv.ring(cx, cy, r * (1.0 + ANNEAU_ECHO.1 * (1.0 - (1.0 - k).powi(3))), 1.2 * scale, queue, 0.7 * (1.0 - k));
                }
            }
            // étiré dans le sens de sa vitesse ; carré un instant sur la caisse claire
            // (superellipse : un cercle à l'exposant 2, presque un carré à 8)
            let v = (self.vitesse.0.powi(2) + self.vitesse.1.powi(2)).sqrt();
            let e = (v / (ANNEAU_ETIRE.1 * scale)).min(ANNEAU_ETIRE.0);
            let (sin, cos) = if v > 1.0 { (self.vitesse.1 / v, self.vitesse.0 / v) } else { (0.0, 1.0) };
            let (ra, rb, puissance) = (r * (1.0 + e), r * (1.0 - e), 2.0 / (2.0 + 6.0 * a.carre.clamp(0.0, 1.0)));
            let point = |angle: f32| {
                let (st, ct) = angle.sin_cos();
                let (x, y) = (ra * ct.signum() * ct.abs().powf(puissance), rb * st.signum() * st.abs().powf(puissance));
                (cx + x * cos - y * sin, cy + x * sin + y * cos)
            };
            pts.clear();
            pts.extend((0..=48).map(|i| point(i as f32 / 48.0 * TAU)));
            // le trait s'épaissit avec la basse et la montée (au plus trois fois l'épaisseur réglée)
            cv.polyline(&pts, c.epaisseur * (1.0 + 1.07 * self.basse + 0.86 * self.montee) * scale, tete, 1.0, true);
            // le point du tempo : un tour par mesure de quatre temps, plus gros sur le charleston
            if self.tempo > 0.01 {
                let (x, y) = point(-PI / 2.0 + self.mesure * TAU);
                cv.circle(x, y, (1.9 + 1.9 * self.charleston) * scale, queue, self.tempo);
            }
        }
    }
}

/// Carré écran de demi-côté `e` px (plus 2 px de marge) autour de `centre`.
fn autour((cx, cy): (f32, f32), e: f32) -> IRect {
    let e = e + 2.0;
    ((cx - e).floor() as i32, (cy - e).floor() as i32, (cx + e).ceil() as i32, (cy + e).ceil() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    /// Avance d'une image, sans bandes de fréquences.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn avancer(f: &mut Figures, t: f64, dt: f32, cursor: (f32, f32), scale: f32, au: &AudioView, coups: &Coups, cfg: &Config) {
        f.avance(t, dt, cursor, scale, &Son { au, coups, bandes: &[] }, cfg);
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
        for (scale, dt) in [(1.0f32, 1.0 / 120.0), (1.5, 1.0 / 20.0)] {
            let mut f = Figures::new();
            let (mut t, mut peints) = (0.0f64, 0usize);
            for i in 0..(4.0 / dt) as usize {
                let cursor = (700.0 + 300.0 * (t as f32 * 5.0).sin(), 700.0 + 120.0 * (t as f32 * 3.0).cos());
                let (au, mut coups) = son(t);
                coups.kicks *= 4; // quatre kicks dans la même image : les ressorts restent bornés
                f.avance(t, dt, cursor, scale, &Son { au: &au, coups: &coups, bandes: &[1.0; 16] }, &cfg);
                if i % 7 == 0 {
                    let (x0, y0, x1, y1) = f.rect(cursor, scale, &cfg).unwrap();
                    let mut pm = Pixmap::new(1600, 1600).unwrap();
                    f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, &cfg, scale);
                    for (j, p) in pm.data().chunks_exact(4).enumerate() {
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
        assert!(pm.data().chunks_exact(4).filter(|p| p[3] > 100).count() > 60);
        for (dx, dy) in [(0i32, 0i32), (2, 2), (-2, 1)] {
            assert_eq!(pm.pixel((300 + dx) as u32, (300 + dy) as u32).unwrap().alpha(), 0);
        }
    }
}

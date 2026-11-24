//! Figures loin du pointeur : équerres aux coins de la fenêtre au premier plan (`[equerres]`),
//! halo le long de son cadre (`[halo]`) et liseré au bord bas de l'écran du pointeur (`[lisere]`). `Effets` les possède, leur passe le son,
//! le cadre de la fenêtre et l'écran ; `main.rs` ajoute leurs zones à celles des fissures.
//!
//! C'est un habillage de la fenêtre et de l'écran, comme la bordure : hors des presets
//! (`profils::in_profile`), il accompagne l'essaim quel que soit le preset, et reste affiché quand
//! l'essaim ne l'est pas (pointeur masqué par la frappe, veille profonde : `main::habillage_seul`).
//!
//! Le moteur redessine, par écran, un seul rectangle qui englobe toutes les zones de l'image : une
//! zone loin du pointeur l'agrandit d'autant. Ces figures ne donnent donc leurs zones que quand
//! leur dessin change (`zones`) ; le reste du temps, ce qui est affiché reste en place. Quand la
//! boucle principale efface les calques, elle le dit (`oublier`).
//!
//! `ui/apercu.js` les reproduit dans le cadre de l'aperçu : les modifier ici oblige à les reporter là.

use crate::canvas::{Canvas, Rgb};
use crate::config::Config;
use crate::figures::{gauss, melange, Ressort, Son};
use crate::layer::IRect;
use crate::render::prisme;
use std::f32::consts::TAU;

/// Équerres : écart au cadre au repos (px à 100 %), élan que lui donnent le kick et le drop
/// (px/s), ce que la basse ajoute aux bras (en longueurs), élan des bras au drop (px/s), bornes
/// de l'écart et des bras (px à 100 %, les bras en longueurs réglées).
const EQUERRE_ECART: f32 = 4.0;
const EQUERRE_ELAN: (f32, f32) = (110.0, 260.0);
const EQUERRE_BASSE: f32 = 1.3;
const EQUERRE_ELAN_BRAS: f32 = 38.0;
const EQUERRE_BORNES: ((f32, f32), (f32, f32)) = ((0.0, 26.0), (0.4, 3.2));
/// Écart de couleur (par composante, sur 255) à partir duquel les équerres sont redessinées :
/// une teinte qui glisse ne redemande pas leurs zones à chaque image.
const EQUERRE_NUANCE: u8 = 8;
/// Les équerres au repos redonnent leurs zones à cet intervalle (s) : un calque resté dix secondes
/// sans zone est retiré (`backend::KEEP_EMPTY`), et un calque refait par le moteur (écrans changés,
/// carte graphique perdue) est vide.
const EQUERRE_RAPPEL: f32 = 3.0;

/// Filet de lumière (liseré, halo) : pas d'un segment (px à 100 %), crans de force et de teinte
/// (les pas voisins d'un même cran sont tracés d'un seul trait), lueur près du pointeur au
/// repos et ce que la basse y ajoute, part d'un drop qui embrase tout le filet et d'une montée,
/// éclat d'un kick et d'un drop (force, durée en s, vitesse en px/s à 100 %), largeur d'un éclat
/// (px à 100 %), éclats gardés, silence attendu avant l'effacement (s), durée du retour et de
/// l'effacement (s).
const FILET_PAS: f32 = 6.0;
const FILET_CRANS: f32 = 16.0;
const FILET_LUEUR: (f32, f32) = (0.1, 0.75);
const FILET_PLEIN: (f32, f32) = (0.8, 0.25);
const FILET_KICK: (f32, f32, f32) = (1.0, 0.6, 560.0);
const FILET_DROP: (f32, f32, f32) = (2.0, 1.1, 760.0);
const FILET_ECLAT: f32 = 40.0;
const FILET_ECLATS_MAX: usize = 8;
const FILET_ATTENTE: f32 = 1.5;
const FILET_FONDU: (f32, f32) = (0.15, 0.8);
/// Liseré : demi-largeur de la lueur sous le pointeur (px à 100 %, puis part de la largeur de
/// l'écran que la basse y ajoute).
const LISERE_LUEUR: (f32, f32) = (36.0, 0.26);
/// Halo : demi-largeur de la lueur près du pointeur (part du tour du cadre), ce qu'ajoute un coup
/// de caisse claire, nappes de lueur vers l'intérieur et opacité au bord du cadre, débord du dessin
/// hors du cadre (px).
const HALO_LUEUR: f32 = 0.11;
const HALO_FRISSON: f32 = 0.2;
const HALO_NAPPES: usize = 4;
const HALO_OPACITE: f32 = 0.45;
const HALO_DEBORD: f32 = 4.0;

/// Deux couleurs que l'œil ne distingue pas sur un trait fin.
fn proches(a: Rgb, b: Rgb) -> bool {
    [(a.0, b.0), (a.1, b.1), (a.2, b.2)].iter().all(|&(x, y)| x.abs_diff(y) < EQUERRE_NUANCE)
}

/// Ce qui est dessiné des équerres : cadre de la fenêtre, écart et bras (px écran), opacité des
/// deux diagonales, couleur. Deux images au même dessin ne redemandent pas leurs zones.
#[derive(Clone, Copy, PartialEq)]
struct Equerres {
    cadre: (i32, i32, i32, i32),
    ecart: f32,
    bras: f32,
    opacites: [f32; 2],
    rgb: Rgb,
    epaisseur: f32,
}

impl Equerres {
    /// Les quatre coins : point du cadre, sens vers l'extérieur, diagonale (0 ou 1).
    fn coins(&self) -> [(f32, f32, f32, f32, usize); 4] {
        let (l, t, r, b) = (self.cadre.0 as f32, self.cadre.1 as f32, self.cadre.2 as f32, self.cadre.3 as f32);
        [(l, t, -1.0, -1.0, 0), (r, t, 1.0, -1.0, 1), (r, b, 1.0, 1.0, 0), (l, b, -1.0, 1.0, 1)]
    }

    /// Zone d'un coin : l'équerre, écartée du cadre, et ses deux bras le long du cadre.
    fn zones(&self) -> [IRect; 4] {
        let e = self.ecart.max(self.bras) + self.epaisseur + 2.0;
        self.coins().map(|(x, y, ..)| ((x - e).floor() as i32, (y - e).floor() as i32, (x + e).ceil() as i32, (y + e).ceil() as i32))
    }

    fn dessiner(&self, cv: &mut dyn Canvas) {
        for (x, y, sx, sy, diag) in self.coins() {
            let (px, py) = (x + sx * self.ecart, y + sy * self.ecart);
            cv.polyline(&[(px - sx * self.bras, py), (px, py), (px, py - sy * self.bras)], self.epaisseur, self.rgb, self.opacites[diag], true);
        }
    }
}

/// Éclat qui court le long d'un filet, des deux côtés de l'abscisse `u` où il est né.
struct Eclat {
    u: f32,
    ne: f64,
    force: f32,
    vie: f32,
    vitesse: f32,
}

/// Lumière qui court le long d'un filet : le bord bas de l'écran (liseré) ou le cadre de la
/// fenêtre (halo). Une lueur près du pointeur, que la basse élargit ; des éclats qui partent de
/// là au kick ; tout le filet qui s'allume au drop. Sans son, le filet s'efface.
struct Filet {
    eclats: Vec<Eclat>,
    embrase: f32,
    silence: f32,
    presence: f32,
}

impl Filet {
    /// Silencieux au départ : le filet n'apparaît qu'avec le son.
    fn new() -> Self {
        Self { eclats: Vec::new(), embrase: 0.0, silence: FILET_ATTENTE, presence: 0.0 }
    }

    /// Avance d'une image. `u` : abscisse du pointeur sur le filet, d'où partent les éclats ;
    /// `son` : quelque chose s'entend (basse, montée).
    fn avance(&mut self, t: f64, dt: f32, u: f32, kick: bool, drop: bool, son: bool) {
        let eclat = |(force, vie, vitesse): (f32, f32, f32)| Eclat { u, ne: t, force, vie, vitesse };
        if kick {
            self.eclats.push(eclat(FILET_KICK));
        }
        if drop {
            self.eclats.push(eclat(FILET_DROP));
            self.embrase = 1.0;
        }
        self.eclats.retain(|e| t - e.ne < e.vie as f64);
        let trop = self.eclats.len().saturating_sub(FILET_ECLATS_MAX);
        self.eclats.drain(..trop);
        self.embrase *= (-dt * 1.5).exp();
        let la = son || !self.eclats.is_empty() || self.embrase > 0.02;
        self.silence = if la { 0.0 } else { self.silence + dt };
        self.presence = if self.silence >= FILET_ATTENTE { (self.presence - dt / FILET_FONDU.1).max(0.0) } else { (self.presence + dt / FILET_FONDU.0).min(1.0) };
    }

    /// Force de la lumière en un point du filet (avant sa présence) : `au_pointeur` = sa distance
    /// à l'aplomb du pointeur, `lueur` = demi-largeur de la lueur du repos, `ecart(u)` = sa
    /// distance à l'abscisse `u`, le long du filet (px).
    fn force(&self, t: f64, scale: f32, (basse, montee): (f32, f32), lueur: f32, au_pointeur: f32, ecart: impl Fn(f32) -> f32) -> f32 {
        let mut f = (FILET_LUEUR.0 + FILET_LUEUR.1 * basse) * gauss(au_pointeur / lueur) + FILET_PLEIN.0 * self.embrase + FILET_PLEIN.1 * montee;
        for e in &self.eclats {
            let age = (t - e.ne) as f32;
            f += e.force * (1.0 - age / e.vie).max(0.0).powf(1.2) * gauss((ecart(e.u) - e.vitesse * scale * age) / (FILET_ECLAT * scale));
        }
        f
    }
}

/// Cadre d'une fenêtre (gauche, haut, droite, bas), parcouru dans le sens des aiguilles d'une
/// montre depuis son coin haut gauche : l'abscisse `u` d'un point est le chemin fait jusqu'à lui.
#[derive(Clone, Copy, PartialEq)]
struct Cadre(f32, f32, f32, f32);

impl Cadre {
    fn tour(&self) -> f32 {
        2.0 * ((self.2 - self.0) + (self.3 - self.1))
    }

    /// Les quatre côtés : départ, direction, longueur, sens vers l'intérieur, abscisse du départ.
    #[allow(clippy::type_complexity)]
    fn cotes(&self) -> [((f32, f32), (f32, f32), f32, (f32, f32), f32); 4] {
        let (l, t, r, b) = (self.0, self.1, self.2, self.3);
        let (w, h) = (r - l, b - t);
        [((l, t), (1.0, 0.0), w, (0.0, 1.0), 0.0), ((r, t), (0.0, 1.0), h, (-1.0, 0.0), w), ((r, b), (-1.0, 0.0), w, (0.0, -1.0), w + h), ((l, b), (0.0, -1.0), h, (1.0, 0.0), 2.0 * w + h)]
    }

    /// Abscisse du point du cadre le plus proche de `p`.
    fn proche(&self, p: (f32, f32)) -> f32 {
        let (x, y) = (p.0.clamp(self.0, self.2), p.1.clamp(self.1, self.3));
        let (w, h) = (self.2 - self.0, self.3 - self.1);
        // distance aux côtés haut, droit, bas, gauche, et abscisse de l'aplomb sur chacun
        let cotes = [(y - self.1, x - self.0), (self.2 - x, w + y - self.1), (self.3 - y, w + h + self.2 - x), (x - self.0, 2.0 * w + h + self.3 - y)];
        cotes.iter().fold((f32::INFINITY, 0.0), |m, &c| if c.0 < m.0 { c } else { m }).1
    }

    /// Chemin le plus court entre deux abscisses, le long du cadre.
    fn ecart(&self, a: f32, b: f32) -> f32 {
        let (p, d) = (self.tour(), (a - b).abs());
        let d = d % p;
        d.min(p - d)
    }

    /// Bandes de `e` px le long des quatre côtés, vers l'intérieur, débord compris.
    fn bandes(&self, e: f32) -> [IRect; 4] {
        let (l, t, r, b) = ((self.0 - HALO_DEBORD).floor() as i32, (self.1 - HALO_DEBORD).floor() as i32, (self.2 + HALO_DEBORD).ceil() as i32, (self.3 + HALO_DEBORD).ceil() as i32);
        let e = (e + HALO_DEBORD).ceil() as i32;
        [(l, t, r, t + e), (r - e, t, r, b), (l, b - e, r, b), (l, t, l + e, b)]
    }
}

/// Halo affiché : cadre de la fenêtre, abscisse du pointeur sur ce cadre, zones redessinées.
#[derive(Clone, Copy)]
struct Halo {
    cadre: Cadre,
    u: f32,
    zones: [IRect; 4],
}

pub struct Loin {
    /// Enveloppe du kick, niveau de la basse lissé, montée suivie.
    kick: f32,
    basse: f32,
    montee: f32,
    // équerres
    ecart: Ressort,
    bras: Ressort,
    diagonales: [f32; 2],
    tour: usize,
    equerres: Option<Equerres>,
    /// Temps depuis la dernière demande des zones des équerres.
    rappel: f32,
    // halo : sa lumière, le frisson d'une caisse claire, ce qui est affiché
    halo: Filet,
    frisson: f32,
    halo_affiche: Option<Halo>,
    // liseré : sa lumière ; bande (écran) et abscisse du pointeur, `None` s'il n'y a rien à dessiner
    lisere: Filet,
    bande: Option<(IRect, f32)>,
    /// Zones à redessiner à cette image.
    zones: Vec<IRect>,
}

impl Loin {
    pub fn new() -> Self {
        Self {
            kick: 0.0,
            basse: 0.0,
            montee: 0.0,
            ecart: Ressort::en(EQUERRE_ECART),
            bras: Ressort::en(1.0),
            diagonales: [0.0; 2],
            tour: 0,
            equerres: None,
            rappel: 0.0,
            halo: Filet::new(),
            frisson: 0.0,
            halo_affiche: None,
            lisere: Filet::new(),
            bande: None,
            zones: Vec::new(),
        }
    }

    pub fn actives(cfg: &Config) -> bool {
        cfg.equerres.actif || cfg.halo.actif || cfg.lisere.actif
    }

    /// Quelque chose est-il affiché ? (Après l'extinction d'un effet, une dernière image l'efface.)
    pub fn affiche(&self) -> bool {
        self.equerres.is_some() || self.halo_affiche.is_some() || self.bande.is_some()
    }

    /// Les calques viennent d'être effacés : plus rien n'est affiché, la prochaine image redonne
    /// toutes les zones.
    pub fn oublier(&mut self) {
        (self.equerres, self.halo_affiche, self.bande) = (None, None, None);
    }

    /// Avance d'une image. `fenetre` : cadre de la fenêtre au premier plan (gauche, haut, droite,
    /// bas) ; `ecran` : celui du pointeur.
    #[allow(clippy::too_many_arguments)]
    pub fn avance(&mut self, t: f64, dt: f32, cursor: (f32, f32), scale: f32, son: &Son, cfg: &Config, fenetre: Option<(f32, f32, f32, f32)>, ecran: Option<IRect>) {
        self.zones.clear();
        let (kicks, caisse, drop) = (son.coups.kicks, son.coups.caisses > 0, son.coups.drops > 0);
        self.kick = if kicks > 0 { 1.0 } else { self.kick * (-dt * 7.0).exp() };
        self.basse += (son.au.bass.clamp(0.0, 1.0) - self.basse) * (1.0 - (-dt * 10.0).exp());
        self.montee += (son.au.tension.clamp(0.0, 1.0) - self.montee) * (dt * 6.0).min(1.0);

        // équerres : les ressorts avancent, puis le dessin de l'image est comparé au précédent
        let avant = self.equerres;
        self.equerres = None;
        if cfg.equerres.actif {
            let c = &cfg.equerres;
            if kicks > 0 {
                self.ecart.v += EQUERRE_ELAN.0;
            }
            if caisse {
                self.tour ^= 1;
                self.diagonales[self.tour] = 1.0;
            }
            if drop {
                self.ecart.v += EQUERRE_ELAN.1;
                self.bras.v += EQUERRE_ELAN_BRAS;
            }
            self.diagonales = self.diagonales.map(|d| d * (-dt * 8.0).exp());
            let tremble = (t * 44.0).sin() as f32 * 0.9 * self.montee;
            self.ecart.vers(EQUERRE_ECART * (1.0 - 0.75 * self.montee) + tremble, (17.0, 0.3), dt, EQUERRE_BORNES.0);
            self.bras.vers(1.0 + EQUERRE_BASSE * self.basse, (14.0, 0.4), dt, EQUERRE_BORNES.1);
            if let Some((l, h, r, b)) = fenetre {
                // quantifiés : un dessin qui ne change pas à l'œil ne redemande pas ses zones
                let q = |v: f32, pas: f32| (v / pas).round() * pas;
                let opacite = |d: f32| q((0.5 + 0.5 * d.max(self.kick)).clamp(0.0, 1.0), 1.0 / 32.0);
                // la couleur dessinée est gardée tant que celle de l'essaim en reste proche
                let rgb = prisme(cfg, t, 1.0);
                let rgb = avant.map(|a| a.rgb).filter(|&a| proches(a, rgb)).unwrap_or(rgb);
                self.equerres = Some(Equerres {
                    cadre: (l.round() as i32, h.round() as i32, r.round() as i32, b.round() as i32),
                    ecart: q(self.ecart.x * scale, 0.25),
                    bras: q(c.bras * self.bras.x * scale, 0.25),
                    opacites: self.diagonales.map(opacite),
                    rgb,
                    epaisseur: c.epaisseur * scale,
                });
            }
        }
        self.rappel += dt;
        if self.equerres != avant {
            // anciennes zones (à effacer) et nouvelles
            self.zones.extend(avant.iter().chain(&self.equerres).flat_map(Equerres::zones));
        } else if let (Some(e), true) = (&self.equerres, self.rappel >= EQUERRE_RAPPEL) {
            self.zones.extend(e.zones());
        }
        if !self.zones.is_empty() {
            self.rappel = 0.0;
        }

        let son_la = self.basse > 0.02 || self.montee > 0.02;

        // halo : la lumière court le long du cadre de la fenêtre, tant qu'il y a du son ; ses
        // quatre bandes sont redonnées à chaque image, et les anciennes quand la fenêtre change
        let avant = self.halo_affiche.take();
        if cfg.halo.actif {
            let cadre = fenetre.map(|(l, h, r, b)| Cadre(l.round(), h.round(), r.round(), b.round()));
            let u = cadre.map_or(0.0, |c| c.proche(cursor));
            self.frisson = if caisse { 1.0 } else { self.frisson * (-dt * 12.0).exp() };
            self.halo.avance(t, dt, u, kicks > 0, drop, son_la || self.frisson > 0.02);
            if let (Some(cadre), true) = (cadre, self.halo.presence > 0.0) {
                let c = &cfg.halo;
                self.halo_affiche = Some(Halo { cadre, u, zones: cadre.bandes((c.profondeur * (1.0 + 1.0 / HALO_NAPPES as f32) + 2.0 * c.epaisseur) * scale) });
            }
        } else {
            (self.halo, self.frisson) = (Filet::new(), 0.0);
        }
        self.zones.extend(self.halo_affiche.iter().flat_map(|h| h.zones));
        if let Some(a) = avant {
            if self.halo_affiche.map(|h| h.zones) != Some(a.zones) {
                self.zones.extend(a.zones);
            }
        }

        // liseré : une bande au bord bas de l'écran du pointeur, tant qu'il y a du son
        let avant = self.bande;
        self.bande = None;
        if cfg.lisere.actif {
            self.lisere.avance(t, dt, cursor.0, kicks > 0, drop, son_la);
            if let (Some(e), true) = (ecran, self.lisere.presence > 0.0) {
                let haut = (cfg.lisere.epaisseur * scale).ceil() as i32 + 2;
                self.bande = Some(((e.0, e.3 - haut, e.2, e.3), cursor.0));
            }
        } else {
            self.lisere = Filet::new();
        }
        // la bande change à chaque image tant qu'elle est là ; quand elle disparaît ou change
        // d'écran, l'ancienne est effacée
        self.zones.extend(self.bande.map(|b| b.0));
        if let Some((zone, _)) = avant {
            if self.bande.map(|b| b.0) != Some(zone) {
                self.zones.push(zone);
            }
        }
    }

    /// Zones à redessiner à cette image : vides tant que le dessin ne change pas.
    pub fn zones(&self) -> &[IRect] {
        &self.zones
    }

    /// Dessine les équerres, le halo et le liseré (à chaque image : ce qui tombe hors des zones
    /// redessinées est rogné par le moteur).
    pub fn dessiner(&self, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32) {
        if let Some(e) = &self.equerres {
            e.dessiner(cv);
        }
        if self.halo_affiche.is_none() && self.bande.is_none() {
            return;
        }
        // filets : la teinte va de celle des têtes à celle des queues le long du filet, et glisse
        let (tete, queue) = (prisme(cfg, t, 1.0), prisme(cfg, t, 0.0));
        let teinte = |part: f32| 0.5 + 0.5 * (part * TAU + (t as f32 * 0.5).rem_euclid(TAU)).sin();
        let (son, pas) = ((self.basse, self.montee), FILET_PAS * scale);
        if let Some(Halo { cadre, u, .. }) = self.halo_affiche {
            let c = &cfg.halo;
            let (ep, tour) = (c.epaisseur * scale, cadre.tour());
            for (depart, dir, long, dedans, u0) in cadre.cotes() {
                // point du côté à `s` px de son départ, rentré de `r` px dans la fenêtre
                let point = |s: f32, r: f32| (depart.0 + dir.0 * s + dedans.0 * r, depart.1 + dir.1 * s + dedans.1 * r);
                let niveau = |s: f32| {
                    let m = u0 + s;
                    let f = self.halo.force(t, scale, son, HALO_LUEUR * tour, cadre.ecart(m, u), |v| cadre.ecart(m, v)) + HALO_FRISSON * self.frisson;
                    (f * self.halo.presence, teinte(m / tour))
                };
                suites(long, pas, niveau, |a, b, f, k| {
                    let rgb = melange(tete, queue, k);
                    cv.bande(point(a, ep / 2.0), point(b, ep / 2.0), ep, rgb, (0.15 + f).min(1.0));
                    // nappes de lueur vers l'intérieur, de plus en plus pâles (toujours aux mêmes
                    // profondeurs : seule leur opacité suit la lumière, sans marche le long du cadre)
                    let fond = c.profondeur * scale;
                    for n in 0..HALO_NAPPES {
                        let r = (n as f32 + 0.5) / HALO_NAPPES as f32;
                        // coupée en onglet aux coins : les nappes de deux côtés ne s'y superposent pas
                        let rentre = ep + r * fond;
                        let (a, b) = (a.max(rentre), b.min(long - rentre));
                        if b > a {
                            cv.bande(point(a, rentre), point(b, rentre), fond / HALO_NAPPES as f32 + 1.0, rgb, HALO_OPACITE * f * (1.0 - r) * (1.0 - r));
                        }
                    }
                });
            }
        }
        if let Some(((x0, _, x1, bas), px)) = self.bande {
            let ep = cfg.lisere.epaisseur * scale;
            let (gauche, largeur, y) = (x0 as f32, (x1 - x0) as f32, bas as f32 - ep / 2.0);
            let lueur = LISERE_LUEUR.0 * scale + LISERE_LUEUR.1 * largeur * self.basse;
            let niveau = |s: f32| {
                let m = gauche + s;
                (self.lisere.force(t, scale, son, lueur, m - px, |u| (m - u).abs()) * self.lisere.presence, teinte(s / largeur))
            };
            suites(largeur, pas, niveau, |a, b, f, k| cv.bande((gauche + a, y), (gauche + b, y), ep, melange(tete, queue, k), (0.15 + f).min(1.0)));
        }
    }
}

/// Parcourt un filet de `long` px par pas de `pas`. `niveau(s)` donne la force de la lumière et sa
/// teinte (0 à 1) au milieu `s` de chaque pas ; `trace(debut, fin, force, teinte)` reçoit chaque
/// suite de pas voisins de même force et de même teinte, à un cran près (`FILET_CRANS`) : un seul
/// trait à bouts droits par suite (`Canvas::bande`), donc aucun recouvrement. Une force sous 0,03 ne se trace pas ;
/// celle passée à `trace` est bornée à 1.
fn suites(long: f32, pas: f32, niveau: impl Fn(f32) -> (f32, f32), mut trace: impl FnMut(f32, f32, f32, f32)) {
    let n = (long / pas).ceil() as usize;
    let cran = |v: f32| (v.clamp(0.0, 1.0) * FILET_CRANS).round() as u8;
    let mut cours: Option<(f32, (u8, u8))> = None;
    for i in 0..=n {
        let a = (i as f32 * pas).min(long);
        let cle = (i < n).then(|| {
            let (f, k) = niveau((a + ((i + 1) as f32 * pas).min(long)) / 2.0);
            (if f < 0.03 { 0 } else { cran(f).max(1) }, cran(k))
        });
        if cours.map(|c| c.1) == cle {
            continue;
        }
        if let Some((debut, (f, k))) = cours.take() {
            if f > 0 {
                trace(debut, a, f as f32 / FILET_CRANS, k as f32 / FILET_CRANS);
            }
        }
        cours = cle.map(|c| (a, c));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioView;
    use crate::render::Coups;
    use tiny_skia::Pixmap;

    const FENETRE: (f32, f32, f32, f32) = (200.0, 150.0, 1000.0, 700.0);
    const ECRAN: IRect = (0, 0, 1200, 800);

    fn avancer(l: &mut Loin, t: f64, bass: f32, coups: Coups, cfg: &Config, fenetre: Option<(f32, f32, f32, f32)>) {
        let au = AudioView { bass, ..Default::default() };
        l.avance(t, 1.0 / 120.0, (600.0, 400.0), 1.0, &Son { au: &au, coups: &coups, bandes: &[] }, cfg, fenetre, Some(ECRAN));
    }

    fn image(l: &Loin, t: f64, cfg: &Config) -> Pixmap {
        let mut pm = Pixmap::new(1200, 800).unwrap();
        l.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, cfg, 1.0);
        pm
    }

    fn dans(zones: &[IRect], x: i32, y: i32) -> bool {
        zones.iter().any(|z| x >= z.0 && x < z.2 && y >= z.1 && y < z.3)
    }

    /// Planche de l'habillage sur une musique simulée (kick deux fois par seconde, drop à 1 s) :
    /// une image par quart de seconde, sur fond sombre, la fenêtre en gris.
    /// `ESSAIM_PLANCHE=<dossier> cargo test --release exporter_planche_loin -- --ignored`
    #[test]
    #[ignore]
    fn exporter_planche_loin() {
        let dossier = std::path::PathBuf::from(std::env::var("ESSAIM_PLANCHE").expect("ESSAIM_PLANCHE : dossier de sortie"));
        let mut cfg = Config::default();
        (cfg.equerres.actif, cfg.halo.actif, cfg.lisere.actif) = (true, true, true);
        let mut l = Loin::new();
        for i in 0..360 {
            let t = i as f64 / 120.0;
            let coups = Coups { kicks: (i % 60 == 0) as u32, caisses: (i % 120 == 60) as u32, drops: (i == 120) as u32, ..Default::default() };
            avancer(&mut l, t, 0.7, coups, &cfg, Some(FENETRE));
            if i % 30 == 29 {
                let mut pm = Pixmap::new(1200, 800).unwrap();
                pm.fill(tiny_skia::Color::from_rgba8(18, 18, 22, 255));
                let fenetre = tiny_skia::Rect::from_ltrb(FENETRE.0, FENETRE.1, FENETRE.2, FENETRE.3).unwrap();
                pm.fill_rect(fenetre, &crate::canvas::paint((44, 44, 50), 1.0), tiny_skia::Transform::identity(), None);
                l.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, &cfg, 1.0);
                pm.save_png(dossier.join(format!("loin-{:02}.png", i / 30))).unwrap();
            }
        }
    }

    #[test]
    fn eteintes_par_defaut_ni_zone_ni_dessin() {
        let cfg = Config::default();
        let mut l = Loin::new();
        avancer(&mut l, 0.0, 1.0, Coups { kicks: 1, ..Default::default() }, &cfg, Some(FENETRE));
        assert!(l.zones().is_empty() && image(&l, 0.0, &cfg).data().iter().all(|&b| b == 0));
    }

    #[test]
    fn equerres_aux_coins_zones_seulement_quand_le_dessin_change() {
        let mut cfg = Config::default();
        cfg.equerres.actif = true;
        // teinte fixe : au repos, le dessin ne change plus
        (cfg.couleur.vitesse_teinte, cfg.couleur.synchro_prisme) = (0.0, false);
        let mut l = Loin::new();
        let dt = 1.0 / 120.0;
        // première image : les quatre coins sont demandés, et tout le dessin y tient
        avancer(&mut l, 0.0, 0.0, Coups::default(), &cfg, Some(FENETRE));
        let zones = l.zones().to_vec();
        assert_eq!(zones.len(), 4);
        let pm = image(&l, 0.0, &cfg);
        let mut peints = 0;
        for (j, p) in pm.data().chunks_exact(4).enumerate() {
            if p[3] > 0 {
                peints += 1;
                assert!(dans(&zones, (j % 1200) as i32, (j / 1200) as i32), "pixel ({}, {}) hors des zones", j % 1200, j / 1200);
            }
        }
        assert!(peints > 100, "{peints} pixels");
        // rien sur le contenu de la fenêtre : son milieu reste vide
        assert_eq!(pm.pixel(600, 425).unwrap().alpha(), 0);
        // au repos : plus aucune zone
        for i in 1..240 {
            avancer(&mut l, i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
            assert!(l.zones().is_empty(), "image {i} : {:?}", l.zones());
        }
        // un kick : les équerres s'écartent (zones redemandées), puis reviennent au repos
        avancer(&mut l, 2.0, 0.0, Coups { kicks: 1, ..Default::default() }, &cfg, Some(FENETRE));
        let mut ecart_max = 0.0f32;
        let mut redemandees = 0;
        for i in 1..12 {
            avancer(&mut l, 2.0 + i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
            ecart_max = ecart_max.max(l.ecart.x);
            // anciennes et nouvelles zones, sauf quand le dessin n'a pas bougé d'un quart de pixel
            assert!(matches!(l.zones().len(), 0 | 8), "{} zones", l.zones().len());
            redemandees += (l.zones().len() == 8) as usize;
        }
        assert!(redemandees >= 8, "{redemandees} images redessinées sur 11");
        assert!(ecart_max > EQUERRE_ECART + 2.0 && ecart_max <= EQUERRE_BORNES.0 .1, "{ecart_max}");
        for i in 12..480 {
            avancer(&mut l, 2.0 + i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
        }
        assert!((l.ecart.x - EQUERRE_ECART).abs() < 0.1, "{}", l.ecart.x);
        // de nouveau au repos : seul le rappel redonne les zones, toutes les trois secondes
        let mut rappels = 0;
        for i in 480..1680 {
            avancer(&mut l, 2.0 + i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
            assert!(matches!(l.zones().len(), 0 | 4), "{} zones", l.zones().len());
            rappels += !l.zones().is_empty() as usize;
        }
        assert!((3..=4).contains(&rappels), "{rappels} rappels en 10 s");
        // les calques sont effacés (pause, frappe) : tout est redemandé à l'image suivante
        l.oublier();
        assert!(!l.affiche() && image(&l, 14.0, &cfg).data().iter().all(|&b| b == 0));
        avancer(&mut l, 14.0, 0.0, Coups::default(), &cfg, Some(FENETRE));
        assert!(l.affiche() && l.zones().len() == 4);
        // la fenêtre disparaît : ses coins sont effacés une fois, puis plus rien
        avancer(&mut l, 14.01, 0.0, Coups::default(), &cfg, None);
        assert_eq!(l.zones().len(), 4);
        assert!(!l.affiche() && image(&l, 14.01, &cfg).data().iter().all(|&b| b == 0));
        avancer(&mut l, 14.02, 0.0, Coups::default(), &cfg, None);
        assert!(l.zones().is_empty());
        // l'effet est éteint pendant qu'il est affiché : une dernière image l'efface
        avancer(&mut l, 14.03, 0.0, Coups::default(), &cfg, Some(FENETRE));
        cfg.equerres.actif = false;
        assert!(l.affiche() && !Loin::actives(&cfg));
        avancer(&mut l, 14.04, 0.0, Coups::default(), &cfg, Some(FENETRE));
        assert!(!l.affiche() && l.zones().len() == 4);
    }

    #[test]
    fn equerres_une_teinte_qui_glisse_redessine_peu() {
        assert!(proches((200, 10, 10), (207, 3, 17)) && !proches((200, 10, 10), (208, 10, 10)));
        let mut cfg = Config::default();
        cfg.equerres.actif = true;
        (cfg.couleur.vitesse_teinte, cfg.couleur.synchro_prisme) = (20.0, false);
        let mut l = Loin::new();
        let mut redessinees = 0;
        for i in 0..480 {
            avancer(&mut l, i as f64 / 120.0, 0.0, Coups::default(), &cfg, Some(FENETRE));
            redessinees += !l.zones().is_empty() as usize;
        }
        // 20°/s : la couleur change d'un cran ~85 fois par seconde, les équerres ~10 fois
        assert!(redessinees < 100, "{redessinees} images redessinées sur 480");
    }

    #[test]
    fn suites_de_pas_de_meme_niveau() {
        // 60 px par pas de 6 : rien, puis une force qui monte par paliers, puis rien
        let niveau = |s: f32| (if (12.0..48.0).contains(&s) { if s < 30.0 { 0.5 } else { 2.0 } } else { 0.0 }, 0.25);
        let mut vues = Vec::new();
        suites(60.0, 6.0, niveau, |a, b, f, k| vues.push((a, b, f, k)));
        assert_eq!(vues, [(12.0, 30.0, 0.5, 0.25), (30.0, 48.0, 1.0, 0.25)]);
        // un filet plus court qu'un pas, allumé jusqu'au bout
        vues.clear();
        suites(4.0, 6.0, |_| (0.5, 1.0), |a, b, f, k| vues.push((a, b, f, k)));
        assert_eq!(vues, [(0.0, 4.0, 0.5, 1.0)]);
    }

    #[test]
    fn le_cadre_se_parcourt_dans_le_sens_des_aiguilles() {
        let c = Cadre(100.0, 50.0, 500.0, 250.0);
        assert_eq!(c.tour(), 1200.0);
        // au plus près : le haut (au-dessus du milieu), la droite, le bas, la gauche
        assert_eq!(c.proche((300.0, 60.0)), 200.0);
        assert_eq!(c.proche((490.0, 150.0)), 400.0 + 100.0);
        assert_eq!(c.proche((300.0, 240.0)), 600.0 + 200.0);
        assert_eq!(c.proche((110.0, 150.0)), 1000.0 + 100.0);
        // pointeur hors de la fenêtre : le point du cadre en face
        assert_eq!(c.proche((300.0, -400.0)), 200.0);
        // le chemin le plus court passe par le coin de départ
        assert_eq!(c.ecart(10.0, 1190.0), 20.0);
        assert_eq!(c.ecart(300.0, 900.0), 600.0);
    }

    #[test]
    fn halo_le_long_du_cadre_seulement_avec_du_son() {
        let mut cfg = Config::default();
        cfg.halo.actif = true;
        let mut l = Loin::new();
        let dt = 1.0 / 120.0;
        // silence : rien
        for i in 0..60 {
            avancer(&mut l, i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
        }
        assert!(l.zones().is_empty() && !l.affiche() && image(&l, 0.5, &cfg).data().iter().all(|&b| b == 0));
        // musique : quatre bandes le long du cadre, tout le dessin y tient
        for i in 60..180 {
            avancer(&mut l, i as f64 * dt, 0.8, Coups { kicks: (i % 60 == 0) as u32, ..Default::default() }, &cfg, Some(FENETRE));
        }
        let zones = l.zones().to_vec();
        assert_eq!(zones.len(), 4);
        let pm = image(&l, 1.5, &cfg);
        let mut peints = 0;
        for (j, p) in pm.data().chunks_exact(4).enumerate() {
            if p[3] > 0 {
                peints += 1;
                assert!(dans(&zones, (j % 1200) as i32, (j / 1200) as i32), "pixel ({}, {}) hors des bandes", j % 1200, j / 1200);
            }
        }
        // le pointeur (600, 400) est au plus près du haut du cadre : la lumière y est, pas au
        // milieu de la fenêtre
        assert!(peints > 300 && pm.pixel(600, 151).unwrap().alpha() > 100, "{peints} pixels");
        assert_eq!(pm.pixel(600, 425).unwrap().alpha(), 0);
        // la profondeur réglée borne la lueur : rien plus loin dans la fenêtre
        let fond = (150.0 + cfg.halo.profondeur * 1.25 + 2.0 * cfg.halo.epaisseur + 4.0) as u32;
        assert_eq!(pm.pixel(600, fond).unwrap().alpha(), 0);
        // la lueur est lisse le long du cadre : à 5 px sous le haut, aucun saut d'un pixel au
        // suivant (les pas ne se recouvrent pas)
        let ligne: Vec<i32> = (480..720).map(|x| pm.pixel(x, 155).unwrap().alpha() as i32).collect();
        let saut = ligne.windows(2).map(|w| (w[0] - w[1]).abs()).max().unwrap();
        assert!(ligne.iter().all(|&a| a > 0) && saut <= 12, "saut de {saut} dans {ligne:?}");
        // la fenêtre change : les anciennes bandes sont redonnées avec les nouvelles
        avancer(&mut l, 1.51, 0.8, Coups::default(), &cfg, Some((300.0, 150.0, 1000.0, 700.0)));
        assert_eq!(l.zones().len(), 8);
        // le son s'arrête : le halo s'efface, puis plus rien
        for i in 0..520 {
            avancer(&mut l, 1.52 + i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
        }
        assert!(l.zones().is_empty() && !l.affiche());
        assert!(image(&l, 6.0, &cfg).data().iter().all(|&b| b == 0));
    }

    #[test]
    fn lisere_au_bord_bas_seulement_avec_du_son() {
        let mut cfg = Config::default();
        cfg.lisere.actif = true;
        let mut l = Loin::new();
        let dt = 1.0 / 120.0;
        // silence : rien
        for i in 0..60 {
            avancer(&mut l, i as f64 * dt, 0.0, Coups::default(), &cfg, None);
        }
        assert!(l.zones().is_empty() && image(&l, 0.5, &cfg).data().iter().all(|&b| b == 0));
        // musique : une bande au bord bas de l'écran, et rien au-dessus
        for i in 60..180 {
            avancer(&mut l, i as f64 * dt, 0.8, Coups { kicks: (i % 60 == 0) as u32, ..Default::default() }, &cfg, None);
        }
        let zones = l.zones().to_vec();
        assert_eq!(zones, [(0, 800 - 5, 1200, 800)]);
        let pm = image(&l, 1.5, &cfg);
        let mut peints = 0;
        for (j, p) in pm.data().chunks_exact(4).enumerate() {
            if p[3] > 0 {
                peints += 1;
                assert!(dans(&zones, (j % 1200) as i32, (j / 1200) as i32), "pixel hors de la bande");
            }
        }
        assert!(peints > 200, "{peints} pixels");
        // le son s'arrête : la bande s'efface (une dernière zone pour l'effacer), puis plus rien
        let mut dernieres = 0;
        for i in 180..700 {
            avancer(&mut l, i as f64 * dt, 0.0, Coups::default(), &cfg, None);
            dernieres = if l.zones().is_empty() { dernieres } else { i };
        }
        assert!(l.zones().is_empty() && dernieres > 180 && dernieres < 700, "dernière zone à l'image {dernieres}");
        assert!(image(&l, 6.0, &cfg).data().iter().all(|&b| b == 0));
    }
}

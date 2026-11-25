//! Gouttes en contour (`[gouttes]`) : un noyau autour du pointeur et quatre perles en orbite, dont
//! on ne dessine que la ligne de niveau du champ qu'elles forment ensemble. La ligne se soude
//! quand deux gouttes se touchent et se sépare quand elles s'éloignent ; l'intérieur reste vide.
//!
//! Le champ (somme des r² / d² de chaque goutte) est calculé sur une petite grille autour du
//! centre, puis son contour au niveau 1 est tiré case par case (marching squares) en petits
//! segments, dessinés comme des traits : rien de neuf pour les moteurs de dessin.
//!
//! `ui/apercu.js` reproduit ce calcul : le modifier ici oblige à le reporter là.

use super::{melange, Elan, Partage, Ressort};
use crate::canvas::{Canvas, Rgb};
use crate::config::Config;
use std::f32::consts::TAU;

/// Demi-côté de la zone calculée (px à 100 %, pour une taille de 1) et nœuds de la grille par
/// côté (pas de 2 px).
const DEMI: f32 = 72.0;
const NOEUDS: usize = 73;
/// Noyau : rayon au repos, ce qu'y ajoutent la basse et une montée, élan d'un kick et d'un drop
/// (px/s), bornes du rayon.
const NOYAU: (f32, f32, f32) = (8.0, 2.5, 4.0);
const NOYAU_ELAN: (f32, f32) = (40.0, 70.0);
const NOYAU_BORNES: (f32, f32) = (2.0, 18.0);
/// Perles : rayon de l'orbite, vitesse (rad/s), rayon de la perle.
const PERLES: [(f32, f32, f32); 4] = [(17.0, 2.1, 4.8), (23.0, -1.6, 4.2), (29.0, 1.2, 5.2), (34.0, -0.9, 4.2)];
/// Élan qui projette une perle : au kick (tiré entre deux valeurs) et au drop (px/s) ; écart le
/// plus grand à son orbite ; part de l'orbite que ferme une montée.
const PERLE_ELAN: ((f32, f32), f32) = ((80.0, 140.0), 200.0);
const PERLE_ECART_MAX: f32 = 24.0;
const PERLE_MONTEE: f32 = 0.9;
/// Bulles d'un drop : nombre, vitesse (px/s), rayon, durée (s), tirés entre deux valeurs ; frein.
const BULLES: usize = 5;
const BULLE_VITESSE: (f32, f32) = (90.0, 160.0);
const BULLE_RAYON: (f32, f32) = (2.2, 3.2);
const BULLE_VIE: (f32, f32) = (0.6, 0.9);
const BULLE_FREIN: f32 = 3.5;
/// Distance au centre où la couleur a fini de passer de celle des têtes à celle des queues.
const DEGRADE: f32 = 45.0;

struct Perle {
    angle: f32,
    ecart: Ressort,
}

/// Bulle lâchée par un drop : place et vitesse autour du centre (px à 100 %).
struct Bulle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    r: f32,
    ne: f64,
    vie: f32,
}

pub(super) struct Gouttes {
    noyau: Ressort,
    perles: [Perle; 4],
    bulles: Vec<Bulle>,
    graine: u32,
    /// Champ aux nœuds de la grille, et segments du contour (px à 100 %, autour du centre).
    champ: Vec<f32>,
    segments: Vec<[(f32, f32); 2]>,
}

impl Gouttes {
    pub(super) fn new() -> Self {
        let perle = |i: usize| Perle { angle: i as f32 * 2.4, ecart: Ressort::default() };
        Self { noyau: Ressort::en(NOYAU.0), perles: [perle(0), perle(1), perle(2), perle(3)], bulles: Vec::new(), graine: 0x51ed_270b, champ: Vec::new(), segments: Vec::new() }
    }

    /// Plus de gouttes : rien à dessiner.
    pub(super) fn oublier(&mut self) {
        self.segments.clear();
        self.bulles.clear();
    }

    /// Gouttes rallumées : noyau et perles reprennent leur place de départ, sans élan.
    pub(super) fn reinitialiser(&mut self) {
        let neuves = Self::new();
        (self.noyau, self.perles) = (neuves.noyau, neuves.perles);
        self.oublier();
    }

    /// Tirage entre `a` et `b` (xorshift).
    fn hasard(&mut self, (a, b): (f32, f32)) -> f32 {
        self.graine ^= self.graine << 13;
        self.graine ^= self.graine >> 17;
        self.graine ^= self.graine << 5;
        a + (b - a) * ((self.graine >> 8) as f32 / (1u32 << 24) as f32)
    }

    /// Avance d'une image ; `el` : les coups à la force des réactions des gouttes.
    pub(super) fn avance(&mut self, p: &Partage, el: &Elan, t: f64, dt: f32, cfg: &Config) {
        if !cfg.gouttes.actif {
            self.oublier();
            return;
        }
        for _ in 0..if el.kick > 0.0 { el.kicks.min(4) } else { 0 } {
            self.noyau.v += NOYAU_ELAN.0 * el.fk;
            for i in 0..self.perles.len() {
                self.perles[i].ecart.v += self.hasard(PERLE_ELAN.0) * el.fk;
            }
        }
        if el.drop > 0.0 {
            self.noyau.v += NOYAU_ELAN.1 * el.drop;
            for q in &mut self.perles {
                q.ecart.v += PERLE_ELAN.1 * el.drop;
            }
            // vitesse bornée à celle d'une réaction de 1 : une bulle plus rapide sortirait de la
            // grille (son contour y serait coupé)
            for i in 0..BULLES {
                let (a, v) = (i as f32 * TAU / BULLES as f32 + self.hasard((-0.3, 0.3)), self.hasard(BULLE_VITESSE) * el.drop.min(1.0));
                let (r, vie) = (self.hasard(BULLE_RAYON), self.hasard(BULLE_VIE));
                self.bulles.push(Bulle { x: 0.0, y: 0.0, vx: a.cos() * v, vy: a.sin() * v, r, ne: t, vie });
            }
            // plusieurs drops de suite : les plus vieilles bulles s'en vont
            let trop = self.bulles.len().saturating_sub(2 * BULLES);
            self.bulles.drain(..trop);
        }
        self.noyau.vers(NOYAU.0 + NOYAU.1 * p.basse + NOYAU.2 * p.montee, (16.0, 0.35), dt, NOYAU_BORNES);
        for (q, &(orbite, vitesse, _)) in self.perles.iter_mut().zip(&PERLES) {
            q.angle = (q.angle + vitesse * (1.0 + 1.5 * p.basse) * dt).rem_euclid(TAU);
            q.ecart.vers(0.0, (9.0, 0.32), dt, (-orbite, PERLE_ECART_MAX));
        }
        let frein = (-dt * BULLE_FREIN).exp();
        for b in &mut self.bulles {
            (b.x, b.y) = (b.x + b.vx * dt, b.y + b.vy * dt);
            (b.vx, b.vy) = (b.vx * frein, b.vy * frein);
        }
        self.bulles.retain(|b| t - b.ne < b.vie as f64);

        // les gouttes de l'image : (x, y, rayon) autour du centre, puis leur contour commun
        let mut gouttes = [(0.0f32, 0.0f32, 0.0f32); 5 + 2 * BULLES];
        gouttes[0] = (0.0, 0.0, self.noyau.x.max(NOYAU_BORNES.0));
        let mut n = 1;
        for (q, &(orbite, _, r)) in self.perles.iter().zip(&PERLES) {
            let d = (orbite * (1.0 - PERLE_MONTEE * p.montee) + q.ecart.x).max(0.0);
            gouttes[n] = (q.angle.cos() * d, q.angle.sin() * d, r);
            n += 1;
        }
        for b in &self.bulles {
            let age = ((t - b.ne) as f32 / b.vie).clamp(0.0, 1.0);
            gouttes[n] = (b.x, b.y, b.r * (1.0 - age).powf(0.7));
            n += 1;
        }
        contour(&gouttes[..n], &mut self.champ, &mut self.segments);
    }

    /// Demi-côté (px à 100 %) du carré autour du centre qui contient les gouttes.
    pub(super) fn etendue(&self, cfg: &Config) -> f32 {
        DEMI * cfg.gouttes.taille + cfg.gouttes.epaisseur + 2.0
    }

    pub(super) fn dessiner(&self, p: &Partage, cv: &mut dyn Canvas, cfg: &Config, scale: f32, (tete, queue): (Rgb, Rgb)) {
        let c = &cfg.gouttes;
        let ((cx, cy), k, largeur) = (p.centre, c.taille * scale, c.epaisseur * scale);
        for &[a, b] in &self.segments {
            // couleur des têtes au centre, des queues au large
            let loin = ((a.0 + b.0).hypot(a.1 + b.1) / 2.0 / DEGRADE).min(1.0);
            cv.line((cx + a.0 * k, cy + a.1 * k), (cx + b.0 * k, cy + b.1 * k), largeur, melange(tete, queue, loin), 1.0, true);
        }
    }
}

/// Contour au niveau 1 du champ des `gouttes` (x, y, rayon ; px autour du centre), en segments.
/// Le champ n'est calculé que sur la partie de la grille que les gouttes peuvent atteindre.
fn contour(gouttes: &[(f32, f32, f32)], champ: &mut Vec<f32>, segments: &mut Vec<[(f32, f32); 2]>) {
    segments.clear();
    champ.resize(NOEUDS * NOEUDS, 0.0);
    let pas = 2.0 * DEMI / (NOEUDS - 1) as f32;
    // nœuds utiles : autour de chaque goutte, le champ tombe sous 1 bien avant 3,2 rayons
    let (mut i0, mut j0, mut i1, mut j1) = (NOEUDS, NOEUDS, 0usize, 0usize);
    let noeud = |v: f32| ((v + DEMI) / pas).clamp(0.0, (NOEUDS - 1) as f32);
    for &(x, y, r) in gouttes {
        let m = r * 3.2 + 3.0;
        (i0, i1) = (i0.min(noeud(x - m).floor() as usize), i1.max(noeud(x + m).ceil() as usize));
        (j0, j1) = (j0.min(noeud(y - m).floor() as usize), j1.max(noeud(y + m).ceil() as usize));
    }
    if i0 >= i1 || j0 >= j1 {
        return;
    }
    for j in j0..=j1 {
        let y = -DEMI + j as f32 * pas;
        for i in i0..=i1 {
            let x = -DEMI + i as f32 * pas;
            // le bord de la grille est toujours dehors : un contour qui l'atteint s'y referme
            let bord = i == 0 || j == 0 || i == NOEUDS - 1 || j == NOEUDS - 1;
            champ[j * NOEUDS + i] = if bord { 0.0 } else { gouttes.iter().map(|&(gx, gy, r)| r * r / ((x - gx) * (x - gx) + (y - gy) * (y - gy) + 1.0)).sum() };
        }
    }
    // point où le champ vaut 1 entre deux nœuds voisins
    let croise = |(xa, ya, va): (f32, f32, f32), (xb, yb, vb): (f32, f32, f32)| {
        let k = if (vb - va).abs() > 1e-6 { ((1.0 - va) / (vb - va)).clamp(0.0, 1.0) } else { 0.5 };
        (xa + (xb - xa) * k, ya + (yb - ya) * k)
    };
    for j in j0..j1 {
        for i in i0..i1 {
            let coin = |di: usize, dj: usize| (-DEMI + (i + di) as f32 * pas, -DEMI + (j + dj) as f32 * pas, champ[(j + dj) * NOEUDS + i + di]);
            // coins dans le sens des aiguilles d'une montre : haut gauche, haut droit, bas droit, bas gauche
            let c = [coin(0, 0), coin(1, 0), coin(1, 1), coin(0, 1)];
            let dedans = c.map(|q| q.2 > 1.0);
            let nombre = dedans.iter().filter(|&&d| d).count();
            if nombre == 0 || nombre == 4 {
                continue;
            }
            // passages sur les arêtes haut, droite, bas, gauche
            let mut passages = [(0.0f32, 0.0f32); 4];
            let mut cotes = [false; 4];
            for a in 0..4 {
                if dedans[a] != dedans[(a + 1) % 4] {
                    (passages[a], cotes[a]) = (croise(c[a], c[(a + 1) % 4]), true);
                }
            }
            if cotes.iter().all(|&x| x) {
                // col : le milieu de la case dit quels coins sont reliés
                let milieu = (c[0].2 + c[1].2 + c[2].2 + c[3].2) / 4.0 > 1.0;
                if milieu == dedans[0] {
                    segments.push([passages[0], passages[1]]);
                    segments.push([passages[2], passages[3]]);
                } else {
                    segments.push([passages[3], passages[0]]);
                    segments.push([passages[1], passages[2]]);
                }
            } else {
                let mut bouts = (0..4).filter(|&a| cotes[a]).map(|a| passages[a]);
                if let (Some(a), Some(b)) = (bouts.next(), bouts.next()) {
                    segments.push([a, b]);
                }
            }
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

    fn image(f: &Figures, t: f64, cfg: &Config) -> Pixmap {
        let mut pm = Pixmap::new(600, 600).unwrap();
        f.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, cfg, 1.0);
        pm
    }

    /// Plus grande distance au centre d'un bout de segment.
    fn portee(g: &Gouttes) -> f32 {
        g.segments.iter().flatten().map(|p| p.0.hypot(p.1)).fold(0.0, f32::max)
    }

    #[test]
    fn une_goutte_seule_donne_un_cercle_ferme() {
        let (mut champ, mut segments) = (Vec::new(), Vec::new());
        contour(&[(4.0, -6.0, 10.0)], &mut champ, &mut segments);
        assert!(segments.len() > 20, "{} segments", segments.len());
        // le champ vaut 1 à √(r² − 1) du centre : tous les bouts y sont, à une fraction de pas près
        for p in segments.iter().flatten() {
            let d = (p.0 - 4.0).hypot(p.1 + 6.0);
            assert!((d - 99.0f32.sqrt()).abs() < 0.3, "bout à {d} px du centre");
        }
        // ligne fermée : chaque bout sert à deux segments
        for p in segments.iter().flatten() {
            let n = segments.iter().flatten().filter(|q| (q.0 - p.0).abs() < 1e-4 && (q.1 - p.1).abs() < 1e-4).count();
            assert_eq!(n, 2, "bout ({}, {}) vu {n} fois", p.0, p.1);
        }
        // deux gouttes qui se touchent se soudent en une seule ligne, plus courte que deux cercles
        let seul = segments.len();
        contour(&[(-7.0, 0.0, 10.0), (7.0, 0.0, 10.0)], &mut champ, &mut segments);
        assert!(segments.len() > seul && segments.len() < 2 * seul, "{} segments", segments.len());
        // rien à dessiner sans goutte
        contour(&[], &mut champ, &mut segments);
        assert!(segments.is_empty());
    }

    #[test]
    fn les_gouttes_restent_dans_leur_zone_et_laissent_le_pointeur_libre() {
        let mut cfg = Config::default();
        // éteintes : ni zone ni dessin
        let mut f = Figures::new();
        avancer(&mut f, 0.0, 1.0 / 120.0, C, 1.0, &AudioView::default(), &Coups::default(), &cfg);
        assert!(f.rect(C, 1.0, &cfg).is_none() && f.gouttes.segments.is_empty());
        cfg.gouttes.actif = true;
        let dt = 1.0 / 120.0;
        let mut loin = 0.0f32;
        // dix secondes de musique forte, un kick par demi-seconde, un drop toutes les deux secondes
        for i in 0..1200 {
            let t = i as f64 * dt as f64;
            let coups = Coups { kicks: (i % 60 == 0) as u32, drops: (i % 240 == 120) as u32, ..Default::default() };
            avancer(&mut f, t, dt, C, 1.0, &AudioView { bass: 0.9, ..Default::default() }, &coups, &cfg);
            loin = loin.max(portee(&f.gouttes));
            assert!(!f.gouttes.segments.is_empty() && f.gouttes.bulles.len() <= 2 * BULLES);
        }
        // projetées par les kicks et les drops, elles restent dans la grille
        assert!(loin > 40.0 && loin < DEMI - 2.0, "portée {loin}");
        // tout le dessin tient dans la zone, et rien ne couvre le pointeur
        let zone = f.rect(C, 1.0, &cfg).unwrap();
        let pm = image(&f, 10.0, &cfg);
        let mut peints = 0;
        for (j, p) in pm.data().as_chunks::<4>().0.iter().enumerate() {
            if p[3] > 0 {
                peints += 1;
                let (x, y) = ((j % 600) as i32, (j / 600) as i32);
                assert!(x >= zone.0 && x < zone.2 && y >= zone.1 && y < zone.3, "pixel ({x}, {y}) hors de la zone");
            }
        }
        assert!(peints > 150 && pm.pixel(300, 300).unwrap().alpha() == 0, "{peints} pixels");
        // une montée referme les orbites : tout se soude près du centre
        for i in 0..240 {
            avancer(&mut f, 10.0 + i as f64 * dt as f64, dt, C, 1.0, &AudioView { tension: 1.0, ..Default::default() }, &Coups::default(), &cfg);
        }
        assert!(portee(&f.gouttes) < 22.0, "portée {} pendant la montée", portee(&f.gouttes));
        // éteintes : plus rien
        cfg.gouttes.actif = false;
        avancer(&mut f, 13.0, dt, C, 1.0, &AudioView::default(), &Coups::default(), &cfg);
        assert!(image(&f, 13.0, &cfg).data().iter().all(|&b| b == 0));
    }
}

//! Gouttes en contour (`[gouttes]`) : un noyau autour du pointeur et des perles en orbite, dont
//! on ne dessine que la ligne de niveau du champ qu'elles forment ensemble. La ligne se soude
//! quand deux gouttes se touchent et se sépare quand elles s'éloignent ; l'intérieur reste vide.
//! Comme un liquide, chaque goutte traîne derrière le pointeur quand il bouge, au bout de son
//! propre ressort (`[gouttes] souplesse`) : l'ensemble s'étire, se sépare, puis revient se souder.
//!
//! Le champ (somme des r² / d² de chaque goutte) est calculé sur une petite grille autour du
//! centre, puis son contour au niveau 1 est tiré case par case (marching squares) en petits
//! segments, dessinés comme des traits : rien de neuf pour les moteurs de dessin.
//!
//! `ui/apercu.js` reproduit ce calcul : le modifier ici oblige à le reporter là.

use super::{ecart_module, melange, Gestes, Partage, Ressort, ACCENT, APOGEE, BATTEMENT};
use crate::canvas::{Canvas, Rgb};
use crate::config::Config;
use crate::modulation::Effet;
use std::f32::consts::TAU;

/// Demi-côté de la zone calculée (px à 100 %, pour une taille de 1) et nœuds de la grille par
/// côté (pas de 2 px).
const DEMI: f32 = 72.0;
const NOEUDS: usize = 73;
/// Noyau : rayon au repos, ce qu'y ajoutent la basse et une montée, élan d'un battement et d'une
/// apogée (px/s), bornes du rayon, ce qu'y ajoute la modulation du noyau à fond (px).
const NOYAU: (f32, f32, f32) = (8.0, 2.5, 4.0);
const NOYAU_ELAN: (f32, f32) = (40.0, 70.0);
const NOYAU_BORNES: (f32, f32) = (2.0, 18.0);
const NOYAU_MODULE: f32 = 8.0;
/// Perles : rayon de l'orbite, vitesse (rad/s), rayon de la perle ; les quatre premières sont
/// celles de toujours (`[gouttes] perles` en garde de 0 à 8).
const PERLES: [(f32, f32, f32); 8] = [
    (17.0, 2.1, 4.8),
    (23.0, -1.6, 4.2),
    (29.0, 1.2, 5.2),
    (34.0, -0.9, 4.2),
    (20.0, -1.9, 3.8),
    (26.0, 1.4, 4.4),
    (31.0, -1.1, 3.9),
    (14.0, 2.6, 3.6),
];
/// Perles ajoutées par la modulation du nombre à fond.
const PERLES_MODULE: f32 = 4.0;
/// Élan qui projette une perle : au battement (tiré entre deux valeurs) et à l'apogée (px/s) ;
/// écart le plus grand à son orbite ; part de l'orbite que ferme une montée.
const PERLE_ELAN: ((f32, f32), f32) = ((80.0, 140.0), 200.0);
const PERLE_ECART_MAX: f32 = 24.0;
const PERLE_MONTEE: f32 = 0.9;
/// Bulles d'une apogée : nombre, vitesse (px/s), rayon, durée (s), tirés entre deux valeurs ;
/// frein ; bulles gardées au plus (apogées et gouttes détachées par l'accent).
const BULLES: usize = 5;
const BULLE_VITESSE: (f32, f32) = (90.0, 160.0);
const BULLE_RAYON: (f32, f32) = (2.2, 3.2);
const BULLE_VIE: (f32, f32) = (0.6, 0.9);
const BULLE_FREIN: f32 = 3.5;
const BULLES_MAX: usize = 12;
/// Goutte détachée par un accent : vitesse (px/s), part du rayon de sa perle, durée (s).
const DETACHEE: (f32, f32, f32) = (110.0, 0.7, 0.8);
/// Traîne liquide : retard le plus grand d'une goutte sur le centre (px à 100 %, souplesse 1),
/// part du mouvement que garde le noyau et chaque perle, pulsation du ressort qui les ramène
/// (rad/s : les perles du large plus lentes) et son amortissement.
const TRAINE_MAX: f32 = 26.0;
const TRAINE_NOYAU: f32 = 0.35;
const TRAINE_RAPPEL: (f32, f32) = (18.0, 0.3);
/// Distance au centre où la couleur a fini de passer de celle des têtes à celle des queues.
const DEGRADE: f32 = 45.0;
/// Gouttes de l'image au plus : noyau, perles, bulles.
const GOUTTES_MAX: usize = 1 + PERLES.len() + BULLES_MAX;

/// Retard d'une goutte sur le centre (px à 100 %), au bout d'un ressort.
#[derive(Clone, Copy, Default)]
struct Traine {
    x: Ressort,
    y: Ressort,
}

impl Traine {
    /// Le centre a bougé de `(dx, dy)` (px à 100 %) : la goutte en garde la part `k` en retard,
    /// puis son ressort la ramène (pulsation `w`), sans dépasser `max`.
    fn avance(&mut self, (dx, dy): (f32, f32), k: f32, w: f32, max: f32, dt: f32) {
        self.x.x -= dx * k;
        self.y.x -= dy * k;
        self.x.vers(0.0, (w, TRAINE_RAPPEL.1), dt, (-max, max));
        self.y.vers(0.0, (w, TRAINE_RAPPEL.1), dt, (-max, max));
    }
}

#[derive(Clone, Copy)]
struct Perle {
    angle: f32,
    ecart: Ressort,
    traine: Traine,
}

/// Bulle lâchée par une apogée ou détachée par un accent : place et vitesse autour du centre (px
/// à 100 %).
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
    traine: Traine,
    perles: [Perle; PERLES.len()],
    bulles: Vec<Bulle>,
    graine: u32,
    /// Champ aux nœuds de la grille, et segments du contour (px à 100 %, autour du centre).
    champ: Vec<f32>,
    segments: Vec<[(f32, f32); 2]>,
}

impl Gouttes {
    pub(super) fn new() -> Self {
        let perles = std::array::from_fn(|i| Perle { angle: i as f32 * 2.4, ecart: Ressort::default(), traine: Traine::default() });
        Self { noyau: Ressort::en(NOYAU.0), traine: Traine::default(), perles, bulles: Vec::new(), graine: 0x51ed_270b, champ: Vec::new(), segments: Vec::new() }
    }

    /// Plus de gouttes : rien à dessiner.
    pub(super) fn oublier(&mut self) {
        self.segments.clear();
        self.bulles.clear();
    }

    /// Gouttes rallumées : noyau et perles reprennent leur place de départ, sans élan.
    pub(super) fn reinitialiser(&mut self) {
        let neuves = Self::new();
        (self.noyau, self.traine, self.perles) = (neuves.noyau, neuves.traine, neuves.perles);
        self.oublier();
    }

    /// Tirage entre `a` et `b` (xorshift).
    fn hasard(&mut self, (a, b): (f32, f32)) -> f32 {
        self.graine ^= self.graine << 13;
        self.graine ^= self.graine >> 17;
        self.graine ^= self.graine << 5;
        a + (b - a) * ((self.graine >> 8) as f32 / (1u32 << 24) as f32)
    }

    /// Perles visibles (de 0 à 8, fractionnaire : la dernière grandit ou fond) : le réglage, plus
    /// la modulation du nombre.
    fn nombre(cfg: &Config, m: &Effet) -> f32 {
        let n = cfg.gouttes.perles as f32 + if m.propres.nombre.is_finite() { m.propres.nombre * PERLES_MODULE } else { 0.0 };
        n.clamp(0.0, PERLES.len() as f32)
    }

    /// Avance d'une image ; `g` : les gestes reçus, `m` : les modulations des gouttes, `bouge` :
    /// déplacement du centre depuis l'image précédente (px écran).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn avance(&mut self, p: &Partage, g: &Gestes, m: &Effet, bouge: (f32, f32), t: f64, dt: f32, scale: f32, cfg: &Config) {
        let c = &cfg.gouttes;
        if !c.actif {
            self.oublier();
            return;
        }
        let nombre = Self::nombre(cfg, m);
        let visible = |j: usize| (nombre - j as f32).clamp(0.0, 1.0);
        for _ in 0..g.n(BATTEMENT) {
            let f = g.fort(BATTEMENT);
            self.noyau.v += NOYAU_ELAN.0 * f;
            for i in 0..self.perles.len() {
                self.perles[i].ecart.v += self.hasard(PERLE_ELAN.0) * f;
            }
        }
        if g.n(APOGEE) > 0 {
            let f = g.fort(APOGEE);
            self.noyau.v += NOYAU_ELAN.1 * f;
            for q in &mut self.perles {
                q.ecart.v += PERLE_ELAN.1 * f;
            }
            // vitesse bornée à celle d'un geste de force 1 : une bulle plus rapide sortirait de la
            // grille (son contour y serait coupé)
            for i in 0..BULLES {
                let (a, v) = (i as f32 * TAU / BULLES as f32 + self.hasard((-0.3, 0.3)), self.hasard(BULLE_VITESSE) * f.min(1.0));
                let (r, vie) = (self.hasard(BULLE_RAYON), self.hasard(BULLE_VIE));
                self.bulles.push(Bulle { x: 0.0, y: 0.0, vx: a.cos() * v, vy: a.sin() * v, r, ne: t, vie });
            }
        }
        // accent : une goutte se détache d'une perle visible et file vers le large
        if g.n(ACCENT) > 0 && nombre >= 1.0 {
            let j = (self.hasard((0.0, nombre.floor())) as usize).min(nombre as usize - 1);
            let (orbite, _, r) = PERLES[j];
            let q = &mut self.perles[j];
            let d = orbite + q.ecart.x;
            let (x, y) = (q.angle.cos() * d + q.traine.x.x, q.angle.sin() * d + q.traine.y.x);
            let v = DETACHEE.0 * g.fort(ACCENT).min(1.0);
            q.ecart.v += 60.0 * g.fort(ACCENT);
            self.bulles.push(Bulle { x, y, vx: q.angle.cos() * v, vy: q.angle.sin() * v, r: r * DETACHEE.1, ne: t, vie: DETACHEE.2 });
        }
        // plusieurs apogées de suite : les plus vieilles bulles s'en vont
        let trop = self.bulles.len().saturating_sub(BULLES_MAX);
        self.bulles.drain(..trop);
        let noyau = NOYAU.0 + NOYAU.1 * p.basse + NOYAU.2 * p.montee + if m.propres.noyau.is_finite() { NOYAU_MODULE * m.propres.noyau } else { 0.0 };
        self.noyau.vers(noyau, (16.0, 0.35), dt, NOYAU_BORNES);
        let vitesse = c.vitesse.clamp(0.0, 3.0) * (1.0 + 1.5 * p.basse);
        for (q, &(orbite, w, _)) in self.perles.iter_mut().zip(&PERLES) {
            q.angle = (q.angle + w * vitesse * dt).rem_euclid(TAU);
            q.ecart.vers(0.0, (9.0, 0.32), dt, (-orbite, PERLE_ECART_MAX));
        }
        // traîne liquide : le centre a bougé, les gouttes en gardent une part en retard (les
        // perles du large plus que celles près du noyau), les bulles restent où elles sont
        let k = (scale * c.taille).max(1e-3);
        let u = if bouge.0.is_finite() && bouge.1.is_finite() { (bouge.0 / k, bouge.1 / k) } else { (0.0, 0.0) };
        let souplesse = c.souplesse.clamp(0.0, 1.0);
        let max = TRAINE_MAX * souplesse;
        self.traine.avance(u, TRAINE_NOYAU * souplesse, TRAINE_RAPPEL.0, max * TRAINE_NOYAU, dt);
        for (q, &(orbite, _, _)) in self.perles.iter_mut().zip(&PERLES) {
            let part = souplesse * (0.5 + orbite / 70.0);
            q.traine.avance(u, part, TRAINE_RAPPEL.0 - orbite / 4.0, max, dt);
        }
        let frein = (-dt * BULLE_FREIN).exp();
        for b in &mut self.bulles {
            (b.x, b.y) = (b.x + b.vx * dt - u.0, b.y + b.vy * dt - u.1);
            (b.vx, b.vy) = (b.vx * frein, b.vy * frein);
        }
        self.bulles.retain(|b| t - b.ne < b.vie as f64 && b.x.abs().max(b.y.abs()) < DEMI);

        // les gouttes de l'image : (x, y, rayon) autour du centre, gardées dans la grille (leur
        // contour n'y est jamais coupé), puis leur contour commun
        let dans = |x: f32, y: f32, r: f32| {
            let lim = (DEMI - r - 3.0).max(0.0);
            (x.clamp(-lim, lim), y.clamp(-lim, lim), r)
        };
        let ecart = ecart_module(m.propres.ecart);
        let mut gouttes = [(0.0f32, 0.0f32, 0.0f32); GOUTTES_MAX];
        gouttes[0] = dans(self.traine.x.x, self.traine.y.x, self.noyau.x.max(NOYAU_BORNES.0));
        let mut n = 1;
        for (j, (q, &(orbite, _, r))) in self.perles.iter().zip(&PERLES).enumerate() {
            let r = r * visible(j);
            if r < 0.5 {
                continue;
            }
            let d = (orbite * ecart * (1.0 - PERLE_MONTEE * p.montee) + q.ecart.x).max(0.0);
            gouttes[n] = dans(q.angle.cos() * d + q.traine.x.x, q.angle.sin() * d + q.traine.y.x, r);
            n += 1;
        }
        for b in &self.bulles {
            let age = ((t - b.ne) as f32 / b.vie).clamp(0.0, 1.0);
            gouttes[n] = dans(b.x, b.y, b.r * (1.0 - age).powf(0.7));
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

    /// Milieu des bouts de segments (px à 100 %, autour du centre).
    fn milieu(g: &Gouttes) -> (f32, f32) {
        let n = (g.segments.len() * 2).max(1) as f32;
        let (sx, sy) = g.segments.iter().flatten().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
        (sx / n, sy / n)
    }

    #[test]
    fn les_gouttes_trainent_derriere_la_souris_comme_un_liquide() {
        // deux gouttes pareilles (mêmes tirages), l'une rigide, l'autre souple : la souris file
        // vers la droite, puis s'arrête
        let dt = 1.0 / 120.0;
        let (au, coups) = (AudioView::default(), Coups::default());
        let cfg = |souplesse: f32| {
            let mut c = Config::default();
            (c.gouttes.actif, c.gouttes.souplesse) = (true, souplesse);
            c
        };
        let (rigide, souple) = (cfg(0.0), cfg(1.0));
        let (mut a, mut b) = (Figures::new(), Figures::new());
        let mut image = |i: usize, cursor: (f32, f32)| {
            avancer(&mut a, i as f64 * dt as f64, dt, cursor, 1.0, &au, &coups, &rigide);
            avancer(&mut b, i as f64 * dt as f64, dt, cursor, 1.0, &au, &coups, &souple);
            (milieu(&a.gouttes).0, milieu(&b.gouttes).0)
        };
        for i in 0..120 {
            image(i, C);
        }
        let mut ecart = 0.0f32;
        for i in 0..30 {
            let (x0, x1) = image(120 + i, (300.0 + 15.0 * i as f32, 300.0));
            ecart = ecart.min(x1 - x0);
        }
        assert!(ecart < -6.0, "la souple doit traîner derrière la souris ({ecart})");
        // à l'arrêt, elle revient se souder comme la rigide
        let mut fin = (0.0, 0.0);
        for i in 0..360 {
            fin = image(150 + i, (735.0, 300.0));
        }
        assert!((fin.1 - fin.0).abs() < 2.0, "{fin:?}");
    }

    #[test]
    fn le_nombre_de_perles_et_ses_modulations() {
        let mut cfg = Config::default();
        (cfg.gouttes.actif, cfg.gouttes.perles) = (true, 0);
        assert_eq!(Gouttes::nombre(&cfg, &Effet::default()), 0.0);
        let plus = Effet { propres: crate::modulation::Propres { nombre: 1.0, ..Default::default() }, ..Effet::default() };
        assert_eq!(Gouttes::nombre(&cfg, &plus), PERLES_MODULE);
        cfg.gouttes.perles = 8;
        assert_eq!(Gouttes::nombre(&cfg, &plus), 8.0, "jamais plus de huit");
    }
}

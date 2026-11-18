//! Simulation et dessin : le point, les deux comètes, l'essaim de satellites et les fissures.
//!
//! Règles (« Essaim durable ») : chaque kick inverse le sens de rotation et ajoute des
//! satellites ; la quantité de basse règle la vitesse ; plus la note de basse est grave, plus
//! les orbites s'éloignent. Les traînées sont gardées en coordonnées écran : quand la souris
//! bouge, elles s'étirent derrière elle (la fenêtre s'agrandit le temps du mouvement).

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
    let synced = f32::from_bits(SYNC_HUE.load(std::sync::atomic::Ordering::Relaxed));
    let base = if synced.is_finite() { synced } else { (cfg.couleur.teinte + t as f32 * cfg.couleur.vitesse_teinte).rem_euclid(360.0) };
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
/// fixe n'est ajouté que tous les `TRAIL_STEP`, mesuré depuis l'avant-dernier (le dernier, lui,
/// est sans cesse rafraîchi).
fn push_trail(tr: &mut Trail, t: f64, x: f32, y: f32, life: f64) {
    let n = tr.len();
    if n >= 2 && t - tr[n - 2].0 < TRAIL_STEP {
        tr[n - 1] = (t, x, y);
    } else {
        tr.push_back((t, x, y));
    }
    while tr.front().is_some_and(|p| t - p.0 > life) {
        tr.pop_front();
    }
}

struct Sat {
    a: f32,
    r: f32,
    mul: f32,
    born: f64,
    trail: Trail,
}

/// Point du trajet des comètes à l'angle `a`, pour un rayon de 1 (y vers le bas).
pub fn chemin(forme: Forme, a: f32) -> (f32, f32) {
    let poly = |k: f32, r: f32| {
        // polygone régulier, pointe en haut, parcouru à vitesse constante
        let u = (a / TAU * k).rem_euclid(k);
        let (j, f) = (u.floor(), u.fract());
        let v = |j: f32| {
            let th = -PI / 2.0 + j * TAU / k;
            (th.cos() * r, th.sin() * r)
        };
        let (p, q) = (v(j), v(j + 1.0));
        (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f)
    };
    match forme {
        Forme::Cercle => (a.cos(), a.sin()),
        Forme::Triangle => poly(3.0, 1.2),
        Forme::Losange => poly(4.0, 1.15),
        Forme::Hexagone => poly(6.0, 1.05),
        Forme::Huit => (1.2 * a.cos(), 0.6 * (2.0 * a).sin()),
    }
}

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
    /// Traînées des comètes, anneau par anneau.
    trails: Vec<Trail>,
    sats: Vec<Sat>,
    /// Satellites permanents (`[essaim] fixes`).
    fixes: Vec<Sat>,
    kicks_seen: Option<u32>,
    center: Option<(f32, f32)>,
    pitch: f32,
    /// Éclat du dernier kick, 1 → 0.
    pub env: f32,
}

impl Swarm {
    pub fn new() -> Self {
        Self {
            rng: Rng::seeded(),
            dir: 1.0,
            spd: 3.5,
            far: 1.0,
            angles: Vec::new(),
            trails: Vec::new(),
            sats: Vec::new(),
            fixes: Vec::new(),
            kicks_seen: None,
            center: None,
            pitch: 50.0,
            env: 0.0,
        }
    }

    /// Rectangle écran (x0, y0, x1, y1) qui contient tout ce qui sera dessiné : traînées, têtes
    /// et éclat du kick (de taille `eclat`). Borné à `MAX_EXTENT` autour du pointeur.
    pub fn bounds(&self, cursor: (f32, f32), scale: f32, eclat: f32) -> (f32, f32, f32, f32) {
        // anneau de l'éclat : jusqu'à 7,5 fois sa taille, plus son épaisseur
        let ring = ((eclat * 7.5 + 1.5) * scale - (2.4 * 2.6 + 3.5 * 2.6 + MARGIN) * scale).max(0.0);
        let mut b = (cursor.0 - ring, cursor.1 - ring, cursor.0 + ring, cursor.1 + ring);
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

    pub fn update(&mut self, t: f64, dt: f32, cursor: (f32, f32), au: AudioView, cfg: &Config, scale: f32) {
        // centre de l'orbite : suit le pointeur avec un léger retard, borné
        let (mut cx, mut cy) = self.center.unwrap_or(cursor);
        let k = 1.0 - (-dt * 14.0).exp();
        cx += (cursor.0 - cx) * k;
        cy += (cursor.1 - cy) * k;
        let (ox, oy) = (cx - cursor.0, cy - cursor.1);
        let d = (ox * ox + oy * oy).sqrt();
        let lim = CENTER_LAG_MAX * scale;
        if d > lim {
            cx = cursor.0 + ox / d * lim;
            cy = cursor.1 + oy / d * lim;
        }
        self.center = Some((cx, cy));

        // kicks : sens inversé + satellites
        let new = self.kicks_seen.map_or(0, |k| au.kicks.wrapping_sub(k)).min(4);
        self.kicks_seen = Some(au.kicks);
        let e = &cfg.essaim;
        for _ in 0..new {
            self.dir = -self.dir;
            self.env = 1.0;
            let (lo, hi) = (e.par_kick_min.min(e.par_kick_max), e.par_kick_max.max(e.par_kick_min));
            let n = self.rng.range(lo as f32, hi as f32 + 0.999) as u32;
            for _ in 0..n {
                if self.sats.len() >= e.maximum {
                    break;
                }
                let (a, r, mul) = (self.rng.range(0.0, TAU), self.rng.range(0.75, SAT_R_MAX), self.rng.range(0.7, 1.3));
                self.sats.push(Sat { a, r, mul, born: t, trail: Trail::new() });
            }
        }
        self.env *= (-dt * 9.0).exp();

        // basse : vitesse ; note grave : éloignement (lissages courts, ~100 ms et ~60 ms)
        if au.pitch > 0.0 {
            self.pitch += (au.pitch - self.pitch) * (dt * 25.0).min(1.0);
        }
        let depth = ((80.0 - self.pitch) / 50.0).clamp(0.0, 1.0);
        let o = &cfg.orbite;
        self.spd += ((o.vitesse_base + o.vitesse_basse * au.bass) - self.spd) * (dt * 10.0).min(1.0);
        let presence = (au.bass * 2.5).clamp(0.0, 1.0);
        self.far += ((1.0 + o.eloignement_grave * depth * presence) - self.far) * (dt * 16.0).min(1.0);

        let w = self.dir * self.spd;
        // comètes : `nombre` par anneau, réparties sur le trajet
        let (anneaux, nombre) = (o.anneaux.max(1) as usize, o.nombre.max(1) as usize);
        self.angles.resize(anneaux, 0.0);
        self.trails.resize_with(anneaux * nombre, Trail::new);
        let r = o.rayon * scale * self.far;
        let life = o.trainee.max(0.01) as f64;
        for (k, angle) in self.angles.iter_mut().enumerate() {
            let (rk, wk) = anneau(k);
            *angle = (*angle + w * wk * dt).rem_euclid(TAU);
            for i in 0..nombre {
                let (x, y) = chemin(o.forme, *angle + i as f32 * TAU / nombre as f32);
                push_trail(&mut self.trails[k * nombre + i], t, cx + x * r * rk, cy + y * r * rk, life);
            }
        }
        let dur = e.duree.max(0.1) as f64;
        self.sats.retain(|s| t - s.born < dur);
        // satellites permanents : répartis une fois pour toutes (nombre d'or), toujours les mêmes
        if self.fixes.len() != e.fixes as usize {
            self.fixes = (0..e.fixes)
                .map(|j| {
                    let u = (j as f32 * 0.618_034).fract();
                    Sat { a: j as f32 * 2.399_963, r: 0.6 + u * (SAT_R_MAX - 0.6), mul: 0.6 + 0.8 * (1.0 - u), born: f64::INFINITY, trail: Trail::new() }
                })
                .collect();
        }
        // satellites : sur le trajet des comètes, ou le leur, à des distances variées
        let forme = e.forme.suivie(o.forme);
        for s in self.sats.iter_mut().chain(self.fixes.iter_mut()) {
            s.a = (s.a + w * s.mul * dt).rem_euclid(TAU);
            let rr = o.rayon * scale * s.r * self.far;
            let (x, y) = chemin(forme, s.a);
            push_trail(&mut s.trail, t, cx + x * rr, cy + y * rr, SAT_TRAIL);
        }
        // saut du pointeur (autre écran…) : la traînée laissée loin derrière est abandonnée
        let half = MAX_EXTENT / 2.0;
        let near = |&(_, x, y): &(f64, f32, f32)| (x - cursor.0).abs() <= half && (y - cursor.1).abs() <= half;
        for tr in self.trails.iter_mut().chain(self.sats.iter_mut().chain(self.fixes.iter_mut()).map(|s| &mut s.trail)) {
            tr.retain(near);
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

        struct Head {
            x: f32,
            y: f32,
            s: f32,
            al: f32,
            front: bool,
        }
        let mut heads: Vec<Head> = Vec::with_capacity(2 + self.sats.len());

        // satellites (traînées sans halo, pour rester léger) ; les permanents restent à 0,8
        for s in self.sats.iter().chain(&self.fixes) {
            let k = if s.born.is_finite() { (1.0 - (t - s.born) / dur) as f32 } else { 0.8 };
            let al = (k * 1.5).min(1.0);
            draw_trail(cv, &s.trail, t, SAT_TRAIL, 2.6 * k * scale, al, false, cfg);
            if let Some(&(_, x, y)) = s.trail.back() {
                heads.push(Head { x, y, s: (0.6 + 1.6 * k) * scale, al, front: y > self.center.map_or(0.0, |c| c.1) });
            }
        }
        // les comètes (devant : sous le centre de l'orbite, comme vues de biais)
        if o.cometes {
            let cy = self.center.map_or(0.0, |c| c.1);
            for tr in &self.trails {
                draw_trail(cv, tr, t, o.trainee.max(0.01) as f64, 3.5 * scale, 1.0, true, cfg);
                if let Some(&(_, x, y)) = tr.back() {
                    heads.push(Head { x, y, s: 2.4 * scale, al: 1.0, front: y > cy });
                }
            }
        }
        let head_rgb = prisme(cfg, t, 1.0);
        let draw_head = |cv: &mut dyn Canvas, h: &Head| {
            // lueur : deux disques translucides (un dégradé radial coûte bien plus cher sur processeur)
            cv.circle(h.x, h.y, h.s * 2.6, head_rgb, 0.12 * h.al * glow);
            cv.circle(h.x, h.y, h.s * 1.6, head_rgb, 0.3 * h.al * glow);
            cv.circle(h.x, h.y, h.s, head_rgb, h.al);
            cv.circle(h.x, h.y, h.s * 0.45, (255, 255, 255), h.al);
        };
        for h in heads.iter().filter(|h| !h.front) {
            draw_head(cv, h);
        }
        if self.env > 0.02 && cfg.eclat.taille > 0.0 {
            // éclat du kick : halo blanc et anneau coloré qui s'élargit sous le pointeur
            let (px, py) = cursor;
            let r = cfg.eclat.taille * scale;
            cv.circle(px, py, r * (2.0 + 2.5 * self.env), (255, 255, 255), 0.35 * self.env);
            cv.ring(px, py, r * (2.5 + 5.0 * (1.0 - self.env)), 1.5 * scale, head_rgb, 0.8 * self.env);
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

#[allow(clippy::too_many_arguments)]
fn draw_trail(cv: &mut dyn Canvas, tr: &Trail, t: f64, life: f64, width: f32, al: f32, halo: bool, cfg: &Config) {
    if tr.len() < 2 {
        return;
    }
    let q: Vec<f32> = tr.iter().map(|&(ti, _, _)| (1.0 - (t - ti) / life).max(0.0) as f32).collect();
    let xy: Vec<(f32, f32)> = tr.iter().map(|&(_, x, y)| (x, y)).collect();
    if halo {
        cv.polyline(&xy, (0.8 + width * 0.7) * 2.6, prisme(cfg, t, 0.7), 0.12 * al, false);
    }
    // les comètes (avec halo) sont lissées ; les satellites non, en deux morceaux (tête vive,
    // queue pâle) : c'est le poste le plus coûteux sur processeur
    let smooth = halo;
    let chunk = if smooth { TRAIL_CHUNK } else { (xy.len() - 1).div_ceil(2).max(1) };
    let mut i = 0;
    while i + 1 < xy.len() {
        let end = (i + chunk).min(xy.len() - 1);
        let k = (q[i] + q[end]) * 0.5;
        cv.polyline(&xy[i..=end], 0.8 + k * width, prisme(cfg, t, k), k * 0.9 * al, smooth);
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
        Self { x, y, segs, ext, born: t }
    }

    /// Rectangle écran (gauche, haut, droite, bas) qui contient la fissure.
    pub fn rect(&self) -> (i32, i32, i32, i32) {
        let e = self.ext.ceil() as i32 + 1;
        (self.x as i32 - e, self.y as i32 - e, self.x as i32 + e, self.y as i32 + e)
    }

    /// Dessine la fissure (coordonnées écran) ; `glow` 0..1 = éclat coloré, `opacity` = fondu.
    pub fn draw(&self, cv: &mut dyn Canvas, glow: f32, rgb: (u8, u8, u8), opacity: f32) {
        let (x, y) = (self.x, self.y);
        for s in &self.segs {
            cv.line((x + s[0], y + s[1]), (x + s[2], y + s[3]), s[4], (2, 3, 8), 0.92 * opacity, true);
        }
        for s in &self.segs {
            let w = (s[4] * 0.35).max(0.5);
            cv.line((x + s[0] + 0.8, y + s[1] + 0.8), (x + s[2] + 0.8, y + s[3] + 0.8), w, (170, 185, 250), 0.25 * opacity, true);
        }
        if glow > 0.02 {
            for s in &self.segs {
                cv.line((x + s[0], y + s[1]), (x + s[2], y + s[3]), 3.5, rgb, glow * 0.3 * opacity, true);
            }
            for s in &self.segs {
                cv.line((x + s[0], y + s[1]), (x + s[2], y + s[3]), 1.2, rgb, glow * opacity, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

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
    fn basse_grave_eloigne_et_accelere() {
        let cfg = Config::default();
        let mut s = Swarm::new();
        let quiet = AudioView { kicks: 0, bass: 0.0, pitch: 0.0 };
        let deep = AudioView { kicks: 0, bass: 1.0, pitch: 30.0 };
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
        let mut au = AudioView { kicks: 0, bass: 0.8, pitch: 40.0 };
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

    #[test]
    fn tout_tient_dans_le_rectangle() {
        // orbite la plus large (basse très grave) : têtes et traînées dans le rectangle
        let cfg = Config::default();
        let mut s = Swarm::new();
        let au = AudioView { kicks: 0, bass: 1.0, pitch: 30.0 };
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
        ] {
            let mut cfg = Config::default();
            (cfg.orbite.forme, cfg.orbite.nombre, cfg.orbite.anneaux, cfg.essaim.fixes, cfg.essaim.forme) = (forme, 8, 4, 32, sat);
            let mut s = Swarm::new();
            let au = AudioView { kicks: 0, bass: 1.0, pitch: 30.0 };
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
    fn les_trajets_sont_continus() {
        use crate::config::Forme;
        for forme in [Forme::Cercle, Forme::Triangle, Forme::Losange, Forme::Hexagone, Forme::Huit] {
            let mut p = chemin(forme, 0.0);
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

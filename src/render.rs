//! Simulation et dessin : le point, les deux comètes, l'essaim de satellites et les fissures.
//!
//! Règles (« Essaim durable ») : chaque kick inverse le sens de rotation et ajoute des
//! satellites ; la quantité de basse règle la vitesse ; plus la note de basse est grave, plus
//! les orbites s'éloignent. Les traînées sont gardées en coordonnées écran : quand la souris
//! bouge, elles s'étirent derrière elle (la fenêtre s'agrandit le temps du mouvement).

use crate::audio::AudioView;
use crate::config::Config;
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};
use tiny_skia::{Color, FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

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

/// Teinte « Prisme » : `k` = 0 en queue de traînée, 1 en tête.
pub fn prisme(cfg: &Config, t: f64, k: f32) -> (u8, u8, u8) {
    let base = (t as f32 * cfg.couleur.vitesse_teinte).rem_euclid(360.0);
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

fn paint(rgb: (u8, u8, u8), alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(rgb.0, rgb.1, rgb.2, (alpha.clamp(0.0, 1.0) * 255.0) as u8));
    p.anti_alias = true;
    p
}

fn circle(pm: &mut Pixmap, x: f32, y: f32, r: f32, p: &Paint) {
    if let Some(path) = PathBuilder::from_circle(x, y, r.max(0.1)) {
        pm.fill_path(&path, p, FillRule::Winding, Transform::identity(), None);
    }
}

fn segment(pm: &mut Pixmap, a: (f32, f32), b: (f32, f32), width: f32, p: &Paint) {
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(b.0, b.1);
    if let Some(path) = pb.finish() {
        let s = Stroke { width, line_cap: LineCap::Round, ..Stroke::default() };
        pm.stroke_path(&path, p, &s, Transform::identity(), None);
    }
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

pub struct Swarm {
    rng: Rng,
    dir: f32,
    spd: f32,
    far: f32,
    angle: f32,
    trails: [Trail; 2],
    sats: Vec<Sat>,
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
            angle: 0.0,
            trails: [Trail::new(), Trail::new()],
            sats: Vec::new(),
            kicks_seen: None,
            center: None,
            pitch: 50.0,
            env: 0.0,
        }
    }

    /// Rectangle écran (x0, y0, x1, y1) qui contient tout ce qui sera dessiné : traînées, têtes
    /// et point. Borné à `MAX_EXTENT` autour du pointeur.
    pub fn bounds(&self, cursor: (f32, f32), scale: f32) -> (f32, f32, f32, f32) {
        let mut b = (cursor.0, cursor.1, cursor.0, cursor.1);
        let pts = self.trails.iter().chain(self.sats.iter().map(|s| &s.trail)).flat_map(|tr| tr.iter());
        for &(_, x, y) in pts {
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        }
        // halo des comètes (le plus large) + marge
        let pad = (2.4 * 2.6 + 3.5 * 2.6 + MARGIN) * scale;
        let half = MAX_EXTENT / 2.0;
        (
            (b.0 - pad).max(cursor.0 - half),
            (b.1 - pad).max(cursor.1 - half),
            (b.2 + pad).min(cursor.0 + half),
            (b.3 + pad).min(cursor.1 + half),
        )
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
        self.angle = (self.angle + w * dt).rem_euclid(TAU);
        let r = o.rayon * scale * self.far;
        let life = o.trainee.max(0.01) as f64;
        for (i, tr) in self.trails.iter_mut().enumerate() {
            let a = self.angle + i as f32 * PI;
            push_trail(tr, t, cx + a.cos() * r, cy + a.sin() * r, life);
        }
        let dur = e.duree.max(0.1) as f64;
        self.sats.retain(|s| t - s.born < dur);
        for s in &mut self.sats {
            s.a = (s.a + w * s.mul * dt).rem_euclid(TAU);
            let rr = o.rayon * scale * s.r * self.far;
            push_trail(&mut s.trail, t, cx + s.a.cos() * rr, cy + s.a.sin() * rr, SAT_TRAIL);
        }
    }

    /// Dessine dans `pm`, dont le coin haut-gauche est à `origin` (coordonnées écran). `dot` =
    /// position du point à dessiner (aucun quand Windows affiche un autre curseur).
    pub fn draw(&self, pm: &mut Pixmap, origin: (f32, f32), t: f64, cfg: &Config, scale: f32, dot: Option<(f32, f32)>) {
        if self.center.is_none() {
            return;
        }
        let c = (-origin.0, -origin.1); // décalage écran → image
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

        // satellites (traînées sans halo, pour rester léger)
        for s in &self.sats {
            let k = (1.0 - (t - s.born) / dur) as f32;
            let al = (k * 1.5).min(1.0);
            draw_trail(pm, &s.trail, c, t, SAT_TRAIL, 2.6 * k * scale, al, false, cfg);
            if let Some(&(_, dx, dy)) = s.trail.back() {
                heads.push(Head { x: c.0 + dx, y: c.1 + dy, s: (0.6 + 1.6 * k) * scale, al, front: s.a.sin() > 0.0 });
            }
        }
        // les deux comètes
        if o.cometes {
            for (i, tr) in self.trails.iter().enumerate() {
                draw_trail(pm, tr, c, t, o.trainee.max(0.01) as f64, 3.5 * scale, 1.0, true, cfg);
                if let Some(&(_, dx, dy)) = tr.back() {
                    let a = self.angle + i as f32 * PI;
                    heads.push(Head { x: c.0 + dx, y: c.1 + dy, s: 2.4 * scale, al: 1.0, front: a.sin() > 0.0 });
                }
            }
        }
        let head_rgb = prisme(cfg, t, 1.0);
        let draw_head = |pm: &mut Pixmap, h: &Head| {
            // lueur : deux disques translucides (un dégradé radial coûte bien plus cher)
            circle(pm, h.x, h.y, h.s * 2.6, &paint(head_rgb, 0.12 * h.al * glow));
            circle(pm, h.x, h.y, h.s * 1.6, &paint(head_rgb, 0.3 * h.al * glow));
            circle(pm, h.x, h.y, h.s, &paint(head_rgb, h.al));
            circle(pm, h.x, h.y, h.s * 0.45, &paint((255, 255, 255), h.al));
        };
        for h in heads.iter().filter(|h| !h.front) {
            draw_head(pm, h);
        }
        if let Some(cursor) = dot {
            let (px, py) = (cursor.0 - origin.0, cursor.1 - origin.1);
            let r = cfg.curseur.taille_point * scale;
            circle(pm, px, py, r * 2.2, &paint((255, 255, 255), 0.10 + 0.25 * self.env));
            circle(pm, px, py, r, &paint((255, 255, 255), 1.0));
        }
        for h in heads.iter().filter(|h| h.front) {
            draw_head(pm, h);
        }
    }
}

/// Segments de traînée regroupés par morceaux : chaque morceau est un seul tracé (couleur,
/// épaisseur et opacité de son milieu). Bien moins coûteux qu'un tracé par segment, à l'œil
/// identique. Le lissage (anticrénelage) coûte ~3,5× plus cher par tracé : il est réservé aux
/// traînées des comètes ; satellites et halos, fins ou translucides et en mouvement, s'en passent.
const TRAIL_CHUNK: usize = 4;

fn polyline(pts: &[(f32, (f32, f32))]) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].1 .0, pts[0].1 .1);
    for p in &pts[1..] {
        pb.line_to(p.1 .0, p.1 .1);
    }
    pb.finish()
}

#[allow(clippy::too_many_arguments)]
fn draw_trail(pm: &mut Pixmap, tr: &Trail, c: (f32, f32), t: f64, life: f64, width: f32, al: f32, halo: bool, cfg: &Config) {
    let pts: Vec<(f32, (f32, f32))> =
        tr.iter().map(|&(ti, x, y)| ((1.0 - (t - ti) / life).max(0.0) as f32, (c.0 + x, c.1 + y))).collect();
    if pts.len() < 2 {
        return;
    }
    let stroke = |w: f32| Stroke { width: w, line_cap: LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Stroke::default() };
    if halo {
        if let Some(path) = polyline(&pts) {
            let mut p = paint(prisme(cfg, t, 0.7), 0.12 * al);
            p.anti_alias = false;
            pm.stroke_path(&path, &p, &stroke((0.8 + width * 0.7) * 2.6), Transform::identity(), None);
        }
    }
    // les comètes (avec halo) sont lissées ; les satellites non
    let smooth = halo;
    let chunk = if smooth { TRAIL_CHUNK } else { TRAIL_CHUNK + 2 };
    let mut i = 0;
    while i + 1 < pts.len() {
        let end = (i + chunk).min(pts.len() - 1);
        let q = (pts[i].0 + pts[end].0) * 0.5;
        if let Some(path) = polyline(&pts[i..=end]) {
            let mut p = paint(prisme(cfg, t, q), q * 0.9 * al);
            p.anti_alias = smooth;
            pm.stroke_path(&path, &p, &stroke(0.8 + q * width), Transform::identity(), None);
        }
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

    /// Dessine la fissure centrée dans `pm` ; `glow` 0..1 = éclat coloré.
    pub fn draw(&self, pm: &mut Pixmap, glow: f32, rgb: (u8, u8, u8)) {
        let c = self.ext;
        let dark = paint((2, 3, 8), 0.92);
        for s in &self.segs {
            segment(pm, (c + s[0], c + s[1]), (c + s[2], c + s[3]), s[4], &dark);
        }
        let light = paint((170, 185, 250), 0.25);
        for s in &self.segs {
            segment(pm, (c + s[0] + 0.8, c + s[1] + 0.8), (c + s[2] + 0.8, c + s[3] + 0.8), (s[4] * 0.35).max(0.5), &light);
        }
        if glow > 0.02 {
            let halo = paint(rgb, glow * 0.3);
            let core = paint(rgb, glow);
            for s in &self.segs {
                segment(pm, (c + s[0], c + s[1]), (c + s[2], c + s[3]), 3.5, &halo);
            }
            for s in &self.segs {
                segment(pm, (c + s[0], c + s[1]), (c + s[2], c + s[3]), 1.2, &core);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (x0, y0, x1, y1) = s.bounds(cursor, scale);
        let origin = (x0.floor(), y0.floor());
        let mut pm = Pixmap::new((x1 - origin.0).ceil() as u32, (y1 - origin.1).ceil() as u32).unwrap();
        s.draw(&mut pm, origin, t, cfg, scale, Some(cursor));
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
        let (x0, _, x1, _) = s.bounds(cursor, 1.0);
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
        let (x0, y0, x1, y1) = s.bounds((500.0, 500.0), 1.5);
        for tr in s.trails.iter().chain(s.sats.iter().map(|q| &q.trail)) {
            for &(_, x, y) in tr {
                assert!(x > x0 && x < x1 && y > y0 && y < y1, "point ({x}, {y}) hors du rectangle");
            }
        }
    }
}

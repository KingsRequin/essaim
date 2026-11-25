//! Interface de dessin commune aux deux moteurs : Direct2D sur la carte graphique (`gpu.rs`) et,
//! en secours, tiny-skia sur le processeur (`PixCanvas`). Coordonnées en pixels écran.

use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

pub type Rgb = (u8, u8, u8);

pub trait Canvas {
    /// Trait aux bouts arrondis. `smooth` = lissé (ignoré par la carte graphique, qui lisse tout).
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32, smooth: bool);
    /// Ligne brisée d'une seule couleur.
    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, rgb: Rgb, alpha: f32, smooth: bool);
    /// Morceau d'une traînée de l'essaim : une ligne brisée, que la carte graphique compte à part
    /// (son plafond de pastilles ne coupe que les traînées, jamais les têtes, les particules, les
    /// figures ni l'habillage).
    fn trainee(&mut self, pts: &[(f32, f32)], width: f32, rgb: Rgb, alpha: f32, smooth: bool) {
        self.polyline(pts, width, rgb, alpha, smooth);
    }
    fn circle(&mut self, x: f32, y: f32, r: f32, rgb: Rgb, alpha: f32);
    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, rgb: Rgb, alpha: f32);
    /// Trait à bouts droits : deux traits tracés bout à bout se rejoignent sans se recouvrir
    /// (filets de lumière). Par défaut, un trait à bouts ronds raccourci d'un quart de sa largeur
    /// à chaque bout : les pastilles de la carte graphique, posées tous les demi-largeurs, gardent
    /// alors leur pas d'un trait au suivant.
    fn bande(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let long = dx.hypot(dy);
        let r = (width / 4.0).min(long / 2.0);
        let (ux, uy) = if long > 1e-3 { (dx / long * r, dy / long * r) } else { (0.0, 0.0) };
        self.line((a.0 + ux, a.1 + uy), (b.0 - ux, b.1 - uy), width, rgb, alpha, false);
    }
    /// Voile de couleur `rgb` et d'opacité `alpha` sur le rectangle `zone` (gauche, haut, droite,
    /// bas, écran), percé d'un disque clair de rayon `r` centré en (x, y) dont le bord s'adoucit
    /// sur `flou` px vers l'extérieur (projecteur « retrouver la souris »).
    #[allow(clippy::too_many_arguments)]
    fn voile(&mut self, zone: (f32, f32, f32, f32), x: f32, y: f32, r: f32, flou: f32, rgb: Rgb, alpha: f32);
    /// Le moteur recopie vite une image toute prête (`image`) : le processeur oui ; la carte
    /// graphique non (ses traits, en lot de sprites, coûtent moins qu'une image à téléverser).
    fn images(&self) -> bool {
        false
    }
    /// Recopie `img` (RGBA prémultiplié), coin haut-gauche en (x, y) écran, à l'opacité `alpha`.
    /// Seulement si `images()`.
    fn image(&mut self, _img: &Pixmap, _x: i32, _y: i32, _alpha: f32) {}
    /// Recopie une petite image toute prête (RGBA prémultiplié), par-dessus le reste : du texte
    /// (étiquette du morceau). Tous les moteurs le font, une vignette à la fois ; `id` change
    /// quand l'image change (la carte graphique garde alors l'image déjà téléversée).
    fn vignette(&mut self, img: &Pixmap, id: u64, x: f32, y: f32, alpha: f32);
}

// --- tiny-skia (processeur) -------------------------------------------------------------------

pub(crate) fn paint(rgb: Rgb, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(rgb.0, rgb.1, rgb.2, (alpha.clamp(0.0, 1.0) * 255.0) as u8));
    p.anti_alias = true;
    p
}

pub(crate) fn circle(pm: &mut Pixmap, x: f32, y: f32, r: f32, p: &Paint) {
    if let Some(path) = PathBuilder::from_circle(x, y, r.max(0.1)) {
        pm.fill_path(&path, p, FillRule::Winding, Transform::identity(), None);
    }
}

pub(crate) fn segment(pm: &mut Pixmap, a: (f32, f32), b: (f32, f32), width: f32, p: &Paint) {
    trait_droit(pm, a, b, width, LineCap::Round, p);
}

fn trait_droit(pm: &mut Pixmap, a: (f32, f32), b: (f32, f32), width: f32, bouts: LineCap, p: &Paint) {
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(b.0, b.1);
    if let Some(path) = pb.finish() {
        let s = Stroke { width, line_cap: bouts, ..Stroke::default() };
        pm.stroke_path(&path, p, &s, Transform::identity(), None);
    }
}

/// Pose le pixel prémultiplié `s` par-dessus `d` (source par-dessus).
#[inline]
fn sur(d: &mut [u8], s: [u8; 4]) {
    if d[3] == 0 {
        d.copy_from_slice(&s);
    } else if s[3] != 0 {
        let k = 255 - s[3] as u32;
        for (dc, sc) in d.iter_mut().zip(s) {
            *dc = sc.saturating_add(((*dc as u32 * k + 127) / 255) as u8);
        }
    }
}

/// Dessin dans une image tiny-skia dont le coin haut-gauche est à `origin` (coordonnées écran).
pub struct PixCanvas<'a> {
    pub pm: &'a mut Pixmap,
    pub origin: (f32, f32),
}

impl PixCanvas<'_> {
    fn at(&self, p: (f32, f32)) -> (f32, f32) {
        (p.0 - self.origin.0, p.1 - self.origin.1)
    }
}

impl Canvas for PixCanvas<'_> {
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32, smooth: bool) {
        let mut p = paint(rgb, alpha);
        p.anti_alias = smooth;
        let (a, b) = (self.at(a), self.at(b));
        segment(self.pm, a, b, width, &p);
    }

    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, rgb: Rgb, alpha: f32, smooth: bool) {
        if pts.len() < 2 {
            return;
        }
        let mut pb = PathBuilder::new();
        let first = self.at(pts[0]);
        pb.move_to(first.0, first.1);
        for &q in &pts[1..] {
            let q = self.at(q);
            pb.line_to(q.0, q.1);
        }
        if let Some(path) = pb.finish() {
            let mut p = paint(rgb, alpha);
            p.anti_alias = smooth;
            let s = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
            self.pm.stroke_path(&path, &p, &s, Transform::identity(), None);
        }
    }

    fn circle(&mut self, x: f32, y: f32, r: f32, rgb: Rgb, alpha: f32) {
        let (x, y) = self.at((x, y));
        circle(self.pm, x, y, r, &paint(rgb, alpha));
    }

    fn bande(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32) {
        let (a, b) = (self.at(a), self.at(b));
        trait_droit(self.pm, a, b, width, LineCap::Butt, &paint(rgb, alpha));
    }

    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, rgb: Rgb, alpha: f32) {
        let (x, y) = self.at((x, y));
        if let Some(path) = PathBuilder::from_circle(x, y, r.max(0.1)) {
            let s = Stroke { width, ..Stroke::default() };
            self.pm.stroke_path(&path, &paint(rgb, alpha), &s, Transform::identity(), None);
        }
    }

    fn voile(&mut self, zone: (f32, f32, f32, f32), x: f32, y: f32, r: f32, flou: f32, rgb: Rgb, alpha: f32) {
        let a = alpha.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        // Rempli ligne par ligne, sans passer par tiny-skia : un dégradé radial tiny-skia sur un
        // écran entier coûtait ~14 ms par image. Sur chaque ligne : le disque clair est sauté,
        // l'anneau adouci prend son opacité dans une table, le reste est un aplat.
        let (w, h) = (self.pm.width() as i32, self.pm.height() as i32);
        let (l, t) = self.at((zone.0, zone.1));
        let (rr, b) = self.at((zone.2, zone.3));
        let (x0, y0) = ((l.round() as i32).clamp(0, w), (t.round() as i32).clamp(0, h));
        let (x1, y1) = ((rr.round() as i32).clamp(x0, w), (b.round() as i32).clamp(y0, h));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (cx, cy) = self.at((x, y));
        let r = r.max(0.0);
        let ext = r + flou.max(1.0);
        // couleur prémultipliée à l'opacité `k` (0 à 255)
        let px = |k: u32| {
            let al = (a * k as f32).round() as u32;
            let m = |c: u8| ((c as u32 * al + 127) / 255) as u8;
            [m(rgb.0), m(rgb.1), m(rgb.2), al as u8]
        };
        // opacité dans l'anneau : 0 au bord du disque, 40 % à mi-largeur, pleine au-delà
        const PAS: usize = 64;
        let table: [[u8; 4]; PAS + 1] = std::array::from_fn(|i| {
            let u = i as f32 / PAS as f32;
            let k = if u < 0.5 { 0.8 * u } else { 0.4 + 1.2 * (u - 0.5) };
            px((k * 255.0).round() as u32)
        });
        let plein = px(255);
        let largeur = (ext - r).max(1e-3);
        let donnees = self.pm.data_mut();
        for py in y0..y1 {
            let dy = py as f32 + 0.5 - cy;
            // étendues (en x) du disque clair et du disque adouci sur cette ligne
            let corde = |rayon: f32| {
                let d2 = rayon * rayon - dy * dy;
                (d2 > 0.0).then(|| {
                    let c = d2.sqrt();
                    (((cx - c - 0.5).ceil() as i32).clamp(x0, x1), ((cx + c - 0.5).floor() as i32 + 1).clamp(x0, x1))
                })
            };
            let (dedans, adouci) = (corde(r), corde(ext));
            let ligne = &mut donnees[(py * w) as usize * 4..(py * w + w) as usize * 4];
            let mut px_x = x0;
            while px_x < x1 {
                // fin du morceau de même nature à partir de px_x
                let (fin, nature) = match (dedans, adouci) {
                    (Some((a0, a1)), _) if px_x >= a0 && px_x < a1 => (a1, 0u8),
                    (_, Some((g0, g1))) if px_x >= g0 && px_x < g1 => (dedans.map_or(g1, |(a0, _)| if px_x < a0 { a0 } else { g1 }), 1),
                    (_, Some((g0, _))) if px_x < g0 => (g0, 2),
                    _ => (x1, 2),
                };
                let fin = fin.clamp(px_x + 1, x1);
                let morceau = ligne[px_x as usize * 4..fin as usize * 4].as_chunks_mut::<4>().0.iter_mut();
                if nature == 2 {
                    morceau.for_each(|d| sur(d, plein));
                } else if nature == 1 {
                    let dx0 = px_x as f32 + 0.5 - cx;
                    for (j, d) in morceau.enumerate() {
                        let dx = dx0 + j as f32;
                        let u = (((dx * dx + dy * dy).sqrt() - r) / largeur).clamp(0.0, 1.0);
                        sur(d, table[(u * PAS as f32) as usize]);
                    }
                }
                px_x = fin;
            }
        }
    }

    fn images(&self) -> bool {
        true
    }

    fn image(&mut self, img: &Pixmap, x: i32, y: i32, alpha: f32) {
        let p = PixmapPaint { opacity: alpha.clamp(0.0, 1.0), ..PixmapPaint::default() };
        let (x, y) = (x - self.origin.0.round() as i32, y - self.origin.1.round() as i32);
        self.pm.draw_pixmap(x, y, img.as_ref(), &p, Transform::identity(), None);
    }

    fn vignette(&mut self, img: &Pixmap, _id: u64, x: f32, y: f32, alpha: f32) {
        self.image(img, x.round() as i32, y.round() as i32, alpha);
    }
}

/// Dessin atténué (fondu d'entrée et de sortie de veille) : toutes les opacités multipliées par `k`.
pub struct Fondu<'a> {
    pub cv: &'a mut dyn Canvas,
    pub k: f32,
}

impl Canvas for Fondu<'_> {
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32, smooth: bool) {
        self.cv.line(a, b, width, rgb, alpha * self.k, smooth);
    }

    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, rgb: Rgb, alpha: f32, smooth: bool) {
        self.cv.polyline(pts, width, rgb, alpha * self.k, smooth);
    }

    fn trainee(&mut self, pts: &[(f32, f32)], width: f32, rgb: Rgb, alpha: f32, smooth: bool) {
        self.cv.trainee(pts, width, rgb, alpha * self.k, smooth);
    }

    fn circle(&mut self, x: f32, y: f32, r: f32, rgb: Rgb, alpha: f32) {
        self.cv.circle(x, y, r, rgb, alpha * self.k);
    }

    fn bande(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32) {
        self.cv.bande(a, b, width, rgb, alpha * self.k);
    }

    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, rgb: Rgb, alpha: f32) {
        self.cv.ring(x, y, r, width, rgb, alpha * self.k);
    }

    fn voile(&mut self, zone: (f32, f32, f32, f32), x: f32, y: f32, r: f32, flou: f32, rgb: Rgb, alpha: f32) {
        self.cv.voile(zone, x, y, r, flou, rgb, alpha * self.k);
    }

    fn images(&self) -> bool {
        self.cv.images()
    }

    fn image(&mut self, img: &Pixmap, x: i32, y: i32, alpha: f32) {
        self.cv.image(img, x, y, alpha * self.k);
    }

    fn vignette(&mut self, img: &Pixmap, id: u64, x: f32, y: f32, alpha: f32) {
        self.cv.vignette(img, id, x, y, alpha * self.k);
    }
}

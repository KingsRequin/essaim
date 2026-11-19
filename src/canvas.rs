//! Interface de dessin commune aux deux moteurs : Direct2D sur la carte graphique (`gpu.rs`) et,
//! en secours, tiny-skia sur le processeur (`PixCanvas`). Coordonnées en pixels écran.

use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

pub type Rgb = (u8, u8, u8);

pub trait Canvas {
    /// Trait aux bouts arrondis. `smooth` = lissé (ignoré par la carte graphique, qui lisse tout).
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32, smooth: bool);
    /// Ligne brisée d'une seule couleur.
    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, rgb: Rgb, alpha: f32, smooth: bool);
    fn circle(&mut self, x: f32, y: f32, r: f32, rgb: Rgb, alpha: f32);
    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, rgb: Rgb, alpha: f32);
    /// Le moteur recopie vite une image toute prête (`image`) : le processeur oui ; la carte
    /// graphique non (ses traits, en lot de sprites, coûtent moins qu'une image à téléverser).
    fn images(&self) -> bool {
        false
    }
    /// Recopie `img` (RGBA prémultiplié), coin haut-gauche en (x, y) écran, à l'opacité `alpha`.
    /// Seulement si `images()`.
    fn image(&mut self, _img: &Pixmap, _x: i32, _y: i32, _alpha: f32) {}
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
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(b.0, b.1);
    if let Some(path) = pb.finish() {
        let s = Stroke { width, line_cap: LineCap::Round, ..Stroke::default() };
        pm.stroke_path(&path, p, &s, Transform::identity(), None);
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

    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, rgb: Rgb, alpha: f32) {
        let (x, y) = self.at((x, y));
        if let Some(path) = PathBuilder::from_circle(x, y, r.max(0.1)) {
            let s = Stroke { width, ..Stroke::default() };
            self.pm.stroke_path(&path, &paint(rgb, alpha), &s, Transform::identity(), None);
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

    fn circle(&mut self, x: f32, y: f32, r: f32, rgb: Rgb, alpha: f32) {
        self.cv.circle(x, y, r, rgb, alpha * self.k);
    }

    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, rgb: Rgb, alpha: f32) {
        self.cv.ring(x, y, r, width, rgb, alpha * self.k);
    }

    fn images(&self) -> bool {
        self.cv.images()
    }

    fn image(&mut self, img: &Pixmap, x: i32, y: i32, alpha: f32) {
        self.cv.image(img, x, y, alpha * self.k);
    }
}

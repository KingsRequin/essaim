//! Variantes du pointeur, une par curseur système de Windows (onglet « Pointeurs »). Elles
//! servent d'images aux vrais curseurs Windows fabriqués par `cursors.rs`.
//!
//! Style commun : tracés blancs cernés d'un liseré sombre (lisibles sur fond clair comme sur
//! fond sombre), touches de couleur Prisme pour ce qui bouge. Chaque glyphe est centré sur le
//! point chaud du curseur Windows qu'il remplace (centre de la barre de texte, pointe du stylo…).

use crate::config::Config;
use crate::render::{circle, paint, prisme, segment};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};
use tiny_skia::{FillRule, PathBuilder, Pixmap, Transform};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// Sélection normale (flèche)
    Normal,
    /// Sélection de lien (main)
    Link,
    /// Sélection de texte
    Text,
    /// Occupé
    Busy,
    /// Travail en arrière-plan
    Working,
    /// Sélection de précision
    Cross,
    /// Sélection d'aide
    Help,
    /// Non disponible
    No,
    /// Déplacer
    Move,
    /// Redimensionnement horizontal
    SizeWE,
    /// Redimensionnement vertical
    SizeNS,
    /// Redimensionnement diagonal 1 (↖↘)
    SizeNWSE,
    /// Redimensionnement diagonal 2 (↗↙)
    SizeNESW,
    /// Écriture manuscrite
    Pen,
    /// Autre sélection
    Up,
}

impl Role {
    pub const ALL: [Role; 15] = [
        Role::Normal,
        Role::Link,
        Role::Text,
        Role::Busy,
        Role::Working,
        Role::Cross,
        Role::Help,
        Role::No,
        Role::Move,
        Role::SizeWE,
        Role::SizeNS,
        Role::SizeNWSE,
        Role::SizeNESW,
        Role::Pen,
        Role::Up,
    ];

    /// Identifiant Windows (`IDC_*` pour la lecture, `OCR_*` pour le remplacement : mêmes valeurs).
    pub fn id(self) -> u32 {
        match self {
            Role::Normal => 32512,
            Role::Text => 32513,
            Role::Busy => 32514,
            Role::Cross => 32515,
            Role::Up => 32516,
            Role::SizeNWSE => 32642,
            Role::SizeNESW => 32643,
            Role::SizeWE => 32644,
            Role::SizeNS => 32645,
            Role::Move => 32646,
            Role::No => 32648,
            Role::Link => 32649,
            Role::Working => 32650,
            Role::Help => 32651,
            Role::Pen => 32631,
        }
    }
}

const DARK: (u8, u8, u8) = (14, 16, 28);
const WHITE: (u8, u8, u8) = (255, 255, 255);
const RED: (u8, u8, u8) = (255, 93, 108);

/// Trait blanc cerné de sombre.
fn line(pm: &mut Pixmap, a: (f32, f32), b: (f32, f32), w: f32) {
    segment(pm, a, b, w + 2.0, &paint(DARK, 0.75));
    segment(pm, a, b, w, &paint(WHITE, 1.0));
}

fn polygon(pm: &mut Pixmap, pts: &[(f32, f32)], rgb: (u8, u8, u8), outline: bool) {
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].0, pts[0].1);
    for p in &pts[1..] {
        pb.line_to(p.0, p.1);
    }
    pb.close();
    if let Some(path) = pb.finish() {
        if outline {
            let st = tiny_skia::Stroke { width: 2.0, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
            pm.stroke_path(&path, &paint(DARK, 0.75), &st, Transform::identity(), None);
        }
        pm.fill_path(&path, &paint(rgb, 1.0), FillRule::Winding, Transform::identity(), None);
    }
}

/// Pointe de flèche pleine, tournée vers `ang`, de taille `s`.
fn arrowhead(pm: &mut Pixmap, x: f32, y: f32, ang: f32, s: f32) {
    let p = |a: f32, r: f32| (x + a.cos() * r, y + a.sin() * r);
    polygon(pm, &[p(ang, s), p(ang + 2.4, s * 0.85), p(ang - 2.4, s * 0.85)], WHITE, true);
}

/// Arc de cercle tracé en petits segments.
fn arc(pm: &mut Pixmap, c: (f32, f32), r: f32, a0: f32, a1: f32, w: f32, p: &tiny_skia::Paint) {
    let n = ((a1 - a0).abs() / 0.25).ceil().max(2.0) as usize;
    let mut pb = PathBuilder::new();
    for i in 0..=n {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        let (x, y) = (c.0 + a.cos() * r, c.1 + a.sin() * r);
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    if let Some(path) = pb.finish() {
        let st = tiny_skia::Stroke { width: w, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
        pm.stroke_path(&path, p, &st, Transform::identity(), None);
    }
}

fn dot(pm: &mut Pixmap, x: f32, y: f32, r: f32) {
    circle(pm, x, y, r * 2.2, &paint(WHITE, 0.12));
    circle(pm, x, y, r + 1.0, &paint(DARK, 0.35)); // liseré : visible aussi sur fond blanc
    circle(pm, x, y, r, &paint(WHITE, 1.0));
}

/// Dessine le pointeur `role` au point chaud (x, y), en coordonnées de l'image. `hue_t` = instant
/// pour la teinte Prisme ; `spin` = angle des éléments qui tournent (chargement).
#[allow(clippy::too_many_arguments)]
pub fn draw(pm: &mut Pixmap, role: Role, x: f32, y: f32, hue_t: f64, spin: f32, cfg: &Config, scale: f32) {
    let s = scale;
    let r = cfg.curseur.taille_point * s;
    let accent = prisme(cfg, hue_t, 1.0);
    match role {
        Role::Normal => dot(pm, x, y, r),
        Role::Link => {
            // le point grossit et prend un anneau coloré : « ça se clique »
            circle(pm, x, y, 9.0 * s, &paint(accent, 0.22));
            arc(pm, (x, y), 9.0 * s, 0.0, TAU, 1.6 * s, &paint(accent, 0.95));
            dot(pm, x, y, r * 1.25);
        }
        Role::Text => {
            let (h, sf) = (9.0 * s, 3.5 * s);
            arc(pm, (x, y), 7.0 * s, 0.0, TAU, 6.0 * s, &paint(accent, 0.10));
            line(pm, (x, y - h), (x, y + h), 1.6 * s);
            line(pm, (x - sf, y - h), (x + sf, y - h), 1.6 * s);
            line(pm, (x - sf, y + h), (x + sf, y + h), 1.6 * s);
        }
        Role::Busy => {
            arc(pm, (x, y), 10.0 * s, 0.0, TAU, 2.6 * s, &paint(DARK, 0.35));
            arc(pm, (x, y), 10.0 * s, spin, spin + 4.2, 2.6 * s, &paint(accent, 1.0));
            dot(pm, x, y, r * 0.8);
        }
        Role::Working => {
            dot(pm, x, y, r);
            let c = (x + 12.0 * s, y + 12.0 * s);
            arc(pm, c, 5.0 * s, 0.0, TAU, 2.0 * s, &paint(DARK, 0.35));
            arc(pm, c, 5.0 * s, spin, spin + 4.2, 2.0 * s, &paint(accent, 1.0));
        }
        Role::Cross => {
            for a in [0.0, FRAC_PI_2, PI, 3.0 * FRAC_PI_2] {
                let (c, sn) = (f32::cos(a), f32::sin(a));
                line(pm, (x + c * 4.0 * s, y + sn * 4.0 * s), (x + c * 12.0 * s, y + sn * 12.0 * s), 1.4 * s);
            }
            circle(pm, x, y, 1.6 * s, &paint(accent, 1.0));
        }
        Role::Help => {
            dot(pm, x, y, r);
            // point d'interrogation en haut à droite
            let c = (x + 12.0 * s, y - 13.0 * s);
            arc(pm, c, 3.6 * s, PI * 1.05, PI * 2.4, 4.0 * s, &paint(DARK, 0.75));
            arc(pm, c, 3.6 * s, PI * 1.05, PI * 2.4, 2.0 * s, &paint(accent, 1.0));
            let bottom = (c.0, c.1 + 3.6 * s);
            segment(pm, (c.0 + 0.6 * s, c.1 + 3.4 * s), (bottom.0, bottom.1 + 2.6 * s), 2.0 * s, &paint(accent, 1.0));
            circle(pm, c.0, c.1 + 9.2 * s, 1.3 * s, &paint(accent, 1.0));
        }
        Role::No => {
            arc(pm, (x, y), 9.0 * s, 0.0, TAU, 4.0 * s, &paint(DARK, 0.6));
            arc(pm, (x, y), 9.0 * s, 0.0, TAU, 2.2 * s, &paint(RED, 1.0));
            let d = 9.0 * s * FRAC_PI_4.cos();
            segment(pm, (x - d, y - d), (x + d, y + d), 2.2 * s, &paint(RED, 1.0));
        }
        Role::Move => {
            for a in [0.0, FRAC_PI_2, PI, 3.0 * FRAC_PI_2] {
                arrowhead(pm, x + a.cos() * 11.0 * s, y + a.sin() * 11.0 * s, a, 5.0 * s);
            }
            dot(pm, x, y, r * 0.8);
        }
        Role::SizeWE | Role::SizeNS | Role::SizeNWSE | Role::SizeNESW => {
            let a = match role {
                Role::SizeWE => 0.0,
                Role::SizeNS => FRAC_PI_2,
                Role::SizeNWSE => FRAC_PI_4,
                _ => -FRAC_PI_4,
            };
            let (c, sn) = (a.cos(), a.sin());
            let l = 9.0 * s;
            line(pm, (x - c * l, y - sn * l), (x + c * l, y + sn * l), 1.6 * s);
            arrowhead(pm, x + c * (l + 3.0 * s), y + sn * (l + 3.0 * s), a, 5.5 * s);
            arrowhead(pm, x - c * (l + 3.0 * s), y - sn * (l + 3.0 * s), a + PI, 5.5 * s);
            circle(pm, x, y, 1.8 * s, &paint(accent, 1.0));
        }
        Role::Pen => {
            // plume, pointe sur le point chaud, corps vers le haut à droite
            let p = |dx: f32, dy: f32| (x + dx * s, y + dy * s);
            polygon(pm, &[p(0.0, 0.0), p(4.0, -12.0), p(12.0, -4.0)], WHITE, true);
            segment(pm, p(1.5, -1.5), p(6.0, -6.0), 1.2 * s, &paint(accent, 1.0));
            circle(pm, x, y, 1.2 * s, &paint(accent, 1.0));
        }
        Role::Up => {
            dot(pm, x, y, r);
            arrowhead(pm, x, y - 12.0 * s, -FRAC_PI_2, 6.0 * s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_variante_se_dessine_autour_du_point_chaud() {
        let cfg = Config::default();
        for role in Role::ALL {
            let mut pm = Pixmap::new(80, 80).unwrap();
            draw(&mut pm, role, 40.0, 40.0, 0.3, 1.0, &cfg, 1.0);
            let mut n = 0;
            let mut far = 0f32;
            for y in 0..80u32 {
                for x in 0..80u32 {
                    if pm.pixel(x, y).is_some_and(|p| p.alpha() > 100) {
                        n += 1;
                        far = far.max(((x as f32 - 40.0).powi(2) + (y as f32 - 40.0).powi(2)).sqrt());
                    }
                }
            }
            assert!(n > 12, "{role:?} : presque rien de dessiné ({n} px)");
            assert!(far < 30.0, "{role:?} : dessin trop loin du point chaud ({far} px)");
        }
    }

    #[test]
    fn identifiants_windows_uniques() {
        let mut ids: Vec<u32> = Role::ALL.iter().map(|r| r.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), Role::ALL.len());
    }
}

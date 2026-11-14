//! Un calque transparent par écran, de la taille de l'écran et immobile. Tout y est dessiné
//! (essaim, fissures) ; seule la zone qui change est effacée, redessinée et envoyée à Windows.
//!
//! Pourquoi immobile : quand une fenêtre bouge ou apparaît sous la souris, Windows redemande
//! leur curseur aux applications en dessous, et certaines (l'Explorateur) répondent « flèche »
//! tant que la souris ne bouge pas — la barre de texte ou le redimensionnement redevenaient
//! le point.

use crate::win::Surface;
use std::time::Instant;
use tiny_skia::{BlendMode, Paint, Pixmap, Rect, Transform};

/// Rectangle (gauche, haut, droite, bas) en coordonnées écran.
pub type IRect = (i32, i32, i32, i32);

pub fn union(a: Option<IRect>, b: Option<IRect>) -> Option<IRect> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))),
        (a, None) => a,
        (None, b) => b,
    }
}

pub fn intersects(a: IRect, b: IRect) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

pub struct Layer {
    pub mon: isize,
    pub rect: IRect,
    surface: Surface,
    pm: Pixmap,
    /// Zone dessinée à l'image précédente (à effacer).
    prev: Option<IRect>,
    /// Depuis quand le calque n'a plus rien à afficher.
    pub empty_since: Option<Instant>,
}

impl Layer {
    pub fn new(mon: isize, rect: IRect) -> anyhow::Result<Self> {
        let pm = Pixmap::new((rect.2 - rect.0).max(1) as u32, (rect.3 - rect.1).max(1) as u32)
            .ok_or_else(|| anyhow::anyhow!("écran de taille invalide : {rect:?}"))?;
        Ok(Self { mon, rect, surface: Surface::new()?, pm, prev: None, empty_since: None })
    }

    /// Image suivante : `dirty` = zone écran de ce qui sera dessiné. La zone de l'image
    /// précédente et celle-ci sont effacées, `draw` dessine (origine du calque en argument),
    /// puis seule cette zone est envoyée.
    pub fn frame(&mut self, dirty: Option<IRect>, draw: impl FnOnce(&mut Pixmap, (f32, f32))) {
        let zone = union(self.prev, dirty);
        self.prev = dirty;
        self.empty_since = match (dirty, self.empty_since) {
            (Some(_), _) => None,
            (None, None) => Some(Instant::now()),
            (None, since) => since,
        };
        let Some(z) = zone else { return };
        let (ox, oy) = (self.rect.0, self.rect.1);
        let local = ((z.0 - ox).max(0), (z.1 - oy).max(0), (z.2 - ox).min(self.pm.width() as i32), (z.3 - oy).min(self.pm.height() as i32));
        if local.2 <= local.0 || local.3 <= local.1 {
            return;
        }
        if let Some(r) = Rect::from_ltrb(local.0 as f32, local.1 as f32, local.2 as f32, local.3 as f32) {
            let clear = Paint { blend_mode: BlendMode::Clear, ..Paint::default() };
            self.pm.fill_rect(r, &clear, Transform::identity(), None);
        }
        draw(&mut self.pm, (ox as f32, oy as f32));
        if let Err(e) = self.surface.update(&self.pm, ox, oy, Some(local)) {
            log::warn!("calque : envoi impossible : {e}");
        }
    }

    pub fn hide(&mut self) {
        self.pm.fill(tiny_skia::Color::TRANSPARENT);
        self.prev = None;
        self.surface.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles() {
        assert_eq!(union(Some((0, 0, 10, 10)), Some((5, -2, 20, 8))), Some((0, -2, 20, 10)));
        assert_eq!(union(None, Some((1, 1, 2, 2))), Some((1, 1, 2, 2)));
        assert!(intersects((0, 0, 10, 10), (9, 9, 20, 20)));
        assert!(!intersects((0, 0, 10, 10), (10, 0, 20, 10)));
    }
}

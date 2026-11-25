//! Un calque transparent par écran, de la taille de l'écran et immobile. Tout y est dessiné
//! (essaim, fissures) ; seule la zone qui change est effacée, redessinée et envoyée à Windows.
//! Le dessin est découpé à cette zone (tracé dans une image de sa taille, puis recopié) : rien
//! n'est laissé hors de la zone, où il ne serait jamais effacé et reparaîtrait au premier envoi
//! complet (après un envoi refusé, `win::Surface` renvoie tout le calque).
//!
//! Pourquoi immobile : quand une fenêtre bouge ou apparaît sous la souris, Windows redemande
//! leur curseur aux applications en dessous, et certaines (l'Explorateur) répondent « flèche »
//! tant que la souris ne bouge pas — la barre de texte ou le redimensionnement redevenaient
//! le point.

use crate::canvas::{Canvas, PixCanvas};
use crate::win::Surface;
use std::time::Instant;
use tiny_skia::{IntSize, Pixmap};

/// Rectangle (gauche, haut, droite, bas) en coordonnées écran.
pub type IRect = (i32, i32, i32, i32);

pub fn union(a: Option<IRect>, b: Option<IRect>) -> Option<IRect> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))),
        (a, None) => a,
        (None, b) => b,
    }
}

/// Zone vide : ne touche aucun écran (image sans essaim : l'habillage seul).
pub const AUCUNE: IRect = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);

/// Écran où se trouve le pointeur (le premier, s'il est hors de tous).
pub fn ecran_du_pointeur(monitors: &[(isize, IRect)], cursor: (f32, f32)) -> Option<IRect> {
    let (cx, cy) = (cursor.0 as i32, cursor.1 as i32);
    monitors.iter().map(|m| m.1).find(|r| cx >= r.0 && cx < r.2 && cy >= r.1 && cy < r.3).or(monitors.first().map(|m| m.1))
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
    /// Envois refusés par Windows depuis la dernière ligne du journal, et date de cette ligne.
    echecs: (u32, Option<Instant>),
    /// Pixels de l'image de la zone redessinée, gardés d'une image à l'autre pour leur mémoire.
    zone_px: Vec<u8>,
}

/// Envois refusés : une ligne de journal par minute au plus.
const NOTE_ECHECS: std::time::Duration = std::time::Duration::from_secs(60);

impl Layer {
    pub fn new(mon: isize, rect: IRect) -> anyhow::Result<Self> {
        let pm = Pixmap::new((rect.2 - rect.0).max(1) as u32, (rect.3 - rect.1).max(1) as u32)
            .ok_or_else(|| anyhow::anyhow!("écran de taille invalide : {rect:?}"))?;
        Ok(Self { mon, rect, surface: Surface::new()?, pm, prev: None, empty_since: None, echecs: (0, None), zone_px: Vec::new() })
    }

    /// Image suivante : `dirty` = zone écran de ce qui sera dessiné. La zone de l'image
    /// précédente et celle-ci sont effacées, `draw` dessine (coordonnées écran, découpé à cette
    /// zone), puis seule cette zone est envoyée.
    /// Renvoie le temps passé à envoyer l'image à Windows (pour les statistiques du journal).
    pub fn frame(&mut self, dirty: Option<IRect>, draw: impl FnOnce(&mut dyn Canvas)) -> std::time::Duration {
        let zone = union(self.prev, dirty);
        self.prev = dirty;
        self.empty_since = match (dirty, self.empty_since) {
            (Some(_), _) => None,
            (None, None) => Some(Instant::now()),
            (None, since) => since,
        };
        let Some(z) = zone else { return std::time::Duration::ZERO };
        let (ox, oy) = (self.rect.0, self.rect.1);
        let local = ((z.0 - ox).max(0), (z.1 - oy).max(0), (z.2 - ox).min(self.pm.width() as i32), (z.3 - oy).min(self.pm.height() as i32));
        if local.2 <= local.0 || local.3 <= local.1 {
            return std::time::Duration::ZERO;
        }
        peindre_zone(&mut self.pm, &mut self.zone_px, local, (ox, oy), draw);
        let sent = Instant::now();
        if let Err(e) = self.surface.update(&self.pm, ox, oy, Some(local)) {
            self.echecs.0 += 1;
            let now = Instant::now();
            if self.echecs.1.is_none_or(|t| now - t >= NOTE_ECHECS) {
                log::warn!("calque : envoi impossible ({} fois depuis la dernière note) : {e}", self.echecs.0);
                self.echecs = (0, Some(now));
            }
        }
        sent.elapsed()
    }

    /// Efface ce qui est affiché (pause, curseur masqué) : la zone dessinée à l'image précédente
    /// est vidée et envoyée transparente. La fenêtre reste affichée (une fenêtre cachée puis
    /// remontrée sous la souris fait redemander leur curseur aux applications).
    pub fn clear(&mut self) {
        self.frame(None, |_| {});
    }
}

/// Efface la zone `local` (pixels du calque `pm`, dont le coin est en `origine` à l'écran) et y
/// laisse dessiner `draw`, découpé à la zone : il dessine dans une image transparente de sa
/// taille (pixels gardés dans `tampon` pour leur mémoire), recopiée ensuite dans le calque.
fn peindre_zone(pm: &mut Pixmap, tampon: &mut Vec<u8>, local: IRect, origine: (i32, i32), draw: impl FnOnce(&mut dyn Canvas)) {
    let (zl, zh) = ((local.2 - local.0) as usize, (local.3 - local.1) as usize);
    let mut px = std::mem::take(tampon);
    px.clear();
    px.resize(zl * zh * 4, 0);
    let taille = IntSize::from_wh(zl as u32, zh as u32).expect("zone non vide");
    let mut zone = Pixmap::from_vec(px, taille).expect("taille de l'image de la zone");
    draw(&mut PixCanvas { pm: &mut zone, origin: ((local.0 + origine.0) as f32, (local.1 + origine.1) as f32) });
    let largeur = pm.width() as usize;
    let calque = pm.data_mut();
    for (ligne, src) in zone.data().chunks_exact(zl * 4).enumerate() {
        let debut = ((local.1 as usize + ligne) * largeur + local.0 as usize) * 4;
        calque[debut..debut + zl * 4].copy_from_slice(src);
    }
    *tampon = zone.take();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Coût d'un envoi (zone de 300 px sur un calque plein écran 1080p et 4K).
    /// `cargo test --release mesure_envoi -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mesure_envoi() {
        for (w, h) in [(1920, 1080), (3840, 2160)] {
            let mut lay = Layer::new(0, (0, 0, w, h)).unwrap();
            let n = 200;
            let mut total = std::time::Duration::ZERO;
            let t0 = Instant::now();
            for i in 0..n {
                let x = 200 + (i % 50) * 4;
                total += lay.frame(Some((x, 200, x + 300, 500)), |cv| {
                    cv.circle(x as f32 + 120.0, 320.0, 20.0, (255, 255, 255), 1.0);
                });
            }
            let all = t0.elapsed();
            eprintln!(
                "calque {w}×{h} : envoi {:.3} ms, image complète {:.3} ms",
                total.as_secs_f64() * 1000.0 / n as f64,
                all.as_secs_f64() * 1000.0 / n as f64
            );
            lay.clear();
        }
    }

    #[test]
    fn dessin_decoupe_a_la_zone() {
        // calque de 300 × 200 dont le coin est en (100, 100) à l'écran ; un disque qui déborde de
        // la zone ne laisse rien au-dehors, où rien ne l'effacerait
        let (mut pm, mut tampon) = (Pixmap::new(300, 200).unwrap(), Vec::new());
        pm.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 0));
        peindre_zone(&mut pm, &mut tampon, (50, 50, 100, 100), (100, 100), |cv| cv.circle(200.0, 200.0, 60.0, (255, 255, 255), 1.0));
        let px = |pm: &Pixmap, x: u32, y: u32| pm.pixel(x - 100, y - 100).unwrap().alpha();
        assert_eq!(px(&pm, 175, 175), 255, "dessiné dans la zone");
        assert_eq!(px(&pm, 205, 205), 0, "rien hors de la zone");
        assert_eq!(px(&pm, 175, 145), 0);
        // zone redessinée vide : effacée
        peindre_zone(&mut pm, &mut tampon, (40, 40, 110, 110), (100, 100), |_| {});
        assert!(pm.data().iter().all(|&o| o == 0), "calque vide");
    }

    #[test]
    fn rectangles() {
        assert_eq!(union(Some((0, 0, 10, 10)), Some((5, -2, 20, 8))), Some((0, -2, 20, 10)));
        assert_eq!(union(None, Some((1, 1, 2, 2))), Some((1, 1, 2, 2)));
        assert!(intersects((0, 0, 10, 10), (9, 9, 20, 20)));
        assert!(!intersects((0, 0, 10, 10), (10, 0, 20, 10)));
    }
}

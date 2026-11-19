//! Choix du moteur de dessin : carte graphique (DirectComposition + Direct2D, `gpu.rs`) d'abord,
//! processeur (tiny-skia + fenêtres superposées, `layer.rs`) en secours. Une erreur de la carte
//! graphique en cours de route (pilote réinitialisé, sortie de veille…) fait basculer sur le
//! processeur ; la carte graphique est retentée 10 s plus tard, puis toutes les 30 s.
//!
//! Dans les deux cas : un calque immobile par écran touché (par l'essaim, qui peut déborder sur
//! l'écran voisin, ou par une fissure), refait si l'écran change de place ou de taille, libéré
//! 10 s après être devenu vide.

use crate::canvas::Canvas;
use crate::gpu::{Gpu, GpuLayer};
use crate::layer::{intersects, union, IRect, Layer};
use std::time::{Duration, Instant};

const KEEP_EMPTY: Duration = Duration::from_secs(10);
const RETRY_FIRST: Duration = Duration::from_secs(10);
const RETRY_NEXT: Duration = Duration::from_secs(30);

enum Kind {
    Gpu(Gpu, Vec<GpuLayer>),
    Cpu(Vec<Layer>),
}

pub struct Backend {
    kind: Kind,
    want_gpu: bool,
    /// Prochain essai de la carte graphique après une erreur.
    retry_at: Option<Instant>,
}

fn try_gpu() -> Option<Kind> {
    match Gpu::new() {
        Ok(g) => {
            log::info!("dessin : carte graphique (DirectComposition + Direct2D)");
            Some(Kind::Gpu(g, Vec::new()))
        }
        Err(e) => {
            log::warn!("dessin : carte graphique indisponible ({e}), processeur utilisé");
            None
        }
    }
}

impl Backend {
    pub fn new(want_gpu: bool) -> Self {
        let kind = want_gpu.then(try_gpu).flatten();
        let retry_at = (want_gpu && kind.is_none()).then(|| Instant::now() + RETRY_NEXT);
        let kind = kind.unwrap_or_else(|| {
            log::info!("dessin : processeur (tiny-skia)");
            Kind::Cpu(Vec::new())
        });
        Self { kind, want_gpu, retry_at }
    }

    pub fn is_gpu(&self) -> bool {
        matches!(self.kind, Kind::Gpu(..))
    }

    pub fn wants_gpu(&self) -> bool {
        self.want_gpu
    }

    /// Oublie tous les calques (configuration des écrans changée) : ils sont refaits au besoin.
    pub fn reset_layers(&mut self) {
        match &mut self.kind {
            Kind::Gpu(gpu, layers) => {
                layers.clear();
                let _ = gpu.commit();
            }
            Kind::Cpu(layers) => layers.clear(),
        }
    }

    /// Dessine une image. `monitors` = écrans (identifiant, rectangle) ; `swarm` = zone de
    /// l'essaim, dessiné sur chaque écran qu'il touche ; `items` = zones des fissures.
    /// `draw(canvas, essaim_ici)` dessine tout. Renvoie le temps de remise au compositeur.
    pub fn frame(&mut self, monitors: &[(isize, IRect)], swarm: IRect, items: &[IRect], draw: &dyn Fn(&mut dyn Canvas, bool)) -> Duration {
        let now = Instant::now();
        if self.retry_at.is_some_and(|r| now >= r) {
            match try_gpu() {
                Some(k) => {
                    self.kind = k;
                    self.retry_at = None;
                }
                None => self.retry_at = Some(now + RETRY_NEXT),
            }
        }
        let touched = |rect: IRect| intersects(rect, swarm) || items.iter().any(|&r| intersects(rect, r));
        let dirty_for = |rect: IRect| {
            let mut d = intersects(rect, swarm).then_some(swarm);
            for &r in items.iter().filter(|&&r| intersects(rect, r)) {
                d = union(d, Some(r));
            }
            d
        };
        let keep = |mon: isize, rect: IRect, empty_since: Option<Instant>| {
            monitors.iter().any(|&(m, r)| m == mon && r == rect) && empty_since.is_none_or(|s| now - s < KEEP_EMPTY)
        };
        let mut sent = Duration::ZERO;
        let mut failed = None;
        match &mut self.kind {
            Kind::Gpu(gpu, layers) => {
                layers.retain(|l| keep(l.mon, l.rect, l.empty_since));
                for &(mon, rect) in monitors.iter().filter(|m| touched(m.1)) {
                    if !layers.iter().any(|l| l.mon == mon) {
                        match gpu.layer(mon, rect) {
                            Ok(l) => layers.push(l),
                            Err(e) => failed = Some(e),
                        }
                    }
                }
                for lay in layers.iter_mut() {
                    let here = intersects(lay.rect, swarm);
                    match lay.frame(gpu, dirty_for(lay.rect), |cv| draw(cv, here)) {
                        Ok(d) => sent += d,
                        Err(e) => failed = Some(e),
                    }
                }
                let t0 = Instant::now();
                if let Err(e) = gpu.commit() {
                    failed = Some(e);
                }
                sent += t0.elapsed();
            }
            Kind::Cpu(layers) => {
                layers.retain(|l| keep(l.mon, l.rect, l.empty_since));
                for &(mon, rect) in monitors.iter().filter(|m| touched(m.1)) {
                    if !layers.iter().any(|l| l.mon == mon) {
                        match Layer::new(mon, rect) {
                            Ok(l) => layers.push(l),
                            Err(e) => log::warn!("calque impossible pour l'écran {rect:?} : {e:#}"),
                        }
                    }
                }
                for lay in layers.iter_mut() {
                    let here = intersects(lay.rect, swarm);
                    sent += lay.frame(dirty_for(lay.rect), |cv| draw(cv, here));
                }
            }
        }
        if let Some(e) = failed {
            log::warn!("dessin : erreur de la carte graphique ({e}), bascule sur le processeur ; nouvel essai dans 10 s");
            self.kind = Kind::Cpu(Vec::new());
            self.retry_at = self.want_gpu.then(|| now + RETRY_FIRST);
        }
        sent
    }

    /// Efface tout (pause, curseur masqué).
    pub fn clear(&mut self) {
        match &mut self.kind {
            Kind::Gpu(gpu, layers) => {
                for l in layers.iter_mut() {
                    let _ = l.clear(gpu);
                }
                let _ = gpu.commit();
            }
            Kind::Cpu(layers) => layers.iter_mut().for_each(Layer::hide),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioView;
    use crate::config::Config;
    use crate::render::Swarm;

    /// Coût complet d'une image (dessin + remise au compositeur) pour chaque moteur, essaim plein.
    /// `cargo test --release mesure_moteurs -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mesure_impact_reglages() {
        // coût d'une image (carte graphique, simulation comprise) avec chaque réglage au minimum
        // puis au maximum, le reste par défaut ; kick toutes les 30 images, basse forte
        fn cout(cfg: &Config, gpu: bool) -> f64 {
            let dt = 1.0 / 144.0;
            let mut s = Swarm::new();
            let mut au = AudioView { kicks: 0, bass: 0.8, pitch: 40.0, ..Default::default() };
            let mut b = Backend::new(gpu);
            let mons = [(1isize, (0, 0, 1920, 1080))];
            let (chauffe, n) = (150, 250);
            let mut durees = Vec::with_capacity(n);
            for i in 0..(chauffe + n) {
                if i % 30 == 0 {
                    au.kicks += 1;
                }
                let t = i as f64 * dt as f64;
                let cursor = (600.0 + 200.0 * (t as f32).sin(), 500.0);
                let t0 = Instant::now();
                s.update(t, dt, cursor, au, cfg, 1.0);
                let (x0, y0, x1, y1) = s.bounds(cursor, 1.0, cfg.eclat.taille);
                let rect = (x0 as i32, y0 as i32, x1 as i32 + 1, y1 as i32 + 1);
                b.frame(&mons, rect, &[], &|cv, _| s.draw(cv, t, cfg, 1.0, cursor));
                if i >= chauffe {
                    durees.push(t0.elapsed().as_secs_f64() * 1000.0);
                }
            }
            mediane(durees)
        }
        fn mediane(mut v: Vec<f64>) -> f64 {
            v.sort_by(|a, b| a.total_cmp(b));
            v[v.len() / 2]
        }
        type Reglage = (&'static str, fn(&mut Config, f32));
        let reglages: [(Reglage, f32, f32); 16] = [
            (("orbite.nombre", |c, v| c.orbite.nombre = v as u32), 1.0, 8.0),
            (("orbite.anneaux", |c, v| c.orbite.anneaux = v as u32), 1.0, 4.0),
            (("orbite.rayon", |c, v| c.orbite.rayon = v), 4.0, 80.0),
            (("orbite.vitesse_base", |c, v| c.orbite.vitesse_base = v), 0.0, 15.0),
            // réactions de base à la basse, devenues des modulations
            (("modulations.basse", |c, v| c.modulations.liste = crate::config::reactions_de_base(v, crate::config::ELOIGNEMENT_GRAVE)), 0.0, 40.0),
            (("modulations.grave", |c, v| c.modulations.liste = crate::config::reactions_de_base(crate::config::VITESSE_BASSE, v)), 0.0, 4.0),
            (("orbite.trainee", |c, v| c.orbite.trainee = v), 0.05, 2.0),
            (("orbite.cometes", |c, v| c.orbite.cometes = v > 0.5), 0.0, 1.0),
            (("essaim.maximum", |c, v| c.essaim.maximum = v as usize), 0.0, 64.0),
            (("essaim.fixes", |c, v| c.essaim.fixes = v as u32), 0.0, 32.0),
            (("essaim.par_kick_max", |c, v| (c.essaim.par_kick_min, c.essaim.par_kick_max) = (v as u32, v as u32)), 0.0, 10.0),
            (("essaim.duree", |c, v| c.essaim.duree = v), 0.5, 10.0),
            (("eclat.taille", |c, v| c.eclat.taille = v), 0.0, 12.0),
            (("orbite.forme", |c, v| c.orbite.forme = if v > 0.5 { crate::config::Forme::Huit } else { crate::config::Forme::Cercle }), 0.0, 1.0),
            (("couleur.vitesse_teinte", |c, v| c.couleur.vitesse_teinte = v), 0.0, 360.0),
            (("couleur.ecart_teinte", |c, v| c.couleur.ecart_teinte = v), 0.0, 360.0),
        ];
        let base = cout(&Config::default(), true);
        eprintln!("défaut : {base:.3} ms (carte graphique), {:.3} ms (processeur)", cout(&Config::default(), false));
        for ((nom, regler), min, max) in reglages {
            let (mut bas, mut haut) = (Config::default(), Config::default());
            regler(&mut bas, min);
            regler(&mut haut, max);
            // minimum et maximum en alternance, médiane des tours : la dérive (chaleur, autres
            // programmes) touche les deux pareil
            let (mut vb, mut vh) = (Vec::new(), Vec::new());
            for _ in 0..5 {
                vb.push(cout(&bas, true));
                vh.push(cout(&haut, true));
            }
            let (b, h) = (mediane(vb), mediane(vh));
            eprintln!("{nom:28} min {b:.3} ms, max {h:.3} ms, écart {:+.3} ms", h - b);
        }
    }

    #[test]
    #[ignore]
        fn mesure_moteurs() {
        let cfg = Config::default();
        let dt = 1.0 / 144.0;
        for gpu in [true, false] {
            let mut s = Swarm::new();
            let mut au = AudioView { kicks: 0, bass: 0.8, pitch: 40.0, ..Default::default() };
            let mut b = Backend::new(gpu);
            let mons = [(1isize, (0, 0, 1920, 1080))];
            let n = 300;
            let t0 = Instant::now();
            let mut sent = Duration::ZERO;
            for i in 0..(200 + n) {
                if i % 30 == 0 {
                    au.kicks += 1;
                }
                let t = i as f64 * dt as f64;
                let cursor = (600.0 + 200.0 * (t as f32).sin(), 500.0);
                s.update(t, dt, cursor, au, &cfg, 1.0);
                let (x0, y0, x1, y1) = s.bounds(cursor, 1.0, cfg.eclat.taille);
                let rect = (x0 as i32, y0 as i32, x1 as i32 + 1, y1 as i32 + 1);
                let d = b.frame(&mons, rect, &[], &|cv, _| s.draw(cv, t, &cfg, 1.0, cursor));
                if i >= 200 {
                    sent += d;
                }
            }
            let per = t0.elapsed().as_secs_f64() * 1000.0 / (200 + n) as f64;
            eprintln!(
                "{} : {per:.3} ms par image (simulation comprise), dont remise {:.3} ms",
                if b.is_gpu() { "carte graphique" } else { "processeur" },
                sent.as_secs_f64() * 1000.0 / n as f64
            );
        }
    }
}

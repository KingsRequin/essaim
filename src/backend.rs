//! Choix du moteur de dessin : carte graphique (DirectComposition + Direct2D, `gpu.rs`) d'abord,
//! processeur (tiny-skia + fenêtres superposées, `layer.rs`) en secours. Une erreur de la carte
//! graphique en cours de route (pilote réinitialisé…) fait basculer sur le processeur.
//!
//! Dans les deux cas : un calque immobile par écran visité, libéré 10 s après l'avoir quitté.

use crate::canvas::Canvas;
use crate::gpu::{Gpu, GpuLayer};
use crate::layer::{intersects, union, IRect, Layer};
use std::time::{Duration, Instant};

const KEEP_EMPTY: Duration = Duration::from_secs(10);

pub enum Backend {
    Gpu(Gpu, Vec<GpuLayer>),
    Cpu(Vec<Layer>),
}

impl Backend {
    pub fn new(gpu: bool) -> Self {
        if gpu {
            match Gpu::new() {
                Ok(g) => {
                    log::info!("dessin : carte graphique (DirectComposition + Direct2D)");
                    return Backend::Gpu(g, Vec::new());
                }
                Err(e) => log::warn!("dessin : carte graphique indisponible ({e}), processeur utilisé"),
            }
        }
        log::info!("dessin : processeur (tiny-skia)");
        Backend::Cpu(Vec::new())
    }

    pub fn is_gpu(&self) -> bool {
        matches!(self, Backend::Gpu(..))
    }

    /// Dessine une image. `mon` = écran de la souris (il porte l'essaim, zone `swarm`) ;
    /// `items` = zones des autres éléments (fissures), dessinés sur l'écran qu'ils touchent.
    /// `draw(canvas, ici)` dessine tout ; `ici` = calque de l'écran de la souris.
    /// Renvoie le temps de remise au compositeur.
    pub fn frame(&mut self, mon: (isize, IRect), swarm: IRect, items: &[IRect], draw: &dyn Fn(&mut dyn Canvas, bool)) -> Duration {
        let now = Instant::now();
        let dirty_for = |rect: IRect, here: bool| {
            let mut d = here.then_some(swarm);
            for &r in items.iter().filter(|&&r| intersects(rect, r)) {
                d = union(d, Some(r));
            }
            d
        };
        let mut sent = Duration::ZERO;
        let mut failed = None;
        match self {
            Backend::Gpu(gpu, layers) => {
                if !layers.iter().any(|l| l.mon == mon.0) {
                    match gpu.layer(mon.0, mon.1) {
                        Ok(l) => layers.push(l),
                        Err(e) => failed = Some(e),
                    }
                }
                for lay in layers.iter_mut() {
                    let here = lay.mon == mon.0;
                    match lay.frame(gpu, dirty_for(lay.rect, here), |cv| draw(cv, here)) {
                        Ok(d) => sent += d,
                        Err(e) => failed = Some(e),
                    }
                }
                let t0 = Instant::now();
                if let Err(e) = gpu.commit() {
                    failed = Some(e);
                }
                sent += t0.elapsed();
                layers.retain(|l| l.mon == mon.0 || l.empty_since.is_none_or(|s| now - s < KEEP_EMPTY));
            }
            Backend::Cpu(layers) => {
                if !layers.iter().any(|l| l.mon == mon.0) {
                    match Layer::new(mon.0, mon.1) {
                        Ok(l) => layers.push(l),
                        Err(e) => log::warn!("calque impossible pour l'écran {:?} : {e:#}", mon.1),
                    }
                }
                for lay in layers.iter_mut() {
                    let here = lay.mon == mon.0;
                    sent += lay.frame(dirty_for(lay.rect, here), |cv| draw(cv, here));
                }
                layers.retain(|l| l.mon == mon.0 || l.empty_since.is_none_or(|s| now - s < KEEP_EMPTY));
            }
        }
        if let Some(e) = failed {
            log::warn!("dessin : erreur de la carte graphique ({e}), bascule sur le processeur");
            *self = Backend::Cpu(Vec::new());
        }
        sent
    }

    /// Efface tout (pause, curseur masqué).
    pub fn clear(&mut self) {
        match self {
            Backend::Gpu(gpu, layers) => {
                for l in layers.iter_mut() {
                    let _ = l.clear(gpu);
                }
                let _ = gpu.commit();
            }
            Backend::Cpu(layers) => layers.iter_mut().for_each(Layer::hide),
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
    fn mesure_moteurs() {
        let cfg = Config::default();
        let dt = 1.0 / 144.0;
        for gpu in [true, false] {
            let mut s = Swarm::new();
            let mut au = AudioView { kicks: 0, bass: 0.8, pitch: 40.0 };
            let mut b = Backend::new(gpu);
            let mon = (1isize, (0, 0, 1920, 1080));
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
                let (x0, y0, x1, y1) = s.bounds(cursor, 1.0);
                let rect = (x0 as i32, y0 as i32, x1 as i32 + 1, y1 as i32 + 1);
                let d = b.frame(mon, rect, &[], &|cv, _| s.draw(cv, t, &cfg, 1.0, cursor));
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

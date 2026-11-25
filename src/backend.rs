//! Choix du moteur de dessin : carte graphique (DirectComposition + Direct2D, `gpu.rs`) d'abord,
//! processeur (tiny-skia + fenêtres superposées, `layer.rs`) en secours.
//!
//! Périphérique perdu (pilote réinitialisé, sortie de veille…) : un nouveau `Gpu` est fait et les
//! mêmes fenêtres lui sont rattachées (nouvelle cible DirectComposition sur chacune), sans en
//! créer ni en détruire sous la souris. Toute autre erreur, une reconstruction qui échoue ou une
//! deuxième perte en moins de `RECONSTRUCTION` font basculer sur le processeur ; la carte
//! graphique est alors retentée 10 s plus tard, puis à intervalles croissants (30 s, 90 s…
//! jusqu'à 10 min) tant qu'elle échoue. Pendant un essai, les calques processeur restent : ils ne
//! sont remplacés qu'après une image réussie sur la carte graphique (sinon, une carte qui échoue
//! à chaque fois créerait et détruirait des fenêtres à chaque essai).
//!
//! Dans les deux cas : un calque immobile par écran touché (par l'essaim, qui peut déborder sur
//! l'écran voisin, ou par une fissure), refait seulement si son écran change de place ou de
//! taille, libéré 10 s après être devenu vide.

use crate::canvas::Canvas;
use crate::gpu::{Gpu, GpuLayer};
use crate::layer::{intersects, union, IRect, Layer};
use std::time::{Duration, Instant};

const KEEP_EMPTY: Duration = Duration::from_secs(10);
/// Attente avant le premier nouvel essai de la carte graphique, triplée à chaque échec suivant
/// jusqu'à `RETRY_MAX`.
const RETRY_FIRST: Duration = Duration::from_secs(10);
const RETRY_MAX: Duration = Duration::from_secs(600);
/// Carte graphique sans erreur pendant ce temps : ses échecs passés sont oubliés.
const STABLE: Duration = Duration::from_secs(300);
/// Deux pertes du périphérique plus rapprochées : la carte graphique n'est plus refaite sur
/// place, le processeur prend le relais.
const RECONSTRUCTION: Duration = Duration::from_secs(10);

/// Attente avant le prochain essai de la carte graphique après `echecs` échecs d'affilée.
fn retry_delay(echecs: u32) -> Duration {
    RETRY_FIRST.saturating_mul(3u32.saturating_pow(echecs)).min(RETRY_MAX)
}

enum Kind {
    Gpu(Gpu, Vec<GpuLayer>),
    Cpu(Vec<Layer>),
}

pub struct Backend {
    kind: Kind,
    want_gpu: bool,
    /// Prochain essai de la carte graphique après une erreur.
    retry_at: Option<Instant>,
    /// Échecs de la carte graphique d'affilée (allongent l'attente avant l'essai suivant).
    echecs: u32,
    /// Depuis quand la carte graphique dessine sans erreur.
    gpu_depuis: Option<Instant>,
    /// Dernière reconstruction après une perte du périphérique.
    reconstruit: Option<Instant>,
}

/// L'écran `rect` est-il touché par l'essaim ou une fissure ?
fn touche(rect: IRect, swarm: IRect, items: &[IRect]) -> bool {
    intersects(rect, swarm) || items.iter().any(|&r| intersects(rect, r))
}

/// Zone à dessiner sur l'écran `rect` : l'essaim et les fissures qui le touchent.
fn zone_a_dessiner(rect: IRect, swarm: IRect, items: &[IRect]) -> Option<IRect> {
    let mut d = intersects(rect, swarm).then_some(swarm);
    for &r in items.iter().filter(|&&r| intersects(rect, r)) {
        d = union(d, Some(r));
    }
    d
}

/// Le calque de l'écran `mon` au rectangle `rect` reste : son écran est toujours là, au même
/// endroit, et il n'est pas vide depuis trop longtemps.
fn garder(monitors: &[(isize, IRect)], mon: isize, rect: IRect, empty_since: Option<Instant>, now: Instant) -> bool {
    monitors.iter().any(|&(m, r)| m == mon && r == rect) && empty_since.is_none_or(|s| now - s < KEEP_EMPTY)
}

/// Une image sur la carte graphique (calques créés au besoin, puis remise au compositeur).
fn frame_gpu(
    gpu: &Gpu,
    layers: &mut Vec<GpuLayer>,
    monitors: &[(isize, IRect)],
    swarm: IRect,
    items: &[IRect],
    draw: &dyn Fn(&mut dyn Canvas, bool),
    now: Instant,
) -> windows::core::Result<Duration> {
    layers.retain(|l| garder(monitors, l.mon, l.rect, l.empty_since, now));
    for &(mon, rect) in monitors.iter().filter(|m| touche(m.1, swarm, items)) {
        if !layers.iter().any(|l| l.mon == mon) {
            layers.push(gpu.layer(mon, rect)?);
        }
    }
    let mut sent = Duration::ZERO;
    for lay in layers.iter_mut() {
        let here = intersects(lay.rect, swarm);
        sent += lay.frame(gpu, zone_a_dessiner(lay.rect, swarm, items), |cv| draw(cv, here))?;
    }
    let t0 = Instant::now();
    gpu.commit()?;
    // fenêtres montrées seulement une fois leur premier contenu remis au compositeur
    layers.iter_mut().for_each(GpuLayer::montrer);
    Ok(sent + t0.elapsed())
}

/// Efface tous les calques de la carte graphique et remet l'image au compositeur.
fn clear_gpu(gpu: &Gpu, layers: &mut [GpuLayer]) -> windows::core::Result<Duration> {
    for l in layers.iter_mut() {
        l.clear(gpu)?;
    }
    gpu.commit()?;
    layers.iter_mut().for_each(GpuLayer::montrer);
    Ok(Duration::ZERO)
}

/// Une image sur le processeur (calques créés au besoin).
fn frame_cpu(
    layers: &mut Vec<Layer>,
    monitors: &[(isize, IRect)],
    swarm: IRect,
    items: &[IRect],
    draw: &dyn Fn(&mut dyn Canvas, bool),
    now: Instant,
) -> Duration {
    layers.retain(|l| garder(monitors, l.mon, l.rect, l.empty_since, now));
    for &(mon, rect) in monitors.iter().filter(|m| touche(m.1, swarm, items)) {
        if !layers.iter().any(|l| l.mon == mon) {
            match Layer::new(mon, rect) {
                Ok(l) => layers.push(l),
                Err(e) => log::warn!("calque impossible pour l'écran {rect:?} : {e:#}"),
            }
        }
    }
    let mut sent = Duration::ZERO;
    for lay in layers.iter_mut() {
        let here = intersects(lay.rect, swarm);
        sent += lay.frame(zone_a_dessiner(lay.rect, swarm, items), |cv| draw(cv, here));
    }
    sent
}

impl Backend {
    pub fn new(want_gpu: bool) -> Self {
        let now = Instant::now();
        let mut b = Self { kind: Kind::Cpu(Vec::new()), want_gpu, retry_at: None, echecs: 1, gpu_depuis: None, reconstruit: None };
        if want_gpu {
            match Gpu::new() {
                Ok(g) => {
                    log::info!("dessin : carte graphique (DirectComposition + Direct2D)");
                    (b.kind, b.echecs, b.gpu_depuis) = (Kind::Gpu(g, Vec::new()), 0, Some(now));
                    return b;
                }
                Err(e) => {
                    let d = b.echec(now);
                    log::warn!("dessin : carte graphique indisponible ({e}), processeur utilisé ; nouvel essai dans {} s", d.as_secs());
                }
            }
        }
        log::info!("dessin : processeur (tiny-skia)");
        b
    }

    /// Note un échec de la carte graphique et planifie l'essai suivant ; renvoie l'attente.
    fn echec(&mut self, now: Instant) -> Duration {
        if self.gpu_depuis.take().is_some_and(|d| now - d >= STABLE) {
            self.echecs = 0;
        }
        let d = retry_delay(self.echecs);
        self.echecs = self.echecs.saturating_add(1);
        self.retry_at = self.want_gpu.then_some(now + d);
        d
    }

    pub fn is_gpu(&self) -> bool {
        matches!(self.kind, Kind::Gpu(..))
    }

    pub fn wants_gpu(&self) -> bool {
        self.want_gpu
    }

    /// Configuration des écrans changée : seuls les calques dont l'écran a disparu, bougé ou
    /// changé de taille sont détruits (refaits au besoin à l'image suivante) ; les autres restent
    /// en place, sans disparaître ni réapparaître sous la souris. Un écran inchangé qui reçoit un
    /// nouvel identifiant garde son calque. Renvoie le nombre de calques détruits.
    pub fn ecrans_changes(&mut self, monitors: &[(isize, IRect)]) -> usize {
        // les écrans ne se chevauchent pas : le rectangle suffit à reconnaître le sien
        let ecran = |rect: IRect| monitors.iter().find(|m| m.1 == rect).map(|m| m.0);
        match &mut self.kind {
            Kind::Gpu(gpu, layers) => {
                let n = layers.len();
                layers.retain_mut(|l| ecran(l.rect).map(|m| l.mon = m).is_some());
                let detruits = n - layers.len();
                if detruits > 0 {
                    let _ = gpu.commit();
                }
                detruits
            }
            Kind::Cpu(layers) => {
                let n = layers.len();
                layers.retain_mut(|l| ecran(l.rect).map(|m| l.mon = m).is_some());
                n - layers.len()
            }
        }
    }

    /// Détruit le calque de l'écran `mon` (pause plein écran prolongée : il ne reste pas
    /// au-dessus du jeu). Refait à la première image qui touche de nouveau cet écran. Vrai s'il y
    /// en avait un.
    pub fn liberer_ecran(&mut self, mon: isize) -> bool {
        match &mut self.kind {
            Kind::Gpu(gpu, layers) => {
                let n = layers.len();
                layers.retain(|l| l.mon != mon);
                let detruit = layers.len() < n;
                if detruit {
                    let _ = gpu.commit();
                }
                detruit
            }
            Kind::Cpu(layers) => {
                let n = layers.len();
                layers.retain(|l| l.mon != mon);
                layers.len() < n
            }
        }
    }

    /// Dessine une image. `monitors` = écrans (identifiant, rectangle) ; `swarm` = zone de
    /// l'essaim, dessiné sur chaque écran qu'il touche ; `items` = zones des fissures.
    /// `draw(canvas, essaim_ici)` dessine tout. Renvoie le temps de remise au compositeur.
    pub fn frame(&mut self, monitors: &[(isize, IRect)], swarm: IRect, items: &[IRect], draw: &dyn Fn(&mut dyn Canvas, bool)) -> Duration {
        let now = Instant::now();
        // Nouvel essai de la carte graphique, seulement sur une image qui a quelque chose à
        // montrer : il doit réussir une vraie image pour remplacer les calques processeur.
        let essai = matches!(self.kind, Kind::Cpu(_))
            && self.retry_at.is_some_and(|r| now >= r)
            && monitors.iter().any(|m| touche(m.1, swarm, items));
        if essai {
            match Gpu::new() {
                Ok(gpu) => {
                    let mut layers = Vec::new();
                    match frame_gpu(&gpu, &mut layers, monitors, swarm, items, draw, now) {
                        Ok(sent) => {
                            log::info!("dessin : de nouveau sur la carte graphique");
                            // les calques processeur (et leurs fenêtres) partent ici
                            self.kind = Kind::Gpu(gpu, layers);
                            (self.retry_at, self.gpu_depuis) = (None, Some(now));
                            return sent;
                        }
                        Err(e) => {
                            drop(layers);
                            let _ = gpu.commit();
                            let d = self.echec(now);
                            log::warn!("dessin : carte graphique toujours en échec ({e}) ; nouvel essai dans {} s", d.as_secs());
                        }
                    }
                }
                Err(e) => {
                    let d = self.echec(now);
                    log::warn!("dessin : carte graphique toujours indisponible ({e}) ; nouvel essai dans {} s", d.as_secs());
                }
            }
        }
        let resultat = match &mut self.kind {
            Kind::Gpu(gpu, layers) => frame_gpu(gpu, layers, monitors, swarm, items, draw, now),
            Kind::Cpu(layers) => Ok(frame_cpu(layers, monitors, swarm, items, draw, now)),
        };
        resultat.unwrap_or_else(|e| self.erreur_gpu(e, now, |gpu, layers| frame_gpu(gpu, layers, monitors, swarm, items, draw, now)))
    }

    /// Efface tout (pause, curseur masqué).
    pub fn clear(&mut self) {
        match &mut self.kind {
            Kind::Gpu(gpu, layers) => {
                if let Err(e) = clear_gpu(gpu, layers) {
                    self.erreur_gpu(e, Instant::now(), |g, l| clear_gpu(g, l));
                }
            }
            Kind::Cpu(layers) => layers.iter_mut().for_each(Layer::clear),
        }
    }

    /// Erreur de la carte graphique pendant une image ou un effacement. Périphérique perdu (et pas
    /// déjà refait il y a moins de `RECONSTRUCTION`) : refait, les mêmes fenêtres rattachées, puis
    /// `refaire` redonne l'image. Sinon, ou si cela échoue : bascule sur le processeur.
    fn erreur_gpu(&mut self, e: windows::core::Error, now: Instant, refaire: impl FnOnce(&Gpu, &mut Vec<GpuLayer>) -> windows::core::Result<Duration>) -> Duration {
        let Kind::Gpu(gpu, _) = &self.kind else { return Duration::ZERO };
        let perdu = crate::gpu::perte(&e) || gpu.perdu();
        if perdu && self.reconstruit.is_none_or(|t| now - t >= RECONSTRUCTION) {
            self.reconstruit = Some(now);
            match self.reconstruire(refaire) {
                Ok(sent) => {
                    log::info!("dessin : périphérique de la carte graphique perdu ({e}), refait sur les mêmes calques");
                    return sent;
                }
                Err(e2) => {
                    let d = self.echec(now);
                    log::warn!("dessin : périphérique de la carte graphique perdu ({e}) et impossible à refaire ({e2}), bascule sur le processeur ; nouvel essai dans {} s", d.as_secs());
                    return Duration::ZERO;
                }
            }
        }
        self.kind = Kind::Cpu(Vec::new());
        let d = self.echec(now);
        log::warn!("dessin : erreur de la carte graphique ({e}), bascule sur le processeur ; nouvel essai dans {} s", d.as_secs());
        Duration::ZERO
    }

    /// Nouveau périphérique, mêmes fenêtres : chacune lâche ses objets DirectComposition, l'ancien
    /// périphérique est libéré (une fenêtre n'a qu'une cible), puis chacune est rattachée au
    /// nouveau et `refaire` dessine. En cas d'échec, le moteur est laissé sur le processeur (les
    /// fenêtres des calques sont alors détruites).
    fn reconstruire(&mut self, refaire: impl FnOnce(&Gpu, &mut Vec<GpuLayer>) -> windows::core::Result<Duration>) -> windows::core::Result<Duration> {
        let Kind::Gpu(_, layers) = &mut self.kind else { return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL)) };
        let mut layers = std::mem::take(layers);
        layers.iter_mut().for_each(GpuLayer::detacher);
        // l'ancien périphérique part ici
        self.kind = Kind::Cpu(Vec::new());
        let gpu = Gpu::new()?;
        for l in layers.iter_mut() {
            gpu.rattacher(l)?;
        }
        let sent = refaire(&gpu, &mut layers)?;
        self.kind = Kind::Gpu(gpu, layers);
        Ok(sent)
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
            // réactions de base à la basse : les deux premières modulations par défaut
            (("modulations.basse", |c, v| c.modulations.liste[0].quantite = v), 0.0, 1.0),
            (("modulations.grave", |c, v| c.modulations.liste[1].quantite = v), 0.0, 1.0),
            (("orbite.trainee", |c, v| c.orbite.trainee = v), 0.05, 2.0),
            (("orbite.cometes", |c, v| c.orbite.cometes = v > 0.5), 0.0, 1.0),
            (("essaim.maximum", |c, v| c.essaim.maximum = v as usize), 0.0, 64.0),
            (("essaim.fixes", |c, v| c.essaim.fixes = v as u32), 0.0, 32.0),
            (("declencheurs.satellites", |c, v| {
                for d in c.declencheurs.liste.iter_mut().filter(|d| d.faire == crate::config::Action::Satellites) {
                    (d.nombre, d.nombre_max) = (v as u32, v as u32);
                }
            }), 0.0, 10.0),
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
    fn essais_de_la_carte_graphique_de_plus_en_plus_espaces() {
        let attentes: Vec<u64> = (0..7).map(|n| retry_delay(n).as_secs()).collect();
        assert_eq!(attentes, [10, 30, 90, 270, 600, 600, 600]);
        assert_eq!(retry_delay(u32::MAX), RETRY_MAX);
        // échecs d'affilée : attente croissante ; après un long moment sans erreur, on repart de 10 s
        let t0 = Instant::now();
        let mut b = Backend { kind: Kind::Cpu(Vec::new()), want_gpu: true, retry_at: None, echecs: 0, gpu_depuis: None, reconstruit: None };
        let suite: Vec<u64> = (0..3).map(|_| b.echec(t0).as_secs()).collect();
        assert_eq!(suite, [10, 30, 90]);
        assert_eq!(b.retry_at, Some(t0 + Duration::from_secs(90)));
        b.gpu_depuis = Some(t0);
        assert_eq!(b.echec(t0 + Duration::from_secs(60)).as_secs(), 270, "une minute sans erreur ne suffit pas");
        b.gpu_depuis = Some(t0);
        assert_eq!(b.echec(t0 + STABLE).as_secs(), 10);
        // carte graphique non voulue : aucun essai
        b.want_gpu = false;
        b.echec(t0);
        assert_eq!(b.retry_at, None);
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

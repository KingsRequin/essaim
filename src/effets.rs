//! Effets autour du pointeur, en plus de l'essaim : projecteur « retrouver la souris »
//! (`[projecteur]`), réticule (`[reticule]`) et anneau de spectre (`[spectre]`). Simulation et
//! dessin ; `main.rs` ajoute leurs zones au rectangle modifié : le réticule et le spectre à celui
//! de l'essaim, le projecteur (qui assombrit tout l'écran du pointeur) aux zones à part, et
//! seulement pendant l'effet.
//!
//! `ui/apercu.js` reproduit le réticule et le spectre (mêmes formules) : les modifier ici oblige
//! à les reporter là.

use crate::audio::{AudioShared, BANDES_MAX};
use crate::canvas::Canvas;
use crate::config::Config;
use crate::layer::IRect;
use crate::figures::Figures;
use crate::render::{prisme, Ecoute};
use std::f32::consts::{PI, TAU};

// --- Secousse de la souris ----------------------------------------------------------------------

/// Inversions de sens horizontales franches qu'il faut, et fenêtre de temps où elles doivent
/// tenir (de la première à la dernière), à la sensibilité 5.
const SECOUSSE_INVERSIONS: usize = 3;
const SECOUSSE_FENETRE: f64 = 0.6;
/// Course horizontale la plus courte d'un aller (px à 100 %) et durée la plus longue, à la
/// sensibilité 5 ; rapport minimal entre course horizontale et étendue verticale (un cercle
/// rapide n'est pas une secousse).
const SECOUSSE_COURSE: f32 = 60.0;
const SECOUSSE_DUREE_MAX: f64 = 0.35;
const SECOUSSE_RAPPORT: f32 = 1.5;
/// Recul qui confirme un changement de sens (px à 100 %) : les tremblements en dessous ne
/// comptent pas.
const SECOUSSE_RECUL: f32 = 10.0;

/// Exigence de la secousse à la sensibilité `s` (`[projecteur] sensibilite`, 1 à 10) : course
/// minimale d'un aller (px à 100 %), durée maximale d'un aller et fenêtre des inversions (s).
/// La course double tous les 5 crans en moins (104 px à 1, 60 à 5, 30 à 10) ; les durées
/// s'allongent comme sa racine quand elle raccourcit (un geste plus petit peut être plus lent).
pub fn secousse_exigence(s: f32) -> (f32, f64, f64) {
    let k = 2f32.powf((5.0 - s.clamp(1.0, 10.0)) / 5.0);
    let lent = f64::from(k.sqrt().recip());
    (SECOUSSE_COURSE * k, SECOUSSE_DUREE_MAX * lent, SECOUSSE_FENETRE * lent)
}

/// Reconnaît une secousse de la souris (va-et-vient horizontal rapide) à partir des positions lues
/// à chaque image. Un aller va d'un changement de sens au suivant ; il est franc s'il est assez
/// long, assez rapide et plutôt horizontal. Trois changements de sens francs d'affilée en moins de
/// 0,6 s font une secousse (seuils de la sensibilité 5 ; `secousse_exigence`).
#[derive(Default)]
pub struct Secousse {
    /// Sens de l'aller en cours : 0 inconnu, 1 vers la droite, -1 vers la gauche.
    sens: i8,
    /// Début de l'aller (instant, x) ; `None` avant la première position.
    depart: Option<(f64, f32)>,
    /// Point le plus avancé de l'aller (x, instant).
    bout: (f32, f64),
    /// Étendue verticale de l'aller.
    y: (f32, f32),
    /// Instants des changements de sens francs récents.
    inversions: Vec<f64>,
}

impl Secousse {
    /// Nouvelle position du pointeur ; vrai si une secousse vient d'être reconnue, à la
    /// sensibilité donnée (1 à 10).
    pub fn pousser(&mut self, t: f64, x: f32, y: f32, scale: f32, sensibilite: f32) -> bool {
        let (course_min, duree_max, fenetre) = secousse_exigence(sensibilite);
        let recul = SECOUSSE_RECUL * scale;
        let Some((t0, x0)) = self.depart else {
            self.recommencer(t, x, y);
            return false;
        };
        self.y = (self.y.0.min(y), self.y.1.max(y));
        if self.sens == 0 {
            if (x - x0).abs() > recul {
                // le mouvement commence : l'aller part d'ici (le temps passé immobile ne compte pas)
                self.sens = if x > x0 { 1 } else { -1 };
                self.depart = Some((t.max(t0), x0));
                self.bout = (x, t);
            }
            return false;
        }
        let s = f32::from(self.sens);
        if (x - self.bout.0) * s >= 0.0 {
            self.bout = (x, t);
            return false;
        }
        if (self.bout.0 - x) * s <= recul {
            return false;
        }
        // changement de sens confirmé au bout de l'aller
        let course = (self.bout.0 - x0).abs();
        let duree = self.bout.1 - t0;
        let haut = self.y.1 - self.y.0;
        let franc = course >= course_min * scale && duree <= duree_max && course >= SECOUSSE_RAPPORT * haut;
        let (tb, xb) = (self.bout.1, self.bout.0);
        if franc {
            self.inversions.push(tb);
            self.inversions.retain(|&ti| tb - ti <= fenetre);
        } else {
            self.inversions.clear();
        }
        self.sens = -self.sens;
        self.depart = Some((tb, xb));
        self.bout = (x, t);
        self.y = (y, y);
        if self.inversions.len() >= SECOUSSE_INVERSIONS {
            self.inversions.clear();
            return true;
        }
        false
    }

    fn recommencer(&mut self, t: f64, x: f32, y: f32) {
        *self = Self { depart: Some((t, x)), bout: (x, t), y: (y, y), ..Self::default() };
    }

    /// Oublie le mouvement en cours (projecteur lancé, pointeur caché).
    pub fn oublier(&mut self) {
        *self = Self::default();
    }
}

// --- Projecteur ---------------------------------------------------------------------------------

/// Ce dont le disque clair se resserre pendant l'effet (px à 100 %, en plus de son rayon final
/// `[projecteur] taille`), part du rayon sur laquelle son bord s'adoucit, gonflement du disque
/// sur un kick.
const PROJ_RESSERRE: f32 = 360.0;
const PROJ_FLOU: f32 = 0.6;
const PROJ_KICK: f32 = 0.18;
/// Teinte du voile : presque noir, à peine bleuté.
const PROJ_VOILE: (u8, u8, u8) = (4, 6, 14);

/// Rayon du disque clair (px écran) et présence du voile (0 à 1, à multiplier par son opacité) à
/// la fraction `p` (0 à 1) de l'effet : le disque se resserre vite puis ralentit, jusqu'à
/// `taille` (px à 100 %) ; le voile monte en 12 % de l'effet et s'efface sur les 30 derniers ;
/// `env` (éclat du kick, 1 → 0) gonfle le disque.
pub fn projecteur_forme(p: f32, env: f32, scale: f32, taille: f32) -> (f32, f32) {
    let p = p.clamp(0.0, 1.0);
    let r = (taille + PROJ_RESSERRE * (1.0 - p).powi(3)) * scale * (1.0 + PROJ_KICK * env.clamp(0.0, 1.0));
    let presence = (p / 0.12).min(1.0) * ((1.0 - p) / 0.3).min(1.0);
    (r, presence.max(0.0))
}

// --- Réticule et spectre ------------------------------------------------------------------------

/// Ce que la lueur du réticule au kick ajoute à l'épaisseur de ses lignes (px à 100 %).
const RETICULE_LUEUR: f32 = 2.8;
/// Longueur des barres sans énergie (px à 100 %).
const SPECTRE_BARRE_MIN: f32 = 1.5;
/// Lissage de l'affichage des bandes (par seconde) : montée vive, retombée douce.
const SPECTRE_MONTEE: f32 = 30.0;
const SPECTRE_DESCENTE: f32 = 8.0;
/// Anneau masqué sans musique (`masquer_sans_musique`) : niveau absolu de la bande la plus forte
/// sous lequel rien ne joue (≈ −60 dBFS), silence attendu avant l'effacement (s), durée du fondu
/// à l'effacement et au retour (s).
const SPECTRE_AUDIBLE: f32 = 0.001;
const SPECTRE_ATTENTE: f32 = 1.5;
const SPECTRE_EFFACEMENT: f32 = 0.8;
const SPECTRE_RETOUR: f32 = 0.15;

/// Épaisseur des barres du spectre (px écran) : la moitié de l'espace entre deux barres, bornée.
pub fn spectre_largeur(n: usize, rayon: f32, scale: f32) -> f32 {
    (TAU * rayon / n.max(1) as f32 * 0.5).clamp(1.5 * scale, 6.0 * scale)
}

pub struct Effets {
    secousse: Secousse,
    /// Raccourci Ctrl+Alt+F reçu, pris en compte à l'image suivante.
    demande: bool,
    /// Début du projecteur en cours.
    projecteur: Option<f64>,
    /// Projecteur de cette image : écran assombri, centre, rayon du disque clair, présence du
    /// voile (0 à 1).
    voile: Option<(IRect, (f32, f32), f32, f32)>,
    /// Bandes lues dans l'analyse du son, et affichées (lissées).
    lues: [f32; BANDES_MAX],
    bandes: [f32; BANDES_MAX],
    /// Silence entendu depuis (s), et visibilité de l'anneau (0 à 1).
    silence: f32,
    visible: f32,
    /// Éclat du kick (celui de l'essaim), 1 → 0.
    env: f32,
    /// Petites figures autour du pointeur (`figures.rs`), et leur écoute des coups du son.
    figures: Figures,
    ecoute: Ecoute,
}

impl Effets {
    pub fn new() -> Self {
        Self { secousse: Secousse::default(), demande: false, projecteur: None, voile: None, lues: [0.0; BANDES_MAX], bandes: [0.0; BANDES_MAX], silence: 0.0, visible: 1.0, env: 0.0, figures: Figures::new(), ecoute: Ecoute::default() }
    }

    /// Raccourci du projecteur (Ctrl+Alt+F).
    pub fn demander_projecteur(&mut self) {
        self.demande = true;
    }

    /// Avance d'une image : secousse et projecteur, bandes du spectre (calculées par l'analyse du
    /// son seulement si l'effet est voulu). `env` = éclat du kick de l'essaim.
    #[allow(clippy::too_many_arguments)]
    pub fn avance(&mut self, t: f64, dt: f32, cursor: (f32, f32), scale: f32, env: f32, shared: &AudioShared, cfg: &Config, monitors: &[(isize, IRect)]) {
        self.env = env;
        // projecteur
        let p = &cfg.projecteur;
        let demande = std::mem::take(&mut self.demande);
        if !p.actif {
            self.projecteur = None;
            self.secousse.oublier();
        } else {
            let en_cours = self.projecteur.is_some_and(|d| t - d < f64::from(p.duree));
            if !p.secousse {
                self.secousse.oublier();
            }
            let secoue = p.secousse && !en_cours && self.secousse.pousser(t, cursor.0, cursor.1, scale, p.sensibilite);
            if demande || secoue {
                self.projecteur = Some(t);
                self.secousse.oublier();
            }
        }
        self.voile = self.projecteur.and_then(|d| {
            let f = ((t - d) / f64::from(p.duree.max(0.1))) as f32;
            if !(0.0..1.0).contains(&f) {
                return None;
            }
            let (r, a) = projecteur_forme(f, env, scale, p.taille);
            let (cx, cy) = (cursor.0 as i32, cursor.1 as i32);
            let ecran = monitors.iter().map(|m| m.1).find(|r| cx >= r.0 && cx < r.2 && cy >= r.1 && cy < r.3).or(monitors.first().map(|m| m.1))?;
            Some((ecran, cursor, r, a))
        });
        if self.voile.is_none() {
            self.projecteur = None;
        }
        // bandes du spectre : demandées à l'analyse du son seulement si l'effet est voulu
        let s = &cfg.spectre;
        let voulues = if s.actif && cfg.audio.actif { s.bandes.clamp(8, BANDES_MAX as u32) } else { 0 };
        shared.set_bandes(voulues);
        if voulues > 0 {
            let n = shared.bandes(&mut self.lues);
            lisser_bandes(&mut self.bandes[..n], &self.lues[..n], dt);
            self.silence = if shared.bandes_crete() > SPECTRE_AUDIBLE { 0.0 } else { self.silence + dt };
        } else {
            self.bandes = [0.0; BANDES_MAX];
        }
        self.visible = visibilite(self.visible, s.masquer_sans_musique && self.silence >= SPECTRE_ATTENTE, dt);
        // petites figures : elles écoutent les coups du son elles-mêmes
        let au = shared.view();
        let coups = self.ecoute.lire(&au);
        self.figures.avance(t, dt, cursor, scale, &au, &coups, cfg);
    }

    /// Projecteur en cours (écran assombri à redessiner en entier à chaque image).
    pub fn rect_ecran(&self) -> Option<IRect> {
        self.voile.map(|v| v.0)
    }

    /// Zone du réticule, du spectre et des petites figures autour du pointeur, à ajouter à celle
    /// de l'essaim.
    pub fn rect_pointeur(&self, cursor: (f32, f32), scale: f32, cfg: &Config) -> Option<IRect> {
        let mut e = 0.0f32;
        if cfg.reticule.actif {
            let re = &cfg.reticule;
            e = e.max((re.vide + re.taille + re.epaisseur + RETICULE_LUEUR) * scale);
        }
        if cfg.spectre.actif {
            let n = cfg.spectre.bandes.clamp(8, BANDES_MAX as u32) as usize;
            let r = cfg.spectre.rayon * scale;
            e = e.max(r + (SPECTRE_BARRE_MIN + cfg.spectre.hauteur) * scale + spectre_largeur(n, r, scale));
        }
        let zone = (e > 0.0).then(|| {
            let e = e + 2.0;
            ((cursor.0 - e).floor() as i32, (cursor.1 - e).floor() as i32, (cursor.0 + e).ceil() as i32, (cursor.1 + e).ceil() as i32)
        });
        crate::layer::union(zone, self.figures.rect(cursor, scale, cfg))
    }

    /// Voile du projecteur, sous tout le reste (fissures et essaim restent visibles par-dessus).
    pub fn dessiner_voile(&self, cv: &mut dyn Canvas, cfg: &Config, t: f64, scale: f32) {
        let Some((ecran, (x, y), r, presence)) = self.voile else { return };
        let zone = (ecran.0 as f32, ecran.1 as f32, ecran.2 as f32, ecran.3 as f32);
        cv.voile(zone, x, y, r, r * PROJ_FLOU, PROJ_VOILE, cfg.projecteur.opacite * presence);
        // liseré à la teinte de l'essaim sur le bord du disque clair
        cv.ring(x, y, r, 2.0 * scale, prisme(cfg, t, 1.0), 0.5 * presence);
    }

    /// Réticule, anneau de spectre et petites figures autour de `cursor`, sous l'essaim.
    pub fn dessiner(&self, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32, cursor: (f32, f32)) {
        self.figures.dessiner(cv, t, cfg, scale);
        let (cx, cy) = cursor;
        let s = &cfg.spectre;
        if s.actif && self.visible > 0.0 {
            let n = s.bandes.clamp(8, BANDES_MAX as u32) as usize;
            let r = s.rayon * scale;
            let w = spectre_largeur(n, r, scale);
            let vis = self.visible;
            for (i, &v) in self.bandes[..n].iter().enumerate() {
                // la bande la plus grave en haut, puis dans le sens des aiguilles d'une montre
                let (sin, cos) = (-PI / 2.0 + i as f32 / n as f32 * TAU).sin_cos();
                let long = (SPECTRE_BARRE_MIN + s.hauteur * v) * scale;
                cv.line((cx + cos * r, cy + sin * r), (cx + cos * (r + long), cy + sin * (r + long)), w, prisme(cfg, t, v), (0.35 + 0.6 * v) * vis, true);
            }
        }
        let re = &cfg.reticule;
        if re.actif {
            let rgb = prisme(cfg, t, 1.0);
            let (vide, long) = (re.vide * scale, (re.vide + re.taille) * scale);
            let env = self.env.clamp(0.0, 1.0);
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let (a, b) = ((cx + dx * vide, cy + dy * vide), (cx + dx * long, cy + dy * long));
                if env > 0.02 {
                    cv.line(a, b, (re.epaisseur + RETICULE_LUEUR) * scale, rgb, 0.3 * env * re.opacite, true);
                }
                cv.line(a, b, re.epaisseur * scale, rgb, re.opacite * (0.6 + 0.4 * env), true);
            }
        }
    }
}

/// Visibilité de l'anneau de spectre après `dt` : vers 0 si `masque` (fondu lent), vers 1
/// sinon (retour rapide).
fn visibilite(v: f32, masque: bool, dt: f32) -> f32 {
    if masque {
        (v - dt / SPECTRE_EFFACEMENT).max(0.0)
    } else {
        (v + dt / SPECTRE_RETOUR).min(1.0)
    }
}

/// Lissage des bandes affichées vers celles lues : montée vive, retombée douce.
pub fn lisser_bandes(affichees: &mut [f32], lues: &[f32], dt: f32) {
    let (km, kd) = ((dt * SPECTRE_MONTEE).min(1.0), (dt * SPECTRE_DESCENTE).min(1.0));
    for (a, &l) in affichees.iter_mut().zip(lues) {
        *a += (l - *a) * if l > *a { km } else { kd };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trajectoire échantillonnée à `ips` images/s : `pos(t)` pendant `duree` s ; nombre de
    /// secousses reconnues à la sensibilité par défaut.
    fn secousses(ips: f64, duree: f64, scale: f32, pos: impl Fn(f64) -> (f32, f32)) -> usize {
        secousses_a(Config::default().projecteur.sensibilite, ips, duree, scale, pos)
    }

    /// Comme `secousses`, à la sensibilité donnée.
    fn secousses_a(sensibilite: f32, ips: f64, duree: f64, scale: f32, pos: impl Fn(f64) -> (f32, f32)) -> usize {
        let mut s = Secousse::default();
        let n = (duree * ips) as usize;
        (0..=n)
            .filter(|&i| {
                let t = i as f64 / ips;
                let (x, y) = pos(t);
                s.pousser(t, x, y, scale, sensibilite)
            })
            .count()
    }

    /// Va-et-vient horizontal de `amplitude` px (crête à crête) à `hz` allers-retours par seconde.
    fn va_et_vient(amplitude: f32, hz: f64) -> impl Fn(f64) -> (f32, f32) {
        move |t| (800.0 + amplitude / 2.0 * (TAU as f64 * hz * t).sin() as f32, 500.0)
    }

    #[test]
    fn secousse_franche_reconnue() {
        for ips in [30.0, 60.0, 144.0] {
            // 250 px, 3 allers-retours par seconde, pendant 0,8 s
            assert!(secousses(ips, 0.8, 1.0, va_et_vient(250.0, 3.0)) >= 1, "{ips} i/s");
            // un peu de vertical dans le geste : toujours une secousse
            assert!(secousses(ips, 0.8, 1.0, |t| (va_et_vient(200.0, 3.5)(t).0, 500.0 + 25.0 * (t * 9.0).sin() as f32)) >= 1, "{ips} i/s");
            // écran à 150 % : mêmes gestes en pixels physiques plus grands
            assert!(secousses(ips, 0.8, 1.5, va_et_vient(375.0, 3.0)) >= 1, "{ips} i/s, 150 %");
        }
    }

    #[test]
    fn mouvement_rapide_en_ligne_droite_ignore() {
        for ips in [30.0, 144.0] {
            // 3000 px/s en ligne droite, horizontale puis en diagonale
            assert_eq!(secousses(ips, 0.6, 1.0, |t| (100.0 + 3000.0 * t as f32, 500.0)), 0);
            assert_eq!(secousses(ips, 0.6, 1.0, |t| (100.0 + 2000.0 * t as f32, 100.0 + 1500.0 * t as f32)), 0);
            // un aller-retour seul (geste qui revient en arrière) : pas une secousse
            assert_eq!(secousses(ips, 1.0, 1.0, |t| (800.0 + 600.0 * (1.0 - (2.0 * t as f32 - 1.0).abs()), 500.0)), 0);
        }
    }

    #[test]
    fn petits_tremblements_et_gestes_lents_ignores() {
        for ips in [60.0, 144.0] {
            // main qui tremble : ±4 px très vite, puis ±20 px (course trop courte)
            assert_eq!(secousses(ips, 3.0, 1.0, va_et_vient(8.0, 8.0)), 0);
            assert_eq!(secousses(ips, 3.0, 1.0, va_et_vient(40.0, 5.0)), 0);
            // va-et-vient ample mais lent (une seconde par aller-retour)
            assert_eq!(secousses(ips, 4.0, 1.0, va_et_vient(400.0, 1.0)), 0);
            // cercles rapides : autant de vertical que d'horizontal
            assert_eq!(secousses(ips, 2.0, 1.0, |t| (800.0 + 120.0 * (TAU as f64 * 3.0 * t).cos() as f32, 500.0 + 120.0 * (TAU as f64 * 3.0 * t).sin() as f32)), 0);
        }
    }

    #[test]
    fn sensibilite_de_la_secousse() {
        // sensibilité 5 : les seuils de toujours
        let (course, duree, fenetre) = secousse_exigence(5.0);
        assert!((course - SECOUSSE_COURSE).abs() < 1e-4 && (duree - SECOUSSE_DUREE_MAX).abs() < 1e-9 && (fenetre - SECOUSSE_FENETRE).abs() < 1e-9);
        // plus sensible : course plus courte, gestes plus lents acceptés ; hors bornes : ramenée
        let (c1, d1, f1) = secousse_exigence(1.0);
        let (c10, d10, f10) = secousse_exigence(10.0);
        assert!(c1 > course && course > c10 && d1 < duree && duree < d10 && f1 < fenetre && fenetre < f10);
        assert_eq!(secousse_exigence(99.0), secousse_exigence(10.0));
        assert_eq!(secousse_exigence(-3.0), secousse_exigence(1.0));
        for ips in [30.0, 60.0, 144.0] {
            // petit geste vif (45 px) : seulement à forte sensibilité
            assert_eq!(secousses_a(5.0, ips, 1.5, 1.0, va_et_vient(45.0, 4.0)), 0, "{ips} i/s");
            assert!(secousses_a(10.0, ips, 1.5, 1.0, va_et_vient(45.0, 4.0)) >= 1, "{ips} i/s");
            // geste ample un peu lent (0,38 s par aller) : seulement à forte sensibilité
            assert_eq!(secousses_a(5.0, ips, 2.0, 1.0, va_et_vient(300.0, 1.3)), 0, "{ips} i/s");
            assert!(secousses_a(10.0, ips, 2.0, 1.0, va_et_vient(300.0, 1.3)) >= 1, "{ips} i/s");
            // geste moyen (90 px) : reconnu par défaut, plus à faible sensibilité
            assert!(secousses_a(5.0, ips, 1.5, 1.0, va_et_vient(90.0, 3.0)) >= 1, "{ips} i/s");
            assert_eq!(secousses_a(1.0, ips, 1.5, 1.0, va_et_vient(90.0, 3.0)), 0, "{ips} i/s");
            // secousse franche : reconnue à toutes les sensibilités
            for s in [1.0, 5.0, 10.0] {
                assert!(secousses_a(s, ips, 0.8, 1.0, va_et_vient(250.0, 3.0)) >= 1, "{ips} i/s, sensibilité {s}");
            }
            // même à la plus forte : ni la main qui tremble, ni la ligne droite, ni les cercles
            assert_eq!(secousses_a(10.0, ips, 3.0, 1.0, va_et_vient(8.0, 8.0)), 0, "{ips} i/s");
            assert_eq!(secousses_a(10.0, ips, 0.6, 1.0, |t| (100.0 + 3000.0 * t as f32, 500.0)), 0, "{ips} i/s");
            assert_eq!(secousses_a(10.0, ips, 2.0, 1.0, |t| (800.0 + 120.0 * (TAU as f64 * 3.0 * t).cos() as f32, 500.0 + 120.0 * (TAU as f64 * 3.0 * t).sin() as f32)), 0, "{ips} i/s");
        }
    }

    #[test]
    fn projecteur_se_resserre_puis_s_efface() {
        let taille = Config::default().projecteur.taille;
        let (r0, a0) = projecteur_forme(0.0, 0.0, 1.0, taille);
        let (r1, a1) = projecteur_forme(0.5, 0.0, 1.0, taille);
        let (r2, a2) = projecteur_forme(1.0, 0.0, 1.0, taille);
        assert!(r0 > r1 && r1 > r2 && (r2 - taille).abs() < 1e-3, "{r0} {r1} {r2}");
        assert!(a0 == 0.0 && a1 == 1.0 && a2 == 0.0, "{a0} {a1} {a2}");
        // un kick gonfle le disque
        assert!(projecteur_forme(0.5, 1.0, 1.0, taille).0 > r1);
        // disque plus grand : il se resserre d'autant, jusqu'à sa taille
        let (g0, _) = projecteur_forme(0.0, 0.0, 1.0, 200.0);
        let (g2, _) = projecteur_forme(1.0, 0.0, 1.0, 200.0);
        assert!((g0 - r0 - (200.0 - taille)).abs() < 1e-3 && (g2 - 200.0).abs() < 1e-3, "{g0} {g2}");
    }

    fn ecran() -> Vec<(isize, IRect)> {
        vec![(1, (0, 0, 1920, 1080)), (2, (1920, 0, 3840, 1080))]
    }

    #[test]
    fn projecteur_le_temps_de_l_effet_seulement() {
        let mut cfg = Config::default();
        cfg.projecteur.duree = 1.0;
        let shared = AudioShared::default();
        let mut e = Effets::new();
        let dt = 1.0 / 60.0;
        e.avance(0.0, dt, (2500.0, 400.0), 1.0, 0.0, &shared, &cfg, &ecran());
        assert_eq!(e.rect_ecran(), None);
        // raccourci : l'écran du pointeur (le second) est assombri pendant la durée de l'effet
        e.demander_projecteur();
        let mut vus = 0;
        for i in 1..=90 {
            e.avance(i as f64 * dt as f64, dt, (2500.0, 400.0), 1.0, 0.0, &shared, &cfg, &ecran());
            if let Some(r) = e.rect_ecran() {
                assert_eq!(r, (1920, 0, 3840, 1080));
                vus += 1;
            }
        }
        assert!((55..=61).contains(&vus), "{vus} images");
        assert_eq!(e.rect_ecran(), None);
        // coupé : ni raccourci ni secousse
        cfg.projecteur.actif = false;
        e.demander_projecteur();
        e.avance(2.0, dt, (500.0, 400.0), 1.0, 0.0, &shared, &cfg, &ecran());
        assert_eq!(e.rect_ecran(), None);
        // secousse : le projecteur part
        cfg.projecteur.actif = true;
        let geste = va_et_vient(250.0, 3.0);
        let lance = (0..60).any(|i| {
            let t = 3.0 + i as f64 * dt as f64;
            e.avance(t, dt, geste(t), 1.0, 0.0, &shared, &cfg, &ecran());
            e.rect_ecran().is_some()
        });
        assert!(lance);
        // secousse coupée : le même geste ne lance rien, le raccourci toujours
        cfg.projecteur.secousse = false;
        for i in 0..240 {
            let t = 10.0 + i as f64 * dt as f64;
            e.avance(t, dt, geste(t), 1.0, 0.0, &shared, &cfg, &ecran());
            assert_eq!(e.rect_ecran(), None, "image {i}");
        }
        e.demander_projecteur();
        e.avance(14.0, dt, (500.0, 400.0), 1.0, 0.0, &shared, &cfg, &ecran());
        e.avance(14.0 + f64::from(dt) * 12.0, dt, (500.0, 400.0), 1.0, 0.0, &shared, &cfg, &ecran());
        assert!(e.rect_ecran().is_some());
    }

    #[test]
    fn voile_a_l_opacite_reglee() {
        // au milieu de l'effet, le coin de l'écran a l'opacité du réglage
        let coin = |opacite: f32| {
            let mut cfg = Config::default();
            (cfg.projecteur.duree, cfg.projecteur.opacite) = (1.0, opacite);
            let shared = AudioShared::default();
            let mons = [(1isize, (0, 0, 400, 300))];
            let mut e = Effets::new();
            e.demander_projecteur();
            e.avance(0.0, 1.0 / 60.0, (200.0, 150.0), 0.2, 0.0, &shared, &cfg, &mons);
            e.avance(0.5, 1.0 / 60.0, (200.0, 150.0), 0.2, 0.0, &shared, &cfg, &mons);
            let mut pm = tiny_skia::Pixmap::new(400, 300).unwrap();
            e.dessiner_voile(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, &cfg, 0.5, 0.2);
            (pm.pixel(2, 2).unwrap().alpha(), pm.pixel(200, 150).unwrap().alpha())
        };
        let (sombre, centre) = coin(0.9);
        assert!((227..=232).contains(&sombre) && centre == 0, "{sombre} {centre}");
        let (clair, centre) = coin(0.3);
        assert!((75..=79).contains(&clair) && centre == 0, "{clair} {centre}");
    }

    #[test]
    fn reticule_et_spectre_tiennent_dans_leur_zone() {
        // dessin dans une grande image : aucun pixel peint hors de `rect_pointeur`
        let mut cfg = Config::default();
        (cfg.reticule.actif, cfg.reticule.taille, cfg.spectre.actif, cfg.spectre.bandes) = (true, 200.0, true, 8);
        (cfg.reticule.epaisseur, cfg.reticule.vide) = (6.0, 60.0);
        (cfg.spectre.rayon, cfg.spectre.hauteur) = (150.0, 100.0);
        let mut e = Effets::new();
        e.bandes = [1.0; BANDES_MAX];
        e.env = 1.0;
        let cursor = (600.0, 600.0);
        for scale in [1.0f32, 1.5] {
            let (x0, y0, x1, y1) = e.rect_pointeur(cursor, scale, &cfg).unwrap();
            let mut pm = tiny_skia::Pixmap::new(1200, 1200).unwrap();
            e.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, 0.0, &cfg, scale, cursor);
            let mut peints = 0;
            for y in 0..1200 {
                for x in 0..1200 {
                    if pm.pixel(x, y).is_some_and(|p| p.alpha() > 0) {
                        peints += 1;
                        let (x, y) = (x as i32, y as i32);
                        assert!(x >= x0 && x < x1 && y >= y0 && y < y1, "pixel ({x}, {y}) hors de {x0},{y0} → {x1},{y1}");
                    }
                }
            }
            assert!(peints > 500, "{peints} pixels");
            // le centre reste vide : le pointeur n'est jamais couvert
            let c = pm.pixel(600, 600).unwrap();
            assert_eq!(c.alpha(), 0);
        }
        // effets coupés : aucune zone
        assert_eq!(Effets::new().rect_pointeur(cursor, 1.0, &Config::default()), None);
    }

    #[test]
    fn voile_assombrit_autour_du_disque_clair() {
        let mut pm = tiny_skia::Pixmap::new(400, 300).unwrap();
        let mut cv = crate::canvas::PixCanvas { pm: &mut pm, origin: (100.0, 50.0) };
        cv.voile((100.0, 50.0, 500.0, 350.0), 300.0, 200.0, 40.0, 24.0, (0, 0, 0), 0.6);
        let a = |x: u32, y: u32| pm.pixel(x, y).unwrap().alpha();
        assert_eq!(a(200, 150), 0, "centre du disque");
        assert!((150..=155).contains(&a(5, 5)), "coin : {}", a(5, 5));
        assert!((150..=155).contains(&a(200, 150 - 80)), "au-delà du bord adouci : {}", a(200, 70));
        let bord = a(200 + 52, 150);
        assert!(bord > 0 && bord < 150, "bord adouci : {bord}");
    }

    #[test]
    fn spectre_lisse_monte_vite_retombe_doucement() {
        let mut a = [0.0f32; 4];
        lisser_bandes(&mut a, &[1.0; 4], 1.0 / 60.0);
        assert!(a[0] > 0.45, "{a:?}");
        let mut b = [1.0f32; 4];
        lisser_bandes(&mut b, &[0.0; 4], 1.0 / 60.0);
        assert!(b[0] > 0.8, "{b:?}");
    }

    /// Coût d'une image (carte graphique puis processeur, simulation comprise) avec chaque effet,
    /// et coût du banc de filtres du spectre :
    /// `cargo test --release mesure_effets -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn mesure_effets() {
        use crate::audio::{AudioView, Bandes};
        use crate::backend::Backend;
        use crate::render::Swarm;
        use std::time::Instant;
        fn cout(cfg: &Config, gpu: bool, projecteur: bool) -> f64 {
            let dt = 1.0 / 144.0;
            let mut s = Swarm::new();
            let mut e = Effets::new();
            let shared = AudioShared::default();
            let mut au = AudioView { kicks: 0, bass: 0.8, pitch: 40.0, ..Default::default() };
            let mut b = Backend::new(gpu);
            let mons = [(1isize, (0, 0, 1920, 1080))];
            let (chauffe, n) = (150, 300);
            let mut durees = Vec::with_capacity(n);
            for i in 0..(chauffe + n) {
                if i % 30 == 0 {
                    au.kicks += 1;
                }
                let t = i as f64 * dt as f64;
                let cursor = (600.0 + 200.0 * (t as f32).sin(), 500.0);
                let t0 = Instant::now();
                if projecteur && e.rect_ecran().is_none() {
                    e.demander_projecteur();
                }
                s.update(t, dt, cursor, au, cfg, 1.0);
                e.avance(t, dt, cursor, 1.0, s.env, &shared, cfg, &mons);
                let (x0, y0, x1, y1) = s.bounds(cursor, 1.0, cfg.eclat.taille);
                let rect = (x0 as i32, y0 as i32, x1 as i32 + 1, y1 as i32 + 1);
                let rect = crate::layer::union(Some(rect), e.rect_pointeur(cursor, 1.0, cfg)).unwrap();
                let items: Vec<IRect> = e.rect_ecran().into_iter().collect();
                b.frame(&mons, rect, &items, &|cv, _| {
                    e.dessiner_voile(cv, cfg, t, 1.0);
                    e.dessiner(cv, t, cfg, 1.0, cursor);
                    s.draw(cv, t, cfg, 1.0, cursor);
                });
                if i >= chauffe {
                    durees.push(t0.elapsed().as_secs_f64() * 1000.0);
                }
            }
            durees.sort_by(|a, b| a.total_cmp(b));
            durees[durees.len() / 2]
        }
        let base = Config::default();
        let mut reticule = base.clone();
        reticule.reticule.actif = true;
        let mut spectre = base.clone();
        (spectre.spectre.actif, spectre.spectre.bandes) = (true, 32);
        for gpu in [true, false] {
            let moteur = if gpu { "carte graphique" } else { "processeur" };
            let b = cout(&base, gpu, false);
            eprintln!("{moteur} : sans effet {b:.3} ms");
            eprintln!("{moteur} : réticule {:+.3} ms", cout(&reticule, gpu, false) - b);
            eprintln!("{moteur} : spectre (32 bandes) {:+.3} ms", cout(&spectre, gpu, false) - b);
            eprintln!("{moteur} : projecteur (pendant l'effet, 1920 × 1080) {:+.3} ms", cout(&base, gpu, true) - b);
        }
        // banc de filtres : une seconde de son à 48 kHz
        let son: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.07).sin() * 0.3).collect();
        for n in [8usize, 16, 32] {
            let mut bandes = Bandes::new(48_000);
            bandes.regler(n);
            let t0 = Instant::now();
            for paquet in son.chunks(480) {
                bandes.push(paquet);
            }
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            eprintln!("filtres du spectre, {n} bandes : {ms:.2} ms par seconde de son ({:.2} % d'un cœur)", ms / 10.0);
        }
    }

    #[test]
    fn spectre_masque_sans_musique() {
        let shared = AudioShared::default();
        let mut cfg = Config::default();
        cfg.spectre.actif = true;
        let mut e = Effets::new();
        let avance = |e: &mut Effets, cfg: &Config, secondes: f32| {
            for _ in 0..(secondes * 60.0) as usize {
                e.avance(0.0, 1.0 / 60.0, (100.0, 100.0), 1.0, 0.0, &shared, cfg, &[]);
            }
        };
        // silence : l'anneau reste un moment, puis s'efface
        avance(&mut e, &cfg, 1.0);
        assert_eq!(e.visible, 1.0);
        avance(&mut e, &cfg, 1.5);
        assert_eq!(e.visible, 0.0);
        // option coupée : il revient et reste
        cfg.spectre.masquer_sans_musique = false;
        avance(&mut e, &cfg, 0.3);
        assert_eq!(e.visible, 1.0);
        // option remise, la musique joue : il reste
        cfg.spectre.masquer_sans_musique = true;
        shared.set_bandes_crete(0.05);
        avance(&mut e, &cfg, 3.0);
        assert_eq!(e.visible, 1.0);
    }
}

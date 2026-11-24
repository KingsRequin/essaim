//! Figures loin du pointeur : équerres aux coins de la fenêtre au premier plan (`[equerres]`) et
//! liseré au bord bas de l'écran du pointeur (`[lisere]`). `Effets` les possède, leur passe le son,
//! le cadre de la fenêtre et l'écran ; `main.rs` ajoute leurs zones à celles des fissures.
//!
//! C'est un habillage de la fenêtre et de l'écran, comme la bordure : hors des presets
//! (`profils::in_profile`), il accompagne l'essaim quel que soit le preset, et reste affiché quand
//! l'essaim ne l'est pas (pointeur masqué par la frappe, veille profonde : `main::habillage_seul`).
//!
//! Le moteur redessine, par écran, un seul rectangle qui englobe toutes les zones de l'image : une
//! zone loin du pointeur l'agrandit d'autant. Ces figures ne donnent donc leurs zones que quand
//! leur dessin change (`zones`) ; le reste du temps, ce qui est affiché reste en place. Quand la
//! boucle principale efface les calques, elle le dit (`oublier`).
//!
//! `ui/apercu.js` les reproduit dans le cadre de l'aperçu : les modifier ici oblige à les reporter là.

use crate::canvas::{Canvas, Rgb};
use crate::config::Config;
use crate::figures::{gauss, melange, Ressort, Son};
use crate::layer::IRect;
use crate::render::prisme;

/// Équerres : écart au cadre au repos (px à 100 %), élan que lui donnent le kick et le drop
/// (px/s), ce que la basse ajoute aux bras (en longueurs), élan des bras au drop (px/s), bornes
/// de l'écart et des bras (px à 100 %, les bras en longueurs réglées).
const EQUERRE_ECART: f32 = 4.0;
const EQUERRE_ELAN: (f32, f32) = (110.0, 260.0);
const EQUERRE_BASSE: f32 = 1.3;
const EQUERRE_ELAN_BRAS: f32 = 38.0;
const EQUERRE_BORNES: ((f32, f32), (f32, f32)) = ((0.0, 26.0), (0.4, 3.2));
/// Écart de couleur (par composante, sur 255) à partir duquel les équerres sont redessinées :
/// une teinte qui glisse ne redemande pas leurs zones à chaque image.
const EQUERRE_NUANCE: u8 = 8;
/// Les équerres au repos redonnent leurs zones à cet intervalle (s) : un calque resté dix secondes
/// sans zone est retiré (`backend::KEEP_EMPTY`), et un calque refait par le moteur (écrans changés,
/// carte graphique perdue) est vide.
const EQUERRE_RAPPEL: f32 = 3.0;

/// Liseré : pas d'un segment (px à 100 %), demi-largeur de la lueur sous le pointeur (px à
/// 100 %, puis part de la largeur de l'écran que la basse y ajoute), éclat d'un kick et d'un drop
/// (force, durée en s, vitesse en px/s à 100 %), largeur d'un éclat (px à 100 %), silence attendu
/// avant l'effacement (s), durée du retour et de l'effacement (s).
const LISERE_PAS: f32 = 6.0;
const LISERE_LUEUR: (f32, f32) = (36.0, 0.26);
const LISERE_KICK: (f32, f32, f32) = (1.0, 0.6, 560.0);
const LISERE_DROP: (f32, f32, f32) = (2.0, 1.1, 760.0);
const LISERE_ECLAT: f32 = 40.0;
const LISERE_ATTENTE: f32 = 1.5;
const LISERE_FONDU: (f32, f32) = (0.15, 0.8);

/// Deux couleurs que l'œil ne distingue pas sur un trait fin.
fn proches(a: Rgb, b: Rgb) -> bool {
    [(a.0, b.0), (a.1, b.1), (a.2, b.2)].iter().all(|&(x, y)| x.abs_diff(y) < EQUERRE_NUANCE)
}

/// Ce qui est dessiné des équerres : cadre de la fenêtre, écart et bras (px écran), opacité des
/// deux diagonales, couleur. Deux images au même dessin ne redemandent pas leurs zones.
#[derive(Clone, Copy, PartialEq)]
struct Equerres {
    cadre: (i32, i32, i32, i32),
    ecart: f32,
    bras: f32,
    opacites: [f32; 2],
    rgb: Rgb,
    epaisseur: f32,
}

impl Equerres {
    /// Les quatre coins : point du cadre, sens vers l'extérieur, diagonale (0 ou 1).
    fn coins(&self) -> [(f32, f32, f32, f32, usize); 4] {
        let (l, t, r, b) = (self.cadre.0 as f32, self.cadre.1 as f32, self.cadre.2 as f32, self.cadre.3 as f32);
        [(l, t, -1.0, -1.0, 0), (r, t, 1.0, -1.0, 1), (r, b, 1.0, 1.0, 0), (l, b, -1.0, 1.0, 1)]
    }

    /// Zone d'un coin : l'équerre, écartée du cadre, et ses deux bras le long du cadre.
    fn zones(&self) -> [IRect; 4] {
        let e = self.ecart.max(self.bras) + self.epaisseur + 2.0;
        self.coins().map(|(x, y, ..)| ((x - e).floor() as i32, (y - e).floor() as i32, (x + e).ceil() as i32, (y + e).ceil() as i32))
    }

    fn dessiner(&self, cv: &mut dyn Canvas) {
        for (x, y, sx, sy, diag) in self.coins() {
            let (px, py) = (x + sx * self.ecart, y + sy * self.ecart);
            cv.polyline(&[(px - sx * self.bras, py), (px, py), (px, py - sy * self.bras)], self.epaisseur, self.rgb, self.opacites[diag], true);
        }
    }
}

/// Éclat qui court le long du liseré, depuis l'aplomb du pointeur.
struct Eclat {
    x: f32,
    ne: f64,
    force: f32,
    vie: f32,
    vitesse: f32,
}

pub struct Loin {
    /// Enveloppe du kick, niveau de la basse lissé, montée suivie.
    kick: f32,
    basse: f32,
    montee: f32,
    // équerres
    ecart: Ressort,
    bras: Ressort,
    diagonales: [f32; 2],
    tour: usize,
    equerres: Option<Equerres>,
    /// Temps depuis la dernière demande des zones des équerres.
    rappel: f32,
    // liseré
    eclats: Vec<Eclat>,
    embrase: f32,
    silence: f32,
    presence: f32,
    /// Bande du liseré (écran) et abscisse du pointeur ; `None` s'il n'y a rien à dessiner.
    bande: Option<(IRect, f32)>,
    /// Zones à redessiner à cette image.
    zones: Vec<IRect>,
}

impl Loin {
    pub fn new() -> Self {
        Self {
            kick: 0.0,
            basse: 0.0,
            montee: 0.0,
            ecart: Ressort::en(EQUERRE_ECART),
            bras: Ressort::en(1.0),
            diagonales: [0.0; 2],
            tour: 0,
            equerres: None,
            rappel: 0.0,
            eclats: Vec::new(),
            embrase: 0.0,
            // silencieux au départ : le liseré n'apparaît qu'avec le son
            silence: LISERE_ATTENTE,
            presence: 0.0,
            bande: None,
            zones: Vec::new(),
        }
    }

    pub fn actives(cfg: &Config) -> bool {
        cfg.equerres.actif || cfg.lisere.actif
    }

    /// Quelque chose est-il affiché ? (Après l'extinction d'un effet, une dernière image l'efface.)
    pub fn affiche(&self) -> bool {
        self.equerres.is_some() || self.bande.is_some()
    }

    /// Les calques viennent d'être effacés : plus rien n'est affiché, la prochaine image redonne
    /// toutes les zones.
    pub fn oublier(&mut self) {
        (self.equerres, self.bande) = (None, None);
    }

    /// Avance d'une image. `fenetre` : cadre de la fenêtre au premier plan (gauche, haut, droite,
    /// bas) ; `ecran` : celui du pointeur.
    #[allow(clippy::too_many_arguments)]
    pub fn avance(&mut self, t: f64, dt: f32, cursor: (f32, f32), scale: f32, son: &Son, cfg: &Config, fenetre: Option<(f32, f32, f32, f32)>, ecran: Option<IRect>) {
        self.zones.clear();
        let (kicks, caisse, drop) = (son.coups.kicks, son.coups.caisses > 0, son.coups.drops > 0);
        self.kick = if kicks > 0 { 1.0 } else { self.kick * (-dt * 7.0).exp() };
        self.basse += (son.au.bass.clamp(0.0, 1.0) - self.basse) * (1.0 - (-dt * 10.0).exp());
        self.montee += (son.au.tension.clamp(0.0, 1.0) - self.montee) * (dt * 6.0).min(1.0);

        // équerres : les ressorts avancent, puis le dessin de l'image est comparé au précédent
        let avant = self.equerres;
        self.equerres = None;
        if cfg.equerres.actif {
            let c = &cfg.equerres;
            if kicks > 0 {
                self.ecart.v += EQUERRE_ELAN.0;
            }
            if caisse {
                self.tour ^= 1;
                self.diagonales[self.tour] = 1.0;
            }
            if drop {
                self.ecart.v += EQUERRE_ELAN.1;
                self.bras.v += EQUERRE_ELAN_BRAS;
            }
            self.diagonales = self.diagonales.map(|d| d * (-dt * 8.0).exp());
            let tremble = (t * 44.0).sin() as f32 * 0.9 * self.montee;
            self.ecart.vers(EQUERRE_ECART * (1.0 - 0.75 * self.montee) + tremble, (17.0, 0.3), dt, EQUERRE_BORNES.0);
            self.bras.vers(1.0 + EQUERRE_BASSE * self.basse, (14.0, 0.4), dt, EQUERRE_BORNES.1);
            if let Some((l, h, r, b)) = fenetre {
                // quantifiés : un dessin qui ne change pas à l'œil ne redemande pas ses zones
                let q = |v: f32, pas: f32| (v / pas).round() * pas;
                let opacite = |d: f32| q((0.5 + 0.5 * d.max(self.kick)).clamp(0.0, 1.0), 1.0 / 32.0);
                // la couleur dessinée est gardée tant que celle de l'essaim en reste proche
                let rgb = prisme(cfg, t, 1.0);
                let rgb = avant.map(|a| a.rgb).filter(|&a| proches(a, rgb)).unwrap_or(rgb);
                self.equerres = Some(Equerres {
                    cadre: (l.round() as i32, h.round() as i32, r.round() as i32, b.round() as i32),
                    ecart: q(self.ecart.x * scale, 0.25),
                    bras: q(c.bras * self.bras.x * scale, 0.25),
                    opacites: self.diagonales.map(opacite),
                    rgb,
                    epaisseur: c.epaisseur * scale,
                });
            }
        }
        self.rappel += dt;
        if self.equerres != avant {
            // anciennes zones (à effacer) et nouvelles
            self.zones.extend(avant.iter().chain(&self.equerres).flat_map(Equerres::zones));
        } else if let (Some(e), true) = (&self.equerres, self.rappel >= EQUERRE_RAPPEL) {
            self.zones.extend(e.zones());
        }
        if !self.zones.is_empty() {
            self.rappel = 0.0;
        }

        // liseré : une bande au bord bas de l'écran du pointeur, tant qu'il y a du son
        let avant = self.bande;
        self.bande = None;
        if cfg.lisere.actif {
            for _ in 0..kicks.min(1) {
                self.eclats.push(Eclat { x: cursor.0, ne: t, force: LISERE_KICK.0, vie: LISERE_KICK.1, vitesse: LISERE_KICK.2 });
            }
            if drop {
                self.eclats.push(Eclat { x: cursor.0, ne: t, force: LISERE_DROP.0, vie: LISERE_DROP.1, vitesse: LISERE_DROP.2 });
                self.embrase = 1.0;
            }
            self.eclats.retain(|e| t - e.ne < e.vie as f64);
            if self.eclats.len() > 8 {
                let trop = self.eclats.len() - 8;
                self.eclats.drain(..trop);
            }
            self.embrase *= (-dt * 1.5).exp();
            let son_la = self.basse > 0.02 || !self.eclats.is_empty() || self.embrase > 0.02 || self.montee > 0.02;
            self.silence = if son_la { 0.0 } else { self.silence + dt };
            self.presence = if self.silence >= LISERE_ATTENTE { (self.presence - dt / LISERE_FONDU.1).max(0.0) } else { (self.presence + dt / LISERE_FONDU.0).min(1.0) };
            if let (Some(e), true) = (ecran, self.presence > 0.0) {
                let haut = (cfg.lisere.epaisseur * scale).ceil() as i32 + 2;
                self.bande = Some(((e.0, e.3 - haut, e.2, e.3), cursor.0));
            }
        } else {
            self.eclats.clear();
            (self.embrase, self.presence, self.silence) = (0.0, 0.0, LISERE_ATTENTE);
        }
        // la bande change à chaque image tant qu'elle est là ; quand elle disparaît ou change
        // d'écran, l'ancienne est effacée
        self.zones.extend(self.bande.map(|b| b.0));
        if let Some((zone, _)) = avant {
            if self.bande.map(|b| b.0) != Some(zone) {
                self.zones.push(zone);
            }
        }
    }

    /// Zones à redessiner à cette image : vides tant que le dessin ne change pas.
    pub fn zones(&self) -> &[IRect] {
        &self.zones
    }

    /// Dessine les équerres et le liseré (à chaque image : ce qui tombe hors des zones
    /// redessinées est rogné par le moteur).
    pub fn dessiner(&self, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32) {
        if let Some(e) = &self.equerres {
            e.dessiner(cv);
        }
        if let Some(((x0, _, x1, bas), px)) = self.bande {
            let (tete, queue) = (prisme(cfg, t, 1.0), prisme(cfg, t, 0.0));
            let (ep, pas) = (cfg.lisere.epaisseur * scale, LISERE_PAS * scale);
            let (largeur, y) = ((x1 - x0) as f32, bas as f32 - ep / 2.0);
            let lueur = LISERE_LUEUR.0 * scale + LISERE_LUEUR.1 * largeur * self.basse;
            let n = (largeur / pas).ceil() as usize;
            for i in 0..n {
                let (a, b) = (x0 as f32 + i as f32 * pas, (x0 as f32 + (i + 1) as f32 * pas).min(x1 as f32));
                let m = (a + b) / 2.0;
                let mut f = (0.1 + 0.75 * self.basse) * gauss((m - px) / lueur) + 0.8 * self.embrase + 0.25 * self.montee;
                for e in &self.eclats {
                    let age = (t - e.ne) as f32;
                    f += e.force * (1.0 - age / e.vie).max(0.0).powf(1.2) * gauss(((m - e.x).abs() - e.vitesse * scale * age) / (LISERE_ECLAT * scale));
                }
                let f = f * self.presence;
                if f >= 0.03 {
                    let teinte = 0.5 + 0.5 * ((m - x0 as f32) / largeur * std::f32::consts::TAU + (t as f32 * 0.5).rem_euclid(std::f32::consts::TAU)).sin();
                    cv.line((a, y), (b, y), ep, melange(tete, queue, teinte), (0.15 + f).min(1.0), false);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioView;
    use crate::render::Coups;
    use tiny_skia::Pixmap;

    const FENETRE: (f32, f32, f32, f32) = (200.0, 150.0, 1000.0, 700.0);
    const ECRAN: IRect = (0, 0, 1200, 800);

    fn avancer(l: &mut Loin, t: f64, bass: f32, coups: Coups, cfg: &Config, fenetre: Option<(f32, f32, f32, f32)>) {
        let au = AudioView { bass, ..Default::default() };
        l.avance(t, 1.0 / 120.0, (600.0, 400.0), 1.0, &Son { au: &au, coups: &coups, bandes: &[] }, cfg, fenetre, Some(ECRAN));
    }

    fn image(l: &Loin, t: f64, cfg: &Config) -> Pixmap {
        let mut pm = Pixmap::new(1200, 800).unwrap();
        l.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, t, cfg, 1.0);
        pm
    }

    fn dans(zones: &[IRect], x: i32, y: i32) -> bool {
        zones.iter().any(|z| x >= z.0 && x < z.2 && y >= z.1 && y < z.3)
    }

    #[test]
    fn eteintes_par_defaut_ni_zone_ni_dessin() {
        let cfg = Config::default();
        let mut l = Loin::new();
        avancer(&mut l, 0.0, 1.0, Coups { kicks: 1, ..Default::default() }, &cfg, Some(FENETRE));
        assert!(l.zones().is_empty() && image(&l, 0.0, &cfg).data().iter().all(|&b| b == 0));
    }

    #[test]
    fn equerres_aux_coins_zones_seulement_quand_le_dessin_change() {
        let mut cfg = Config::default();
        cfg.equerres.actif = true;
        // teinte fixe : au repos, le dessin ne change plus
        (cfg.couleur.vitesse_teinte, cfg.couleur.synchro_prisme) = (0.0, false);
        let mut l = Loin::new();
        let dt = 1.0 / 120.0;
        // première image : les quatre coins sont demandés, et tout le dessin y tient
        avancer(&mut l, 0.0, 0.0, Coups::default(), &cfg, Some(FENETRE));
        let zones = l.zones().to_vec();
        assert_eq!(zones.len(), 4);
        let pm = image(&l, 0.0, &cfg);
        let mut peints = 0;
        for (j, p) in pm.data().chunks_exact(4).enumerate() {
            if p[3] > 0 {
                peints += 1;
                assert!(dans(&zones, (j % 1200) as i32, (j / 1200) as i32), "pixel ({}, {}) hors des zones", j % 1200, j / 1200);
            }
        }
        assert!(peints > 100, "{peints} pixels");
        // rien sur le contenu de la fenêtre : son milieu reste vide
        assert_eq!(pm.pixel(600, 425).unwrap().alpha(), 0);
        // au repos : plus aucune zone
        for i in 1..240 {
            avancer(&mut l, i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
            assert!(l.zones().is_empty(), "image {i} : {:?}", l.zones());
        }
        // un kick : les équerres s'écartent (zones redemandées), puis reviennent au repos
        avancer(&mut l, 2.0, 0.0, Coups { kicks: 1, ..Default::default() }, &cfg, Some(FENETRE));
        let mut ecart_max = 0.0f32;
        let mut redemandees = 0;
        for i in 1..12 {
            avancer(&mut l, 2.0 + i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
            ecart_max = ecart_max.max(l.ecart.x);
            // anciennes et nouvelles zones, sauf quand le dessin n'a pas bougé d'un quart de pixel
            assert!(matches!(l.zones().len(), 0 | 8), "{} zones", l.zones().len());
            redemandees += (l.zones().len() == 8) as usize;
        }
        assert!(redemandees >= 8, "{redemandees} images redessinées sur 11");
        assert!(ecart_max > EQUERRE_ECART + 2.0 && ecart_max <= EQUERRE_BORNES.0 .1, "{ecart_max}");
        for i in 12..480 {
            avancer(&mut l, 2.0 + i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
        }
        assert!((l.ecart.x - EQUERRE_ECART).abs() < 0.1, "{}", l.ecart.x);
        // de nouveau au repos : seul le rappel redonne les zones, toutes les trois secondes
        let mut rappels = 0;
        for i in 480..1680 {
            avancer(&mut l, 2.0 + i as f64 * dt, 0.0, Coups::default(), &cfg, Some(FENETRE));
            assert!(matches!(l.zones().len(), 0 | 4), "{} zones", l.zones().len());
            rappels += !l.zones().is_empty() as usize;
        }
        assert!((3..=4).contains(&rappels), "{rappels} rappels en 10 s");
        // les calques sont effacés (pause, frappe) : tout est redemandé à l'image suivante
        l.oublier();
        assert!(!l.affiche() && image(&l, 14.0, &cfg).data().iter().all(|&b| b == 0));
        avancer(&mut l, 14.0, 0.0, Coups::default(), &cfg, Some(FENETRE));
        assert!(l.affiche() && l.zones().len() == 4);
        // la fenêtre disparaît : ses coins sont effacés une fois, puis plus rien
        avancer(&mut l, 14.01, 0.0, Coups::default(), &cfg, None);
        assert_eq!(l.zones().len(), 4);
        assert!(!l.affiche() && image(&l, 14.01, &cfg).data().iter().all(|&b| b == 0));
        avancer(&mut l, 14.02, 0.0, Coups::default(), &cfg, None);
        assert!(l.zones().is_empty());
        // l'effet est éteint pendant qu'il est affiché : une dernière image l'efface
        avancer(&mut l, 14.03, 0.0, Coups::default(), &cfg, Some(FENETRE));
        cfg.equerres.actif = false;
        assert!(l.affiche() && !Loin::actives(&cfg));
        avancer(&mut l, 14.04, 0.0, Coups::default(), &cfg, Some(FENETRE));
        assert!(!l.affiche() && l.zones().len() == 4);
    }

    #[test]
    fn equerres_une_teinte_qui_glisse_redessine_peu() {
        assert!(proches((200, 10, 10), (207, 3, 17)) && !proches((200, 10, 10), (208, 10, 10)));
        let mut cfg = Config::default();
        cfg.equerres.actif = true;
        (cfg.couleur.vitesse_teinte, cfg.couleur.synchro_prisme) = (20.0, false);
        let mut l = Loin::new();
        let mut redessinees = 0;
        for i in 0..480 {
            avancer(&mut l, i as f64 / 120.0, 0.0, Coups::default(), &cfg, Some(FENETRE));
            redessinees += !l.zones().is_empty() as usize;
        }
        // 20°/s : la couleur change d'un cran ~85 fois par seconde, les équerres ~10 fois
        assert!(redessinees < 100, "{redessinees} images redessinées sur 480");
    }

    #[test]
    fn lisere_au_bord_bas_seulement_avec_du_son() {
        let mut cfg = Config::default();
        cfg.lisere.actif = true;
        let mut l = Loin::new();
        let dt = 1.0 / 120.0;
        // silence : rien
        for i in 0..60 {
            avancer(&mut l, i as f64 * dt, 0.0, Coups::default(), &cfg, None);
        }
        assert!(l.zones().is_empty() && image(&l, 0.5, &cfg).data().iter().all(|&b| b == 0));
        // musique : une bande au bord bas de l'écran, et rien au-dessus
        for i in 60..180 {
            avancer(&mut l, i as f64 * dt, 0.8, Coups { kicks: (i % 60 == 0) as u32, ..Default::default() }, &cfg, None);
        }
        let zones = l.zones().to_vec();
        assert_eq!(zones, [(0, 800 - 5, 1200, 800)]);
        let pm = image(&l, 1.5, &cfg);
        let mut peints = 0;
        for (j, p) in pm.data().chunks_exact(4).enumerate() {
            if p[3] > 0 {
                peints += 1;
                assert!(dans(&zones, (j % 1200) as i32, (j / 1200) as i32), "pixel hors de la bande");
            }
        }
        assert!(peints > 200, "{peints} pixels");
        // le son s'arrête : la bande s'efface (une dernière zone pour l'effacer), puis plus rien
        let mut dernieres = 0;
        for i in 180..700 {
            avancer(&mut l, i as f64 * dt, 0.0, Coups::default(), &cfg, None);
            dernieres = if l.zones().is_empty() { dernieres } else { i };
        }
        assert!(l.zones().is_empty() && dernieres > 180 && dernieres < 700, "dernière zone à l'image {dernieres}");
        assert!(image(&l, 6.0, &cfg).data().iter().all(|&b| b == 0));
    }
}

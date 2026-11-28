//! Modulations (`[modulations]`) : une source du son (basse, note grave ou aiguë, kick, caisse
//! claire, charleston, montée, drop, stéréo, tempo, contretemps, brillance) agit sur un
//! paramètre de l'essaim (rotation du trajet, vitesse, rayon, taille, teinte, décalage,
//! luminosité, traînée, opacité), pour les comètes, les satellites ou les deux, ou pour les
//! figures autour du pointeur (toutes ou l'une d'elles : `Effets::figures`). Chaque source
//! est ramenée de 0 à 1 (stéréo de -1 à 1) ; les coups (kick, caisse claire, charleston, drop,
//! contretemps) sont des enveloppes qui montent d'un coup et retombent. Chaque modulation a son
//! seuil, son lissage et peut prendre sa source à l'envers. Les réactions continues d'avant (la
//! basse qui accélère, la note grave qui éloigne l'orbite, la montée qui resserre…) sont des
//! modulations par défaut (`migration::Ancien`). Reproduit dans `ui/apercu.js` (`Sources`,
//! `effetsModulations`).

use crate::audio::AudioView;
use crate::config::{Cible, Groupe, Modulation, Source};
use std::f32::consts::TAU;

/// Effet d'une modulation à fond (quantité 1, source 1) : tours/min ajoutés à la rotation du
/// trajet, radians/s ajoutés à la vitesse sur le trajet, part du rayon, de la taille, de la
/// lueur, de la traînée et de l'opacité ajoutée (ou ôtée), degrés de teinte, rayons de
/// décalage (vers la droite).
pub const ROTATION: f32 = 90.0;
pub const VITESSE: f32 = 40.0;
pub const ACCELERATION: f32 = 2.0;
pub const RAYON: f32 = 4.0;
pub const TAILLE: f32 = 1.5;
pub const TEINTE: f32 = 180.0;
pub const DECALAGE: f32 = 1.6;
pub const LUMINOSITE: f32 = 2.0;
pub const TRAINEE: f32 = 3.0;
pub const OPACITE: f32 = 1.0;

/// Bornes des totaux, réglage compris : rotation du trajet ou d'une figure (tr/min, de chaque
/// côté), vitesse sur le trajet (radians/s). Au-delà, l'œil ne suit plus (et une somme de
/// modulations à fond dépasserait des centaines de tours par minute).
pub const ROTATION_MAX: f32 = 180.0;
pub const VITESSE_MAX: f32 = 60.0;

/// Cibles qui se multiplient : effet à fond (part ajoutée par quantité 1) et bornes du facteur
/// total. `ui/app.js` (`BORNES_MOD`) en tire l'effet affiché, borné, et la quantité au-delà de laquelle
/// la source à fond ne change plus rien (dite dans l'infobulle ; la glissière va de −100 à 100 %).
pub fn multipliee(c: Cible) -> Option<(f32, f32, f32)> {
    Some(match c {
        Cible::Acceleration => (ACCELERATION, 0.0, 5.0),
        Cible::Rayon => (RAYON, 0.1, 5.0),
        Cible::Taille => (TAILLE, 0.0, 4.0),
        Cible::Luminosite => (LUMINOSITE, 0.0, 4.0),
        Cible::Trainee => (TRAINEE, 0.1, 5.0),
        Cible::Opacite => (OPACITE, 0.0, 1.0),
        Cible::Rotation | Cible::Vitesse | Cible::Teinte | Cible::Decalage | Cible::Aucune => return None,
        Cible::Ouverture | Cible::Ecart | Cible::Forme | Cible::Relief | Cible::Duree | Cible::Nombre | Cible::Noyau => return None,
    })
}

/// Rotation totale (tr/min) ramenée dans ±`ROTATION_MAX` (0 si elle n'est pas finie).
pub fn borne_rotation(tours_min: f32) -> f32 {
    if tours_min.is_finite() { tours_min.clamp(-ROTATION_MAX, ROTATION_MAX) } else { 0.0 }
}

/// Retombée des enveloppes (par seconde) : kick, caisse claire, charleston, drop, contretemps ;
/// suivi de la montée.
const RETOMBEE: [f32; 5] = [7.0, 8.0, 10.0, 0.9, 10.0];
const SUIVI_MONTEE: f32 = 6.0;

/// Enveloppes des coups (1 au coup, puis retombée) et montée suivie.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sources {
    pub kick: f32,
    pub caisse: f32,
    pub charleston: f32,
    pub drop: f32,
    pub contretemps: f32,
    pub montee: f32,
}

impl Sources {
    /// Avance de `dt` secondes : `coups` = nouveaux kicks, caisses claires, attaques des aigus,
    /// drops et contretemps depuis l'image précédente ; `tension` = montée détectée (0 à 1).
    pub fn avancer(&mut self, dt: f32, coups: [u32; 5], tension: f32) {
        let env = [&mut self.kick, &mut self.caisse, &mut self.charleston, &mut self.drop, &mut self.contretemps];
        for ((e, n), r) in env.into_iter().zip(coups).zip(RETOMBEE) {
            *e = if n > 0 { 1.0 } else { *e * (-dt * r).exp() };
            if !e.is_finite() {
                *e = 0.0;
            }
        }
        let tension = if tension.is_finite() { tension.clamp(0.0, 1.0) } else { 0.0 };
        self.montee += (tension - self.montee) * (dt * SUIVI_MONTEE).min(1.0);
        if !self.montee.is_finite() {
            self.montee = 0.0;
        }
    }

    /// Valeur de `source` en ce moment ; `grave` = 0 à 1 selon la note de la basse (1 : très
    /// grave), `presence` = 0 à 1 : la basse est-elle là (pleine dès 0,4 de niveau).
    pub fn valeur(&self, source: Source, au: &AudioView, grave: f32, presence: f32) -> f32 {
        let v = match source {
            Source::Basse => au.bass.clamp(0.0, 1.0),
            Source::Grave => grave * presence,
            Source::Aigu => (1.0 - grave) * presence,
            Source::Kick => self.kick,
            Source::Caisse => self.caisse,
            Source::Charleston => self.charleston,
            Source::Montee => self.montee,
            Source::Drop => self.drop,
            Source::Contretemps => self.contretemps,
            Source::Brillance => au.brillance.clamp(0.0, 1.0),
            Source::Stereo => au.balance.clamp(-1.0, 1.0),
            Source::Tempo => au.bpm.map_or(0.0, |_| 0.5 + 0.5 * (TAU * au.phase).cos()),
            Source::Aucune => 0.0,
        };
        if v.is_finite() { v } else { 0.0 }
    }
}

/// Valeur visée d'une modulation (quantité × source) : la source à l'envers si demandé (1 − v ;
/// stéréo : −v), rien sous le seuil puis jusqu'à 1 au-dessus (stéréo : de chaque côté).
pub fn visee(m: &Modulation, source: f32) -> f32 {
    let signe = if m.source == Source::Stereo { source.signum() } else { 1.0 };
    let v = if m.source == Source::Stereo { source.abs() } else { source.max(0.0) };
    let v = if m.inverser && m.source != Source::Stereo { 1.0 - v.min(1.0) } else { v };
    let signe = if m.inverser && m.source == Source::Stereo { -signe } else { signe };
    let s = m.seuil.clamp(0.0, 0.95);
    let v = ((v - s) / (1.0 - s)).max(0.0);
    let q = if m.quantite.is_finite() { m.quantite.clamp(-1.0, 1.0) } else { 0.0 };
    let r = q * v * signe;
    if r.is_finite() { r } else { 0.0 }
}

/// Part du chemin vers la valeur visée faite en `dt` secondes : `lissage` 0 = aussitôt, 1 =
/// constante de temps de 0,4 s (0,35 ≈ 50 ms).
pub fn part_lissage(lissage: f32, dt: f32) -> f32 {
    let tau = 0.4 * lissage.clamp(0.0, 1.0).powi(2);
    if tau < 1e-4 { 1.0 } else { 1.0 - (-dt / tau).exp() }
}

/// Paramètres modulés d'un groupe (comètes ou satellites).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effet {
    /// Tours par minute ajoutés à la rotation du trajet, radians/s ajoutés à la vitesse sur le
    /// trajet, degrés de teinte ajoutés, décalage vers la droite (en rayons de l'orbite).
    pub rotation: f32,
    pub vitesse: f32,
    pub teinte: f32,
    pub decalage: f32,
    /// Facteurs de la vitesse (accélération), du rayon, de la taille, de la lueur, de la traînée
    /// et de l'opacité.
    pub acceleration: f32,
    pub rayon: f32,
    pub taille: f32,
    pub luminosite: f32,
    pub trainee: f32,
    pub opacite: f32,
    /// Parts ajoutées aux paramètres propres d'une figure (quantité × source, additionnées) :
    /// chaque figure lit celles qui la concernent (`figures.rs`), les autres n'y font rien.
    pub propres: Propres,
}

/// Paramètres propres des figures que les modulations font varier (cibles `ouverture`, `ecart`,
/// `forme`, `relief`, `duree`, `nombre`, `noyau`) : 0 sans modulation.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Propres {
    pub ouverture: f32,
    pub ecart: f32,
    pub forme: f32,
    pub relief: f32,
    pub duree: f32,
    pub nombre: f32,
    pub noyau: f32,
}

impl Default for Effet {
    fn default() -> Self {
        Self { rotation: 0.0, vitesse: 0.0, teinte: 0.0, decalage: 0.0, acceleration: 1.0, rayon: 1.0, taille: 1.0, luminosite: 1.0, trainee: 1.0, opacite: 1.0, propres: Propres::default() }
    }
}

/// Effets des modulations : sur les comètes et sur les satellites, sur chaque figure (dans
/// l'ordre de `config::FIGURES`), et teinte de tout ce qu'Essaim dessine (modulations de la teinte
/// « sur tout »).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Effets {
    pub groupes: [Effet; 2],
    pub figures: [Effet; NB_FIGURES],
    pub teinte: f32,
}

/// Nombre de figures que les modulations peuvent viser.
pub const NB_FIGURES: usize = crate::config::FIGURES.len();

/// Effets de `liste`, `valeurs[i]` étant la valeur lissée de la modulation `i` (`visee`). Les
/// rotations, vitesses, teintes et décalages s'additionnent ; rayon, taille, lueur, traînée et
/// opacité se multiplient (chaque modulation garde sa part : deux réactions qui resserrent
/// l'orbite de moitié la resserrent au quart), dans des bornes lisibles (le rayon ne s'annule
/// pas : `multipliee`). La rotation et la vitesse totales sont bornées là où le réglage s'y
/// ajoute (`Swarm::update`, `Swarm::vitesse`, `Figures::poser` : `ROTATION_MAX`, `VITESSE_MAX`).
pub fn effets(liste: &[Modulation], valeurs: &[f32]) -> Effets {
    let mut r = Effets::default();
    for (m, &v) in liste.iter().zip(valeurs) {
        if !v.is_finite() {
            continue;
        }
        let facteur = |c: Cible| multipliee(c).map_or(1.0, |(fond, ..)| (1.0 + v * fond).max(0.0));
        // teinte « sur tout » : celle de tout ce qu'Essaim dessine
        if m.cible == Cible::Teinte && m.sur == Groupe::Tout {
            r.teinte += v * TEINTE;
            continue;
        }
        let groupes = r.groupes.iter_mut().enumerate().filter(|(g, _)| m.sur.inclut(*g == 1)).map(|(_, e)| e);
        let figures = r.figures.iter_mut().enumerate().filter(|(i, _)| m.sur.vise_figure(*i)).map(|(_, e)| e);
        for e in groupes.chain(figures) {
            match m.cible {
                Cible::Rotation => e.rotation += v * ROTATION,
                Cible::Vitesse => e.vitesse += v * VITESSE,
                Cible::Teinte => e.teinte += v * TEINTE,
                Cible::Decalage => e.decalage += v * DECALAGE,
                Cible::Acceleration => e.acceleration *= facteur(m.cible),
                Cible::Rayon => e.rayon *= facteur(m.cible),
                Cible::Taille => e.taille *= facteur(m.cible),
                Cible::Luminosite => e.luminosite *= facteur(m.cible),
                Cible::Trainee => e.trainee *= facteur(m.cible),
                Cible::Opacite => e.opacite *= facteur(m.cible),
                Cible::Ouverture => e.propres.ouverture += v,
                Cible::Ecart => e.propres.ecart += v,
                Cible::Forme => e.propres.forme += v,
                Cible::Relief => e.propres.relief += v,
                Cible::Duree => e.propres.duree += v,
                Cible::Nombre => e.propres.nombre += v,
                Cible::Noyau => e.propres.noyau += v,
                Cible::Aucune => {}
            }
        }
    }
    let borne = |x: &mut f32, c: Cible| {
        if let Some((_, lo, hi)) = multipliee(c) {
            *x = x.clamp(lo, hi);
        }
    };
    for e in r.groupes.iter_mut().chain(&mut r.figures) {
        borne(&mut e.acceleration, Cible::Acceleration);
        borne(&mut e.rayon, Cible::Rayon);
        borne(&mut e.taille, Cible::Taille);
        borne(&mut e.luminosite, Cible::Luminosite);
        borne(&mut e.trainee, Cible::Trainee);
        borne(&mut e.opacite, Cible::Opacite);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Groupe;

    fn m(source: Source, cible: Cible, sur: Groupe, quantite: f32) -> Modulation {
        Modulation { source, cible, sur, quantite, ..Default::default() }
    }

    fn a_fond(liste: &[Modulation]) -> Effets {
        let v: Vec<f32> = liste.iter().map(|x| visee(x, 1.0)).collect();
        effets(liste, &v)
    }

    #[test]
    fn chaque_modulation_touche_son_groupe() {
        let liste = [m(Source::Drop, Cible::Rotation, Groupe::Satellites, 0.5), m(Source::Basse, Cible::Rayon, Groupe::Tout, 0.25)];
        let [c, s] = a_fond(&liste).groupes;
        assert_eq!(c.rotation, 0.0);
        assert_eq!(s.rotation, 0.5 * ROTATION);
        assert_eq!((c.rayon, s.rayon), (2.0, 2.0));
        // source muette : rien ne change
        assert_eq!(effets(&liste, &[0.0, 0.0]), Effets::default());
    }

    #[test]
    fn rayons_qui_se_multiplient_et_bornes() {
        // deux réactions qui resserrent de moitié : au quart
        let moitie = m(Source::Montee, Cible::Rayon, Groupe::Tout, -0.5 / RAYON);
        assert!((a_fond(&[moitie, moitie]).groupes[0].rayon - 0.25).abs() < 1e-5);
        let liste = [
            m(Source::Kick, Cible::Rayon, Groupe::Cometes, -1.0),
            m(Source::Kick, Cible::Vitesse, Groupe::Cometes, -1.0),
            m(Source::Kick, Cible::Taille, Groupe::Cometes, f32::NAN),
            m(Source::Kick, Cible::Opacite, Groupe::Cometes, 1.0),
            m(Source::Aucune, Cible::Aucune, Groupe::Tout, 1.0),
        ];
        let [c, s] = a_fond(&liste).groupes;
        assert_eq!((c.rayon, c.vitesse, c.taille, c.opacite), (0.1, -VITESSE, 1.0, 1.0));
        assert_eq!(s, Effet::default());
    }

    #[test]
    fn les_figures_ont_leurs_modulations() {
        use crate::config::FIGURES;
        let orbe = FIGURES.iter().position(|&f| f == "orbe").unwrap();
        let liste = [m(Source::Kick, Cible::Rayon, Groupe::Orbe, 0.25), m(Source::Kick, Cible::Opacite, Groupe::Figures, -0.5), m(Source::Kick, Cible::Taille, Groupe::Tout, 1.0)];
        let e = a_fond(&liste);
        for (i, f) in e.figures.iter().enumerate() {
            // « tout » reste l'essaim : comètes et satellites
            assert_eq!((f.opacite, f.taille), (0.5, 1.0), "{}", FIGURES[i]);
            assert_eq!(f.rayon, if i == orbe { 2.0 } else { 1.0 }, "{}", FIGURES[i]);
        }
        assert_eq!((e.groupes[0].rayon, e.groupes[0].opacite, e.groupes[1].taille), (1.0, 1.0, 2.5));
        // le groupe d'une figure ne touche ni les comètes ni les satellites
        assert!(!Groupe::Orbe.inclut(false) && !Groupe::Figures.inclut(true));
        assert!(Groupe::Figures.vise_figure(8) && !Groupe::Tout.vise_figure(0) && Groupe::Gouttes.vise_figure(8) && !Groupe::Gouttes.vise_figure(3));
        // noms écrits dans les réglages
        assert_eq!(serde_json::to_value(Groupe::QuatreTemps).unwrap(), "quatre_temps");
        let g: Groupe = serde_json::from_value(serde_json::json!("pinceau")).unwrap();
        assert_eq!(g, Groupe::Pinceau);
    }

    #[test]
    fn teinte_sur_tout_et_par_groupe() {
        let liste = [m(Source::Basse, Cible::Teinte, Groupe::Tout, 0.5), m(Source::Basse, Cible::Teinte, Groupe::Cometes, 0.25)];
        let e = a_fond(&liste);
        assert_eq!(e.teinte, 90.0);
        assert_eq!((e.groupes[0].teinte, e.groupes[1].teinte), (45.0, 0.0));
    }

    #[test]
    fn seuil_inverser_et_stereo() {
        let mut x = m(Source::Basse, Cible::Rayon, Groupe::Tout, 1.0);
        x.seuil = 0.5;
        assert_eq!((visee(&x, 0.4), visee(&x, 0.75), visee(&x, 1.0)), (0.0, 0.5, 1.0));
        x.seuil = 0.0;
        x.inverser = true;
        assert_eq!((visee(&x, 0.0), visee(&x, 1.0)), (1.0, 0.0));
        let mut st = m(Source::Stereo, Cible::Decalage, Groupe::Tout, 1.0);
        assert_eq!((visee(&st, -0.5), visee(&st, 0.5)), (-0.5, 0.5));
        st.inverser = true;
        st.seuil = 0.5;
        assert_eq!((visee(&st, -1.0), visee(&st, 0.25)), (1.0, 0.0));
        assert_eq!(visee(&st, f32::NAN), 0.0);
    }

    #[test]
    fn lissage() {
        assert_eq!(part_lissage(0.0, 0.016), 1.0);
        let k = part_lissage(0.35, 0.016);
        assert!(k > 0.2 && k < 0.35, "{k}");
        assert!(part_lissage(1.0, 0.016) < 0.05);
    }

    #[test]
    fn coups_montent_puis_retombent() {
        let mut s = Sources::default();
        s.avancer(0.01, [1, 0, 0, 1, 1], 0.0);
        assert_eq!((s.kick, s.drop, s.caisse, s.contretemps), (1.0, 1.0, 0.0, 1.0));
        for _ in 0..50 {
            s.avancer(0.01, [0; 5], 1.0);
        }
        // le kick est retombé en une demi-seconde, le drop dure plus longtemps ; la montée suit
        assert!(s.kick < 0.05 && s.drop > 0.6 && s.montee > 0.9, "{s:?}");
        s.avancer(0.01, [0; 5], f32::NAN);
        assert!(s.montee.is_finite());
    }

    #[test]
    fn valeurs_des_sources() {
        let s = Sources::default();
        let au = AudioView { balance: -0.5, bpm: Some(120.0), phase: 0.0, bass: 0.3, brillance: 0.6, ..Default::default() };
        assert_eq!(s.valeur(Source::Basse, &au, 0.0, 0.75), 0.3);
        assert_eq!(s.valeur(Source::Brillance, &au, 0.0, 0.0), 0.6);
        assert_eq!(s.valeur(Source::Stereo, &au, 0.0, 0.0), -0.5);
        assert_eq!(s.valeur(Source::Tempo, &au, 0.0, 0.0), 1.0);
        assert_eq!(s.valeur(Source::Tempo, &AudioView::default(), 0.0, 0.0), 0.0);
        assert_eq!(s.valeur(Source::Grave, &au, 0.8, 0.5), 0.4);
        assert!((s.valeur(Source::Aigu, &au, 0.8, 0.5) - 0.1).abs() < 1e-6);
    }
}

#[cfg(test)]
mod reactions {
    use super::*;
    use crate::migration::Ancien;

    #[test]
    fn reactions_de_base_comme_avant() {
        // basse à fond : +11 rad/s ; note très grave, basse là : rayon × 2,3 ; montée à fond :
        // rayon × 0,55, vitesse × 2,2
        let l = Ancien::default().modulations(true, true);
        assert_eq!(l.len(), 4);
        let un = |sources: &[Source]| -> Effets {
            let v: Vec<f32> = l.iter().map(|x| visee(x, if sources.contains(&x.source) { 1.0 } else { 0.0 })).collect();
            effets(&l, &v)
        };
        let [c, s] = un(&[Source::Basse, Source::Grave]).groupes;
        assert!((c.vitesse - 11.0).abs() < 0.01 && (c.rayon - 2.3).abs() < 0.01, "{c:?}");
        assert_eq!(c, s);
        let [c, _] = un(&[Source::Montee]).groupes;
        assert!((c.acceleration - 2.2).abs() < 0.01 && (c.rayon - 0.55).abs() < 0.01, "{c:?}");
    }
}

//! Modulations (`[modulations]`) : une source du son (basse, note grave ou aiguë, kick, caisse
//! claire, charleston, montée, drop, stéréo, tempo) agit sur un paramètre de l'essaim (rotation
//! du trajet, vitesse, rayon, taille, teinte), pour les comètes, les satellites ou les deux.
//! Chaque source est ramenée de 0 à 1 (stéréo de -1 à 1) ; les coups (kick, caisse claire,
//! charleston, drop) sont des enveloppes qui montent d'un coup et retombent. Les réactions de
//! base (la basse qui accélère, la note grave qui éloigne l'orbite) sont deux modulations,
//! présentes par défaut (`config::reactions_de_base`). Reproduit dans `ui/apercu.js`
//! (`Sources`, `effetsModulations`).

use crate::audio::AudioView;
use crate::config::{Cible, Modulation, Source};
use std::f32::consts::TAU;

/// Effet d'une modulation à fond (quantité 1, source 1) : tours par minute ajoutés à la
/// rotation du trajet, radians/s ajoutés à la vitesse sur le trajet (comme l'ancien
/// `vitesse_basse` à fond), part du rayon et de la taille ajoutée (comme l'ancien
/// `eloignement_grave`), degrés de teinte ajoutés.
pub const ROTATION: f32 = 90.0;
pub const VITESSE: f32 = 40.0;
pub const RAYON: f32 = 4.0;
pub const TAILLE: f32 = 1.5;
pub const TEINTE: f32 = 180.0;

/// Retombée des enveloppes (par seconde) : kick, caisse claire, charleston, drop ; suivi de la
/// montée.
const RETOMBEE: [f32; 4] = [7.0, 8.0, 10.0, 0.9];
const SUIVI_MONTEE: f32 = 6.0;

/// Enveloppes des coups (1 au coup, puis retombée) et montée suivie.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sources {
    pub kick: f32,
    pub caisse: f32,
    pub charleston: f32,
    pub drop: f32,
    pub montee: f32,
}

impl Sources {
    /// Avance de `dt` secondes : `coups` = nouveaux kicks, caisses claires, attaques des aigus et
    /// drops depuis l'image précédente ; `tension` = montée détectée (0 à 1).
    pub fn avancer(&mut self, dt: f32, coups: [u32; 4], tension: f32) {
        let env = [&mut self.kick, &mut self.caisse, &mut self.charleston, &mut self.drop];
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
            Source::Stereo => au.balance.clamp(-1.0, 1.0),
            Source::Tempo => au.bpm.map_or(0.0, |_| 0.5 + 0.5 * (TAU * au.phase).cos()),
            Source::Aucune => 0.0,
        };
        if v.is_finite() { v } else { 0.0 }
    }
}

/// Paramètres modulés d'un groupe (comètes ou satellites).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effet {
    /// Tours par minute ajoutés à la rotation du trajet, radians/s ajoutés à la vitesse sur le
    /// trajet.
    pub rotation: f32,
    pub vitesse: f32,
    /// Facteurs du rayon et de la taille.
    pub rayon: f32,
    pub taille: f32,
    /// Degrés de teinte ajoutés.
    pub teinte: f32,
}

impl Default for Effet {
    fn default() -> Self {
        Self { rotation: 0.0, vitesse: 0.0, rayon: 1.0, taille: 1.0, teinte: 0.0 }
    }
}

impl Effet {
    /// Rapproche de `vise` d'une part `k` (0 à 1).
    pub fn lisser(&mut self, vise: &Effet, k: f32) {
        self.rotation += (vise.rotation - self.rotation) * k;
        self.vitesse += (vise.vitesse - self.vitesse) * k;
        self.rayon += (vise.rayon - self.rayon) * k;
        self.taille += (vise.taille - self.taille) * k;
        self.teinte += (vise.teinte - self.teinte) * k;
        if ![self.rotation, self.vitesse, self.rayon, self.taille, self.teinte].iter().all(|v| v.is_finite()) {
            *self = Effet::default();
        }
    }
}

/// Effets de `liste` sur les comètes (`[0]`) et les satellites (`[1]`), `valeur` donnant la
/// valeur de chaque source. Les modulations d'une même cible s'additionnent ; rayon et taille
/// restent dans des bornes lisibles (le rayon ne s'annule pas) ; la vitesse totale est bornée
/// par `Swarm::update` (jamais négative).
pub fn effets(liste: &[Modulation], valeur: impl Fn(Source) -> f32) -> [Effet; 2] {
    let mut somme = [[0f32; 5]; 2];
    for m in liste {
        let q = if m.quantite.is_finite() { m.quantite.clamp(-1.0, 1.0) } else { 0.0 };
        let i = match m.cible {
            Cible::Rotation => 0,
            Cible::Vitesse => 1,
            Cible::Rayon => 2,
            Cible::Taille => 3,
            Cible::Teinte => 4,
            Cible::Aucune => continue,
        };
        let v = q * valeur(m.source);
        for (g, s) in somme.iter_mut().enumerate() {
            if m.sur.inclut(g == 1) {
                s[i] += v;
            }
        }
    }
    somme.map(|s| Effet {
        rotation: s[0] * ROTATION,
        vitesse: s[1] * VITESSE,
        rayon: (1.0 + s[2] * RAYON).clamp(0.1, 5.0),
        taille: (1.0 + s[3] * TAILLE).clamp(0.0, 4.0),
        teinte: s[4] * TEINTE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Groupe;

    fn m(source: Source, cible: Cible, sur: Groupe, quantite: f32) -> Modulation {
        Modulation { source, cible, sur, quantite }
    }

    #[test]
    fn chaque_modulation_touche_son_groupe() {
        let liste = [m(Source::Drop, Cible::Rotation, Groupe::Satellites, 0.5), m(Source::Basse, Cible::Rayon, Groupe::Tout, 1.0)];
        let [c, s] = effets(&liste, |_| 1.0);
        assert_eq!(c.rotation, 0.0);
        assert_eq!(s.rotation, 0.5 * ROTATION);
        assert_eq!((c.rayon, s.rayon), (5.0, 5.0));
        // source muette : rien ne change
        assert_eq!(effets(&liste, |_| 0.0), [Effet::default(); 2]);
    }

    #[test]
    fn bornes_et_valeurs_absurdes() {
        let liste = [
            m(Source::Kick, Cible::Rayon, Groupe::Cometes, -1.0),
            m(Source::Kick, Cible::Vitesse, Groupe::Cometes, -1.0),
            m(Source::Kick, Cible::Taille, Groupe::Cometes, f32::NAN),
            m(Source::Aucune, Cible::Aucune, Groupe::Tout, 1.0),
        ];
        let [c, s] = effets(&liste, |_| 1.0);
        assert_eq!((c.rayon, c.vitesse, c.taille), (0.1, -VITESSE, 1.0));
        assert_eq!(s, Effet::default());
    }

    #[test]
    fn coups_montent_puis_retombent() {
        let mut s = Sources::default();
        s.avancer(0.01, [1, 0, 0, 1], 0.0);
        assert_eq!((s.kick, s.drop, s.caisse), (1.0, 1.0, 0.0));
        for _ in 0..50 {
            s.avancer(0.01, [0; 4], 1.0);
        }
        // le kick est retombé en une demi-seconde, le drop dure plus longtemps ; la montée suit
        assert!(s.kick < 0.05 && s.drop > 0.6 && s.montee > 0.9, "{s:?}");
        s.avancer(0.01, [0; 4], f32::NAN);
        assert!(s.montee.is_finite());
    }

    #[test]
    fn valeurs_des_sources() {
        let s = Sources::default();
        let au = AudioView { balance: -0.5, bpm: Some(120.0), phase: 0.0, bass: 0.3, ..Default::default() };
        assert_eq!(s.valeur(Source::Basse, &au, 0.0, 0.75), 0.3);
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
    use crate::config::{reactions_de_base, Groupe};

    #[test]
    fn reactions_de_base_comme_avant() {
        // vitesse_basse = 11 et eloignement_grave = 1,3 : basse à fond, +11 rad/s ; note très
        // grave avec la basse là, rayon × 2,3
        let l = reactions_de_base(11.0, 1.3);
        assert!(l.iter().all(|m| m.sur == Groupe::Tout));
        let [c, s] = effets(&l, |src| if src == Source::Basse || src == Source::Grave { 1.0 } else { 0.0 });
        assert!((c.vitesse - 11.0).abs() < 0.01 && (c.rayon - 2.3).abs() < 0.01, "{c:?}");
        assert_eq!(c, s);
        // réaction nulle : omise
        assert_eq!(reactions_de_base(0.0, 1.0).len(), 1);
    }
}

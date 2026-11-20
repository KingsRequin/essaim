//! Presets fournis avec Essaim (`builtins`), et ceux d'avant gardés pour les essais
//! (`presets_d_avant`). Chacun part des réglages par défaut ; `profils.rs` les liste avec ceux de
//! l'utilisateur.

use crate::config::{Config, Declencheur, Forme, Modulation};
#[cfg(test)]
use crate::migration::Ancien;
use crate::profils::Profile;

/// Presets fournis : chacun a son caractère (forme, couleurs, effets, réactions à la musique),
/// pour montrer ce que l'essaim sait faire. Ils partent des réglages par défaut et de leurs
/// réactions ; ceux qui changent de réactions les donnent toutes.
pub(crate) fn builtins() -> Vec<Profile> {
    use crate::config::{Action as A, Cible as Ci, Evenement as E, FormeSatellites as FS, Groupe as G, Source as S};
    let make = |name: &str, change: fn(&mut Config)| {
        let mut cfg = Config::default();
        change(&mut cfg);
        Profile { name: name.to_owned(), builtin: true, cfg, date: None }
    };
    vec![
        make("Défaut", |_| {}),
        // pour travailler : petit, lent, peu de satellites, pas de fissure
        make("Discret", |c| {
            (c.orbite.rayon, c.orbite.vitesse_base, c.orbite.trainee) = (15.0, 2.0, 0.2);
            (c.essaim.maximum, c.essaim.duree) = (6, 2.0);
            (c.couleur.vitesse_teinte, c.couleur.ecart_teinte) = (15.0, 70.0);
            (c.eclat.taille, c.particules.taille, c.bordure.repos) = (1.8, 0.6, 0.3);
            c.modulations.liste = [vec![m(S::Basse, Ci::Vitesse, G::Tout, 0.12), m(S::Grave, Ci::Rayon, G::Tout, 0.15)], montee(0.3)].concat();
            c.declencheurs.liste = vec![
                d(E::Kick, A::Inversion, 1.0),
                d(E::Kick, A::Eclat, 0.5),
                sats(E::Kick, 1, 1),
                d(E::Kick, A::Bordure, 0.4),
                d(E::Clic, A::Onde, 0.6),
                d(E::ClicDroit, A::Implosion, 0.6),
            ];
        }),
        // tout à fond : deux anneaux, beaucoup de satellites, fissures et éclats partout
        make("Intense", |c| {
            (c.orbite.rayon, c.orbite.nombre, c.orbite.anneaux, c.orbite.vitesse_base, c.orbite.trainee) = (28.0, 3, 2, 5.0, 0.6);
            (c.essaim.maximum, c.essaim.duree) = (40, 3.5);
            (c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.palette, c.couleur.ecart_palette) = (120.0, 200.0, 2, 180.0);
            (c.fissures.taille, c.fissures.duree, c.particules.taille, c.eclat.taille) = (2.2, 4.0, 1.4, 4.5);
            c.audio.sensibilite_kick = 6.0;
            c.modulations.liste = [
                vec![m(S::Basse, Ci::Vitesse, G::Tout, 0.4), m(S::Grave, Ci::Rayon, G::Tout, 0.45), m(S::Kick, Ci::Taille, G::Tout, 0.5)],
                montee(1.0),
            ]
            .concat();
            c.declencheurs.liste = vec![
                d(E::Kick, A::Inversion, 1.0),
                d(E::Kick, A::Eclat, 1.0),
                sats(E::Kick, 3, 5),
                d(E::Kick, A::Bordure, 1.0),
                Declencheur { probabilite: 0.25, ..d(E::Kick, A::Onde, 0.7) },
                d(E::Caisse, A::Carre, 1.3),
                n(E::Charleston, A::Etincelles, 6),
                n(E::Drop, A::Explosion, 12),
                d(E::Clic, A::Onde, 1.4),
                d(E::Clic, A::Eclats, 1.2),
                d(E::Clic, A::Fissure, 1.0),
                d(E::ClicDroit, A::Implosion, 1.4),
                n(E::Piste, A::Rafale, 10),
            ];
        }),
        // respiration lente : trois anneaux, longues traînées, l'orbite qui gonfle au tempo
        make("Zen", |c| {
            (c.orbite.rayon, c.orbite.anneaux, c.orbite.vitesse_base, c.orbite.trainee) = (18.0, 3, 1.0, 1.2);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (185.0, 6.0, 60.0, false);
            (c.eclat.taille, c.bordure.repos, c.projecteur.actif) = (0.0, 0.4, false);
            c.modulations.liste = vec![
                Modulation { lissage: 1.0, ..m(S::Tempo, Ci::Rayon, G::Tout, 0.03) },
                Modulation { lissage: 1.0, ..m(S::Basse, Ci::Luminosite, G::Tout, 0.35) },
                Modulation { lissage: 1.0, ..m(S::Brillance, Ci::Teinte, G::Tout, 0.15) },
            ];
            c.declencheurs.liste = vec![d(E::Temps, A::Bordure, 0.25), d(E::Drop, A::Onde, 1.6), d(E::Clic, A::Onde, 0.7)];
        }),
        // pour jouer : un réticule et quelques têtes serrées, l'éclat au kick
        make("Viseur", |c| {
            (c.orbite.rayon, c.orbite.nombre, c.orbite.vitesse_base, c.orbite.trainee) = (12.0, 4, 6.0, 0.12);
            c.essaim.maximum = 0;
            (c.reticule.actif, c.reticule.taille, c.reticule.opacite) = (true, 28.0, 0.8);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (355.0, 0.0, 25.0, false);
            (c.eclat.taille, c.particules.taille, c.bordure.active, c.projecteur.actif) = (1.6, 0.5, false, false);
            c.modulations.liste = vec![m(S::Kick, Ci::Rayon, G::Tout, 0.1), m(S::Basse, Ci::Vitesse, G::Tout, 0.1)];
            c.declencheurs.liste = vec![d(E::Kick, A::Eclat, 0.6), d(E::Clic, A::Onde, 0.4)];
        }),
        // l'anneau de spectre autour du pointeur, deux petites comètes à l'intérieur
        make("Spectre", |c| {
            (c.orbite.rayon, c.orbite.vitesse_base, c.orbite.trainee) = (14.0, 2.5, 0.35);
            c.essaim.maximum = 0;
            (c.spectre.actif, c.spectre.bandes, c.spectre.rayon, c.spectre.hauteur) = (true, 24, 34.0, 30.0);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte) = (140.0, 25.0, 120.0);
            c.eclat.taille = 2.4;
            c.modulations.liste = vec![m(S::Basse, Ci::Vitesse, G::Tout, 0.2)];
            c.declencheurs.liste = vec![
                d(E::Kick, A::Eclat, 0.7),
                d(E::Kick, A::Bordure, 0.8),
                n(E::Charleston, A::Etincelles, 3),
                d(E::Drop, A::Onde, 2.0),
                d(E::Clic, A::Onde, 1.0),
                d(E::ClicDroit, A::Implosion, 1.0),
            ];
        }),
        // une seule comète, à longue traînée, qui traîne derrière la souris et laisse de la poussière
        make("Comète", |c| {
            (c.orbite.rayon, c.orbite.nombre, c.orbite.vitesse_base, c.orbite.trainee, c.orbite.inertie) = (32.0, 1, 2.0, 0.9, 0.6);
            c.essaim.maximum = 0;
            (c.sillage.actif, c.sillage.densite) = (true, 8.0);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (28.0, 0.0, 55.0, false);
            c.modulations.liste = [
                vec![m(S::Basse, Ci::Vitesse, G::Tout, 0.08), m(S::Grave, Ci::Rayon, G::Tout, 0.2), m(S::Kick, Ci::Taille, G::Tout, 0.5), m(S::Kick, Ci::Luminosite, G::Tout, 0.5)],
                montee(0.6),
            ]
            .concat();
            c.declencheurs.liste = vec![
                d(E::Kick, A::Eclat, 0.6),
                d(E::Kick, A::Bordure, 1.0),
                d(E::Drop, A::Explosion, 1.0),
                d(E::Clic, A::Onde, 1.0),
                d(E::ClicDroit, A::Implosion, 1.0),
            ];
        }),
        // magenta et vert sur un Lissajous, traînées longues, tout s'allume au kick
        make("Néon", |c| {
            (c.orbite.forme, c.orbite.rayon, c.orbite.nombre, c.orbite.vitesse_base, c.orbite.trainee) = (Forme::Lissajous, 26.0, 3, 1.8, 1.0);
            (c.essaim.maximum, c.essaim.forme) = (12, FS::Cercle);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (305.0, 0.0, 30.0, false);
            (c.couleur.palette, c.couleur.ecart_palette, c.bordure.repos, c.sillage.actif) = (2, 165.0, 0.8, true);
            c.modulations.liste = vec![
                m(S::Basse, Ci::Vitesse, G::Tout, 0.2),
                m(S::Kick, Ci::Luminosite, G::Tout, 0.8),
                m(S::Kick, Ci::Taille, G::Tout, 0.4),
                Modulation { lissage: 0.8, ..m(S::Montee, Ci::Trainee, G::Tout, 0.5) },
            ];
            c.declencheurs.liste = vec![
                d(E::Kick, A::Eclat, 0.8),
                sats(E::Kick, 1, 2),
                d(E::Kick, A::Bordure, 1.0),
                n(E::Drop, A::Explosion, 8),
                d(E::Clic, A::Onde, 1.0),
                d(E::Clic, A::Eclats, 0.8),
                d(E::ClicDroit, A::Implosion, 1.0),
            ];
        }),
        // vert phosphore sur un carré, traînées courtes, carrés sur la caisse claire
        make("Arcade", |c| {
            (c.orbite.forme, c.orbite.rayon, c.orbite.nombre, c.orbite.vitesse_base, c.orbite.trainee) = (Forme::Carre, 22.0, 4, 4.5, 0.12);
            (c.essaim.maximum, c.essaim.duree, c.essaim.forme) = (16, 2.0, FS::Carre);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (125.0, 0.0, 10.0, false);
            (c.particules.taille, c.eclat.taille) = (0.8, 2.6);
            c.modulations.liste = [vec![m(S::Basse, Ci::Vitesse, G::Tout, 0.25), m(S::Tempo, Ci::Taille, G::Tout, 0.3)], montee(1.0)].concat();
            c.declencheurs.liste = vec![
                d(E::Kick, A::Inversion, 1.0),
                d(E::Kick, A::Eclat, 0.8),
                sats(E::Kick, 2, 2),
                d(E::Kick, A::Bordure, 1.0),
                d(E::Caisse, A::Carre, 1.5),
                n(E::Drop, A::Explosion, 6),
                d(E::Clic, A::Carre, 1.2),
                d(E::ClicDroit, A::Implosion, 0.8),
                n(E::Piste, A::Rafale, 6),
            ];
        }),
        // éclairs : astroïde bleu électrique, fissures sur les kicks forts, étincelles aux aigus
        make("Orage", |c| {
            (c.orbite.forme, c.orbite.rayon, c.orbite.nombre, c.orbite.vitesse_base, c.orbite.trainee) = (Forme::Astroide, 26.0, 4, 6.0, 0.15);
            (c.essaim.maximum, c.essaim.duree) = (20, 1.5);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (212.0, 0.0, 20.0, false);
            (c.fissures.taille, c.fissures.duree, c.eclat.taille, c.projecteur.duree) = (2.6, 4.5, 5.0, 2.0);
            c.modulations.liste = [vec![m(S::Basse, Ci::Vitesse, G::Tout, 0.3), m(S::Charleston, Ci::Luminosite, G::Tout, 0.6)], montee(1.0)].concat();
            c.declencheurs.liste = vec![
                d(E::Kick, A::Eclat, 1.0),
                sats(E::Kick, 2, 3),
                d(E::Kick, A::Bordure, 1.0),
                Declencheur { probabilite: 0.2, delai: 2.0, ..d(E::Kick, A::Fissure, 0.8) },
                n(E::Charleston, A::Etincelles, 5),
                d(E::Caisse, A::Eclats, 0.7),
                d(E::Drop, A::Fissure, 2.0),
                n(E::Drop, A::Explosion, 10),
                d(E::Clic, A::Fissure, 1.2),
                d(E::ClicDroit, A::Implosion, 1.2),
            ];
        }),
        // triangle cyan qui tourne à l'envers, satellites sur le même trajet
        make("Delta", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base, c.orbite.rotation) = (Forme::Triangle, 3, 2.4, -6.0);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (178.0, 0.0, 40.0, false);
            c.modulations.liste.push(m(S::Drop, Ci::Rotation, G::Tout, -0.5));
        }),
        // ruche : hexagone ambre, satellites permanents reliés en alvéoles
        make("Ruche", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base, c.orbite.rotation) = (Forme::Hexagone, 6, 1.25, 3.0);
            (c.essaim.fixes, c.essaim.maximum, c.essaim.forme, c.essaim.constellation) = (6, 12, FS::Hexagone, true);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (40.0, 0.0, 25.0, false);
        }),
        // un huit violet qui penche du côté qui joue le plus fort
        make("Huit", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base, c.orbite.inertie) = (Forme::Huit, 2, 1.65, 0.3);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.synchro_prisme) = (265.0, 30.0, false);
            c.modulations.liste.push(Modulation { lissage: 0.6, ..m(S::Stereo, Ci::Decalage, G::Tout, 0.5) });
        }),
        // galaxie : quatre anneaux qui tournent ensemble, satellites sur une ellipse
        make("Spirale", |c| {
            (c.orbite.rayon, c.orbite.nombre, c.orbite.anneaux, c.orbite.vitesse_base, c.orbite.trainee) = (16.0, 2, 4, 2.4, 0.7);
            (c.orbite.rotation, c.essaim.forme, c.essaim.fixes) = (8.0, FS::Ellipse, 4);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.palette, c.couleur.ecart_palette, c.couleur.synchro_prisme) = (220.0, 8.0, 2, 60.0, false);
            c.modulations.liste.push(m(S::Basse, Ci::Rotation, G::Tout, 0.15));
        }),
        // rosace : trois anneaux de fleur reliés par des lignes, palette chaude
        make("Mandala", |c| {
            (c.orbite.forme, c.orbite.rayon, c.orbite.nombre, c.orbite.anneaux, c.orbite.vitesse_base) = (Forme::Fleur, 20.0, 4, 3, 1.4);
            (c.orbite.rotation, c.orbite.trainee) = (4.0, 0.5);
            (c.essaim.maximum, c.essaim.constellation) = (8, true);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.palette, c.couleur.ecart_palette, c.couleur.synchro_prisme) = (330.0, 10.0, 3, 35.0, false);
            c.modulations.liste.push(Modulation { lissage: 0.7, ..m(S::Tempo, Ci::Luminosite, G::Tout, 0.4) });
        }),
        // un cœur rose qui bat au tempo
        make("Battement", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Coeur, 2, 1.5);
            (c.essaim.maximum, c.essaim.forme) = (10, FS::Coeur);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (345.0, 0.0, 30.0, false);
            c.modulations.liste.extend([m(S::Tempo, Ci::Taille, G::Tout, 0.5), Modulation { lissage: 0.0, ..m(S::Tempo, Ci::Rayon, G::Tout, 0.05) }]);
            c.declencheurs.liste.retain(|x| x.faire != A::Inversion);
        }),
        make("Constellation", |c| {
            (c.orbite.nombre, c.orbite.vitesse_base) = (3, 2.0);
            (c.essaim.fixes, c.essaim.maximum, c.essaim.constellation) = (12, 12, true);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.palette, c.couleur.ecart_palette, c.couleur.synchro_prisme) = (200.0, 15.0, 2, 150.0, false);
            c.modulations.liste.push(Modulation { lissage: 0.8, ..m(S::Aigu, Ci::Luminosite, G::Satellites, 0.5) });
        }),
        // pour danser : contretemps, stéréo, ressort, trois teintes
        make("Festival", |c| {
            (c.orbite.nombre, c.orbite.rayon, c.orbite.inertie) = (3, 26.0, 0.4);
            c.essaim.maximum = 30;
            (c.couleur.palette, c.couleur.ecart_palette, c.couleur.vitesse_teinte) = (3, 120.0, 40.0);
            c.modulations.liste.extend([
                Modulation { lissage: 0.0, ..m(S::Contretemps, Ci::Taille, G::Tout, 0.33) },
                Modulation { lissage: 0.4, ..m(S::Contretemps, Ci::Rayon, G::Tout, 0.015) },
                Modulation { lissage: 0.42, ..m(S::Stereo, Ci::Decalage, G::Tout, 0.6) },
            ]);
            if let Some(k) = c.declencheurs.liste.iter_mut().find(|x| x.quand == E::Kick && x.faire == A::Satellites) {
                (k.nombre, k.nombre_max) = (2, 4);
            }
            c.declencheurs.liste.push(d(E::Clic, A::Eclats, 1.0));
        }),
        // le trajet des satellites, à part, s'emballe au drop ; les comètes grossissent au kick
        make("Tourbillon", |c| {
            (c.orbite.forme, c.orbite.rotation, c.orbite.rotation_sur) = (Forme::Etoile, 12.0, G::Satellites);
            (c.essaim.forme, c.essaim.maximum, c.essaim.fixes) = (FS::Cercle, 24, 6);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.palette, c.couleur.ecart_palette, c.couleur.synchro_prisme) = (280.0, 0.0, 2, 60.0, false);
            c.modulations.liste.extend([m(S::Drop, Ci::Rotation, G::Satellites, 0.8), m(S::Kick, Ci::Taille, G::Cometes, 0.6)]);
        }),
        // pas de comètes : des grains qui flottent, de la poussière derrière la souris
        make("Pollen", |c| {
            (c.orbite.cometes, c.orbite.vitesse_base) = (false, 1.4);
            (c.essaim.maximum, c.essaim.fixes) = (6, 16);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (52.0, 0.0, 20.0, false);
            (c.sillage.actif, c.sillage.densite) = (true, 6.0);
            c.declencheurs.liste.retain(|x| x.faire != A::Inversion);
        }),
        // la teinte de la pochette du morceau qui joue, en deux tons sur un trèfle
        make("Pochette", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Trefle, 3, 1.8);
            (c.essaim.forme, c.essaim.maximum) = (FS::Trefle, 18);
            (c.couleur.pochette, c.couleur.vitesse_teinte, c.couleur.palette, c.couleur.ecart_palette) = (true, 0.0, 2, 30.0);
            c.modulations.liste.push(Modulation { lissage: 0.65, ..m(S::Brillance, Ci::Teinte, G::Satellites, 0.15) });
        }),
        // la forme et la couleur de l'icône de l'application au premier plan
        make("Caméléon", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.rayon, c.orbite.vitesse_base) = (Forme::Icone, 2, 28.0, 1.6);
            (c.couleur.application, c.couleur.site, c.couleur.vitesse_teinte, c.couleur.ecart_teinte) = (true, true, 0.0, 50.0);
            c.essaim.maximum = 10;
        }),
    ]
}

/// Modulation `source` → `cible` (lissage par défaut).
fn m(source: crate::config::Source, cible: crate::config::Cible, sur: crate::config::Groupe, quantite: f32) -> Modulation {
    Modulation { source, cible, sur, quantite, ..Default::default() }
}

/// Réactions à la montée, comme par défaut à `force` = 1 : vitesse accélérée, orbite resserrée.
fn montee(force: f32) -> Vec<Modulation> {
    use crate::config::{Cible, Groupe, Source};
    vec![
        Modulation { lissage: 0.0, ..m(Source::Montee, Cible::Acceleration, Groupe::Tout, 0.6 * force) },
        Modulation { lissage: 0.4, ..m(Source::Montee, Cible::Rayon, Groupe::Tout, -0.1125 * force) },
    ]
}

/// Déclencheur `quand` → `faire`, de force `force`.
fn d(quand: crate::config::Evenement, faire: crate::config::Action, force: f32) -> Declencheur {
    Declencheur { force, ..Declencheur::new(quand, faire) }
}

/// Déclencheur qui fait `nombre` satellites ou étincelles.
fn n(quand: crate::config::Evenement, faire: crate::config::Action, nombre: u32) -> Declencheur {
    Declencheur { nombre, nombre_max: nombre, ..Declencheur::new(quand, faire) }
}

/// De `min` à `max` satellites à chaque `quand`.
fn sats(quand: crate::config::Evenement, min: u32, max: u32) -> Declencheur {
    Declencheur { nombre: min, nombre_max: max, ..Declencheur::new(quand, crate::config::Action::Satellites) }
}

/// Presets fournis d'Essaim 0.4 à 0.6, écrits avec les réglages d'avant (`migration::Ancien` :
/// vitesse ajoutée par la basse, satellites par kick, éclats au clic…) : l'essai
/// `presets_comme_avant` vérifie sur eux que ces réglages convertis font toujours la même chose.
#[cfg(test)]
pub(crate) fn presets_d_avant() -> Vec<Profile> {
    let make = |name: &str, change: fn(&mut Config, &mut Ancien)| {
        let (mut cfg, mut a) = (Config::default(), Ancien::default());
        change(&mut cfg, &mut a);
        reagir(&mut cfg, &a);
        Profile { name: name.to_owned(), builtin: true, cfg, date: None }
    };
    let mut l = vec![
        make("Défaut", |_, _| {}),
        make("Discret", |c, a| {
            (c.orbite.rayon, c.orbite.vitesse_base, c.orbite.trainee) = (16.0, 2.0, 0.2);
            (a.orbite.vitesse_basse, a.orbite.eloignement_grave) = (5.0, 0.6);
            (a.essaim.par_kick_min, a.essaim.par_kick_max, c.essaim.maximum, c.essaim.duree) = (1, 2, 8, 2.0);
            (c.couleur.vitesse_teinte, c.couleur.ecart_teinte) = (20.0, 90.0);
            (c.fissures.taille, c.fissures.duree) = (1.0, 2.0);
            (a.bordure.intensite, c.bordure.repos) = (0.5, 0.3);
        }),
        make("Intense", |c, a| {
            (c.orbite.rayon, c.orbite.vitesse_base, c.orbite.trainee) = (28.0, 5.0, 0.6);
            (a.orbite.vitesse_basse, a.orbite.eloignement_grave) = (18.0, 1.8);
            (a.essaim.par_kick_min, a.essaim.par_kick_max, c.essaim.maximum, c.essaim.duree) = (3, 5, 40, 3.5);
            (c.couleur.vitesse_teinte, c.couleur.ecart_teinte) = (120.0, 200.0);
            (c.fissures.taille, c.fissures.duree) = (2.2, 4.0);
            c.audio.sensibilite_kick = 6.0;
            c.orbite.nombre = 3;
        }),
        make("Point seul", |c, a| {
            c.orbite.cometes = false;
            c.essaim.maximum = 0;
            a.fissures.actives = false;
            c.bordure.active = false;
        }),
        // formes : une teinte fixe chacune (la synchro Prisme la remplacerait)
        make("Delta", |c, _| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Triangle, 3, 2.4);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (175.0, 0.0, 40.0, false);
        }),
        make("Ruche", |c, _| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Hexagone, 6, 1.25);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (38.0, 0.0, 30.0, false);
        }),
        make("Huit", |c, _| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Huit, 2, 1.65);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.synchro_prisme) = (265.0, 60.0, false);
        }),
        make("Pollen", |c, _| {
            (c.orbite.cometes, c.orbite.vitesse_base) = (false, 1.4);
            (c.essaim.maximum, c.essaim.fixes) = (0, 14);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (52.0, 0.0, 20.0, false);
        }),
        make("Losange", |c, _| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Losange, 4, 2.1);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (25.0, 0.0, 40.0, false);
        }),
        make("Spirale", |c, _| {
            (c.orbite.anneaux, c.orbite.nombre, c.orbite.vitesse_base) = (3, 1, 2.6);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.synchro_prisme) = (215.0, 60.0, false);
        }),
        // un preset de base pour chacune des autres formes du trajet
        forme("Ellipse", Forme::Ellipse, 2, 2.2, 160.0),
        forme("Carré", Forme::Carre, 4, 1.8, 205.0),
        forme("Pentagone", Forme::Pentagone, 5, 1.6, 95.0),
        forme("Étoile", Forme::Etoile, 2, 1.4, 50.0),
        forme("Fleur", Forme::Fleur, 5, 1.5, 300.0),
        forme("Trèfle", Forme::Trefle, 3, 1.7, 130.0),
        forme("Cœur", Forme::Coeur, 2, 1.5, 350.0),
        forme("Lissajous", Forme::Lissajous, 3, 1.3, 240.0),
        forme("Astroïde", Forme::Astroide, 4, 1.6, 275.0),
        // les réactions à la musique et les couleurs en plus
        make("Constellation", |c, _| {
            (c.orbite.nombre, c.orbite.vitesse_base) = (3, 2.0);
            (c.essaim.fixes, c.essaim.maximum, c.essaim.constellation) = (12, 12, true);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.palette, c.couleur.ecart_palette, c.couleur.synchro_prisme) = (200.0, 15.0, 2, 150.0, false);
        }),
        make("Festival", |c, a| {
            (c.orbite.nombre, c.orbite.rayon, c.orbite.inertie) = (3, 26.0, 0.4);
            (c.essaim.maximum, a.essaim.par_kick_max) = (30, 4);
            (a.musique.contretemps, a.musique.stereo) = (true, 0.6);
            (a.clic.eclats, c.couleur.palette, c.couleur.ecart_palette, c.couleur.vitesse_teinte) = (true, 3, 120.0, 40.0);
        }),
        // modulations : le trajet des satellites, à part, s'emballe au drop ; l'orbite s'ouvre
        // un peu sur les graves (+60 %) ; les comètes grossissent au kick
        make("Tourbillon", |c, a| {
            use crate::config::{FormeSatellites, Groupe};
            (c.orbite.forme, c.orbite.rotation, c.orbite.rotation_sur) = (Forme::Etoile, 12.0, Groupe::Satellites);
            (c.essaim.forme, c.essaim.maximum, c.essaim.fixes) = (FormeSatellites::Cercle, 24, 6);
            a.orbite.eloignement_grave = 0.6;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.palette, c.couleur.ecart_palette, c.couleur.synchro_prisme) = (280.0, 0.0, 2, 60.0, false);
        }),
    ];
    if let Some(p) = l.iter_mut().find(|p| p.name == "Tourbillon") {
        use crate::config::{Cible, Groupe, Modulation, Source};
        let m = |source, cible, sur, quantite| Modulation { source, cible, sur, quantite, ..Default::default() };
        p.cfg.modulations.liste.extend([m(Source::Drop, Cible::Rotation, Groupe::Satellites, 0.8), m(Source::Kick, Cible::Taille, Groupe::Cometes, 0.6)]);
    }
    l
}

/// Réactions d'un preset d'avant, réglées par `a`.
#[cfg(test)]
fn reagir(c: &mut Config, a: &Ancien) {
    c.modulations.liste = a.modulations(true, true);
    c.declencheurs.liste = a.declencheurs();
}

/// Preset de base d'une forme du trajet : `nombre` comètes, vitesse de rotation (radians/s),
/// teinte fixe (la synchro Prisme la remplacerait), sans satellites.
#[cfg(test)]
fn forme(nom: &str, forme: Forme, nombre: u32, vitesse: f32, teinte: f32) -> Profile {
    let mut c = Config::default();
    (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (forme, nombre, vitesse);
    c.essaim.maximum = 0;
    (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (teinte, 0.0, 40.0, false);
    reagir(&mut c, &Ancien::default());
    Profile { name: nom.to_owned(), builtin: true, cfg: c, date: None }
}

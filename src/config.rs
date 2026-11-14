//! Réglages, lus dans `%APPDATA%\Essaim\config.toml` et rechargés dès que le fichier change.

use serde::Deserialize;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub curseur: Curseur,
    pub orbite: Orbite,
    pub essaim: Essaim,
    pub couleur: Couleur,
    pub fissures: Fissures,
    pub audio: Audio,
    pub diagnostic: Diagnostic,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Diagnostic {
    /// Note dans le journal chaque changement de curseur, avec l'appli sous la souris.
    pub curseur: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Curseur {
    pub remplacer_curseurs: bool,
    pub variantes: bool,
    pub taille_point: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Orbite {
    pub rayon: f32,
    pub cometes: bool,
    pub vitesse_base: f32,
    pub vitesse_basse: f32,
    pub eloignement_grave: f32,
    pub trainee: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Essaim {
    pub par_kick_min: u32,
    pub par_kick_max: u32,
    pub maximum: usize,
    pub duree: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Couleur {
    pub vitesse_teinte: f32,
    pub ecart_teinte: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Fissures {
    pub actives: bool,
    pub duree: f32,
    pub taille: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Audio {
    pub actif: bool,
    pub sensibilite_kick: f32,
}

impl Default for Curseur {
    fn default() -> Self {
        Self { remplacer_curseurs: true, variantes: true, taille_point: 3.4 }
    }
}
impl Default for Orbite {
    fn default() -> Self {
        Self { rayon: 22.0, cometes: true, vitesse_base: 3.5, vitesse_basse: 11.0, eloignement_grave: 1.3, trainee: 0.3 }
    }
}
impl Default for Essaim {
    fn default() -> Self {
        Self { par_kick_min: 2, par_kick_max: 3, maximum: 16, duree: 2.8 }
    }
}
impl Default for Couleur {
    fn default() -> Self {
        Self { vitesse_teinte: 60.0, ecart_teinte: 140.0 }
    }
}
impl Default for Fissures {
    fn default() -> Self {
        Self { actives: true, duree: 3.0, taille: 1.4 }
    }
}
impl Default for Audio {
    fn default() -> Self {
        Self { actif: true, sensibilite_kick: 5.0 }
    }
}

const DEFAULT_FILE: &str = r#"# Réglages d'Essaim. Enregistre le fichier : les changements s'appliquent tout de suite.
# Tailles en pixels à 100 % d'échelle (multipliées par l'échelle de l'écran).

[curseur]
remplacer_curseurs = true # la flèche de Windows devient le point (curseur Windows, sans retard)
variantes = true          # et les autres curseurs aussi (texte, lien, chargement, redimensionnement…)
taille_point = 3.4

[orbite]
rayon = 22              # rayon de l'orbite sans basse
cometes = true          # les deux comètes face à face
vitesse_base = 3.5      # vitesse de rotation sans basse (radians/s)
vitesse_basse = 11      # vitesse ajoutée quand la basse est au maximum
eloignement_grave = 1.3 # écartement ajouté pour une note très grave (1.3 = +130 %)
trainee = 0.3           # durée de la traînée des comètes (s)

[essaim]
par_kick_min = 2        # satellites ajoutés à chaque kick
par_kick_max = 3
maximum = 16            # satellites en même temps, au plus
duree = 2.8             # durée de vie d'un satellite (s)

[couleur]
vitesse_teinte = 60     # degrés de teinte par seconde (Prisme)
ecart_teinte = 140      # écart de teinte entre la tête et la queue des traînées

[fissures]
actives = true
duree = 3.0             # secondes avant que la fissure soit refermée
taille = 1.4

[audio]
actif = true
sensibilite_kick = 5    # 1 = peu sensible, 10 = très sensible

[diagnostic]
curseur = false         # note chaque changement de curseur dans essaim.log
"#;

pub fn dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Essaim")
}

pub fn path() -> PathBuf {
    dir().join("config.toml")
}

pub struct Watched {
    pub cfg: Config,
    modified: Option<SystemTime>,
}

impl Watched {
    pub fn load() -> Self {
        let p = path();
        if !p.exists() {
            let _ = std::fs::create_dir_all(dir());
            if let Err(e) = std::fs::write(&p, DEFAULT_FILE) {
                log::warn!("config : écriture de {} impossible : {e}", p.display());
            }
        }
        let mut w = Self { cfg: Config::default(), modified: None };
        w.reload();
        w
    }

    /// Recharge si le fichier a changé. Renvoie vrai si la configuration a été remplacée.
    pub fn reload(&mut self) -> bool {
        let p = path();
        let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
        if modified.is_none() || modified == self.modified {
            return false;
        }
        self.modified = modified;
        match std::fs::read_to_string(&p).map_err(anyhow::Error::from).and_then(|s| Ok(toml::from_str::<Config>(&s)?)) {
            Ok(cfg) => {
                log::info!("config : chargée");
                self.cfg = cfg;
                true
            }
            Err(e) => {
                log::warn!("config : {} illisible, réglages précédents conservés : {e}", p.display());
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn le_fichier_par_defaut_se_lit() {
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        assert_eq!(c.essaim.maximum, 16);
        assert!((c.essaim.duree - 2.8).abs() < 1e-6);
    }
}

//! Réglages, lus dans `%APPDATA%\Essaim\config.toml` et rechargés dès que le fichier change.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub curseur: Curseur,
    pub orbite: Orbite,
    pub essaim: Essaim,
    pub couleur: Couleur,
    pub fissures: Fissures,
    pub audio: Audio,
    pub diagnostic: Diagnostic,
    pub rendu: Rendu,
    pub bordure: Bordure,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Bordure {
    /// La bordure de la fenêtre au premier plan prend la couleur d'Essaim (Windows 11).
    pub active: bool,
    /// Force de l'illumination sur un kick (0 = aucune, 1 = jusqu'à la couleur pleine).
    pub intensite: f32,
    /// Luminosité sans kick (0 = noire, 1 = couleur pleine).
    pub repos: f32,
}

impl Default for Bordure {
    fn default() -> Self {
        Self { active: true, intensite: 1.0, repos: 0.5 }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Rendu {
    /// Images par seconde au plus pour l'essaim (0 = celle de l'écran).
    pub cadence: u32,
    /// Dessin par la carte graphique (sinon par le processeur).
    pub carte_graphique: bool,
    /// Images par seconde quand rien ne bouge (souris immobile, pas de musique) ; 0 = pas de repos.
    pub cadence_repos: u32,
}

impl Default for Rendu {
    fn default() -> Self {
        Self { cadence: 0, carte_graphique: true, cadence_repos: 30 }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Diagnostic {
    /// Note dans le journal chaque changement de curseur, avec l'appli sous la souris.
    pub curseur: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Curseur {
    pub remplacer_curseurs: bool,
    pub variantes: bool,
    pub taille_point: f32,
    /// Suit le réglage « taille du pointeur » de Windows (multiplie la taille du point et des
    /// variantes).
    pub taille_windows: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Orbite {
    pub rayon: f32,
    pub cometes: bool,
    pub vitesse_base: f32,
    pub vitesse_basse: f32,
    pub eloignement_grave: f32,
    pub trainee: f32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Essaim {
    pub par_kick_min: u32,
    pub par_kick_max: u32,
    pub maximum: usize,
    pub duree: f32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Couleur {
    pub vitesse_teinte: f32,
    pub ecart_teinte: f32,
    /// Prend la teinte de l'éclairage de Prisme quand il tourne (sinon la teinte tourne seule).
    pub synchro_prisme: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Fissures {
    pub actives: bool,
    pub duree: f32,
    pub taille: f32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Audio {
    pub actif: bool,
    pub sensibilite_kick: f32,
    /// Délai minimal entre deux kicks (s) : un kick long qui ondule n'est compté qu'une fois.
    pub delai_kick: f32,
}

impl Default for Curseur {
    fn default() -> Self {
        Self { remplacer_curseurs: true, variantes: true, taille_point: 3.4, taille_windows: true }
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
        Self { vitesse_teinte: 60.0, ecart_teinte: 140.0, synchro_prisme: true }
    }
}
impl Default for Fissures {
    fn default() -> Self {
        Self { actives: true, duree: 3.0, taille: 1.4 }
    }
}
impl Default for Audio {
    fn default() -> Self {
        Self { actif: true, sensibilite_kick: 5.0, delai_kick: 0.1 }
    }
}

const DEFAULT_FILE: &str = r#"# Réglages d'Essaim. Enregistre le fichier : les changements s'appliquent tout de suite.
# Tailles en pixels à 100 % d'échelle (multipliées par l'échelle de l'écran).

[curseur]
remplacer_curseurs = true # la flèche de Windows devient le point (curseur Windows, sans retard)
variantes = true          # et les autres curseurs aussi (texte, lien, chargement, redimensionnement…)
taille_point = 3.4
taille_windows = true     # suit le réglage « taille du pointeur » de Windows

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
synchro_prisme = true   # prend la teinte de l'éclairage de Prisme quand il tourne

[fissures]
actives = true
duree = 3.0             # secondes avant que la fissure soit refermée
taille = 1.4

[audio]
actif = true
sensibilite_kick = 5    # 1 = peu sensible, 10 = très sensible
delai_kick = 0.1        # délai minimal entre deux kicks (s) ; plus long si un kick long compte double

[rendu]
cadence = 0             # images par seconde de l'essaim ; 0 = celle de l'écran (144…), 72 = deux fois moins de calcul
carte_graphique = true  # dessin par la carte graphique ; false = par le processeur
cadence_repos = 30      # images par seconde quand la souris est immobile et sans musique ; 0 = jamais

[bordure]
active = true           # la bordure de la fenêtre au premier plan prend la couleur d'Essaim
intensite = 1.0         # illumination sur chaque kick : 0 = aucune, 1 = jusqu'à la couleur pleine
repos = 0.5             # luminosité sans kick : 0 = noire, 1 = couleur pleine

[diagnostic]
curseur = false         # note chaque changement de curseur dans essaim.log
"#;

pub fn dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Essaim")
}

pub fn path() -> PathBuf {
    dir().join("config.toml")
}

/// Écrit `[section] key = value` dans `config.toml` en gardant commentaires et mise en page
/// (le fichier est ensuite rechargé comme s'il avait été modifié à la main). Refuse une section
/// ou une clé inconnue et une valeur du mauvais type.
pub fn set_value(section: &str, key: &str, value: &serde_json::Value) -> Result<(), String> {
    set_value_in(&path(), section, key, value)
}

/// Bornes des réglages numériques : une valeur absurde (fichier mal tapé, client local) ne doit
/// pas pouvoir surcharger l'appli (des milliers de satellites, une cadence folle…).
fn limits(section: &str, key: &str) -> (f64, f64) {
    match (section, key) {
        ("curseur", "taille_point") => (1.0, 20.0),
        ("orbite", "rayon") => (4.0, 200.0),
        ("orbite", "trainee") => (0.01, 3.0),
        ("orbite", _) => (0.0, 100.0),
        ("essaim", "maximum") => (0.0, 64.0),
        ("essaim", "duree") => (0.1, 30.0),
        ("essaim", _) => (0.0, 10.0),
        ("couleur", _) => (0.0, 720.0),
        ("fissures", "duree") => (0.2, 30.0),
        ("fissures", "taille") => (0.2, 6.0),
        ("audio", "sensibilite_kick") => (1.0, 10.0),
        ("audio", "delai_kick") => (0.05, 1.0),
        ("rendu", _) => (0.0, 500.0),
        ("bordure", _) => (0.0, 1.0),
        _ => (0.0, 1000.0),
    }
}

impl Config {
    /// Ramène chaque réglage numérique dans ses bornes.
    fn clamped(self) -> Config {
        let Ok(mut v) = serde_json::to_value(&self) else { return self };
        if let Some(sections) = v.as_object_mut() {
            for (section, keys) in sections.iter_mut() {
                for (key, x) in keys.as_object_mut().into_iter().flatten() {
                    if let Some(f) = x.as_f64() {
                        let (lo, hi) = limits(section, key);
                        let c = f.clamp(lo, hi);
                        if c != f {
                            log::warn!("config : [{section}] {key} = {f} hors bornes, {c} utilisé");
                            *x = if x.is_f64() { c.into() } else { (c as i64).into() };
                        }
                    }
                }
            }
        }
        serde_json::from_value(v).unwrap_or(self)
    }
}

fn set_value_in(p: &std::path::Path, section: &str, key: &str, value: &serde_json::Value) -> Result<(), String> {
    if let Some(f) = value.as_f64() {
        let (lo, hi) = limits(section, key);
        if !(lo..=hi).contains(&f) {
            return Err(format!("[{section}] {key} : {f} hors bornes ({lo} à {hi})"));
        }
    }
    let text = std::fs::read_to_string(p).unwrap_or_else(|_| DEFAULT_FILE.to_owned());
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("config.toml illisible : {e}"))?;
    let known = serde_json::to_value(Config::default()).map_err(|e| e.to_string())?;
    let Some(old) = known.get(section).and_then(|s| s.get(key)) else {
        return Err(format!("réglage inconnu : [{section}] {key}"));
    };
    let v: toml_edit::Value = match (old, value) {
        (serde_json::Value::Bool(_), serde_json::Value::Bool(b)) => (*b).into(),
        (serde_json::Value::Number(o), serde_json::Value::Number(n)) if o.is_f64() => n.as_f64().unwrap_or(0.0).into(),
        (serde_json::Value::Number(_), serde_json::Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)).unwrap_or(0).into(),
        _ => return Err(format!("[{section}] {key} : valeur du mauvais type")),
    };
    if doc.get(section).is_none() {
        doc[section] = toml_edit::table();
    }
    // garde le commentaire de fin de ligne de l'ancienne valeur
    let decor = doc[section].get(key).and_then(|i| i.as_value()).map(|v| v.decor().clone());
    let mut v = v;
    if let Some(d) = decor {
        *v.decor_mut() = d;
    }
    doc[section][key] = toml_edit::Item::Value(v);
    let new = doc.to_string();
    toml::from_str::<Config>(&new).map_err(|e| format!("valeur refusée : {e}"))?;
    std::fs::write(p, new).map_err(|e| format!("écriture de {} impossible : {e}", p.display()))
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
                self.cfg = cfg.clamped();
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
        assert!(c.bordure.active);
        assert!((c.bordure.repos - super::Bordure::default().repos).abs() < 1e-6);
    }

    #[test]
    fn ecrire_un_reglage_garde_les_commentaires() {
        let p = std::env::temp_dir().join(format!("essaim-test-{}.toml", std::process::id()));
        std::fs::write(&p, super::DEFAULT_FILE).unwrap();
        super::set_value_in(&p, "orbite", "rayon", &serde_json::json!(30)).unwrap();
        super::set_value_in(&p, "essaim", "maximum", &serde_json::json!(20)).unwrap();
        super::set_value_in(&p, "couleur", "synchro_prisme", &serde_json::json!(false)).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        let c: super::Config = toml::from_str(&text).unwrap();
        assert!((c.orbite.rayon - 30.0).abs() < 1e-6);
        assert_eq!(c.essaim.maximum, 20);
        assert!(!c.couleur.synchro_prisme);
        assert!(text.contains("# rayon de l'orbite sans basse"), "commentaire perdu :\n{text}");
        // refus : clé inconnue, mauvais type, hors bornes
        assert!(super::set_value_in(&p, "orbite", "inconnu", &serde_json::json!(1)).is_err());
        assert!(super::set_value_in(&p, "orbite", "rayon", &serde_json::json!(true)).is_err());
        assert!(super::set_value_in(&p, "essaim", "maximum", &serde_json::json!(100000)).is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn valeurs_absurdes_ramenees_dans_les_bornes() {
        let c: super::Config = toml::from_str("[essaim]\nmaximum = 100000\n[rendu]\ncadence = 99999").unwrap();
        let c = c.clamped();
        assert_eq!(c.essaim.maximum, 64);
        assert_eq!(c.rendu.cadence, 500);
        let c: super::Config = toml::from_str("[bordure]\nintensite = 3\nrepos = -1").unwrap();
        let c = c.clamped();
        assert_eq!((c.bordure.intensite, c.bordure.repos), (1.0, 0.0));
    }
}

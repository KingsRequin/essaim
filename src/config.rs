//! Réglages, lus dans `%APPDATA%\Essaim\config.toml` et rechargés dès que le fichier change.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub eclat: Eclat,
    pub orbite: Orbite,
    pub essaim: Essaim,
    pub couleur: Couleur,
    pub fissures: Fissures,
    pub scintillement: Scintillement,
    pub clic: Clic,
    pub sillage: Sillage,
    pub audio: Audio,
    pub rendu: Rendu,
    pub bordure: Bordure,
    pub souris: Souris,
    pub automatique: Automatique,
    pub raccourcis: Raccourcis,
    pub bibliotheque: Bibliotheque,
    pub mise_a_jour: MiseAJour,
}

/// Étincelles sur les attaques des aigus (charleston, cymbales).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Scintillement {
    pub actif: bool,
    /// Étincelles par attaque (1 à 10).
    pub quantite: f32,
}

impl Default for Scintillement {
    fn default() -> Self {
        Self { actif: true, quantite: 4.0 }
    }
}

/// Effets d'un clic, en plus des fissures.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Clic {
    /// Onde de choc : un anneau qui s'élargit.
    pub onde: bool,
    /// Éclats projetés autour du clic.
    pub eclats: bool,
    pub taille: f32,
}

impl Default for Clic {
    fn default() -> Self {
        Self { onde: true, eclats: false, taille: 1.0 }
    }
}

/// Sillage de poussière derrière la souris quand elle file vite.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Sillage {
    pub actif: bool,
    /// Densité de la poussière (1 à 10).
    pub densite: f32,
}

impl Default for Sillage {
    fn default() -> Self {
        Self { actif: false, densite: 5.0 }
    }
}

/// Preset choisi tout seul selon l'application au premier plan ou l'heure.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Automatique {
    pub actif: bool,
    /// Application (nom de l'exécutable) → preset ; prioritaire sur les horaires.
    pub applis: Vec<RegleAppli>,
    /// Plage horaire (« 22:00 » à « 07:00 », peut passer minuit) → preset.
    pub horaires: Vec<Horaire>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct RegleAppli {
    pub appli: String,
    pub preset: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Horaire {
    pub debut: String,
    pub fin: String,
    pub preset: String,
}

/// Raccourcis clavier globaux (RegisterHotKey, pas de hook).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Raccourcis {
    /// Ctrl+Alt+→ / ← : preset suivant / précédent ; Ctrl+Alt+P : pause.
    pub actif: bool,
}

impl Default for Raccourcis {
    fn default() -> Self {
        Self { actif: true }
    }
}

/// Bibliothèque de presets partagés, hébergée sur GitHub.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Bibliotheque {
    /// Dépôt GitHub `propriétaire/nom` : `index.json` à sa racine (branche `main`).
    pub depot: String,
}

impl Default for Bibliotheque {
    fn default() -> Self {
        Self { depot: "KingsRequin/essaim-presets".into() }
    }
}

/// Mises à jour depuis les versions publiées sur GitHub (`maj.rs`).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct MiseAJour {
    /// Cherche une nouvelle version, la télécharge et prévient ; elle s'applique au redémarrage.
    pub auto: bool,
}

impl Default for MiseAJour {
    fn default() -> Self {
        Self { auto: true }
    }
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

/// Éclat du kick, dessiné sous le pointeur (le pointeur lui-même est celui de Windows).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Eclat {
    /// Taille de l'éclat (0 = aucun).
    pub taille: f32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Orbite {
    pub rayon: f32,
    pub cometes: bool,
    /// Trajet des comètes.
    pub forme: Forme,
    /// Comètes par anneau, réparties sur le trajet.
    pub nombre: u32,
    /// Anneaux de comètes emboîtés (les extérieurs, plus larges, tournent moins vite).
    pub anneaux: u32,
    /// Rotation du trajet lui-même (tours par minute ; négatif : dans l'autre sens).
    pub rotation: f32,
    /// Trajet dessiné à la main (`forme = "trace"`) : points (x, y), rayon 1 environ.
    pub trace: Vec<[f32; 2]>,
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
    /// Satellites permanents, présents même sans musique.
    pub fixes: u32,
    /// Trajet des satellites ; par défaut celui des comètes.
    pub forme: FormeSatellites,
}

/// Trajet des satellites : celui des comètes, ou une forme à eux.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FormeSatellites {
    #[default]
    Cometes,
    Cercle,
    Triangle,
    Losange,
    Hexagone,
    Huit,
    Etoile,
    Fleur,
    Carre,
    Pentagone,
    Ellipse,
    Coeur,
    Trefle,
    Lissajous,
    Astroide,
    Trace,
}

impl FormeSatellites {
    /// La forme suivie, `cometes` étant celle des comètes.
    pub fn suivie(self, cometes: Forme) -> Forme {
        match self {
            Self::Cometes => cometes,
            Self::Cercle => Forme::Cercle,
            Self::Triangle => Forme::Triangle,
            Self::Losange => Forme::Losange,
            Self::Hexagone => Forme::Hexagone,
            Self::Huit => Forme::Huit,
            Self::Etoile => Forme::Etoile,
            Self::Fleur => Forme::Fleur,
            Self::Carre => Forme::Carre,
            Self::Pentagone => Forme::Pentagone,
            Self::Ellipse => Forme::Ellipse,
            Self::Coeur => Forme::Coeur,
            Self::Trefle => Forme::Trefle,
            Self::Lissajous => Forme::Lissajous,
            Self::Astroide => Forme::Astroide,
            Self::Trace => Forme::Trace,
        }
    }
}

/// Trajet des comètes autour du pointeur.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Forme {
    #[default]
    Cercle,
    Triangle,
    Losange,
    Hexagone,
    Huit,
    Etoile,
    Fleur,
    Carre,
    Pentagone,
    Ellipse,
    Coeur,
    Trefle,
    Lissajous,
    Astroide,
    /// Dessiné à la main (`[orbite] trace`).
    Trace,
}

/// Jeu de curseurs de Windows.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Souris {
    /// Jeu de curseurs des réglages (nom d'un jeu de Windows ; vide = ne pas y toucher).
    pub jeu: String,
    /// Choisir un preset installe aussi son jeu de curseurs.
    pub avec_preset: bool,
}

impl Default for Souris {
    fn default() -> Self {
        Self { jeu: String::new(), avec_preset: true }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Couleur {
    /// Teinte de départ (degrés) : la couleur fixe quand `vitesse_teinte` vaut 0.
    pub teinte: f32,
    pub vitesse_teinte: f32,
    pub ecart_teinte: f32,
    /// Prend la teinte de l'éclairage de Prisme quand il tourne (sinon la teinte tourne seule).
    pub synchro_prisme: bool,
    /// Prend la teinte dominante de l'icône de l'application au premier plan (prioritaire).
    pub application: bool,
    /// Dans un navigateur, prend la couleur dominante du haut de la page affichée.
    pub site: bool,
    /// La teinte glisse avec la musique : plus elle est forte et brillante, plus elle s'écarte.
    pub musique: bool,
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

impl Default for Eclat {
    fn default() -> Self {
        Self { taille: 3.4 }
    }
}
impl Default for Orbite {
    fn default() -> Self {
        Self { rayon: 22.0, cometes: true, forme: Forme::Cercle, nombre: 2, anneaux: 1, rotation: 0.0, trace: Vec::new(), vitesse_base: 3.5, vitesse_basse: 11.0, eloignement_grave: 1.3, trainee: 0.3 }
    }
}
impl Default for Essaim {
    fn default() -> Self {
        Self { par_kick_min: 2, par_kick_max: 3, maximum: 16, duree: 2.8, fixes: 0, forme: FormeSatellites::Cometes }
    }
}
impl Default for Couleur {
    fn default() -> Self {
        Self { teinte: 0.0, vitesse_teinte: 60.0, ecart_teinte: 140.0, synchro_prisme: true, application: false, site: false, musique: false }
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

[eclat]
taille = 3.4            # éclat du kick sous le pointeur ; 0 = aucun

[orbite]
rayon = 22              # rayon de l'orbite sans basse
cometes = true          # comètes en orbite
forme = "cercle"        # trajet : cercle, triangle, losange, hexagone, huit, etoile, fleur, carre,
                        # pentagone, ellipse, coeur, trefle, lissajous ou astroide
nombre = 2              # comètes par anneau
anneaux = 1             # anneaux emboîtés (les extérieurs tournent moins vite)
rotation = 0            # rotation du trajet lui-même (tours/min ; négatif : autre sens)
trace = []              # trajet dessiné à la main (forme = "trace"), points [x, y]
vitesse_base = 3.5      # vitesse de rotation sans basse (radians/s)
vitesse_basse = 11      # vitesse ajoutée quand la basse est au maximum
eloignement_grave = 1.3 # écartement ajouté pour une note très grave (1.3 = +130 %)
trainee = 0.3           # durée de la traînée des comètes (s)

[essaim]
par_kick_min = 2        # satellites ajoutés à chaque kick
par_kick_max = 3
maximum = 16            # satellites en même temps, au plus
duree = 2.8             # durée de vie d'un satellite (s)
fixes = 0               # satellites permanents, présents même sans musique
forme = "cometes"       # trajet des satellites : "cometes" (le même), ou une forme du trajet des comètes

[couleur]
teinte = 0              # teinte de départ (0 rouge, 120 vert, 240 bleu) ; fixe si vitesse_teinte = 0
vitesse_teinte = 60     # degrés de teinte par seconde
ecart_teinte = 140      # écart de teinte entre la tête et la queue des traînées
synchro_prisme = true   # prend la teinte de l'éclairage de Prisme quand il tourne
application = false     # prend la teinte de l'icône de l'application au premier plan
site = false            # dans un navigateur, prend la couleur du haut de la page affichée
musique = false         # la teinte glisse avec l'intensité et la brillance de la musique

[fissures]
actives = true
duree = 3.0             # secondes avant que la fissure soit refermée
taille = 1.4

[scintillement]
actif = true            # étincelles sur les attaques des aigus (charleston, cymbales)
quantite = 4            # étincelles par attaque (1 à 10)

[clic]
onde = true             # onde de choc à chaque clic
eclats = false          # éclats projetés autour du clic
taille = 1.0

[sillage]
actif = false           # poussière derrière la souris quand elle file vite
densite = 5             # 1 à 10

[audio]
actif = true
sensibilite_kick = 5    # 1 = peu sensible, 10 = très sensible
delai_kick = 0.1        # délai minimal entre deux kicks (s) ; plus long si un kick long compte double

[rendu]
cadence = 0             # images par seconde de l'essaim ; 0 = celle de l'écran (144…), 72 = deux fois moins de calcul
carte_graphique = true  # dessin par la carte graphique ; false = par le processeur
cadence_repos = 30      # images par seconde quand la souris est immobile et sans musique ; 0 = jamais

[souris]
jeu = ""                # jeu de curseurs de Windows des réglages ("" = ne pas y toucher)
avec_preset = true      # choisir un preset installe aussi son jeu de curseurs

[automatique]
actif = false           # choisit tout seul un preset selon l'application au premier plan ou l'heure
applis = []             # [{ appli = "jeu.exe", preset = "Discret" }]
horaires = []           # [{ debut = "22:00", fin = "07:00", preset = "Discret" }]

[raccourcis]
actif = true            # Ctrl+Alt+→ / ← : preset suivant / précédent ; Ctrl+Alt+P : pause

[bibliotheque]
depot = "KingsRequin/essaim-presets" # dépôt GitHub des presets partagés (index.json à la racine)

[mise_a_jour]
auto = true             # cherche les nouvelles versions sur GitHub, les télécharge et prévient

[bordure]
active = true           # la bordure de la fenêtre au premier plan prend la couleur d'Essaim
intensite = 1.0         # illumination sur chaque kick : 0 = aucune, 1 = jusqu'à la couleur pleine
repos = 0.5             # luminosité sans kick : 0 = noire, 1 = couleur pleine
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
        ("eclat", "taille") => (0.0, 20.0),
        ("orbite", "rayon") => (4.0, 200.0),
        ("orbite", "trainee") => (0.01, 3.0),
        ("orbite", "nombre") => (1.0, 8.0),
        ("orbite", "anneaux") => (1.0, 4.0),
        ("orbite", "rotation") => (-60.0, 60.0),
        ("orbite", _) => (0.0, 100.0),
        ("essaim", "fixes") => (0.0, 32.0),
        ("scintillement", "quantite") => (1.0, 10.0),
        ("clic", "taille") => (0.3, 3.0),
        ("sillage", "densite") => (1.0, 10.0),
        ("essaim", "maximum") => (0.0, 64.0),
        ("essaim", "duree") => (0.1, 30.0),
        ("essaim", _) => (0.0, 10.0),
        ("couleur", "teinte") => (0.0, 360.0),
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
    /// Lit un fichier de réglages (manquants : valeurs par défaut), bornes appliquées.
    pub fn read(p: &std::path::Path) -> anyhow::Result<Config> {
        Ok(toml::from_str::<Config>(&std::fs::read_to_string(p)?)?.clamped())
    }

    /// Ramène chaque réglage numérique dans ses bornes.
    pub fn clamped(mut self) -> Config {
        // tracé : 2 000 points au plus, sans valeur non finie ni démesurée (avant le passage en
        // JSON, où un point non fini empêcherait toute la relecture)
        self.orbite.trace.retain(|p| p.iter().all(|v| v.is_finite() && v.abs() < 1e6));
        self.orbite.trace.truncate(2000);
        let Ok(mut v) = serde_json::to_value(&self) else { return self };
        let defaut = serde_json::to_value(Config::default()).unwrap_or_default();
        if let Some(sections) = v.as_object_mut() {
            for (section, keys) in sections.iter_mut() {
                for (key, x) in keys.as_object_mut().into_iter().flatten() {
                    // nan et inf deviennent `null` en JSON : la borne basse
                    if x.is_null() && defaut.get(section.as_str()).and_then(|s| s.get(key.as_str())).is_some_and(|d| d.is_number()) {
                        let (lo, _) = limits(section, key);
                        log::warn!("config : [{section}] {key} n'est pas un nombre fini, {lo} utilisé");
                        *x = lo.into();
                        continue;
                    }
                    if let Some(f) = x.as_f64() {
                        let (lo, hi) = limits(section, key);
                        // nan ou inf (écrits à la main) : la borne basse
                        let c = if f.is_finite() { f.clamp(lo, hi) } else { lo };
                        if c != f {
                            log::warn!("config : [{section}] {key} = {f} hors bornes, {c} utilisé");
                            *x = if x.is_f64() { c.into() } else { (c as i64).into() };
                        }
                    }
                }
            }
        }
        // relecture impossible (ne devrait pas arriver) : les réglages par défaut, pas des
        // valeurs non bornées
        serde_json::from_value(v).unwrap_or_else(|e| {
            log::warn!("config : bornes impossibles à appliquer ({e}), réglages par défaut");
            Config::default()
        })
    }
}

pub(crate) fn set_value_in(p: &std::path::Path, section: &str, key: &str, value: &serde_json::Value) -> Result<(), String> {
    edit_file(p, |doc, known| set_in_doc(doc, known, section, key, value))
}

/// Écrit d'un coup les valeurs de `cfg` retenues par `keep(section, clé)` (application d'un
/// profil), en gardant commentaires et mise en page.
pub fn apply_keys(cfg: &Config, keep: fn(&str, &str) -> bool) -> Result<(), String> {
    apply_keys_in(&path(), cfg, keep)
}

pub(crate) fn apply_keys_in(p: &std::path::Path, cfg: &Config, keep: fn(&str, &str) -> bool) -> Result<(), String> {
    let values = serde_json::to_value(cfg.clone().clamped()).map_err(|e| e.to_string())?;
    edit_file(p, |doc, known| {
        for (section, keys) in values.as_object().into_iter().flatten() {
            for (key, v) in keys.as_object().into_iter().flatten().filter(|(k, _)| keep(section, k)) {
                set_in_doc(doc, known, section, key, v)?;
            }
        }
        Ok(())
    })
}

/// Lit `config.toml` (ou le fichier par défaut), applique `change`, vérifie que le résultat se
/// lit, puis l'écrit.
fn edit_file(
    p: &std::path::Path,
    change: impl FnOnce(&mut toml_edit::DocumentMut, &serde_json::Value) -> Result<(), String>,
) -> Result<(), String> {
    let text = std::fs::read_to_string(p).unwrap_or_else(|_| DEFAULT_FILE.to_owned());
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("config.toml illisible : {e}"))?;
    let known = serde_json::to_value(Config::default()).map_err(|e| e.to_string())?;
    change(&mut doc, &known)?;
    let new = doc.to_string();
    if new == text {
        return Ok(());
    }
    toml::from_str::<Config>(&new).map_err(|e| format!("valeur refusée : {e}"))?;
    std::fs::write(p, new).map_err(|e| format!("écriture de {} impossible : {e}", p.display()))
}

fn set_in_doc(
    doc: &mut toml_edit::DocumentMut,
    known: &serde_json::Value,
    section: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    if let Some(f) = value.as_f64() {
        let (lo, hi) = limits(section, key);
        if !(lo..=hi).contains(&f) {
            return Err(format!("[{section}] {key} : {f} hors bornes ({lo} à {hi})"));
        }
    }
    let Some(old) = known.get(section).and_then(|s| s.get(key)) else {
        return Err(format!("réglage inconnu : [{section}] {key}"));
    };
    let mut v: toml_edit::Value = match (old, value) {
        (serde_json::Value::Bool(_), serde_json::Value::Bool(b)) => (*b).into(),
        (serde_json::Value::String(_), serde_json::Value::String(v)) => v.as_str().into(),
        // 4 décimales : un f32 élargi (3.4000000953…) s'écrit 3.4
        (serde_json::Value::Number(o), serde_json::Value::Number(n)) if o.is_f64() => {
            ((n.as_f64().unwrap_or(0.0) * 1e4).round() / 1e4).into()
        }
        (serde_json::Value::Number(_), serde_json::Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)).unwrap_or(0).into(),
        // tableaux (tracé, règles) : la lecture de la config qui suit vérifie leur forme
        (serde_json::Value::Array(_), serde_json::Value::Array(_)) => vers_toml(value).ok_or(format!("[{section}] {key} : valeur illisible"))?,
        _ => return Err(format!("[{section}] {key} : valeur du mauvais type")),
    };
    if doc.get(section).is_none() {
        doc[section] = toml_edit::table();
    }
    // même valeur : rien à changer (le fichier n'est pas réécrit)
    if let Some(cur) = doc[section].get(key).and_then(|i| i.as_value()) {
        if same(cur, &v) {
            return Ok(());
        }
        // garde le commentaire de fin de ligne de l'ancienne valeur
        *v.decor_mut() = cur.decor().clone();
    }
    doc[section][key] = toml_edit::Item::Value(v);
    Ok(())
}

/// Valeur JSON → valeur TOML (tableaux et tables en ligne compris), nombres à 4 décimales.
fn vers_toml(v: &serde_json::Value) -> Option<toml_edit::Value> {
    Some(match v {
        serde_json::Value::Bool(b) => (*b).into(),
        serde_json::Value::String(s) => s.as_str().into(),
        serde_json::Value::Number(n) if n.is_f64() => ((n.as_f64()? * 1e4).round() / 1e4).into(),
        serde_json::Value::Number(n) => n.as_i64()?.into(),
        serde_json::Value::Array(a) => {
            let mut t = toml_edit::Array::new();
            for x in a {
                t.push(vers_toml(x)?);
            }
            toml_edit::Value::Array(t)
        }
        serde_json::Value::Object(o) => {
            let mut t = toml_edit::InlineTable::new();
            for (k, x) in o {
                t.insert(k, vers_toml(x)?);
            }
            toml_edit::Value::InlineTable(t)
        }
        serde_json::Value::Null => return None,
    })
}

fn same(a: &toml_edit::Value, b: &toml_edit::Value) -> bool {
    if let (Some(x), Some(y)) = (a.as_bool(), b.as_bool()) {
        return x == y;
    }
    if let (Some(x), Some(y)) = (a.as_str(), b.as_str()) {
        return x == y;
    }
    let num = |v: &toml_edit::Value| v.as_float().or(v.as_integer().map(|i| i as f64));
    matches!((num(a), num(b)), (Some(x), Some(y)) if (x - y).abs() < 1e-9)
}

/// Demande à la boucle principale de relire `config.toml` sans attendre son tour (réglages
/// changés depuis la fenêtre : effet immédiat).
static RELOAD_NOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn request_reload() {
    RELOAD_NOW.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn take_reload_request() -> bool {
    RELOAD_NOW.swap(false, std::sync::atomic::Ordering::Relaxed)
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
        match Config::read(&p) {
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
        // texte : une forme connue passe, une inconnue est refusée
        super::set_value_in(&p, "orbite", "forme", &serde_json::json!("huit")).unwrap();
        assert!(super::set_value_in(&p, "orbite", "forme", &serde_json::json!("rond")).is_err());
        let c: super::Config = toml::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(c.orbite.forme, super::Forme::Huit);
        // tableaux : un tracé et des règles
        super::set_value_in(&p, "orbite", "trace", &serde_json::json!([[0.5, -1.0], [1.0, 0.25], [-0.75, 0.5]])).unwrap();
        super::set_value_in(&p, "automatique", "applis", &serde_json::json!([{ "appli": "jeu.exe", "preset": "Discret" }])).unwrap();
        assert!(super::set_value_in(&p, "automatique", "applis", &serde_json::json!([{ "appli": 3 }])).is_err());
        let c: super::Config = toml::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(c.orbite.trace, vec![[0.5, -1.0], [1.0, 0.25], [-0.75, 0.5]]);
        assert_eq!(c.automatique.applis[0].preset, "Discret");
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
        // nan, inf et points non finis du tracé : bornés ou écartés
        let texte = "[scintillement]\nquantite = nan\n[clic]\ntaille = inf\n[orbite]\ntrace = [[1.0, nan], [2.0, 3.0]]";
        let c = toml::from_str::<super::Config>(texte).unwrap().clamped();
        assert_eq!((c.scintillement.quantite, c.clic.taille), (1.0, 0.3));
        assert_eq!(c.orbite.trace, vec![[2.0, 3.0]]);
    }
}

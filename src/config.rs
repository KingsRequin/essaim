//! Réglages, lus dans `%APPDATA%\Essaim\config.toml` et rechargés dès que le fichier change.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub eclat: Eclat,
    pub orbite: Orbite,
    pub essaim: Essaim,
    pub couleur: Couleur,
    pub fissures: Fissures,
    pub particules: Particules,
    pub sillage: Sillage,
    pub projecteur: Projecteur,
    pub reticule: Reticule,
    pub spectre: Spectre,
    pub rubans: Rubans,
    pub ressorts: Ressorts,
    pub arcs: Arcs,
    pub quatre_temps: QuatreTemps,
    pub anneau: Anneau,
    pub orbe: Orbe,
    pub voix: Voix,
    pub trame: Trame,
    pub pinceau: Pinceau,
    pub pendule: Pendule,
    pub gouttes: Gouttes,
    pub equerres: Equerres,
    pub halo: Halo,
    pub lisere: Lisere,
    pub etiquette: Etiquette,
    pub audio: Audio,
    pub rendu: Rendu,
    pub bordure: Bordure,
    pub souris: Souris,
    pub automatique: Automatique,
    pub raccourcis: Raccourcis,
    pub bibliotheque: Bibliotheque,
    pub mise_a_jour: MiseAJour,
    pub pause: Pause,
    pub stream: Stream,
    pub modulations: Modulations,
    pub declencheurs: Declencheurs,
}

/// Fenêtre pour le stream (`stream.rs`), propre à la machine : hors presets.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Stream {
    /// Fond de la fenêtre : transparent (OBS « Capture de jeu » avec la transparence), ou uni
    /// (« Capture de fenêtre » et incrustation chroma).
    pub fond: Fond,
}

/// Fond de la fenêtre pour le stream.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fond {
    #[default]
    Transparent,
    Vert,
    Bleu,
    Magenta,
    Noir,
}

impl Orbite {
    /// Contours du trajet « image » : ceux d'une image, sinon le trajet dessiné d'avant.
    pub fn chemins(&self) -> Vec<Vec<[f32; 2]>> {
        if !self.contours.is_empty() {
            self.contours.clone()
        } else if self.trace.len() >= 3 {
            vec![self.trace.clone()]
        } else {
            Vec::new()
        }
    }
}

impl Config {
    /// La figure `i` (dans l'ordre de `FIGURES`) : est-elle active, et son allure.
    pub fn figure(&self, i: usize) -> (bool, &Allure) {
        match i {
            0 => (self.arcs.actif, &self.arcs.allure),
            1 => (self.quatre_temps.actif, &self.quatre_temps.allure),
            2 => (self.anneau.actif, &self.anneau.allure),
            3 => (self.orbe.actif, &self.orbe.allure),
            4 => (self.voix.actif, &self.voix.allure),
            5 => (self.trame.actif, &self.trame.allure),
            6 => (self.pinceau.actif, &self.pinceau.allure),
            7 => (self.pendule.actif, &self.pendule.allure),
            _ => (self.gouttes.actif, &self.gouttes.allure),
        }
    }

    /// Le trajet suit-il l'icône de l'application au premier plan (comètes ou satellites) ?
    pub fn suit_icone(&self) -> bool {
        self.orbite.forme == Forme::Icone || self.essaim.forme == FormeSatellites::Icone
    }

    /// Une modulation ou un déclencheur demande-t-il l'analyse musicale (caisse claire, montée,
    /// drop) ?
    pub fn analyse_musicale(&self) -> bool {
        self.modulations.liste.iter().any(|m| matches!(m.source, Source::Caisse | Source::Montee | Source::Drop))
            || self.declencheurs.liste.iter().any(|d| matches!(d.quand, Evenement::Caisse | Evenement::Drop))
            || crate::figures::Figures::actives(self)
            || crate::loin::Loin::actives(self)
    }
}

/// Les neuf figures autour du pointeur, dans l'ordre des effets de modulation qui les visent
/// (`modulation::Effets::figures`) : leurs sections dans les réglages.
pub const FIGURES: [&str; 9] = ["arcs", "quatre_temps", "anneau", "orbe", "voix", "trame", "pinceau", "pendule", "gouttes"];

/// Allure d'une figure, commune aux neuf (`figures.rs`) : les figures sont des alternatives à
/// l'essaim, elles se règlent autant que lui. Toutes les valeurs par défaut laissent la figure
/// telle qu'elle est dessinée.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Allure {
    /// Échelle de toute la figure (0,5 à 2) ; sans effet sur le trait et le pendule.
    pub echelle: f32,
    /// Opacité (0,1 à 1).
    pub opacite: f32,
    /// Teinte : degrés ajoutés à celle de l'essaim (-180 à 180).
    pub teinte: f32,
    /// Rotation continue autour du pointeur (tours par minute, -30 à 30) ; sans effet sur le
    /// trait et le pendule.
    pub rotation: f32,
    /// Place par rapport au pointeur (px à 100 %, -80 à 80) ; sans effet sur le trait et le pendule.
    pub decalage_x: f32,
    pub decalage_y: f32,
    /// Force des réactions (0 : aucune, 1 : normale, 2 : double) au kick, à la caisse claire, au
    /// charleston, à la basse, à la montée et au drop.
    pub reaction_kick: f32,
    pub reaction_caisse: f32,
    pub reaction_charleston: f32,
    pub reaction_basse: f32,
    pub reaction_montee: f32,
    pub reaction_drop: f32,
}

impl Default for Allure {
    fn default() -> Self {
        Self {
            echelle: 1.0,
            opacite: 1.0,
            teinte: 0.0,
            rotation: 0.0,
            decalage_x: 0.0,
            decalage_y: 0.0,
            reaction_kick: 1.0,
            reaction_caisse: 1.0,
            reaction_charleston: 1.0,
            reaction_basse: 1.0,
            reaction_montee: 1.0,
            reaction_drop: 1.0,
        }
    }
}

/// Bornes des réglages d'allure d'une figure.
fn bornes_allure(key: &str) -> Option<(f64, f64)> {
    Some(match key {
        "echelle" => (0.5, 2.0),
        "opacite" => (0.1, 1.0),
        "teinte" => (-180.0, 180.0),
        "rotation" => (-30.0, 30.0),
        "decalage_x" | "decalage_y" => (-80.0, 80.0),
        "reaction_kick" | "reaction_caisse" | "reaction_charleston" | "reaction_basse" | "reaction_montee" | "reaction_drop" => (0.0, 2.0),
        _ => return None,
    })
}

/// Arcs : deux arcs fins autour du pointeur, comme les anneaux d'une montre. Le grand mesure la
/// basse, le petit les aigus (`figures.rs`).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Arcs {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Rayon du grand arc (px à 100 %, 10 à 80) ; le petit est aux deux tiers.
    pub rayon: f32,
    /// Épaisseur du trait du grand arc (0,5 à 6).
    pub epaisseur: f32,
}

impl Default for Arcs {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), rayon: 20.0, epaisseur: 2.1 }
    }
}

/// Quatre temps : quatre points sous le pointeur, un par temps ; celui du temps en cours saute
/// sur le kick et s'étire sur la caisse claire. Effacés sans tempo régulier.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct QuatreTemps {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Rayon d'un point (px à 100 %, 1 à 5).
    pub taille: f32,
    /// Écart entre deux points (4 à 20).
    pub ecart: f32,
}

impl Default for QuatreTemps {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), taille: 1.6, ecart: 7.5 }
    }
}

/// Anneau suiveur : un anneau fin qui suit le pointeur avec un temps de retard, s'allonge dans le
/// mouvement et bat avec le kick.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Anneau {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Rayon de l'anneau (px à 100 %, 6 à 60).
    pub rayon: f32,
    /// Épaisseur de son trait au repos (0,5 à 6).
    pub epaisseur: f32,
}

impl Default for Anneau {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), rayon: 13.0, epaisseur: 1.4 }
    }
}

/// Orbe : le spectre en une seule ligne fermée autour du pointeur, que les bandes de fréquences
/// déforment (graves en haut, aigus en bas) ; chaque kick en détache un écho.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Orbe {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Rayon de la ligne au repos (px à 100 %, 10 à 60).
    pub rayon: f32,
    /// Relief : ce qu'une bande à fond ajoute au rayon (2 à 30).
    pub relief: f32,
    /// Épaisseur du trait (0,5 à 5).
    pub epaisseur: f32,
}

impl Default for Orbe {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), rayon: 21.0, relief: 9.0, epaisseur: 1.6 }
    }
}

/// Ligne de voix : une petite onde sous le pointeur, comme celle d'un message vocal ; elle
/// s'efface sans son.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Voix {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Largeur de la ligne (px à 100 %, 20 à 120).
    pub largeur: f32,
    /// Hauteur des grandes ondulations (2 à 20).
    pub hauteur: f32,
    /// Épaisseur du trait (0,5 à 5).
    pub epaisseur: f32,
}

impl Default for Voix {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), largeur: 46.0, hauteur: 6.5, epaisseur: 1.7 }
    }
}

/// Trame : une grille de points fixe à l'écran, visible seulement autour du pointeur, que les
/// coups traversent en vagues.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Trame {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Rayon de la loupe (px à 100 %, 20 à 120).
    pub rayon: f32,
    /// Pas de la grille (6 à 24).
    pub pas: f32,
    /// Taille des points (0,3 à 3).
    pub taille: f32,
}

impl Default for Trame {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), rayon: 46.0, pas: 10.0, taille: 1.0 }
    }
}

/// Trait de pinceau : rien au repos ; quand la souris file, elle laisse un trait, plus épais
/// quand elle va vite, où chaque kick fait glisser une perle.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Pinceau {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Largeur du trait à pleine vitesse (px à 100 %, 2 à 12).
    pub largeur: f32,
    /// Durée du trait (0,2 à 1 s).
    pub duree: f32,
}

impl Default for Pinceau {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), largeur: 5.5, duree: 0.42 }
    }
}

/// Pendule : un petit poids pendu au pointeur par un fil, qui se balance quand la souris bouge
/// et saute sur le kick.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Pendule {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Longueur du fil (px à 100 %, 8 à 60).
    pub longueur: f32,
    /// Rayon du poids (1 à 6).
    pub taille: f32,
}

impl Default for Pendule {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), longueur: 22.0, taille: 2.6 }
    }
}

/// Gouttes en contour : un noyau et quatre perles en orbite, dont seule la ligne de contour est
/// dessinée ; elle se soude et se sépare (`figures/gouttes.rs`).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Gouttes {
    pub actif: bool,
    /// Taille, opacité, teinte, rotation, place et réactions (communes aux figures).
    #[serde(flatten)]
    pub allure: Allure,
    /// Taille de l'ensemble (0,6 à 1,6 ; 1 : perles jusqu'à 35 px du pointeur).
    pub taille: f32,
    /// Épaisseur de la ligne (1 à 4).
    pub epaisseur: f32,
}

impl Default for Gouttes {
    fn default() -> Self {
        Self { actif: false, allure: Allure::default(), taille: 1.0, epaisseur: 1.4 }
    }
}

/// Équerres : quatre petites équerres aux coins de la fenêtre au premier plan, qui s'écartent sur
/// le kick et s'allongent avec la basse (`loin.rs`). Habillage de la fenêtre, hors des presets
/// (`profils::in_profile`) : il reste d'un preset à l'autre.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Equerres {
    pub actif: bool,
    /// Longueur des bras au repos (px à 100 %, 4 à 30).
    pub bras: f32,
    /// Épaisseur du trait (1 à 5).
    pub epaisseur: f32,
}

impl Default for Equerres {
    fn default() -> Self {
        Self { actif: false, bras: 10.0, epaisseur: 2.0 }
    }
}

/// Étiquette du morceau : à chaque changement de piste, le titre et l'artiste s'écrivent à côté
/// du pointeur, puis s'effacent (`etiquette.rs`). Une information, hors des presets.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Etiquette {
    pub actif: bool,
    /// Taille du titre (px à 100 %, 9 à 20).
    pub taille: f32,
    /// Temps d'affichage (s, 1,5 à 8), avant l'effacement.
    pub duree: f32,
}

impl Default for Etiquette {
    fn default() -> Self {
        Self { actif: false, taille: 12.0, duree: 3.0 }
    }
}

/// Halo de fenêtre : la lumière court le long du cadre de la fenêtre au premier plan, depuis le
/// bord le plus proche du pointeur, et déborde un peu vers l'intérieur ; effacé sans son.
/// Habillage de la fenêtre, hors des presets comme les équerres.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Halo {
    pub actif: bool,
    /// Profondeur de la lueur vers l'intérieur de la fenêtre (px à 100 %, 6 à 30).
    pub profondeur: f32,
    /// Épaisseur du trait le long du cadre (1 à 4).
    pub epaisseur: f32,
}

impl Default for Halo {
    fn default() -> Self {
        Self { actif: false, profondeur: 14.0, epaisseur: 2.0 }
    }
}

/// Liseré : un filet de lumière au bord bas de l'écran du pointeur, à son aplomb, où courent les
/// kicks ; effacé sans son. Habillage de l'écran, hors des presets comme les équerres.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Lisere {
    pub actif: bool,
    /// Épaisseur du filet (px à 100 %, 1 à 8).
    pub epaisseur: f32,
}

impl Default for Lisere {
    fn default() -> Self {
        Self { actif: false, epaisseur: 3.0 }
    }
}

/// Apparence des particules, quel que soit l'événement qui les fait naître (`[declencheurs]`).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Particules {
    /// Étincelles, éclats et poussière rebondissent sur les bords de la fenêtre au premier plan.
    pub rebonds: bool,
    /// Taille des ondes, des éclats et des implosions (0,3 à 3).
    pub taille: f32,
}

impl Default for Particules {
    fn default() -> Self {
        Self { rebonds: true, taille: 1.0 }
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

/// Rubans : les traînées des comètes deviennent des rubans larges, affinés aux deux bouts, qui se
/// vrillent ; les comètes n'ont alors plus de tête.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Rubans {
    pub actif: bool,
    /// Largeur d'un ruban à son plus large (px à 100 %, 1 à 14).
    pub largeur: f32,
    /// Vrille : 0 = ruban plat, 1 = sa largeur ondule le long de la traînée, comme un ruban qui tourne.
    pub vrille: f32,
}

impl Default for Rubans {
    fn default() -> Self {
        Self { actif: false, largeur: 4.0, vrille: 1.0 }
    }
}

/// Ressorts : le rayon de l'orbite passe par un ressort (il part, dépasse, revient), les têtes
/// s'étirent dans leur élan, les satellites naissent du centre et l'éclat devient un anneau épais.
/// Les actions `rebond`, `poussee` et `frein` des déclencheurs agissent même sans ce mode.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Ressorts {
    pub actif: bool,
    /// Raideur du ressort : sa pulsation (radians/s, 6 à 30). Plus raide, il revient plus vite.
    pub raideur: f32,
    /// Amortissement (0,1 à 1) : 0,1 rebondit longtemps, 1 revient sans dépasser.
    pub amorti: f32,
    /// Étirement des têtes dans le sens de leur vitesse (0 = rondes, 2 = très étirées).
    pub etirement: f32,
}

impl Default for Ressorts {
    fn default() -> Self {
        Self { actif: false, raideur: 19.0, amorti: 0.26, etirement: 1.0 }
    }
}

/// Projecteur « retrouver la souris » : l'écran s'assombrit autour d'un disque clair centré sur
/// le pointeur, qui se resserre puis s'efface. Déclenché en secouant la souris ou par Ctrl+Alt+F.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Projecteur {
    pub actif: bool,
    /// Durée de l'effet (s).
    pub duree: f32,
    /// Secouer la souris le lance (sinon, seulement Ctrl+Alt+F). Propre à la main et à la
    /// souris de l'utilisateur, comme `sensibilite` : hors presets.
    pub secousse: bool,
    /// Sensibilité à la secousse : 1 = geste ample et vif, 10 = petit geste, plus lent
    /// (`effets::secousse_exigence`).
    pub sensibilite: f32,
    /// Rayon du disque clair à la fin de l'effet (px).
    pub taille: f32,
    /// Opacité du voile qui assombrit l'écran (0,2 à 0,95).
    pub opacite: f32,
}

impl Default for Projecteur {
    fn default() -> Self {
        Self { actif: true, duree: 1.2, secousse: true, sensibilite: 5.0, taille: 60.0, opacite: 0.62 }
    }
}

/// Réticule : deux fines lignes croisées centrées sur le pointeur (vide au centre).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Reticule {
    pub actif: bool,
    /// Longueur de chaque branche (px).
    pub taille: f32,
    /// Opacité des lignes (0,05 à 1).
    pub opacite: f32,
    /// Épaisseur des lignes (px).
    pub epaisseur: f32,
    /// Vide au centre, du pointeur au début de chaque branche (px).
    pub vide: f32,
}

impl Default for Reticule {
    fn default() -> Self {
        Self { actif: false, taille: 40.0, opacite: 0.6, epaisseur: 1.2, vide: 12.0 }
    }
}

/// Anneau de spectre : barres radiales autour du pointeur, une par bande de fréquences.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Spectre {
    pub actif: bool,
    /// Nombre de bandes (8 à 32), réparties de 40 Hz à 12 kHz.
    pub bandes: u32,
    /// Rayon intérieur de l'anneau (px).
    pub rayon: f32,
    /// Longueur des barres à pleine énergie (px).
    pub hauteur: f32,
    /// L'anneau s'efface quand plus aucun son ne joue, et revient avec la musique.
    pub masquer_sans_musique: bool,
}

impl Default for Spectre {
    fn default() -> Self {
        Self { actif: false, bandes: 16, rayon: 40.0, hauteur: 24.0, masquer_sans_musique: true }
    }
}

/// Preset choisi tout seul : règles « si … alors … » (prioritaires), puis application au
/// premier plan, puis heure.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Automatique {
    pub actif: bool,
    /// Si la condition `si` est vraie (`artiste = Skrillex`, `bpm > 140 et appli = jeu.exe`…),
    /// le preset `alors` ; la première règle vraie l'emporte (`auto.rs`).
    pub regles: Vec<Regle>,
    /// Application (nom de l'exécutable) → preset ; prioritaire sur les horaires.
    pub applis: Vec<RegleAppli>,
    /// Plage horaire (« 22:00 » à « 07:00 », peut passer minuit) → preset.
    pub horaires: Vec<Horaire>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Regle {
    pub si: String,
    pub alors: String,
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
    /// Ctrl+Alt+→ / ← : preset suivant / précédent ; Ctrl+Alt+P : pause ; Ctrl+Alt+F : projecteur.
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

/// `propriétaire/nom` tel que GitHub les accepte (vide : pas de bibliothèque). Le dépôt entre
/// dans des adresses : rien d'autre ne doit pouvoir s'y glisser (`..`, `?`, `@`, espaces…).
pub fn depot_valide(depot: &str) -> bool {
    if depot.is_empty() {
        return true;
    }
    let Some((proprio, nom)) = depot.split_once('/') else { return false };
    let proprio_ok = (1..=39).contains(&proprio.len()) && proprio.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-');
    let nom_ok = (1..=100).contains(&nom.len())
        && nom.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        && nom != "."
        && nom != "..";
    proprio_ok && nom_ok
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

/// Pauses automatiques (propres à la machine, hors presets) : `pauses.rs`.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Pause {
    /// Pause quand Windows ne veut pas être dérangé (jeu ou appli en plein écran, présentation).
    pub ne_pas_deranger: bool,
    /// Sur batterie ou économiseur de batterie actif.
    pub batterie: ModeBatterie,
    /// Exécutables qui mettent Essaim en pause quand ils sont au premier plan.
    pub applis: Vec<String>,
}

impl Default for Pause {
    fn default() -> Self {
        Self { ne_pas_deranger: true, batterie: ModeBatterie::Reduire, applis: Vec::new() }
    }
}

/// Que faire sur batterie : rien, réduire la cadence (30 images/s au plus) ou se mettre en pause.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModeBatterie {
    Rien,
    #[default]
    Reduire,
    Pause,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Bordure {
    /// La bordure de la fenêtre au premier plan prend la couleur d'Essaim (Windows 11).
    pub active: bool,
    /// Luminosité au repos (0 = noire, 1 = couleur pleine). Elle s'illumine sur les
    /// déclencheurs « bordure » (par défaut : chaque kick).
    pub repos: f32,
}

impl Default for Bordure {
    fn default() -> Self {
        Self { active: true, repos: 0.5 }
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
    /// Veille profonde (plus aucun dessin) après ce nombre de secondes sans mouvement de souris,
    /// son ni clic ; 0 = jamais.
    pub veille_apres: u32,
    /// Calques exclus des captures d'écran et des streams (`WDA_EXCLUDEFROMCAPTURE`).
    pub masquer_captures: bool,
    /// Cadence réduite par paliers quand une image coûte cher à calculer (`adaptatif.rs`).
    pub adaptatif: bool,
    /// Sans flashs (photosensibilité) : éclats adoucis, au plus trois par seconde, bordure qui
    /// ne clignote plus. `auto` : quand Windows demande de réduire les animations.
    pub sans_flashs: SansFlashs,
}

impl Default for Rendu {
    fn default() -> Self {
        Self {
            cadence: 0,
            carte_graphique: true,
            cadence_repos: 30,
            veille_apres: 60,
            masquer_captures: false,
            adaptatif: true,
            sans_flashs: SansFlashs::Auto,
        }
    }
}

/// Mode sans flashs : selon le réglage de Windows (« Afficher les animations »), toujours, ou jamais.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SansFlashs {
    #[default]
    Auto,
    Oui,
    Non,
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
    /// Ce qui tourne avec le trajet : comètes, satellites ou tout.
    pub rotation_sur: Groupe,
    /// Trajet dessiné à la main d'avant (`forme = "trace"`) : points (x, y). Gardé pour les presets
    /// d'avant ; une image le remplit aussi (son plus grand contour, pour les versions d'avant).
    pub trace: Vec<[f32; 2]>,
    /// Contours d'une image (`forme = "trace"`), un par anneau de comètes (4 au plus) : points
    /// (x, y) d'un même repère (pixels de l'image), que le moteur met à l'échelle ensemble.
    pub contours: Vec<Vec<[f32; 2]>>,
    /// Inertie du centre de l'orbite : 0 = suit le pointeur de près, 1 = ressort souple qui
    /// traîne derrière la souris et la dépasse un peu à l'arrêt.
    pub inertie: f32,
    pub vitesse_base: f32,
    pub trainee: f32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Essaim {
    pub maximum: usize,
    pub duree: f32,
    /// Satellites permanents, présents même sans musique.
    pub fixes: u32,
    /// Trajet des satellites ; par défaut celui des comètes.
    pub forme: FormeSatellites,
    /// Constellation : fines lignes entre les satellites et les comètes proches.
    pub constellation: bool,
}

/// À qui s'applique une rotation ou une modulation.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Groupe {
    /// Les comètes et les satellites (l'essaim).
    #[default]
    Tout,
    Cometes,
    Satellites,
    /// Toutes les figures autour du pointeur, ou l'une d'elles.
    Figures,
    Arcs,
    #[serde(rename = "quatre_temps")]
    QuatreTemps,
    Anneau,
    Orbe,
    Voix,
    Trame,
    Pinceau,
    Pendule,
    Gouttes,
}

impl Groupe {
    /// Comprend-il les comètes (`satellites = false`) ou les satellites (`true`) ?
    pub fn inclut(self, satellites: bool) -> bool {
        match self {
            Self::Tout => true,
            Self::Cometes => !satellites,
            Self::Satellites => satellites,
            _ => false,
        }
    }

    /// Vise-t-il la figure `i` (dans l'ordre de `FIGURES`) ?
    pub fn vise_figure(self, i: usize) -> bool {
        let seule = match self {
            Self::Figures => return true,
            Self::Arcs => 0,
            Self::QuatreTemps => 1,
            Self::Anneau => 2,
            Self::Orbe => 3,
            Self::Voix => 4,
            Self::Trame => 5,
            Self::Pinceau => 6,
            Self::Pendule => 7,
            Self::Gouttes => 8,
            Self::Tout | Self::Cometes | Self::Satellites => return false,
        };
        seule == i
    }
}

/// Modulations (`[modulations] liste`) : chacune fait agir une source du son sur un paramètre
/// de l'essaim, pour les comètes, les satellites ou les deux (`modulation.rs`).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Modulations {
    pub liste: Vec<Modulation>,
}

impl Default for Modulations {
    fn default() -> Self {
        Self { liste: crate::migration::Ancien::default().modulations(true, true) }
    }
}

/// Réglages lus d'un texte TOML, réglages d'avant les réactions converties
/// (`migration::migrer`).
pub fn de_texte(texte: &str) -> Result<Config, toml::de::Error> {
    let mut t: toml::Table = toml::from_str(texte)?;
    crate::migration::migrer(&mut t);
    Config::deserialize(t)
}

/// Convertit `config.toml` une fois pour toutes : les anciennes clés (réactions à la basse,
/// à la musique, aux clics) sont retirées et remplacées par leurs modulations et déclencheurs,
/// commentaires gardés.
pub fn migrer_fichier() -> Result<(), String> {
    migrer_fichier_in(&path())
}

pub(crate) fn migrer_fichier_in(p: &Path) -> Result<(), String> {
    if !p.exists() {
        return Ok(());
    }
    edit_file(p, |doc, known| {
        let mut t: toml::Table = toml::from_str(&doc.to_string()).map_err(|e| format!("config.toml illisible : {e}"))?;
        if !crate::migration::migrer(&mut t) {
            return Ok(());
        }
        let cfg = Config::deserialize(t).map_err(|e| format!("config.toml illisible : {e}"))?;
        for (section, cle) in crate::migration::ANCIENNES_CLES {
            if let Some(sec) = doc.get_mut(section).and_then(|o| o.as_table_like_mut()) {
                sec.remove(cle);
            }
        }
        for section in crate::migration::ANCIENNES_SECTIONS {
            if doc.get(section).and_then(|o| o.as_table_like()).is_some_and(|t| t.is_empty()) {
                doc.remove(section);
            }
        }
        let valeurs = serde_json::to_value(&cfg).map_err(|e| e.to_string())?;
        for (section, cle) in [("modulations", "liste"), ("declencheurs", "liste"), ("particules", "rebonds"), ("particules", "taille")] {
            set_in_doc(doc, known, section, cle, &valeurs[section][cle])?;
        }
        log::info!("config : réactions converties en modulations et déclencheurs");
        Ok(())
    })
}

/// Modulations gardées au plus.
pub const MODULATIONS_MAX: usize = 16;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Modulation {
    pub source: Source,
    pub cible: Cible,
    pub sur: Groupe,
    /// Force, de -1 à 1 (négatif : dans l'autre sens).
    pub quantite: f32,
    /// En dessous de ce niveau de la source (0 à 0,95), rien ; au-dessus, l'effet monte jusqu'au
    /// maximum.
    pub seuil: f32,
    /// Réaction douce (1, ~0,4 s) ou brusque (0, aussitôt).
    pub lissage: f32,
    /// La source à l'envers : 1 − valeur (stéréo : de l'autre côté).
    pub inverser: bool,
}

/// Lissage d'une modulation par défaut (~50 ms).
pub const LISSAGE: f32 = 0.35;

impl Default for Modulation {
    fn default() -> Self {
        Self { source: Source::Basse, cible: Cible::Rotation, sur: Groupe::Tout, quantite: 0.5, seuil: 0.0, lissage: LISSAGE, inverser: false }
    }
}

/// Source d'une modulation, ramenée de 0 à 1 (stéréo : -1 à gauche, 1 à droite). Une source
/// inconnue (preset d'une version plus récente) ne fait rien.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Niveau de la basse.
    Basse,
    /// Basse présente et note grave.
    Grave,
    /// Basse présente et note haute.
    Aigu,
    /// Coup de chaque kick, qui retombe vite.
    Kick,
    /// Coup de chaque caisse claire (ou clap).
    Caisse,
    /// Attaques des aigus (charleston, cymbales).
    Charleston,
    /// Tension d'une montée, avant le drop.
    Montee,
    /// Drop : 1 au drop, retombe en deux secondes environ.
    Drop,
    /// Côté qui joue le plus fort.
    Stereo,
    /// Battement du tempo (1 sur chaque temps), quand il est régulier.
    Tempo,
    /// Coup de chaque contretemps (milieu du temps), quand le tempo est régulier.
    Contretemps,
    /// Brillance du son (part d'aigus).
    Brillance,
    #[serde(other)]
    Aucune,
}

/// Paramètre modulé. Une cible inconnue ne fait rien.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Cible {
    /// Rotation du trajet (tours par minute ajoutés).
    Rotation,
    /// Vitesse sur le trajet (radians/s ajoutés).
    Vitesse,
    /// Accélération : la vitesse sur le trajet multipliée.
    Acceleration,
    /// Rayon de l'orbite.
    Rayon,
    /// Taille des têtes et épaisseur des traînées.
    Taille,
    /// Teinte (degrés ajoutés) ; « tout » : celle de tout ce qu'Essaim dessine, bordure comprise.
    Teinte,
    /// Décalage de l'orbite vers la gauche ou la droite (en rayons).
    Decalage,
    /// Lueur des têtes.
    Luminosite,
    /// Longueur des traînées.
    Trainee,
    /// Opacité des traînées et des têtes.
    Opacite,
    #[serde(other)]
    Aucune,
}

/// Déclencheurs (`[declencheurs] liste`) : quand un événement arrive (kick, caisse claire, drop,
/// clic…), une action (éclat, onde, satellites, fissure…). Ils remplacent les réactions fixes
/// d'avant : ce sont, par défaut, celles-là (`migration::Ancien::declencheurs`).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Declencheurs {
    pub liste: Vec<Declencheur>,
}

impl Default for Declencheurs {
    fn default() -> Self {
        Self { liste: crate::migration::Ancien::default().declencheurs() }
    }
}

/// Déclencheurs gardés au plus.
pub const DECLENCHEURS_MAX: usize = 24;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Declencheur {
    pub quand: Evenement,
    pub faire: Action,
    /// Force de l'éclat (0 à 1), de l'illumination de la bordure (0 à 1), ou taille de l'onde,
    /// des éclats, de la fissure, du carré, de l'implosion (0,2 à 3).
    pub force: f32,
    /// Satellites, étincelles : au moins `nombre`, au plus `nombre_max`.
    pub nombre: u32,
    pub nombre_max: u32,
    /// Délai minimal entre deux déclenchements (s ; 0 = aucun).
    pub delai: f32,
    /// Chance de se déclencher à chaque événement (0 à 1).
    pub probabilite: f32,
}

impl Default for Declencheur {
    fn default() -> Self {
        Self { quand: Evenement::Kick, faire: Action::Onde, force: 1.0, nombre: 3, nombre_max: 3, delai: 0.0, probabilite: 1.0 }
    }
}

impl Declencheur {
    pub fn new(quand: Evenement, faire: Action) -> Self {
        Self { quand, faire, ..Default::default() }
    }
}

/// Événement qui déclenche. Un événement inconnu ne déclenche rien.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Evenement {
    Kick,
    /// Caisse claire ou clap.
    Caisse,
    /// Attaque des aigus (charleston, cymbales).
    Charleston,
    /// Drop, après une montée ou un passage calme.
    Drop,
    /// Chaque temps, quand le tempo est régulier.
    Temps,
    /// Chaque temps pendant un break (la musique continue sans kick).
    Break,
    /// Milieu de chaque temps, quand le tempo est régulier.
    Contretemps,
    /// Changement de piste pendant la lecture.
    Piste,
    /// Clic (bouton principal), au point du clic.
    Clic,
    /// Clic droit (bouton secondaire), au point du clic.
    ClicDroit,
    #[serde(other)]
    Aucun,
}

/// Action d'un déclencheur. Une action inconnue ne fait rien.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Éclat sous le pointeur (halo et anneau), qui fait aussi briller les têtes.
    Eclat,
    /// Satellites ajoutés.
    Satellites,
    /// Inverse le sens de rotation.
    Inversion,
    /// Illumine la bordure de la fenêtre au premier plan.
    Bordure,
    /// Onde de choc qui s'élargit.
    Onde,
    /// Éclats projetés.
    Eclats,
    /// Fissure de l'écran.
    Fissure,
    /// Onde qui se referme et étincelles aspirées.
    Implosion,
    /// Carré posé sur la pointe qui s'ouvre.
    Carre,
    /// Étincelles qui jaillissent des têtes.
    Etincelles,
    /// Explosion du drop : éclat, satellites, grande onde, gerbe d'étincelles.
    Explosion,
    /// Rafale : éclat, satellites et grande onde.
    Rafale,
    /// Rebond de l'orbite : son rayon part vers l'extérieur, dépasse et revient (`[ressorts]`).
    Rebond,
    /// Poussée : la rotation prend de la vitesse, qui retombe.
    Poussee,
    /// Coup de frein : la rotation ralentit d'un coup, puis repart.
    Frein,
    #[serde(other)]
    Aucune,
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
    Icone,
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
            Self::Icone => Forme::Icone,
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
    /// Contours d'une image (`[orbite] contours`), ou trajet dessiné à la main d'avant
    /// (`[orbite] trace`).
    Trace,
    /// Contours de l'icône de l'application au premier plan.
    Icone,
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
    /// Prend la teinte dominante de la pochette du morceau qui joue (prioritaire sur
    /// `application`, `site` et Prisme).
    pub pochette: bool,
    /// Teintes de la palette (1 à 3) : les comètes et satellites se les partagent, chacune
    /// décalée de `ecart_palette` degrés de la précédente.
    pub palette: u32,
    pub ecart_palette: f32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Fissures {
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
    /// Sortie son écoutée (identifiant Windows) ; vide = sortie par défaut. Propre à la
    /// machine : hors presets.
    pub sortie: String,
    /// Applications écoutées (noms d'exécutables, `spotify.exe`) : seul leur son est capté ;
    /// vide = tout le son de la sortie. Propre à la machine : hors presets.
    pub applis: Vec<String>,
}

impl Default for Eclat {
    fn default() -> Self {
        Self { taille: 3.4 }
    }
}
impl Default for Orbite {
    fn default() -> Self {
        Self {
            rayon: 22.0,
            cometes: true,
            forme: Forme::Cercle,
            nombre: 2,
            anneaux: 1,
            rotation: 0.0,
            rotation_sur: Groupe::Tout,
            trace: Vec::new(),
            contours: Vec::new(),
            inertie: 0.0,
            vitesse_base: 3.5,
            trainee: 0.3,
        }
    }
}
impl Default for Essaim {
    fn default() -> Self {
        Self { maximum: 16, duree: 2.8, fixes: 0, forme: FormeSatellites::Cometes, constellation: false }
    }
}
impl Default for Couleur {
    fn default() -> Self {
        Self {
            teinte: 0.0,
            vitesse_teinte: 60.0,
            ecart_teinte: 140.0,
            synchro_prisme: true,
            application: false,
            site: false,
            pochette: false,
            palette: 1,
            ecart_palette: 120.0,
        }
    }
}
impl Default for Fissures {
    fn default() -> Self {
        Self { duree: 3.0, taille: 1.4 }
    }
}
impl Default for Audio {
    fn default() -> Self {
        Self { actif: true, sensibilite_kick: 5.0, delai_kick: 0.1, sortie: String::new(), applis: Vec::new() }
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
                        # pentagone, ellipse, coeur, trefle, lissajous, astroide, trace (une image)
                        # ou icone (l'icône de l'application au premier plan)
nombre = 2              # comètes par anneau
anneaux = 1             # anneaux emboîtés (les extérieurs tournent moins vite)
rotation = 0            # rotation du trajet lui-même (tours/min ; négatif : autre sens)
rotation_sur = "tout"   # ce qui tourne avec le trajet : "tout", "cometes" ou "satellites"
trace = []              # trajet d'avant les images (forme = "trace"), points [x, y]
contours = []           # contours d'une image (forme = "trace"), un par anneau : [[[x, y], …], …]
inertie = 0             # 0 = l'orbite suit le pointeur de près, 1 = ressort souple qui traîne derrière
vitesse_base = 3.5      # vitesse de rotation sans basse (radians/s)
trainee = 0.3           # durée de la traînée des comètes (s)

[essaim]
maximum = 16            # satellites en même temps, au plus
duree = 2.8             # durée de vie d'un satellite (s)
fixes = 0               # satellites permanents, présents même sans musique
forme = "cometes"       # trajet des satellites : "cometes" (le même), ou une forme du trajet des comètes
constellation = false   # fines lignes entre les satellites et les comètes proches

[couleur]
teinte = 0              # teinte de départ (0 rouge, 120 vert, 240 bleu) ; fixe si vitesse_teinte = 0
vitesse_teinte = 60     # degrés de teinte par seconde
ecart_teinte = 140      # écart de teinte entre la tête et la queue des traînées
synchro_prisme = true   # prend la teinte de l'éclairage de Prisme quand il tourne
application = false     # prend la teinte de l'icône de l'application au premier plan
site = false            # dans un navigateur, prend la couleur du haut de la page affichée
pochette = false        # prend la teinte de la pochette du morceau qui joue (avant appli, site et Prisme)
palette = 1             # teintes que se partagent comètes et satellites (1 à 3)
ecart_palette = 120     # écart entre deux teintes de la palette (degrés)

[fissures]
duree = 3.0             # secondes avant que la fissure soit refermée
taille = 1.4

[particules]
rebonds = true          # étincelles, éclats et poussière rebondissent sur les bords de la fenêtre active
taille = 1.0            # taille des ondes, des éclats et des implosions (0.3 à 3)

[sillage]
actif = false           # poussière derrière la souris quand elle file vite
densite = 5             # 1 à 10

[rubans]
actif = false           # les traînées des comètes deviennent des rubans, sans tête
largeur = 4             # largeur d'un ruban à son plus large (1 à 14)
vrille = 1              # 0 = ruban plat, 1 = il se vrille

[ressorts]
actif = false           # l'orbite rebondit, les têtes s'étirent, les satellites naissent du centre
raideur = 19            # pulsation du ressort (6 à 30) : plus raide, il revient plus vite
amorti = 0.26           # 0.1 = rebondit longtemps, 1 = revient sans dépasser
etirement = 1           # étirement des têtes dans leur élan (0 = rondes, 2 = très étirées)

# Figures autour du pointeur : des alternatives à l'essaim. En plus de ses réglages, chacune
# accepte son allure (valeurs par défaut) : echelle = 1 (0.5 à 2), opacite = 1 (0.1 à 1),
# teinte = 0 (degrés ajoutés, -180 à 180), rotation = 0 (tr/min, -30 à 30), decalage_x = 0 et
# decalage_y = 0 (px, -80 à 80), et la force de ses réactions, de 0 à 2 : reaction_kick,
# reaction_caisse, reaction_charleston, reaction_basse, reaction_montee, reaction_drop = 1.
# Le trait (pinceau) et le pendule n'ont ni échelle, ni rotation, ni décalage.
[arcs]
actif = false           # deux arcs fins autour du pointeur : le grand mesure la basse, le petit les aigus
rayon = 20              # rayon du grand arc (10 à 80)
epaisseur = 2.1         # épaisseur du trait (0.5 à 6)

[quatre_temps]
actif = false           # quatre points sous le pointeur, un par temps ; effacés sans tempo régulier
taille = 1.6            # rayon d'un point (1 à 5)
ecart = 7.5             # écart entre deux points (4 à 20)

[anneau]
actif = false           # un anneau fin qui suit le pointeur avec du retard et bat avec le kick
rayon = 13              # rayon de l'anneau (6 à 60)
epaisseur = 1.4         # épaisseur du trait au repos (0.5 à 6)

[orbe]
actif = false           # le spectre en une seule ligne fermée autour du pointeur (graves en haut)
rayon = 21              # rayon de la ligne au repos (10 à 60)
relief = 9              # ce qu'une bande de fréquences à fond ajoute au rayon (2 à 30)
epaisseur = 1.6         # épaisseur du trait (0.5 à 5)

[voix]
actif = false           # une petite onde sous le pointeur, comme un message vocal ; effacée sans son
largeur = 46            # largeur de la ligne (20 à 120)
hauteur = 6.5           # hauteur des grandes ondulations (2 à 20)
epaisseur = 1.7         # épaisseur du trait (0.5 à 5)

[trame]
actif = false           # une grille de points visible autour du pointeur, traversée par des vagues
rayon = 46              # rayon de la loupe (20 à 120)
pas = 10                # pas de la grille (6 à 24)
taille = 1              # taille des points (0.3 à 3)

[pinceau]
actif = false           # quand la souris file, elle laisse un trait de pinceau ; rien au repos
largeur = 5.5           # largeur du trait à pleine vitesse (2 à 12)
duree = 0.42            # durée du trait (0.2 à 1 s)

[pendule]
actif = false           # un petit poids pendu au pointeur par un fil, qui saute sur le kick
longueur = 22           # longueur du fil (8 à 60)
taille = 2.6            # rayon du poids (1 à 6)

[gouttes]
actif = false           # gouttes en contour : une ligne liquide qui se soude et se sépare autour du pointeur
taille = 1              # taille de l'ensemble (0.6 à 1.6)
epaisseur = 1.4         # épaisseur de la ligne (1 à 4)

# Habillage de la fenêtre et de l'écran : hors des presets, il reste quand on en change et prend
# les couleurs de l'essaim.
[equerres]
actif = false           # quatre petites équerres aux coins de la fenêtre au premier plan
bras = 10               # longueur des bras au repos (4 à 30)
epaisseur = 2           # épaisseur du trait (1 à 5)

[halo]
actif = false           # la lumière court le long du cadre de la fenêtre au premier plan ; effacé sans son
profondeur = 14         # jusqu'où la lueur entre dans la fenêtre (6 à 30)
epaisseur = 2           # épaisseur du trait le long du cadre (1 à 4)

[lisere]
actif = false           # un filet de lumière au bord bas de l'écran, sous le pointeur ; effacé sans son
epaisseur = 3           # épaisseur du filet (1 à 8)

# Information, hors des presets elle aussi.
[etiquette]
actif = false           # à chaque changement de morceau, son titre s'écrit à côté du pointeur, puis s'efface
taille = 12             # taille du titre (9 à 20)
duree = 3               # temps d'affichage en secondes (1.5 à 8)

[modulations]
# Chaque modulation fait agir une source du son sur un paramètre, pour les comètes, les
# satellites ou tout (16 au plus) :
#   source   : basse (niveau), grave, aigu (note de la basse), kick, caisse, charleston, montee,
#              drop, contretemps, tempo (battement), brillance, stereo (côté qui joue)
#   cible    : rotation (du trajet, 1 = +90 tr/min), vitesse (1 = +40 radians/s),
#              acceleration (vitesse multipliée, 1 = ×3), rayon
#              (1 = +400 %), taille (1 = +150 %), teinte (1 = +180° ; « sur tout » : tout ce
#              qu'Essaim dessine), decalage (1 = 1,6 rayon vers la droite), luminosite (1 = +200 %),
#              trainee (1 = +300 %), opacite (-1 = transparent)
#   sur      : "tout" (comètes et satellites), "cometes", "satellites", "figures" (toutes les
#              figures) ou une figure : "arcs", "quatre_temps", "anneau", "orbe", "voix", "trame",
#              "pinceau", "pendule", "gouttes"
#   quantite : de -1 à 1 (négatif : dans l'autre sens)
#   seuil    : rien sous ce niveau de la source (0 à 0.95) ; lissage : 0 = aussitôt, 1 = doux
#   inverser : la source à l'envers (1 - valeur ; stéréo : l'autre côté)
# Par défaut, les réactions de base : la basse accélère, une note grave éloigne l'orbite, une
# montée resserre l'orbite et accélère. Exemple : le trajet des satellites qui s'emballe au
# drop, { source = "drop", cible = "rotation", sur = "satellites", quantite = 0.8 }
liste = [
    { source = "basse", cible = "vitesse", sur = "tout", quantite = 0.275 },
    { source = "grave", cible = "rayon", sur = "tout", quantite = 0.325 },
    { source = "montee", cible = "acceleration", sur = "tout", quantite = 0.6, lissage = 0 },
    { source = "montee", cible = "rayon", sur = "tout", quantite = -0.1125, lissage = 0.4 },
]

[declencheurs]
# Chaque déclencheur fait une action quand un événement arrive (24 au plus), dans l'ordre :
#   quand    : kick, caisse, charleston (aigus), drop, temps, break (temps sans kick),
#              contretemps, piste (changement de piste), clic, clic_droit
#   faire    : eclat, satellites, inversion (du sens), bordure, onde, eclats, fissure, implosion,
#              carre, etincelles, explosion, rafale, rebond (de l'orbite), poussee (de la
#              rotation), frein
#   force    : 0 à 3 (éclat, bordure et frein : 0 à 1) ; nombre, nombre_max : satellites, étincelles
#   delai    : secondes au moins entre deux fois ; probabilite : chance à chaque fois (0 à 1)
# Par défaut, les réactions de toujours.
liste = [
    { quand = "kick", faire = "inversion" },
    { quand = "kick", faire = "eclat" },
    { quand = "kick", faire = "satellites", nombre = 2, nombre_max = 3 },
    { quand = "kick", faire = "bordure" },
    { quand = "break", faire = "eclat", force = 0.35 },
    { quand = "caisse", faire = "carre" },
    { quand = "drop", faire = "explosion", nombre = 6, nombre_max = 6 },
    { quand = "charleston", faire = "etincelles", nombre = 4, nombre_max = 4 },
    { quand = "clic", faire = "onde" },
    { quand = "clic", faire = "fissure" },
    { quand = "clic_droit", faire = "implosion" },
    { quand = "piste", faire = "rafale", nombre = 6, nombre_max = 6 },
]

[projecteur]
actif = true            # secouer la souris (ou Ctrl+Alt+F) : l'écran s'assombrit autour du pointeur
duree = 1.2             # durée de l'effet (s)
secousse = true         # secouer la souris le lance ; false = seulement Ctrl+Alt+F ; hors presets
sensibilite = 5         # à la secousse : 1 = geste ample et vif, 10 = petit geste, plus lent ; hors presets
taille = 60             # rayon du disque clair à la fin de l'effet (30 à 300)
opacite = 0.62          # assombrissement de l'écran (0.2 à 0.95)

[reticule]
actif = false           # deux fines lignes croisées centrées sur le pointeur
taille = 40             # longueur de chaque branche
opacite = 0.6           # 0.05 à 1
epaisseur = 1.2         # épaisseur des lignes (0.5 à 6)
vide = 12               # vide au centre, autour du pointeur (4 à 60)

[spectre]
actif = false           # barres autour du pointeur qui suivent l'énergie de chaque bande de fréquences
bandes = 16             # nombre de bandes (8 à 32), de 40 Hz à 12 kHz
rayon = 40              # rayon intérieur de l'anneau
hauteur = 24            # longueur des barres à pleine énergie
masquer_sans_musique = true # l'anneau s'efface quand plus aucun son ne joue

[audio]
actif = true
sensibilite_kick = 5    # 1 = peu sensible, 10 = très sensible
delai_kick = 0.1        # délai minimal entre deux kicks (s) ; plus long si un kick long compte double
sortie = ""             # sortie son écoutée (identifiant Windows) ; "" = sortie par défaut ; hors presets
applis = []             # n'écouter que ces applications (["spotify.exe"]) ; [] = tout le son ; hors presets

[rendu]
cadence = 0             # images par seconde de l'essaim ; 0 = celle de l'écran (144…), 72 = deux fois moins de calcul
carte_graphique = true  # dessin par la carte graphique ; false = par le processeur
cadence_repos = 30      # images par seconde quand la souris est immobile et sans musique ; 0 = jamais
veille_apres = 60       # plus rien n'est dessiné après ces secondes sans souris, son ni clic ; 0 = jamais
masquer_captures = false # invisible des captures d'écran et des streams (OBS, Discord, Teams…)
adaptatif = true        # cadence réduite par paliers (120, 60, 30) quand une image coûte cher à calculer
sans_flashs = "auto"    # photosensibilité : "auto" (si Windows réduit les animations), "oui" ou "non"

[souris]
jeu = ""                # jeu de curseurs de Windows des réglages ("" = ne pas y toucher)
avec_preset = true      # choisir un preset installe aussi son jeu de curseurs

[automatique]
actif = false           # choisit tout seul un preset selon les règles, l'application au premier plan ou l'heure
regles = []             # [{ si = "artiste = Skrillex", alors = "Triangle" }] ; clés : artiste, titre, appli, bpm, heure ;
                        # opérateurs : = != ~ (contient) < > <= >= ; plusieurs conditions : « et »
applis = []             # [{ appli = "jeu.exe", preset = "Discret" }]
horaires = []           # [{ debut = "22:00", fin = "07:00", preset = "Discret" }]

[raccourcis]
actif = true            # Ctrl+Alt+→ / ← : preset suivant / précédent ; Ctrl+Alt+P : pause ; Ctrl+Alt+F : projecteur

[bibliotheque]
depot = "KingsRequin/essaim-presets" # dépôt GitHub des presets partagés (index.json à la racine)

[mise_a_jour]
auto = true             # cherche les nouvelles versions sur GitHub, les télécharge et prévient

[pause]
ne_pas_deranger = true  # pause quand Windows ne veut pas être dérangé (plein écran, présentation)
batterie = "reduire"    # sur batterie : "rien", "reduire" (30 images/s au plus) ou "pause"
applis = []             # pause quand l'une d'elles est au premier plan : ["obs64.exe", "zoom.exe"]

[stream]
fond = "transparent"    # fond de la fenêtre pour le stream : "transparent", "vert", "bleu", "magenta" ou "noir" ; hors presets

[bordure]
active = true           # la bordure de la fenêtre au premier plan prend la couleur d'Essaim
repos = 0.5             # luminosité au calme : 0 = noire, 1 = couleur pleine ; elle s'illumine sur les
                        # déclencheurs « bordure » (par défaut, chaque kick)
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
        ("orbite", "inertie") => (0.0, 1.0),
        ("orbite", _) => (0.0, 100.0),
        ("essaim", "fixes") => (0.0, 32.0),
        ("particules", "taille") => (0.3, 3.0),
        ("sillage", "densite") => (1.0, 10.0),
        ("projecteur", "duree") => (0.3, 5.0),
        ("projecteur", "sensibilite") => (1.0, 10.0),
        ("projecteur", "taille") => (30.0, 300.0),
        ("projecteur", "opacite") => (0.2, 0.95),
        ("reticule", "taille") => (10.0, 200.0),
        ("reticule", "opacite") => (0.05, 1.0),
        ("reticule", "epaisseur") => (0.5, 6.0),
        ("reticule", "vide") => (4.0, 60.0),
        ("spectre", "bandes") => (8.0, 32.0),
        ("spectre", "rayon") => (10.0, 150.0),
        ("spectre", "hauteur") => (4.0, 100.0),
        ("rubans", "largeur") => (1.0, 14.0),
        ("rubans", "vrille") => (0.0, 1.0),
        ("ressorts", "raideur") => (6.0, 30.0),
        ("ressorts", "amorti") => (0.1, 1.0),
        ("ressorts", "etirement") => (0.0, 2.0),
        ("arcs", "rayon") => (10.0, 80.0),
        ("arcs", "epaisseur") | ("anneau", "epaisseur") => (0.5, 6.0),
        ("quatre_temps", "taille") => (1.0, 5.0),
        ("quatre_temps", "ecart") => (4.0, 20.0),
        ("anneau", "rayon") => (6.0, 60.0),
        ("orbe", "rayon") => (10.0, 60.0),
        ("orbe", "relief") => (2.0, 30.0),
        ("orbe", "epaisseur") | ("voix", "epaisseur") => (0.5, 5.0),
        ("voix", "largeur") => (20.0, 120.0),
        ("voix", "hauteur") => (2.0, 20.0),
        ("trame", "rayon") => (20.0, 120.0),
        ("trame", "pas") => (6.0, 24.0),
        ("trame", "taille") => (0.3, 3.0),
        ("pinceau", "largeur") => (2.0, 12.0),
        ("pinceau", "duree") => (0.2, 1.0),
        ("pendule", "longueur") => (8.0, 60.0),
        ("pendule", "taille") => (1.0, 6.0),
        ("gouttes", "taille") => (0.6, 1.6),
        ("gouttes", "epaisseur") => (1.0, 4.0),
        ("equerres", "bras") => (4.0, 30.0),
        ("equerres", "epaisseur") => (1.0, 5.0),
        ("lisere", "epaisseur") => (1.0, 8.0),
        ("halo", "profondeur") => (6.0, 30.0),
        ("halo", "epaisseur") => (1.0, 4.0),
        ("etiquette", "taille") => (9.0, 20.0),
        ("etiquette", "duree") => (1.5, 8.0),
        ("essaim", "maximum") => (0.0, 64.0),
        ("essaim", "duree") => (0.1, 30.0),
        ("essaim", _) => (0.0, 10.0),
        ("couleur", "teinte") => (0.0, 360.0),
        ("couleur", "palette") => (1.0, 3.0),
        ("couleur", "ecart_palette") => (0.0, 180.0),
        ("couleur", _) => (0.0, 720.0),
        ("fissures", "duree") => (0.2, 30.0),
        ("fissures", "taille") => (0.2, 6.0),
        ("audio", "sensibilite_kick") => (1.0, 10.0),
        ("audio", "delai_kick") => (0.05, 1.0),
        ("rendu", "veille_apres") => (0.0, 3600.0),
        ("rendu", _) => (0.0, 500.0),
        ("bordure", _) => (0.0, 1.0),
        (s, k) if FIGURES.contains(&s) && bornes_allure(k).is_some() => bornes_allure(k).unwrap_or((0.0, 1.0)),
        _ => (0.0, 1000.0),
    }
}

/// La valeur `f` est-elle dans `[lo, hi]`, à une erreur d'arrondi près ? Un f32 relu en f64
/// (0,01 → 0,00999999977) ne doit être ni refusé ni signalé hors bornes.
fn dans_les_bornes(f: f64, lo: f64, hi: f64) -> bool {
    let marge = |b: f64| 1e-6 * b.abs().max(1e-3);
    f >= lo - marge(lo) && f <= hi + marge(hi)
}

impl Config {
    /// Lit un fichier de réglages (manquants : valeurs par défaut), bornes appliquées.
    pub fn read(p: &Path) -> anyhow::Result<Config> {
        Ok(de_texte(&std::fs::read_to_string(p)?)?.clamped())
    }

    /// Ramène chaque réglage numérique dans ses bornes.
    pub fn clamped(mut self) -> Config {
        // tracé : 2 000 points au plus, sans valeur non finie ni démesurée (avant le passage en
        // JSON, où un point non fini empêcherait toute la relecture)
        self.orbite.trace.retain(|p| p.iter().all(|v| v.is_finite() && v.abs() < 1e6));
        self.orbite.trace.truncate(2000);
        self.orbite.contours.truncate(crate::contours::CONTOURS_MAX);
        for c in &mut self.orbite.contours {
            c.retain(|p| p.iter().all(|v| v.is_finite() && v.abs() < 1e6));
            c.truncate(2000);
        }
        self.orbite.contours.retain(|c| c.len() >= 3);
        self.modulations.liste.truncate(MODULATIONS_MAX);
        let borne = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        for m in &mut self.modulations.liste {
            m.quantite = borne(m.quantite, -1.0, 1.0, 0.0);
            m.seuil = borne(m.seuil, 0.0, 0.95, 0.0);
            m.lissage = borne(m.lissage, 0.0, 1.0, LISSAGE);
        }
        self.declencheurs.liste.truncate(DECLENCHEURS_MAX);
        for d in &mut self.declencheurs.liste {
            d.force = borne(d.force, 0.0, 3.0, 1.0);
            d.nombre = d.nombre.min(40);
            d.nombre_max = d.nombre_max.min(40);
            d.delai = borne(d.delai, 0.0, 10.0, 0.0);
            d.probabilite = borne(d.probabilite, 0.0, 1.0, 1.0);
        }
        if !depot_valide(&self.bibliotheque.depot) {
            log::warn!("config : [bibliotheque] depot « {} » invalide, dépôt par défaut utilisé", self.bibliotheque.depot);
            self.bibliotheque = Bibliotheque::default();
        }
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
                        if !f.is_finite() || !dans_les_bornes(f, lo, hi) {
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

pub(crate) fn set_value_in(p: &Path, section: &str, key: &str, value: &serde_json::Value) -> Result<(), String> {
    edit_file(p, |doc, known| set_in_doc(doc, known, section, key, value))
}

/// Écrit d'un coup les valeurs de `cfg` retenues par `keep(section, clé)` (application d'un
/// profil), en gardant commentaires et mise en page.
pub fn apply_keys(cfg: &Config, keep: impl Fn(&str, &str) -> bool) -> Result<(), String> {
    apply_keys_in(&path(), cfg, keep)
}

pub(crate) fn apply_keys_in(p: &Path, cfg: &Config, keep: impl Fn(&str, &str) -> bool) -> Result<(), String> {
    let values = serde_json::to_value(cfg.clone().clamped()).map_err(|e| e.to_string())?;
    edit_file(p, |doc, known| set_keys(doc, known, &values, keep))
}

/// Texte `base` (TOML) complété des valeurs de `cfg` retenues par `keep` (fichier d'un preset).
pub(crate) fn text_with_keys(base: &str, cfg: &Config, keep: impl Fn(&str, &str) -> bool) -> Result<String, String> {
    let values = serde_json::to_value(cfg.clone().clamped()).map_err(|e| e.to_string())?;
    Ok(edit_text(base, |doc, known| set_keys(doc, known, &values, keep))?.unwrap_or_else(|| base.to_owned()))
}

fn set_keys(
    doc: &mut toml_edit::DocumentMut,
    known: &serde_json::Value,
    values: &serde_json::Value,
    keep: impl Fn(&str, &str) -> bool,
) -> Result<(), String> {
    for (section, keys) in values.as_object().into_iter().flatten() {
        for (key, v) in keys.as_object().into_iter().flatten().filter(|(k, _)| keep(section, k)) {
            set_in_doc(doc, known, section, key, v)?;
        }
    }
    Ok(())
}

/// Modifie `config.toml` par `change` sur les réglages lus : seules les valeurs changées sont
/// réécrites (commentaires gardés), en un seul cycle lire-modifier-écrire sous le verrou.
pub fn modifier(change: impl FnOnce(&mut Config)) -> Result<(), String> {
    modifier_in(&path(), change)
}

pub(crate) fn modifier_in(p: &Path, change: impl FnOnce(&mut Config)) -> Result<(), String> {
    edit_file(p, |doc, known| {
        let lu: Config = de_texte(&doc.to_string()).map_err(|e| format!("config.toml illisible : {e}"))?;
        let mut cfg = lu.clone();
        change(&mut cfg);
        let json = |c: &Config| serde_json::to_value(c).map_err(|e| e.to_string());
        let (avant, apres) = (json(&lu)?, json(&cfg)?);
        for (section, keys) in apres.as_object().into_iter().flatten() {
            for (key, v) in keys.as_object().into_iter().flatten() {
                if avant.get(section).and_then(|s| s.get(key)) != Some(v) {
                    set_in_doc(doc, known, section, key, v)?;
                }
            }
        }
        Ok(())
    })
}

/// Un seul écrivain à la fois pour `config.toml` et les presets : le fil des réglages, les
/// clients de l'interface locale et la boucle principale (preset automatique, raccourcis)
/// écrivent tous.
static ECRITURE: Mutex<()> = Mutex::new(());

/// Prend le verrou d'écriture (un écrivain qui a paniqué ne le bloque pas).
pub(crate) fn verrou() -> MutexGuard<'static, ()> {
    ECRITURE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Écritures de `config.toml` faites par l'appli : chacune force la relecture suivante (deux
/// écritures dans la même date du fichier ne sont pas manquées).
static ECRITURES: AtomicU64 = AtomicU64::new(0);

/// Remplace le fichier `p` d'un coup : écrit à côté (`.tmp`), puis le met à sa place
/// (`fs::rename`, qui remplace le fichier existant sous Windows). Un échec laisse l'ancien
/// fichier intact : jamais de fichier vide ou à moitié écrit.
pub(crate) fn ecrire_atomique(p: &Path, contenu: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut nom = p.file_name().unwrap_or_default().to_os_string();
    nom.push(format!(".{}.tmp", std::process::id()));
    let tmp = p.with_file_name(nom);
    let r = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contenu.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, p)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// Lit `config.toml` (le fichier par défaut s'il n'existe pas), applique `change`, vérifie que
/// le résultat se lit, puis le met en place d'un coup, le tout sous le verrou d'écriture. Un
/// fichier présent mais illisible (UTF-16, ANSI, verrouillé…) n'est jamais remplacé.
fn edit_file(
    p: &Path,
    change: impl FnOnce(&mut toml_edit::DocumentMut, &serde_json::Value) -> Result<(), String>,
) -> Result<(), String> {
    let _verrou = verrou();
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DEFAULT_FILE.to_owned(),
        Err(e) => return Err(format!("{} illisible ({e}), rien n'a été écrit", p.display())),
    };
    let Some(new) = edit_text(&text, change)? else { return Ok(()) };
    ecrire_atomique(p, &new).map_err(|e| format!("écriture de {} impossible : {e}", p.display()))?;
    ECRITURES.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Applique `change` au texte TOML ; `None` s'il reste le même. Le résultat doit se lire.
fn edit_text(
    text: &str,
    change: impl FnOnce(&mut toml_edit::DocumentMut, &serde_json::Value) -> Result<(), String>,
) -> Result<Option<String>, String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("config.toml illisible : {e}"))?;
    let known = serde_json::to_value(Config::default()).map_err(|e| e.to_string())?;
    change(&mut doc, &known)?;
    let new = doc.to_string();
    if new == text {
        return Ok(None);
    }
    de_texte(&new).map_err(|e| format!("valeur refusée : {e}"))?;
    Ok(Some(new))
}

fn set_in_doc(
    doc: &mut toml_edit::DocumentMut,
    known: &serde_json::Value,
    section: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    // un f32 relu (0,00999999977 pour 0,01) passe, ramené sur la borne
    let ramene;
    let mut value = value;
    if let Some(f) = value.as_f64() {
        let (lo, hi) = limits(section, key);
        if !dans_les_bornes(f, lo, hi) {
            return Err(format!("[{section}] {key} : {f} hors bornes ({lo} à {hi})"));
        }
        if !(lo..=hi).contains(&f) {
            ramene = serde_json::Value::from(f.clamp(lo, hi));
            value = &ramene;
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
static RELOAD_NOW: AtomicBool = AtomicBool::new(false);

pub fn request_reload() {
    RELOAD_NOW.store(true, Ordering::Relaxed);
}

pub fn take_reload_request() -> bool {
    RELOAD_NOW.swap(false, Ordering::Relaxed)
}

/// Date et taille d'un fichier : deux écritures rapprochées peuvent avoir la même date.
type Signature = (SystemTime, u64);

fn signature(p: &Path) -> Option<Signature> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
}

pub struct Watched {
    pub cfg: Config,
    p: PathBuf,
    /// Fichier lu en dernier, noté seulement après une lecture réussie.
    lu: Option<Signature>,
    /// Écritures de l'appli déjà prises en compte.
    ecritures: u64,
    /// Dernier fichier illisible signalé au journal (pas de répétition chaque seconde).
    signale: Option<Signature>,
}

impl Watched {
    pub fn load() -> Self {
        let p = path();
        if !p.exists() {
            let _ = std::fs::create_dir_all(dir());
            if let Err(e) = ecrire_atomique(&p, DEFAULT_FILE) {
                log::warn!("config : écriture de {} impossible : {e}", p.display());
            }
        }
        Self::watch(p)
    }

    fn watch(p: PathBuf) -> Self {
        let mut w = Self { cfg: Config::default(), p, lu: None, ecritures: u64::MAX, signale: None };
        w.reload();
        w
    }

    /// Recharge si le fichier a changé (date ou taille) ou si l'appli l'a écrit. Renvoie vrai si
    /// la configuration a été remplacée. Un fichier illisible (en cours d'écriture par un
    /// éditeur, mal tapé) est relu au tour suivant.
    pub fn reload(&mut self) -> bool {
        // relevés avant la lecture : une écriture faite pendant celle-ci sera vue au tour suivant
        let ecritures = ECRITURES.load(Ordering::Relaxed);
        let Some(sig) = signature(&self.p) else { return false };
        if Some(sig) == self.lu && ecritures == self.ecritures {
            return false;
        }
        match Config::read(&self.p) {
            Ok(cfg) => {
                log::info!("config : chargée");
                (self.cfg, self.lu, self.ecritures, self.signale) = (cfg, Some(sig), ecritures, None);
                true
            }
            Err(e) => {
                if self.signale != Some(sig) {
                    self.signale = Some(sig);
                    log::warn!("config : {} illisible, réglages précédents conservés : {e}", self.p.display());
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn depot_de_la_bibliotheque() {
        for bon in ["", "KingsRequin/essaim-presets", "a/b", "x-y/z.y_w"] {
            assert!(super::depot_valide(bon), "{bon} refusé");
        }
        for mauvais in ["sans-barre", "a/b/c", "../x", "a/..", "a/b?x=1", "a b/c", "a@evil.com/x", "/x", "a/", "a_b/c"] {
            assert!(!super::depot_valide(mauvais), "{mauvais} accepté");
        }
        let mut c = super::Config::default();
        c.bibliotheque.depot = "x/../../y".into();
        assert_eq!(c.clamped().bibliotheque.depot, super::Bibliotheque::default().depot);
    }

    #[test]
    fn le_fichier_par_defaut_se_lit() {
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        assert_eq!(c.essaim.maximum, 16);
        assert!((c.essaim.duree - 2.8).abs() < 1e-6);
        assert!(c.bordure.active);
        assert!((c.bordure.repos - super::Bordure::default().repos).abs() < 1e-6);
    }

    #[test]
    fn reglages_de_la_machine() {
        // fichier par défaut : mêmes valeurs que `Default`
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        let d = super::Config::default();
        assert!(!c.rendu.masquer_captures && !d.rendu.masquer_captures);
        assert!(c.rendu.adaptatif && d.rendu.adaptatif);
        assert!(c.pause.ne_pas_deranger && d.pause.ne_pas_deranger);
        assert_eq!((c.pause.batterie, d.pause.batterie), (super::ModeBatterie::Reduire, super::ModeBatterie::Reduire));
        assert!(c.pause.applis.is_empty() && d.pause.applis.is_empty());
        // hors presets
        assert!(c.audio.applis.is_empty() && d.audio.applis.is_empty());
        for (s, k) in [("rendu", "masquer_captures"), ("rendu", "adaptatif"), ("pause", "ne_pas_deranger"), ("pause", "batterie"), ("pause", "applis"), ("audio", "applis")] {
            assert!(!crate::profils::in_profile(s, k), "[{s}] {k}");
        }
        // écriture : valeur connue, inconnue refusée, liste d'applications
        let d = dossier("machine");
        let p = d.join("config.toml");
        std::fs::write(&p, super::DEFAULT_FILE).unwrap();
        super::set_value_in(&p, "pause", "batterie", &serde_json::json!("pause")).unwrap();
        assert!(super::set_value_in(&p, "pause", "batterie", &serde_json::json!("parfois")).is_err());
        super::set_value_in(&p, "pause", "applis", &serde_json::json!(["obs64.exe", "zoom.exe"])).unwrap();
        super::set_value_in(&p, "audio", "applis", &serde_json::json!(["spotify.exe"])).unwrap();
        assert!(super::set_value_in(&p, "pause", "applis", &serde_json::json!([3])).is_err());
        super::set_value_in(&p, "rendu", "masquer_captures", &serde_json::json!(true)).unwrap();
        let c = super::Config::read(&p).unwrap();
        assert_eq!(c.pause.batterie, super::ModeBatterie::Pause);
        assert_eq!(c.pause.applis, ["obs64.exe", "zoom.exe"]);
        assert_eq!(c.audio.applis, ["spotify.exe"]);
        assert!(c.rendu.masquer_captures);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn reglages_d_avant_convertis_en_modulations_et_declencheurs() {
        use super::{Action, Cible, Evenement, Source};
        use crate::migration::Ancien;
        let quand = |c: &super::Config, e: Evenement, f: Action| c.declencheurs.liste.iter().find(|d| d.quand == e && d.faire == f).copied();
        // preset d'Essaim 0.3 : la basse et les réactions fixes, avec leurs valeurs
        let ancien = "[orbite]\nrayon = 28\nvitesse_base = 5\nvitesse_basse = 18\neloignement_grave = 1.8\n\
                      [essaim]\npar_kick_min = 3\npar_kick_max = 5\n[clic]\neclats = true\ntaille = 1.5\ndroit = false\n\
                      [fissures]\nactives = false\n[bordure]\nintensite = 0.5\n[musique]\ncontretemps = true\nstereo = 0.6\n\
                      [scintillement]\nquantite = 7\nrebonds = false\n[couleur]\nmusique = true\n[audio]\npulsation_tempo = false\n";
        let c = super::de_texte(ancien).unwrap();
        assert_eq!(c.orbite.rayon, 28.0);
        let l = &c.modulations.liste;
        let m = |s: Source, ci: Cible| l.iter().find(|m| m.source == s && m.cible == ci).map(|m| m.quantite);
        assert_eq!((m(Source::Basse, Cible::Vitesse), m(Source::Grave, Cible::Rayon)), (Some(18.0 / 40.0), Some(1.8 / 4.0)));
        assert_eq!(m(Source::Montee, Cible::Acceleration), Some(0.6), "la montée : +120 % de vitesse");
        assert!(m(Source::Contretemps, Cible::Taille).is_some() && m(Source::Stereo, Cible::Decalage) == Some(0.6));
        assert!(m(Source::Basse, Cible::Teinte).is_some() && m(Source::Brillance, Cible::Teinte).is_some());
        let sats = quand(&c, Evenement::Kick, Action::Satellites).unwrap();
        assert_eq!((sats.nombre, sats.nombre_max), (3, 5));
        assert_eq!(quand(&c, Evenement::Drop, Action::Explosion).map(|d| d.nombre), Some(10));
        assert_eq!(quand(&c, Evenement::Kick, Action::Bordure).map(|d| d.force), Some(0.5));
        assert_eq!(quand(&c, Evenement::Charleston, Action::Etincelles).map(|d| d.nombre), Some(7));
        assert!(quand(&c, Evenement::Clic, Action::Eclats).is_some() && quand(&c, Evenement::Clic, Action::Onde).is_some());
        assert!(quand(&c, Evenement::Clic, Action::Fissure).is_none() && quand(&c, Evenement::ClicDroit, Action::Implosion).is_none());
        assert!(quand(&c, Evenement::Break, Action::Eclat).is_none());
        assert_eq!((c.particules.rebonds, c.particules.taille), (false, 1.5));
        // preset sans ces listes (fait à la main, ou partiel) : les réactions de toujours
        let c = super::de_texte("[orbite]\nrayon = 26\n").unwrap();
        assert_eq!(c.modulations.liste, super::Modulations::default().liste);
        assert_eq!(c.declencheurs.liste, super::Declencheurs::default().liste);
        // réglages d'Essaim 0.4 : modulations déjà là (gardées), réactions fixes ajoutées après
        let v04 = "[modulations]\nliste = [{ source = \"kick\", cible = \"taille\" }]\n[musique]\nmontee = false\n";
        let c = super::de_texte(v04).unwrap();
        assert_eq!(c.modulations.liste.len(), 1, "ni basse (déjà convertie en 0.4) ni montée (coupée)");
        assert_eq!(c.modulations.liste[0].source, Source::Kick);
        assert!(quand(&c, Evenement::Drop, Action::Explosion).is_none() && quand(&c, Evenement::Caisse, Action::Carre).is_some());
        // réglages de cette version : rien à faire ; une vieille clé égarée est retirée
        let mut t: toml::Table = toml::from_str("[declencheurs]\nliste = []\n[modulations]\nliste = []\n").unwrap();
        assert!(!crate::migration::migrer(&mut t));
        let c = super::de_texte("[declencheurs]\nliste = []\n[clic]\nonde = true\n").unwrap();
        assert!(c.declencheurs.liste.is_empty());
        // les réglages par défaut sont ceux d'avant
        assert_eq!(super::Modulations::default().liste, Ancien::default().modulations(true, true));
        // le fichier est converti une fois, commentaires gardés
        let d = dossier("conversion");
        let p = d.join("config.toml");
        std::fs::write(&p, "# mes réglages\n[orbite]\nrayon = 30 # grand\nvitesse_basse = 5\n[clic]\nonde = false\n[musique]\nrafale_piste = false\n").unwrap();
        super::migrer_fichier_in(&p).unwrap();
        let texte = std::fs::read_to_string(&p).unwrap();
        for vieux in ["vitesse_basse", "[clic]", "[musique]", "onde = false"] {
            assert!(!texte.contains(vieux), "{vieux} : {texte}");
        }
        assert!(texte.contains("rayon = 30 # grand") && texte.contains("[declencheurs]"), "{texte}");
        let c = super::Config::read(&p).unwrap();
        assert!(quand(&c, Evenement::Clic, Action::Onde).is_none() && quand(&c, Evenement::Piste, Action::Rafale).is_none());
        assert_eq!(c.modulations.liste[0].quantite, 5.0 / 40.0);
        super::migrer_fichier_in(&p).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), texte, "rien de plus la deuxième fois");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn modulations_ecrites_lues_et_bornees() {
        use super::{Cible, Groupe, Source};
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        // par défaut, les réactions de base ; le fichier par défaut dit la même chose
        assert_eq!(c.modulations.liste, super::Modulations::default().liste);
        assert_eq!(c.declencheurs.liste, super::Declencheurs::default().liste);
        assert_eq!((c.modulations.liste.len(), c.declencheurs.liste.len()), (4, 12));
        assert!(c.orbite.rotation_sur == Groupe::Tout);
        assert!(crate::profils::in_profile("modulations", "liste") && crate::profils::in_profile("orbite", "rotation_sur"));
        let d = dossier("modulations");
        let p = d.join("config.toml");
        std::fs::write(&p, super::DEFAULT_FILE).unwrap();
        let liste = serde_json::json!([{ "source": "drop", "cible": "rotation", "sur": "satellites", "quantite": 0.8 }]);
        super::set_value_in(&p, "modulations", "liste", &liste).unwrap();
        assert!(super::set_value_in(&p, "modulations", "liste", &serde_json::json!([{ "quantite": "fort" }])).is_err());
        let c = super::Config::read(&p).unwrap();
        assert_eq!(c.modulations.liste.len(), 1);
        let m = c.modulations.liste[0];
        assert_eq!((m.source, m.cible, m.sur, m.quantite), (Source::Drop, Cible::Rotation, Groupe::Satellites, 0.8));
        // la musique analysée seulement si une source ou un événement la demande
        let mut a = super::Config::default();
        assert!(a.analyse_musicale());
        a.modulations.liste.retain(|m| m.source != Source::Montee);
        a.declencheurs.liste.retain(|d| !matches!(d.quand, super::Evenement::Caisse | super::Evenement::Drop));
        assert!(!a.analyse_musicale());
        a.modulations.liste = c.modulations.liste.clone();
        assert!(a.analyse_musicale());
        // source ou cible d'une version plus récente : ignorée ; quantité bornée ; 16 au plus
        let texte = "[modulations]\nliste = [{ source = \"futur\", cible = \"autre\", quantite = 9 }]";
        let c = toml::from_str::<super::Config>(texte).unwrap().clamped();
        assert_eq!((c.modulations.liste[0].source, c.modulations.liste[0].cible, c.modulations.liste[0].quantite), (Source::Aucune, Cible::Aucune, 1.0));
        let mut c = super::Config::default();
        c.modulations.liste = vec![super::Modulation::default(); 40];
        assert_eq!(c.clamped().modulations.liste.len(), super::MODULATIONS_MAX);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn effets_projecteur_reticule_spectre_lus_et_bornes() {
        // fichier par défaut : mêmes valeurs que les réglages par défaut
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        let d = super::Config::default();
        assert_eq!((c.projecteur.actif, c.reticule.actif, c.spectre.actif), (true, false, false));
        assert_eq!((c.projecteur.duree, c.reticule.taille, c.reticule.opacite), (d.projecteur.duree, d.reticule.taille, d.reticule.opacite));
        assert_eq!((c.spectre.bandes, c.spectre.rayon, c.spectre.hauteur), (d.spectre.bandes, d.spectre.rayon, d.spectre.hauteur));
        assert_eq!((c.projecteur.secousse, c.projecteur.sensibilite, c.projecteur.taille, c.projecteur.opacite), (d.projecteur.secousse, d.projecteur.sensibilite, d.projecteur.taille, d.projecteur.opacite));
        assert_eq!((c.reticule.epaisseur, c.reticule.vide), (d.reticule.epaisseur, d.reticule.vide));
        // fichier d'avant ces réglages : leurs valeurs par défaut
        let c: super::Config = toml::from_str("[projecteur]\nduree = 2\n[reticule]\nactif = true").unwrap();
        assert_eq!((c.projecteur.secousse, c.projecteur.sensibilite, c.reticule.epaisseur, c.reticule.vide), (true, 5.0, 1.2, 12.0));
        // valeurs absurdes ramenées dans leurs bornes
        let texte = "[projecteur]\nduree = 99\nsensibilite = 40\ntaille = 5\nopacite = 3\n[reticule]\ntaille = 1\nopacite = 7\nepaisseur = 90\nvide = 0\n[spectre]\nbandes = 300\nrayon = -5\nhauteur = 1e9";
        let c = toml::from_str::<super::Config>(texte).unwrap().clamped();
        assert_eq!((c.projecteur.duree, c.reticule.taille, c.reticule.opacite), (5.0, 10.0, 1.0));
        assert_eq!((c.projecteur.sensibilite, c.projecteur.taille, c.projecteur.opacite), (10.0, 30.0, 0.95));
        assert_eq!((c.reticule.epaisseur, c.reticule.vide), (6.0, 4.0));
        assert_eq!((c.spectre.bandes, c.spectre.rayon, c.spectre.hauteur), (32, 10.0, 100.0));
        let c = toml::from_str::<super::Config>("[spectre]\nbandes = 2\n[projecteur]\nduree = 0").unwrap().clamped();
        assert_eq!((c.spectre.bandes, c.projecteur.duree), (8, 0.3));
        // écriture depuis la fenêtre : refusée hors bornes, acceptée dedans
        let p = std::env::temp_dir().join(format!("essaim-test-effets-{}.toml", std::process::id()));
        std::fs::write(&p, super::DEFAULT_FILE).unwrap();
        assert!(super::set_value_in(&p, "spectre", "bandes", &serde_json::json!(33)).is_err());
        assert!(super::set_value_in(&p, "reticule", "opacite", &serde_json::json!(0.0)).is_err());
        super::set_value_in(&p, "spectre", "bandes", &serde_json::json!(24)).unwrap();
        super::set_value_in(&p, "projecteur", "actif", &serde_json::json!(false)).unwrap();
        assert!(super::set_value_in(&p, "projecteur", "sensibilite", &serde_json::json!(11)).is_err());
        super::set_value_in(&p, "projecteur", "sensibilite", &serde_json::json!(8)).unwrap();
        let c = super::Config::read(&p).unwrap();
        assert_eq!((c.spectre.bandes, c.projecteur.actif, c.projecteur.sensibilite), (24, false, 8.0));
        // config d'avant ces réglages : la clé absente est ajoutée à sa section
        std::fs::write(&p, "[projecteur]\nactif = true\n").unwrap();
        super::set_value_in(&p, "projecteur", "sensibilite", &serde_json::json!(7.5)).unwrap();
        super::set_value_in(&p, "reticule", "epaisseur", &serde_json::json!(3)).unwrap();
        let c = super::Config::read(&p).unwrap();
        assert_eq!((c.projecteur.actif, c.projecteur.sensibilite, c.reticule.epaisseur), (true, 7.5, 3.0));
        // la secousse et sa sensibilité sont propres à l'utilisateur : hors presets
        assert!(!crate::profils::in_profile("projecteur", "secousse") && !crate::profils::in_profile("projecteur", "sensibilite"));
        assert!(crate::profils::in_profile("projecteur", "taille") && crate::profils::in_profile("reticule", "vide"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn rubans_et_ressorts_lus_bornes_et_gardes_dans_les_presets() {
        use super::Action;
        // fichier par défaut : mêmes valeurs que les réglages par défaut, effets éteints
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        let d = super::Config::default();
        assert_eq!((c.rubans.actif, c.ressorts.actif), (false, false));
        assert_eq!((c.rubans.largeur, c.rubans.vrille), (d.rubans.largeur, d.rubans.vrille));
        assert_eq!((c.ressorts.raideur, c.ressorts.amorti, c.ressorts.etirement), (d.ressorts.raideur, d.ressorts.amorti, d.ressorts.etirement));
        // valeurs absurdes ramenées dans leurs bornes
        let texte = "[rubans]\nlargeur = 400\nvrille = -3\n[ressorts]\nraideur = 0\namorti = 9\netirement = 50";
        let c = toml::from_str::<super::Config>(texte).unwrap().clamped();
        assert_eq!((c.rubans.largeur, c.rubans.vrille), (14.0, 0.0));
        assert_eq!((c.ressorts.raideur, c.ressorts.amorti, c.ressorts.etirement), (6.0, 1.0, 2.0));
        // les trois actions des ressorts se lisent ; une action d'une version plus récente ne fait rien
        let c = super::de_texte("[declencheurs]\nliste = [{ quand = \"kick\", faire = \"rebond\" }, { quand = \"kick\", faire = \"poussee\" }, { quand = \"caisse\", faire = \"frein\" }, { quand = \"kick\", faire = \"demain\" }]").unwrap();
        let actions: Vec<Action> = c.declencheurs.liste.iter().map(|d| d.faire).collect();
        assert_eq!(actions, [Action::Rebond, Action::Poussee, Action::Frein, Action::Aucune]);
        // config d'avant ces sections : écrire un réglage les ajoute ; ils font partie des presets
        let p = std::env::temp_dir().join(format!("essaim-test-rubans-{}.toml", std::process::id()));
        std::fs::write(&p, "[orbite]\nrayon = 30\n").unwrap();
        super::set_value_in(&p, "rubans", "actif", &serde_json::json!(true)).unwrap();
        super::set_value_in(&p, "ressorts", "raideur", &serde_json::json!(24)).unwrap();
        assert!(super::set_value_in(&p, "ressorts", "amorti", &serde_json::json!(0.0)).is_err());
        let c = super::Config::read(&p).unwrap();
        assert_eq!((c.orbite.rayon, c.rubans.actif, c.ressorts.raideur), (30.0, true, 24.0));
        for (s, k) in [("rubans", "actif"), ("rubans", "largeur"), ("ressorts", "actif"), ("ressorts", "etirement")] {
            assert!(crate::profils::in_profile(s, k), "[{s}] {k}");
        }
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn petites_figures_lues_bornees_et_gardees_dans_les_presets() {
        // fichier par défaut : mêmes valeurs que les réglages par défaut, figures éteintes
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        let d = super::Config::default();
        assert_eq!((c.arcs.actif, c.quatre_temps.actif, c.anneau.actif), (false, false, false));
        assert_eq!((c.arcs.rayon, c.arcs.epaisseur), (d.arcs.rayon, d.arcs.epaisseur));
        assert_eq!((c.quatre_temps.taille, c.quatre_temps.ecart), (d.quatre_temps.taille, d.quatre_temps.ecart));
        assert_eq!((c.anneau.rayon, c.anneau.epaisseur), (d.anneau.rayon, d.anneau.epaisseur));
        // éteintes, elles ne demandent pas l'analyse musicale ; une figure active, si
        let mut c = super::Config::default();
        (c.modulations.liste, c.declencheurs.liste) = (Vec::new(), Vec::new());
        assert!(!c.analyse_musicale());
        c.anneau.actif = true;
        assert!(c.analyse_musicale());
        // valeurs absurdes ramenées dans leurs bornes
        let texte = "[arcs]\nrayon = 900\nepaisseur = 0\n[quatre_temps]\ntaille = 40\necart = 1\n[anneau]\nrayon = 1\nepaisseur = 99";
        let c = toml::from_str::<super::Config>(texte).unwrap().clamped();
        assert_eq!((c.arcs.rayon, c.arcs.epaisseur), (80.0, 0.5));
        assert_eq!((c.quatre_temps.taille, c.quatre_temps.ecart), (5.0, 4.0));
        assert_eq!((c.anneau.rayon, c.anneau.epaisseur), (6.0, 6.0));
        for (s, k) in [("arcs", "actif"), ("arcs", "rayon"), ("quatre_temps", "ecart"), ("anneau", "epaisseur")] {
            assert!(crate::profils::in_profile(s, k), "[{s}] {k}");
        }
        // orbe, ligne de voix, trame, trait de pinceau, pendule : mêmes règles
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        assert_eq!((c.orbe.actif, c.voix.actif, c.trame.actif, c.pinceau.actif, c.pendule.actif), (false, false, false, false, false));
        assert_eq!((c.orbe.rayon, c.orbe.relief, c.orbe.epaisseur), (d.orbe.rayon, d.orbe.relief, d.orbe.epaisseur));
        assert_eq!((c.voix.largeur, c.voix.hauteur, c.voix.epaisseur), (d.voix.largeur, d.voix.hauteur, d.voix.epaisseur));
        assert_eq!((c.trame.rayon, c.trame.pas, c.trame.taille), (d.trame.rayon, d.trame.pas, d.trame.taille));
        assert_eq!((c.pinceau.largeur, c.pinceau.duree, c.pendule.longueur, c.pendule.taille), (d.pinceau.largeur, d.pinceau.duree, d.pendule.longueur, d.pendule.taille));
        let texte = "[orbe]\nrayon = 1\nrelief = 99\n[voix]\nlargeur = 5\nhauteur = 80\n[trame]\nrayon = 500\npas = 1\ntaille = 9\n[pinceau]\nlargeur = 40\nduree = 9\n[pendule]\nlongueur = 1\ntaille = 20";
        let c = toml::from_str::<super::Config>(texte).unwrap().clamped();
        assert_eq!((c.orbe.rayon, c.orbe.relief, c.voix.largeur, c.voix.hauteur), (10.0, 30.0, 20.0, 20.0));
        assert_eq!((c.trame.rayon, c.trame.pas, c.trame.taille), (120.0, 6.0, 3.0));
        assert_eq!((c.pinceau.largeur, c.pinceau.duree, c.pendule.longueur, c.pendule.taille), (12.0, 1.0, 8.0, 6.0));
        for s in ["orbe", "voix", "trame", "pinceau", "pendule", "gouttes"] {
            assert!(crate::profils::in_profile(s, "actif"), "[{s}] actif");
        }
        // gouttes : éteintes par défaut, bornées
        let c: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        assert_eq!((c.gouttes.actif, c.gouttes.taille, c.gouttes.epaisseur), (false, d.gouttes.taille, d.gouttes.epaisseur));
        let c = toml::from_str::<super::Config>("[gouttes]\ntaille = 9\nepaisseur = 0").unwrap().clamped();
        assert_eq!((c.gouttes.taille, c.gouttes.epaisseur), (1.6, 1.0));
    }

    /// Équerres et liseré : éteints par défaut, bornés, et hors des presets. Appliquer un preset
    /// ne les éteint pas et ne touche pas à leurs réglages ; aucun preset fourni ne les allume.
    #[test]
    fn habillage_de_la_fenetre_hors_des_presets() {
        let (c, d): (super::Config, _) = (toml::from_str(super::DEFAULT_FILE).unwrap(), super::Config::default());
        assert_eq!((c.equerres.actif, c.halo.actif, c.lisere.actif), (false, false, false));
        assert_eq!((c.equerres.bras, c.equerres.epaisseur, c.lisere.epaisseur), (d.equerres.bras, d.equerres.epaisseur, d.lisere.epaisseur));
        assert_eq!((c.halo.profondeur, c.halo.epaisseur), (d.halo.profondeur, d.halo.epaisseur));
        let c = toml::from_str::<super::Config>("[equerres]\nbras = 400\nepaisseur = 0\n[lisere]\nepaisseur = 90\n[halo]\nprofondeur = 1\nepaisseur = 9").unwrap().clamped();
        assert_eq!((c.equerres.bras, c.equerres.epaisseur, c.lisere.epaisseur), (30.0, 1.0, 8.0));
        assert_eq!((c.halo.profondeur, c.halo.epaisseur), (6.0, 4.0));
        let hors = [("equerres", "actif"), ("equerres", "bras"), ("equerres", "epaisseur"), ("lisere", "actif"), ("lisere", "epaisseur"), ("halo", "actif"), ("halo", "profondeur"), ("halo", "epaisseur")];
        // l'étiquette du morceau non plus : éteinte par défaut, bornée, hors des presets
        let e: super::Config = toml::from_str(super::DEFAULT_FILE).unwrap();
        assert_eq!((e.etiquette.actif, e.etiquette.taille, e.etiquette.duree), (false, d.etiquette.taille, d.etiquette.duree));
        let e = toml::from_str::<super::Config>("[etiquette]\ntaille = 90\nduree = 0").unwrap().clamped();
        assert_eq!((e.etiquette.taille, e.etiquette.duree), (20.0, 1.5));
        for k in ["actif", "taille", "duree"] {
            assert!(!crate::profils::in_profile("etiquette", k), "[etiquette] {k}");
        }
        for (s, k) in hors {
            assert!(!crate::profils::in_profile(s, k), "[{s}] {k}");
        }
        assert!(crate::prereglages::builtins().iter().all(|p| !crate::loin::Loin::actives(&p.cfg)));
        // l'utilisateur les a allumés et réglés ; un preset (qui les ignore) est appliqué par-dessus
        let dir = dossier("habillage");
        let p = dir.join("config.toml");
        let mut cfg = super::Config::default();
        (cfg.equerres.actif, cfg.equerres.bras, cfg.lisere.actif, cfg.lisere.epaisseur) = (true, 18.0, true, 5.0);
        (cfg.halo.actif, cfg.halo.profondeur) = (true, 20.0);
        super::apply_keys_in(&p, &cfg, |_, _| true).unwrap();
        let mut preset = super::Config::default();
        preset.orbite.rayon = 40.0;
        super::apply_keys_in(&p, &preset, crate::profils::in_profile).unwrap();
        let c = super::Config::read(&p).unwrap();
        assert_eq!(c.orbite.rayon, 40.0);
        assert_eq!((c.equerres.actif, c.equerres.bras, c.lisere.actif, c.lisere.epaisseur), (true, 18.0, true, 5.0));
        assert_eq!((c.halo.actif, c.halo.profondeur), (true, 20.0));
        let _ = std::fs::remove_dir_all(&dir);
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
        // contours d'une image, forme « icône »
        let tri = serde_json::json!([[0.0, 0.0], [10.0, 0.0], [5.0, 8.0]]);
        super::set_value_in(&p, "orbite", "contours", &serde_json::json!([tri, tri])).unwrap();
        super::set_value_in(&p, "orbite", "forme", &serde_json::json!("icone")).unwrap();
        let c: super::Config = toml::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!((c.orbite.contours.len(), c.orbite.contours[1][2]), (2, [5.0, 8.0]));
        assert!(c.suit_icone() && c.orbite.chemins().len() == 2);
        assert_eq!(c.automatique.applis[0].preset, "Discret");
        let _ = std::fs::remove_file(&p);
    }

    /// Dossier temporaire propre à un test (jamais le vrai %APPDATA%).
    fn dossier(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("essaim-test-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn un_fichier_illisible_n_est_jamais_ecrase() {
        let d = dossier("illisible");
        // UTF-16 (Bloc-notes « Unicode ») : ni lu ni remplacé par les défauts
        let p = d.join("config.toml");
        let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain("[orbite]\nrayon = 30\n".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        std::fs::write(&p, &utf16).unwrap();
        assert!(super::set_value_in(&p, "orbite", "rayon", &serde_json::json!(40)).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), utf16);
        // ANSI (é en Windows-1252) : pareil
        std::fs::write(&p, b"# r\xe9glages\n[orbite]\nrayon = 30\n").unwrap();
        assert!(super::apply_keys_in(&p, &super::Config::default(), |_, _| true).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"# r\xe9glages\n[orbite]\nrayon = 30\n");
        // fichier absent : créé depuis le fichier par défaut
        let neuf = d.join("neuf.toml");
        super::set_value_in(&neuf, "orbite", "rayon", &serde_json::json!(40)).unwrap();
        let text = std::fs::read_to_string(&neuf).unwrap();
        assert!(text.contains("rayon = 40") && text.contains("[bordure]"), "{text}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ecriture_atomique_sans_fichier_temporaire_restant() {
        let d = dossier("atomique");
        let p = d.join("a.toml");
        std::fs::write(&p, "ancien").unwrap();
        super::ecrire_atomique(&p, "nouveau").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "nouveau");
        // remplacement impossible (un dossier à la place du fichier) : rien d'abîmé, pas de .tmp
        let bloque = d.join("bloque.toml");
        std::fs::create_dir(&bloque).unwrap();
        assert!(super::ecrire_atomique(&bloque, "x").is_err());
        assert!(bloque.is_dir());
        let restes: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "tmp")).collect();
        assert!(restes.is_empty(), "{restes:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ecrivains_simultanes_sans_perte() {
        let d = dossier("simultanes");
        let p = d.join("config.toml");
        std::fs::write(&p, super::DEFAULT_FILE).unwrap();
        let cles = [("orbite", "rayon", 31), ("essaim", "maximum", 12), ("essaim", "fixes", 5), ("orbite", "nombre", 4)];
        std::thread::scope(|s| {
            for (section, cle, v) in cles {
                let p = &p;
                s.spawn(move || {
                    for i in 0..10 {
                        // v + 1 puis v, en alternance : la dernière écriture est v
                        super::set_value_in(p, section, cle, &serde_json::json!(v + (i + 1) % 2)).unwrap();
                    }
                });
            }
        });
        let c = super::Config::read(&p).unwrap();
        assert_eq!((c.orbite.rayon, c.essaim.maximum, c.essaim.fixes, c.orbite.nombre), (31.0, 12, 5, 4));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rechargement_par_date_taille_et_ecritures() {
        let d = dossier("rechargement");
        let p = d.join("config.toml");
        std::fs::write(&p, "[orbite]\nrayon = 30\n").unwrap();
        let date = std::fs::metadata(&p).unwrap().modified().unwrap();
        let mut w = super::Watched::watch(p.clone());
        assert_eq!(w.cfg.orbite.rayon, 30.0);
        let remettre_date = || std::fs::File::options().write(true).open(&p).unwrap().set_modified(date).unwrap();
        // même date, autre taille : relu
        std::fs::write(&p, "[orbite]\nrayon = 150\n").unwrap();
        remettre_date();
        assert!(w.reload());
        assert_eq!(w.cfg.orbite.rayon, 150.0);
        // fichier illisible (lu en pleine écriture) : rien de noté, relu dès qu'il est réparé,
        // même s'il retrouve la date et la taille de la lecture ratée
        std::fs::write(&p, "[orbite]\nrayon = 15xy\n").unwrap();
        remettre_date();
        assert!(!w.reload());
        assert_eq!(w.cfg.orbite.rayon, 150.0);
        std::fs::write(&p, "[orbite]\nrayon = 15.5\n").unwrap();
        remettre_date();
        assert!(w.reload());
        assert_eq!(w.cfg.orbite.rayon, 15.5);
        // écriture de l'appli : relue même si la date et la taille n'ont pas bougé
        super::set_value_in(&p, "orbite", "rayon", &serde_json::json!(16.5)).unwrap(); // même taille que 15.5
        remettre_date();
        assert!(w.reload());
        assert_eq!(w.cfg.orbite.rayon, 16.5);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_f32_relu_reste_dans_ses_bornes() {
        let d = dossier("f32");
        let p = d.join("config.toml");
        let mut cfg = super::Config::default();
        cfg.orbite.trainee = 0.01; // borne basse ; relu 0,00999999977 en f64
        cfg.bordure.repos = 1.0;
        super::apply_keys_in(&p, &cfg, |s, _| s == "orbite" || s == "bordure").unwrap();
        let c = super::Config::read(&p).unwrap();
        assert!((c.orbite.trainee - 0.01).abs() < 1e-7);
        assert!(std::fs::read_to_string(&p).unwrap().contains("trainee = 0.01"));
        assert!(super::set_value_in(&p, "orbite", "trainee", &serde_json::json!(0.01f32 as f64)).is_ok());
        assert!(super::set_value_in(&p, "orbite", "trainee", &serde_json::json!(0.009)).is_err());
        assert!(super::set_value_in(&p, "bordure", "repos", &serde_json::json!(1.01)).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn modifier_ne_reecrit_que_les_valeurs_changees() {
        let d = dossier("modifier");
        let p = d.join("config.toml");
        std::fs::write(&p, super::DEFAULT_FILE).unwrap();
        super::modifier_in(&p, |c| {
            c.automatique.applis.push(super::RegleAppli { appli: "jeu.exe".into(), preset: "Nuit".into() });
            c.orbite.rayon = 25.0;
        })
        .unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        let c = super::Config::read(&p).unwrap();
        assert_eq!(c.automatique.applis[0].preset, "Nuit");
        assert_eq!(c.orbite.rayon, 25.0);
        assert!(text.contains("# rayon de l'orbite sans basse") && text.contains("trainee = 0.3 "), "{text}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn valeurs_absurdes_ramenees_dans_les_bornes() {
        let c: super::Config = toml::from_str("[essaim]\nmaximum = 100000\n[rendu]\ncadence = 99999").unwrap();
        let c = c.clamped();
        assert_eq!(c.essaim.maximum, 64);
        assert_eq!(c.rendu.cadence, 500);
        let c: super::Config = toml::from_str("[bordure]\nrepos = -1").unwrap();
        let c = c.clamped();
        assert_eq!(c.bordure.repos, 0.0);
        // nan, inf et points non finis du tracé : bornés ou écartés
        let texte = "[particules]\ntaille = inf\n[orbite]\ntrace = [[1.0, nan], [2.0, 3.0]]";
        let c = toml::from_str::<super::Config>(texte).unwrap().clamped();
        assert_eq!(c.particules.taille, 0.3);
        assert_eq!(c.orbite.trace, vec![[2.0, 3.0]]);
        // déclencheurs : force, nombres, délai et chance bornés ; 24 au plus
        let texte = "[declencheurs]\nliste = [{ quand = \"kick\", faire = \"onde\", force = 9, nombre = 500, delai = -1, probabilite = 2 }]";
        let d = toml::from_str::<super::Config>(texte).unwrap().clamped().declencheurs.liste[0];
        assert_eq!((d.force, d.nombre, d.delai, d.probabilite), (3.0, 40, 0.0, 1.0));
        let mut c = super::Config::default();
        c.declencheurs.liste = vec![super::Declencheur::default(); 50];
        assert_eq!(c.clamped().declencheurs.liste.len(), super::DECLENCHEURS_MAX);
    }
}

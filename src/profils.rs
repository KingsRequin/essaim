//! Profils : jeux de réglages d'apparence et de réaction au son, à appliquer d'un clic.
//!
//! Préréglages intégrés (non modifiables, chacun refaisable à la main dans les réglages) et
//! profils de l'utilisateur, un fichier `%APPDATA%\Essaim\profils\<nom>.toml` chacun.
//! Appliquer un profil réécrit ses réglages dans `config.toml` (commentaires gardés). Les choix
//! propres à la machine n'en font pas partie : `[rendu]`, `[souris] avec_preset` et
//! `[audio] sortie`.

use crate::config::{self, Config, Forme};
use crate::migration::Ancien;
use std::path::PathBuf;

/// Sections enregistrées dans un profil.
const SECTIONS: [&str; 15] = [
    "eclat", "orbite", "essaim", "couleur", "fissures", "particules", "sillage", "projecteur", "reticule", "spectre", "audio", "bordure",
    "souris", "modulations", "declencheurs",
];

/// Le réglage `[section] key` fait-il partie des profils ?
pub fn in_profile(section: &str, key: &str) -> bool {
    SECTIONS.contains(&section) && !matches!((section, key), ("souris", "avec_preset") | ("audio", "sortie"))
}

pub struct Profile {
    pub name: String,
    pub builtin: bool,
    pub cfg: Config,
    /// Dernière modification du fichier (profils de l'utilisateur).
    pub date: Option<std::time::SystemTime>,
}

pub fn dir() -> PathBuf {
    config::dir().join("profils")
}

/// Presets fournis. Leurs réactions (modulations et déclencheurs) sont celles de toujours,
/// réglées comme avant (`migration::Ancien` : vitesse ajoutée par la basse, satellites par kick,
/// éclats au clic…), pour que chacun garde son allure (essai `presets_comme_avant`).
pub(crate) fn builtins() -> Vec<Profile> {
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

/// Réactions d'un preset fourni : celles d'avant, réglées par `a`.
fn reagir(c: &mut Config, a: &Ancien) {
    c.modulations.liste = a.modulations(true, true);
    c.declencheurs.liste = a.declencheurs();
}

/// Preset de base d'une forme du trajet : `nombre` comètes, vitesse de rotation (radians/s),
/// teinte fixe (la synchro Prisme la remplacerait), sans satellites.
fn forme(nom: &str, forme: Forme, nombre: u32, vitesse: f32, teinte: f32) -> Profile {
    let mut c = Config::default();
    (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (forme, nombre, vitesse);
    c.essaim.maximum = 0;
    (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (teinte, 0.0, 40.0, false);
    reagir(&mut c, &Ancien::default());
    Profile { name: nom.to_owned(), builtin: true, cfg: c, date: None }
}

/// Préréglages puis profils de l'utilisateur (ordre alphabétique).
pub fn list() -> Vec<Profile> {
    let mut user: Vec<Profile> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()? != "toml" {
                return None;
            }
            let name = p.file_stem()?.to_string_lossy().into_owned();
            match Config::read(&p) {
                Ok(cfg) => Some(Profile { name, builtin: false, cfg, date: e.metadata().and_then(|m| m.modified()).ok() }),
                Err(e) => {
                    log::warn!("profil {} illisible : {e}", p.display());
                    None
                }
            }
        })
        .collect();
    user.sort_by_key(|p| p.name.to_lowercase());
    let mut all = builtins();
    all.retain(|b| !user.iter().any(|u| meme_nom(&u.name, &b.name)));
    all.extend(user);
    all
}

/// Deux noms de preset désignent-ils le même fichier ? (Windows ignore la casse, accents
/// compris : « Été » et « été ».)
pub fn meme_nom(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Nom réservé par Windows (CON, PRN, AUX, NUL, COM1 à 9, LPT1 à 9), avec ou sans extension :
/// le fichier « nul.toml » ne peut pas être créé.
pub fn nom_reserve(nom: &str) -> bool {
    let racine = nom.split('.').next().unwrap_or("").trim_end().to_uppercase();
    let chiffre = |c: char| matches!(c, '1'..='9' | '¹' | '²' | '³');
    match racine.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        r => {
            let mut c = r.chars();
            let tete: String = c.by_ref().take(3).collect();
            matches!(tete.as_str(), "COM" | "LPT") && c.next().is_some_and(chiffre) && c.next().is_none()
        }
    }
}

/// Longueur maximale d'un nom de preset.
const NOM_MAX: usize = 40;

/// Nom de profil utilisable comme nom de fichier.
fn check_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("donne un nom au profil".into());
    }
    if name.chars().count() > NOM_MAX {
        return Err(format!("nom trop long ({NOM_MAX} caractères au plus)"));
    }
    if name.chars().any(|c| c.is_control() || r#"\/:*?"<>|"#.contains(c)) || name.ends_with('.') {
        return Err(r#"caractères interdits dans le nom : \ / : * ? " < > |"#.into());
    }
    if nom_reserve(name) {
        return Err(format!("« {name} » est un nom réservé par Windows : choisis un autre nom"));
    }
    Ok(name)
}

fn is_builtin(name: &str) -> bool {
    builtins().iter().any(|b| meme_nom(&b.name, name))
}

/// Enregistre les sections de profil de `cfg` sous `name` (remplace un profil du même nom).
pub fn save(name: &str, cfg: &Config) -> Result<String, String> {
    save_in(&dir(), name, cfg)
}

fn save_in(dir: &std::path::Path, name: &str, cfg: &Config) -> Result<String, String> {
    let name = check_name(name)?;
    if is_builtin(name) {
        return Err(format!("« {name} » est un préréglage : choisis un autre nom"));
    }
    let body = config::text_with_keys("", cfg, in_profile)?;
    let p = dir.join(format!("{name}.toml"));
    let _verrou = config::verrou();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    config::ecrire_atomique(&p, &format!("# Profil d'Essaim « {name} » (fenêtre des réglages).\n\n{body}"))
        .map_err(|e| format!("écriture de {} impossible : {e}", p.display()))?;
    Ok(name.to_owned())
}

/// Enregistre un preset venu d'ailleurs (bibliothèque, fichier importé) sans écraser un preset
/// de l'utilisateur différent qui porte déjà ce nom (à la casse près).
pub fn ajouter(name: &str, cfg: &Config) -> Result<String, String> {
    ajouter_in(&dir(), name, cfg)
}

fn ajouter_in(dir: &std::path::Path, name: &str, cfg: &Config) -> Result<String, String> {
    let nom = check_name(name)?;
    let existant = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).find(|p| {
        p.extension().is_some_and(|x| x.eq_ignore_ascii_case("toml")) && p.file_stem().is_some_and(|s| meme_nom(&s.to_string_lossy(), nom))
    });
    if let Some(p) = existant {
        // le même preset déjà installé : rien à refuser
        if !Config::read(&p).is_ok_and(|c| same_strict(&c, cfg)) {
            let deja = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            return Err(format!("un de tes presets s'appelle déjà « {deja} » : renomme-le ou supprime-le d'abord"));
        }
    }
    save_in(dir, nom, cfg)
}

/// Fichier d'un profil de l'utilisateur.
pub fn fichier(name: &str) -> PathBuf {
    dir().join(format!("{}.toml", name.trim()))
}

/// Fichier du preset `name`, après vérification du nom (nom venu de la page des réglages).
pub fn fichier_verifie(name: &str) -> Result<PathBuf, String> {
    check_name(name).map(fichier)
}

/// Chemin absolu sur un disque de ce PC (pas `\\serveur\partage`, ni `\\.\périphérique`).
pub fn chemin_local(chemin: &std::path::Path) -> bool {
    use std::path::{Component, Prefix};
    chemin.is_absolute()
        && matches!(chemin.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
}

/// Renomme un profil de l'utilisateur, et les règles automatiques qui le citent ; renvoie le
/// nouveau nom.
pub fn rename(ancien: &str, nouveau: &str) -> Result<String, String> {
    let ancien = check_name(ancien)?;
    let nouveau = check_name(nouveau)?;
    if is_builtin(nouveau) {
        return Err(format!("« {nouveau} » est un préréglage : choisis un autre nom"));
    }
    let cible = fichier(nouveau);
    // même nom à la casse près : Windows accepte le renommage
    if cible.exists() && !meme_nom(ancien, nouveau) {
        return Err(format!("un preset s'appelle déjà « {nouveau} »"));
    }
    std::fs::rename(fichier(ancien), &cible).map_err(|_| format!("aucun profil « {ancien} »"))?;
    regles_suivent(ancien, Some(nouveau));
    Ok(nouveau.to_owned())
}

/// Les règles `[automatique]` qui citent le preset `ancien` passent à `nouveau` (renommage) ou
/// disparaissent (`None` : suppression). Renvoie vrai si une règle a changé.
fn regles_renommees(cfg: &mut Config, ancien: &str, nouveau: Option<&str>) -> bool {
    let a = &mut cfg.automatique;
    let n = a.applis.len() + a.horaires.len() + a.regles.len();
    let mut change = false;
    match nouveau {
        Some(nouveau) => {
            let presets = a
                .applis
                .iter_mut()
                .map(|r| &mut r.preset)
                .chain(a.horaires.iter_mut().map(|r| &mut r.preset))
                .chain(a.regles.iter_mut().map(|r| &mut r.alors));
            for p in presets.filter(|p| meme_nom(p, ancien)) {
                *p = nouveau.to_owned();
                change = true;
            }
        }
        None => {
            a.applis.retain(|r| !meme_nom(&r.preset, ancien));
            a.horaires.retain(|r| !meme_nom(&r.preset, ancien));
            a.regles.retain(|r| !meme_nom(&r.alors, ancien));
        }
    }
    change || a.applis.len() + a.horaires.len() + a.regles.len() != n
}

/// Met à jour les règles automatiques dans `config.toml` (un échec est seulement noté : le
/// preset, lui, a bien été renommé ou supprimé).
fn regles_suivent(ancien: &str, nouveau: Option<&str>) {
    let mut change = false;
    match config::modifier(|c| change = regles_renommees(c, ancien, nouveau)) {
        Ok(()) if change => config::request_reload(),
        Ok(()) => {}
        Err(e) => log::warn!("règles automatiques non mises à jour pour « {ancien} » ({e})"),
    }
}

/// Copie un preset (fourni ou non) sous « nom (copie) » ; renvoie le nom de la copie.
pub fn dupliquer(name: &str) -> Result<String, String> {
    let liste = list();
    let p = liste.iter().find(|p| p.name == name).ok_or_else(|| format!("aucun preset « {name} »"))?;
    let pris = |n: &str| liste.iter().any(|q| meme_nom(&q.name, n));
    let nom = (1..100).map(|i| nom_de_copie(name, i)).find(|n| !pris(n)).ok_or("trop de copies")?;
    save(&nom, &p.cfg)
}

/// « nom (copie) », « nom (copie 2) »… : le nom est raccourci pour que le tout tienne dans la
/// longueur permise.
fn nom_de_copie(name: &str, i: u32) -> String {
    let suffixe = if i == 1 { " (copie)".to_owned() } else { format!(" (copie {i})") };
    let base: String = name.trim().chars().take(NOM_MAX - suffixe.chars().count()).collect();
    format!("{}{suffixe}", base.trim_end())
}

/// Importe un fichier de preset (.toml) sous le nom du fichier ; renvoie ce nom.
pub fn importer(chemin: &std::path::Path) -> Result<String, String> {
    let nom = chemin.file_stem().map(|s| s.to_string_lossy().into_owned()).ok_or("fichier sans nom")?;
    if chemin.extension().is_none_or(|e| !e.eq_ignore_ascii_case("toml")) {
        return Err(format!("« {nom} » n'est pas un preset (.toml attendu)"));
    }
    let cfg = Config::read(chemin).map_err(|e| format!("« {nom} » illisible : {e}"))?;
    ajouter(&nom, &cfg)
}

/// Deux valeurs de réglage pareilles (nombres aux arrondis près). `vide_neutre` : un jeu de
/// curseurs non précisé d'un côté ne départage pas.
fn pareilles(v: &serde_json::Value, w: Option<&serde_json::Value>, vide_neutre: bool) -> bool {
    match (v, w) {
        (serde_json::Value::String(x), Some(serde_json::Value::String(y))) if vide_neutre && (x.is_empty() || y.is_empty()) => true,
        (serde_json::Value::Number(m), Some(serde_json::Value::Number(n))) => {
            let (m, n) = (m.as_f64().unwrap_or(0.0), n.as_f64().unwrap_or(0.0));
            (m - n).abs() <= 1e-3 * m.abs().max(n.abs()).max(1.0)
        }
        // listes (modulations, tracé) : élément par élément, nombres aux arrondis près
        (serde_json::Value::Array(a), Some(serde_json::Value::Array(b))) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| pareilles(x, Some(y), false))
        }
        (serde_json::Value::Object(a), Some(serde_json::Value::Object(b))) => {
            a.len() == b.len() && a.iter().all(|(k, x)| pareilles(x, b.get(k), false))
        }
        (v, w) => Some(v) == w,
    }
}

/// Réglages de profil `(section, clé, valeur)` de `cfg`.
fn reglages_de_profil(cfg: &Config) -> Vec<(String, String, serde_json::Value)> {
    let Ok(serde_json::Value::Object(v)) = serde_json::to_value(cfg) else { return Vec::new() };
    v.into_iter()
        .flat_map(|(s, keys)| {
            let keys = if let serde_json::Value::Object(k) = keys { k } else { Default::default() };
            keys.into_iter().map(move |(k, x)| (s.clone(), k, x))
        })
        .filter(|(s, k, _)| in_profile(s, k))
        .collect()
}

/// Valeur `[section] key` de `cfg` en JSON.
fn valeur(cfg: &serde_json::Value, section: &str, key: &str) -> Option<serde_json::Value> {
    cfg.get(section).and_then(|s| s.get(key)).cloned()
}

/// Nombre de réglages de profil qui diffèrent entre `a` et `b`.
pub fn ecarts(a: &Config, b: &Config) -> usize {
    let Ok(b) = serde_json::to_value(b) else { return usize::MAX };
    reglages_de_profil(a).iter().filter(|(s, k, v)| !pareilles(v, valeur(&b, s, k).as_ref(), true)).count()
}

pub fn delete(name: &str) -> Result<(), String> {
    let name = check_name(name)?;
    if is_builtin(name) {
        return Err(format!("« {name} » est un préréglage : il ne peut pas être supprimé"));
    }
    let p = dir().join(format!("{name}.toml"));
    std::fs::remove_file(&p).map_err(|_| format!("aucun profil « {name} »"))?;
    regles_suivent(name, None);
    Ok(())
}

/// Écrit le profil dans `config.toml`. Un profil sans jeu de curseurs garde celui des réglages.
pub fn apply(p: &Profile) -> Result<(), String> {
    let sans_jeu = p.cfg.souris.jeu.is_empty();
    config::apply_keys(&p.cfg, |s, k| in_profile(s, k) && !(sans_jeu && (s, k) == ("souris", "jeu")))
}

/// Installe le jeu de curseurs d'un preset qu'on vient d'appliquer, si l'option est cochée et
/// qu'il en précise un autre que celui en place.
pub fn curseurs_du_preset(preset: &Config) -> Result<(), String> {
    let avec = Config::read(&config::path()).map(|c| c.souris.avec_preset).unwrap_or(true);
    let jeu = &preset.souris.jeu;
    if !avec || jeu.is_empty() || *jeu == crate::souris::actuel() {
        return Ok(());
    }
    crate::souris::installer(jeu)
}

/// Applique le preset `nom` (et ses curseurs), puis fait relire la config.
pub fn appliquer(nom: &str) -> Result<(), String> {
    let liste = list();
    let p = liste.iter().find(|p| p.name == nom).ok_or_else(|| format!("aucun preset « {nom} »"))?;
    apply(p)?;
    config::request_reload();
    curseurs_du_preset(&p.cfg)
}

/// Réglages et curseurs gardés pour y revenir (« Annuler », fin d'une règle automatique).
pub struct Avant {
    /// Réglages tels qu'écrits dans `config.toml`.
    cfg: Config,
    /// Jeu de curseurs réellement en place (celui du fichier peut être périmé ou vide).
    jeu: String,
    /// Valeurs des curseurs en place, pour revenir aussi à des curseurs sans nom de jeu.
    curseurs: Option<crate::souris::Etat>,
}

/// Note les réglages et les curseurs en place ; `None` si `config.toml` est illisible.
pub fn capturer() -> Option<Avant> {
    let cfg = Config::read(&config::path()).ok()?;
    Some(Avant { cfg, jeu: crate::souris::actuel(), curseurs: crate::souris::etat() })
}

/// Remet des réglages gardés (« Annuler »), curseurs compris.
pub fn remettre(avant: &Avant) -> Result<(), String> {
    let mut cfg = avant.cfg.clone();
    cfg.souris.jeu.clone_from(&avant.jeu);
    config::apply_keys(&cfg, in_profile)?;
    config::request_reload();
    match &avant.curseurs {
        Some(e) if crate::souris::etat().as_ref() != Some(e) => crate::souris::restaurer(e),
        Some(_) => Ok(()),
        None if !avant.jeu.is_empty() && avant.jeu != crate::souris::actuel() => crate::souris::installer(&avant.jeu),
        None => Ok(()),
    }
}

/// Fin d'une règle automatique : ne remet que ce que les presets de la règle ont changé (de
/// `avant` à `apres`) et que l'utilisateur n'a pas retouché depuis.
pub fn remettre_partiel(avant: &Avant, apres: &Avant) -> Result<(), String> {
    let actuel = Config::read(&config::path()).map_err(|e| format!("réglages illisibles ({e})"))?;
    let cles = cles_a_remettre(&avant.cfg, &apres.cfg, &actuel);
    if !cles.is_empty() {
        config::apply_keys(&avant.cfg, |s, k| cles.iter().any(|(a, b)| a == s && b == k))?;
        config::request_reload();
    }
    // curseurs : seulement s'ils sont toujours ceux mis par la règle
    match (&avant.curseurs, &apres.curseurs) {
        (Some(a), Some(p)) if a != p && crate::souris::etat().as_ref() == Some(p) => crate::souris::restaurer(a),
        _ => Ok(()),
    }
}

/// Réglages de profil changés de `avant` à `apres` et encore à leur valeur de `apres` dans
/// `actuel`.
fn cles_a_remettre(avant: &Config, apres: &Config, actuel: &Config) -> Vec<(String, String)> {
    let (Ok(av), Ok(ac)) = (serde_json::to_value(avant), serde_json::to_value(actuel)) else { return Vec::new() };
    reglages_de_profil(apres)
        .into_iter()
        .filter(|(s, k, v)| !pareilles(v, valeur(&av, s, k).as_ref(), false) && pareilles(v, valeur(&ac, s, k).as_ref(), false))
        .map(|(s, k, _)| (s, k))
        .collect()
}

/// Preset voisin de celui en place dans la liste (`pas` = 1 : suivant, -1 : précédent).
pub fn voisin(cfg: &Config, pas: i32) -> Option<String> {
    let liste = list();
    if liste.is_empty() {
        return None;
    }
    let n = liste.len() as i32;
    let i = liste.iter().position(|p| same(&p.cfg, cfg)).map_or(if pas > 0 { -1 } else { 0 }, |i| i as i32);
    Some(liste[(i + pas).rem_euclid(n) as usize].name.clone())
}

/// Les réglages de profil de `a` et `b` sont-ils identiques (aux arrondis près) ?
/// Un jeu de curseurs non précisé d'un côté ne départage pas.
pub fn same(a: &Config, b: &Config) -> bool {
    meme_reglages(a, b, true)
}

/// Comme `same`, mais le jeu de curseurs compte même vide.
fn same_strict(a: &Config, b: &Config) -> bool {
    meme_reglages(a, b, false)
}

fn meme_reglages(a: &Config, b: &Config, vide_neutre: bool) -> bool {
    let Ok(b) = serde_json::to_value(b) else { return false };
    reglages_de_profil(a).iter().all(|(s, k, v)| pareilles(v, valeur(&b, s, k).as_ref(), vide_neutre))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Écrit les presets fournis en JSON (`{ nom: réglages }`) dans le fichier `ESSAIM_PRESETS`,
    /// pour les images du README : `cargo test --release exporter_presets -- --ignored`.
    #[test]
    #[ignore]
    fn exporter_presets() {
        let sortie = std::env::var("ESSAIM_PRESETS").expect("ESSAIM_PRESETS : fichier de sortie");
        let presets: serde_json::Map<String, serde_json::Value> =
            builtins().into_iter().map(|p| (p.name, serde_json::to_value(p.cfg).unwrap())).collect();
        std::fs::write(sortie, serde_json::to_string(&presets).unwrap()).unwrap();
    }

    #[test]
    fn ecarts_entre_reglages() {
        let a = Config::default();
        let mut b = Config::default();
        assert_eq!(ecarts(&a, &b), 0);
        b.orbite.rayon = 40.0;
        b.essaim.maximum = 3;
        b.rendu.cadence = 60; // hors profil
        b.audio.sortie = "{casque}".into(); // propre à la machine : hors profil
        assert_eq!(ecarts(&a, &b), 2);
        b.declencheurs.liste.pop(); // dans les profils
        assert_eq!(ecarts(&a, &b), 3);
    }

    #[test]
    fn la_sortie_son_reste_hors_des_presets() {
        assert!(!in_profile("audio", "sortie"));
        assert!(in_profile("audio", "sensibilite_kick"));
        let mut cfg = Config::default();
        cfg.audio.sortie = "{casque}".into();
        let texte = config::text_with_keys("", &cfg, in_profile).unwrap();
        assert!(!texte.contains("sortie"), "{texte}");
        assert!(texte.contains("sensibilite_kick"), "{texte}");
    }

    #[test]
    fn noms_refuses() {
        assert!(fichier_verifie("../../x").is_err());
        assert!(chemin_local(std::path::Path::new(r"C:\presets\a.toml")));
        assert!(chemin_local(std::path::Path::new(r"\\?\C:\presets\a.toml")));
        for c in [r"\\serveur\partage\a.toml", r"\\?\UNC\serveur\p\a.toml", r"\\.\pipe\x", r"presets\a.toml", r"\a.toml"] {
            assert!(!chemin_local(std::path::Path::new(c)), "{c}");
        }
        assert!(check_name("  ").is_err());
        assert!(check_name("a/b").is_err());
        assert!(check_name("fin.").is_err());
        assert_eq!(check_name(" Soirée ").unwrap(), "Soirée");
        // noms réservés par Windows, avec ou sans extension, toutes casses
        for n in ["CON", "con", "Nul", "aux.txt", "PRN.toml", "COM1", "lpt9", "com¹", "nul .x"] {
            assert!(check_name(n).is_err(), "{n} accepté");
        }
        for n in ["Console", "COM0", "COM10", "LPT", "Nullité", "AUXI"] {
            assert!(check_name(n).is_ok(), "{n} refusé");
        }
    }

    #[test]
    fn prereglages_reconnus_sans_casse() {
        assert!(is_builtin("défaut"));
        assert!(is_builtin("DÉFAUT"), "casse Unicode ignorée");
        assert!(is_builtin(" point SEUL "));
        assert!(!is_builtin("Défauts"));
    }

    #[test]
    fn noms_de_copie_dans_la_longueur_permise() {
        assert_eq!(nom_de_copie("Nuit", 1), "Nuit (copie)");
        assert_eq!(nom_de_copie("Nuit", 12), "Nuit (copie 12)");
        let long = "Une soirée électro très très longue à nommer";
        for i in [1, 2, 99] {
            let n = nom_de_copie(long, i);
            assert!(check_name(&n).is_ok(), "{n}");
            assert!(n.chars().count() <= NOM_MAX);
        }
    }

    #[test]
    fn un_preset_installe_n_ecrase_pas_un_autre() {
        let d = std::env::temp_dir().join(format!("essaim-profils-ajout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut mien = Config::default();
        mien.orbite.rayon = 40.0;
        save_in(&d, "Soirée", &mien).unwrap();
        let mut autre = Config::default();
        autre.orbite.rayon = 60.0;
        let e = ajouter_in(&d, "SOIRÉE", &autre).unwrap_err();
        assert!(e.contains("Soirée"), "{e}");
        assert!(same(&Config::read(&d.join("Soirée.toml")).unwrap(), &mien), "preset écrasé");
        // le même preset réinstallé : accepté
        assert_eq!(ajouter_in(&d, "soirée", &mien).unwrap(), "soirée");
        assert_eq!(ajouter_in(&d, "Autre", &autre).unwrap(), "Autre");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_echec_d_ecriture_laisse_le_preset_intact() {
        let d = std::env::temp_dir().join(format!("essaim-profils-echec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        save_in(&d, "Fixe", &Config::default()).unwrap();
        let p = d.join("Fixe.toml");
        let avant = std::fs::read(&p).unwrap();
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&p, perms.clone()).unwrap();
        let mut cfg = Config::default();
        cfg.orbite.rayon = 50.0;
        assert!(save_in(&d, "Fixe", &cfg).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), avant, "preset abîmé");
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&p, perms).unwrap();
        let restes = std::fs::read_dir(&d).unwrap().flatten().count();
        assert_eq!(restes, 1, "fichier temporaire resté");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn regles_automatiques_suivent_le_preset() {
        let mut cfg = Config::default();
        cfg.automatique.applis = vec![
            config::RegleAppli { appli: "jeu.exe".into(), preset: "Nuit".into() },
            config::RegleAppli { appli: "film.exe".into(), preset: "Autre".into() },
        ];
        cfg.automatique.horaires = vec![config::Horaire { debut: "22:00".into(), fin: "07:00".into(), preset: "NUIT".into() }];
        cfg.automatique.regles = vec![config::Regle { si: "bpm > 140".into(), alors: "nuit".into() }];
        assert!(regles_renommees(&mut cfg, "nuit", Some("Soir")));
        assert_eq!(cfg.automatique.applis[0].preset, "Soir");
        assert_eq!(cfg.automatique.applis[1].preset, "Autre");
        assert_eq!(cfg.automatique.horaires[0].preset, "Soir");
        assert_eq!(cfg.automatique.regles[0].alors, "Soir");
        assert!(!regles_renommees(&mut cfg, "Absent", Some("X")));
        assert!(regles_renommees(&mut cfg, "soir", None));
        assert_eq!(cfg.automatique.applis.len(), 1);
        assert!(cfg.automatique.horaires.is_empty() && cfg.automatique.regles.is_empty());
    }

    #[test]
    fn fin_de_regle_garde_les_retouches() {
        let avant = Config::default();
        let mut apres = avant.clone();
        (apres.orbite.rayon, apres.essaim.maximum, apres.couleur.teinte) = (40.0, 30, 200.0);
        let mut actuel = apres.clone();
        actuel.essaim.maximum = 5; // retouché pendant la règle : gardé
        actuel.fissures.taille = 3.0; // changé par l'utilisateur seulement : gardé
        let mut cles = cles_a_remettre(&avant, &apres, &actuel);
        cles.sort();
        assert_eq!(cles, vec![("couleur".to_owned(), "teinte".to_owned()), ("orbite".to_owned(), "rayon".to_owned())]);
    }

    #[test]
    fn enregistrer_puis_relire_un_profil() {
        let d = std::env::temp_dir().join(format!("essaim-profils-{}", std::process::id()));
        let mut cfg = Config::default();
        cfg.orbite.rayon = 31.5;
        cfg.couleur.teinte = 200.0;
        cfg.essaim.maximum = 12;
        cfg.rendu.cadence = 60; // hors profil : non enregistré
        save_in(&d, "Essai", &cfg).unwrap();
        let text = std::fs::read_to_string(d.join("Essai.toml")).unwrap();
        let back = Config::read(&d.join("Essai.toml")).unwrap();
        assert!(same(&cfg, &back));
        assert_eq!(back.rendu.cadence, 0);
        assert!(!text.contains("[rendu]"), "{text}");
        assert!(!same(&Config::default(), &back));
        assert!(save_in(&d, "défaut", &cfg).is_err(), "préréglage écrasé");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn prereglages_distincts_et_dans_les_bornes() {
        let b = builtins();
        for (i, p) in b.iter().enumerate() {
            assert!(same(&p.cfg, &p.cfg.clone().clamped()), "{} hors bornes", p.name);
            for q in &b[i + 1..] {
                assert!(!same(&p.cfg, &q.cfg), "{} = {}", p.name, q.name);
            }
        }
    }
}

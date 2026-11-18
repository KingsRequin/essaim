//! Profils : jeux de réglages d'apparence et de réaction au son, à appliquer d'un clic.
//!
//! Préréglages intégrés (non modifiables, chacun refaisable à la main dans les réglages) et
//! profils de l'utilisateur, un fichier `%APPDATA%\Essaim\profils\<nom>.toml` chacun.
//! Appliquer un profil réécrit ses réglages dans `config.toml` (commentaires gardés). Les choix
//! propres à la machine n'en font pas partie : `[rendu]` et `[souris] avec_preset`.

use crate::config::{self, Config, Forme};
use std::path::PathBuf;

/// Sections enregistrées dans un profil.
const SECTIONS: [&str; 11] =
    ["eclat", "orbite", "essaim", "couleur", "fissures", "scintillement", "clic", "sillage", "audio", "bordure", "souris"];

/// Le réglage `[section] key` fait-il partie des profils ?
pub fn in_profile(section: &str, key: &str) -> bool {
    SECTIONS.contains(&section) && (section, key) != ("souris", "avec_preset")
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

fn builtins() -> Vec<Profile> {
    let make = |name: &str, change: fn(&mut Config)| {
        let mut cfg = Config::default();
        change(&mut cfg);
        Profile { name: name.to_owned(), builtin: true, cfg, date: None }
    };
    vec![
        make("Défaut", |_| {}),
        make("Discret", |c| {
            (c.orbite.rayon, c.orbite.vitesse_base, c.orbite.vitesse_basse) = (16.0, 2.0, 5.0);
            (c.orbite.eloignement_grave, c.orbite.trainee) = (0.6, 0.2);
            (c.essaim.par_kick_min, c.essaim.par_kick_max, c.essaim.maximum, c.essaim.duree) = (1, 2, 8, 2.0);
            (c.couleur.vitesse_teinte, c.couleur.ecart_teinte) = (20.0, 90.0);
            (c.fissures.taille, c.fissures.duree) = (1.0, 2.0);
            (c.bordure.intensite, c.bordure.repos) = (0.5, 0.3);
        }),
        make("Intense", |c| {
            (c.orbite.rayon, c.orbite.vitesse_base, c.orbite.vitesse_basse) = (28.0, 5.0, 18.0);
            (c.orbite.eloignement_grave, c.orbite.trainee) = (1.8, 0.6);
            (c.essaim.par_kick_min, c.essaim.par_kick_max, c.essaim.maximum, c.essaim.duree) = (3, 5, 40, 3.5);
            (c.couleur.vitesse_teinte, c.couleur.ecart_teinte) = (120.0, 200.0);
            (c.fissures.taille, c.fissures.duree) = (2.2, 4.0);
            c.audio.sensibilite_kick = 6.0;
            c.orbite.nombre = 3;
        }),
        make("Point seul", |c| {
            c.orbite.cometes = false;
            c.essaim.maximum = 0;
            c.fissures.actives = false;
            c.bordure.active = false;
        }),
        // formes : une teinte fixe chacune (la synchro Prisme la remplacerait)
        make("Delta", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Triangle, 3, 2.4);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (175.0, 0.0, 40.0, false);
        }),
        make("Ruche", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Hexagone, 6, 1.25);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (38.0, 0.0, 30.0, false);
        }),
        make("Huit", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Huit, 2, 1.65);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.synchro_prisme) = (265.0, 60.0, false);
        }),
        make("Pollen", |c| {
            (c.orbite.cometes, c.orbite.vitesse_base) = (false, 1.4);
            (c.essaim.maximum, c.essaim.fixes) = (0, 14);
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (52.0, 0.0, 20.0, false);
        }),
        make("Losange", |c| {
            (c.orbite.forme, c.orbite.nombre, c.orbite.vitesse_base) = (Forme::Losange, 4, 2.1);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.ecart_teinte, c.couleur.synchro_prisme) = (25.0, 0.0, 40.0, false);
        }),
        make("Spirale", |c| {
            (c.orbite.anneaux, c.orbite.nombre, c.orbite.vitesse_base) = (3, 1, 2.6);
            c.essaim.maximum = 0;
            (c.couleur.teinte, c.couleur.vitesse_teinte, c.couleur.synchro_prisme) = (215.0, 60.0, false);
        }),
    ]
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
    all.retain(|b| !user.iter().any(|u| u.name.eq_ignore_ascii_case(&b.name)));
    all.extend(user);
    all
}

/// Nom de profil utilisable comme nom de fichier.
fn check_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("donne un nom au profil".into());
    }
    if name.chars().count() > 40 {
        return Err("nom trop long (40 caractères au plus)".into());
    }
    if name.chars().any(|c| c.is_control() || r#"\/:*?"<>|"#.contains(c)) || name.ends_with('.') {
        return Err(r#"caractères interdits dans le nom : \ / : * ? " < > |"#.into());
    }
    Ok(name)
}

fn is_builtin(name: &str) -> bool {
    builtins().iter().any(|b| b.name.eq_ignore_ascii_case(name))
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
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let p = dir.join(format!("{name}.toml"));
    let failed = |e: std::io::Error| format!("écriture de {} impossible : {e}", p.display());
    std::fs::write(&p, "").map_err(failed)?;
    config::apply_keys_in(&p, cfg, in_profile)?;
    let body = std::fs::read_to_string(&p).map_err(failed)?;
    std::fs::write(&p, format!("# Profil d'Essaim « {name} » (fenêtre des réglages).\n\n{body}")).map_err(failed)?;
    Ok(name.to_owned())
}

/// Fichier d'un profil de l'utilisateur.
pub fn fichier(name: &str) -> PathBuf {
    dir().join(format!("{}.toml", name.trim()))
}

/// Renomme un profil de l'utilisateur ; renvoie le nouveau nom.
pub fn rename(ancien: &str, nouveau: &str) -> Result<String, String> {
    let ancien = check_name(ancien)?;
    let nouveau = check_name(nouveau)?;
    if is_builtin(nouveau) {
        return Err(format!("« {nouveau} » est un préréglage : choisis un autre nom"));
    }
    let cible = fichier(nouveau);
    // même nom à la casse près : Windows accepte le renommage
    if cible.exists() && !ancien.eq_ignore_ascii_case(nouveau) {
        return Err(format!("un preset s'appelle déjà « {nouveau} »"));
    }
    std::fs::rename(fichier(ancien), &cible).map_err(|_| format!("aucun profil « {ancien} »"))?;
    Ok(nouveau.to_owned())
}

/// Copie un preset (fourni ou non) sous « nom (copie) » ; renvoie le nom de la copie.
pub fn dupliquer(name: &str) -> Result<String, String> {
    let liste = list();
    let p = liste.iter().find(|p| p.name == name).ok_or_else(|| format!("aucun preset « {name} »"))?;
    let base: String = name.chars().take(30).collect();
    let pris = |n: &str| liste.iter().any(|q| q.name.eq_ignore_ascii_case(n));
    let nom = (1..100)
        .map(|i| if i == 1 { format!("{base} (copie)") } else { format!("{base} (copie {i})") })
        .find(|n| !pris(n))
        .ok_or("trop de copies")?;
    save(&nom, &p.cfg)
}

/// Importe un fichier de preset (.toml) sous le nom du fichier ; renvoie ce nom.
pub fn importer(chemin: &std::path::Path) -> Result<String, String> {
    let nom = chemin.file_stem().map(|s| s.to_string_lossy().into_owned()).ok_or("fichier sans nom")?;
    if chemin.extension().is_none_or(|e| !e.eq_ignore_ascii_case("toml")) {
        return Err(format!("« {nom} » n'est pas un preset (.toml attendu)"));
    }
    let cfg = Config::read(chemin).map_err(|e| format!("« {nom} » illisible : {e}"))?;
    save(&nom, &cfg)
}

/// Nombre de réglages de profil qui diffèrent entre `a` et `b`.
pub fn ecarts(a: &Config, b: &Config) -> usize {
    let (Ok(a), Ok(b)) = (serde_json::to_value(a), serde_json::to_value(b)) else { return usize::MAX };
    let mut n = 0;
    for (s, keys) in a.as_object().into_iter().flatten() {
        for (k, v) in keys.as_object().into_iter().flatten().filter(|(k, _)| in_profile(s, k)) {
            let w = b.get(s).and_then(|y| y.get(k));
            let pareil = match (v, w) {
                (serde_json::Value::Number(m), Some(serde_json::Value::Number(n))) => {
                    let (m, n) = (m.as_f64().unwrap_or(0.0), n.as_f64().unwrap_or(0.0));
                    (m - n).abs() <= 1e-3 * m.abs().max(n.abs()).max(1.0)
                }
                (serde_json::Value::String(x), Some(serde_json::Value::String(y))) if x.is_empty() || y.is_empty() => true,
                (v, w) => Some(v) == w,
            };
            n += usize::from(!pareil);
        }
    }
    n
}

pub fn delete(name: &str) -> Result<(), String> {
    let name = check_name(name)?;
    if is_builtin(name) {
        return Err(format!("« {name} » est un préréglage : il ne peut pas être supprimé"));
    }
    let p = dir().join(format!("{name}.toml"));
    std::fs::remove_file(&p).map_err(|_| format!("aucun profil « {name} »"))
}

/// Écrit le profil dans `config.toml`. Un profil sans jeu de curseurs garde celui des réglages.
pub fn apply(p: &Profile) -> Result<(), String> {
    let mut cfg = p.cfg.clone();
    if cfg.souris.jeu.is_empty() {
        cfg.souris.jeu = Config::read(&config::path()).map(|c| c.souris.jeu).unwrap_or_default();
    }
    config::apply_keys(&cfg, in_profile)
}

/// Les réglages de profil de `a` et `b` sont-ils identiques (aux arrondis près) ?
pub fn same(a: &Config, b: &Config) -> bool {
    let (Ok(a), Ok(b)) = (serde_json::to_value(a), serde_json::to_value(b)) else { return false };
    a.as_object().into_iter().flatten().all(|(s, keys)| {
        keys.as_object().into_iter().flatten().filter(|(k, _)| in_profile(s, k)).all(|(k, v)| {
            match (v, b.get(s).and_then(|y| y.get(k))) {
                // jeu de curseurs non précisé d'un côté : il ne départage pas
                (serde_json::Value::String(x), Some(serde_json::Value::String(y))) if x.is_empty() || y.is_empty() => true,
                (serde_json::Value::Number(m), Some(serde_json::Value::Number(n))) => {
                    let (m, n) = (m.as_f64().unwrap_or(0.0), n.as_f64().unwrap_or(0.0));
                    (m - n).abs() <= 1e-3 * m.abs().max(n.abs()).max(1.0)
                }
                (v, w) => Some(v) == w,
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecarts_entre_reglages() {
        let a = Config::default();
        let mut b = Config::default();
        assert_eq!(ecarts(&a, &b), 0);
        b.orbite.rayon = 40.0;
        b.essaim.maximum = 3;
        b.rendu.cadence = 60; // hors profil
        assert_eq!(ecarts(&a, &b), 2);
    }

    #[test]
    fn noms_refuses() {
        assert!(check_name("  ").is_err());
        assert!(check_name("a/b").is_err());
        assert!(check_name("fin.").is_err());
        assert_eq!(check_name(" Soirée ").unwrap(), "Soirée");
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

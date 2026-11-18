//! Profils : jeux de réglages d'apparence et de réaction au son, à appliquer d'un clic.
//!
//! Quatre préréglages intégrés (non modifiables) et les profils de l'utilisateur, un fichier
//! `%APPDATA%\Essaim\profils\<nom>.toml` chacun. Appliquer un profil réécrit ses réglages dans
//! `config.toml` (commentaires gardés). Les choix propres à la machine n'en font pas partie :
//! `[rendu]`, `[diagnostic]` et le remplacement des curseurs de Windows (`[curseur]` sauf la
//! taille du point).

use crate::config::{self, Config};
use std::path::PathBuf;

/// Sections enregistrées dans un profil.
const SECTIONS: [&str; 7] = ["curseur", "orbite", "essaim", "couleur", "fissures", "audio", "bordure"];
/// Réglages de ces sections qui restent hors des profils.
const MACHINE: [(&str, &str); 3] = [("curseur", "remplacer_curseurs"), ("curseur", "variantes"), ("curseur", "taille_windows")];

/// Le réglage `[section] key` fait-il partie des profils ?
pub fn in_profile(section: &str, key: &str) -> bool {
    SECTIONS.contains(&section) && !MACHINE.contains(&(section, key))
}

pub struct Profile {
    pub name: String,
    pub builtin: bool,
    pub cfg: Config,
}

fn dir() -> PathBuf {
    config::dir().join("profils")
}

fn builtins() -> Vec<Profile> {
    let make = |name: &str, change: fn(&mut Config)| {
        let mut cfg = Config::default();
        change(&mut cfg);
        Profile { name: name.to_owned(), builtin: true, cfg }
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
        }),
        make("Point seul", |c| {
            c.orbite.cometes = false;
            c.essaim.maximum = 0;
            c.fissures.actives = false;
            c.bordure.active = false;
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
                Ok(cfg) => Some(Profile { name, builtin: false, cfg }),
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

pub fn delete(name: &str) -> Result<(), String> {
    let name = check_name(name)?;
    if is_builtin(name) {
        return Err(format!("« {name} » est un préréglage : il ne peut pas être supprimé"));
    }
    let p = dir().join(format!("{name}.toml"));
    std::fs::remove_file(&p).map_err(|_| format!("aucun profil « {name} »"))
}

/// Écrit le profil dans `config.toml`.
pub fn apply(p: &Profile) -> Result<(), String> {
    config::apply_keys(&p.cfg, in_profile)
}

/// Les réglages de profil de `a` et `b` sont-ils identiques (aux arrondis près) ?
pub fn same(a: &Config, b: &Config) -> bool {
    let (Ok(a), Ok(b)) = (serde_json::to_value(a), serde_json::to_value(b)) else { return false };
    a.as_object().into_iter().flatten().all(|(s, keys)| {
        keys.as_object().into_iter().flatten().filter(|(k, _)| in_profile(s, k)).all(|(k, v)| {
            match (v, b.get(s).and_then(|y| y.get(k))) {
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
        // hors profil : non enregistrés
        cfg.rendu.cadence = 60;
        cfg.curseur.remplacer_curseurs = false;
        save_in(&d, "Essai", &cfg).unwrap();
        let text = std::fs::read_to_string(d.join("Essai.toml")).unwrap();
        let back = Config::read(&d.join("Essai.toml")).unwrap();
        assert!(same(&cfg, &back));
        assert_eq!(back.rendu.cadence, 0);
        assert!(!text.contains("remplacer_curseurs") && !text.contains("[rendu]"), "{text}");
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

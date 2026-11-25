//! Preset automatique (`[automatique]`) : règles « si … alors … » (artiste, titre, application,
//! tempo, heure ; prioritaires), puis application au premier plan, puis heure. Quand plus aucune
//! règle ne s'applique, les réglages d'avant reviennent, sauf ceux que l'utilisateur a retouchés
//! pendant la règle. Ces réglages d'avant sont aussi écrits dans `auto-avant.toml` : un arrêt
//! d'Essaim pendant une règle ne les perd pas.

use crate::config::{Config, Horaire, Regle};
use crate::profils::{self, Avant, Echec};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Un changement de preset attend que sa cible soit stable ce temps-là (une fenêtre traversée
/// d'un Alt+Tab ne change rien)…
const STABLE: Duration = Duration::from_secs(1);
/// … et celui dû à la musique (artiste, titre, tempo), ce temps-là.
const STABLE_MUSIQUE: Duration = Duration::from_secs(2);
/// Réglages d'avant repris au démarrage : la règle n'est tenue pour finie qu'une fois le tempo
/// relu et ce temps passé…
const STABLE_REPRISE: Duration = Duration::from_secs(10);
/// … ou, tempo encore inconnu (il faut quelques secondes de musique, et la capture peut démarrer
/// tard), ce temps-là : sans lui, une règle sur le tempo serait retirée puis remise.
const STABLE_REPRISE_SANS_TEMPO: Duration = Duration::from_secs(20);

/// Fichier des réglages d'avant une règle en cours.
fn fichier_avant() -> PathBuf {
    crate::config::dir().join("auto-avant.toml")
}

#[derive(Default)]
pub struct Auto {
    /// Preset mis par une règle, et si cette règle parlait de la musique.
    courant: Option<String>,
    courant_musical: bool,
    /// Cible vue en attente de stabilité, et depuis quand.
    attente: Option<(Option<String>, Instant)>,
    /// Règles illisibles déjà notées au journal.
    illisibles: Vec<String>,
    /// Réglages d'avant la première règle, et juste après le dernier preset appliqué.
    avant: Option<Avant>,
    apres: Option<Avant>,
    prochain: Option<Instant>,
    /// Échec déjà noté au journal (réessayé à chaque tour sans le répéter).
    echec: bool,
    /// Fichier des réglages d'avant déjà relu au démarrage, et réglages d'avant repris de ce
    /// fichier sans décision encore prise.
    relu: bool,
    repris: bool,
    /// Dernier tempo lu pendant que le son était écouté.
    bpm: Option<f32>,
}

/// Minutes depuis minuit d'une heure « HH:MM ».
fn minutes(h: &str) -> Option<u32> {
    let (a, b) = h.trim().split_once(':')?;
    let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
    (a < 24 && b < 60).then_some(a * 60 + b)
}

/// Preset de la plage horaire qui contient `maintenant` (minutes depuis minuit) ; une plage
/// dont la fin précède le début passe minuit.
pub fn horaire(regles: &[Horaire], maintenant: u32) -> Option<&str> {
    regles.iter().find_map(|r| {
        let (d, f) = (minutes(&r.debut)?, minutes(&r.fin)?);
        let dedans = if d <= f { (d..f).contains(&maintenant) } else { maintenant >= d || maintenant < f };
        dedans.then_some(r.preset.as_str())
    })
}

fn heure_locale() -> u32 {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    t.wHour as u32 * 60 + t.wMinute as u32
}

/// Écrit les réglages d'avant une règle (et ceux d'après son dernier preset) dans `p`, d'un coup.
fn sauver_avant_in(p: &Path, avant: &Avant, apres: Option<&Avant>) -> Result<(), String> {
    let texte = profils::avant_en_texte(avant, apres)?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    crate::config::ecrire_atomique(p, &texte).map_err(|e| e.to_string())
}

/// Relit les réglages gardés dans `p` ; `None` sans fichier. Un fichier abîmé est noté au
/// journal et retiré (il ne servirait à rien de le relire à chaque démarrage).
fn lire_avant_in(p: &Path) -> Option<(Avant, Option<Avant>)> {
    let texte = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            log::warn!("automatique : {} illisible ({e}), réglages d'avant non repris", p.display());
            return None;
        }
    };
    match profils::avant_de_texte(&texte) {
        Ok(r) => Some(r),
        Err(e) => {
            log::warn!("automatique : {} abîmé ({e}), retiré", p.display());
            effacer_avant_in(p);
            None
        }
    }
}

/// Retire le fichier des réglages d'avant (fin de la règle).
fn effacer_avant_in(p: &Path) {
    if let Err(e) = std::fs::remove_file(p) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("automatique : {} non retiré ({e})", p.display());
        }
    }
}

/// Capture du son relancée depuis moins de ce temps : un tempo encore inconnu ne compte pas (il
/// faut quelques secondes de musique pour le mesurer).
const TEMPO_RELU: Duration = Duration::from_secs(8);

impl Auto {
    /// Garde sur le disque les réglages d'avant en cours ; un échec n'empêche pas la règle.
    fn sauver(&self) {
        let Some(avant) = &self.avant else { return };
        if let Err(e) = sauver_avant_in(&fichier_avant(), avant, self.apres.as_ref()) {
            log::warn!("automatique : réglages d'avant gardés en mémoire seulement ({e})");
        }
    }

    /// À appeler à chaque image ; ne regarde qu'une fois toutes les 0,7 s. `bpm` : tempo des
    /// kicks, quand il est stable ; `ecoute` : depuis quand la capture du son tourne (`None` :
    /// coupée, en pause ou en plein écran). Capture coupée ou tout juste relancée, le tempo n'est
    /// pas mesuré : les conditions sur le tempo gardent la dernière valeur lue.
    pub fn tick(&mut self, cfg: &Config, now: Instant, bpm: Option<f32>, ecoute: Option<Instant>) {
        if self.prochain.is_some_and(|p| now < p) {
            return;
        }
        self.prochain = Some(now + Duration::from_millis(700));
        if !self.relu {
            self.relu = true;
            if let Some((avant, apres)) = lire_avant_in(&fichier_avant()) {
                log::info!("automatique : réglages d'avant la règle repris (Essaim arrêté pendant une règle)");
                (self.avant, self.apres, self.repris) = (Some(avant), apres, true);
            }
        }
        if ecoute.is_some_and(|depuis| bpm.is_some() || now.duration_since(depuis) >= TEMPO_RELU) {
            self.bpm = bpm;
        }
        let a = &cfg.automatique;
        let (cible, musicale, tempo) = if a.actif {
            // fenêtre d'Essaim ou sans exécutable lisible : rien ne change
            let Some(exe) = crate::appli::nom_premier_plan() else { return };
            self.signaler_illisibles(&a.regles);
            let minutes = heure_locale();
            let morceau = if crate::media::en_lecture() == Some(true) { crate::media::morceau() } else { None };
            let ctx = Contexte { appli: Some(exe.clone()), morceau: morceau.map(|m| (m.titre, m.artiste)), bpm: self.bpm, minutes };
            match regle_vraie(&a.regles, &ctx) {
                Some(c) => (Some(c.preset.to_owned()), c.musicale, c.tempo),
                None => (
                    a.applis
                        .iter()
                        .find(|r| r.appli.trim().eq_ignore_ascii_case(&exe))
                        .map(|r| r.preset.clone())
                        .or_else(|| horaire(&a.horaires, minutes).map(str::to_owned)),
                    false,
                    false,
                ),
            }
        } else {
            (None, false, false)
        };
        // rien à faire, sauf des réglages d'avant repris au démarrage alors que plus aucune règle
        // ne s'applique : ils reviennent comme à une fin de règle
        if cible == self.courant && !(cible.is_none() && self.avant.is_some()) {
            self.attente = None;
            return;
        }
        // un changement n'est fait que s'il tient un moment
        if a.actif {
            let stable = if self.repris && cible.is_none() {
                if self.bpm.is_some() { STABLE_REPRISE } else { STABLE_REPRISE_SANS_TEMPO }
            } else if musicale || self.courant_musical {
                STABLE_MUSIQUE
            } else {
                STABLE
            };
            match &self.attente {
                Some((vue, depuis)) if *vue == cible => {
                    if now.duration_since(*depuis) < stable {
                        return;
                    }
                }
                _ => {
                    self.attente = Some((cible, now));
                    return;
                }
            }
        }
        self.attente = None;
        match &cible {
            Some(nom) => {
                if self.avant.is_none() {
                    // réglages illisibles : rien n'est appliqué, nouvel essai au tour suivant
                    let Some(avant) = profils::capturer() else { return self.echoue("réglages d'avant illisibles") };
                    (self.avant, self.apres) = (Some(avant), None);
                    self.sauver();
                }
                // règle sur le tempo : le preset ne touche pas à `[audio]`, qui change la mesure
                // du tempo (sinon la règle pourrait se défaire et se refaire sans fin)
                match profils::appliquer_detaille(nom, tempo) {
                    Ok(()) => log::info!("automatique : preset « {nom} »"),
                    // rien n'a été écrit (fichier verrouillé…) : nouvel essai au tour suivant
                    Err(Echec::Reglages(e)) => return self.echoue(&format!("preset « {nom} » non appliqué ({e})")),
                    // aucun preset de ce nom, curseurs refusés : noté, la règle compte comme faite
                    Err(e) => log::warn!("automatique : preset « {nom} » non appliqué ({e})"),
                }
                self.apres = profils::capturer();
                self.sauver();
            }
            None => {
                if let Some(avant) = &self.avant {
                    let r = match &self.apres {
                        Some(apres) => profils::remettre_partiel(avant, apres),
                        // état d'après inconnu : tout revient
                        None => profils::remettre(avant),
                    };
                    if let Err(e) = r {
                        return self.echoue(&format!("retour impossible ({e})"));
                    }
                    log::info!("automatique : réglages d'avant remis");
                    (self.avant, self.apres) = (None, None);
                    effacer_avant_in(&fichier_avant());
                }
            }
        }
        self.repris = false;
        self.echec = false;
        self.courant = cible;
        self.courant_musical = musicale;
    }

    /// Note au journal (une fois chacune) les règles dont la condition est illisible.
    fn signaler_illisibles(&mut self, regles: &[Regle]) {
        for r in regles {
            if let Err(e) = analyser(&r.si) {
                if !self.illisibles.contains(&r.si) {
                    log::warn!("automatique : règle « si {} alors {} » ignorée ({e})", r.si, r.alors);
                    self.illisibles.push(r.si.clone());
                }
            }
        }
    }

    /// Note l'échec une fois ; `courant` ne bouge pas, donc le tour suivant réessaie.
    fn echoue(&mut self, quoi: &str) {
        if !self.echec {
            self.echec = true;
            log::warn!("automatique : {quoi}, nouvel essai dans un instant");
        }
    }
}


// ---------------------------------------------------------------------------------------------
// Règles « si … alors … » (`[automatique] regles`) : `si` est une suite de conditions jointes
// par « et », chacune `clé opérateur valeur` (`artiste = Skrillex`, `bpm > 140`,
// `appli = obs64.exe`, `titre ~ remix`, `heure >= 22:00`).

/// Ce qu'une condition peut regarder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cle {
    /// Artiste du morceau qui joue (pas en pause).
    Artiste,
    /// Titre du morceau qui joue.
    Titre,
    /// Exécutable au premier plan.
    Appli,
    /// Tempo des kicks, quand il est stable.
    Bpm,
    /// Heure locale (« HH:MM »).
    Heure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Egal,
    Different,
    Contient,
    Inf,
    Sup,
    InfEgal,
    SupEgal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub cle: Cle,
    pub op: Op,
    /// Texte en minuscules (clés de texte), ou nombre (tempo, minutes depuis minuit).
    pub texte: String,
    pub nombre: f32,
}

/// Ce que voient les règles à cet instant.
#[derive(Clone, Debug, Default)]
pub struct Contexte {
    pub appli: Option<String>,
    /// Morceau en cours de lecture (titre, artiste) ; `None` en pause ou sans morceau.
    pub morceau: Option<(String, String)>,
    pub bpm: Option<f32>,
    pub minutes: u32,
}

/// Minuscules, espaces de bord retirés, et guillemets autour retirés.
fn propre(v: &str) -> String {
    let v = v.trim();
    let v = v.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(v);
    v.trim().to_lowercase()
}

/// Analyse le texte `si` d'une règle ; une erreur dit ce qui ne va pas, en clair.
pub fn analyser(si: &str) -> Result<Vec<Condition>, String> {
    if si.trim().is_empty() {
        return Err("condition vide".into());
    }
    // « et » entouré d'espaces sépare les conditions, hors des guillemets (« "Simon et
    // Garfunkel" » reste un seul nom)
    let mut morceaux = Vec::new();
    let (mut debut, mut dans_guillemets) = (0, false);
    let octets = si.as_bytes();
    let et = |i: usize| octets.len() >= i + 4 && octets[i] == b' ' && octets[i + 1].eq_ignore_ascii_case(&b'e') && octets[i + 2].eq_ignore_ascii_case(&b't') && octets[i + 3] == b' ';
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'"' {
            dans_guillemets = !dans_guillemets;
        } else if !dans_guillemets && et(i) {
            morceaux.push(&si[debut..i]);
            i += 4;
            debut = i;
            continue;
        }
        i += 1;
    }
    morceaux.push(&si[debut..]);
    morceaux.into_iter().map(condition).collect()
}

fn condition(texte: &str) -> Result<Condition, String> {
    // opérateur le plus à gauche (le plus long à égalité : « <= » avant « < »)
    const OPS: [(&str, Op); 7] =
        [("<=", Op::InfEgal), (">=", Op::SupEgal), ("!=", Op::Different), ("=", Op::Egal), ("~", Op::Contient), ("<", Op::Inf), (">", Op::Sup)];
    let (pos, sym, op) = OPS
        .iter()
        .filter_map(|&(s, op)| texte.find(s).map(|p| (p, s, op)))
        .min_by_key(|&(p, s, _)| (p, std::cmp::Reverse(s.len())))
        .ok_or_else(|| format!("« {} » : il manque un opérateur (=, !=, ~, <, >)", texte.trim()))?;
    let (gauche, droite) = (propre(&texte[..pos]), propre(&texte[pos + sym.len()..]));
    let cle = match gauche.as_str() {
        "artiste" | "artist" => Cle::Artiste,
        "titre" | "morceau" | "title" => Cle::Titre,
        "appli" | "application" | "app" => Cle::Appli,
        "bpm" | "tempo" => Cle::Bpm,
        "heure" => Cle::Heure,
        autre => return Err(format!("« {autre} » : clé inconnue (artiste, titre, appli, bpm ou heure)")),
    };
    if droite.is_empty() {
        return Err(format!("« {} » : valeur manquante", texte.trim()));
    }
    let nombre = match cle {
        Cle::Bpm => droite.replace(',', ".").parse::<f32>().ok().filter(|b| b.is_finite()).ok_or_else(|| format!("« {droite} » : tempo attendu (un nombre)"))?,
        Cle::Heure => minutes(&droite).ok_or_else(|| format!("« {droite} » : heure attendue (HH:MM)"))? as f32,
        _ => 0.0,
    };
    let numerique = matches!(cle, Cle::Bpm | Cle::Heure);
    match op {
        Op::Contient if numerique => return Err(format!("« ~ » ne va qu'avec un texte ({})", texte.trim())),
        Op::Inf | Op::Sup | Op::InfEgal | Op::SupEgal if !numerique => {
            return Err(format!("« <, > » ne vont qu'avec bpm ou heure ({})", texte.trim()));
        }
        _ => {}
    }
    let texte = if cle == Cle::Appli && !droite.ends_with(".exe") { format!("{droite}.exe") } else { droite };
    Ok(Condition { cle, op, texte, nombre })
}

/// Artistes d'un champ « A, B & C feat. D » : chacun compte pour `artiste = …`.
fn artistes(champ: &str) -> impl Iterator<Item = &str> {
    champ.split([',', '&', ';', '/']).flat_map(|a| a.split(" feat. ")).flat_map(|a| a.split(" ft. ")).flat_map(|a| a.split(" x ")).map(str::trim)
}

impl Condition {
    /// Vraie à cet instant ? Une donnée absente (rien ne joue, tempo inconnu) la rend fausse.
    pub fn vraie(&self, c: &Contexte) -> bool {
        let texte = |valeur: Option<&str>| {
            let Some(v) = valeur.map(str::to_lowercase) else { return false };
            let egal = v == self.texte || (self.cle == Cle::Artiste && artistes(&v).any(|a| a == self.texte));
            match self.op {
                Op::Egal => egal,
                Op::Different => !egal,
                Op::Contient => v.contains(&self.texte),
                _ => false,
            }
        };
        let nombre = |valeur: Option<f32>, tolerance: f32| {
            let Some(v) = valeur else { return false };
            let n = self.nombre;
            match self.op {
                Op::Egal => (v - n).abs() <= tolerance,
                Op::Different => (v - n).abs() > tolerance,
                Op::Inf => v < n,
                Op::Sup => v > n,
                Op::InfEgal => v <= n + tolerance,
                Op::SupEgal => v >= n - tolerance,
                Op::Contient => false,
            }
        };
        match self.cle {
            Cle::Artiste => texte(c.morceau.as_ref().map(|m| m.1.as_str())),
            Cle::Titre => texte(c.morceau.as_ref().map(|m| m.0.as_str())),
            Cle::Appli => texte(c.appli.as_deref()),
            // à 2 BPM près pour « = » (le tempo mesuré varie un peu)
            Cle::Bpm => nombre(c.bpm, 2.0),
            Cle::Heure => nombre(Some(c.minutes as f32), 0.0),
        }
    }
}

/// La règle parle-t-elle de la musique (artiste, titre, tempo) ? Un changement de preset dû à la
/// musique attend qu'elle soit stable 2 s (un tempo qui hésite autour d'un seuil ne fait pas
/// clignoter les presets).
pub fn musicale(conditions: &[Condition]) -> bool {
    conditions.iter().any(|c| matches!(c.cle, Cle::Artiste | Cle::Titre | Cle::Bpm))
}

/// Règle retenue : son preset, et ce dont elle parle.
#[derive(Debug, PartialEq, Eq)]
pub struct Choix<'a> {
    pub preset: &'a str,
    /// Elle parle de la musique (artiste, titre, tempo).
    pub musicale: bool,
    /// Elle parle du tempo : son preset n'écrit pas `[audio]`, qui change la mesure du tempo.
    pub tempo: bool,
}

/// Première règle dont toutes les conditions sont vraies. Les règles illisibles sont ignorées.
pub fn regle_vraie<'a>(regles: &'a [Regle], c: &Contexte) -> Option<Choix<'a>> {
    regles.iter().find_map(|r| {
        let conditions = analyser(&r.si).ok()?;
        conditions.iter().all(|x| x.vraie(c)).then(|| Choix {
            preset: r.alors.as_str(),
            musicale: musicale(&conditions),
            tempo: conditions.iter().any(|x| x.cle == Cle::Bpm),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(appli: &str, morceau: Option<(&str, &str)>, bpm: Option<f32>, minutes: u32) -> Contexte {
        Contexte { appli: Some(appli.into()), morceau: morceau.map(|(t, a)| (t.into(), a.into())), bpm, minutes }
    }

    #[test]
    fn conditions_lues_et_refusees() {
        let c = analyser("artiste = Skrillex").unwrap();
        assert_eq!((c[0].cle, c[0].op, c[0].texte.as_str()), (Cle::Artiste, Op::Egal, "skrillex"));
        let c = analyser("BPM >= 140 et appli = jeu").unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].cle, c[0].op, c[0].nombre), (Cle::Bpm, Op::SupEgal, 140.0));
        assert_eq!(c[1].texte, "jeu.exe");
        let c = analyser("artiste = \"Simon et Garfunkel\"").unwrap();
        assert_eq!((c.len(), c[0].texte.as_str()), (1, "simon et garfunkel"));
        assert_eq!(analyser("heure < 7:30").unwrap()[0].nombre, 450.0);
        for faux in ["", "artiste Skrillex", "genre = rock", "bpm > vite", "artiste > A", "bpm ~ 120", "heure > 25:00", "titre ="] {
            assert!(analyser(faux).is_err(), "« {faux} » accepté");
        }
    }

    #[test]
    fn conditions_vraies_selon_le_contexte() {
        let vrai = |si: &str, c: &Contexte| analyser(si).unwrap().iter().all(|x| x.vraie(c));
        let skrillex = ctx("spotify.exe", Some(("Bangarang", "Skrillex, Sirah")), Some(110.0), 23 * 60);
        assert!(vrai("artiste = skrillex", &skrillex), "un des artistes");
        assert!(vrai("artiste ~ krill", &skrillex));
        assert!(vrai("titre = bangarang et heure >= 22:00", &skrillex));
        assert!(!vrai("artiste != Skrillex", &skrillex));
        assert!(vrai("bpm = 111", &skrillex), "à 2 BPM près");
        assert!(!vrai("bpm > 140", &skrillex));
        assert!(vrai("appli = spotify", &skrillex));
        // rien ne joue, tempo inconnu : les conditions sur la musique sont fausses
        let silence = ctx("code.exe", None, None, 12 * 60);
        for si in ["artiste = Skrillex", "artiste != Skrillex", "bpm < 200", "titre ~ a"] {
            assert!(!vrai(si, &silence), "« {si} » vraie sans musique");
        }
        let regles = [
            Regle { si: "artiste = Skrillex".into(), alors: "Triangle".into() },
            Regle { si: "faux".into(), alors: "X".into() },
            Regle { si: "appli = spotify.exe".into(), alors: "Calme".into() },
        ];
        assert_eq!(regle_vraie(&regles, &skrillex), Some(Choix { preset: "Triangle", musicale: true, tempo: false }));
        assert_eq!(regle_vraie(&regles[1..], &skrillex), Some(Choix { preset: "Calme", musicale: false, tempo: false }));
        assert_eq!(regle_vraie(&regles, &silence), None);
        let tempo = [Regle { si: "bpm < 120 et appli = spotify".into(), alors: "Lent".into() }];
        assert_eq!(regle_vraie(&tempo, &skrillex), Some(Choix { preset: "Lent", musicale: true, tempo: true }));
    }

    #[test]
    fn reglages_d_avant_gardes_sur_le_disque() {
        let d = std::env::temp_dir().join(format!("essaim-auto-avant-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let p = d.join("auto-avant.toml");
        assert!(lire_avant_in(&p).is_none(), "sans fichier");
        let mut cfg = Config::default();
        cfg.orbite.rayon = 33.0;
        let avant = Avant::essai(cfg.clone());
        sauver_avant_in(&p, &avant, None).unwrap();
        let (relu, apres) = lire_avant_in(&p).expect("relu");
        assert!(profils::same(&relu.reglages(), &cfg) && apres.is_none());
        // après le premier preset : les deux sont gardés
        let mut ensuite = Config::default();
        ensuite.orbite.rayon = 50.0;
        sauver_avant_in(&p, &avant, Some(&Avant::essai(ensuite.clone()))).unwrap();
        let (_, apres) = lire_avant_in(&p).unwrap();
        assert!(apres.is_some_and(|a| profils::same(&a.reglages(), &ensuite)));
        effacer_avant_in(&p);
        assert!(!p.exists() && lire_avant_in(&p).is_none());
        effacer_avant_in(&p); // déjà retiré : rien
        // fichier abîmé : ignoré et retiré
        std::fs::write(&p, "avant = 3").unwrap();
        assert!(lire_avant_in(&p).is_none() && !p.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn plages_horaires() {
        let r = |d: &str, f: &str, p: &str| Horaire { debut: d.into(), fin: f.into(), preset: p.into() };
        let regles = [r("22:00", "07:00", "Nuit"), r("12:00", "14:00", "Midi"), r("25:00", "26:00", "Faux")];
        assert_eq!(horaire(&regles, 23 * 60), Some("Nuit"));
        assert_eq!(horaire(&regles, 6 * 60 + 59), Some("Nuit"));
        assert_eq!(horaire(&regles, 7 * 60), None);
        assert_eq!(horaire(&regles, 13 * 60), Some("Midi"));
        assert_eq!(horaire(&regles, 14 * 60), None);
    }
}

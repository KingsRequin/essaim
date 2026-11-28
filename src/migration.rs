//! Réglages d'avant les modulations et les déclencheurs (Essaim 0.3 et 0.4) : les réactions
//! fixes (`[orbite] vitesse_basse`, `[musique] montee`, `[clic] onde`, `[fissures] actives`…)
//! deviennent des modulations et des déclencheurs, avec leurs valeurs, pour que l'essaim reste
//! le même. Les réglages par défaut en sortent aussi (`Ancien::default`) : un preset sans ces
//! listes donne l'essaim de toujours.

use crate::config::{Action, Cible, Declencheur, Evenement, Groupe, Modulation, Source, LISSAGE};
use crate::modulation::{ACCELERATION, RAYON, TAILLE, TEINTE, VITESSE};
use serde::Deserialize;

/// Clés retirées, converties en modulations, en déclencheurs ou déplacées dans `[particules]`.
pub const ANCIENNES_CLES: [(&str, &str); 21] = [
    ("orbite", "vitesse_basse"),
    ("orbite", "eloignement_grave"),
    ("orbite", "inverse_au_kick"),
    ("essaim", "par_kick_min"),
    ("essaim", "par_kick_max"),
    ("scintillement", "actif"),
    ("scintillement", "quantite"),
    ("scintillement", "rebonds"),
    ("clic", "onde"),
    ("clic", "eclats"),
    ("clic", "taille"),
    ("clic", "droit"),
    ("fissures", "actives"),
    ("bordure", "intensite"),
    ("couleur", "musique"),
    ("musique", "rafale_piste"),
    ("musique", "montee"),
    ("musique", "caisse_claire"),
    ("musique", "contretemps"),
    ("musique", "stereo"),
    ("audio", "pulsation_tempo"),
];
/// Sections qui n'existent plus (retirées si elles sont vides après la conversion).
pub const ANCIENNES_SECTIONS: [&str; 3] = ["scintillement", "clic", "musique"];

/// Les réglages d'avant, avec leurs valeurs par défaut d'alors (les presets fournis jusqu'à la
/// 0.6 en partent aussi, `prereglages::presets_d_avant`).
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Ancien {
    pub(crate) orbite: AOrbite,
    pub(crate) essaim: AEssaim,
    pub(crate) scintillement: AScintillement,
    pub(crate) clic: AClic,
    pub(crate) fissures: AFissures,
    pub(crate) bordure: ABordure,
    pub(crate) couleur: ACouleur,
    pub(crate) musique: AMusique,
    pub(crate) audio: AAudio,
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub(crate) struct AOrbite {
    pub(crate) vitesse_basse: f32,
    pub(crate) eloignement_grave: f32,
    pub(crate) inverse_au_kick: bool,
}
impl Default for AOrbite {
    fn default() -> Self {
        Self { vitesse_basse: 11.0, eloignement_grave: 1.3, inverse_au_kick: true }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub(crate) struct AEssaim {
    pub(crate) par_kick_min: u32,
    pub(crate) par_kick_max: u32,
}
impl Default for AEssaim {
    fn default() -> Self {
        Self { par_kick_min: 2, par_kick_max: 3 }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub(crate) struct AScintillement {
    pub(crate) actif: bool,
    pub(crate) quantite: f32,
}
impl Default for AScintillement {
    fn default() -> Self {
        Self { actif: true, quantite: 4.0 }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub(crate) struct AClic {
    pub(crate) onde: bool,
    pub(crate) eclats: bool,
    pub(crate) droit: bool,
}
impl Default for AClic {
    fn default() -> Self {
        Self { onde: true, eclats: false, droit: true }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub(crate) struct AFissures {
    pub(crate) actives: bool,
}
impl Default for AFissures {
    fn default() -> Self {
        Self { actives: true }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub(crate) struct ABordure {
    pub(crate) intensite: f32,
}
impl Default for ABordure {
    fn default() -> Self {
        Self { intensite: 1.0 }
    }
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub(crate) struct ACouleur {
    pub(crate) musique: bool,
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub(crate) struct AMusique {
    pub(crate) rafale_piste: bool,
    pub(crate) montee: bool,
    pub(crate) caisse_claire: bool,
    pub(crate) contretemps: bool,
    pub(crate) stereo: f32,
}
impl Default for AMusique {
    fn default() -> Self {
        Self { rafale_piste: true, montee: true, caisse_claire: true, contretemps: false, stereo: 0.0 }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub(crate) struct AAudio {
    pub(crate) pulsation_tempo: bool,
}
impl Default for AAudio {
    fn default() -> Self {
        Self { pulsation_tempo: true }
    }
}

/// Modulation de quantité `q` (bornée, arrondie à 4 décimales).
fn m(source: Source, cible: Cible, sur: Groupe, q: f32, lissage: f32) -> Modulation {
    Modulation { source, cible, sur, quantite: ((q.clamp(-1.0, 1.0)) * 1e4).round() / 1e4, lissage, ..Default::default() }
}

impl Ancien {
    /// Modulations des réactions d'avant : `basse` = la basse qui accélère et la note grave qui
    /// éloigne l'orbite ; `reste` = la montée (plus serré, plus vite), le contretemps (têtes qui
    /// pulsent), la teinte qui suit la musique, le décalage stéréo. Une réaction nulle ou
    /// coupée est omise.
    pub fn modulations(&self, basse: bool, reste: bool) -> Vec<Modulation> {
        let mut l = Vec::new();
        let o = &self.orbite;
        if basse {
            if o.vitesse_basse != 0.0 {
                l.push(m(Source::Basse, Cible::Vitesse, Groupe::Tout, o.vitesse_basse / VITESSE, LISSAGE));
            }
            if o.eloignement_grave != 0.0 {
                l.push(m(Source::Grave, Cible::Rayon, Groupe::Tout, o.eloignement_grave / RAYON, LISSAGE));
            }
        }
        if !reste {
            return l;
        }
        let mu = &self.musique;
        if mu.montee {
            // vitesse × 2,2, orbite resserrée de 45 %
            // (la montée est déjà lissée : pas de lissage en plus)
            l.push(m(Source::Montee, Cible::Acceleration, Groupe::Tout, 1.2 / ACCELERATION, 0.0));
            l.push(m(Source::Montee, Cible::Rayon, Groupe::Tout, -0.45 / RAYON, 0.4));
        }
        if mu.contretemps {
            // têtes 50 % plus grosses d'un coup, orbites écartées de 6 %
            l.push(m(Source::Contretemps, Cible::Taille, Groupe::Tout, 0.5 / TAILLE, 0.0));
            l.push(m(Source::Contretemps, Cible::Rayon, Groupe::Tout, 0.06 / RAYON, 0.4));
        }
        if self.couleur.musique {
            // jusqu'à 160° : 60 % par la basse, 40 % par la brillance, lissé ~170 ms
            l.push(m(Source::Basse, Cible::Teinte, Groupe::Tout, 0.6 * 160.0 / TEINTE, 0.65));
            l.push(m(Source::Brillance, Cible::Teinte, Groupe::Tout, 0.4 * 160.0 / TEINTE, 0.65));
        }
        if mu.stereo > 0.0 {
            l.push(m(Source::Stereo, Cible::Decalage, Groupe::Tout, mu.stereo, 0.42));
        }
        l
    }

    /// Déclencheurs des réactions d'avant, dans l'ordre où elles se faisaient.
    pub fn declencheurs(&self) -> Vec<Declencheur> {
        let d = Declencheur::new;
        let (e, mu) = (&self.essaim, &self.musique);
        // le drop et le changement de piste : deux fois les satellites d'un kick fort
        let double = 2 * e.par_kick_min.max(e.par_kick_max).max(2);
        let mut l = Vec::new();
        if self.orbite.inverse_au_kick {
            l.push(d(Evenement::Kick, Action::Inversion));
        }
        l.push(d(Evenement::Kick, Action::Eclat));
        let (lo, hi) = (e.par_kick_min.min(e.par_kick_max), e.par_kick_max.max(e.par_kick_min));
        l.push(Declencheur { nombre: lo, nombre_max: hi, ..d(Evenement::Kick, Action::Satellites) });
        if self.bordure.intensite > 0.0 {
            l.push(Declencheur { force: self.bordure.intensite.clamp(0.0, 1.0), ..d(Evenement::Kick, Action::Bordure) });
        }
        if self.audio.pulsation_tempo {
            l.push(Declencheur { force: 0.35, ..d(Evenement::Break, Action::Eclat) });
        }
        if mu.caisse_claire {
            l.push(d(Evenement::Caisse, Action::Carre));
        }
        if mu.montee {
            l.push(Declencheur { nombre: double, nombre_max: double, ..d(Evenement::Drop, Action::Explosion) });
        }
        if self.scintillement.actif {
            let n = self.scintillement.quantite.clamp(1.0, 10.0).round() as u32;
            l.push(Declencheur { nombre: n, nombre_max: n, ..d(Evenement::Charleston, Action::Etincelles) });
        }
        if self.clic.onde {
            l.push(d(Evenement::Clic, Action::Onde));
        }
        if self.clic.eclats {
            l.push(d(Evenement::Clic, Action::Eclats));
        }
        if self.fissures.actives {
            l.push(d(Evenement::Clic, Action::Fissure));
        }
        if self.clic.droit {
            l.push(d(Evenement::ClicDroit, Action::Implosion));
        }
        if mu.rafale_piste {
            l.push(Declencheur { nombre: double, nombre_max: double, ..d(Evenement::Piste, Action::Rafale) });
        }
        l
    }
}

fn table<'a>(t: &'a mut toml::Table, section: &str) -> &'a mut toml::Table {
    let v = t.entry(section).or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !v.is_table() {
        *v = toml::Value::Table(toml::Table::new());
    }
    v.as_table_mut().expect("table")
}

/// Convertit une table TOML (fichier de réglages ou preset) d'avant les modulations ou les
/// déclencheurs. Les anciennes clés sont retirées ; sans `[declencheurs]`, les réactions
/// d'avant (valeurs du fichier, ou celles par défaut d'alors) deviennent les déclencheurs et
/// s'ajoutent aux modulations (sauf à une liste de modulations vidée exprès : seules les réactions
/// écrites dans la table comptent alors, une réaction non écrite, même active par défaut comme la
/// montée, est coupée partout, modulations et déclencheurs) ; sans `[modulations]`, ou avec les anciennes clés de la
/// basse, les réactions à la basse passent en tête des modulations. Une modulation de même
/// source, même cible et même groupe déjà là est gardée. Puis les réactions fixes des figures
/// d'avant la 0.26 deviennent des déclencheurs (`migrer_figures`). Renvoie vrai si la table a
/// changé.
pub fn migrer(t: &mut toml::Table) -> bool {
    let reactions = migrer_reactions(t);
    migrer_figures(t) || reactions
}

/// Clés de l'allure des figures d'avant la 0.26 : la force de leurs réactions. Celles des coups
/// deviennent la force des déclencheurs de leurs gestes (dans l'ordre de `config::GESTES`) ;
/// celles de la basse et de la montée, propres à la figure, sont retirées.
pub const REACTIONS_FIGURES: [&str; 6] = ["reaction_kick", "reaction_caisse", "reaction_charleston", "reaction_drop", "reaction_basse", "reaction_montee"];

/// Réactions des figures d'avant la 0.26, fixes et réglées par `reaction_kick`… : sans
/// `[declencheurs] figures`, chaque figure allumée reçoit les déclencheurs de ses gestes
/// (`config::declencheurs_de_figure`), à la force de sa réaction d'alors (aucun pour une réaction
/// à 0), et le drapeau est mis. Les clés `reaction_*` sont retirées partout. Renvoie vrai si la
/// table a changé.
pub fn migrer_figures(t: &mut toml::Table) -> bool {
    use crate::config::{declencheurs_de_figure, FIGURES};
    let deja = t.get("declencheurs").and_then(|d| d.get("figures")).and_then(toml::Value::as_bool) == Some(true);
    let mut change = false;
    let mut nouveaux = Vec::new();
    for (i, s) in FIGURES.iter().enumerate() {
        let Some(sec) = t.get_mut(*s).and_then(|v| v.as_table_mut()) else { continue };
        let mut forces = [1.0f32; 4];
        for (g, cle) in REACTIONS_FIGURES.iter().enumerate() {
            let Some(v) = sec.remove(*cle) else { continue };
            change = true;
            if let (Some(f), Some(force)) = (forces.get_mut(g), v.as_float().or_else(|| v.as_integer().map(|n| n as f64))) {
                *f = (force as f32).clamp(0.0, 2.0);
            }
        }
        if deja || !sec.get("actif").and_then(toml::Value::as_bool).unwrap_or(false) {
            continue;
        }
        for d in declencheurs_de_figure(i) {
            let force = d.faire.geste().map_or(1.0, |g| forces[g]);
            if force > 0.0 {
                nouveaux.extend(toml::Value::try_from(Declencheur { force, ..d }).ok());
            }
        }
    }
    if deja {
        return change;
    }
    let decl = table(t, "declencheurs");
    // une section sans liste a les déclencheurs par défaut : ils restent, ceux des figures suivent
    let liste = decl.entry("liste").or_insert_with(|| toml::Value::Array(Ancien::default().declencheurs().iter().filter_map(|d| toml::Value::try_from(d).ok()).collect()));
    if let Some(l) = liste.as_array_mut() {
        l.extend(nouveaux);
    }
    decl.insert("figures".into(), toml::Value::Boolean(true));
    true
}

/// Réactions fixes d'Essaim 0.3 et 0.4 (voir `migrer`).
fn migrer_reactions(t: &mut toml::Table) -> bool {
    let (avait_mod, avait_decl) = (t.contains_key("modulations"), t.contains_key("declencheurs"));
    let mut vieux = toml::Table::new();
    let mut basse_explicite = false;
    for (section, cle) in ANCIENNES_CLES {
        let Some(v) = t.get_mut(section).and_then(|s| s.as_table_mut()).and_then(|s| s.remove(cle)) else { continue };
        basse_explicite |= section == "orbite" && cle != "inverse_au_kick";
        table(&mut vieux, section).insert(cle.into(), v);
    }
    let trouve = !vieux.is_empty();
    for section in ANCIENNES_SECTIONS {
        if t.get(section).and_then(|s| s.as_table()).is_some_and(|s| s.is_empty()) {
            t.remove(section);
        }
    }
    if avait_decl && !trouve {
        return false;
    }
    // déplacées dans [particules] (si elles n'y sont pas déjà)
    for (section, cle, nouvelle) in [("scintillement", "rebonds", "rebonds"), ("clic", "taille", "taille")] {
        if let Some(v) = vieux.get(section).and_then(|s| s.get(cle)).cloned() {
            table(t, "particules").entry(nouvelle).or_insert(v);
        }
    }
    let ecrite = |section: &str, cle: &str| vieux.get(section).and_then(|s| s.get(cle)).is_some();
    // une liste vidée exprès n'en reçoit pas par défaut : seulement les réactions écrites dans la
    // table, une réaction non écrite y compte comme coupée (même active par défaut, la montée)
    let liste = match table(t, "modulations").remove("liste") {
        Some(toml::Value::Array(a)) => Some(a),
        _ => None,
    };
    let videe = liste.as_ref().is_some_and(Vec::is_empty);
    let mut ecrites = Ancien::deserialize(vieux.clone()).unwrap_or_default();
    if videe {
        let (o, mu) = (&mut ecrites.orbite, &mut ecrites.musique);
        if !ecrite("orbite", "vitesse_basse") {
            o.vitesse_basse = 0.0;
        }
        if !ecrite("orbite", "eloignement_grave") {
            o.eloignement_grave = 0.0;
        }
        mu.montee &= ecrite("musique", "montee");
        mu.contretemps &= ecrite("musique", "contretemps");
        ecrites.couleur.musique &= ecrite("couleur", "musique");
        if !ecrite("musique", "stereo") {
            mu.stereo = 0.0;
        }
    }
    let tete = if !avait_mod || basse_explicite { ecrites.modulations(true, false) } else { Vec::new() };
    let queue = if avait_decl { Vec::new() } else { ecrites.modulations(false, true) };
    let liste = liste.unwrap_or_default();
    fn nom<T: serde::Serialize>(x: T) -> Option<String> {
        toml::Value::try_from(x).ok().and_then(|v| v.as_str().map(str::to_owned))
    }
    // même source, même cible et même groupe (`sur` absent : « tout », sa valeur par défaut)
    let champ = |v: &toml::Value, cle: &str, defaut: Option<String>| v.get(cle).and_then(|x| x.as_str()).map(str::to_owned).or(defaut);
    let deja = |l: &[toml::Value], r: &Modulation| {
        l.iter().any(|v| {
            champ(v, "source", None) == nom(r.source) && champ(v, "cible", None) == nom(r.cible) && champ(v, "sur", nom(Groupe::default())) == nom(r.sur)
        })
    };
    let mut nouvelle: Vec<toml::Value> = tete.iter().filter(|r| !deja(&liste, r)).filter_map(|r| toml::Value::try_from(r).ok()).collect();
    nouvelle.extend(liste);
    for r in &queue {
        if !deja(&nouvelle, r) {
            nouvelle.extend(toml::Value::try_from(r).ok());
        }
    }
    table(t, "modulations").insert("liste".into(), toml::Value::Array(nouvelle));
    // une réaction non écrite comptée comme coupée l'est partout : la montée d'une liste vidée
    // n'amène pas non plus l'explosion au drop
    if !avait_decl {
        let l: Vec<toml::Value> = ecrites.declencheurs().iter().filter_map(|d| toml::Value::try_from(d).ok()).collect();
        table(t, "declencheurs").insert("liste".into(), toml::Value::Array(l));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reactions_des_figures_converties_en_gestes() {
        use crate::config::{Config, FIGURES};
        let lire = |texte: &str| -> (Config, bool, toml::Table) {
            let mut t: toml::Table = toml::from_str(texte).unwrap();
            let change = migrer(&mut t);
            (toml::Value::Table(t.clone()).try_into().unwrap(), change, t)
        };
        // réglages 0.25 : gouttes allumées, kick à moitié, drop coupé ; arcs éteints
        let (c, change, t) = lire("[declencheurs]\nliste = []\n[gouttes]\nactif = true\nreaction_kick = 0.5\nreaction_drop = 0\nreaction_basse = 2\n[arcs]\nreaction_kick = 0\n");
        assert!(change && c.declencheurs.figures);
        let g = FIGURES.iter().position(|&f| f == "gouttes").unwrap();
        let gestes: Vec<(Action, f32)> = c.declencheurs.liste.iter().filter(|d| d.sur == Groupe::de_figure(g)).map(|d| (d.faire, d.force)).collect();
        assert_eq!(gestes, [(Action::Battement, 0.5), (Action::Accent, 1.0)], "drop à 0 : pas d'apogée");
        assert!(c.declencheurs.liste.iter().all(|d| d.sur.figure() != Some(0)), "arcs éteints : rien");
        assert!(REACTIONS_FIGURES.iter().all(|k| t["gouttes"].get(*k).is_none() && t["arcs"].get(*k).is_none()));
        // une fois converties, plus rien : un geste retiré exprès ne revient pas
        let (_, change, _) = lire("[declencheurs]\nliste = []\nfigures = true\n[gouttes]\nactif = true\n");
        assert!(!change);
        // section sans liste : les déclencheurs par défaut restent, ceux des figures s'ajoutent
        let (c, _, _) = lire("[declencheurs]\n[pendule]\nactif = true\n");
        assert!(c.declencheurs.liste.len() == Ancien::default().declencheurs().len() + 3, "{:?}", c.declencheurs.liste);
    }

    fn modulations(texte: &str) -> Vec<Modulation> {
        let mut t: toml::Table = toml::from_str(texte).unwrap();
        migrer(&mut t);
        let c: crate::config::Config = toml::Value::Table(t).try_into().unwrap();
        c.modulations.liste
    }

    #[test]
    fn liste_videe_et_groupes_distincts() {
        // sans [declencheurs] ni [modulations] : les réactions de toujours
        assert_eq!(modulations("").len(), 4);
        // liste vidée exprès : elle le reste
        assert!(modulations("[modulations]\nliste = []").is_empty());
        // sauf pour une réaction d'avant écrite dans le preset
        let l = modulations("[modulations]\nliste = []\n[musique]\ncontretemps = true");
        assert!(l.iter().all(|m| m.source == Source::Contretemps) && l.len() == 2, "seulement le contretemps écrit, pas la montée par défaut : {l:?}");
        // la montée, écrite, y entre ; une clé de la basse écrite n'amène pas l'autre
        let l = modulations("[modulations]\nliste = []\n[musique]\nmontee = true\n[orbite]\nvitesse_basse = 5.0");
        assert!(l.iter().filter(|m| m.source == Source::Montee).count() == 2 && l.iter().any(|m| m.source == Source::Basse), "{l:?}");
        assert!(!l.iter().any(|m| m.source == Source::Grave), "{l:?}");
        // hors liste vidée : les réactions par défaut d'alors, comme avant
        let l = modulations("[modulations]\nliste = [{ source = \"kick\", cible = \"taille\", quantite = 0.5 }]\n[musique]\ncontretemps = true");
        assert!(l.iter().any(|m| m.source == Source::Montee) && l.iter().any(|m| m.source == Source::Contretemps), "{l:?}");
        // une modulation de même source et même cible, sur un autre groupe, n'en remplace pas une
        // réaction d'avant
        let l = modulations("[modulations]\nliste = [{ source = \"montee\", cible = \"rayon\", sur = \"satellites\", quantite = 0.5 }]");
        assert_eq!(l.iter().filter(|m| m.source == Source::Montee && m.cible == Cible::Rayon).count(), 2, "{l:?}");
        // liste vidée : la montée non écrite n'amène pas non plus l'explosion au drop
        let decl = |texte: &str| {
            let mut t: toml::Table = toml::from_str(texte).unwrap();
            migrer(&mut t);
            let c: crate::config::Config = toml::Value::Table(t).try_into().unwrap();
            c.declencheurs.liste.iter().any(|d| d.quand == Evenement::Drop && d.faire == Action::Explosion)
        };
        assert!(!decl("[modulations]
liste = []"));
        assert!(decl("[modulations]
liste = []
[musique]
montee = true"));
        assert!(decl(""), "hors liste vidée : la montée par défaut d'alors");
        // même groupe (« tout » écrit ou sous-entendu) : gardée seule
        let l = modulations("[modulations]\nliste = [{ source = \"montee\", cible = \"rayon\", quantite = 0.5 }]");
        assert_eq!(l.iter().filter(|m| m.source == Source::Montee && m.cible == Cible::Rayon).count(), 1, "{l:?}");
    }
}

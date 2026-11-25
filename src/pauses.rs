//! Pauses automatiques (`[pause]`, propres à la machine, hors presets) : Windows ne veut pas être
//! dérangé, batterie, application de la liste au premier plan ; et, sans réglage, session
//! verrouillée, déconnectée ou à distance (`win::session_retenue`, notifiée par Windows). Le
//! reste est interrogé à intervalles
//! (pas de hook) : l'état de notification de l'utilisateur et l'application au premier plan
//! chaque seconde, l'alimentation toutes les 5 s ou dès que Windows la signale changée
//! (`WM_POWERBROADCAST`, fenêtre de l'icône).

use crate::config::{ModeBatterie, Pause};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
};

/// Cadence plafonnée sur batterie (`batterie = "reduire"`).
pub const CADENCE_BATTERIE: u32 = 30;
const INTERVALLE: Duration = Duration::from_secs(1);
const INTERVALLE_BATTERIE: Duration = Duration::from_secs(5);

/// Windows demande-t-il de ne pas déranger ? Oui pour une appli en plein écran (`QUNS_BUSY`,
/// confirmé par `plein_ecran` : Windows le dit aussi pour un calque transparent de la taille d'un
/// écran, celui d'une autre surcouche), un jeu Direct3D en plein écran exclusif et le mode
/// présentation. Pas pour `QUNS_QUIET_TIME` :
/// documenté comme la première heure après la première ouverture de session d'un nouveau compte,
/// il mettrait en pause un Essaim fraîchement installé sans raison visible ; et le « Ne pas
/// déranger » manuel de Windows 11 n'est pas exposé par une API documentée.
pub fn derange(etat: QUERY_USER_NOTIFICATION_STATE, plein_ecran: impl FnOnce() -> bool) -> bool {
    match etat {
        QUNS_BUSY => plein_ecran(),
        _ => [QUNS_RUNNING_D3D_FULL_SCREEN, QUNS_PRESENTATION_MODE].contains(&etat),
    }
}

/// Effet de l'alimentation sur l'essaim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffetBatterie {
    Aucun,
    Reduire,
    Pause,
}

/// Sur batterie ou économiseur de batterie actif : ce que demande `mode`.
pub fn effet_batterie(mode: ModeBatterie, sur_batterie: bool, economiseur: bool) -> EffetBatterie {
    match mode {
        _ if !(sur_batterie || economiseur) => EffetBatterie::Aucun,
        ModeBatterie::Rien => EffetBatterie::Aucun,
        ModeBatterie::Reduire => EffetBatterie::Reduire,
        ModeBatterie::Pause => EffetBatterie::Pause,
    }
}

/// Entrée de `liste` qui désigne l'exécutable `exe` (« obs64.exe »), sans tenir compte de la
/// casse ni des espaces ; « obs64 » sans extension convient aussi.
pub fn appli_en_pause<'a>(liste: &'a [String], exe: &str) -> Option<&'a str> {
    let sans_ext = |s: &str| {
        let s = s.trim();
        // découpe par `get` : un nom non latin (« 微信 ») ne tombe pas au milieu d'un caractère
        match s.len().checked_sub(4).and_then(|i| Some((s.get(..i)?, s.get(i..)?))) {
            Some((nom, ext)) if !nom.is_empty() && ext.eq_ignore_ascii_case(".exe") => nom.to_owned(),
            _ => s.to_owned(),
        }
    };
    let exe = sans_ext(exe);
    liste.iter().map(|a| a.as_str()).find(|a| !a.trim().is_empty() && sans_ext(a).eq_ignore_ascii_case(&exe))
}

/// Cadence `cadence` (0 = celle de l'écran) plafonnée à `plafond` (0 = aucun plafond) ; sert
/// aussi à prendre le plus petit de deux plafonds.
pub fn plafonner(cadence: u32, plafond: u32) -> u32 {
    match (cadence, plafond) {
        (c, 0) => c,
        (0, p) => p,
        (c, p) => c.min(p),
    }
}

/// Raison lisible (courte, pour la ligne d'état de la fenêtre des réglages) de ce qui retient
/// l'essaim ; vide si rien. La pause demandée par l'utilisateur se lit déjà sur le bouton Pause.
/// `autre_limite` : cadence que rien d'autre que la qualité adaptative n'empêche (cadence choisie
/// ou de repos, batterie, fréquence de l'écran ; 0 = inconnue) ; `avant` : la raison affichée
/// jusqu'ici. La cadence mesurée (`charge`) départage quand la fréquence de l'écran est inconnue.
pub fn raison(pause_utilisateur: bool, plein_ecran: bool, pause: Option<&str>, batterie: u32, adaptatif: u32, autre_limite: u32, avant: &str) -> String {
    let adaptatif = if adaptatif_en_cause(adaptatif, autre_limite, crate::charge::lire().1, avant.starts_with(ALLEGE)) { adaptatif } else { 0 };
    raison_retenue(pause_utilisateur, plein_ecran, pause, batterie, adaptatif)
}

/// Début de la raison du plafond adaptatif.
const ALLEGE: &str = "Allégé";

/// Part de son plafond que doit atteindre la cadence mesurée pour que la qualité adaptative soit
/// dite en cause (au plafond, la mesure tombe un peu en dessous : 59,6 pour 60), et part sous
/// laquelle elle cesse de l'être : entre les deux, la raison affichée reste (pas de clignotement
/// quand la mesure hésite autour d'un seuil).
const PLAFOND_ATTEINT: f32 = 0.9;
const PLAFOND_QUITTE: f32 = 0.75;

/// Le plafond adaptatif `adaptatif` (0 = aucun) limite-t-il vraiment la cadence ? Non s'il est
/// au-dessus de ce qui la limite déjà (`autre_limite`, écran à 60 Hz sous un plafond de 120,
/// cadence choisie plus basse, repos) : il ne change rien. Oui s'il est en dessous. Sans
/// `autre_limite` connue, la cadence mesurée `ips` décide (0 = pas encore de mesure), avec un
/// écart entre l'entrée et la sortie ; `affiche` : il est dit en ce moment.
fn adaptatif_en_cause(adaptatif: u32, autre_limite: u32, ips: f32, affiche: bool) -> bool {
    if adaptatif == 0 {
        return false;
    }
    if autre_limite > 0 {
        return adaptatif < autre_limite;
    }
    if ips < 1.0 {
        return true;
    }
    let p = adaptatif as f32;
    if affiche { ips >= p * PLAFOND_QUITTE } else { ips >= p * PLAFOND_ATTEINT }
}

/// Raison à afficher, le plafond adaptatif déjà retenu ou écarté (0).
fn raison_retenue(pause_utilisateur: bool, plein_ecran: bool, pause: Option<&str>, batterie: u32, adaptatif: u32) -> String {
    if pause_utilisateur {
        String::new()
    } else if plein_ecran {
        "Plein écran".into()
    } else if let Some(p) = pause {
        p.to_owned()
    } else if batterie > 0 && (adaptatif == 0 || batterie <= adaptatif) {
        format!("Batterie : {batterie} i/s")
    } else if adaptatif > 0 {
        format!("{ALLEGE} : {adaptatif} i/s")
    } else {
        String::new()
    }
}

/// Dernière application vue au premier plan (hors Essaim), proposée dans la page pour la liste.
static RECENTE: Mutex<String> = Mutex::new(String::new());

pub fn recente() -> String {
    RECENTE.lock().map(|r| r.clone()).unwrap_or_default()
}

/// Ce que les pauses automatiques demandent à la boucle.
pub struct Etat {
    /// Pause, avec sa raison (« Ne pas déranger », « Sur batterie », « Appli : obs64.exe »).
    pub pause: Option<String>,
    /// Plafond de cadence (0 = aucun).
    pub plafond: u32,
}

#[derive(Default)]
pub struct Pauses {
    derange: bool,
    prochain: Option<Instant>,
    /// Sur batterie, économiseur actif.
    alimentation: (bool, bool),
    prochain_batterie: Option<Instant>,
    /// Exécutable au premier plan (minuscules) ; gardé quand c'est une fenêtre d'Essaim.
    premier_plan: Option<String>,
    /// Raison de pause et plafond notés au journal.
    note: (Option<String>, u32),
}

impl Pauses {
    /// À appeler à chaque tour de boucle ; n'interroge Windows qu'aux intervalles.
    pub fn tick(&mut self, cfg: &Pause, now: Instant) -> Etat {
        if self.prochain.is_none_or(|p| now >= p) {
            self.prochain = Some(now + INTERVALLE);
            self.derange = cfg.ne_pas_deranger && unsafe { SHQueryUserNotificationState() }.is_ok_and(|e| derange(e, crate::win::plein_ecran_quelque_part));
            // fenêtre d'Essaim ou exécutable illisible : l'état précédent est gardé
            if let Some(exe) = crate::appli::nom_premier_plan() {
                if let Ok(mut r) = RECENTE.lock() {
                    r.clone_from(&exe);
                }
                self.premier_plan = Some(exe);
            }
        }
        if cfg.batterie == ModeBatterie::Rien {
            self.prochain_batterie = None;
            self.alimentation = (false, false);
        } else if crate::win::take_power_changed() || self.prochain_batterie.is_none_or(|p| now >= p) {
            self.prochain_batterie = Some(now + INTERVALLE_BATTERIE);
            self.alimentation = alimentation();
        }
        let batterie = effet_batterie(cfg.batterie, self.alimentation.0, self.alimentation.1);
        let appli = self.premier_plan.as_deref().and_then(|exe| appli_en_pause(&cfg.applis, exe));
        // session verrouillée, déconnectée ou à distance : toujours, quel que soit `[pause]`
        let pause = if let Some(s) = crate::win::session_retenue() {
            Some(s.to_owned())
        } else if self.derange {
            Some("Ne pas déranger".to_owned())
        } else if batterie == EffetBatterie::Pause {
            Some("Sur batterie".to_owned())
        } else {
            appli.map(|a| format!("Appli : {}", a.trim()))
        };
        let plafond = if batterie == EffetBatterie::Reduire { CADENCE_BATTERIE } else { 0 };
        self.noter(&pause, plafond);
        Etat { pause, plafond }
    }

    /// Entrées et sorties notées au journal, comme le plein écran.
    fn noter(&mut self, pause: &Option<String>, plafond: u32) {
        if *pause != self.note.0 {
            match (pause, &self.note.0) {
                (Some(p), _) => log::info!("pause automatique : {p}"),
                (None, Some(avant)) => log::info!("fin de la pause automatique ({avant})"),
                (None, None) => {}
            }
            self.note.0.clone_from(pause);
        }
        if plafond != self.note.1 {
            if plafond > 0 {
                log::info!("batterie : cadence plafonnée à {plafond} images/s");
            } else {
                log::info!("batterie : fin du plafond de cadence");
            }
            self.note.1 = plafond;
        }
    }
}

/// (sur batterie, économiseur de batterie actif). Lecture impossible : ni l'un ni l'autre.
fn alimentation() -> (bool, bool) {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return (false, false);
    }
    // ACLineStatus : 0 = sur batterie, 1 = sur secteur, 255 = inconnu (PC de bureau)
    (s.ACLineStatus == 0, s.SystemStatusFlag == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Shell::{QUNS_ACCEPTS_NOTIFICATIONS, QUNS_NOT_PRESENT, QUNS_QUIET_TIME};

    #[test]
    fn decision_batterie() {
        use EffetBatterie::*;
        for mode in [ModeBatterie::Rien, ModeBatterie::Reduire, ModeBatterie::Pause] {
            assert_eq!(effet_batterie(mode, false, false), Aucun, "{mode:?} sur secteur");
        }
        assert_eq!(effet_batterie(ModeBatterie::Rien, true, true), Aucun);
        assert_eq!(effet_batterie(ModeBatterie::Reduire, true, false), Reduire);
        assert_eq!(effet_batterie(ModeBatterie::Reduire, false, true), Reduire, "économiseur sur secteur");
        assert_eq!(effet_batterie(ModeBatterie::Pause, true, false), Pause);
        assert_eq!(effet_batterie(ModeBatterie::Pause, false, true), Pause);
    }

    #[test]
    fn liste_d_applis_insensible_a_la_casse() {
        let liste: Vec<String> = ["OBS64.EXE", " zoom.exe ", "Teams", ""].map(String::from).to_vec();
        assert_eq!(appli_en_pause(&liste, "obs64.exe"), Some("OBS64.EXE"));
        assert_eq!(appli_en_pause(&liste, "Zoom.exe"), Some(" zoom.exe "));
        assert_eq!(appli_en_pause(&liste, "teams.exe"), Some("Teams"));
        assert_eq!(appli_en_pause(&liste, "obs.exe"), None);
        assert_eq!(appli_en_pause(&liste, ".exe"), None);
        assert_eq!(appli_en_pause(&[], "obs64.exe"), None);
        // noms non latins écrits à la main : pas de découpe au milieu d'un caractère
        let liste: Vec<String> = ["微信", "Zoé VR", "é"].map(String::from).to_vec();
        assert_eq!(appli_en_pause(&liste, "微信.exe"), Some("微信"));
        assert_eq!(appli_en_pause(&liste, "zoé vr.EXE"), Some("Zoé VR"));
        assert_eq!(appli_en_pause(&liste, "é.exe"), Some("é"));
    }

    #[test]
    fn etats_qui_derangent() {
        let oui = || true;
        assert!(derange(QUNS_BUSY, oui) && derange(QUNS_RUNNING_D3D_FULL_SCREEN, oui) && derange(QUNS_PRESENTATION_MODE, oui));
        assert!(!derange(QUNS_ACCEPTS_NOTIFICATIONS, oui) && !derange(QUNS_QUIET_TIME, oui) && !derange(QUNS_NOT_PRESENT, oui));
        // « occupé » sans vraie fenêtre en plein écran (calque d'une autre surcouche) : pas de pause
        let non = || false;
        assert!(!derange(QUNS_BUSY, non) && derange(QUNS_RUNNING_D3D_FULL_SCREEN, non));
    }

    #[test]
    fn plafonds_de_cadence() {
        assert_eq!(plafonner(0, 0), 0);
        assert_eq!(plafonner(0, 30), 30);
        assert_eq!(plafonner(144, 0), 144);
        assert_eq!(plafonner(144, 60), 60);
        assert_eq!(plafonner(20, 30), 20);
    }

    #[test]
    fn raisons_lisibles() {
        let raison = raison_retenue;
        assert_eq!(raison(true, true, Some("Ne pas déranger"), 30, 60), "");
        assert_eq!(raison(false, true, Some("Ne pas déranger"), 0, 0), "Plein écran");
        assert_eq!(raison(false, false, Some("Appli : obs64.exe"), 30, 0), "Appli : obs64.exe");
        assert_eq!(raison(false, false, None, 30, 60), "Batterie : 30 i/s");
        assert_eq!(raison(false, false, None, 30, 30), "Batterie : 30 i/s");
        assert_eq!(raison(false, false, None, 0, 60), "Allégé : 60 i/s");
        assert_eq!(raison(false, false, None, 0, 0), "");
    }

    #[test]
    fn plafond_adaptatif_dit_seulement_s_il_joue() {
        // écran à 60 Hz sous un plafond de 120, ou à 60 sous 60 : il ne change rien
        assert!(!adaptatif_en_cause(120, 60, 60.0, false));
        assert!(!adaptatif_en_cause(60, 60, 59.6, true));
        // cadence choisie (ou de repos) plus basse que lui : rien non plus
        assert!(!adaptatif_en_cause(60, 30, 30.0, false));
        // sous l'écran à 144 Hz : il joue, même si la machine peine à le tenir (mesure basse)
        assert!(adaptatif_en_cause(60, 144, 59.6, false));
        assert!(adaptatif_en_cause(30, 144, 18.0, false));
        assert!(!adaptatif_en_cause(0, 144, 144.0, false));
    }

    #[test]
    fn frequence_inconnue_mesure_avec_ecart() {
        // plafond atteint (la mesure tombe juste en dessous) : dit
        assert!(adaptatif_en_cause(60, 0, 59.6, false));
        // nettement en dessous : autre chose limite
        assert!(!adaptatif_en_cause(120, 0, 60.0, false));
        // entre les deux seuils, l'affichage en cours reste : pas de clignotement
        assert!(adaptatif_en_cause(60, 0, 50.0, true));
        assert!(!adaptatif_en_cause(60, 0, 50.0, false));
        assert!(!adaptatif_en_cause(60, 0, 40.0, true));
        // pas encore de mesure : dit
        assert!(adaptatif_en_cause(60, 0, 0.0, false));
    }
}

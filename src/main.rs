//! Essaim : surcouche réactive à la musique autour du pointeur (comètes et essaim en orbite,
//! éclat du kick, fissures au clic).
//!
//! Le pointeur reste celui de Windows (pack de curseurs installé par les réglages de la souris) :
//! l'appli ne dessine que ce qui l'entoure.
//!
//! Options : `--reglages` ouvre la fenêtre des réglages (de l'instance en cours s'il y en a
//! une) ; `--stream` ouvre la fenêtre pour le stream ; `--quitter` ferme l'instance en cours ;
//! `--restaurer-bordure` rend sa couleur à la fenêtre restée colorée (fermeture, plantage, pause
//! et plein écran le font tout seuls).

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod adaptatif;
mod appli;
mod audio;
mod auto;
mod backend;
mod bordure;
mod canvas;
mod charge;
mod config;
mod contours;
mod effets;
mod etiquette;
mod figures;
mod gpu;
mod ipc;
mod layer;
mod loin;
mod maj;
mod media;
mod migration;
mod modulation;
mod pauses;
mod prereglages;
mod prisme;
mod profils;
mod reglages;
mod render;
mod souris;
mod stream;
mod win;

use config::Evenement;
use render::{Crack, Rng, Swarm};
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetSystemMetrics, PostMessageW, SM_SWAPBUTTON, WM_CLOSE, WM_COMMAND};

const MAX_CRACKS: usize = 12;
/// Sans mouvement de souris ni musique depuis ce délai : cadence de repos.
const IDLE: Duration = Duration::from_secs(2);
/// Verrou d'instance unique.
const VERROU: PCWSTR = w!("Local\\Essaim-curseur");
/// Classe de la fenêtre des réglages (`reglages.rs`).
const CLASSE_REGLAGES: PCWSTR = w!("EssaimReglages");
/// Fondu de la veille profonde : durée de l'extinction et du réveil.
const FONDU_EXTINCTION: f32 = 1.0;
const FONDU_REVEIL: f32 = 0.25;
/// Veille profonde : souris, son et clics lus à ce rythme (un clic de 60 ms est attrapé).
const VEILLE_LECTURE: Duration = Duration::from_millis(40);
/// Attente la plus longue entre deux lectures des clics quand une cadence est choisie.
const LECTURE_CLICS: Duration = Duration::from_millis(30);
/// Capture du son impossible à lancer : nouvel essai après ce délai.
const SON_REESSAI: Duration = Duration::from_secs(30);
/// Pause plein écran plus longue : le calque de l'écran couvert est détruit (une fenêtre
/// superposée vide resterait au-dessus d'un jeu en plein écran sans bordure).
const LIBERER_PLEIN_ECRAN: Duration = Duration::from_secs(10);

fn start_instant() -> Instant {
    Instant::now()
}

/// Taille au-delà de laquelle le journal passe dans `essaim.old.log` (l'ancien est remplacé).
const JOURNAL_MAX: u64 = 1 << 20;
/// Taille du journal vérifiée toutes les `JOURNAL_LIGNES` lignes ou toutes les minutes.
const JOURNAL_LIGNES: u32 = 200;
const JOURNAL_VERIF: Duration = Duration::from_secs(60);

struct Journal {
    chemin: std::path::PathBuf,
    f: Option<std::fs::File>,
    lignes: u32,
    verifie: Instant,
}

impl Journal {
    fn ouvrir(chemin: &std::path::Path) -> Option<std::fs::File> {
        std::fs::OpenOptions::new().create(true).append(true).open(chemin).ok()
    }

    /// Trop gros : renommé en `essaim.old.log`, puis un journal neuf est ouvert. Renommage
    /// impossible (fichier ouvert ailleurs sans partage) : on continue dans le même, nouvel essai
    /// plus tard.
    fn tourner_si_gros(&mut self) {
        (self.lignes, self.verifie) = (0, Instant::now());
        let gros = self.f.as_ref().and_then(|f| f.metadata().ok()).is_some_and(|m| m.len() > JOURNAL_MAX);
        if !gros {
            return;
        }
        self.f = None;
        let tourne = tourner(&self.chemin);
        self.f = Self::ouvrir(&self.chemin);
        if let (true, Some(f)) = (tourne, self.f.as_mut()) {
            let _ = writeln!(f, "{} INFO journal précédent : essaim.old.log", secondes());
        }
    }
}

/// Renomme le journal `p` en `essaim.old.log` (à côté) ; vrai si c'est fait.
fn tourner(p: &std::path::Path) -> bool {
    std::fs::rename(p, p.with_file_name("essaim.old.log")).is_ok()
}

fn secondes() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

struct FileLog(std::sync::Mutex<Journal>);
impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            if let Ok(mut j) = self.0.lock() {
                j.lignes += 1;
                if j.lignes >= JOURNAL_LIGNES || j.verifie.elapsed() >= JOURNAL_VERIF {
                    j.tourner_si_gros();
                }
                if let Some(f) = j.f.as_mut() {
                    let _ = writeln!(f, "{} {} {}", secondes(), r.level(), r.args());
                }
            }
        }
    }
    fn flush(&self) {}
}

/// Ouvre le journal. `tourner_au_debut` : seulement pour l'instance qui tient le verrou (une
/// seconde instance, qui s'arrête aussitôt, ne doit pas renommer le journal de la première).
fn init_log(tourner_au_debut: bool) {
    let _ = std::fs::create_dir_all(config::dir());
    let p = config::dir().join("essaim.log");
    if tourner_au_debut && std::fs::metadata(&p).is_ok_and(|m| m.len() > JOURNAL_MAX) {
        tourner(&p);
    }
    if let Some(f) = Journal::ouvrir(&p) {
        let journal = Journal { chemin: p, f: Some(f), lignes: 0, verifie: Instant::now() };
        let _ = log::set_boxed_logger(Box::new(FileLog(std::sync::Mutex::new(journal))));
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// Une instance tient le verrou (elle tourne, ou démarre).
fn instance_lancee() -> bool {
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, VERROU) } {
        Ok(h) => {
            let _ = unsafe { CloseHandle(h) };
            true
        }
        Err(_) => false,
    }
}

/// Fenêtre de l'icône de l'instance en cours. Une instance qui tient le verrou sans avoir encore
/// créé sa fenêtre (elle démarre) est attendue jusqu'à 2 s.
fn fenetre_instance() -> Option<HWND> {
    for i in 0..20 {
        if let Ok(h) = unsafe { FindWindowW(win::TRAY_CLASS, None) } {
            return Some(h);
        }
        if i == 0 && !instance_lancee() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

fn main() {
    // COM en appartement monothread pour le fil principal (DirectComposition, ShellExecute)
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--restaurer-bordure") {
        // instance en cours : seulement la couleur, son témoin reste son filet
        if instance_lancee() {
            bordure::rendre_couleur_seule();
        } else {
            bordure::restore();
        }
        return;
    }
    let open_settings = args.iter().any(|a| a == "--reglages");
    let open_stream = args.iter().any(|a| a == "--stream");
    if open_settings || open_stream {
        // instance en cours : c'est elle qui ouvre sa fenêtre
        if let Some(h) = fenetre_instance() {
            unsafe {
                for (voulu, cmd) in [(open_settings, win::CMD_CONFIG), (open_stream, win::CMD_STREAM)] {
                    if voulu {
                        let _ = PostMessageW(Some(h), WM_COMMAND, WPARAM(cmd), LPARAM(0));
                    }
                }
            }
            return;
        }
    }
    if args.iter().any(|a| a == "--quitter") {
        unsafe {
            if let Ok(h) = FindWindowW(win::TRAY_CLASS, None) {
                let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        // rendre la main seulement une fois l'instance sortie : un lancement juste après ne
        // tombe pas sur son verrou
        win::attendre_fin_instance(VERROU, Duration::from_secs(5));
        return;
    }
    // verrou d'instance unique, gardé jusqu'à la fin (ou rendu avant de relancer)
    let (verrou, deja) = unsafe {
        let verrou = CreateMutexW(None, true, VERROU);
        (verrou, GetLastError() == ERROR_ALREADY_EXISTS)
    };
    init_log(!deja);
    if deja {
        log::info!("déjà lancé, sortie");
        return;
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    // Quoi qu'il arrive, la bordure des fenêtres doit revenir (y compris celle laissée colorée
    // par une instance tuée).
    bordure::restore();
    // plantage hors Rust (exception non gérée) : la couleur est rendue aussi
    bordure::installer_filtre_plantage();
    // Plantage du fil principal : la bordure est rendue. Celui d'un autre fil (réglages, mises à
    // jour…) ne l'est pas : le fil principal continue de colorer, et lui retirer la couleur et
    // le témoin la laisserait sans filet si Essaim était tué ensuite.
    let principal = std::thread::current().id();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // panique arrêtée dans une procédure de fenêtre (`win::sans_panique`) : l'essaim continue
        // et colore encore, la bordure et son témoin restent
        if std::thread::current().id() == principal && !win::panique_arretee() {
            bordure::restore();
        }
        log::error!("plantage : {info}");
        default_hook(info);
    }));
    log::info!("démarrage d'Essaim {}", env!("CARGO_PKG_VERSION"));
    let relancer = run(open_settings, open_stream).unwrap_or_else(|e| {
        log::error!("arrêt sur erreur : {e:#}");
        false
    });
    bordure::restore();
    // une mise à jour en train de remplacer l'exécutable, ou des curseurs en cours d'écriture,
    // finissent avant la sortie (sinon : plus d'essaim.exe, ou un jeu de curseurs à moitié écrit)
    maj::attendre_remplacement(maj::duree_remplacement_max());
    souris::attendre_curseurs(Duration::from_secs(2));
    // nouvelle version en place : le verrou est rendu avant de lancer la suivante
    if let (true, Some(exe)) = (relancer, maj::exe()) {
        if let Ok(v) = verrou {
            unsafe {
                let _ = CloseHandle(v);
            }
        }
        match std::process::Command::new(exe).spawn() {
            Ok(_) => log::info!("relance pour la mise à jour"),
            Err(e) => log::error!("relance impossible : {e}"),
        }
    }
    log::info!("arrêt");
}

/// Image de l'habillage seul (équerres, liseré), quand l'essaim n'est pas dessiné : pointeur
/// masqué par la frappe, veille profonde. Comme la bordure, il reste en place. La zone de l'essaim
/// de l'image précédente est effacée par le calque, le reste n'est redessiné que s'il change.
#[allow(clippy::too_many_arguments)]
fn habillage_seul(
    effets: &mut effets::Effets,
    backend: &mut backend::Backend,
    monitors: &[(isize, layer::IRect)],
    shared: &audio::AudioShared,
    cfg: &config::Config,
    t: f64,
    dt: f32,
    pos: (i32, i32),
) {
    let scale = win::scale_at(pos.0, pos.1);
    effets.avance_habillage(t, dt, (pos.0 as f32, pos.1 as f32), scale, shared, cfg, monitors);
    let zones: Vec<layer::IRect> = effets.zones_loin().collect();
    backend.frame(monitors, layer::AUCUNE, &zones, &|cv, _| effets.dessiner_loin(cv, cfg, t, scale));
}

/// Relit le cadre de la fenêtre au premier plan (s'il sert : rebonds, équerres, halo) ; renvoie
/// les murs des rebonds. Quelques appels à Windows, de l'ordre de la dizaine de microsecondes :
/// fait à chaque image dessinée (ses écrans ne sont relus que quand elle change :
/// `loin::FenetreLue`).
fn lire_fenetre(effets: &mut effets::Effets, cfg: &config::Config) -> Option<(f32, f32, f32, f32)> {
    let voulu = cfg.particules.rebonds || cfg.equerres.actif || cfg.halo.actif;
    effets.fenetre = if voulu { win::cadre_premier_plan() } else { None };
    effets.fenetre.filter(|_| cfg.particules.rebonds)
}

/// Bouton principal (`secondaire` = faux) ou secondaire de la souris enfoncé. Boutons inversés
/// dans Windows (gauchers) : `GetAsyncKeyState` lit le bouton physique, le principal est alors
/// le droit.
fn bouton(secondaire: bool) -> bool {
    unsafe {
        let inverses = GetSystemMetrics(SM_SWAPBUTTON) != 0;
        let vk = if inverses != secondaire { VK_RBUTTON } else { VK_LBUTTON };
        GetAsyncKeyState(vk.0 as i32) < 0
    }
}


/// Fenêtre des réglages ouverte (elle montre le son en direct : la capture doit continuer).
fn reglages_ouverts() -> bool {
    unsafe { FindWindowW(CLASSE_REGLAGES, None) }.is_ok()
}

/// Capture du son : lancée quand elle est voulue, arrêtée sinon (pause, plein écran, son
/// désactivé). État publié pour la fenêtre des réglages.
#[derive(Default)]
struct Son {
    capture: Option<audio::AudioThread>,
    /// Lancement de la capture en cours.
    lancee: Option<Instant>,
    /// Sortie écoutée par la capture en cours (`[audio] sortie`, vide = par défaut) et
    /// applications écoutées (`[audio] applis`, vide = tout le son).
    sortie: String,
    applis: Vec<String>,
    /// Le dernier lancement a échoué ; nouvel essai à cette date.
    reessai: Option<Instant>,
    publie: Option<&'static str>,
}

impl Son {
    fn regler(&mut self, voulu: bool, sortie: &str, applis: &[String], shared: &Arc<audio::AudioShared>, now: Instant) {
        // autre sortie ou autres applications choisies : la capture est relancée tout de suite
        if self.capture.is_some() && (self.sortie != sortie || self.applis != applis) {
            (self.capture, self.lancee) = (None, None);
            self.reessai = None;
            log::info!("son : sortie ou applications écoutées changées, capture relancée");
        }
        // fil de capture terminé de lui-même (panique rattrapée, `lost` déjà appelé) : relancé
        // après le délai habituel (une panique qui revient ne relance pas à chaque image)
        if self.capture.as_ref().is_some_and(|c| !c.actif()) {
            (self.capture, self.lancee) = (None, None);
            shared.reset();
            self.reessai = Some(now + SON_REESSAI);
            log::error!("son : le fil de capture s'est arrêté seul, nouvel essai dans 30 s");
        }
        if !voulu {
            self.reessai = None;
            self.lancee = None;
            if self.capture.take().is_some() {
                // fil arrêté (attendu) : plus rien n'écrit, les niveaux peuvent être remis à zéro
                shared.reset();
                log::info!("son : capture arrêtée");
            }
        } else if self.capture.is_none() && self.reessai.is_none_or(|r| now >= r) {
            match audio::start(shared.clone(), sortie, applis) {
                Ok(c) => {
                    (self.capture, self.lancee) = (Some(c), Some(now));
                    sortie.clone_into(&mut self.sortie);
                    applis.clone_into(&mut self.applis);
                    self.reessai = None;
                    log::info!("son : capture lancée");
                }
                Err(e) => {
                    if self.reessai.is_none() {
                        log::error!("son : capture impossible ({e}) ; Essaim continue sans son, nouvel essai toutes les 30 s");
                    }
                    self.reessai = Some(now + SON_REESSAI);
                }
            }
        }
        let etat = match (&self.capture, self.reessai) {
            (Some(_), _) if shared.perdu() => "erreur",
            (Some(_), _) => "active",
            (None, Some(_)) => "erreur",
            (None, None) => "coupee",
        };
        if self.publie != Some(etat) {
            charge::publier_capture(etat);
            self.publie = Some(etat);
        }
    }
}

/// Boucle de l'appli. Renvoie vrai s'il faut relancer Essaim (mise à jour prête).
fn run(open_settings: bool, open_stream: bool) -> anyhow::Result<bool> {
    // premier lancement (aucun réglage encore) : la fenêtre des réglages et son accueil
    let premier = !config::path().exists();
    // réglages d'une version d'avant les modulations : convertis une fois pour toutes
    if let Err(e) = config::migrer_fichier() {
        log::warn!("config : conversion des réactions à la basse impossible ({e})");
    }
    let mut conf = config::Watched::load();
    render::set_traces(&conf.cfg.orbite.chemins());
    let shared = Arc::new(audio::AudioShared::default());
    shared.set_sensitivity(conf.cfg.audio.sensibilite_kick);
    shared.set_min_gap(conf.cfg.audio.delai_kick);
    shared.set_musique(conf.cfg.analyse_musicale());
    // capture lancée au premier tour de boucle ; un échec n'empêche pas Essaim de tourner
    let mut son = Son::default();

    let _tray = win::Tray::new()?;
    maj::start(conf.cfg.mise_a_jour.auto);
    win::raccourcis(conf.cfg.raccourcis.actif);
    let mut raccourcis = conf.cfg.raccourcis.actif;
    let mut automatique = auto::Auto::default();
    if open_settings || premier {
        reglages::open(shared.clone());
    }
    if open_stream {
        stream::ouvrir(shared.clone());
    }
    // interface locale pour Horizon, et synchro de teinte avec Prisme
    let ipc_state = Arc::new(ipc::Shared::default());
    let (ipc_tx, ipc_rx) = std::sync::mpsc::channel();
    ipc::start(ipc_state.clone(), ipc_tx);
    let prisme = prisme::start();
    let mut appli = appli::TeinteAppli::default();
    let mut sync_hue: Option<f32> = None;
    let mut source_teinte: Option<media::Source> = None;
    // morceau en cours (titre, pochette, changements de piste), lu sur son propre fil
    media::demarrer();
    let mut pistes_vues = media::pistes();
    // écrans : relus quand leur configuration change (WM_DISPLAYCHANGE)
    let mut monitors = win::monitors();
    let (mut last_pos, mut last_move, mut last_sound, mut last_click, mut last_kicks) =
        ((0, 0), start_instant(), start_instant(), start_instant(), 0u32);
    let mut published: Option<(bool, bool, bool, bool, bool, u32)> = None;
    // un calque immobile par écran visité ; les fissures y sont retracées à chaque image
    let mut backend = backend::Backend::new(conf.cfg.rendu.carte_graphique);
    win::masquer_captures(conf.cfg.rendu.masquer_captures);
    let mut cracks: Vec<Crack> = Vec::new();
    let mut swarm = Swarm::new();
    let mut effets = effets::Effets::new();
    let mut etiquette = etiquette::Etiquette::new();
    let mut rng = Rng::seeded();
    let mut border = bordure::Border::default();
    let mut ecoute_bordure = render::Ecoute::default();
    // trajet « icône » : contours de l'icône de l'application au premier plan
    let mut icone_appli = appli::IconeAppli::default();
    // pauses automatiques (ne pas déranger, batterie, applications) et raison publiée
    let mut pauses = pauses::Pauses::default();
    let mut raison_publiee = String::new();
    let mut adaptatif = adaptatif::Adaptatif::default();
    // cadence visée sans le plafond adaptatif (0 = celle de l'écran), et fréquence de l'écran du
    // pointeur (0 = inconnue) : la raison « Allégé » n'est dite que si ce plafond joue
    let mut cadence_visee = conf.cfg.rendu.cadence;
    let mut frequence = 0u32;
    // plus haute fréquence des écrans : au-dessus de celle de l'écran du pointeur, `DwmFlush`
    // (qui suit le compositeur, calé sur l'écran le plus rapide) ne règle plus la cadence
    let mut frequence_max = win::frequence_max(&monitors);

    let start = Instant::now();
    let mut last = start;
    let mut user_paused = false;
    // plein écran sous le pointeur : son écran, depuis quand, calque de cet écran déjà libéré
    let mut plein_ecran: Option<(isize, Instant, bool)> = None;
    let mut conf_gen = 0u32;
    let mut was_down = false;
    let mut droit_avant = false;
    // mode sans flashs (relu chaque seconde : Windows peut changer d'avis) ; le cadre de la
    // fenêtre au premier plan est relu à chaque image (`lire_fenetre`)
    let mut doux = win::sans_flashs(conf.cfg.rendu.sans_flashs);
    let mut last_state = "";
    let mut cleared = false;
    let mut backend_gen = 0u32;
    // veille profonde : niveau du fondu (1 = éveillé, 0 = éteint) et extinction complète
    let (mut eveil, mut profond) = (1.0f32, false);
    // icône : état montré (pause, veille, génération des réglages) et preset en place
    let mut icone: Option<(bool, bool, u32, u32)> = None;
    let mut preset: Option<(u32, Option<String>)> = None;
    let mut next_conf = start;
    let mut next_fs = start;
    let mut stat_send = Duration::ZERO;
    let mut relancer = false;
    // charge publiée chaque seconde pour la fenêtre des réglages
    let (mut charge_busy, mut charge_frames, mut charge_depuis) = (Duration::ZERO, 0u32, start);
    let (mut stat_frames, mut stat_time, mut stat_busy, mut stat_max, mut next_stat) =
        (0u32, 0f64, Duration::ZERO, Duration::ZERO, start + Duration::from_secs(60));

    loop {
        if !win::pump() {
            break;
        }
        // toutes les commandes arrivées depuis le dernier tour (menu, raccourcis), dans l'ordre
        let mut sortir = false;
        while let c @ 1.. = win::take_command() {
            match c {
                win::CMD_QUIT => {
                    sortir = true;
                    break;
                }
                win::CMD_REDEMARRER if maj::prete().is_some() => {
                    relancer = true;
                    sortir = true;
                    break;
                }
                win::CMD_PAUSE => {
                    user_paused = !user_paused;
                    charge::publier_pause(user_paused);
                    log::info!("{}", if user_paused { "pause" } else { "reprise" });
                }
                win::CMD_CONFIG => reglages::open(shared.clone()),
                win::CMD_STREAM => stream::ouvrir(shared.clone()),
                win::CMD_PROJECTEUR => effets.demander_projecteur(),
                c @ (win::CMD_SUIVANT | win::CMD_PRECEDENT) => {
                    let pas = if c == win::CMD_SUIVANT { 1 } else { -1 };
                    if let Some(nom) = profils::voisin(&conf.cfg, pas) {
                        match profils::appliquer(&nom) {
                            Ok(()) => win::bulle(&format!("Preset « {nom} »")),
                            Err(e) => log::warn!("raccourci : preset « {nom} » non appliqué ({e})"),
                        }
                    }
                }
                c if c >= win::CMD_PRESET => {
                    // le nom de l'entrée choisie, pas le rang dans une liste qui a pu changer
                    if let Some(nom) = win::preset_du_menu(c - win::CMD_PRESET) {
                        if let Err(e) = profils::appliquer(&nom) {
                            log::warn!("menu : preset « {nom} » non appliqué ({e})");
                        }
                    }
                }
                _ => {}
            }
        }
        if sortir {
            break;
        }
        while let Ok(c) = ipc_rx.try_recv() {
            match c {
                ipc::Cmd::SetPaused(p) if p != user_paused => {
                    user_paused = p;
                    charge::publier_pause(p);
                    log::info!("{} (Horizon)", if p { "pause" } else { "reprise" });
                }
                ipc::Cmd::SetPaused(_) => {}
                ipc::Cmd::OpenConfig => win::open_file(&config::path()),
                ipc::Cmd::OpenSettings => reglages::open(shared.clone()),
            }
        }
        if win::take_display_changed() {
            monitors = win::monitors();
            frequence_max = win::frequence_max(&monitors);
            // seuls les calques des écrans disparus ou déplacés sont refaits : un calque refait
            // sous la souris lui ferait perdre son curseur
            let refaits = backend.ecrans_changes(&monitors);
            log::info!("écrans modifiés : {refaits} calque(s) refait(s)");
            effets.oublier_loin();
        }
        let now = Instant::now();
        let t = (now - start).as_secs_f64();
        if now - charge_depuis >= Duration::from_secs(1) {
            let ms = charge_busy.as_secs_f32() * 1000.0 / charge_frames.max(1) as f32;
            let ips = charge_frames as f32 / (now - charge_depuis).as_secs_f32();
            charge::publier(ms, ips);
            // qualité adaptative : cadence plafonnée par paliers quand une image coûte cher
            let change = if conf.cfg.rendu.adaptatif { adaptatif.mesure(ms, ips) } else { adaptatif.lever() };
            if change {
                match adaptatif.plafond() {
                    0 => log::info!("qualité adaptative : plus de plafond de cadence"),
                    p => log::info!("qualité adaptative : cadence plafonnée à {p} images/s (calcul moyen {ms:.2} ms par image)"),
                }
            }
            (charge_busy, charge_frames, charge_depuis) = (Duration::ZERO, 0, now);
        }

        let cur = win::cursor_state();
        if now >= next_fs {
            next_fs = now + Duration::from_millis(250);
            // la fenêtre sous le pointeur, pas celle qui a le clavier
            let ecran = cur.as_ref().and_then(|c| win::plein_ecran_sous(c.pos.0, c.pos.1));
            if ecran != plein_ecran.map(|p| p.0) {
                match (ecran, plein_ecran) {
                    (Some(_), None) => log::info!("plein écran : en veille"),
                    (None, _) => log::info!("fin du plein écran"),
                    (Some(_), Some(_)) => log::info!("plein écran sur un autre écran"),
                }
                plein_ecran = ecran.map(|e| (e, now, false));
            }
        }
        let fs_paused = plein_ecran.is_some();
        // plein écran qui dure : le calque de son écran est détruit (il resterait au-dessus du
        // jeu) ; refait à la sortie, à la première image qui touche cet écran
        if let Some((ecran, depuis, libere @ false)) = plein_ecran.as_mut() {
            if now - *depuis >= LIBERER_PLEIN_ECRAN {
                *libere = true;
                if backend.liberer_ecran(*ecran) {
                    effets.oublier_loin();
                    log::info!("plein écran prolongé : calque de son écran libéré");
                }
            }
        }
        // réglages : relus toutes les secondes, ou tout de suite s'ils viennent de la fenêtre
        let timed = now >= next_conf;
        if timed {
            next_conf = now + Duration::from_secs(1);
            let d = win::sans_flashs(conf.cfg.rendu.sans_flashs);
            if d != doux {
                doux = d;
                log::info!("{}", if d { "mode sans flashs" } else { "fin du mode sans flashs" });
            }
            // disposition de clavier ajoutée ou retirée : un raccourci peut se mettre à écrire un
            // caractère avec AltGr (ou cesser de le faire)
            win::raccourcis_suivent_clavier();
            // fréquence de l'écran du pointeur : dit si le plafond adaptatif change quelque chose
            frequence = win::frequence_ecran(last_pos.0, last_pos.1);
        }
        if (timed || config::take_reload_request()) && conf.reload() {
            conf_gen += 1;
            render::set_traces(&conf.cfg.orbite.chemins());
            if conf.cfg.raccourcis.actif != raccourcis {
                raccourcis = conf.cfg.raccourcis.actif;
                win::raccourcis(raccourcis);
            }
            shared.set_sensitivity(conf.cfg.audio.sensibilite_kick);
            shared.set_min_gap(conf.cfg.audio.delai_kick);
            shared.set_musique(conf.cfg.analyse_musicale());
            maj::regler(conf.cfg.mise_a_jour.auto);
            win::masquer_captures(conf.cfg.rendu.masquer_captures);
        }
        // pause automatique : traitée comme le plein écran (rien dessiné, son coupé, bordure rendue)
        let retenue = pauses.tick(&conf.cfg.pause, now, cur.as_ref().map(|c| c.pos));
        let retrait = fs_paused || retenue.pause.is_some();
        // ce qui limite la cadence hors qualité adaptative : cadence choisie (ou de repos),
        // batterie, fréquence de l'écran
        let autre_limite = pauses::plafonner(cadence_visee, frequence);
        let raison = pauses::raison(user_paused, fs_paused, retenue.pause.as_deref(), retenue.plafond, adaptatif.plafond(), autre_limite, &raison_publiee);
        if raison != raison_publiee {
            charge::publier_raison(&raison);
            raison_publiee = raison;
        }
        // son : coupé en pause et en plein écran (sauf fenêtre des réglages ou du stream ouverte,
        // qui le montrent)
        let son_voulu = conf.cfg.audio.actif && (!(user_paused || retrait) || reglages_ouverts() || stream::ouverte());
        son.regler(son_voulu, &conf.cfg.audio.sortie, &conf.cfg.audio.applis, &shared, now);
        // tempo mesuré seulement pendant la capture ; son coupé dans les réglages : il n'y en a
        // pas, les règles sur le tempo sont fausses
        let ecoute = if conf.cfg.audio.actif { son.lancee } else { Some(start) };
        automatique.tick(&conf.cfg, now, shared.view().bpm, ecoute);
        let cfg = &conf.cfg;
        prisme.set_wanted(cfg.couleur.synchro_prisme);
        // preset en place (interface locale, infobulle de l'icône) : recherché quand les réglages changent
        if preset.as_ref().is_none_or(|p| p.0 != conf_gen) {
            let nom = profils::list().into_iter().find(|p| profils::same(&p.cfg, cfg)).map(|p| p.name);
            preset = Some((conf_gen, nom));
        }
        let nom_preset = preset.as_ref().and_then(|p| p.1.as_deref());
        let fields = (user_paused, fs_paused, !user_paused && !retrait, backend.is_gpu(), prisme.connected(), conf_gen);
        if published != Some(fields) {
            ipc_state.publish(serde_json::json!({
                "paused": user_paused,
                "fullscreen": fs_paused,
                "active": fields.2,
                "gpu": fields.3,
                "prisme": fields.4,
                "preset": nom_preset,
                "config": ipc::config_publiee(cfg),
            }));
            published = Some(fields);
        }
        // icône : grisée en pause, infobulle avec le preset en place, l'état et le morceau
        let montre = (user_paused, retrait || profond, conf_gen, media::version());
        if icone != Some(montre) {
            win::etat_icone(user_paused, &media::infobulle(&win::infobulle(nom_preset, montre.0, montre.1)));
            icone = Some(montre);
        }

        let active = !user_paused && !retrait;
        // le masquage par la frappe (curseur caché par Windows le temps de taper) n'est pas noté :
        // il reviendrait à chaque mot
        let state = match &cur {
            _ if !active => "en pause",
            None => "position du curseur illisible",
            Some(_) => "actif",
        };
        if state != last_state {
            log::info!("état : {state}");
            last_state = state;
        }
        let au = shared.view();
        if au.bass > 0.03 || au.kicks != last_kicks {
            last_kicks = au.kicks;
            last_sound = now;
        }
        let (down, droit) = (bouton(false), bouton(true));
        if down || droit {
            last_click = now;
        }
        // changement de piste : vu même en pause (pas de rafale en retard à la reprise)
        let pistes = media::pistes();
        let nouvelle_piste = pistes != pistes_vues;
        pistes_vues = pistes;
        let (clic_neuf, droit_neuf) = (down && !was_down, droit && !droit_avant);
        // bordure : ses déclencheurs (kick, clic…), même quand le pointeur est masqué par la frappe ;
        // rien en mode sans flashs
        let declencheurs = &cfg.declencheurs.liste;
        let souris = [(Evenement::Clic, clic_neuf), (Evenement::ClicDroit, droit_neuf), (Evenement::Piste, nouvelle_piste)];
        for ev in ecoute_bordure.lire(&au).evenements().chain(souris.into_iter().filter(|e| e.1).map(|e| e.0)) {
            border.reagir(ev, declencheurs, doux, now);
        }
        // pointeur masqué par la frappe : sa position, pour l'habillage qui continue
        let masque = cur.as_ref().filter(|c| active && !c.showing).map(|c| c.pos);
        let Some(cur) = cur.filter(|c| active && c.showing) else {
            // curseur masqué (frappe) : la bordure continue ; pause ou plein écran : remise
            border.tick(active, &cfg.bordure, render::prisme(cfg, t, 1.0), now);
            // l'habillage (équerres, liseré) continue comme elle ; tout le reste est effacé
            match masque.filter(|_| effets.habillage(cfg)) {
                Some(pos) => {
                    cracks.clear();
                    let dt = (now - last).as_secs_f32().clamp(0.0005, 0.25);
                    lire_fenetre(&mut effets, cfg);
                    habillage_seul(&mut effets, &mut backend, &monitors, &shared, cfg, t, dt, pos);
                    cleared = false;
                }
                None if !cleared => {
                    backend.clear();
                    cracks.clear();
                    effets.oublier_loin();
                    cleared = true;
                }
                None => {}
            }
            // les coups joués pendant ce temps ne partent pas en rafale à la reprise, et
            // l'essaim ne glisse pas depuis l'ancienne place du pointeur ; le premier pas de la
            // reprise part d'ici
            swarm.oublier_coups(&au);
            effets.oublier_coups(&shared);
            // un projecteur demandé pendant la frappe part au retour du pointeur ; en pause ou en
            // plein écran, il est jeté
            if !active {
                effets.oublier_demande();
            }
            last = now;
            // à la reprise, un bouton tenu pendant la pause ne compte pas comme un clic, et le
            // compte à rebours de la veille profonde repart de zéro
            (was_down, droit_avant) = (down, droit);
            last_move = now;
            if profond {
                profond = false;
                charge::publier_veille(false);
            }
            // l'habillage se redessine à 60 i/s ; sans lui, rien n'est dessiné : une lecture
            // toutes les 40 ms suffit (clics, retour du pointeur)
            let habille = active && masque.is_some() && effets.habillage(cfg);
            win::attendre(if !active { Duration::from_millis(100) } else if habille { Duration::from_millis(16) } else { VEILLE_LECTURE });
            continue;
        };
        cleared = false;
        let busy_start = Instant::now();
        let scale = win::scale_at(cur.pos.0, cur.pos.1);
        let cursor = (cur.pos.0 as f32, cur.pos.1 as f32);

        // clic, clic droit, changement de piste : leurs déclencheurs (lus à chaque tour, même
        // sans image à dessiner) ; les fissures demandées s'ouvrent plus bas
        for (ev, oui) in souris {
            if oui {
                swarm.evenement(ev, cursor.0, cursor.1, t, cfg, scale);
            }
        }
        (was_down, droit_avant) = (down, droit);
        if cur.pos != last_pos {
            last_pos = cur.pos;
            last_move = now;
        }

        // veille profonde : souris immobile, ni son ni clic depuis `veille_apres` → fondu, puis
        // plus aucun dessin jusqu'au prochain mouvement, son ou clic
        let calme = now - last_move.max(last_sound).max(last_click);
        let endormi = cfg.rendu.veille_apres > 0 && calme >= Duration::from_secs(cfg.rendu.veille_apres.into());
        if endormi && eveil <= 0.0 {
            // l'habillage (équerres, liseré) reste, comme la bordure : seul l'essaim s'éteint
            let habille = effets.habillage(cfg);
            if !profond {
                profond = true;
                if !habille {
                    backend.clear();
                }
                cracks.clear();
                charge::publier_veille(true);
                log::info!("veille profonde : plus rien n'est dessiné");
            }
            border.tick(true, &cfg.bordure, render::prisme(cfg, t, 1.0), now);
            if habille {
                let dt = (now - last).as_secs_f32().clamp(0.0005, 0.25);
                last = now;
                lire_fenetre(&mut effets, cfg);
                habillage_seul(&mut effets, &mut backend, &monitors, &shared, cfg, t, dt, cur.pos);
            }
            win::attendre(VEILLE_LECTURE);
            continue;
        }
        if profond {
            profond = false;
            charge::publier_veille(false);
            log::info!("fin de la veille profonde");
        }

        // repos : souris immobile et pas de musique depuis 2 s → cadence réduite
        let idle = cfg.rendu.cadence_repos > 0 && now - last_move > IDLE && now - last_sound > IDLE && !down;
        let cadence = match (idle, cfg.rendu.cadence) {
            (true, 0) => cfg.rendu.cadence_repos,
            (true, c) => c.min(cfg.rendu.cadence_repos),
            (false, c) => c,
        };
        cadence_visee = pauses::plafonner(cadence, retenue.plafond);
        let cadence = pauses::plafonner(cadence_visee, adaptatif.plafond());
        // cadence de l'écran avec des écrans à fréquences mélangées : le compositeur suit le plus
        // rapide, `DwmFlush` donnerait sa cadence ; l'attente se cale alors sur la fréquence de
        // l'écran du pointeur (sans synchronisation sur son balayage)
        let cadence = pauses::cadence_ecran(cadence, frequence, frequence_max);
        // cadence choisie : attente jusqu'à l'image suivante, par tranches courtes pour que les
        // clics restent lus (un clic de 60 ms doit être vu)
        if cadence > 0 {
            let echeance = last + Duration::from_secs_f64(1.0 / cadence as f64);
            if now + Duration::from_millis(1) < echeance {
                win::attendre((echeance - now).min(LECTURE_CLICS));
                continue;
            }
        }
        // écart réel depuis l'image précédente : la simulation le découpe en pas courts
        let dt = (now - last).as_secs_f32().clamp(0.0005, 0.25);
        last = now;
        // cadre de la fenêtre au premier plan, relu à chaque image : le halo et les équerres
        // suivent une fenêtre qu'on déplace ou redimensionne
        let murs = lire_fenetre(&mut effets, cfg);
        // fondu de veille : extinction lente, réveil rapide (écart borné : pas de saut au réveil)
        let df = dt.min(render::PAS_MAX);
        eveil = if endormi { (eveil - df / FONDU_EXTINCTION).max(0.0) } else { (eveil + df / FONDU_REVEIL).min(1.0) };

        // simulation ; teinte qui glisse avec la musique, lissée (~150 ms)
        (swarm.murs, swarm.doux) = (murs, doux);
        swarm.avance(t, dt, cursor, au, cfg, scale);
        effets.moduler_figures(swarm.effets_figures());
        effets.bouton = down || droit;
        effets.avance(t, dt, cursor, scale, swarm.env, &shared, cfg, &monitors);
        etiquette.avance(t, dt, cursor, scale, &au, cfg, layer::ecran_du_pointeur(&monitors, cursor));
        // fissures demandées par les déclencheurs (clic, drop…) ; teinte de tout ce qu'Essaim
        // dessine (modulations « sur tout »)
        for (x, y, force) in swarm.fissures.drain(..) {
            if cracks.len() >= MAX_CRACKS {
                cracks.remove(0);
            }
            cracks.push(Crack::new(x, y, t, cfg.fissures.taille * force * scale, &mut rng).a_l_echelle(scale));
        }
        render::set_decalage(swarm.teinte_globale());
        if let Some(c) = icone_appli.suivre(cfg.suit_icone()) {
            render::set_icone(&c);
        }
        // teinte imposée : la pochette du morceau qui joue, sinon l'application ou le site au
        // premier plan, sinon l'éclairage (Prisme) ; suivie en douceur, par le plus court
        let c = &cfg.couleur;
        let imposee = media::teinte_imposee(
            if c.pochette { media::teinte_pochette() } else { None },
            || if c.application || c.site { appli.courante(c.application, c.site, now) } else { None },
            || if c.synchro_prisme { prisme.current() } else { None },
        );
        let cible = imposee.map(|(h, _)| h);
        sync_hue = match (cible, sync_hue) {
            (Some(target), Some(h)) => {
                let d = (target - h + 540.0).rem_euclid(360.0) - 180.0;
                Some((h + d * (dt * 4.0).min(1.0)).rem_euclid(360.0))
            }
            (Some(target), None) => Some(target),
            _ => None,
        };
        render::set_sync_hue(sync_hue);
        let source = imposee.map(|(_, s)| s);
        if source != source_teinte {
            source_teinte = source;
            log::info!("teinte : {}", source.map_or("cycle propre", media::Source::nom));
        }
        let life = cfg.fissures.duree.max(0.2) as f64;
        cracks.retain(|c| t - c.born <= life);
        if cfg.rendu.carte_graphique != backend.wants_gpu() && conf_gen != backend_gen {
            backend = backend::Backend::new(cfg.rendu.carte_graphique);
            effets.oublier_loin();
        }
        backend_gen = conf_gen;

        // dessin : sur chaque écran que l'essaim ou une fissure touche
        let (x0, y0, x1, y1) = swarm.bounds(cursor, scale, cfg.eclat.taille);
        let swarm_rect = (x0.floor() as i32, y0.floor() as i32, x1.ceil() as i32, y1.ceil() as i32);
        // réticule et spectre autour du pointeur : dans la zone de l'essaim ; projecteur : tout
        // l'écran du pointeur, le temps de l'effet seulement
        let swarm_rect = layer::union(Some(swarm_rect), effets.rect_pointeur(cursor, scale, cfg)).unwrap_or(swarm_rect);
        // étiquette du morceau, à côté du pointeur le temps qu'elle se montre
        let swarm_rect = layer::union(Some(swarm_rect), etiquette.rect()).unwrap_or(swarm_rect);
        let mut crack_rects: Vec<layer::IRect> = cracks.iter().map(Crack::rect).collect();
        crack_rects.extend(effets.rect_ecran());
        // équerres et liseré : seulement quand leur dessin change
        crack_rects.extend(effets.zones_loin());
        let rgb = render::prisme(cfg, t, 1.0);
        border.tick(true, &cfg.bordure, rgb, now);
        let env = swarm.env;
        let dessiner = |cv: &mut dyn canvas::Canvas, here: bool| {
            for c in &cracks {
                let age = t - c.born;
                // éclat coloré au début et sur les kicks, fondu sur la dernière seconde
                let glow = ((1.0 - age / 0.45).max(0.0) as f32).max(env * 0.6);
                c.draw(cv, glow, rgb, (life - age).clamp(0.0, 1.0) as f32);
            }
            if here {
                effets.dessiner(cv, t, cfg, scale, cursor);
                swarm.draw(cv, t, cfg, scale, cursor);
                etiquette.dessiner(cv, t, cfg, scale);
            }
        };
        let draw = |cv: &mut dyn canvas::Canvas, here: bool| {
            // le voile du projecteur sous tout le reste, habillage compris (comme le moteur carte
            // graphique, qui le pose avant le lot de sprites) ; il s'estompe avec l'essaim
            if eveil < 1.0 {
                effets.dessiner_voile(&mut canvas::Fondu { cv: &mut *cv, k: eveil }, cfg, t, scale);
            } else {
                effets.dessiner_voile(cv, cfg, t, scale);
            }
            // l'habillage ne s'estompe pas avec l'essaim : il reste pendant la veille
            effets.dessiner_loin(cv, cfg, t, scale);
            if eveil < 1.0 {
                dessiner(&mut canvas::Fondu { cv, k: eveil }, here);
            } else {
                dessiner(cv, here);
            }
        };
        stat_send += backend.frame(&monitors, swarm_rect, &crack_rects, &draw);

        let busy = busy_start.elapsed();
        (charge_busy, charge_frames) = (charge_busy + busy, charge_frames + 1);
        stat_frames += 1;
        stat_time += dt as f64;
        stat_busy += busy;
        stat_max = stat_max.max(busy);
        if now >= next_stat {
            log::info!(
                "images : {:.0}/s, calcul moyen {:.2} ms dont envoi {:.2} ms, pire {:.2} ms",
                stat_frames as f64 / stat_time.max(1e-3),
                stat_busy.as_secs_f64() * 1000.0 / stat_frames.max(1) as f64,
                stat_send.as_secs_f64() * 1000.0 / stat_frames.max(1) as f64,
                stat_max.as_secs_f64() * 1000.0
            );
            (stat_frames, stat_time, stat_busy, stat_send, stat_max, next_stat) =
                (0, 0.0, Duration::ZERO, Duration::ZERO, Duration::ZERO, now + Duration::from_secs(60));
        }
        // cadence de l'écran : une image par rafraîchissement (une cadence choisie attend en haut)
        if cadence == 0 && unsafe { DwmFlush() }.is_err() {
            win::attendre(Duration::from_millis(7));
        }
    }
    drop(son);
    Ok(relancer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_trop_gros_renomme_puis_rouvert() {
        let d = std::env::temp_dir().join(format!("essaim-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("essaim.log");
        std::fs::write(&p, vec![b'x'; JOURNAL_MAX as usize + 1]).unwrap();
        let mut j = Journal { chemin: p.clone(), f: Journal::ouvrir(&p), lignes: JOURNAL_LIGNES, verifie: Instant::now() };
        j.tourner_si_gros();
        let ancien = std::fs::metadata(d.join("essaim.old.log")).unwrap().len();
        assert_eq!(ancien, JOURNAL_MAX + 1);
        assert!(std::fs::metadata(&p).unwrap().len() < 100, "journal neuf");
        assert!(j.f.is_some() && j.lignes == 0);
        // petit : rien ne bouge
        j.tourner_si_gros();
        assert_eq!(std::fs::metadata(d.join("essaim.old.log")).unwrap().len(), ancien);
        drop(j);
        let _ = std::fs::remove_dir_all(&d);
    }
}

//! Fenêtre des réglages : une page web (dossier `ui/`, intégré à l'exécutable) affichée par
//! WebView2 dans une fenêtre Win32, sur son propre fil.
//!
//! La page ne parle à l'appli que par messages JSON (`window.ipc.postMessage`) ; l'appli lui
//! répond et la tient à jour par `essaim.recevoir(…)`. Elle ne touche jamais aux fichiers :
//! les réglages passent par `config.toml` (`config::set_value`, puis `request_reload` pour
//! un effet immédiat), les presets par `profils`. Le son analysé (kicks, basse, note) lui est
//! envoyé 30 fois par seconde pour animer les aperçus.

use crate::audio::AudioShared;
use crate::config::{self, Config};
use crate::profils;
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::num::NonZeroIsize;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, FillRect, GetMonitorInfoW, MonitorFromPoint, UpdateWindow, HDC, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use wry::{WebView, WebViewBuilder, WebViewBuilderExtWindows};

const CLASS: PCWSTR = w!("EssaimReglages");
/// Taille à l'ouverture et taille minimale, à 100 % d'échelle.
const TAILLE: (i32, i32) = (1180, 800);
const TAILLE_MIN: (i32, i32) = (900, 620);
const TIMER_SON: usize = 1;
const TIMER_FICHIERS: usize = 2;
/// Délai au bout duquel la fenêtre se montre même si la page ne s'est pas dite prête.
const TIMER_MONTRER: usize = 3;
const ATTENTE_PAGE_MS: u32 = 3000;
/// Adresse de la page : wry sert le protocole `essaim` sous `http://essaim.localhost/`.
const ORIGINE: &str = "http://essaim.localhost/";

/// Fenêtre ouverte (0 = aucune ; `OUVERTURE` : place réservée, fenêtre en cours de création).
static WINDOW: AtomicIsize = AtomicIsize::new(0);
const OUVERTURE: isize = -1;
/// Message privé de la fenêtre : « reviens devant », envoyé d'un autre fil (`open`).
const WM_DEVANT: u32 = WM_APP + 1;

/// Ouvre la fenêtre, ou la ramène devant si elle l'est déjà. La place est réservée avant de
/// lancer le fil (`compare_exchange`) : deux demandes rapprochées n'ouvrent qu'une fenêtre.
/// Appelée par le fil de rendu : rien ici n'attend le fil de la fenêtre.
pub fn open(son: Arc<AudioShared>) {
    loop {
        match WINDOW.compare_exchange(0, OUVERTURE, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            // déjà en cours d'ouverture : la fenêtre arrive
            Err(OUVERTURE) => return,
            // ouverte : ramenée devant par son propre fil (si elle se ferme à l'instant, le
            // message se perd et son fil libère la place)
            Err(h) if unsafe { IsWindow(Some(HWND(h as *mut _))) }.as_bool() => {
                let hwnd = HWND(h as *mut _);
                unsafe {
                    if IsIconic(hwnd).as_bool() {
                        let _ = ShowWindowAsync(hwnd, SW_RESTORE);
                    }
                    let _ = PostMessageW(Some(hwnd), WM_DEVANT, WPARAM(0), LPARAM(0));
                }
                return;
            }
            // fenêtre détruite, son fil pas encore fini (il n'a pas rendu la place) : reprise,
            // comme pour la fenêtre du stream ; le fil d'avant ne la rendra plus, elle n'est
            // plus la sienne (si elle vient de changer, nouvel essai)
            Err(h) => {
                if WINDOW.compare_exchange(h, OUVERTURE, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                    break;
                }
            }
        }
    }
    let lance = std::thread::Builder::new().name("essaim-reglages".into()).spawn(move || {
        // rend la place à la fin du fil, même s'il panique
        let place = Place(Cell::new(OUVERTURE));
        if let Err(e) = run(son, &place) {
            log::warn!("réglages : fenêtre impossible à ouvrir ({e:#})");
        }
    });
    if let Err(e) = lance {
        let _ = WINDOW.compare_exchange(OUVERTURE, 0, Ordering::AcqRel, Ordering::Acquire);
        log::warn!("réglages : fil impossible à lancer ({e})");
    }
}

/// Place de la fenêtre tenue par le fil des réglages (`WINDOW`) : la valeur qu'il y a mise
/// (`OUVERTURE`, puis la fenêtre). Rendue à sa destruction, seulement si elle est encore la sienne.
struct Place(Cell<isize>);

impl Place {
    /// La fenêtre est créée : elle prend la place réservée.
    fn publier(&self, hwnd: HWND) {
        let h = hwnd.0 as isize;
        if WINDOW.compare_exchange(self.0.get(), h, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            self.0.set(h);
        }
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let _ = WINDOW.compare_exchange(self.0.get(), 0, Ordering::AcqRel, Ordering::Acquire);
    }
}

// ---------------------------------------------------------------------------------------------
// Fichiers de la page

/// Fichiers de `ui/` servis à la page : (chemin, type, contenu).
const FICHIERS: [(&str, &str, &[u8]); 6] = [
    ("index.html", "text/html; charset=utf-8", include_bytes!("../ui/index.html")),
    ("style.css", "text/css; charset=utf-8", include_bytes!("../ui/style.css")),
    ("app.js", "text/javascript; charset=utf-8", include_bytes!("../ui/app.js")),
    ("apercu.js", "text/javascript; charset=utf-8", include_bytes!("../ui/apercu.js")),
    ("lancement.js", "text/javascript; charset=utf-8", include_bytes!("../ui/lancement.js")),
    ("polices/Archivo.ttf", "font/ttf", include_bytes!("../ui/polices/Archivo.ttf")),
];

/// La page ne charge que ses propres fichiers ; seuls l'index de la bibliothèque et les images
/// d'aperçu de ses presets viennent d'ailleurs.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; \
                   img-src 'self' data: https://raw.githubusercontent.com; \
                   connect-src https://raw.githubusercontent.com";

fn servir(req: wry::http::Request<Vec<u8>>) -> wry::http::Response<Cow<'static, [u8]>> {
    let chemin = req.uri().path().trim_start_matches('/');
    let chemin = if chemin.is_empty() { "index.html" } else { chemin };
    let reponse = wry::http::Response::builder().header("Content-Security-Policy", CSP).header("Cache-Control", "no-store");
    match FICHIERS.iter().find(|(c, ..)| *c == chemin) {
        Some((_, mime, octets)) => reponse.status(200).header("Content-Type", *mime).body(Cow::Borrowed(*octets)),
        None => reponse.status(404).body(Cow::Borrowed(&b""[..])),
    }
    .unwrap_or_else(|_| wry::http::Response::new(Cow::Borrowed(&b""[..])))
}

// ---------------------------------------------------------------------------------------------
// Messages de la page

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Demande {
    /// Réglages, presets et dépôt de la bibliothèque.
    Etat,
    /// La page est dessinée : la fenêtre, cachée jusque-là, peut se montrer.
    Prete,
    Regler { section: String, cle: String, valeur: Value },
    Appliquer { nom: String },
    /// Enregistre les réglages actuels sous `nom` ; `publier` ouvre la proposition sur GitHub.
    /// Un autre preset de ce nom n'est remplacé que si `remplacer` (sinon réponse `existe`).
    Enregistrer {
        nom: String,
        publier: bool,
        #[serde(default)]
        remplacer: bool,
    },
    Supprimer { nom: String },
    /// Copie un preset de la bibliothèque dans les presets de l'utilisateur.
    Installer { nom: String, reglages: Value, appliquer: bool },
    OuvrirConfig,
    OuvrirDepot,
    /// Jeux de curseurs de Windows, avec leur aperçu.
    Jeux,
    /// Installe un jeu de curseurs et le garde dans les réglages.
    Souris { jeu: String },
    /// Les 17 curseurs d'un jeu, un par un.
    DetailJeu { cle: String },
    /// Les quatre curseurs montrés sur la carte d'un jeu (et au centre des aperçus).
    ApercuJeu { cle: String },
    Renommer { ancien: String, nouveau: String },
    Dupliquer { nom: String },
    /// Montre le fichier du preset dans l'Explorateur (pour le partager).
    Exporter { nom: String },
    /// Choisit un fichier de preset à importer.
    Importer,
    /// Fichiers glissés sur la fenêtre.
    ImporterFichiers { chemins: Vec<String> },
    /// Revient aux réglages d'avant le dernier preset appliqué.
    Annuler,
    /// Applique des réglages venus de la page (« Surprends-moi »).
    AppliquerReglages { nom: String, reglages: Value },
    Pause,
    Demarrage { actif: bool },
    /// Exécutables des fenêtres ouvertes, pour les règles automatiques.
    Applis,
    /// Enregistre une image GIF (base64) dans les images de l'utilisateur.
    EnregistrerGif { nom: String, donnees: String },
    /// Sorties son de Windows, pour le choix de la sortie écoutée (`[audio] sortie`).
    Sorties,
    /// Applications qui ont du son ouvert, pour le choix des applications écoutées
    /// (`[audio] applis`).
    ApplisSonores,
    /// Ouvre la fenêtre pour le stream (`stream.rs`).
    Stream,
    /// Contours d'une image (pixels RVBA en base64, `largeur` × `hauteur`), pour le trajet
    /// « image » (`contours.rs`).
    Contours { largeur: usize, hauteur: usize, pixels: String, seuil: Option<u8>, inverser: bool, tous: bool },
    /// Contours de l'icône de l'application au premier plan, pour l'aperçu (trajet « icône »).
    Icone,
    /// Cherche une mise à jour tout de suite (`maj::chercher`).
    ChercherMaj,
    /// Redémarre sur la version téléchargée, comme le menu de l'icône.
    Redemarrer,
    /// Zoom de la page au clavier (Ctrl+plus : 1, Ctrl+moins : -1, Ctrl+0 : 0) : WebView2 ne
    /// les traite pas lui-même, ses raccourcis de navigateur étant coupés (Ctrl+P, Ctrl+R…).
    Zoom { pas: i32 },
}

/// Paliers du zoom de la page, comme ceux d'Edge.
const ZOOMS: [f64; 9] = [0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75];
const ZOOM_NORMAL: usize = 4;

/// Zoom après la touche `pas` (1 : plus grand, -1 : plus petit, 0 : normal), depuis le zoom
/// `actuel` de la vue : il peut être entre deux paliers (Ctrl+molette le change par petits pas),
/// le suivant est alors le premier palier au-delà, dans le sens voulu.
fn zoom_suivant(actuel: f64, pas: i32) -> f64 {
    const PRES: f64 = 0.005;
    match pas.signum() {
        0 => ZOOMS[ZOOM_NORMAL],
        1 => ZOOMS.iter().copied().find(|&z| z > actuel + PRES).unwrap_or(ZOOMS[ZOOMS.len() - 1]),
        _ => ZOOMS.iter().rev().copied().find(|&z| z < actuel - PRES).unwrap_or(ZOOMS[0]),
    }
}

struct Fenetre {
    hwnd: HWND,
    vue: WebView,
    son: Arc<AudioShared>,
    /// Dernier son envoyé (kicks et aigus, basse et phase) : rien n'est renvoyé s'il n'a pas changé.
    son_envoye: (u32, u32),
    /// Dates de `config.toml` et du dossier des presets au dernier état envoyé.
    dates: (Option<SystemTime>, Option<SystemTime>),
    /// Numéro de la liste des presets envoyée en dernier à la page (`profils::generation_liste`) :
    /// elle n'est renvoyée que s'il change, ou si la page (re)chargée redemande son état.
    presets_envoyes: Option<u64>,
    /// Aperçus des jeux de curseurs déjà dessinés (clé → images).
    apercus: std::collections::HashMap<String, Value>,
    /// Réglages et curseurs d'avant le dernier preset appliqué (pour « Annuler »).
    avant: Option<Retour>,
    /// La page a demandé son état : elle est chargée et sait recevoir. Le son, la charge et la
    /// mise à jour ne lui sont envoyés qu'ensuite (avant, ils seraient perdus).
    ecoute: bool,
    /// Jeux de curseurs de Windows, lus une fois par ouverture (relus à chaque liste demandée).
    jeux: Option<Vec<crate::souris::Jeu>>,
    /// Fenêtre réduite : la vue est cachée et le son n'est plus envoyé.
    reduite: bool,
    /// La fenêtre a été montrée (elle naît cachée, le temps que la page se dessine).
    montree: bool,
    /// Dernier état de la mise à jour envoyé à la page (rien au départ).
    maj: Option<crate::maj::Etat>,
}

thread_local! {
    static FENETRE: RefCell<Option<Fenetre>> = const { RefCell::new(None) };
    /// Messages reçus de la page, traités après chaque message Windows (le gestionnaire de
    /// wry n'a pas accès à la vue).
    static RECUS: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    /// Fermeture demandée pendant qu'une demande de la page était traitée (la vue ne pouvait pas
    /// être détruite à ce moment) : refaite après elle.
    static FERMER: Cell<bool> = const { Cell::new(false) };
}

/// De quoi revenir en arrière après un preset appliqué par la fenêtre (« Annuler »).
struct Retour {
    /// Réglages et curseurs d'avant.
    avant: profils::Avant,
    /// Réglages écrits par la fenêtre depuis (le preset, puis ses retouches dans la page) ;
    /// `None` s'ils n'ont pas pu être relus. Si `config.toml` ne les a plus (preset changé par
    /// l'icône, Horizon, une règle), « Annuler » déferait ce changement-là : il est oublié.
    apres: Option<Config>,
}

/// Minuteries de la fenêtre (identifiant, période en ms) : aucune avant que la page ait demandé
/// son état (`ecoute`), elle ne recevrait rien ; pas le son quand la fenêtre est réduite.
fn minuteries(ecoute: bool, reduite: bool) -> Vec<(usize, u32)> {
    [(TIMER_SON, 33), (TIMER_FICHIERS, 700)].into_iter().filter(|&(t, _)| ecoute && !(reduite && t == TIMER_SON)).collect()
}

/// Lit une demande de la page ; sinon, la réponse d'erreur à lui envoyer (la cause va au journal).
fn lire_demande(texte: &str) -> Result<Demande, Value> {
    serde_json::from_str::<Demande>(texte).map_err(|e| {
        log::warn!("réglages : message de la page illisible ({e})");
        json!({ "type": "erreur", "texte": "Demande non comprise." })
    })
}

/// Réponse `applique` à un preset appliqué (son bandeau propose « Annuler ») ; des curseurs non
/// installés n'y font pas un message à part (qui effacerait le bandeau ou serait effacé par lui) :
/// la page montre `avertissement` dans le bandeau.
fn applique(nom: &str, curseurs: Result<(), String>) -> Value {
    match curseurs {
        Ok(()) => json!({ "type": "applique", "nom": nom }),
        Err(e) => json!({ "type": "applique", "nom": nom, "avertissement": format!("Preset appliqué, mais curseurs non installés : {e}.") }),
    }
}

/// Réponses à un preset de la bibliothèque installé : le message, puis, s'il devait être
/// appliqué, `applique` (dernier, comme pour `Appliquer`, avec l'avertissement des curseurs s'il
/// y en a un), ou l'erreur d'application.
fn reponses_installation(nom: &str, application: Option<Result<(), String>>, curseurs: Result<(), String>) -> Vec<Value> {
    let mut r = vec![json!({ "type": "message", "texte": format!("Preset « {nom} » installé dans Mes presets.") })];
    match application {
        Some(Ok(())) => r.push(applique(nom, curseurs)),
        Some(Err(e)) => r.push(json!({ "type": "erreur", "texte": format!("Preset « {nom} » installé mais non appliqué : {e}.") })),
        None => {}
    }
    r
}

/// Réglage que `config.toml` n'a pas pris : la page retire la valeur qu'elle tenait en attente
/// (seulement si c'est encore celle-là : `valeur`, la valeur refusée, est jointe).
fn reglage_refuse(section: &str, cle: &str, valeur: &Value) -> Value {
    json!({ "type": "reglage_refuse", "section": section, "cle": cle, "valeur": valeur })
}

/// Réponse `contours` à une image illisible : liste vide, la page n'attend plus son analyse.
fn contours_vides(seuil: Option<u8>) -> Value {
    json!({ "type": "contours", "liste": [], "seuil": seuil.unwrap_or(128), "transparence": false })
}

fn dates() -> (Option<SystemTime>, Option<SystemTime>) {
    let date = |p: std::path::PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    (date(config::path()), date(profils::dir()))
}

impl Fenetre {
    fn envoyer(&self, v: &Value) {
        if let Err(e) = self.vue.evaluate_script(&format!("essaim.recevoir({v})")) {
            log::warn!("réglages : envoi à la page impossible ({e})");
        }
    }

    fn etat(&mut self) {
        self.dates = dates();
        // un fichier illisible (faute de frappe) : la page le dit, au lieu de montrer et
        // d'enregistrer des réglages par défaut sans rapport avec ce qui tourne
        let lu = Config::read(&config::path());
        let illisible = lu.as_ref().err().map(|e| e.to_string());
        // réglages changés hors de la fenêtre depuis son dernier preset : plus rien à annuler
        if let (Ok(cfg), Some(Retour { apres: Some(apres), .. })) = (&lu, &self.avant) {
            if !profils::same(apres, cfg) {
                self.avant = None;
            }
        }
        let cfg = lu.unwrap_or_default();
        let actif = profils::en_place(&cfg);
        // la liste des presets (plusieurs Mo de JSON) n'est renvoyée que si elle a changé depuis
        // le dernier envoi ; sinon `presets` manque et la page garde la sienne
        let generation = profils::generation_liste();
        let envoyer_liste = self.presets_envoyes != Some(generation);
        let liste = if envoyer_liste || actif.is_none() { profils::list() } else { Vec::new() };
        // sans preset identique : le plus proche, s'il ne diffère que de quelques réglages
        let proche = if actif.is_some() {
            None
        } else {
            liste.iter().map(|p| (profils::ecarts(&p.cfg, &cfg), p)).filter(|(n, _)| *n <= 6).min_by_key(|(n, _)| *n)
        };
        let date = |p: &profils::Profile| {
            p.date.and_then(|d| d.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs())
        };
        let presets: Option<Vec<Value>> = envoyer_liste
            .then(|| liste.iter().map(|p| json!({ "nom": p.name, "fourni": p.builtin, "reglages": p.cfg, "date": date(p) })).collect());
        let curseurs = crate::souris::actuel();
        let curseurs_nom = self.nom_du_jeu(&curseurs);
        let refus = crate::win::raccourcis_refuses();
        let demarrage = demarrage_actif();
        let (demarrage_coche, demarrage_ailleurs) = demarrage.json();
        let mut etat = json!({
            "type": "etat",
            "reglages": cfg,
            "actif": actif,
            "curseurs": curseurs,
            // nom affiché du jeu (« Windows par défaut » plutôt que « @main.cpl,-1020 »)
            "curseurs_nom": curseurs_nom,
            // raccourcis clavier non enregistrés (« Ctrl+Alt+F »…), et pourquoi : pris par une
            // autre application, ou la touche écrit un caractère avec AltGr sur ce clavier
            "raccourcis_refuses": refus.iter().map(|r| r.raccourci).collect::<Vec<_>>(),
            "raccourcis_raisons": refus.iter().map(|r| json!({ "raccourci": r.raccourci, "raison": r.raison })).collect::<Vec<_>>(),
            "proche": proche.map(|(n, p)| json!({ "nom": p.name, "ecarts": n })),
            "defauts": Config::default(),
            // coché seulement pour l'exécutable en cours ; inscrit pour un autre : son chemin
            "demarrage": demarrage_coche,
            "demarrage_ailleurs": demarrage_ailleurs,
            "version": env!("CARGO_PKG_VERSION"),
            "illisible": illisible,
            "prisme": crate::prisme::present(),
        });
        if let Some(presets) = presets {
            etat["presets"] = Value::Array(presets);
            self.presets_envoyes = Some(generation);
        }
        self.envoyer(&etat);
    }

    /// État de la mise à jour, envoyé à la page quand il change.
    fn maj(&mut self) {
        let etat = crate::maj::etat();
        if self.maj.as_ref() != Some(&etat) {
            let mut m = etat.json();
            m["type"] = json!("maj");
            self.envoyer(&m);
            self.maj = Some(etat);
        }
    }

    /// Réduite : la vue est cachée (WebView2 suspend alors son rendu) et le son n'est plus
    /// envoyé ; tout reprend à la restauration.
    fn reduire(&mut self, reduite: bool) {
        if reduite == self.reduite {
            return;
        }
        self.reduite = reduite;
        if let Err(e) = self.vue.set_visible(!reduite) {
            log::warn!("réglages : vue non {} ({e})", if reduite { "cachée" } else { "montrée" });
        }
        if reduite {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_SON);
            }
        } else {
            self.son_envoye = (u32::MAX, 0);
            self.armer();
        }
    }

    /// Arme les minuteries que demandent l'état de la page et de la fenêtre (`minuteries`).
    fn armer(&self) {
        for (t, ms) in minuteries(self.ecoute, self.reduite) {
            unsafe {
                SetTimer(Some(self.hwnd), t, ms, None);
            }
        }
    }

    /// Note les réglages d'avant un preset que la fenêtre vient d'appliquer, et ceux d'après.
    fn noter_avant(&mut self, avant: Option<profils::Avant>) {
        self.avant = avant.map(|avant| Retour { avant, apres: Config::read(&config::path()).ok() });
    }

    fn message(&self, texte: &str) {
        self.envoyer(&json!({ "type": "message", "texte": texte }));
    }

    fn erreur(&self, texte: &str) {
        self.envoyer(&json!({ "type": "erreur", "texte": texte }));
    }

    fn traiter(&mut self, texte: &str) {
        let demande = match lire_demande(texte) {
            Ok(d) => d,
            Err(reponse) => return self.envoyer(&reponse),
        };
        match demande {
            Demande::Etat => {
                // première demande : la page est chargée, les envois réguliers commencent
                if !self.ecoute {
                    self.ecoute = true;
                    self.armer();
                }
                // page (re)chargée : elle n'a aucune liste, renvoyée en entier
                self.presets_envoyes = None;
                self.etat();
                // la page (re)chargée n'a pas l'état de la mise à jour : renvoyé
                self.maj = None;
                self.maj();
            }
            Demande::Prete => self.montrer(),
            Demande::Regler { section, cle, valeur } => {
                // changement venu d'ailleurs pas encore vu (`tick`) : il n'est pas adopté ci-dessous
                if let Some(Retour { apres: Some(apres), .. }) = &self.avant {
                    if Config::read(&config::path()).is_ok_and(|cfg| !profils::same(apres, &cfg)) {
                        self.avant = None;
                    }
                }
                match config::set_value(&section, &cle, &valeur) {
                    Ok(()) => {
                        config::request_reload();
                        // retouche faite dans la fenêtre : « Annuler » revient toujours d'avant le preset
                        if let Some(r) = self.avant.as_mut() {
                            r.apres = Config::read(&config::path()).ok();
                        }
                    }
                    Err(e) => {
                        self.erreur(&format!("Réglage refusé : {e}."));
                        // la page retire la valeur en attente, puis revient à la vraie valeur
                        self.envoyer(&reglage_refuse(&section, &cle, &valeur));
                        self.etat();
                    }
                }
            }
            Demande::Appliquer { nom } => {
                let liste = profils::list();
                let avant = profils::capturer();
                match liste.iter().find(|p| profils::meme_nom(&p.name, &nom)).map(|p| (p, profils::apply(p))) {
                    Some((p, Ok(()))) => {
                        config::request_reload();
                        let curseurs = profils::curseurs_du_preset(&p.cfg);
                        self.noter_avant(avant);
                        self.envoyer(&applique(&nom, curseurs));
                    }
                    Some((_, Err(e))) => self.erreur(&format!("Preset « {nom} » non appliqué : {e}.")),
                    None => self.erreur(&format!("Preset « {nom} » introuvable.")),
                }
                self.etat();
            }
            Demande::Enregistrer { nom, publier, remplacer } => {
                let mut cfg = match Config::read(&config::path()) {
                    Ok(c) => c,
                    Err(e) => {
                        let texte = format!("Preset non enregistré : config.toml est illisible ({e}). Corrige le fichier d'abord.");
                        self.erreur(&texte);
                        // réponse propre à l'enregistrement : la page libère son bouton
                        return self.envoyer(&json!({ "type": "refus", "texte": texte }));
                    }
                };
                // un autre preset de ce nom : la page demande d'abord s'il faut le remplacer (le
                // preset en place, lui, se met à jour sans question)
                if !remplacer && profils::existe(&nom) {
                    let actif = profils::list().iter().any(|p| profils::meme_nom(&p.name, &nom) && profils::same(&p.cfg, &cfg));
                    if !actif {
                        return self.envoyer(&json!({ "type": "existe", "nom": nom.trim() }));
                    }
                }
                // un preset remplacé garde le nom de son fichier (« Soirée » remplacé sous
                // « SOIRÉE » n'est pas renommé)
                let nom = profils::nom_existant(&nom).unwrap_or(nom);
                // le preset garde le jeu de curseurs installé en ce moment
                cfg.souris.jeu = crate::souris::actuel();
                match profils::save(&nom, &cfg) {
                    Ok(nom) => {
                        self.etat();
                        self.envoyer(&json!({ "type": "enregistre", "nom": nom }));
                        if publier {
                            self.publier(&nom);
                        } else {
                            self.message(&format!("Preset « {nom} » enregistré."));
                        }
                    }
                    Err(e) => self.envoyer(&json!({ "type": "refus", "texte": format!("{}.", majuscule(&e)) })),
                }
            }
            Demande::Supprimer { nom } => match profils::delete(&nom) {
                Ok(()) => {
                    self.etat();
                    self.message(&format!("Preset « {} » supprimé.", nom.trim()));
                }
                Err(e) => self.erreur(&format!("{}.", majuscule(&e))),
            },
            Demande::Installer { nom, reglages, appliquer } => {
                // comme un fichier importé : réglages d'avant convertis, puis bornés (sinon le
                // même preset, réinstallé, ne serait plus reconnu)
                let cfg = config_distante(reglages);
                // un preset de l'utilisateur du même nom (à la casse près) n'est pas écrasé
                match cfg.and_then(|cfg| profils::ajouter(&nom, &cfg).map(|n| (n, cfg))) {
                    Ok((nom, cfg)) => {
                        let mut curseurs = Ok(());
                        let application = appliquer.then(|| {
                            let avant = profils::capturer();
                            let p = profils::list().into_iter().find(|p| profils::meme_nom(&p.name, &nom));
                            let r = p.as_ref().map_or_else(|| Err("preset introuvable après l'installation".to_owned()), profils::apply);
                            if r.is_ok() {
                                config::request_reload();
                                self.noter_avant(avant);
                                curseurs = profils::curseurs_du_preset(&cfg);
                            }
                            r
                        });
                        self.etat();
                        for m in reponses_installation(&nom, application, curseurs) {
                            self.envoyer(&m);
                        }
                    }
                    Err(e) => self.erreur(&format!("Installation impossible : {e}.")),
                }
            }
            Demande::OuvrirConfig => crate::win::open_file(&config::path()),
            Demande::Jeux => {
                // la liste est relue (un pack a pu être installé depuis l'ouverture)
                let jeux = crate::souris::jeux();
                let liste: Vec<Value> = jeux.iter().map(|j| json!({ "cle": j.cle, "nom": j.nom, "systeme": j.systeme })).collect();
                self.jeux = Some(jeux);
                // un jeu réinstallé sous le même nom a pu changer : aperçus redessinés
                self.apercus.clear();
                self.envoyer(&json!({ "type": "jeux", "jeux": liste }));
            }
            Demande::ApercuJeu { cle } => {
                if !self.apercus.contains_key(&cle) {
                    // jeu inconnu (preset venu d'une autre machine) : réponse vide, la page
                    // n'attend pas
                    let apercu = self.jeux().iter().find(|j| j.cle == cle).map_or(json!([]), |j| json!(crate::souris::apercu(j)));
                    self.apercus.insert(cle.clone(), apercu);
                }
                self.envoyer(&json!({ "type": "apercu_jeu", "cle": cle, "curseurs": self.apercus[&cle] }));
            }
            Demande::DetailJeu { cle } => {
                let (nom, curseurs) = match self.jeux().iter().find(|j| j.cle == cle) {
                    Some(j) => (
                        j.nom.clone(),
                        crate::souris::curseurs(j).into_iter().map(|(nom, image)| json!({ "nom": nom, "image": image })).collect(),
                    ),
                    None => (cle.clone(), Vec::<Value>::new()),
                };
                self.envoyer(&json!({ "type": "detail_jeu", "cle": cle, "nom": nom, "curseurs": curseurs }));
            }
            Demande::Souris { jeu } => {
                match crate::souris::installer(&jeu).and_then(|()| config::set_value("souris", "jeu", &json!(jeu))) {
                    Ok(()) => {
                        config::request_reload();
                        let nom = self.nom_du_jeu(&jeu);
                        self.message(&format!("Curseurs « {nom} » installés."));
                    }
                    Err(e) => self.erreur(&format!("Curseurs non installés : {e}.")),
                }
                // Ctrl+Z ne défait pas un choix de curseurs (il remettrait aussi ceux d'avant le
                // dernier preset)
                self.avant = None;
                self.etat();
            }
            Demande::Renommer { ancien, nouveau } => match profils::rename(&ancien, &nouveau) {
                Ok(nom) => {
                    self.etat();
                    self.message(&format!("Preset renommé en « {nom} »."));
                }
                Err(e) => self.erreur(&format!("{}.", majuscule(&e))),
            },
            Demande::Dupliquer { nom } => match profils::dupliquer(&nom) {
                Ok(copie) => {
                    self.etat();
                    self.message(&format!("Copie « {copie} » ajoutée à Mes presets."));
                }
                Err(e) => self.erreur(&format!("{}.", majuscule(&e))),
            },
            Demande::Exporter { nom } => {
                let f = match profils::fichier_verifie(&nom) {
                    Ok(f) => f,
                    Err(e) => return self.erreur(&format!("{}.", majuscule(&e))),
                };
                if f.exists() {
                    montrer_dans_l_explorateur(&f);
                } else {
                    self.erreur(&format!("« {nom} » est un preset fourni : duplique-le pour avoir son fichier."));
                }
            }
            Demande::Importer => {
                if let Some(f) = choisir_fichier(self.hwnd) {
                    self.importer(&[f]);
                }
            }
            Demande::ImporterFichiers { chemins } => {
                // fichiers glissés dans la fenêtre : disques de ce PC seulement (un chemin réseau
                // `\\serveur\…` ferait envoyer l'identité Windows à ce serveur)
                let (locaux, distants): (Vec<std::path::PathBuf>, Vec<_>) =
                    chemins.into_iter().map(std::path::PathBuf::from).partition(|c| profils::chemin_local(c));
                if !distants.is_empty() {
                    self.erreur("Fichiers sur le réseau ignorés : copie-les d'abord sur ton PC.");
                }
                // une image : pour le trajet « image » (la page la décode) ; le reste : des presets
                let (images, presets): (Vec<_>, Vec<_>) = locaux.into_iter().partition(|c| type_image(c).is_some());
                if let Some(c) = images.first() {
                    match image_glissee(c) {
                        Ok(m) => self.envoyer(&m),
                        Err(e) => self.erreur(&e),
                    }
                }
                self.importer(&presets);
            }
            Demande::Contours { largeur, hauteur, pixels, seuil, inverser, tous } => {
                // dimensions bornées avant de les multiplier (pas de débordement)
                let taille = (largeur <= 4096 && hauteur <= 4096).then(|| largeur * hauteur).filter(|&n| n <= 1024 * 1024);
                let px = depuis_base64(&pixels).filter(|px| taille.is_some_and(|n| px.len() == n * 4));
                match px {
                    Some(px) => {
                        let r = crate::contours::contours(&px, largeur, hauteur, &crate::contours::Options { seuil, inverser, tous });
                        self.envoyer(&json!({ "type": "contours", "liste": r.contours, "seuil": r.seuil, "transparence": r.transparence }));
                    }
                    None => {
                        self.erreur("Image illisible.");
                        self.envoyer(&contours_vides(seuil));
                    }
                }
            }
            Demande::Annuler => match self.avant.take() {
                Some(Retour { avant, .. }) => {
                    match profils::remettre(&avant) {
                        Ok(()) => self.message("Réglages d'avant remis."),
                        Err(e) => self.erreur(&format!("Retour impossible : {e}.")),
                    }
                    self.etat();
                }
                None => self.erreur("Rien à annuler."),
            },
            // comme un preset installé : réglages d'avant convertis, puis bornés
            Demande::AppliquerReglages { nom, reglages } => match config_distante(reglages) {
                Ok(cfg) => {
                    let avant = profils::capturer();
                    // compté comme un preset appliqué : les traînées repartent de zéro
                    match profils::ecrire_d_un_coup(&cfg, |s, k| profils::in_profile(s, k) && (s, k) != ("souris", "jeu")) {
                        Ok(()) => {
                            config::request_reload();
                            self.noter_avant(avant);
                            self.envoyer(&json!({ "type": "applique", "nom": nom }));
                        }
                        Err(e) => self.erreur(&format!("Réglages refusés : {e}.")),
                    }
                    self.etat();
                }
                Err(e) => self.erreur(&format!("Réglages refusés : {e}.")),
            },
            Demande::Pause => commande(crate::win::CMD_PAUSE),
            Demande::Icone => {
                let liste: Vec<Vec<[f32; 2]>> = crate::render::contours_icone();
                self.envoyer(&json!({ "type": "icone", "liste": liste }));
            }
            Demande::Stream => commande(crate::win::CMD_STREAM),
            Demande::ChercherMaj => {
                crate::maj::chercher();
                self.maj();
            }
            Demande::Redemarrer => commande(crate::win::CMD_REDEMARRER),
            Demande::Zoom { pas } => {
                // zoom relu à chaque fois : Ctrl+molette le change sans le dire
                use wry::WebViewExtWindows;
                let mut actuel = 1.0;
                if let Err(e) = unsafe { self.vue.controller().ZoomFactor(&mut actuel) } {
                    log::warn!("réglages : zoom actuel illisible ({e})");
                }
                if let Err(e) = self.vue.zoom(zoom_suivant(actuel, pas)) {
                    log::warn!("réglages : zoom impossible ({e})");
                }
            }
            Demande::Demarrage { actif } => {
                match regler_demarrage(actif) {
                    Ok(()) if actif => {
                        let exe = std::env::current_exe().unwrap_or_default();
                        self.message(&message_demarrage_active(&exe, &dossiers_provisoires()));
                    }
                    Ok(()) => self.message("Essaim ne démarrera plus avec Windows."),
                    Err(e) => self.erreur(&format!("Démarrage avec Windows non modifié : {e}.")),
                }
                self.etat();
            }
            Demande::Applis => self.envoyer(&json!({ "type": "applis", "applis": crate::appli::applis_ouvertes(), "recente": crate::pauses::recente() })),
            Demande::EnregistrerGif { nom, donnees } => match enregistrer_gif(&nom, &donnees) {
                Ok(f) => {
                    montrer_dans_l_explorateur(&f);
                    self.message(&format!("Aperçu enregistré : {}.", f.display()));
                }
                Err(e) => self.erreur(&format!("GIF non enregistré : {e}.")),
            },
            Demande::Sorties => {
                let liste: Vec<Value> = audio_commun::capture::sorties().into_iter().map(|(id, nom)| json!({ "id": id, "nom": nom })).collect();
                self.envoyer(&json!({ "type": "sorties", "liste": liste }));
            }
            Demande::ApplisSonores => {
                let liste: Vec<Value> = audio_commun::capture::applis_sonores_detail().into_iter().map(|a| json!({ "exe": a.exe, "nom": a.nom, "joue": a.joue })).collect();
                self.envoyer(&json!({ "type": "applis_sonores", "liste": liste }));
            }
            Demande::OuvrirDepot => {
                let depot = Config::read(&config::path()).unwrap_or_default().bibliotheque.depot;
                crate::win::open_url(&format!("https://github.com/{depot}"));
            }
        }
    }

    /// Jeux de curseurs, lus au premier besoin.
    fn jeux(&mut self) -> &[crate::souris::Jeu] {
        self.jeux.get_or_insert_with(crate::souris::jeux)
    }

    /// Nom affiché du jeu de curseurs `cle` (sa clé s'il est inconnu, vide si `cle` l'est).
    fn nom_du_jeu(&mut self, cle: &str) -> String {
        if cle.is_empty() {
            return String::new();
        }
        self.jeux().iter().find(|j| j.cle == cle).map_or_else(|| cle.to_owned(), |j| j.nom.clone())
    }

    fn importer(&mut self, chemins: &[std::path::PathBuf]) {
        let (mut faits, mut erreurs) = (Vec::new(), Vec::new());
        for c in chemins {
            match profils::importer(c) {
                Ok(nom) => faits.push(nom),
                Err(e) => erreurs.push(e),
            }
        }
        self.etat();
        if !faits.is_empty() {
            self.message(&format!("Importé dans Mes presets : {}.", faits.join(", ")));
        }
        if !erreurs.is_empty() {
            self.erreur(&format!("{}.", majuscule(&erreurs.join(" ; "))));
        }
    }

    /// Ouvre dans le navigateur la création du fichier du preset dans le dépôt de la
    /// bibliothèque : GitHub propose alors une copie du dépôt et une pull request.
    fn publier(&self, nom: &str) {
        let depot = Config::read(&config::path()).unwrap_or_default().bibliotheque.depot;
        let texte = match std::fs::read_to_string(profils::fichier(nom)) {
            Ok(t) => t,
            Err(e) => return self.erreur(&format!("Preset « {nom} » enregistré, mais illisible pour la publication ({e}).")),
        };
        let url = adresse_de_publication(&depot, nom, &texte);
        // une adresse trop longue est refusée par GitHub : le preset passe alors par le
        // presse-papiers, et le fichier ouvert dit de le coller
        if url.len() <= URL_MAX {
            crate::win::open_url(&url);
            self.message(&format!("Preset « {nom} » enregistré. GitHub s'ouvre pour le proposer à la bibliothèque."));
            return;
        }
        if let Err(e) = copier(&texte) {
            return self.erreur(&format!(
                "Preset « {nom} » enregistré, mais trop long pour l'adresse et non copié ({e}) : exporte-le pour le publier."
            ));
        }
        crate::win::open_url(&adresse_de_publication(&depot, nom, A_COLLER));
        self.message(&format!(
            "Preset « {nom} » enregistré et copié : GitHub s'ouvre, colle-le (Ctrl+A, Ctrl+V) dans le fichier pour le proposer à la bibliothèque."
        ));
    }

    /// Montre la fenêtre, une fois : quand la page se dit prête, ou au bout de l'attente.
    fn montrer(&mut self) {
        if self.montree {
            return;
        }
        self.montree = true;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_MONTRER);
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            // le fond est peint tout de suite (WM_ERASEBKGND), avant que la fenêtre soit à l'écran
            let _ = UpdateWindow(self.hwnd);
            let _ = SetForegroundWindow(self.hwnd);
        }
        let _ = self.vue.focus();
    }

    /// Son et fichiers : envoyés à la page quand ils changent.
    fn tick(&mut self, quoi: usize) {
        if quoi == TIMER_MONTRER {
            log::warn!("réglages : la page ne s'est pas dite prête, fenêtre montrée quand même");
            return self.montrer();
        }
        if quoi == TIMER_SON && !self.reduite {
            let au = self.son.view();
            let compteurs = au.kicks ^ au.aigus.rotate_left(16) ^ au.caisses.rotate_left(8) ^ au.drops.rotate_left(24);
            let cle = (compteurs, au.bass.to_bits() ^ au.phase.to_bits() ^ au.tension.to_bits().rotate_left(7) ^ au.balance.to_bits().rotate_left(13));
            if cle != self.son_envoye {
                self.son_envoye = cle;
                // tempo : `bpm` absent (null) tant qu'il n'est pas stable ; `relais` = break sans
                // kick, l'essaim pulse alors sur `phase` ; caisses claires, drops, montée
                // (`tension`) et côté qui joue le plus fort (`balance`)
                self.envoyer(&json!({
                    "type": "son", "kicks": au.kicks, "basse": au.bass, "hauteur": au.pitch, "aigus": au.aigus, "brillance": au.brillance,
                    "teinte": crate::render::sync_hue(), "bpm": au.bpm, "phase": au.phase, "relais": au.relais,
                    "caisses": au.caisses, "drops": au.drops, "tension": au.tension, "balance": au.balance,
                }));
            }
        } else if quoi == TIMER_FICHIERS {
            let (ms, ips) = crate::charge::lire();
            self.envoyer(&json!({
                "type": "charge",
                "ms": ms,
                "ips": ips,
                "pause": crate::charge::pause(),
                "capture": crate::charge::capture(),
                "veille": crate::charge::veille(),
                // morceau en cours ({titre, artiste} ou null), lecture (null sans session) et compteur des
                // changements de piste
                "morceau": crate::media::json(),
                "lecture": crate::media::en_lecture(),
                "pistes": crate::media::pistes(),
                "raison": crate::charge::raison(),
                "prisme": crate::prisme::present(),
                // écoute par application refusée par Windows : tout le son est écouté en attendant
                "repli": self.son.repli_applis(),
            }));
            if dates() != self.dates {
                self.etat();
            }
            self.maj();
        }
    }
}

/// Envoie une commande du menu de l'icône (pause, fenêtre pour le stream, redémarrage) à la
/// boucle principale.
fn commande(cmd: usize) {
    unsafe {
        if let Ok(h) = FindWindowW(crate::win::TRAY_CLASS, None) {
            let _ = PostMessageW(Some(h), WM_COMMAND, WPARAM(cmd), LPARAM(0));
        }
    }
}

const CLE_RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
/// Entrées de démarrage coupées ou permises dans le Gestionnaire des tâches (onglet Démarrage).
const CLE_RUN_APPROUVE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");

/// Démarrage avec la session, tel que la page le montre.
#[derive(Debug, PartialEq)]
enum Demarrage {
    /// Inscrit pour l'exécutable en cours, et permis.
    Actif,
    /// Inscrit et permis, mais pour un autre exécutable (déplacé, autre copie) : c'est lui qui
    /// démarrera, ou rien s'il n'existe plus.
    Ailleurs(String),
    /// Pas inscrit, ou coupé dans le Gestionnaire des tâches.
    Inactif,
}

impl Demarrage {
    /// Pour la page : `demarrage` (coché ou non) et `demarrage_ailleurs` (chemin inscrit, ou null).
    fn json(&self) -> (bool, Option<&str>) {
        match self {
            Demarrage::Actif => (true, None),
            Demarrage::Ailleurs(chemin) => (false, Some(chemin.as_str())),
            Demarrage::Inactif => (false, None),
        }
    }
}

/// Chemin de l'exécutable dans une valeur de la clé Run : entre guillemets (suivi d'éventuels
/// arguments) ; sans guillemets, jusqu'à `.exe` compris (les arguments qui suivent sont laissés),
/// ou toute la valeur s'il n'y en a pas.
fn exe_de_la_commande(valeur: &str) -> &str {
    let v = valeur.trim();
    match v.strip_prefix('"') {
        Some(reste) => reste.split('"').next().unwrap_or(reste).trim(),
        // premier `.exe` en fin de valeur ou suivi d'un espace (pas un dossier « x.exes »)
        None => v
            .to_ascii_lowercase()
            .match_indices(".exe")
            .map(|(i, _)| i + 4)
            .find(|&f| v[f..].is_empty() || v[f..].starts_with(char::is_whitespace))
            .map_or(v, |f| &v[..f]),
    }
}

/// Même fichier : chemins égaux sans guillemets ni casse, ou mêmes chemins canoniques (nom court
/// `ESSAIM~1`, lien, `..` : `std::fs::canonicalize`, qui demande que le fichier existe).
fn meme_exe(a: &std::path::Path, b: &std::path::Path) -> bool {
    let texte = |p: &std::path::Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
    if texte(a) == texte(b) {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => texte(&ca) == texte(&cb),
        _ => false,
    }
}

/// État du démarrage d'après la valeur `Essaim` de la clé Run (`valeur`), sa permission dans le
/// Gestionnaire des tâches (`permis`) et l'exécutable en cours (`exe`) : `meme_exe`.
fn etat_demarrage(valeur: Option<&str>, permis: bool, exe: &std::path::Path) -> Demarrage {
    let Some(valeur) = valeur.filter(|_| permis) else { return Demarrage::Inactif };
    let inscrit = exe_de_la_commande(valeur);
    if meme_exe(std::path::Path::new(inscrit), exe) {
        Demarrage::Actif
    } else {
        Demarrage::Ailleurs(inscrit.to_owned())
    }
}

/// Argument de la commande inscrite dans la clé Run : lancement automatique à l'ouverture de
/// session. Une seconde instance lancée avec lui sort en silence ; sans lui (double-clic), elle
/// ouvre les réglages de l'instance en cours.
pub const ARG_DEMARRAGE: &str = "--demarrage";

/// Commande inscrite dans la clé Run pour `exe`.
fn commande_demarrage(exe: &std::path::Path) -> String {
    format!("\"{}\" {ARG_DEMARRAGE}", exe.display())
}

/// La commande de la clé Run porte-t-elle `ARG_DEMARRAGE` (après le chemin de l'exécutable) ?
fn avec_arg_demarrage(valeur: &str) -> bool {
    let v = valeur.trim();
    let exe = exe_de_la_commande(v);
    // reste après le chemin (et son guillemet fermant, s'il y en a un)
    let reste = v.find(exe).map_or("", |i| &v[i + exe.len()..]).trim_start_matches('"');
    reste.split_whitespace().any(|a| a.eq_ignore_ascii_case(ARG_DEMARRAGE))
}

/// Valeur `Essaim` de la clé Run de l'utilisateur, s'il y en a une.
fn valeur_run() -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    // la commande inscrite (taille demandée d'abord, en octets)
    let mut taille = 0u32;
    unsafe {
        if RegGetValueW(HKEY_CURRENT_USER, CLE_RUN, w!("Essaim"), RRF_RT_REG_SZ, None, None, Some(&mut taille)).is_err() {
            None
        } else {
            let mut tampon = vec![0u16; (taille as usize).div_ceil(2).max(1)];
            let mut n = (tampon.len() * 2) as u32;
            RegGetValueW(HKEY_CURRENT_USER, CLE_RUN, w!("Essaim"), RRF_RT_REG_SZ, None, Some(tampon.as_mut_ptr() as *mut _), Some(&mut n))
                .is_ok()
                .then(|| String::from_utf16_lossy(&tampon[..(n as usize / 2).min(tampon.len())]).trim_end_matches('\0').to_owned())
        }
    }
}

/// Écrit la valeur `Essaim` de la clé Run.
fn ecrire_run(valeur: &str) -> Result<(), String> {
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let octets: Vec<u8> = valeur.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect();
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, CLE_RUN, w!("Essaim"), REG_SZ.0, Some(octets.as_ptr() as *const _), octets.len() as u32) }
        .ok()
        .map_err(|e| e.to_string())
}

/// Clé Run d'une ancienne installation : une commande `Essaim` inscrite sans `ARG_DEMARRAGE`
/// (un lancement sans argument peut alors être le démarrage de la session).
pub fn demarrage_sans_argument() -> bool {
    valeur_run().is_some_and(|v| !avec_arg_demarrage(&v))
}

/// Clé Run qui lance l'exécutable en cours sans `ARG_DEMARRAGE` (ancienne installation) :
/// réécrite avec lui. Sa permission dans le Gestionnaire des tâches n'est pas touchée.
pub fn completer_demarrage() {
    let (Some(valeur), Ok(exe)) = (valeur_run(), std::env::current_exe()) else { return };
    if avec_arg_demarrage(&valeur) || !meme_exe(std::path::Path::new(exe_de_la_commande(&valeur)), &exe) {
        return;
    }
    match ecrire_run(&commande_demarrage(&exe)) {
        Ok(()) => log::info!("démarrage : commande complétée de {ARG_DEMARRAGE}"),
        Err(e) => log::warn!("démarrage : commande non complétée ({e})"),
    }
}

/// Essaim démarre-t-il avec la session, et lequel ? Valeur `Essaim` de la clé Run de
/// l'utilisateur, pas coupée dans le Gestionnaire des tâches, comparée à l'exécutable en cours
/// (arguments ignorés).
fn demarrage_actif() -> Demarrage {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY};
    let valeur = valeur_run();
    let mut etat = [0u8; 12];
    let mut n = etat.len() as u32;
    let lu = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, CLE_RUN_APPROUVE, w!("Essaim"), RRF_RT_REG_BINARY, None, Some(etat.as_mut_ptr() as *mut _), Some(&mut n))
    }
    .is_ok();
    // sans valeur : permise ; sinon, premier octet pair (02, 06) = permise, impair (03) = coupée
    let permis = !lu || n == 0 || demarrage_approuve(etat[0]);
    etat_demarrage(valeur.as_deref(), permis, &std::env::current_exe().unwrap_or_default())
}

/// `exe` est-il dans un dossier qu'on vide (Téléchargements, dossier temporaire) ? Comparé sans
/// casse, dossier par dossier.
fn dans_un_dossier_provisoire(exe: &std::path::Path, provisoires: &[std::path::PathBuf]) -> bool {
    let bas = |p: &std::path::Path| std::path::PathBuf::from(p.to_string_lossy().replace('/', "\\").to_lowercase());
    let exe = bas(exe);
    provisoires.iter().filter(|d| !d.as_os_str().is_empty()).any(|d| exe.starts_with(bas(d)))
}

/// Message après l'activation du démarrage : un exécutable laissé dans Téléchargements ou le
/// dossier temporaire risque d'être effacé, et le démarrage ne trouverait plus rien.
fn message_demarrage_active(exe: &std::path::Path, provisoires: &[std::path::PathBuf]) -> String {
    let mut m = String::from("Essaim démarrera avec Windows.");
    if dans_un_dossier_provisoire(exe, provisoires) {
        m.push_str(" Il est lancé depuis un dossier qu'on vide souvent : range d'abord essaim.exe dans un dossier où il restera, puis réactive cette option.");
    }
    m
}

/// Dossiers qu'on vide : Téléchargements et dossier temporaire.
fn dossiers_provisoires() -> Vec<std::path::PathBuf> {
    let mut d: Vec<std::path::PathBuf> = dossier_connu(&windows::Win32::UI::Shell::FOLDERID_Downloads).into_iter().collect();
    d.push(std::env::temp_dir());
    d
}

/// Premier octet de la valeur de `StartupApproved\Run` : l'entrée est-elle permise ?
fn demarrage_approuve(octet: u8) -> bool {
    octet & 1 == 0
}

/// Démarrage avec la session : l'exécutable en cours, ou plus rien.
fn regler_demarrage(actif: bool) -> Result<(), String> {
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};
    unsafe {
        if actif {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            ecrire_run(&commande_demarrage(&exe))?;
            // une coupure faite dans le Gestionnaire des tâches est levée : sans valeur, l'entrée
            // est permise
            let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, CLE_RUN_APPROUVE, w!("Essaim"));
            if r.is_ok() || r == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(r.to_hresult().message()) }
        } else {
            let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, CLE_RUN, w!("Essaim"));
            if r.is_ok() || r == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(r.to_hresult().message()) }
        }
    }
}

/// Type MIME d'une image que la page sait ouvrir, d'après l'extension du fichier.
fn type_image(chemin: &std::path::Path) -> Option<&'static str> {
    let ext = chemin.extension()?.to_string_lossy().to_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

/// Image glissée sur la fenêtre, pour la page : son nom et ses octets en adresse `data:` (20 Mo au
/// plus).
fn image_glissee(chemin: &std::path::Path) -> Result<Value, String> {
    let nom = chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let taille = std::fs::metadata(chemin).map_err(|e| format!("« {nom} » illisible ({e})"))?.len();
    if taille > 20 << 20 {
        return Err(format!("« {nom} » est trop grande (20 Mo au plus)."));
    }
    let octets = std::fs::read(chemin).map_err(|e| format!("« {nom} » illisible ({e})"))?;
    let mime = type_image(chemin).unwrap_or("image/png");
    Ok(json!({ "type": "image", "nom": nom, "donnees": format!("data:{mime};base64,{}", vers_base64(&octets)) }))
}

/// Encode en base64 standard.
fn vers_base64(octets: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(octets.len().div_ceil(3) * 4);
    for c in octets.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

/// Décode du base64 standard (sans espaces).
fn depuis_base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let octets = s.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(octets.len() * 3 / 4);
    for bloc in octets.chunks(4) {
        let v: Vec<u8> = bloc.iter().map(|&c| val(c)).collect::<Option<_>>()?;
        let n = v.iter().enumerate().fold(0u32, |n, (i, &x)| n | (x as u32) << (18 - 6 * i));
        out.extend_from_slice(&n.to_be_bytes()[1..v.len()]);
    }
    Some(out)
}

/// Dossier connu de l'utilisateur (Images, Téléchargements…), là où Windows le place (il peut
/// être déplacé ou redirigé).
fn dossier_connu(id: &windows::core::GUID) -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(std::path::PathBuf::from)
    }
}

/// Enregistre un GIF dans « Images\Essaim » ; renvoie son chemin.
fn enregistrer_gif(nom: &str, donnees: &str) -> Result<std::path::PathBuf, String> {
    let octets = depuis_base64(donnees).ok_or("image illisible")?;
    if !octets.starts_with(b"GIF89a") || octets.len() > 40 << 20 {
        return Err("ce n'est pas un GIF".into());
    }
    let dossier = dossier_connu(&windows::Win32::UI::Shell::FOLDERID_Pictures).ok_or("dossier Images introuvable")?.join("Essaim");
    std::fs::create_dir_all(&dossier).map_err(|e| e.to_string())?;
    ecrire_sans_ecraser(&dossier, &nom_de_gif(nom), "gif", &octets)
}

/// Écrit `octets` dans `dossier\<nom>.<ext>`, ou `<nom> (2).<ext>`, `(3)`… si le nom est pris :
/// un fichier existant n'est jamais écrasé (création exclusive). Renvoie le chemin écrit.
fn ecrire_sans_ecraser(dossier: &std::path::Path, nom: &str, ext: &str, octets: &[u8]) -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    for i in 1..=999 {
        let f = dossier.join(if i == 1 { format!("{nom}.{ext}") } else { format!("{nom} ({i}).{ext}") });
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&f) {
            Ok(mut fichier) => {
                if let Err(e) = fichier.write_all(octets) {
                    drop(fichier);
                    let _ = std::fs::remove_file(&f);
                    return Err(e.to_string());
                }
                return Ok(f);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(format!("trop de fichiers « {nom} » dans {}", dossier.display()))
}

/// Nom de fichier du GIF : sans caractère interdit ni nom réservé par Windows (« CON »…).
fn nom_de_gif(nom: &str) -> String {
    let propre: String = nom.chars().filter(|c| !r#"\/:*?"<>|"#.contains(*c) && !c.is_control()).take(40).collect();
    match propre.trim().trim_end_matches('.') {
        "" => "Essaim".to_owned(),
        p if profils::nom_reserve(p) => format!("Essaim {p}"),
        p => p.to_owned(),
    }
}

/// Adresse la plus longue confiée au navigateur (au-delà, le preset passe par le presse-papiers).
/// GitHub répond « 414 » vers 8 190 caractères, comptés tels que sa page réécrit l'adresse : en
/// encodage de formulaire, celui d'`encoder`, pour que la longueur mesurée ici soit la sienne.
/// Essayé en vrai : 8 000 passe.
const URL_MAX: usize = 8000;

/// Contenu du fichier ouvert sur GitHub quand le preset, trop long pour l'adresse, attend dans le
/// presse-papiers : un commentaire TOML, sans effet s'il reste dans le fichier.
const A_COLLER: &str = "# Preset trop long pour l'adresse : Essaim l'a copié. Colle-le ici à la place de cette ligne (Ctrl+A, puis Ctrl+V).\n";

/// Adresse GitHub qui ouvre la création de `presets/<nom>.toml` dans `depot`, rempli de `texte`.
fn adresse_de_publication(depot: &str, nom: &str, texte: &str) -> String {
    format!("https://github.com/{depot}/new/main?filename={}&value={}", encoder(&format!("presets/{nom}.toml")), encoder(texte))
}

/// Copie `texte` dans le presse-papiers de Windows.
fn copier(texte: &str) -> Result<(), String> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    let utf16: Vec<u16> = texte.encode_utf16().chain(Some(0)).collect();
    unsafe {
        OpenClipboard(None).map_err(|e| e.message())?;
        let r = (|| -> windows::core::Result<()> {
            EmptyClipboard()?;
            let mem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2)?;
            let p = GlobalLock(mem) as *mut u16;
            if p.is_null() {
                let _ = GlobalFree(Some(mem));
                return Err(windows::core::Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), p, utf16.len());
            let _ = GlobalUnlock(mem);
            // le presse-papiers devient propriétaire de la mémoire, sauf en cas d'échec
            if let Err(e) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(mem.0))) {
                let _ = GlobalFree(Some(mem));
                return Err(e);
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        r.map_err(|e| e.message())
    }
}

fn montrer_dans_l_explorateur(f: &std::path::Path) {
    let args: Vec<u16> = format!("/select,\"{}\"", f.display()).encode_utf16().chain(Some(0)).collect();
    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(None, w!("open"), w!("explorer.exe"), PCWSTR(args.as_ptr()), None, SW_SHOWNORMAL);
    }
}

/// Boîte « Ouvrir » de Windows pour un fichier de preset.
fn choisir_fichier(parent: HWND) -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Controls::Dialogs::{GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW};
    let filtre: Vec<u16> = "Presets Essaim (*.toml)\0*.toml\0\0".encode_utf16().collect();
    let mut buf = vec![0u16; 1024];
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: parent,
        lpstrFilter: PCWSTR(filtre.as_ptr()),
        lpstrFile: windows::core::PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrTitle: w!("Importer un preset"),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    unsafe { GetOpenFileNameW(&mut ofn) }.as_bool().then(|| {
        std::path::PathBuf::from(String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(0)]))
    })
}

fn majuscule(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Encodage de formulaire (`application/x-www-form-urlencoded`, celui de `URLSearchParams`) :
/// l'espace devient « + », un caractère au lieu de trois, ce qui compte pour un preset.
fn encoder(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => (b as char).to_string(),
            b' ' => "+".to_owned(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Thème : celui des applications de Windows (clair ou sombre)

fn theme_sombre() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut v = 1u32;
    let mut n = std::mem::size_of::<u32>() as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut n),
        )
    }
    .is_ok();
    ok && v == 0
}

/// Fond de la page (jeton `fond` de la charte), pour la barre de titre et avant le chargement.
fn fond(sombre: bool) -> (u8, u8, u8) {
    if sombre { (0x12, 0x12, 0x11) } else { (0xe8, 0xe7, 0xe1) }
}

/// Barre de titre aux couleurs de la page (Windows 11 ; ignoré ailleurs).
fn barre_de_titre(hwnd: HWND) {
    let sombre = theme_sombre();
    let ((r, g, b), texte) = (fond(sombre), if sombre { 0x00e4ebec } else { 0x00111111 });
    let fond = r as u32 | (g as u32) << 8 | (b as u32) << 16;
    unsafe {
        let s = BOOL::from(sombre);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &s as *const _ as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &COLORREF(fond) as *const _ as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &COLORREF(texte) as *const _ as *const _, 4);
    }
}

// ---------------------------------------------------------------------------------------------
// Fenêtre

struct Parent(HWND);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let h = NonZeroIsize::new(self.0 .0 as isize).ok_or(HandleError::Unavailable)?;
        // SAFETY : la fenêtre vit plus longtemps que la vue, détruite avant elle (`WM_CLOSE`,
        // ou à la fin de `run`).
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(h))) })
    }
}

/// Page de téléchargement officielle du runtime WebView2 de Microsoft.
const ADRESSE_WEBVIEW2: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

/// Le runtime WebView2 est-il installé ? Méthode donnée par Microsoft : la valeur `pv` (version)
/// de sa clé EdgeUpdate, pour la machine (64 ou 32 bits) ou pour l'utilisateur, existe et n'est
/// ni vide ni « 0.0.0.0 ».
fn webview2_installe() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    const CLIENT: &str = "Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let cles = [
        (HKEY_LOCAL_MACHINE, format!("SOFTWARE\\WOW6432Node\\{CLIENT}")),
        (HKEY_LOCAL_MACHINE, format!("SOFTWARE\\{CLIENT}")),
        (HKEY_CURRENT_USER, format!("Software\\{CLIENT}")),
    ];
    cles.iter().any(|(racine, chemin)| {
        let mut buf = [0u16; 64];
        let mut n = std::mem::size_of_val(&buf) as u32;
        let lu = unsafe {
            RegGetValueW(*racine, &windows::core::HSTRING::from(chemin), w!("pv"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut n))
        }
        .is_ok();
        let version = String::from_utf16_lossy(&buf[..(n as usize / 2).min(buf.len())]);
        lu && version_webview2_valide(&version)
    })
}

/// Version lue dans `pv` : vide ou « 0.0.0.0 », le runtime n'est pas (ou plus) installé.
fn version_webview2_valide(version: &str) -> bool {
    let v = version.trim_end_matches('\0').trim();
    !v.is_empty() && v != "0.0.0.0"
}

/// La fenêtre ne peut pas s'ouvrir faute de WebView2 : le dire par une bulle de l'icône, une fois
/// par lancement (sinon rien ne se passerait au clic), avec l'adresse où le télécharger.
fn signaler_webview2_absent() {
    static DIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !DIT.swap(true, Ordering::Relaxed) {
        crate::win::bulle(&texte_webview2_absent());
    }
}

/// Texte de la bulle : il tient dans une bulle (250 caractères au plus, `win::bulle`).
fn texte_webview2_absent() -> String {
    format!("Fenêtre des réglages impossible à ouvrir : il manque le runtime WebView2 de Microsoft. Il se télécharge sur {ADRESSE_WEBVIEW2}")
}

fn run(son: Arc<AudioShared>, place: &Place) -> anyhow::Result<()> {
    let hwnd = unsafe { creer_fenetre()? };
    place.publier(hwnd);
    barre_de_titre(hwnd);

    let recus = RECUS.with(|r| r.clone());
    let (r, g, b) = fond(theme_sombre());
    let mut contexte = wry::WebContext::new(Some(config::dir().join("webview")));
    let vue = WebViewBuilder::new_with_web_context(&mut contexte)
        .with_custom_protocol("essaim".into(), |_, req| servir(req))
        .with_url(format!("{ORIGINE}index.html"))
        .with_background_color((r, g, b, 255))
        // seule la page d'Essaim parle à l'appli (pas une page où une navigation aurait mené)
        .with_ipc_handler(move |req| {
            let origine = req.uri().to_string();
            if origine_permise(&origine) {
                recus.borrow_mut().push(req.into_body());
            } else {
                log::warn!("réglages : message ignoré, venu de « {origine} »");
            }
        })
        .with_drag_drop_handler(|ev| {
            // fichiers de preset glissés sur la fenêtre : importés (et rien d'autre ne s'ouvre)
            if let wry::DragDropEvent::Drop { paths, .. } = ev {
                let chemins: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
                let m = json!({ "cmd": "importer_fichiers", "chemins": chemins }).to_string();
                RECUS.with(|r| r.borrow_mut().push(m));
            }
            true
        })
        .with_navigation_handler(|url| url.starts_with(ORIGINE))
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        // Ctrl+molette ; Ctrl+plus, Ctrl+moins et Ctrl+0 passent par la page (demande `zoom`),
        // les raccourcis de navigateur étant coupés
        .with_hotkeys_zoom(true)
        .with_default_context_menus(cfg!(debug_assertions))
        .with_browser_accelerator_keys(false)
        .with_devtools(cfg!(debug_assertions))
        .build(&Parent(hwnd));
    let vue = match vue {
        Ok(v) => v,
        Err(e) => {
            // fenêtre encore cachée : la détruire ne montre rien
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            drop(contexte);
            if !webview2_installe() {
                signaler_webview2_absent();
                anyhow::bail!("runtime WebView2 absent ({e})");
            }
            return Err(e.into());
        }
    };
    let _ = vue.focus();
    let fenetre = Fenetre {
        hwnd,
        vue,
        son,
        son_envoye: (u32::MAX, 0),
        dates: (None, None),
        presets_envoyes: None,
        apercus: Default::default(),
        avant: None,
        ecoute: false,
        jeux: None,
        reduite: false,
        montree: false,
        maj: None,
    };
    FENETRE.with(|f| *f.borrow_mut() = Some(fenetre));
    unsafe {
        // Le son et la charge attendent la première demande `etat` (`minuteries`) : avant, la
        // page n'est pas chargée et les perdrait.
        // La fenêtre reste cachée tant que la page n'est pas dessinée (message `prete`) : montrée
        // tout de suite, elle serait blanche le temps que WebView2 rende sa première image.
        SetTimer(Some(hwnd), TIMER_MONTRER, ATTENTE_PAGE_MS, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            // une panique dans le traitement d'une demande laisse un état douteux : la fenêtre
            // se ferme proprement
            if crate::win::sans_panique("réglages", traiter_recus).is_none() || FERMER.with(|f| f.replace(false)) {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
    // la vue est détruite à `WM_CLOSE`, avant sa fenêtre ; ici seulement si la fenêtre a été
    // détruite autrement, et toujours avant le contexte
    FENETRE.with(|f| f.borrow_mut().take());
    drop(contexte);
    Ok(())
}

/// Adresse de la page d'où vient un message : seule celle d'Essaim est écoutée.
fn origine_permise(adresse: &str) -> bool {
    adresse.starts_with(ORIGINE)
}

fn traiter_recus() {
    let lot: Vec<String> = RECUS.with(|r| std::mem::take(&mut *r.borrow_mut()));
    if lot.is_empty() {
        return;
    }
    FENETRE.with(|f| {
        if let Some(f) = f.borrow_mut().as_mut() {
            for m in &lot {
                f.traiter(m);
            }
        }
    });
}

unsafe fn creer_fenetre() -> windows::core::Result<HWND> {
    let hinst = GetModuleHandleW(None)?;
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: hinst.into(),
        lpszClassName: CLASS,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hIcon: crate::win::app_icon(false).unwrap_or_default(),
        hIconSm: crate::win::app_icon(true).unwrap_or_default(),
        ..Default::default()
    };
    RegisterClassExW(&wc);
    // au centre de l'écran de la souris, à son échelle
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
    let (mut dpi, mut dy) = (96u32, 96u32);
    let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dpi, &mut dy);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(mon, &mut mi);
    let wa = mi.rcWork;
    let style = WS_OVERLAPPEDWINDOW;
    let mut r = RECT { left: 0, top: 0, right: TAILLE.0 * dpi as i32 / 96, bottom: TAILLE.1 * dpi as i32 / 96 };
    let _ = AdjustWindowRectExForDpi(&mut r, style, false, WINDOW_EX_STYLE(0), dpi);
    let (w, h) = ((r.right - r.left).min(wa.right - wa.left), (r.bottom - r.top).min(wa.bottom - wa.top));
    CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        w!("Essaim"),
        style,
        wa.left + (wa.right - wa.left - w) / 2,
        wa.top + (wa.bottom - wa.top - h) / 2,
        w,
        h,
        None,
        None,
        Some(hinst.into()),
        None,
    )
}

/// Détruit la vue (WebView2) tant que sa fenêtre existe ; faux si elle est occupée (une demande
/// de la page en cours, plus haut sur la pile).
fn detruire_vue() -> bool {
    match FENETRE.with(|f| f.try_borrow_mut().ok().map(|mut f| f.take())) {
        Some(vue) => {
            drop(vue);
            true
        }
        None => false,
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match crate::win::sans_panique("fenêtre des réglages", || unsafe { message(hwnd, msg, wp, lp) }) {
        Some(r) => r,
        // état douteux après une panique : la fenêtre se ferme proprement plutôt que de continuer
        None => unsafe {
            match msg {
                WM_DESTROY => PostQuitMessage(0),
                WM_CLOSE => {
                    detruire_vue();
                    let _ = DestroyWindow(hwnd);
                }
                _ => {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        },
    }
}

unsafe fn message(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_DEVANT => {
                // demandé par un autre fil (`open`) : seulement une fois la fenêtre montrée (elle
                // se montre d'elle-même quand la page est prête)
                FENETRE.with(|f| {
                    if let Ok(f) = f.try_borrow() {
                        if let Some(f) = f.as_ref().filter(|f| f.montree) {
                            let _ = SetForegroundWindow(hwnd);
                            let _ = f.vue.focus();
                        }
                    }
                });
                return LRESULT(0);
            }
            WM_TIMER => {
                FENETRE.with(|f| {
                    if let Ok(mut f) = f.try_borrow_mut() {
                        if let Some(f) = f.as_mut() {
                            f.tick(wp.0);
                        }
                    }
                });
                return LRESULT(0);
            }
            WM_SIZE => {
                // réduite ou restaurée ; le message continue vers wry (taille de la vue)
                let reduite = wp.0 == SIZE_MINIMIZED as usize;
                if reduite || wp.0 == SIZE_RESTORED as usize || wp.0 == SIZE_MAXIMIZED as usize {
                    FENETRE.with(|f| {
                        if let Ok(mut f) = f.try_borrow_mut() {
                            if let Some(f) = f.as_mut() {
                                f.reduire(reduite);
                            }
                        }
                    });
                }
            }
            WM_GETMINMAXINFO => {
                let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                let mm = &mut *(lp.0 as *mut MINMAXINFO);
                mm.ptMinTrackSize = POINT { x: TAILLE_MIN.0 * dpi / 96, y: TAILLE_MIN.1 * dpi / 96 };
                return LRESULT(0);
            }
            WM_ERASEBKGND => {
                // Le fond de la fenêtre, à la couleur de la page : c'est lui qu'on voit tant que
                // WebView2 n'a pas présenté sa première image (sur un grand écran, quelques
                // images après l'apparition). Sans lui, la fenêtre s'ouvre blanche.
                let (r, g, b) = fond(theme_sombre());
                let mut zone = RECT::default();
                let _ = GetClientRect(hwnd, &mut zone);
                let pinceau = CreateSolidBrush(COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16));
                FillRect(HDC(wp.0 as *mut _), &zone, pinceau);
                let _ = DeleteObject(pinceau.into());
                return LRESULT(1);
            }
            WM_DPICHANGED => {
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                return LRESULT(0);
            }
            WM_SETTINGCHANGE => {
                // « ImmersiveColorSet » : thème clair/sombre changé (la page suit d'elle-même)
                barre_de_titre(hwnd);
            }
            WM_SETFOCUS => {
                FENETRE.with(|f| {
                    if let Ok(f) = f.try_borrow() {
                        if let Some(f) = f.as_ref() {
                            let _ = f.vue.focus();
                        }
                    }
                });
            }
            WM_CLOSE => {
                // la vue d'abord, tant que sa fenêtre existe ; occupée, la fermeture est refaite
                // une fois la demande en cours finie (`FERMER`)
                if detruire_vue() {
                    let _ = DestroyWindow(hwnd);
                } else {
                    FERMER.with(|f| f.set(true));
                }
                return LRESULT(0);
            }
            WM_DESTROY => {
                for t in [TIMER_SON, TIMER_FICHIERS, TIMER_MONTRER] {
                    let _ = KillTimer(Some(hwnd), t);
                }
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

/// Réglages d'un preset reçus de la page (bibliothèque) : convertis comme un fichier importé
/// (`migration::migrer` : réactions d'avant les modulations), puis bornés (`clamped`).
fn config_distante(reglages: Value) -> Result<Config, String> {
    let mut t: toml::Table = toml::Table::try_from(reglages).map_err(|e| format!("preset illisible ({e})"))?;
    crate::migration::migrer(&mut t);
    use serde::Deserialize;
    Config::deserialize(t).map(Config::clamped).map_err(|e| format!("preset illisible ({e})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webview2_absent_dit_ou_le_trouver() {
        let t = texte_webview2_absent();
        assert!(t.encode_utf16().count() <= 250, "bulle tronquée : {} caractères", t.encode_utf16().count());
        assert!(t.ends_with(ADRESSE_WEBVIEW2));
        assert!(version_webview2_valide("131.0.2903.70\0"));
        for v in ["", "0.0.0.0", "\0", " "] {
            assert!(!version_webview2_valide(v), "{v:?}");
        }
    }

    #[test]
    fn base64_decode() {
        for texte in [&b"Man"[..], b"Ma", b"M", b"", b"\x00\xff\x10essaim"] {
            assert_eq!(depuis_base64(&vers_base64(texte)).unwrap(), texte);
        }
        assert_eq!(vers_base64(b"Ma"), "TWE=");
        assert_eq!(depuis_base64("TWFu").unwrap(), b"Man");
        assert_eq!(depuis_base64("TWE=").unwrap(), b"Ma");
        assert_eq!(depuis_base64("TQ==").unwrap(), b"M");
        assert!(depuis_base64("T@==").is_none());
    }

    #[test]
    fn noms_de_gif() {
        assert_eq!(nom_de_gif("Nuit / blanche"), "Nuit  blanche");
        assert_eq!(nom_de_gif("con"), "Essaim con");
        assert_eq!(nom_de_gif("LPT1.x"), "Essaim LPT1.x");
        assert_eq!(nom_de_gif(" ?? "), "Essaim");
        assert_eq!(nom_de_gif("fin..."), "fin");
    }

    #[test]
    fn encodage_d_url() {
        assert_eq!(encoder("presets/Nuit blanche.toml"), "presets%2FNuit+blanche.toml");
        assert_eq!(encoder("é=1\n"), "%C3%A9%3D1%0A");
        assert_eq!(encoder("1+1 ~*-._"), "1%2B1+%7E*-._");
    }

    #[test]
    fn adresse_de_publication_d_un_preset() {
        assert_eq!(
            adresse_de_publication("a/b", "Nuit blanche", "[essaim]\nforme = \"cercle\"\n"),
            "https://github.com/a/b/new/main?filename=presets%2FNuit+blanche.toml&value=%5Bessaim%5D%0Aforme+%3D+%22cercle%22%0A"
        );
        // le mot laissé dans le fichier quand le preset passe par le presse-papiers tient dans
        // l'adresse et reste une seule ligne de commentaire TOML
        assert!(adresse_de_publication("a/b", &"n".repeat(40), A_COLLER).len() <= URL_MAX);
        assert!(A_COLLER.starts_with("# ") && A_COLLER.ends_with('\n') && A_COLLER.lines().count() == 1);
    }

    #[test]
    fn la_page_et_ses_fichiers_sont_servis() {
        let req = |p: &str| wry::http::Request::builder().uri(format!("{ORIGINE}{p}")).body(Vec::new()).unwrap();
        assert_eq!(servir(req("")).status(), 200);
        assert_eq!(servir(req("app.js")).headers()["Content-Type"], "text/javascript; charset=utf-8");
        assert_eq!(servir(req("../src/main.rs")).status(), 404);
        // chaque fichier nommé par la page existe
        let page = std::str::from_utf8(FICHIERS[0].2).unwrap();
        for (chemin, ..) in &FICHIERS[1..5] {
            assert!(page.contains(chemin), "{chemin} non chargé par index.html");
        }
        // l'ouverture animée : la page a le voile que `lancement.js` anime
        assert!(page.contains(r#"id="ouverture""#));
    }

    #[test]
    fn messages_de_la_page() {
        let d: Demande = serde_json::from_str(r#"{"cmd":"regler","section":"orbite","cle":"rayon","valeur":30}"#).unwrap();
        assert!(matches!(d, Demande::Regler { .. }));
        let d: Demande = serde_json::from_str(r#"{"cmd":"enregistrer","nom":"Nuit","publier":true}"#).unwrap();
        assert!(matches!(d, Demande::Enregistrer { publier: true, .. }));
        assert!(matches!(serde_json::from_str::<Demande>(r#"{"cmd":"chercher_maj"}"#).unwrap(), Demande::ChercherMaj));
        assert!(matches!(serde_json::from_str::<Demande>(r#"{"cmd":"redemarrer"}"#).unwrap(), Demande::Redemarrer));
        assert!(matches!(serde_json::from_str::<Demande>(r#"{"cmd":"prete"}"#).unwrap(), Demande::Prete));
        assert!(serde_json::from_str::<Demande>(r#"{"cmd":"formater_disque"}"#).is_err());
        // `remplacer` est facultatif (faux), et lu quand il est là
        let d: Demande = serde_json::from_str(r#"{"cmd":"enregistrer","nom":"Nuit","publier":false}"#).unwrap();
        assert!(matches!(d, Demande::Enregistrer { remplacer: false, .. }));
        let d: Demande = serde_json::from_str(r#"{"cmd":"enregistrer","nom":"Nuit","publier":false,"remplacer":true}"#).unwrap();
        assert!(matches!(d, Demande::Enregistrer { remplacer: true, .. }));
        assert!(matches!(serde_json::from_str::<Demande>(r#"{"cmd":"zoom","pas":-1}"#).unwrap(), Demande::Zoom { pas: -1 }));
    }

    #[test]
    fn rien_n_est_envoye_avant_la_demande_d_etat() {
        // page pas encore chargée : aucune minuterie, donc ni son, ni charge, ni mise à jour
        assert!(minuteries(false, false).is_empty());
        assert!(minuteries(false, true).is_empty());
        let ids = |v: Vec<(usize, u32)>| v.into_iter().map(|(t, _)| t).collect::<Vec<_>>();
        assert_eq!(ids(minuteries(true, false)), [TIMER_SON, TIMER_FICHIERS]);
        // réduite : la charge et la mise à jour continuent, pas le son
        assert_eq!(ids(minuteries(true, true)), [TIMER_FICHIERS]);
        // l'attente de la page ne dépend pas d'elle
        assert!(!minuteries(true, false).iter().any(|&(t, _)| t == TIMER_MONTRER));
    }

    #[test]
    fn preset_installe_et_applique_annonce() {
        let r = reponses_installation("Nuit", Some(Ok(())), Ok(()));
        assert_eq!(r.last().unwrap(), &json!({ "type": "applique", "nom": "Nuit" }), "le bandeau « Annuler » reste affiché");
        assert!(r.iter().any(|m| m["type"] == "message"));
        // curseurs refusés : pas d'erreur à part, l'avertissement va dans `applique`, qui garde
        // son bandeau « Annuler »
        let r = reponses_installation("Nuit", Some(Ok(())), Err("registre verrouillé".into()));
        let dernier = r.last().unwrap();
        assert_eq!((&dernier["type"], &dernier["nom"]), (&json!("applique"), &json!("Nuit")));
        assert!(dernier["avertissement"].as_str().unwrap().contains("curseurs non installés : registre verrouillé"), "{r:?}");
        assert!(r.iter().all(|m| m["type"] != "erreur"), "{r:?}");
        let r = reponses_installation("Nuit", Some(Err("verrouillé".into())), Ok(()));
        assert!(r.iter().all(|m| m["type"] != "applique"));
        assert_eq!(r.last().unwrap()["type"], "erreur");
        assert_eq!(reponses_installation("Nuit", None, Ok(())).len(), 1, "installé seulement");
    }

    #[test]
    fn preset_applique_avec_avertissement_des_curseurs() {
        // `Appliquer` : la même réponse que l'installation
        assert_eq!(applique("Nuit", Ok(())), json!({ "type": "applique", "nom": "Nuit" }));
        let r = applique("Nuit", Err("jeu introuvable".into()));
        assert_eq!(r["avertissement"], "Preset appliqué, mais curseurs non installés : jeu introuvable.");
    }

    #[test]
    fn reglage_refuse_nomme_sa_cle_et_sa_valeur() {
        assert_eq!(
            reglage_refuse("orbite", "rayon", &json!(999)),
            json!({ "type": "reglage_refuse", "section": "orbite", "cle": "rayon", "valeur": 999 })
        );
    }

    #[test]
    fn demande_incomprise_repond_une_erreur() {
        for texte in [r#"{"cmd":"formater_disque"}"#, r#"{"cmd":"regler","section":"orbite"}"#, "pas du JSON", ""] {
            let r = lire_demande(texte).err().unwrap_or_else(|| panic!("{texte} accepté"));
            assert_eq!(r, json!({ "type": "erreur", "texte": "Demande non comprise." }));
        }
        assert!(matches!(lire_demande(r#"{"cmd":"etat"}"#), Ok(Demande::Etat)));
    }

    #[test]
    fn image_illisible_repond_des_contours_vides() {
        // même forme que la réponse normale : la page cesse d'attendre l'analyse
        assert_eq!(contours_vides(Some(40)), json!({ "type": "contours", "liste": [], "seuil": 40, "transparence": false }));
        assert_eq!(contours_vides(None)["seuil"], 128);
    }

    #[test]
    fn messages_d_une_autre_page_ignores() {
        assert!(origine_permise("http://essaim.localhost/index.html"));
        for autre in ["https://exemple.org/", "http://essaim.localhost.exemple.org/", "http://essaim.localhostx/", "about:blank", ""] {
            assert!(!origine_permise(autre), "{autre}");
        }
    }

    #[test]
    fn zoom_par_paliers() {
        assert_eq!(ZOOMS[ZOOM_NORMAL], 1.0);
        assert_eq!(zoom_suivant(1.0, 1), 1.1);
        assert_eq!(zoom_suivant(1.75, 1), 1.75);
        assert_eq!(zoom_suivant(0.67, -1), 0.67);
        assert_eq!(zoom_suivant(1.5, 0), 1.0);
        // entre deux paliers (Ctrl+molette) : le premier au-delà, dans le sens voulu
        assert_eq!(zoom_suivant(1.3, 1), 1.5);
        assert_eq!(zoom_suivant(1.3, -1), 1.25);
        assert_eq!(zoom_suivant(1.249_99, 1), 1.5);
        assert_eq!(zoom_suivant(3.0, -1), 1.75);
        assert_eq!(zoom_suivant(0.25, 1), 0.67);
        assert!(ZOOMS.windows(2).all(|z| z[0] < z[1]));
    }

    #[test]
    fn demarrage_coupe_dans_le_gestionnaire_des_taches() {
        assert!(demarrage_approuve(0x02) && demarrage_approuve(0x06));
        assert!(!demarrage_approuve(0x03) && !demarrage_approuve(0x07));
    }

    #[test]
    fn demarrage_compare_au_chemin_de_l_exe() {
        use std::path::{Path, PathBuf};
        let exe = Path::new(r"C:\Outils\Essaim\essaim.exe");
        // inscrit pour lui : guillemets, casse et barres obliques ne comptent pas
        assert_eq!(etat_demarrage(Some(r#""C:\Outils\Essaim\essaim.exe""#), true, exe), Demarrage::Actif);
        assert_eq!(etat_demarrage(Some(r#""c:/outils/essaim/ESSAIM.EXE" --reglages"#), true, exe), Demarrage::Actif);
        assert_eq!(etat_demarrage(Some(r"C:\Outils\Essaim\essaim.exe"), true, exe), Demarrage::Actif);
        // pour un autre exécutable (déplacé depuis Téléchargements) : son chemin
        let ailleurs = etat_demarrage(Some(r#""C:\Users\a\Downloads\essaim.exe""#), true, exe);
        assert_eq!(ailleurs, Demarrage::Ailleurs(r"C:\Users\a\Downloads\essaim.exe".into()));
        assert_eq!(ailleurs.json(), (false, Some(r"C:\Users\a\Downloads\essaim.exe")));
        // absent, ou coupé dans le Gestionnaire des tâches
        assert_eq!(etat_demarrage(None, true, exe), Demarrage::Inactif);
        assert_eq!(etat_demarrage(Some(r#""C:\Outils\Essaim\essaim.exe""#), false, exe), Demarrage::Inactif);
        assert_eq!(Demarrage::Actif.json(), (true, None));
        // sans guillemets, avec un argument : coupé après `.exe`
        assert_eq!(exe_de_la_commande(r"C:\Outils\Essaim\essaim.exe --reglages"), r"C:\Outils\Essaim\essaim.exe");
        assert_eq!(etat_demarrage(Some(r"C:\Outils\Essaim\ESSAIM.exe --reglages"), true, exe), Demarrage::Actif);
        assert_eq!(exe_de_la_commande(r"C:\x.exes\essaim.exe /b"), r"C:\x.exes\essaim.exe");
        // commande écrite par Essaim : l'exécutable entre guillemets, puis `--demarrage`, ignoré
        // pour la comparaison
        let commande = commande_demarrage(exe);
        assert_eq!(commande, r#""C:\Outils\Essaim\essaim.exe" --demarrage"#);
        assert_eq!(etat_demarrage(Some(&commande), true, exe), Demarrage::Actif);
        assert!(avec_arg_demarrage(&commande));
        assert!(avec_arg_demarrage(r"C:\Outils\Essaim\essaim.exe --DEMARRAGE"));
        // ancienne installation : sans l'argument (ou avec un autre, ou dans le chemin)
        assert!(!avec_arg_demarrage(r#""C:\Outils\Essaim\essaim.exe""#));
        assert!(!avec_arg_demarrage(r"C:\Outils\Essaim\essaim.exe --reglages"));
        assert!(!avec_arg_demarrage(r#""C:\--demarrage\essaim.exe""#));
        // même fichier par un autre chemin (`..`) : chemins canoniques égaux
        let d = std::env::temp_dir().join(format!("essaim-demarrage-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sous")).unwrap();
        let vrai = d.join("essaim.exe");
        std::fs::write(&vrai, b"").unwrap();
        let detour = d.join("sous").join("..").join("essaim.exe");
        assert_eq!(etat_demarrage(Some(&format!("\"{}\"", detour.display())), true, &vrai), Demarrage::Actif);
        let autre = d.join("sous").join("essaim.exe");
        std::fs::write(&autre, b"").unwrap();
        assert!(matches!(etat_demarrage(Some(&autre.to_string_lossy()), true, &vrai), Demarrage::Ailleurs(_)));
        let _ = std::fs::remove_dir_all(&d);
        // dossier provisoire : le message prévient
        let provisoires = [PathBuf::from(r"C:\Users\a\Downloads"), PathBuf::from(r"C:\Users\a\AppData\Local\Temp\")];
        assert!(dans_un_dossier_provisoire(Path::new(r"c:\users\A\downloads\essaim.exe"), &provisoires));
        assert!(dans_un_dossier_provisoire(Path::new(r"C:\Users\a\AppData\Local\Temp\x\essaim.exe"), &provisoires));
        assert!(!dans_un_dossier_provisoire(Path::new(r"C:\Users\a\Downloads2\essaim.exe"), &provisoires));
        assert!(!dans_un_dossier_provisoire(exe, &provisoires));
        assert!(message_demarrage_active(Path::new(r"C:\Users\a\Downloads\essaim.exe"), &provisoires).contains("range d'abord essaim.exe dans un dossier où il restera"));
        assert_eq!(message_demarrage_active(exe, &provisoires), "Essaim démarrera avec Windows.");
    }

    #[test]
    fn gif_jamais_ecrase() {
        let d = std::env::temp_dir().join(format!("essaim-test-gif-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let a = ecrire_sans_ecraser(&d, "Nuit", "gif", b"GIF89a 1").unwrap();
        let b = ecrire_sans_ecraser(&d, "Nuit", "gif", b"GIF89a 2").unwrap();
        let c = ecrire_sans_ecraser(&d, "Nuit", "gif", b"GIF89a 3").unwrap();
        assert_eq!((a.file_name().unwrap(), b.file_name().unwrap(), c.file_name().unwrap()), ("Nuit.gif".as_ref(), "Nuit (2).gif".as_ref(), "Nuit (3).gif".as_ref()));
        assert_eq!(std::fs::read(&a).unwrap(), b"GIF89a 1");
        assert_eq!(std::fs::read(&c).unwrap(), b"GIF89a 3");
        let _ = std::fs::remove_dir_all(&d);
    }
}

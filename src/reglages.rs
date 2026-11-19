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
use std::cell::RefCell;
use std::num::NonZeroIsize;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
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
/// Adresse de la page : wry sert le protocole `essaim` sous `http://essaim.localhost/`.
const ORIGINE: &str = "http://essaim.localhost/";

/// Fenêtre ouverte (0 = aucune).
static WINDOW: AtomicIsize = AtomicIsize::new(0);

/// Ouvre la fenêtre, ou la ramène devant si elle l'est déjà.
pub fn open(son: Arc<AudioShared>) {
    let h = WINDOW.load(Ordering::Relaxed);
    if h != 0 {
        let hwnd = HWND(h as *mut _);
        unsafe {
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
                return;
            }
        }
    }
    let _ = std::thread::Builder::new().name("essaim-reglages".into()).spawn(move || {
        if let Err(e) = run(son) {
            log::warn!("réglages : fenêtre impossible à ouvrir ({e:#})");
        }
        WINDOW.store(0, Ordering::Relaxed);
    });
}

// ---------------------------------------------------------------------------------------------
// Fichiers de la page

/// Fichiers de `ui/` servis à la page : (chemin, type, contenu).
const FICHIERS: [(&str, &str, &[u8]); 5] = [
    ("index.html", "text/html; charset=utf-8", include_bytes!("../ui/index.html")),
    ("style.css", "text/css; charset=utf-8", include_bytes!("../ui/style.css")),
    ("app.js", "text/javascript; charset=utf-8", include_bytes!("../ui/app.js")),
    ("apercu.js", "text/javascript; charset=utf-8", include_bytes!("../ui/apercu.js")),
    ("polices/Archivo.ttf", "font/ttf", include_bytes!("../ui/polices/Archivo.ttf")),
];

/// La page ne charge que ses propres fichiers ; seul l'index de la bibliothèque vient d'ailleurs.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; \
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
    Regler { section: String, cle: String, valeur: Value },
    Appliquer { nom: String },
    /// Enregistre les réglages actuels sous `nom` ; `publier` ouvre la proposition sur GitHub.
    Enregistrer { nom: String, publier: bool },
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
}

struct Fenetre {
    hwnd: HWND,
    vue: WebView,
    son: Arc<AudioShared>,
    /// Dernier son envoyé (kicks, basse) : rien n'est renvoyé s'il n'a pas changé.
    son_envoye: (u32, u32),
    /// Dates de `config.toml` et du dossier des presets au dernier état envoyé.
    dates: (Option<SystemTime>, Option<SystemTime>),
    /// Aperçus des jeux de curseurs déjà dessinés (clé → images).
    apercus: std::collections::HashMap<String, Value>,
    /// Réglages et curseurs d'avant le dernier preset appliqué (pour « Annuler »).
    avant: Option<profils::Avant>,
    /// Jeux de curseurs de Windows, lus une fois par ouverture (relus à chaque liste demandée).
    jeux: Option<Vec<crate::souris::Jeu>>,
    /// Fenêtre réduite : la vue est cachée et le son n'est plus envoyé.
    reduite: bool,
}

thread_local! {
    static FENETRE: RefCell<Option<Fenetre>> = const { RefCell::new(None) };
    /// Messages reçus de la page, traités après chaque message Windows (le gestionnaire de
    /// wry n'a pas accès à la vue).
    static RECUS: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
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
        let cfg = Config::read(&config::path()).unwrap_or_default();
        let liste = profils::list();
        let actif = liste.iter().find(|p| profils::same(&p.cfg, &cfg)).map(|p| p.name.clone());
        // sans preset identique : le plus proche, s'il ne diffère que de quelques réglages
        let proche = if actif.is_some() {
            None
        } else {
            liste.iter().map(|p| (profils::ecarts(&p.cfg, &cfg), p)).filter(|(n, _)| *n <= 6).min_by_key(|(n, _)| *n)
        };
        let date = |p: &profils::Profile| {
            p.date.and_then(|d| d.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs())
        };
        let presets: Vec<Value> =
            liste.iter().map(|p| json!({ "nom": p.name, "fourni": p.builtin, "reglages": p.cfg, "date": date(p) })).collect();
        self.envoyer(&json!({
            "type": "etat",
            "reglages": cfg,
            "presets": presets,
            "actif": actif,
            "depot": cfg.bibliotheque.depot,
            "curseurs": crate::souris::actuel(),
            "proche": proche.map(|(n, p)| json!({ "nom": p.name, "ecarts": n })),
            "defauts": Config::default(),
            "demarrage": demarrage_actif(),
        }));
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
        unsafe {
            if reduite {
                let _ = KillTimer(Some(self.hwnd), TIMER_SON);
            } else {
                SetTimer(Some(self.hwnd), TIMER_SON, 33, None);
                self.son_envoye = (u32::MAX, 0);
            }
        }
    }

    fn message(&self, texte: &str) {
        self.envoyer(&json!({ "type": "message", "texte": texte }));
    }

    fn erreur(&self, texte: &str) {
        self.envoyer(&json!({ "type": "erreur", "texte": texte }));
    }

    fn traiter(&mut self, texte: &str) {
        let demande = match serde_json::from_str::<Demande>(texte) {
            Ok(d) => d,
            Err(e) => return log::warn!("réglages : message de la page illisible ({e})"),
        };
        match demande {
            Demande::Etat => self.etat(),
            Demande::Regler { section, cle, valeur } => match config::set_value(&section, &cle, &valeur) {
                Ok(()) => config::request_reload(),
                Err(e) => self.erreur(&format!("Réglage refusé : {e}.")),
            },
            Demande::Appliquer { nom } => {
                let liste = profils::list();
                let avant = profils::capturer();
                match liste.iter().find(|p| p.name == nom).map(|p| (p, profils::apply(p))) {
                    Some((p, Ok(()))) => {
                        config::request_reload();
                        self.curseurs_du_preset(&p.cfg);
                        self.avant = avant;
                        self.envoyer(&json!({ "type": "applique", "nom": nom }));
                    }
                    Some((_, Err(e))) => self.erreur(&format!("Preset « {nom} » non appliqué : {e}.")),
                    None => self.erreur(&format!("Preset « {nom} » introuvable.")),
                }
                self.etat();
            }
            Demande::Enregistrer { nom, publier } => {
                let mut cfg = Config::read(&config::path()).unwrap_or_default();
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
                let cfg = serde_json::from_value::<Config>(reglages).map_err(|e| format!("preset illisible ({e})"));
                // un preset de l'utilisateur du même nom (à la casse près) n'est pas écrasé
                match cfg.and_then(|cfg| profils::ajouter(&nom, &cfg).map(|n| (n, cfg))) {
                    Ok((nom, cfg)) => {
                        if appliquer {
                            self.avant = profils::capturer();
                            let p = profils::list().into_iter().find(|p| p.name == nom);
                            match p.as_ref().map(profils::apply) {
                                Some(Ok(())) => self.curseurs_du_preset(&cfg),
                                Some(Err(e)) => self.erreur(&format!("Preset « {nom} » installé mais non appliqué : {e}.")),
                                None => {}
                            }
                            config::request_reload();
                        }
                        self.etat();
                        self.message(&format!("Preset « {nom} » installé dans Mes presets."));
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
                        self.message(&format!("Curseurs « {jeu} » installés."));
                    }
                    Err(e) => self.erreur(&format!("Curseurs non installés : {e}.")),
                }
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
                let f = profils::fichier(&nom);
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
                let chemins: Vec<std::path::PathBuf> = chemins.into_iter().map(Into::into).collect();
                self.importer(&chemins);
            }
            Demande::Annuler => match self.avant.take() {
                Some(avant) => {
                    match profils::remettre(&avant) {
                        Ok(()) => self.message("Réglages d'avant remis."),
                        Err(e) => self.erreur(&format!("Retour impossible : {e}.")),
                    }
                    self.etat();
                }
                None => self.erreur("Rien à annuler."),
            },
            Demande::AppliquerReglages { nom, reglages } => match serde_json::from_value::<Config>(reglages) {
                Ok(cfg) => {
                    self.avant = profils::capturer();
                    match config::apply_keys(&cfg, |s, k| profils::in_profile(s, k) && (s, k) != ("souris", "jeu")) {
                        Ok(()) => {
                            config::request_reload();
                            self.envoyer(&json!({ "type": "applique", "nom": nom }));
                        }
                        Err(e) => self.erreur(&format!("Réglages refusés : {e}.")),
                    }
                    self.etat();
                }
                Err(e) => self.erreur(&format!("Réglages illisibles : {e}.")),
            },
            Demande::Pause => unsafe {
                if let Ok(h) = FindWindowW(crate::win::TRAY_CLASS, None) {
                    let _ = PostMessageW(Some(h), WM_COMMAND, WPARAM(crate::win::CMD_PAUSE), LPARAM(0));
                }
            },
            Demande::Demarrage { actif } => {
                match regler_demarrage(actif) {
                    Ok(()) => self.message(if actif { "Essaim démarrera avec Windows." } else { "Essaim ne démarrera plus avec Windows." }),
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

    /// Installe le jeu de curseurs d'un preset qu'on vient d'appliquer, si l'option est cochée
    /// et qu'il en précise un autre que celui en place.
    fn curseurs_du_preset(&self, preset: &Config) {
        if let Err(e) = profils::curseurs_du_preset(preset) {
            self.erreur(&format!("Preset appliqué, mais curseurs non installés : {e}."));
        }
    }

    /// Ouvre dans le navigateur la création du fichier du preset dans le dépôt de la
    /// bibliothèque : GitHub propose alors une copie du dépôt et une pull request.
    fn publier(&self, nom: &str) {
        let depot = Config::read(&config::path()).unwrap_or_default().bibliotheque.depot;
        let texte = match std::fs::read_to_string(profils::dir().join(format!("{nom}.toml"))) {
            Ok(t) => t,
            Err(e) => return self.erreur(&format!("Preset « {nom} » enregistré, mais illisible pour la publication ({e}).")),
        };
        let page = format!("https://github.com/{depot}/new/main?filename={}", encoder(&format!("presets/{nom}.toml")));
        let url = format!("{page}&value={}", encoder(&texte));
        // une adresse trop longue est coupée par le navigateur ou refusée par GitHub : le
        // preset passe alors par le presse-papiers
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
        crate::win::open_url(&page);
        self.message(&format!(
            "Preset « {nom} » enregistré et copié : GitHub s'ouvre, colle-le (Ctrl+V) dans le fichier pour le proposer à la bibliothèque."
        ));
    }

    /// Son et fichiers : envoyés à la page quand ils changent.
    fn tick(&mut self, quoi: usize) {
        if quoi == TIMER_SON && !self.reduite {
            let au = self.son.view();
            let cle = (au.kicks ^ au.aigus.rotate_left(16), au.bass.to_bits());
            if cle != self.son_envoye {
                self.son_envoye = cle;
                self.envoyer(&json!({ "type": "son", "kicks": au.kicks, "basse": au.bass, "hauteur": au.pitch, "aigus": au.aigus, "brillance": au.brillance, "teinte": crate::render::sync_hue() }));
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
                "raison": crate::charge::raison(),
            }));
            if dates() != self.dates {
                self.etat();
            }
        }
    }
}

const CLE_RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

/// Essaim démarre-t-il avec la session (valeur `Essaim` de la clé Run de l'utilisateur) ?
fn demarrage_actif() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    unsafe { RegGetValueW(HKEY_CURRENT_USER, CLE_RUN, w!("Essaim"), RRF_RT_REG_SZ, None, None, None) }.is_ok()
}

/// Démarrage avec la session : l'exécutable en cours, ou plus rien.
fn regler_demarrage(actif: bool) -> Result<(), String> {
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    unsafe {
        if actif {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let valeur = format!("\"{}\"", exe.display());
            let octets: Vec<u8> = valeur.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect();
            RegSetKeyValueW(HKEY_CURRENT_USER, CLE_RUN, w!("Essaim"), REG_SZ.0, Some(octets.as_ptr() as *const _), octets.len() as u32)
                .ok()
                .map_err(|e| e.to_string())
        } else {
            let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, CLE_RUN, w!("Essaim"));
            if r.is_ok() || r == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(r.to_hresult().message()) }
        }
    }
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

/// Dossier Images de l'utilisateur, là où Windows le place (il peut être déplacé ou redirigé).
fn dossier_images() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Pictures, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Pictures, KF_FLAG_DEFAULT, None).ok()?;
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
    let dossier = dossier_images().ok_or("dossier Images introuvable")?.join("Essaim");
    std::fs::create_dir_all(&dossier).map_err(|e| e.to_string())?;
    let f = dossier.join(format!("{}.gif", nom_de_gif(nom)));
    std::fs::write(&f, octets).map_err(|e| e.to_string())?;
    Ok(f)
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
const URL_MAX: usize = 6000;

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

/// Encodage d'URL (RFC 3986) : tout sauf les caractères non réservés.
fn encoder(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
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
        // SAFETY : la fenêtre vit tant que la vue (détruite avant elle, dans `run`).
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(h))) })
    }
}

fn run(son: Arc<AudioShared>) -> anyhow::Result<()> {
    let hwnd = unsafe { creer_fenetre()? };
    WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    barre_de_titre(hwnd);

    let recus = RECUS.with(|r| r.clone());
    let (r, g, b) = fond(theme_sombre());
    let mut contexte = wry::WebContext::new(Some(config::dir().join("webview")));
    let vue = WebViewBuilder::new_with_web_context(&mut contexte)
        .with_custom_protocol("essaim".into(), |_, req| servir(req))
        .with_url(format!("{ORIGINE}index.html"))
        .with_background_color((r, g, b, 255))
        .with_ipc_handler(move |req| recus.borrow_mut().push(req.into_body()))
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
        .with_hotkeys_zoom(false)
        .with_default_context_menus(cfg!(debug_assertions))
        .with_browser_accelerator_keys(false)
        .with_devtools(cfg!(debug_assertions))
        .build(&Parent(hwnd))?;
    let _ = vue.focus();
    let fenetre = Fenetre {
        hwnd,
        vue,
        son,
        son_envoye: (u32::MAX, 0),
        dates: (None, None),
        apercus: Default::default(),
        avant: None,
        jeux: None,
        reduite: false,
    };
    FENETRE.with(|f| *f.borrow_mut() = Some(fenetre));
    unsafe {
        SetTimer(Some(hwnd), TIMER_SON, 33, None);
        SetTimer(Some(hwnd), TIMER_FICHIERS, 700, None);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            traiter_recus();
        }
    }
    // la vue avant le contexte et la fenêtre
    FENETRE.with(|f| f.borrow_mut().take());
    drop(contexte);
    Ok(())
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
        hIcon: crate::win::app_icon().unwrap_or_default(),
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

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
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
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_DESTROY => {
                FENETRE.with(|f| {
                    if let Ok(f) = f.try_borrow() {
                        if let Some(f) = f.as_ref() {
                            let _ = KillTimer(Some(f.hwnd), TIMER_SON);
                            let _ = KillTimer(Some(f.hwnd), TIMER_FICHIERS);
                        }
                    }
                });
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decode() {
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
        assert_eq!(encoder("presets/Nuit blanche.toml"), "presets%2FNuit%20blanche.toml");
        assert_eq!(encoder("é=1\n"), "%C3%A9%3D1%0A");
    }

    #[test]
    fn la_page_et_ses_fichiers_sont_servis() {
        let req = |p: &str| wry::http::Request::builder().uri(format!("{ORIGINE}{p}")).body(Vec::new()).unwrap();
        assert_eq!(servir(req("")).status(), 200);
        assert_eq!(servir(req("app.js")).headers()["Content-Type"], "text/javascript; charset=utf-8");
        assert_eq!(servir(req("../src/main.rs")).status(), 404);
        // chaque fichier nommé par la page existe
        let page = std::str::from_utf8(FICHIERS[0].2).unwrap();
        for (chemin, ..) in &FICHIERS[1..4] {
            assert!(page.contains(chemin), "{chemin} non chargé par index.html");
        }
    }

    #[test]
    fn messages_de_la_page() {
        let d: Demande = serde_json::from_str(r#"{"cmd":"regler","section":"orbite","cle":"rayon","valeur":30}"#).unwrap();
        assert!(matches!(d, Demande::Regler { .. }));
        let d: Demande = serde_json::from_str(r#"{"cmd":"enregistrer","nom":"Nuit","publier":true}"#).unwrap();
        assert!(matches!(d, Demande::Enregistrer { publier: true, .. }));
        assert!(serde_json::from_str::<Demande>(r#"{"cmd":"formater_disque"}"#).is_err());
    }
}

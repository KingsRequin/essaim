//! Morceau en cours, lu par l'API média de Windows (`GlobalSystemMediaTransportControls`, celle
//! des touches multimédia et du panneau du volume) : titre, artiste, lecture ou pause, et teinte
//! dominante de la pochette (`[couleur] pochette`). Un changement de piste pendant la lecture
//! incrémente un compteur que la boucle d'affichage salue d'une rafale (`[musique] rafale_piste`).
//!
//! Tout se passe sur un fil à part (« essaim-media ») : les appels WinRT asynchrones y sont
//! attendus, `ATTENTE_MAX` au plus chacun (au-delà : annulés, le tour est sauté), jamais sur le
//! fil de rendu, qui ne lit que l'état publié ici. Mise à jour
//! sur les événements de Windows (session courante, propriétés du morceau, lecture), et par une
//! interrogation de secours toutes les 5 s (chaque seconde tant qu'une pochette ou une reprise
//! de lecture est attendue). Sans l'API (Windows N sans Media Feature Pack), rien ne change : c'est
//! noté une fois au journal.

use crate::appli::teinte_dominante;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform, ColorManagementMode,
    ExifOrientationMode,
};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Gestionnaire,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Proprietes,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Statut,
};
use windows::core::{Error, Result as WinResult, HRESULT};
use windows::Win32::Foundation::{E_ABORT, E_FAIL, ERROR_TIMEOUT};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// Interrogation de secours quand aucun événement n'arrive (les événements de Windows suffisent
/// d'ordinaire : elle ne rattrape qu'un événement perdu).
const INTERROGATION: Duration = Duration::from_secs(5);
/// Interrogation rapprochée tant qu'une réponse est attendue sous peu : pochette pas encore
/// arrivée, ou changement de piste vu en pause qui compte si la lecture reprend.
const INTERROGATION_PROCHE: Duration = Duration::from_secs(1);
/// Après un événement, on attend que les suivants (titre, puis pochette…) soient passés.
const APAISEMENT: Duration = Duration::from_millis(150);
/// Un changement de piste vu en pause compte si la lecture reprend dans ce délai (titre et
/// lecture n'arrivent pas toujours dans le même ordre).
const DELAI_LECTURE: Duration = Duration::from_secs(3);
/// Pochette absente au changement de morceau (elle arrive souvent après le titre) : relue aux
/// interrogations suivantes, ce nombre de fois au plus.
const ESSAIS_POCHETTE: u8 = 3;
/// Côté de la miniature décodée pour la teinte (pixels).
const COTE_MINIATURE: u32 = 48;
/// Pochette plus grande (côté en pixels, ou taille du fichier) : ignorée, sans teinte. Une
/// pochette ordinaire fait quelques centaines de pixels et quelques centaines de Ko.
const POCHETTE_COTE_MAX: u32 = 4096;
const POCHETTE_OCTETS_MAX: u64 = 16 << 20;
/// Attente au plus d'un appel asynchrone de Windows (une application qui ne répond plus ne bloque
/// pas le fil), et pas de l'interrogation de son état.
const ATTENTE_MAX: Duration = Duration::from_secs(2);
const ATTENTE_PAS: Duration = Duration::from_millis(5);

/// Attend une opération asynchrone WinRT (`IAsyncOperation`) `ATTENTE_MAX` au plus : son
/// résultat, son erreur, ou `ERROR_TIMEOUT` après l'avoir annulée. Une macro, parce que le type
/// de l'opération (crate `windows-future`, non réexportée) n'est pas nommable d'ici.
macro_rules! attendre {
    ($op:expr) => {
        $op.and_then(|op| attente(|| op.Status().map(|s| s.0), || op.ErrorCode(), || op.Cancel()).and_then(|()| op.GetResults()))
    };
}

/// États d'une opération asynchrone (`AsyncStatus`).
const EN_COURS: i32 = 0;
const TERMINEE: i32 = 1;
const ANNULEE: i32 = 2;

/// Interroge `statut` jusqu'à la fin de l'opération, `ATTENTE_MAX` au plus (puis `annuler`) ;
/// `Ok` si elle s'est terminée sans erreur.
fn attente(statut: impl Fn() -> WinResult<i32>, code: impl Fn() -> WinResult<HRESULT>, annuler: impl Fn() -> WinResult<()>) -> WinResult<()> {
    let limite = Instant::now() + ATTENTE_MAX;
    loop {
        match statut()? {
            EN_COURS if Instant::now() < limite => std::thread::sleep(ATTENTE_PAS),
            EN_COURS => {
                let _ = annuler();
                return Err(Error::new(ERROR_TIMEOUT.to_hresult(), "délai dépassé"));
            }
            TERMINEE => return Ok(()),
            ANNULEE => return Err(Error::new(E_ABORT, "opération annulée")),
            _ => {
                let c = code()?;
                return Err(if c.is_err() { Error::from_hresult(c) } else { Error::new(E_FAIL, "opération en échec") });
            }
        }
    }
}

fn delai_depasse(e: &Error) -> bool {
    e.code() == ERROR_TIMEOUT.to_hresult()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Morceau {
    pub titre: String,
    pub artiste: String,
}

struct Etat {
    morceau: Option<Morceau>,
    /// Lecture (vrai) ou pause/arrêt (faux) ; aucune session : `None`.
    lecture: Option<bool>,
    /// Teinte dominante de la pochette du morceau en cours (aucune : grise ou absente).
    pochette: Option<f32>,
}

static ETAT: Mutex<Etat> = Mutex::new(Etat { morceau: None, lecture: None, pochette: None });
/// Changements de piste salués depuis le lancement.
static PISTES: AtomicU32 = AtomicU32::new(0);
/// Incrémenté quand le morceau ou la lecture change (infobulle de l'icône).
static VERSION: AtomicU32 = AtomicU32::new(0);
static LANCE: Once = Once::new();

fn etat() -> std::sync::MutexGuard<'static, Etat> {
    ETAT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Lance le fil de lecture (une seule fois).
pub fn demarrer() {
    LANCE.call_once(|| {
        if let Err(e) = std::thread::Builder::new().name("essaim-media".into()).spawn(fil) {
            log::warn!("média : fil impossible ({e}) ; titre, pochette et changement de piste désactivés");
        }
    });
}

/// Morceau en cours (session média courante), s'il y en a un.
pub fn morceau() -> Option<Morceau> {
    etat().morceau.clone()
}

/// Lecture en cours (vrai) ou en pause (faux) ; `None` sans session média.
pub fn en_lecture() -> Option<bool> {
    etat().lecture
}

/// Teinte de la pochette, seulement quand le morceau joue.
pub fn teinte_pochette() -> Option<f32> {
    if en_lecture() == Some(true) {
        etat().pochette
    } else {
        None
    }
}

/// Compteur des changements de piste (comparé d'une image à l'autre, comme celui des kicks).
pub fn pistes() -> u32 {
    PISTES.load(Ordering::Relaxed)
}

/// Change avec le morceau ou la lecture.
pub fn version() -> u32 {
    VERSION.load(Ordering::Relaxed)
}

/// Morceau pour la page des réglages : `{"titre", "artiste"}`, ou `null`.
pub fn json() -> serde_json::Value {
    morceau().map_or(serde_json::Value::Null, |m| serde_json::json!({ "titre": m.titre, "artiste": m.artiste }))
}

/// Infobulle de l'icône : `base`, et le morceau en cours sur une seconde ligne.
pub fn infobulle(base: &str) -> String {
    ligne_infobulle(base, morceau().as_ref())
}

/// `base` suivi du morceau (« titre — artiste »), tronqué pour tenir dans les 127 caractères
/// UTF-16 d'une infobulle de notification.
fn ligne_infobulle(base: &str, morceau: Option<&Morceau>) -> String {
    let Some(m) = morceau else { return base.to_owned() };
    let texte = match (m.titre.is_empty(), m.artiste.is_empty()) {
        (false, false) => format!("{} — {}", m.titre, m.artiste),
        (false, true) => m.titre.clone(),
        (true, false) => m.artiste.clone(),
        (true, true) => return base.to_owned(),
    };
    let place = 127usize.saturating_sub(base.encode_utf16().count() + 1);
    if place < 2 {
        return base.to_owned();
    }
    let texte = if texte.encode_utf16().count() <= place {
        texte
    } else {
        let mut court = String::new();
        let mut n = 0;
        for c in texte.chars() {
            if n + c.len_utf16() > place - 1 {
                break;
            }
            n += c.len_utf16();
            court.push(c);
        }
        court.trim_end().to_owned() + "…"
    };
    format!("{base}\n{texte}")
}

/// Source de la teinte imposée à l'essaim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Pochette,
    Appli,
    Prisme,
}

impl Source {
    pub fn nom(self) -> &'static str {
        match self {
            Source::Pochette => "celle de la pochette du morceau en cours",
            Source::Appli => "celle de l'application ou du site au premier plan",
            Source::Prisme => "celle de l'éclairage (Prisme)",
        }
    }
}

/// Teinte imposée : la pochette du morceau qui joue, sinon l'application ou le site au premier
/// plan, sinon l'éclairage de Prisme. Chaque source vaut `None` quand son option est coupée ou
/// qu'elle n'a pas de teinte ; les suivantes ne sont interrogées que si besoin.
pub fn teinte_imposee(
    pochette: Option<f32>,
    appli: impl FnOnce() -> Option<f32>,
    prisme: impl FnOnce() -> Option<f32>,
) -> Option<(f32, Source)> {
    pochette
        .map(|h| (h, Source::Pochette))
        .or_else(|| appli().map(|h| (h, Source::Appli)))
        .or_else(|| prisme().map(|h| (h, Source::Prisme)))
}

/// Détection d'un changement de piste à saluer : le morceau change pendant la lecture. Ni le
/// premier morceau vu (lancement d'Essaim, première lecture), ni une pause ou une reprise, ni
/// une session qui disparaît puis revient sur le même morceau ne comptent.
#[derive(Default)]
struct SuiviPiste {
    vu: Option<Morceau>,
    /// Changement vu hors lecture : compte si la lecture reprend avant cette date.
    differe: Option<Instant>,
}

impl SuiviPiste {
    fn observer(&mut self, morceau: Option<&Morceau>, lecture: bool, now: Instant) -> bool {
        if let Some(m) = morceau {
            if self.vu.as_ref() != Some(m) {
                let premier = self.vu.is_none();
                self.vu = Some(m.clone());
                self.differe = (!premier && !lecture).then_some(now + DELAI_LECTURE);
                return !premier && lecture;
            }
        }
        if lecture {
            self.differe.take().is_some_and(|d| now <= d)
        } else {
            if self.differe.is_some_and(|d| now > d) {
                self.differe = None;
            }
            false
        }
    }
}

enum Signal {
    Session,
    Proprietes,
    Lecture,
}

/// Abonnement aux événements d'une session (retiré quand elle n'est plus la courante).
struct Abonnement {
    session: Session,
    jetons: (Option<i64>, Option<i64>),
}

impl Abonnement {
    fn new(session: Session, tx: &Sender<Signal>) -> Self {
        let (a, b) = (tx.clone(), tx.clone());
        let proprietes = session
            .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                let _ = a.send(Signal::Proprietes);
                Ok(())
            }))
            .ok();
        let lecture = session
            .PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                let _ = b.send(Signal::Lecture);
                Ok(())
            }))
            .ok();
        Self { session, jetons: (proprietes, lecture) }
    }
}

impl Drop for Abonnement {
    fn drop(&mut self) {
        if let Some(j) = self.jetons.0 {
            let _ = self.session.RemoveMediaPropertiesChanged(j);
        }
        if let Some(j) = self.jetons.1 {
            let _ = self.session.RemovePlaybackInfoChanged(j);
        }
    }
}

/// Ce que dit une session : morceau (titres vides : aucun), lecture, propriétés.
type Lu = (Option<Morceau>, Option<bool>, Option<Proprietes>);

/// Lecture et propriétés de la session. `Err` seulement si l'application tarde à répondre : le
/// tour est alors sauté, l'état publié ne change pas.
fn lire(session: &Session) -> WinResult<Lu> {
    let lecture = session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).ok().map(|s| s == Statut::Playing);
    let props = match attendre!(session.TryGetMediaPropertiesAsync()) {
        Ok(p) => Some(p),
        Err(e) if delai_depasse(&e) => return Err(e),
        Err(_) => None,
    };
    let morceau = props.as_ref().and_then(|p| {
        let texte = |r: WinResult<windows::core::HSTRING>| r.map(|h| h.to_string().trim().to_owned()).unwrap_or_default();
        let m = Morceau { titre: texte(p.Title()), artiste: texte(p.Artist()) };
        (!m.titre.is_empty() || !m.artiste.is_empty()).then_some(m)
    });
    Ok((morceau, lecture, props))
}

/// Une pochette de `octets` et de plus grand côté `cote` (pixels) est-elle à décoder ?
fn pochette_acceptable(octets: u64, cote: u32) -> bool {
    octets <= POCHETTE_OCTETS_MAX && cote <= POCHETTE_COTE_MAX
}

/// Teinte dominante de la pochette (miniature réduite à 48 × 48 avant le calcul). `Err` : pas
/// de miniature lisible (relue au tour suivant) ; `Ok(None)` : pas de teinte, ou pochette trop
/// lourde ou trop grande, ignorée sans être décodée.
fn teinte_miniature(props: &Proprietes) -> WinResult<Option<f32>> {
    let flux = attendre!(props.Thumbnail().and_then(|t| t.OpenReadAsync()))?;
    let octets = flux.Size()?;
    if !pochette_acceptable(octets, 0) {
        log::info!("média : pochette de {} Mo ignorée", octets >> 20);
        return Ok(None);
    }
    // l'en-tête seulement : la taille de l'image est connue avant tout décodage
    let decodeur = attendre!(BitmapDecoder::CreateAsync(&flux))?;
    let (l, h) = (decodeur.PixelWidth()?, decodeur.PixelHeight()?);
    if !pochette_acceptable(octets, l.max(h)) {
        log::info!("média : pochette de {l} × {h} px ignorée");
        return Ok(None);
    }
    let reduction = BitmapTransform::new()?;
    reduction.SetScaledWidth(COTE_MINIATURE)?;
    reduction.SetScaledHeight(COTE_MINIATURE)?;
    reduction.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    let pixels = attendre!(decodeur.GetPixelDataTransformedAsync(
        BitmapPixelFormat::Bgra8,
        BitmapAlphaMode::Straight,
        &reduction,
        ExifOrientationMode::IgnoreExifOrientation,
        ColorManagementMode::DoNotColorManage,
    ))?
    .DetachPixelData()?;
    Ok(teinte_dominante(&pixels))
}

/// Nom de l'application d'une session (pour le journal).
fn nom_appli(session: Option<&Session>) -> String {
    session.and_then(|s| s.SourceAppUserModelId().ok()).map_or_else(|| "aucune".into(), |h| h.to_string())
}

fn fil() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    // le service média de Windows peut tarder (ouverture de session) : redemandé plus tard
    let gestionnaire = loop {
        match attendre!(Gestionnaire::RequestAsync()) {
            Ok(g) => break g,
            Err(e) if delai_depasse(&e) => {
                log::info!("média : API de Windows lente à répondre, nouvel essai");
                std::thread::sleep(INTERROGATION);
            }
            Err(e) => {
                log::warn!("média : API de Windows indisponible ({e}) ; titre, pochette et changement de piste désactivés");
                return;
            }
        }
    };
    let (tx, rx) = channel::<Signal>();
    let t = tx.clone();
    let _jeton = gestionnaire.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = t.send(Signal::Session);
        Ok(())
    }));
    let mut lecteur = Lecteur::default();
    loop {
        lecteur.tour(&gestionnaire, &tx);
        // attente : un événement (puis la fin de sa rafale), ou l'interrogation de secours
        let attendu = (lecteur.essais > 0 && etat().pochette.is_none()) || lecteur.suivi.differe.is_some();
        let mut premier = match rx.recv_timeout(if attendu { INTERROGATION_PROCHE } else { INTERROGATION }) {
            Ok(s) => Some(s),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        while let Some(s) = premier {
            if matches!(s, Signal::Proprietes | Signal::Session) {
                lecteur.relire_pochette = true;
            }
            premier = rx.recv_timeout(APAISEMENT).ok();
        }
    }
}

/// État du fil « essaim-media » d'un tour à l'autre.
#[derive(Default)]
struct Lecteur {
    abonnement: Option<Abonnement>,
    suivi: SuiviPiste,
    /// Morceau dont la pochette a été lue, et essais restants si elle manquait.
    pochette_de: Option<Morceau>,
    essais: u8,
    relire_pochette: bool,
    appli_vue: String,
    /// Une application a tardé à répondre (noté une fois au journal jusqu'à sa prochaine réponse).
    lente: bool,
    miniature_illisible: bool,
}

impl Lecteur {
    /// Un tour : session courante, morceau, pochette, état publié. Une application qui tarde à
    /// répondre fait sauter le tour, sans rien publier.
    fn tour(&mut self, gestionnaire: &Gestionnaire, tx: &Sender<Signal>) {
        // la pochette n'est relue que si le morceau change ou sur un événement : un même objet
        // de session rendu sous un autre pointeur ne coûte qu'un réabonnement
        let courante = gestionnaire.GetCurrentSession().ok();
        if self.abonnement.as_ref().map(|a| &a.session) != courante.as_ref() {
            let nom = nom_appli(courante.as_ref());
            if nom != self.appli_vue {
                log::info!("média : session de {nom}");
                self.appli_vue = nom;
            }
            self.abonnement = courante.map(|s| Abonnement::new(s, tx));
        }
        let (morceau, lecture, props) = match self.abonnement.as_ref().map(|a| lire(&a.session)).transpose() {
            Ok(lu) => {
                self.lente = false;
                lu.unwrap_or_default()
            }
            Err(e) => {
                if !std::mem::replace(&mut self.lente, true) {
                    log::info!("média : {} ne répond pas ({e}), tour sauté", self.appli_vue);
                }
                return;
            }
        };
        if morceau != self.pochette_de {
            (self.pochette_de, self.essais, self.relire_pochette) = (morceau.clone(), ESSAIS_POCHETTE, true);
        }
        let mut pochette = etat().pochette;
        if self.relire_pochette || (pochette.is_none() && self.essais > 0) {
            self.relire_pochette = false;
            self.essais = self.essais.saturating_sub(1);
            pochette = match props.as_ref().filter(|_| morceau.is_some()).map(teinte_miniature) {
                Some(Ok(h)) => {
                    self.essais = 0;
                    h
                }
                Some(Err(e)) => {
                    if self.essais == 0 && !std::mem::replace(&mut self.miniature_illisible, true) {
                        log::info!("média : pochette illisible ({e})");
                    }
                    None
                }
                None => None,
            };
        }
        let salue = self.suivi.observer(morceau.as_ref(), lecture == Some(true), Instant::now());
        {
            let mut e = etat();
            if e.morceau != morceau || e.lecture != lecture {
                VERSION.fetch_add(1, Ordering::Relaxed);
            }
            if e.pochette.map(f32::round) != pochette.map(f32::round) {
                log::info!("média : teinte de la pochette {}", pochette.map_or("grise ou absente".into(), |h| format!("{h:.0}°")));
            }
            (e.morceau, e.lecture, e.pochette) = (morceau, lecture, pochette);
        }
        if salue {
            PISTES.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(titre: &str, artiste: &str) -> Morceau {
        Morceau { titre: titre.into(), artiste: artiste.into() }
    }

    #[test]
    fn changement_de_piste_et_pause() {
        let t0 = Instant::now();
        let s = |x: u64| t0 + Duration::from_millis(x);
        let mut p = SuiviPiste::default();
        // lancement d'Essaim pendant un morceau : rien
        assert!(!p.observer(Some(&m("A", "X")), true, s(0)));
        assert!(!p.observer(Some(&m("A", "X")), true, s(1000)));
        // pause puis reprise : rien
        assert!(!p.observer(Some(&m("A", "X")), false, s(2000)));
        assert!(!p.observer(Some(&m("A", "X")), true, s(3000)));
        // piste suivante pendant la lecture : une rafale, une seule
        assert!(p.observer(Some(&m("B", "X")), true, s(4000)));
        assert!(!p.observer(Some(&m("B", "X")), true, s(5000)));
        // même titre, autre artiste : nouvelle piste
        assert!(p.observer(Some(&m("B", "Y")), true, s(6000)));
        // session disparue puis revenue sur le même morceau : rien
        assert!(!p.observer(None, false, s(7000)));
        assert!(!p.observer(Some(&m("B", "Y")), true, s(8000)));
        // titre changé un instant avant la reprise de la lecture : rafale à la reprise
        assert!(!p.observer(Some(&m("C", "Y")), false, s(9000)));
        assert!(p.observer(Some(&m("C", "Y")), true, s(9300)));
        assert!(!p.observer(Some(&m("C", "Y")), true, s(9600)));
        // piste choisie en pause, lecture reprise bien plus tard : rien
        assert!(!p.observer(Some(&m("D", "Y")), false, s(10_000)));
        assert!(!p.observer(Some(&m("D", "Y")), false, s(20_000)));
        assert!(!p.observer(Some(&m("D", "Y")), true, s(30_000)));
    }

    #[test]
    fn premiere_lecture_sans_rafale() {
        let t0 = Instant::now();
        let mut p = SuiviPiste::default();
        // aucune session au lancement, puis un morceau lancé : ce n'est pas un changement
        assert!(!p.observer(None, false, t0));
        assert!(!p.observer(Some(&m("A", "X")), true, t0 + Duration::from_secs(5)));
    }

    #[test]
    fn priorite_des_teintes() {
        let jamais = || -> Option<f32> { panic!("source interrogée pour rien") };
        assert_eq!(teinte_imposee(Some(10.0), jamais, jamais), Some((10.0, Source::Pochette)));
        assert_eq!(teinte_imposee(None, || Some(20.0), jamais), Some((20.0, Source::Appli)));
        assert_eq!(teinte_imposee(None, || None, || Some(30.0)), Some((30.0, Source::Prisme)));
        assert_eq!(teinte_imposee(None, || None, || None), None);
    }

    #[test]
    fn teinte_d_une_pochette() {
        // pochette 48 × 48 : fond noir, grand aplat bleu, un peu de texte blanc
        let mut px = Vec::new();
        for i in 0..48 * 48 {
            let [r, g, b] = match i % 10 {
                0..=5 => [30, 80, 220],
                6 => [255, 255, 255],
                _ => [0, 0, 0],
            };
            px.extend([b, g, r, 255]);
        }
        let h = teinte_dominante(&px).unwrap();
        assert!((h - 224.0).abs() < 4.0, "{h}");
    }

    #[test]
    fn infobulle_avec_le_morceau() {
        assert_eq!(ligne_infobulle("Essaim", None), "Essaim");
        assert_eq!(ligne_infobulle("Essaim", Some(&m("Titre", "Artiste"))), "Essaim\nTitre — Artiste");
        assert_eq!(ligne_infobulle("Essaim", Some(&m("Titre", ""))), "Essaim\nTitre");
        assert_eq!(ligne_infobulle("Essaim", Some(&m("", ""))), "Essaim");
        let long = ligne_infobulle("Essaim — preset « Néon »", Some(&m(&"é".repeat(200), "Artiste")));
        assert_eq!(long.encode_utf16().count(), 127, "{long}");
        assert!(long.ends_with('…'));
    }

    #[test]
    fn attente_bornee() {
        use std::cell::Cell;
        // terminée, en échec, annulée
        assert!(attente(|| Ok(TERMINEE), || Ok(HRESULT(0)), || Ok(())).is_ok());
        let e = attente(|| Ok(3), || Ok(E_FAIL), || Ok(())).unwrap_err();
        assert_eq!(e.code(), E_FAIL);
        assert!(!delai_depasse(&attente(|| Ok(ANNULEE), || Ok(HRESULT(0)), || Ok(())).unwrap_err()));
        // terminée après quelques interrogations
        let n = Cell::new(0);
        let statut = || {
            n.set(n.get() + 1);
            Ok(if n.get() < 4 { EN_COURS } else { TERMINEE })
        };
        assert!(attente(statut, || Ok(HRESULT(0)), || Ok(())).is_ok());
        // jamais finie : annulée après `ATTENTE_MAX`, et le délai est reconnu
        let (annulee, t0) = (Cell::new(false), Instant::now());
        let e = attente(
            || Ok(EN_COURS),
            || Ok(HRESULT(0)),
            || {
                annulee.set(true);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(delai_depasse(&e) && annulee.get());
        let duree = t0.elapsed();
        assert!(duree >= ATTENTE_MAX && duree < ATTENTE_MAX + Duration::from_secs(1), "{duree:?}");
    }

    #[test]
    fn pochettes_demesurees_ignorees() {
        assert!(pochette_acceptable(300_000, 600));
        assert!(pochette_acceptable(POCHETTE_OCTETS_MAX, POCHETTE_COTE_MAX));
        assert!(!pochette_acceptable(POCHETTE_OCTETS_MAX + 1, 600));
        assert!(!pochette_acceptable(300_000, 20_000));
    }

    /// Lit la session média réelle (lecture seule) : `cargo test --release -- --ignored
    /// session_reelle --nocapture`.
    #[test]
    #[ignore]
    fn session_reelle() {
        let g = Gestionnaire::RequestAsync().and_then(|op| op.join()).expect("API média");
        let Ok(s) = g.GetCurrentSession() else {
            println!("aucune session média");
            return;
        };
        let meme = g.GetCurrentSession().map(|s2| s2 == s).unwrap_or(false);
        let (morceau, lecture, props) = lire(&s).expect("session média muette");
        println!("appli : {}", nom_appli(Some(&s)));
        println!("même objet d'un appel à l'autre : {meme}");
        println!("morceau : {morceau:?}");
        println!("lecture : {lecture:?}");
        println!("pochette : {:?}", props.as_ref().map(teinte_miniature));
    }
}

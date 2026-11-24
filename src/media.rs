//! Morceau en cours, lu par l'API média de Windows (`GlobalSystemMediaTransportControls`, celle
//! des touches multimédia et du panneau du volume) : titre, artiste, lecture ou pause, et teinte
//! dominante de la pochette (`[couleur] pochette`). Un changement de piste pendant la lecture
//! incrémente un compteur que la boucle d'affichage salue d'une rafale (`[musique] rafale_piste`).
//!
//! Tout se passe sur un fil à part (« essaim-media ») : les appels WinRT asynchrones y sont
//! attendus (`join`), jamais sur le fil de rendu, qui ne lit que l'état publié ici. Mise à jour
//! sur les événements de Windows (session courante, propriétés du morceau, lecture), et par une
//! interrogation de secours toutes les 5 s (chaque seconde tant qu'une pochette ou une reprise
//! de lecture est attendue). Sans l'API (Windows N sans Media Feature Pack), rien ne change : c'est
//! noté une fois au journal.

use crate::appli::teinte_dominante;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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

/// Lecture et propriétés de la session (titres vides : aucun morceau).
fn lire(session: &Session) -> (Option<Morceau>, Option<bool>, Option<Proprietes>) {
    let lecture = session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).ok().map(|s| s == Statut::Playing);
    let props = session.TryGetMediaPropertiesAsync().and_then(|op| op.join()).ok();
    let morceau = props.as_ref().and_then(|p| {
        let texte = |r: windows::core::Result<windows::core::HSTRING>| r.map(|h| h.to_string().trim().to_owned()).unwrap_or_default();
        let m = Morceau { titre: texte(p.Title()), artiste: texte(p.Artist()) };
        (!m.titre.is_empty() || !m.artiste.is_empty()).then_some(m)
    });
    (morceau, lecture, props)
}

/// Teinte dominante de la pochette (miniature réduite à 48 × 48 avant le calcul). `Err` : pas
/// de miniature lisible.
fn teinte_miniature(props: &Proprietes) -> windows::core::Result<Option<f32>> {
    let flux = props.Thumbnail()?.OpenReadAsync()?.join()?;
    let decodeur = BitmapDecoder::CreateAsync(&flux)?.join()?;
    let reduction = BitmapTransform::new()?;
    reduction.SetScaledWidth(COTE_MINIATURE)?;
    reduction.SetScaledHeight(COTE_MINIATURE)?;
    reduction.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    let pixels = decodeur
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Straight,
            &reduction,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )?
        .join()?
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
    let gestionnaire = match Gestionnaire::RequestAsync().and_then(|op| op.join()) {
        Ok(g) => g,
        Err(e) => {
            log::warn!("média : API de Windows indisponible ({e}) ; titre, pochette et changement de piste désactivés");
            return;
        }
    };
    let (tx, rx) = channel::<Signal>();
    let t = tx.clone();
    let _jeton = gestionnaire.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = t.send(Signal::Session);
        Ok(())
    }));
    static MINIATURE_ILLISIBLE: AtomicBool = AtomicBool::new(false);
    let mut abonnement: Option<Abonnement> = None;
    let mut suivi = SuiviPiste::default();
    // morceau dont la pochette a été lue, et essais restants si elle manquait
    let (mut pochette_de, mut essais) = (None::<Morceau>, 0u8);
    let mut relire_pochette = false;
    let mut appli_vue = String::new();
    loop {
        // la pochette n'est relue que si le morceau change ou sur un événement : un même objet
        // de session rendu sous un autre pointeur ne coûte qu'un réabonnement
        let courante = gestionnaire.GetCurrentSession().ok();
        if abonnement.as_ref().map(|a| &a.session) != courante.as_ref() {
            let nom = nom_appli(courante.as_ref());
            if nom != appli_vue {
                log::info!("média : session de {nom}");
                appli_vue = nom;
            }
            abonnement = courante.map(|s| Abonnement::new(s, &tx));
        }
        let (morceau, lecture, props) = abonnement.as_ref().map(|a| lire(&a.session)).unwrap_or_default();
        if morceau != pochette_de {
            (pochette_de, essais, relire_pochette) = (morceau.clone(), ESSAIS_POCHETTE, true);
        }
        let mut pochette = etat().pochette;
        if relire_pochette || (pochette.is_none() && essais > 0) {
            relire_pochette = false;
            essais = essais.saturating_sub(1);
            pochette = match props.as_ref().filter(|_| morceau.is_some()).map(teinte_miniature) {
                Some(Ok(h)) => {
                    essais = 0;
                    h
                }
                Some(Err(e)) => {
                    if essais == 0 && !MINIATURE_ILLISIBLE.swap(true, Ordering::Relaxed) {
                        log::info!("média : pochette illisible ({e})");
                    }
                    None
                }
                None => None,
            };
        }
        let salue = suivi.observer(morceau.as_ref(), lecture == Some(true), Instant::now());
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
        // attente : un événement (puis la fin de sa rafale), ou l'interrogation de secours
        let attendu = (essais > 0 && pochette.is_none()) || suivi.differe.is_some();
        let mut premier = match rx.recv_timeout(if attendu { INTERROGATION_PROCHE } else { INTERROGATION }) {
            Ok(s) => Some(s),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        while let Some(s) = premier {
            if matches!(s, Signal::Proprietes | Signal::Session) {
                relire_pochette = true;
            }
            premier = rx.recv_timeout(APAISEMENT).ok();
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
        let (morceau, lecture, props) = lire(&s);
        println!("appli : {}", nom_appli(Some(&s)));
        println!("même objet d'un appel à l'autre : {meme}");
        println!("morceau : {morceau:?}");
        println!("lecture : {lecture:?}");
        println!("pochette : {:?}", props.as_ref().map(teinte_miniature));
    }
}

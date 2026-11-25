//! Jeux de curseurs de Windows (ceux du panneau « Pointeurs » des paramètres de la souris).
//!
//! Essaim ne dessine pas le pointeur : il installe un jeu existant comme le fait Windows
//! (valeurs de `HKCU\Control Panel\Cursors`, puis `SPI_SETCURSORS`). Les jeux viennent de
//! `HKCU\…\Cursors\Schemes` (ceux de l'utilisateur, packs installés) et de la clé système.

use std::ffi::c_void;
use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, HANDLE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegEnumValueW, RegGetValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_DWORD, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Shell::SHLoadIndirectString;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyCursor, DrawIconEx, GetIconInfo, LoadImageW, ICONINFO, SystemParametersInfoW, DI_NORMAL, HCURSOR, HICON, IMAGE_CURSOR, LR_LOADFROMFILE,
    SPIF_SENDCHANGE, SPI_SETCURSORS,
};

/// Rôles des curseurs, dans l'ordre des fichiers d'un jeu.
const ROLES: [&str; 17] = [
    "Arrow", "Help", "AppStarting", "Wait", "Crosshair", "IBeam", "NWPen", "No", "SizeNS", "SizeWE", "SizeNWSE", "SizeNESW",
    "SizeAll", "UpArrow", "Hand", "Pin", "Person",
];
const CURSEURS: &str = "Control Panel\\Cursors";
const JEUX_UTILISATEUR: &str = "Control Panel\\Cursors\\Schemes";
const JEUX_SYSTEME: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Control Panel\\Cursors\\Schemes";

pub struct Jeu {
    /// Nom de la valeur dans le registre (ce qu'enregistrent la config et les presets).
    pub cle: String,
    /// Nom affiché (les noms système `@main.cpl,-…` sont traduits).
    pub nom: String,
    pub systeme: bool,
    fichiers: Vec<String>,
}

struct Cle(HKEY);
impl Drop for Cle {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn ouvrir(racine: HKEY, chemin: &str, ecriture: bool) -> Option<Cle> {
    let mut k = HKEY::default();
    let droits = if ecriture { KEY_READ | KEY_SET_VALUE } else { KEY_READ };
    (unsafe { RegOpenKeyExW(racine, &HSTRING::from(chemin), None, droits, &mut k) } == ERROR_SUCCESS).then_some(Cle(k))
}

/// Texte d'une valeur ; le tampon est agrandi si elle est longue (un jeu aux chemins longs).
fn lire_texte(racine: HKEY, chemin: &str, nom: &str) -> Option<String> {
    let (chemin, nom) = (HSTRING::from(chemin), HSTRING::from(nom));
    let mut buf = vec![0u16; 4096];
    // trois essais au plus : la taille donnée par un refus, au cas où la valeur change entre-temps
    for _ in 0..3 {
        let mut n = (buf.len() * 2) as u32;
        let r = unsafe {
            RegGetValueW(
                racine,
                &chemin,
                &nom,
                RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
                None,
                Some(buf.as_mut_ptr() as *mut c_void),
                Some(&mut n),
            )
        };
        match r {
            ERROR_SUCCESS => {
                let n = (n as usize / 2).min(buf.len());
                return Some(String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').to_owned());
            }
            ERROR_MORE_DATA => buf.resize((n as usize / 2 + 1).max(buf.len() * 2), 0),
            _ => return None,
        }
    }
    None
}

/// Taille de base du pointeur de Windows (px), celle d'un pointeur non agrandi.
const TAILLE_POINTEUR_BASE: u32 = 32;

/// Rapport de la taille du pointeur à sa taille de base (1 = normale), en bits de `f32` ; relu par
/// `relire_taille_pointeur`.
static ECHELLE_POINTEUR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3F80_0000);

/// Pointeur agrandi par Windows (Accessibilité, « Taille du pointeur ») : ce qui est posé à côté
/// de la pointe (étiquette du morceau, figures sous le pointeur) s'écarte d'autant. Rapport de la
/// taille du pointeur à 32 px.
pub fn echelle_pointeur() -> f32 {
    f32::from_bits(ECHELLE_POINTEUR.load(std::sync::atomic::Ordering::Relaxed))
}

/// Rapport d'une taille de base lue (`CursorBaseSize`) à la taille normale, borné ; 1 sans valeur.
fn echelle_de_taille(taille: Option<u32>) -> f32 {
    taille.filter(|&t| t > 0).map_or(1.0, |t| (t as f32 / TAILLE_POINTEUR_BASE as f32).clamp(1.0, 8.0))
}

/// Relit la taille du pointeur (`HKCU\Control Panel\Cursors\CursorBaseSize`, 32 par défaut),
/// en lecture seule ; la boucle principale l'appelle chaque seconde.
pub fn relire_taille_pointeur() {
    use windows::Win32::System::Registry::RRF_RT_REG_DWORD;
    let mut v = 0u32;
    let mut n = std::mem::size_of::<u32>() as u32;
    let lu = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(CURSEURS), w!("CursorBaseSize"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut c_void), Some(&mut n))
    } == ERROR_SUCCESS;
    let e = echelle_de_taille(lu.then_some(v));
    ECHELLE_POINTEUR.store(e.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Longueur maximale d'un nom de valeur du registre (caractères, sans le zéro final).
const NOM_DE_VALEUR_MAX: usize = 16_383;

/// Noms des valeurs de `cle`. Un nom plus long que le tampon le fait agrandir ; une valeur
/// illisible est passée sans arrêter l'énumération.
fn noms_de_valeurs(cle: &Cle) -> Vec<String> {
    let mut noms = Vec::new();
    let mut buf = vec![0u16; 512];
    let mut i = 0;
    loop {
        let mut n = buf.len() as u32;
        match unsafe { RegEnumValueW(cle.0, i, Some(PWSTR(buf.as_mut_ptr())), &mut n, None, None, None, None) } {
            ERROR_SUCCESS => noms.push(String::from_utf16_lossy(&buf[..(n as usize).min(buf.len())])),
            // nom trop long pour le tampon : agrandi à la taille maximale, même valeur relue
            ERROR_MORE_DATA if buf.len() <= NOM_DE_VALEUR_MAX => {
                buf.resize(NOM_DE_VALEUR_MAX + 1, 0);
                continue;
            }
            // toujours trop long (ne devrait pas arriver) : valeur passée
            ERROR_MORE_DATA => {}
            ERROR_NO_MORE_ITEMS => break,
            // clé supprimée ou inaccessible : la suite ne se lira pas mieux
            _ => break,
        }
        i += 1;
    }
    noms
}

/// Texte d'un tampon terminé par un zéro (tout le tampon s'il n'en a pas).
fn jusqu_au_zero(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
}

fn nom_affiche(cle: &str) -> String {
    if !cle.starts_with('@') {
        return cle.to_owned();
    }
    let mut buf = [0u16; 256];
    match unsafe { SHLoadIndirectString(&HSTRING::from(cle), &mut buf, None) } {
        Ok(()) => jusqu_au_zero(&buf),
        Err(_) => cle.to_owned(),
    }
}

/// Jeux de curseurs installés : ceux de l'utilisateur puis ceux de Windows, par nom.
pub fn jeux() -> Vec<Jeu> {
    let mut tous = Vec::new();
    for (racine, chemin, systeme) in [(HKEY_CURRENT_USER, JEUX_UTILISATEUR, false), (HKEY_LOCAL_MACHINE, JEUX_SYSTEME, true)] {
        let Some(k) = ouvrir(racine, chemin, false) else { continue };
        let mut liste: Vec<Jeu> = noms_de_valeurs(&k)
            .into_iter()
            .filter(|cle| !cle.is_empty())
            .filter_map(|cle| {
                let valeur = lire_texte(racine, chemin, &cle)?;
                let fichiers = valeur.split(',').map(|f| f.trim().to_owned()).collect();
                Some(Jeu { nom: nom_affiche(&cle), cle, systeme, fichiers })
            })
            .collect();
        liste.sort_by_key(|j| j.nom.to_lowercase());
        tous.extend(liste);
    }
    tous
}

/// Jeu installé en ce moment (vide : curseurs par défaut ou personnalisés à la main).
pub fn actuel() -> String {
    lire_texte(HKEY_CURRENT_USER, CURSEURS, "").unwrap_or_default()
}

/// Tenu pendant chaque écriture des valeurs de `HKCU\Control Panel\Cursors` : deux fils
/// (fenêtre des réglages, règle automatique) ne mêlent pas leurs jeux, et la sortie d'Essaim
/// attend la fin d'une écriture commencée (`attendre_curseurs`). Il n'est pas tenu pendant
/// `SPI_SETCURSORS`, qui prévient toutes les fenêtres (dont celles du fil principal, qui
/// pourrait attendre ce verrou).
static ECRITURE: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Essaim se ferme : plus aucune écriture ne commence.
static FERMETURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Écrit sous `ECRITURE`, sauf si Essaim se ferme.
fn ecrire<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let _verrou = ECRITURE.lock().unwrap_or_else(|e| e.into_inner());
    if FERMETURE.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Essaim se ferme".into());
    }
    f()
}

/// Fait recharger les curseurs par Windows depuis le registre ; l'appel peut signaler un échec
/// (code d'erreur périmé) alors que les curseurs ont bien changé : noté, sans plus.
fn recharger() {
    if let Err(e) = unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE) } {
        log::info!("curseurs : SPI_SETCURSORS a renvoyé {e}");
    }
}

/// À la sortie d'Essaim : plus aucune écriture de curseurs ne commence, et celle en cours est
/// attendue `delai` au plus. Faux si le délai est passé avant sa fin.
pub fn attendre_curseurs(delai: std::time::Duration) -> bool {
    FERMETURE.store(true, std::sync::atomic::Ordering::SeqCst);
    let fin = std::time::Instant::now() + delai;
    loop {
        match ECRITURE.try_lock() {
            Ok(_) | Err(std::sync::TryLockError::Poisoned(_)) => return true,
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        if std::time::Instant::now() >= fin {
            log::warn!("curseurs : écriture encore en cours à la sortie");
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Installe le jeu `cle`, comme le bouton Appliquer du panneau de la souris.
pub fn installer(cle: &str) -> Result<(), String> {
    let jeux = jeux();
    let jeu = jeux.iter().find(|j| j.cle == cle).ok_or_else(|| format!("jeu de curseurs « {cle} » introuvable"))?;
    let texte = |s: &str| s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect::<Vec<u8>>();
    ecrire(|| {
        let k = ouvrir(HKEY_CURRENT_USER, CURSEURS, true).ok_or("réglages des curseurs de Windows inaccessibles")?;
        unsafe {
            for (i, role) in ROLES.iter().enumerate() {
                let f = jeu.fichiers.get(i).map(String::as_str).unwrap_or("");
                let _ = RegSetValueExW(k.0, &HSTRING::from(*role), None, REG_EXPAND_SZ, Some(&texte(f)));
            }
            let _ = RegSetValueExW(k.0, PCWSTR::null(), None, REG_SZ, Some(&texte(&jeu.cle)));
            let source: u32 = if jeu.systeme { 2 } else { 1 };
            let _ = RegSetValueExW(k.0, w!("Scheme Source"), None, REG_DWORD, Some(&source.to_le_bytes()));
        }
        Ok(())
    })?;
    recharger();
    log::info!("curseurs : jeu « {} » installé", jeu.nom);
    Ok(())
}

/// Valeurs de `HKCU\Control Panel\Cursors` qui font les curseurs en place, telles quelles (type
/// et octets ; `None` = absente). Garde aussi des curseurs choisis un par un, sans nom de jeu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Etat(Vec<(&'static str, Option<Valeur>)>);

/// Valeur brute du registre : type (`REG_SZ`…) et octets.
type Valeur = (u32, Vec<u8>);

/// Valeurs gardées : le nom du jeu (valeur par défaut), sa source et les 17 rôles.
fn valeurs_etat() -> impl Iterator<Item = &'static str> {
    ["", "Scheme Source"].into_iter().chain(ROLES)
}

/// Curseurs en place, pour y revenir (« Annuler », fin d'une règle automatique). `None` si une
/// valeur n'a pas pu être lue (autre erreur que « absente ») : un état incomplet effacerait au
/// retour des rôles qui existent bien.
pub fn etat() -> Option<Etat> {
    let k = ouvrir(HKEY_CURRENT_USER, CURSEURS, false)?;
    // `Ok(None)` : valeur absente ; `Err(())` : lecture ratée
    let lire = |nom: &str| -> Result<Option<Valeur>, ()> {
        let nom = HSTRING::from(nom);
        let (mut genre, mut n) = (REG_VALUE_TYPE(0), 0u32);
        unsafe {
            match RegQueryValueExW(k.0, &nom, None, Some(&mut genre), None, Some(&mut n)) {
                ERROR_SUCCESS => {}
                ERROR_FILE_NOT_FOUND => return Ok(None),
                _ => return Err(()),
            }
            let mut octets = vec![0u8; n as usize];
            if RegQueryValueExW(k.0, &nom, None, Some(&mut genre), Some(octets.as_mut_ptr()), Some(&mut n)) != ERROR_SUCCESS {
                return Err(());
            }
            octets.truncate(n as usize);
            Ok(Some((genre.0, octets)))
        }
    };
    let valeurs: Result<Vec<_>, ()> = valeurs_etat().map(|nom| lire(nom).map(|v| (nom, v))).collect();
    match valeurs {
        Ok(v) => Some(Etat(v)),
        Err(()) => {
            log::warn!("curseurs : lecture des curseurs en place ratée, ils ne seront pas remis");
            None
        }
    }
}

/// Une valeur de `Etat` telle qu'écrite dans un fichier (`auto-avant.toml`) : type et octets en
/// hexadécimal, ou rien si elle était absente.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ValeurSauvee {
    pub nom: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub octets: Option<String>,
}

impl Etat {
    /// Valeurs à écrire dans un fichier.
    pub fn sauver(&self) -> Vec<ValeurSauvee> {
        self.0
            .iter()
            .map(|(nom, v)| ValeurSauvee {
                nom: (*nom).to_owned(),
                genre: v.as_ref().map(|v| v.0),
                octets: v.as_ref().map(|v| v.1.iter().map(|o| format!("{o:02x}")).collect()),
            })
            .collect()
    }

    /// Relit des valeurs écrites par `sauver` ; `None` si une valeur gardée manque, est en trop
    /// ou est abîmée (rien n'est alors remis plutôt qu'un état faux).
    pub fn reprendre(valeurs: &[ValeurSauvee]) -> Option<Etat> {
        if valeurs.len() != valeurs_etat().count() {
            return None;
        }
        let hex = |s: &str| -> Option<Vec<u8>> {
            if !s.len().is_multiple_of(2) {
                return None;
            }
            (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
        };
        let etat = valeurs_etat()
            .map(|nom| {
                let v = valeurs.iter().find(|v| v.nom == nom)?;
                let valeur = match (&v.genre, &v.octets) {
                    (Some(g), Some(o)) => Some((*g, hex(o)?)),
                    (None, None) => None,
                    _ => return None,
                };
                Some((nom, valeur))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Etat(etat))
    }
}

/// Remet les curseurs gardés par `etat` si `condition`, lue sur les curseurs en place sous le
/// même verrou que l'écriture (aucun autre jeu ne s'installe entre la lecture et l'écriture).
/// Vrai s'ils ont été remis.
pub fn restaurer_si(etat: &Etat, condition: impl FnOnce(Option<&Etat>) -> bool) -> Result<bool, String> {
    let remis = ecrire(|| {
        if !condition(self::etat().as_ref()) {
            return Ok(false);
        }
        let k = ouvrir(HKEY_CURRENT_USER, CURSEURS, true).ok_or("réglages des curseurs de Windows inaccessibles")?;
        unsafe {
            for (nom, valeur) in &etat.0 {
                let nom = HSTRING::from(*nom);
                let _ = match valeur {
                    Some((genre, octets)) => RegSetValueExW(k.0, &nom, None, REG_VALUE_TYPE(*genre), Some(octets)),
                    None => RegDeleteValueW(k.0, &nom),
                };
            }
        }
        Ok(true)
    })?;
    if remis {
        recharger();
        log::info!("curseurs : curseurs d'avant remis");
    }
    Ok(remis)
}

/// Remplace les `%VARIABLE%` d'un chemin.
fn developper(chemin: &str) -> String {
    let mut out = String::new();
    let mut reste = chemin;
    while let Some(i) = reste.find('%') {
        out.push_str(&reste[..i]);
        let apres = &reste[i + 1..];
        match apres.find('%') {
            Some(j) => {
                let var = &apres[..j];
                out.push_str(&std::env::var(var).unwrap_or_else(|_| format!("%{var}%")));
                reste = &apres[j + 1..];
            }
            None => {
                out.push('%');
                reste = apres;
            }
        }
    }
    out.push_str(reste);
    out
}

/// Nom de chaque rôle, comme dans les paramètres de la souris de Windows.
const NOMS_ROLES: [&str; 17] = [
    "Sélection normale", "Aide", "Travail en arrière-plan", "Occupé", "Précision", "Texte", "Écriture manuscrite",
    "Non disponible", "Redimensionnement vertical", "Redimensionnement horizontal", "Redimensionnement diagonal 1",
    "Redimensionnement diagonal 2", "Déplacement", "Autre sélection", "Lien", "Sélection d'emplacement",
    "Sélection de personne",
];

/// Un curseur dessiné : ses images côte à côte dans une bande PNG (une seule s'il est fixe),
/// la durée de chaque image (ms) et son point chaud (px dans le carré de `cote`).
#[derive(serde::Serialize)]
pub struct Image {
    pub png: String,
    pub cote: u32,
    pub images: u32,
    pub durees: Vec<u32>,
    pub chaud: (u32, u32),
}

/// Rôles montrés sur la carte d'un jeu : flèche, lien, texte, travail en arrière-plan.
const ROLES_APERCU: [usize; 4] = [0, 14, 5, 2];

/// Aperçu d'un jeu : ses quatre curseurs les plus vus.
pub fn apercu(jeu: &Jeu) -> Vec<Option<Image>> {
    ROLES_APERCU.iter().map(|&r| image(jeu, r, 44)).collect()
}

/// Chaque curseur du jeu : (nom du rôle, image si le jeu en a un).
pub fn curseurs(jeu: &Jeu) -> Vec<(&'static str, Option<Image>)> {
    NOMS_ROLES.iter().enumerate().map(|(i, nom)| (*nom, image(jeu, i, 64))).collect()
}

/// Animation d'un curseur `.ani` : pour chaque pas, l'image à montrer et sa durée (ms). Lue
/// dans les morceaux `anih` (nombre d'images et de pas, cadence), `rate` et `seq ` du fichier.
fn animation(octets: &[u8]) -> Option<Vec<(u32, u32)>> {
    if octets.len() < 12 || &octets[0..4] != b"RIFF" || &octets[8..12] != b"ACON" {
        return None;
    }
    let mot = |o: &[u8], i: usize| o.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (mut images, mut pas, mut cadence) = (0u32, 0u32, 0u32);
    let (mut rate, mut seq): (Option<Vec<u32>>, Option<Vec<u32>>) = (None, None);
    let mut i = 12;
    while i + 8 <= octets.len() {
        let id = &octets[i..i + 4];
        let taille = mot(octets, i + 4)? as usize;
        let data = octets.get(i + 8..(i + 8 + taille).min(octets.len()))?;
        let liste = |d: &[u8]| (0..d.len() / 4).filter_map(|k| mot(d, k * 4)).collect::<Vec<u32>>();
        match id {
            b"anih" => {
                images = mot(data, 4)?;
                pas = mot(data, 8)?;
                cadence = mot(data, 28)?;
            }
            b"rate" => rate = Some(liste(data)),
            b"seq " => seq = Some(liste(data)),
            _ => {}
        }
        i += 8 + taille + (taille & 1);
    }
    if images == 0 {
        return None;
    }
    let pas = pas.clamp(1, 120);
    Some(
        (0..pas)
            .map(|k| {
                let image = seq.as_ref().and_then(|s| s.get(k as usize).copied()).unwrap_or(k % images).min(images - 1);
                let jiffies = rate.as_ref().and_then(|r| r.get(k as usize).copied()).unwrap_or(cadence).max(1);
                // un fichier abîmé peut annoncer une durée énorme : pas de débordement
                (image, jiffies.saturating_mul(1000) / 60)
            })
            .collect(),
    )
}

/// Le curseur `role` du jeu, dessiné dans un carré de `cote` px (toutes ses images s'il est animé).
fn image(jeu: &Jeu, role: usize, cote: i32) -> Option<Image> {
    let f = jeu.fichiers.get(role).filter(|f| !f.is_empty())?;
    let chemin = developper(f);
    let pas = std::fs::read(&chemin).ok().and_then(|o| animation(&o)).unwrap_or_else(|| vec![(0, 0)]);
    unsafe {
        let HANDLE(c) = LoadImageW(None, &HSTRING::from(chemin.as_str()), IMAGE_CURSOR, cote, cote, LR_LOADFROMFILE).ok()?;
        let curseur = HICON(c);
        let mut info = ICONINFO::default();
        let chaud = if GetIconInfo(curseur, &mut info).is_ok() {
            let _ = DeleteObject(info.hbmColor.into());
            let _ = DeleteObject(info.hbmMask.into());
            (info.xHotspot.min(cote as u32 - 1), info.yHotspot.min(cote as u32 - 1))
        } else {
            (0, 0)
        };
        let n = pas.len() as i32;
        // dessin sur fond noir puis sur fond blanc : l'écart donne la transparence (marche aussi
        // pour les vieux curseurs monochromes, qui n'ont pas d'alpha)
        let noir = dessiner(curseur, &pas, cote, 0);
        let blanc = dessiner(curseur, &pas, cote, 255);
        let _ = DestroyCursor(HCURSOR(c));
        let (noir, blanc) = (noir?, blanc?);
        let mut pm = tiny_skia::Pixmap::new((cote * n) as u32, cote as u32)?;
        for ((d, n), b) in pm.data_mut().chunks_exact_mut(4).zip(noir.chunks_exact(4)).zip(blanc.chunks_exact(4)) {
            let a = 255 - (b[1] as i32 - n[1] as i32).clamp(0, 255);
            // BGRA sur noir = couleur × alpha (déjà prémultipliée)
            d.copy_from_slice(&[n[2].min(a as u8), n[1].min(a as u8), n[0].min(a as u8), a as u8]);
        }
        let png = pm.encode_png().ok()?;
        Some(Image {
            png: format!("data:image/png;base64,{}", base64(&png)),
            cote: cote as u32,
            images: n as u32,
            durees: pas.iter().map(|p| p.1).collect(),
            chaud,
        })
    }
}

/// Les images `pas` du curseur côte à côte sur un fond uni ; pixels BGRA.
unsafe fn dessiner(curseur: HICON, pas: &[(u32, u32)], cote: i32, fond: u8) -> Option<Vec<u8>> {
    let (w, h) = (cote * pas.len() as i32, cote);
    let dc = CreateCompatibleDC(None);
    let bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let Ok(bmp) = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
        let _ = DeleteDC(dc);
        return None;
    };
    if bits.is_null() {
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
        return None;
    }
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
    pixels.fill(fond);
    let ancien = SelectObject(dc, bmp.into());
    let mut dessines = 0;
    for (i, &(image, _)) in pas.iter().enumerate() {
        if DrawIconEx(dc, i as i32 * cote, 0, curseur, cote, cote, image, None, DI_NORMAL).is_ok() {
            dessines += 1;
        }
    }
    let copie = pixels.to_vec();
    SelectObject(dc, ancien);
    let _ = DeleteObject(bmp.into());
    let _ = DeleteDC(dc);
    (dessines > 0).then_some(copie)
}

fn base64(octets: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(octets.len().div_ceil(3) * 4);
    for c in octets.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { A[(n >> (18 - 6 * i)) as usize & 63] as char } else { '=' });
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointeur_agrandi_ecarte_ce_qui_l_entoure() {
        assert_eq!(echelle_de_taille(None), 1.0);
        assert_eq!(echelle_de_taille(Some(32)), 1.0);
        assert_eq!(echelle_de_taille(Some(64)), 2.0);
        assert_eq!(echelle_de_taille(Some(256)), 8.0);
        // valeur nulle ou plus petite que la normale : la taille normale
        assert_eq!(echelle_de_taille(Some(0)), 1.0);
        assert_eq!(echelle_de_taille(Some(16)), 1.0);
    }

    #[test]
    fn base64_et_chemins() {
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"M"), "TQ==");
        std::env::set_var("ESSAIM_TEST_RACINE", "C:\\Windows");
        assert_eq!(developper("%ESSAIM_TEST_RACINE%\\cursors\\a.cur"), "C:\\Windows\\cursors\\a.cur");
        assert_eq!(developper("100%"), "100%");
    }

    #[test]
    fn texte_sans_zero_final_garde_en_entier() {
        let t: Vec<u16> = "Aero".encode_utf16().collect();
        assert_eq!(jusqu_au_zero(&t), "Aero");
        let t: Vec<u16> = "Aero\0reste".encode_utf16().collect();
        assert_eq!(jusqu_au_zero(&t), "Aero");
        assert_eq!(jusqu_au_zero(&[]), "");
    }

    #[test]
    fn duree_enorme_d_un_curseur_anime_sans_debordement() {
        let morceau = |id: &[u8], data: &[u8]| [id, &(data.len() as u32).to_le_bytes(), data].concat();
        let mut anih = vec![0u8; 36];
        anih[4..8].copy_from_slice(&2u32.to_le_bytes()); // images
        anih[8..12].copy_from_slice(&2u32.to_le_bytes()); // pas
        anih[28..32].copy_from_slice(&6u32.to_le_bytes()); // cadence : 6 jiffies
        let rate = [u32::MAX.to_le_bytes(), 3u32.to_le_bytes()].concat();
        let corps = [&b"ACON"[..], &morceau(b"anih", &anih), &morceau(b"rate", &rate)].concat();
        let fichier = [&b"RIFF"[..], &(corps.len() as u32).to_le_bytes(), &corps].concat();
        assert_eq!(animation(&fichier), Some(vec![(0, u32::MAX / 60), (1, 50)]));
    }

    /// Un nom de valeur plus long que le premier tampon n'arrête pas l'énumération (clé
    /// temporaire de `HKCU\Software`, supprimée à la fin).
    #[test]
    fn nom_de_valeur_long_lu_sans_arreter_la_liste() {
        use windows::Win32::System::Registry::{RegCreateKeyExW, RegDeleteKeyW, RegDeleteTreeW, KEY_ALL_ACCESS, REG_OPTION_VOLATILE};
        let chemin = format!("Software\\Essaim-essai-souris-{}", std::process::id());
        let mut k = HKEY::default();
        let r = unsafe {
            RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(chemin.as_str()), None, PCWSTR::null(), REG_OPTION_VOLATILE, KEY_ALL_ACCESS, None, &mut k, None)
        };
        assert_eq!(r, ERROR_SUCCESS);
        let cle = Cle(k);
        let long = "L".repeat(2000);
        let noms = ["Avant".to_owned(), long.clone(), "Après".to_owned()];
        for nom in &noms {
            let r = unsafe { RegSetValueExW(cle.0, &HSTRING::from(nom.as_str()), None, REG_SZ, Some(&[0, 0])) };
            assert_eq!(r, ERROR_SUCCESS);
        }
        let mut lus = noms_de_valeurs(&cle);
        drop(cle);
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(chemin.as_str()));
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(chemin.as_str()));
        }
        lus.sort();
        let mut attendus = noms.to_vec();
        attendus.sort();
        assert_eq!(lus, attendus);
    }

    #[test]
    fn etat_sauve_puis_repris() {
        let etat = Etat(valeurs_etat().enumerate().map(|(i, nom)| (nom, (i % 3 != 0).then(|| (1, vec![i as u8, 0xff, 0])))).collect());
        let sauve = etat.sauver();
        assert_eq!(Etat::reprendre(&sauve), Some(etat.clone()));
        // une valeur manquante ou abîmée : rien n'est repris
        assert_eq!(Etat::reprendre(&sauve[1..]), None);
        let mut abime = sauve.clone();
        abime[1].octets = Some("zz".into());
        assert_eq!(Etat::reprendre(&abime), None);
        let mut sans_type = sauve;
        sans_type[1].genre = None;
        assert_eq!(Etat::reprendre(&sans_type), None);
    }

    /// Les pas d'un curseur animé du jeu en place donnent des images différentes (`--ignored`).
    #[test]
    #[ignore]
    fn les_images_d_un_curseur_anime_different() {
        let jeux = jeux();
        let jeu = jeux.iter().find(|j| j.cle == actuel()).expect("jeu en place");
        for role in [2usize, 3] {
            let f = developper(&jeu.fichiers[role]);
            let Some(pas) = std::fs::read(&f).ok().and_then(|o| animation(&o)) else { continue };
            let c = unsafe { LoadImageW(None, &HSTRING::from(f.as_str()), IMAGE_CURSOR, 32, 32, LR_LOADFROMFILE) }.unwrap();
            let px = unsafe { dessiner(HICON(c.0), &pas, 32, 0) }.unwrap();
            let image = |k: usize| (0..32).flat_map(|y| px[(y * 32 * pas.len() + k * 32) * 4..(y * 32 * pas.len() + k * 32 + 32) * 4].to_vec()).collect::<Vec<u8>>();
            let differentes = (1..pas.len()).filter(|&k| image(k) != image(0)).count();
            eprintln!("{} : {} pas, {differentes} différents du premier", NOMS_ROLES[role], pas.len());
            assert!(differentes > 0);
        }
    }

    /// Lit les jeux du vrai registre et dessine leurs aperçus (`--ignored --nocapture`).
    #[test]
    #[ignore]
    fn jeux_de_cette_machine() {
        for j in jeux() {
            let a = apercu(&j);
            let images: Vec<u32> = a.iter().map(|i| i.as_ref().map_or(0, |i| i.images)).collect();
            let octets: usize = a.iter().flatten().map(|i| i.png.len()).sum();
            eprintln!("{} : images par curseur {images:?}, {octets} octets", j.nom);
        }
        eprintln!("actuel : « {} »", actuel());
    }
}

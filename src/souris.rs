//! Jeux de curseurs de Windows (ceux du panneau « Pointeurs » des paramètres de la souris).
//!
//! Essaim ne dessine pas le pointeur : il installe un jeu existant comme le fait Windows
//! (valeurs de `HKCU\Control Panel\Cursors`, puis `SPI_SETCURSORS`). Les jeux viennent de
//! `HKCU\…\Cursors\Schemes` (ceux de l'utilisateur, packs installés) et de la clé système.

use std::ffi::c_void;
use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
    KEY_READ, KEY_SET_VALUE, REG_DWORD, REG_EXPAND_SZ, REG_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Shell::SHLoadIndirectString;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyCursor, DrawIconEx, LoadImageW, SystemParametersInfoW, DI_NORMAL, HCURSOR, HICON, IMAGE_CURSOR, LR_LOADFROMFILE,
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

fn lire_texte(racine: HKEY, chemin: &str, nom: &str) -> Option<String> {
    let mut buf = vec![0u16; 4096];
    let mut n = (buf.len() * 2) as u32;
    let ok = unsafe {
        RegGetValueW(
            racine,
            &HSTRING::from(chemin),
            &HSTRING::from(nom),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut n),
        )
    } == ERROR_SUCCESS;
    ok.then(|| String::from_utf16_lossy(&buf[..(n as usize / 2)]).trim_end_matches('\0').to_owned())
}

fn noms_de_valeurs(cle: &Cle) -> Vec<String> {
    let mut noms = Vec::new();
    for i in 0.. {
        let mut buf = [0u16; 512];
        let mut n = buf.len() as u32;
        if unsafe { RegEnumValueW(cle.0, i, Some(PWSTR(buf.as_mut_ptr())), &mut n, None, None, None, None) } != ERROR_SUCCESS {
            break;
        }
        noms.push(String::from_utf16_lossy(&buf[..n as usize]));
    }
    noms
}

fn nom_affiche(cle: &str) -> String {
    if !cle.starts_with('@') {
        return cle.to_owned();
    }
    let mut buf = [0u16; 256];
    match unsafe { SHLoadIndirectString(&HSTRING::from(cle), &mut buf, None) } {
        Ok(()) => String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(0)]),
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

/// Installe le jeu `cle`, comme le bouton Appliquer du panneau de la souris.
pub fn installer(cle: &str) -> Result<(), String> {
    let jeux = jeux();
    let jeu = jeux.iter().find(|j| j.cle == cle).ok_or_else(|| format!("jeu de curseurs « {cle} » introuvable"))?;
    let k = ouvrir(HKEY_CURRENT_USER, CURSEURS, true).ok_or("réglages des curseurs de Windows inaccessibles")?;
    let texte = |s: &str| s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect::<Vec<u8>>();
    unsafe {
        for (i, role) in ROLES.iter().enumerate() {
            let f = jeu.fichiers.get(i).map(String::as_str).unwrap_or("");
            let _ = RegSetValueExW(k.0, &HSTRING::from(*role), None, REG_EXPAND_SZ, Some(&texte(f)));
        }
        let _ = RegSetValueExW(k.0, PCWSTR::null(), None, REG_SZ, Some(&texte(&jeu.cle)));
        let source: u32 = if jeu.systeme { 2 } else { 1 };
        let _ = RegSetValueExW(k.0, w!("Scheme Source"), None, REG_DWORD, Some(&source.to_le_bytes()));
        // recharge les curseurs depuis le registre ; l'appel peut signaler un échec (code
        // d'erreur périmé) alors que les curseurs ont bien changé : noté, sans plus
        if let Err(e) = SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_SENDCHANGE) {
            log::info!("curseurs : SPI_SETCURSORS a renvoyé {e}");
        }
    }
    log::info!("curseurs : jeu « {} » installé", jeu.nom);
    Ok(())
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

/// Aperçu du jeu : flèche, main, texte et attente côte à côte, en PNG (`data:` URL).
pub fn apercu(jeu: &Jeu) -> Option<String> {
    image(jeu, &[0, 14, 5, 2], 44)
}

/// Chaque curseur du jeu, seul : (nom du rôle, image PNG en `data:` URL si le jeu en a un).
pub fn curseurs(jeu: &Jeu) -> Vec<(&'static str, Option<String>)> {
    NOMS_ROLES.iter().enumerate().map(|(i, nom)| (*nom, image(jeu, &[i], 64))).collect()
}

/// Les curseurs `roles` du jeu côte à côte, chacun dans un carré de `cote` px.
fn image(jeu: &Jeu, roles: &[usize], cote: i32) -> Option<String> {
    let (w, h) = (cote * roles.len() as i32 + 12 * (roles.len() as i32 - 1), cote);
    // dessin sur fond noir puis sur fond blanc : l'écart donne la transparence (marche aussi
    // pour les vieux curseurs monochromes, qui n'ont pas d'alpha)
    let noir = dessiner(jeu, roles, cote, w, h, 0)?;
    let blanc = dessiner(jeu, roles, cote, w, h, 255)?;
    let mut pm = tiny_skia::Pixmap::new(w as u32, h as u32)?;
    for ((d, n), b) in pm.data_mut().chunks_exact_mut(4).zip(noir.chunks_exact(4)).zip(blanc.chunks_exact(4)) {
        let a = 255 - (b[1] as i32 - n[1] as i32).clamp(0, 255);
        // BGRA sur noir = couleur × alpha (déjà prémultipliée)
        d.copy_from_slice(&[n[2].min(a as u8), n[1].min(a as u8), n[0].min(a as u8), a as u8]);
    }
    let png = pm.encode_png().ok()?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

fn dessiner(jeu: &Jeu, roles: &[usize], cote: i32, w: i32, h: i32, fond: u8) -> Option<Vec<u8>> {
    unsafe {
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
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
        pixels.fill(fond);
        let ancien = SelectObject(dc, bmp.into());
        let mut dessines = 0;
        for (i, &role) in roles.iter().enumerate() {
            let Some(f) = jeu.fichiers.get(role).filter(|f| !f.is_empty()) else { continue };
            let chemin = HSTRING::from(developper(f));
            if let Ok(HANDLE(c)) = LoadImageW(None, &chemin, IMAGE_CURSOR, cote, cote, LR_LOADFROMFILE) {
                if DrawIconEx(dc, i as i32 * (cote + 12), 0, HICON(c), cote, cote, 0, None, DI_NORMAL).is_ok() {
                    dessines += 1;
                }
                let _ = DestroyCursor(HCURSOR(c));
            }
        }
        let copie = pixels.to_vec();
        SelectObject(dc, ancien);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
        (dessines > 0).then_some(copie)
    }
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
    fn base64_et_chemins() {
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"M"), "TQ==");
        std::env::set_var("ESSAIM_TEST_RACINE", "C:\\Windows");
        assert_eq!(developper("%ESSAIM_TEST_RACINE%\\cursors\\a.cur"), "C:\\Windows\\cursors\\a.cur");
        assert_eq!(developper("100%"), "100%");
    }

    /// Lit les jeux du vrai registre et dessine leurs aperçus (`--ignored --nocapture`).
    #[test]
    #[ignore]
    fn jeux_de_cette_machine() {
        for j in jeux() {
            let a = apercu(&j);
            eprintln!("{} ({}) : {} fichiers, aperçu {} octets", j.nom, j.cle, j.fichiers.len(), a.map_or(0, |a| a.len()));
        }
        eprintln!("actuel : « {} »", actuel());
    }
}

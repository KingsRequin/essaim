//! Teinte de l'application au premier plan : la couleur dominante de l'icône de son exécutable
//! (`[couleur] application`), calculée une fois par exécutable ; et, dans un navigateur, celle du
//! site affiché (`[couleur] site`) : la couleur dominante du haut de la page, sous la barre
//! d'adresse, mesurée quand le titre de la fenêtre change et toutes les 3 s, sur un fil à part
//! (la copie d'écran ne retarde pas l'image). La fenêtre au premier plan est lue à chaque image
//! (`GetForegroundWindow`, sans hook), son titre toutes les 250 ms au plus.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, SelectObject,
    BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetForegroundWindow, GetIconInfo, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, HICON,
    ICONINFO,
};

/// Navigateurs reconnus (nom de l'exécutable) pour la couleur du site.
const NAVIGATEURS: [&str; 13] = [
    "chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe", "arc.exe", "zen.exe", "librewolf.exe",
    "floorp.exe", "waterfox.exe", "thorium.exe", "chromium.exe",
];

#[derive(Default)]
pub struct TeinteAppli {
    fenetre: isize,
    /// Teinte de l'icône de l'application au premier plan (aucune : grise).
    icone: Option<f32>,
    navigateur: bool,
    /// Titre de la fenêtre du navigateur à la dernière lecture, et prochaine mesure de la page.
    titre: String,
    mesure: Option<Instant>,
    /// Dernière lecture du titre.
    titre_lu: Option<Instant>,
    teinte: Option<f32>,
    cache: HashMap<String, Option<f32>>,
    /// Fil de mesure de la page, lancé à la première mesure.
    mesureur: Option<Mesureur>,
}

/// Lecture d'écran sur un fil à part : on lui envoie une fenêtre, il répond (fenêtre, teinte).
struct Mesureur {
    demandes: Sender<isize>,
    reponses: Receiver<(isize, Option<f32>)>,
    en_cours: bool,
}

impl Mesureur {
    fn lancer() -> Option<Self> {
        let (demandes, recues) = channel::<isize>();
        let (envoi, reponses) = channel();
        std::thread::Builder::new()
            .name("essaim-teinte".into())
            .spawn(move || {
                for h in recues {
                    if envoi.send((h, teinte_page(HWND(h as *mut c_void)))).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| log::warn!("teinte du site : fil impossible ({e})"))
            .ok()?;
        Some(Self { demandes, reponses, en_cours: false })
    }
}

impl TeinteAppli {
    /// Teinte (degrés) à suivre : celle du site dans un navigateur (`site`), sinon celle de
    /// l'icône de l'application (`appli`). Essaim lui-même, une icône grise ou une page sans
    /// couleur nette gardent la teinte précédente ; ailleurs, sans option qui s'applique, aucune.
    pub fn courante(&mut self, appli: bool, site: bool, now: Instant) -> Option<f32> {
        let fg = unsafe { GetForegroundWindow() };
        if fg.0 as isize != self.fenetre {
            self.fenetre = fg.0 as isize;
            let Some(exe) = executable(fg) else {
                self.navigateur = false;
                return self.teinte;
            };
            let nom = exe.rsplit('\\').next().unwrap_or(&exe).to_lowercase();
            self.navigateur = NAVIGATEURS.contains(&nom.as_str());
            self.icone = *self.cache.entry(exe.clone()).or_insert_with(|| {
                let t = icone(&exe).and_then(|px| teinte_dominante(&px));
                log::info!("teinte de {nom} : {}", t.map_or("grise, ignorée".into(), |h| format!("{h:.0}°")));
                t
            });
            (self.titre, self.mesure, self.titre_lu) = (String::new(), Some(now + Duration::from_millis(300)), None);
        }
        if site && self.navigateur {
            // nouvelle page (titre changé) : mesure un peu après, le temps qu'elle s'affiche
            if self.titre_lu.is_none_or(|t| now - t >= Duration::from_millis(250)) {
                self.titre_lu = Some(now);
                let titre = titre(fg);
                if titre != self.titre {
                    self.titre = titre;
                    self.mesure = Some(now + Duration::from_millis(700));
                }
            }
            if self.mesure.is_some_and(|m| now >= m) && !self.mesureur.as_ref().is_some_and(|m| m.en_cours) {
                self.mesure = Some(now + Duration::from_secs(3));
                if self.mesureur.is_none() {
                    self.mesureur = Mesureur::lancer();
                }
                if let Some(m) = &mut self.mesureur {
                    m.en_cours = m.demandes.send(self.fenetre).is_ok();
                }
            }
            // réponse du fil de mesure (ignorée si la fenêtre a changé entre-temps)
            if let Some((h, page)) = self.mesureur.as_mut().and_then(|m| m.reponses.try_recv().ok().inspect(|_| m.en_cours = false)) {
                if h == self.fenetre {
                    if let Some(t) = page.filter(|h| self.teinte.is_none_or(|t| ((h - t + 540.0) % 360.0 - 180.0).abs() > 10.0)) {
                        log::info!("teinte du site : {t:.0}°");
                    }
                    if let Some(t) = page.or(if appli { self.icone } else { None }) {
                        self.teinte = Some(t);
                    }
                }
            }
            return self.teinte;
        }
        if appli {
            if self.icone.is_some() {
                self.teinte = self.icone;
            }
            return self.teinte;
        }
        None
    }
}

fn titre(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Teinte dominante du haut de la page affichée par un navigateur : une bande sous les onglets
/// et la barre d'adresse, lue à l'écran (sans les calques d'Essaim, qui ne sont pas copiés).
fn teinte_page(hwnd: HWND) -> Option<f32> {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut r = RECT::default();
        GetWindowRect(hwnd, &mut r).ok()?;
        let e = GetDpiForWindow(hwnd).max(96) as i32;
        let px = |v: i32| v * e / 96;
        let (x, y) = (r.left + (r.right - r.left) / 20, r.top + px(130));
        let (w, h) = ((r.right - r.left) * 9 / 10, px(220).min(r.bottom - y));
        if w < 50 || h < 30 {
            return None;
        }
        let ecran = GetDC(None);
        let dc = CreateCompatibleDC(Some(ecran));
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
        let resultat = match CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bmp) => {
                let ancien = SelectObject(dc, bmp.into());
                let copie = BitBlt(dc, 0, 0, w, h, Some(ecran), x, y, SRCCOPY).is_ok();
                // un pixel sur trois dans chaque sens suffit ; alpha à 255 (la copie n'en a pas)
                let tout = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
                let echantillon: Vec<u8> = tout
                    .chunks_exact(4)
                    .enumerate()
                    .filter(|(i, _)| (i % w as usize).is_multiple_of(3) && (i / w as usize).is_multiple_of(3))
                    .flat_map(|(_, p)| [p[0], p[1], p[2], 255])
                    .collect();
                SelectObject(dc, ancien);
                let _ = DeleteObject(bmp.into());
                if copie { teinte_dominante(&echantillon) } else { None }
            }
            Err(_) => None,
        };
        let _ = DeleteDC(dc);
        ReleaseDC(None, ecran);
        resultat
    }
}

/// Contours de l'icône de l'application au premier plan (`[orbite] forme = "icone"`), relus à
/// chaque changement d'application et gardés par exécutable. Essaim lui-même (ses réglages) ne
/// compte pas : c'est l'application d'avant qui reste suivie. Une application sans icône ou une
/// icône sans forme nette gardent les contours précédents.
#[derive(Default)]
pub struct IconeAppli {
    fenetre: isize,
    /// Dernière application au premier plan (hors Essaim), et celle dont les contours sont suivis.
    derniere: String,
    exe: String,
    cache: HashMap<String, Vec<Vec<[f32; 2]>>>,
}

impl IconeAppli {
    /// Les contours, s'ils ont changé depuis l'appel précédent ; `actif` : le trajet suit l'icône
    /// (sinon, seule l'application au premier plan est notée).
    pub fn suivre(&mut self, actif: bool) -> Option<Vec<Vec<[f32; 2]>>> {
        let fg = unsafe { GetForegroundWindow() };
        if fg.0 as isize != self.fenetre {
            self.fenetre = fg.0 as isize;
            if let Some(exe) = executable(fg) {
                self.derniere = exe;
            }
        }
        if !actif || self.derniere.is_empty() || self.derniere == self.exe {
            return None;
        }
        let exe = self.derniere.clone();
        self.exe = exe.clone();
        let contours = self
            .cache
            .entry(exe.clone())
            .or_insert_with(|| {
                let o = crate::contours::Options { tous: true, ..Default::default() };
                let c = icone_rgba(&exe, 128).map(|(px, l, h)| crate::contours::contours(&px, l, h, &o).contours).unwrap_or_default();
                log::info!("contour de l'icône de {} : {} forme(s)", exe.rsplit('\\').next().unwrap_or(&exe), c.len());
                c
            })
            .clone();
        (!contours.is_empty()).then_some(contours)
    }
}

/// Nom (en minuscules) de l'exécutable de la fenêtre au premier plan ; aucun pour Essaim.
pub fn nom_premier_plan() -> Option<String> {
    let exe = executable(unsafe { GetForegroundWindow() })?;
    Some(exe.rsplit('\\').next().unwrap_or(&exe).to_lowercase())
}

/// Exécutables des fenêtres visibles (pour proposer des règles automatiques).
pub fn applis_ouvertes() -> Vec<String> {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};
    unsafe extern "system" fn chaque(h: HWND, l: LPARAM) -> windows::core::BOOL {
        let liste = unsafe { &mut *(l.0 as *mut Vec<String>) };
        if unsafe { IsWindowVisible(h) }.as_bool() && !titre(h).is_empty() {
            if let Some(exe) = executable(h) {
                let nom = exe.rsplit('\\').next().unwrap_or(&exe).to_lowercase();
                if !liste.contains(&nom) {
                    liste.push(nom);
                }
            }
        }
        true.into()
    }
    let mut liste: Vec<String> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(chaque), LPARAM(&mut liste as *mut _ as isize));
    }
    liste.sort();
    liste
}

/// Chemin de l'exécutable de la fenêtre (aucun pour les fenêtres d'Essaim).
fn executable(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == GetCurrentProcessId() {
            return None;
        }
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// Pixels BGRA de la grande icône de l'exécutable.
fn icone(exe: &str) -> Option<Vec<u8>> {
    let wide: Vec<u16> = exe.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let mut grande = HICON::default();
        if ExtractIconExW(PCWSTR(wide.as_ptr()), 0, Some(&mut grande), None, 1) == 0 || grande.is_invalid() {
            return None;
        }
        pixels_icone(grande).map(|(px, _, _)| px)
    }
}

/// Pixels RGBA de l'icône de l'exécutable, en `cote` × `cote` si elle existe à cette taille
/// (sinon la plus proche que Windows propose), avec sa largeur et sa hauteur. Une vieille icône
/// sans transparence est rendue opaque.
fn icone_rgba(exe: &str, cote: i32) -> Option<(Vec<u8>, usize, usize)> {
    use windows::Win32::UI::WindowsAndMessaging::PrivateExtractIconsW;
    let mut nom = [0u16; 260];
    for (d, c) in nom.iter_mut().zip(exe.encode_utf16().take(259)) {
        *d = c;
    }
    let mut h = [HICON::default()];
    let n = unsafe { PrivateExtractIconsW(&nom, 0, cote, cote, Some(&mut h), None, 0) };
    if n == 0 || n == u32::MAX || h[0].is_invalid() {
        return None;
    }
    let (mut px, l, ht) = unsafe { pixels_icone(h[0])? };
    let sans_alpha = px.chunks_exact(4).all(|p| p[3] == 0);
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
        if sans_alpha {
            p[3] = 255;
        }
    }
    Some((px, l, ht))
}

/// Pixels BGRA d'une icône (détruite ensuite), avec sa largeur et sa hauteur.
unsafe fn pixels_icone(icone: HICON) -> Option<(Vec<u8>, usize, usize)> {
    unsafe {
        let mut info = ICONINFO::default();
        let lu = GetIconInfo(icone, &mut info).is_ok();
        let _ = DestroyIcon(icone);
        if !lu {
            return None;
        }
        let px = (|| {
            let mut bm = BITMAP::default();
            if info.hbmColor.is_invalid()
                || GetObjectW(info.hbmColor.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut c_void)) == 0
            {
                return None;
            }
            let (w, h) = (bm.bmWidth, bm.bmHeight);
            let mut bi = BITMAPINFO {
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
            let mut px = vec![0u8; (w * h * 4) as usize];
            let dc = GetDC(None);
            let n = GetDIBits(dc, info.hbmColor, 0, h as u32, Some(px.as_mut_ptr() as *mut c_void), &mut bi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            (n > 0).then_some((px, w as usize, h as usize))
        })();
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
        px
    }
}

/// Teinte dominante (degrés) de pixels BGRA : histogramme de 36 cases pondéré par la saturation
/// et la luminosité, puis moyenne circulaire autour de la case la plus lourde. Rien si l'image
/// est presque grise.
pub fn teinte_dominante(bgra: &[u8]) -> Option<f32> {
    // vieilles icônes sans alpha : tout est opaque
    let sans_alpha = bgra.chunks_exact(4).all(|p| p[3] == 0);
    let mut cases = [0f32; 36];
    let mut pixels: Vec<(f32, f32)> = Vec::new();
    for p in bgra.chunks_exact(4) {
        let a = if sans_alpha { 1.0 } else { p[3] as f32 / 255.0 };
        let (b, g, r) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let s = if max > 0.0 { (max - min) / max } else { 0.0 };
        if a < 0.5 || s < 0.25 || max < 0.2 {
            continue;
        }
        let d = max - min;
        let h = if max == r { 60.0 * ((g - b) / d).rem_euclid(6.0) } else if max == g { 60.0 * ((b - r) / d + 2.0) } else { 60.0 * ((r - g) / d + 4.0) };
        let poids = s * max * a;
        cases[(h / 10.0) as usize % 36] += poids;
        pixels.push((h, poids));
    }
    let total: f32 = cases.iter().sum();
    let n = (bgra.len() / 4) as f32;
    if total < n * 0.03 {
        return None;
    }
    let (best, _) = cases.iter().enumerate().fold((0, 0.0), |m, (i, &w)| if w > m.1 { (i, w) } else { m });
    let centre = best as f32 * 10.0 + 5.0;
    let (mut x, mut y) = (0f32, 0f32);
    for (h, w) in pixels {
        let d = (h - centre + 540.0).rem_euclid(360.0) - 180.0;
        if d.abs() <= 20.0 {
            x += w * h.to_radians().cos();
            y += w * h.to_radians().sin();
        }
    }
    Some(y.atan2(x).to_degrees().rem_euclid(360.0))
}

#[cfg(test)]
mod tests {
    use super::teinte_dominante;

    fn image(pixels: &[([u8; 3], usize)]) -> Vec<u8> {
        pixels.iter().flat_map(|&([r, g, b], n)| std::iter::repeat_n([b, g, r, 255], n).flatten()).collect()
    }

    #[test]
    fn teinte_d_une_icone() {
        // icône Spotify : vert dominant, un peu de noir et de blanc
        let h = teinte_dominante(&image(&[([30, 215, 96], 500), ([0, 0, 0], 300), ([255, 255, 255], 224)])).unwrap();
        assert!((h - 141.0).abs() < 4.0, "{h}");
        // rouge, de part et d'autre de 0°
        let h = teinte_dominante(&image(&[([230, 20, 30], 400), ([230, 30, 20], 400)])).unwrap();
        assert!(!(10.0..350.0).contains(&h), "{h}");
        // grise : rien
        assert!(teinte_dominante(&image(&[([128, 128, 130], 1024)])).is_none());
    }
}

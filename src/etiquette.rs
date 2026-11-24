//! Étiquette du morceau (`[etiquette]`) : quand le morceau change, son titre et son artiste
//! s'écrivent à côté du pointeur, soulignés d'un filet qui bat avec le kick, puis s'effacent. Le
//! reste du temps, rien. C'est une information, pas une allure de l'essaim : hors des presets
//! (`profils::in_profile`), elle reste quand on en change.
//!
//! Le texte est écrit une fois par morceau dans une petite image (GDI, police Segoe UI), blanc
//! cerné de sombre pour rester lisible sur un fond clair comme sur un fond sombre ; les moteurs
//! de dessin la recopient (`Canvas::vignette`).
//!
//! `ui/apercu.js` en montre un exemple (« Titre du morceau ») : la modifier ici oblige à le
//! reporter là.

use crate::audio::AudioView;
use crate::canvas::Canvas;
use crate::config::Config;
use crate::layer::IRect;
use crate::render::prisme;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use tiny_skia::Pixmap;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GdiFlush, GetTextExtentPoint32W, SelectObject, SetBkMode, SetTextColor, TextOutW,
    ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, HFONT, OUT_DEFAULT_PRECIS, TRANSPARENT,
};

/// Place du texte par rapport au pointeur (px à 100 %) : à droite et dessous, hors de la flèche.
const DECALAGE: (f32, f32) = (16.0, 24.0);
/// Durée de l'entrée et de la sortie (s), et glissement du texte pendant l'entrée (px à 100 %).
const ENTREE: f32 = 0.3;
const SORTIE: f32 = 0.4;
const GLISSE: f32 = 6.0;
/// Cerne sombre autour des lettres : rayon (px) et opacité.
const CERNE: i32 = 2;
const CERNE_OPACITE: f32 = 0.6;
/// Ligne de l'artiste, par rapport au titre : taille et opacité.
const ARTISTE: (f32, f32) = (0.85, 0.9);
/// Graisse du titre et de l'artiste.
const GRAISSE: (i32, i32) = (700, 600);
/// Lettres gardées par ligne (au-delà : coupée, avec des points de suspension).
const LETTRES_MAX: usize = 48;
/// Filet sous le texte : épaisseur au repos et ce qu'un kick y ajoute (px à 100 %), écart au texte.
const FILET: (f32, f32) = (1.4, 1.6);
const FILET_ECART: f32 = 2.0;

fn lisse(a: f32, b: f32, x: f32) -> f32 {
    let k = ((x - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// Opacité du texte `age` secondes après le changement de piste : entrée, tenue `duree`, sortie.
fn opacite(age: f32, duree: f32) -> f32 {
    lisse(0.0, ENTREE, age) * (1.0 - lisse(duree, duree + SORTIE, age))
}

/// Ligne coupée à `LETTRES_MAX` lettres.
fn abregee(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= LETTRES_MAX {
        s.to_string()
    } else {
        s.chars().take(LETTRES_MAX - 1).chain(['…']).collect()
    }
}

/// Texte écrit : son image (RGBA prémultiplié), un numéro qui change à chaque écriture (la carte
/// graphique garde l'image tant qu'il ne change pas), la largeur du titre et la marge autour des
/// lettres (px).
struct Texte {
    image: Pixmap,
    id: u64,
    titre: f32,
    marge: f32,
}

/// Écrit `titre` (taille `taille` px, gras) et dessous `artiste` (plus petit, plus pâle, s'il y en
/// a un), en blanc cerné de sombre. `None` si le titre est vide ou si Windows refuse.
fn ecrire(titre: &str, artiste: &str, taille: f32) -> Option<Texte> {
    static NUMERO: AtomicU64 = AtomicU64::new(1);
    let lignes: Vec<(Vec<u16>, f32, i32, f32)> = [(abregee(titre), taille, GRAISSE.0, 1.0), (abregee(artiste), taille * ARTISTE.0, GRAISSE.1, ARTISTE.1)]
        .into_iter()
        .filter(|l| !l.0.is_empty())
        .map(|(s, px, graisse, opacite)| (s.encode_utf16().collect(), px, graisse, opacite))
        .collect();
    if titre.trim().is_empty() {
        return None;
    }
    let marge = CERNE + 1;
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let police = |px: f32, graisse: i32| -> HFONT {
            CreateFontW(-(px.round().max(6.0) as i32), 0, 0, 0, graisse, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, ANTIALIASED_QUALITY, 0, w!("Segoe UI"))
        };
        // mesure de chaque ligne : (police, largeur, hauteur)
        let mesures: Vec<(HFONT, i32, i32)> = lignes
            .iter()
            .map(|(texte, px, graisse, _)| {
                let f = police(*px, *graisse);
                let ancien = SelectObject(dc, f.into());
                let mut s = SIZE::default();
                let _ = GetTextExtentPoint32W(dc, texte, &mut s);
                SelectObject(dc, ancien);
                (f, s.cx.max(1), s.cy.max(1))
            })
            .collect();
        let (l, h) = (mesures.iter().map(|m| m.1).max().unwrap_or(1) + 2 * marge, mesures.iter().map(|m| m.2).sum::<i32>() + 2 * marge);
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: l, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // couverture de chaque pixel (0 à 255), déjà à l'opacité de sa ligne
        let mut couverture = vec![0u8; (l * h) as usize];
        let ecrit = match CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bmp) if !bits.is_null() => {
                let ancien = SelectObject(dc, bmp.into());
                std::slice::from_raw_parts_mut(bits as *mut u8, (l * h * 4) as usize).fill(0);
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, COLORREF(0x00ff_ffff));
                let mut y = marge;
                // lignes de l'image où commence chaque ligne de texte, avec son opacité
                let mut bandes = Vec::new();
                for ((texte, _, _, opacite), (f, _, haut)) in lignes.iter().zip(&mesures) {
                    let avant = SelectObject(dc, (*f).into());
                    let _ = TextOutW(dc, marge, y, texte);
                    SelectObject(dc, avant);
                    bandes.push((y, y + haut, *opacite));
                    y += haut;
                }
                let _ = GdiFlush();
                let pixels = std::slice::from_raw_parts(bits as *const u8, (l * h * 4) as usize);
                for (i, (c, p)) in couverture.iter_mut().zip(pixels.chunks_exact(4)).enumerate() {
                    let ligne = (i as i32) / l;
                    let opacite = bandes.iter().find(|b| ligne >= b.0 && ligne < b.1).map_or(0.0, |b| b.2);
                    *c = (p[0].max(p[1]).max(p[2]) as f32 * opacite).round() as u8;
                }
                SelectObject(dc, ancien);
                let _ = DeleteObject(bmp.into());
                true
            }
            Ok(bmp) => {
                let _ = DeleteObject(bmp.into());
                false
            }
            Err(_) => false,
        };
        for m in &mesures {
            let _ = DeleteObject(m.0.into());
        }
        let _ = DeleteDC(dc);
        if !ecrit {
            return None;
        }
        // cerne : la couverture la plus forte dans un disque de rayon `CERNE` autour du pixel
        let mut image = Pixmap::new(l as u32, h as u32)?;
        let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= l || y >= h { 0 } else { couverture[(y * l + x) as usize] };
        for (i, px) in image.data_mut().chunks_exact_mut(4).enumerate() {
            let (x, y) = (i as i32 % l, i as i32 / l);
            let c = couverture[i] as f32 / 255.0;
            let mut autour = 0u8;
            for dy in -CERNE..=CERNE {
                for dx in -CERNE..=CERNE {
                    if dx * dx + dy * dy <= CERNE * CERNE + 1 {
                        autour = autour.max(at(x + dx, y + dy));
                    }
                }
            }
            // blanc par-dessus le cerne sombre, prémultiplié
            let a = c + CERNE_OPACITE * (autour as f32 / 255.0) * (1.0 - c);
            let blanc = (c * 255.0).round() as u8;
            px.copy_from_slice(&[blanc, blanc, blanc, (a * 255.0).round() as u8]);
        }
        Some(Texte { image, id: NUMERO.fetch_add(1, Ordering::Relaxed), titre: mesures[0].1 as f32, marge: marge as f32 })
    }
}

pub struct Etiquette {
    /// Changements de piste et kicks déjà vus (compteurs du son) ; `None` avant la première image.
    pistes: Option<u32>,
    kicks: Option<u32>,
    /// Instant du changement de piste montré.
    debut: Option<f64>,
    /// Texte écrit pour ce morceau, et échelle de l'écran pour laquelle il l'a été.
    texte: Option<Texte>,
    echelle: f32,
    /// Enveloppe du kick (1 au coup, puis retombée).
    kick: f32,
    /// Coin haut gauche du texte (écran).
    coin: (f32, f32),
}

impl Etiquette {
    pub fn new() -> Self {
        Self { pistes: None, kicks: None, debut: None, texte: None, echelle: 1.0, kick: 0.0, coin: (0.0, 0.0) }
    }

    /// Avance d'une image : un changement de piste montre le morceau ; `ecran` est celui du
    /// pointeur (le texte passe à gauche ou au-dessus du pointeur plutôt que d'en sortir).
    #[allow(clippy::too_many_arguments)]
    pub fn avance(&mut self, t: f64, dt: f32, cursor: (f32, f32), scale: f32, au: &AudioView, cfg: &Config, ecran: Option<IRect>) {
        let c = &cfg.etiquette;
        if !c.actif {
            *self = Self::new();
            return;
        }
        // un changement de piste : le compteur a bougé depuis l'image précédente (rien au lancement)
        let pistes = crate::media::pistes();
        if self.pistes.is_some_and(|p| p != pistes) {
            (self.debut, self.texte) = (Some(t), None);
        }
        self.pistes = Some(pistes);
        if self.debut.is_some_and(|d| (t - d) as f32 >= c.duree + SORTIE) {
            (self.debut, self.texte) = (None, None);
        }
        if self.debut.is_some() && (self.texte.is_none() || self.echelle != scale) {
            let morceau = crate::media::morceau().unwrap_or_default();
            self.ecrire(&morceau.titre, &morceau.artiste, c.taille, scale);
        }
        self.kick = if self.kicks.is_some_and(|k| k != au.kicks) { 1.0 } else { self.kick * (-dt * 7.0).exp() };
        self.kicks = Some(au.kicks);
        self.placer(cursor, scale, ecran);
    }

    /// Écrit le morceau à montrer ; sans titre, rien n'est montré.
    fn ecrire(&mut self, titre: &str, artiste: &str, taille: f32, scale: f32) {
        (self.texte, self.echelle) = (ecrire(titre, artiste, taille * scale), scale);
        if self.texte.is_none() {
            self.debut = None;
        }
    }

    /// Place le texte près du pointeur, du côté où il tient dans l'écran.
    fn placer(&mut self, cursor: (f32, f32), scale: f32, ecran: Option<IRect>) {
        let (dx, dy) = (DECALAGE.0 * scale, DECALAGE.1 * scale);
        self.coin = (cursor.0 + dx, cursor.1 + dy);
        if let (Some(e), Some(tx)) = (ecran, &self.texte) {
            let (l, h) = (tx.image.width() as f32, tx.image.height() as f32 + (FILET_ECART + FILET.0 + FILET.1) * scale);
            if self.coin.0 + l > e.2 as f32 {
                self.coin.0 = (cursor.0 - dx - l).max(e.0 as f32);
            }
            if self.coin.1 + h > e.3 as f32 {
                self.coin.1 = (cursor.1 - dy - h).max(e.1 as f32);
            }
        }
        self.coin = (self.coin.0.round(), self.coin.1.round());
    }

    /// Zone écran du texte et de son filet ; `None` quand rien n'est montré.
    pub fn rect(&self) -> Option<IRect> {
        let (tx, scale) = (self.texte.as_ref().filter(|_| self.debut.is_some())?, self.echelle);
        let (l, h) = (tx.image.width() as f32, tx.image.height() as f32 + (FILET_ECART + FILET.0 + FILET.1) * scale);
        let (x, y) = self.coin;
        Some(((x - GLISSE * scale - 2.0).floor() as i32, (y - 2.0).floor() as i32, (x + l + 2.0).ceil() as i32, (y + h + 2.0).ceil() as i32))
    }

    pub fn dessiner(&self, cv: &mut dyn Canvas, t: f64, cfg: &Config, scale: f32) {
        let (Some(debut), Some(tx)) = (self.debut, &self.texte) else { return };
        let age = (t - debut) as f32;
        let (entree, a) = (lisse(0.0, ENTREE, age), opacite(age, cfg.etiquette.duree));
        if a <= 0.0 {
            return;
        }
        let (x, y) = (self.coin.0 - (GLISSE * scale * (1.0 - entree)).round(), self.coin.1);
        cv.vignette(&tx.image, tx.id, x, y, a);
        // filet sous le texte : il se déploie à l'entrée et bat avec le kick
        let epais = (FILET.0 + FILET.1 * self.kick) * scale;
        let yf = y + tx.image.height() as f32 - tx.marge + FILET_ECART * scale + epais / 2.0;
        cv.line((x + tx.marge, yf), (x + tx.marge + tx.titre * entree, yf), epais, prisme(cfg, t, 1.0), a, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ECRAN: IRect = (0, 0, 800, 600);

    #[test]
    fn le_texte_est_blanc_cerne_de_sombre() {
        let tx = ecrire("Titre du morceau", "Artiste", 12.0).expect("texte écrit");
        let (l, h) = (tx.image.width(), tx.image.height());
        assert!(l > 60 && h > 20 && h < 60, "{l} × {h}");
        let px: Vec<&[u8]> = tx.image.data().chunks_exact(4).collect();
        // du blanc plein (les lettres), du sombre translucide (le cerne), du vide autour
        assert!(px.iter().any(|p| p[0] > 230 && p[3] > 230), "pas de blanc");
        assert!(px.iter().any(|p| p[0] < 20 && p[3] > 100 && p[3] < 200), "pas de cerne");
        assert!(px.iter().filter(|p| p[3] == 0).count() > px.len() / 4, "pas de vide");
        // prémultiplié : aucune composante au-dessus de l'opacité
        assert!(px.iter().all(|p| p[0] <= p[3]));
        // le titre est plus large que l'artiste, plus petit ; un titre plus long élargit l'image
        assert!(tx.titre > 40.0 && tx.titre <= l as f32);
        let long = ecrire("Titre du morceau, version longue", "Artiste", 12.0).unwrap();
        assert!(long.image.width() > l && long.id != tx.id);
        // sans artiste : une seule ligne ; sans titre : rien
        assert!(ecrire("Titre", "", 12.0).unwrap().image.height() < h);
        assert!(ecrire("  ", "Artiste", 12.0).is_none());
        // un titre interminable est coupé
        let coupe = ecrire(&"a".repeat(400), "", 12.0).unwrap();
        assert!(coupe.image.width() < 800, "{} px", coupe.image.width());
        assert_eq!(abregee(&"é".repeat(60)).chars().count(), LETTRES_MAX);
    }

    /// Zone de l'écran (gauche, haut, droite, bas) lue par capture, en BGRA.
    fn capture(z: IRect) -> Vec<u8> {
        use windows::Win32::Graphics::Gdi::{BitBlt, GetDC, ReleaseDC, ROP_CODE};
        let (l, h) = (z.2 - z.0, z.3 - z.1);
        unsafe {
            let ecran = GetDC(None);
            let dc = CreateCompatibleDC(Some(ecran));
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: l, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
            let ancien = SelectObject(dc, bmp.into());
            // CAPTUREBLT : les fenêtres superposées (les calques d'Essaim) sont dans la capture
            BitBlt(dc, 0, 0, l, h, Some(ecran), z.0, z.1, ROP_CODE(0x40CC_0020)).unwrap();
            let pixels = std::slice::from_raw_parts(bits as *const u8, (l * h * 4) as usize).to_vec();
            SelectObject(dc, ancien);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            ReleaseDC(None, ecran);
            pixels
        }
    }

    /// Essai réel : l'étiquette dessinée par chacun des deux moteurs (carte graphique, puis
    /// processeur) au milieu de l'écran principal, puis relue par capture d'écran.
    /// `cargo test --release etiquette_reelle -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn etiquette_reelle() {
        use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
        let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let mut cfg = Config::default();
        cfg.etiquette.actif = true;
        let monitors = crate::win::monitors();
        let ecran = monitors[0].1;
        let centre = (((ecran.0 + ecran.2) / 2) as f32, ((ecran.1 + ecran.3) / 2) as f32);
        for gpu in [true, false] {
            let mut backend = crate::backend::Backend::new(gpu);
            let mut e = Etiquette::new();
            e.debut = Some(0.0);
            e.ecrire("Titre du morceau", "Artiste", cfg.etiquette.taille, 1.0);
            e.placer(centre, 1.0, Some(ecran));
            let zone = e.rect().unwrap();
            let avant = capture(zone);
            for _ in 0..20 {
                backend.frame(&monitors, zone, &[], &|cv, _| e.dessiner(cv, 1.0, &cfg, 1.0));
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let apres = capture(zone);
            if let Ok(dossier) = std::env::var("ESSAIM_PLANCHE") {
                let (l, h) = ((zone.2 - zone.0) as u32, (zone.3 - zone.1) as u32);
                let rgba: Vec<u8> = apres.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
                let pm = Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(l, h).unwrap()).unwrap();
                pm.save_png(std::path::Path::new(&dossier).join(format!("etiquette-{}.png", if backend.is_gpu() { "carte" } else { "processeur" }))).unwrap();
            }
            backend.clear();
            // les lettres et leur cerne ont changé l'écran ; du blanc franc s'y trouve
            let changes = avant.chunks_exact(4).zip(apres.chunks_exact(4)).filter(|(a, b)| (0..3).any(|i| a[i].abs_diff(b[i]) > 40)).count();
            let blancs = apres.chunks_exact(4).filter(|p| p[0] > 235 && p[1] > 235 && p[2] > 235).count();
            eprintln!("{} : {changes} pixels changés, {blancs} blancs", if backend.is_gpu() { "carte graphique" } else { "processeur" });
            assert_eq!(backend.is_gpu(), gpu, "moteur demandé indisponible");
            assert!(changes > 150 && blancs > 80, "étiquette absente de l'écran");
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    #[test]
    fn l_etiquette_entre_tient_et_sort() {
        assert_eq!(opacite(0.0, 3.0), 0.0);
        assert_eq!(opacite(1.0, 3.0), 1.0);
        assert!(opacite(3.2, 3.0) > 0.0 && opacite(3.2, 3.0) < 1.0);
        assert_eq!(opacite(3.0 + SORTIE, 3.0), 0.0);
    }

    #[test]
    fn l_etiquette_tient_dans_sa_zone_et_dans_l_ecran() {
        let mut cfg = Config::default();
        cfg.etiquette.actif = true;
        let mut e = Etiquette::new();
        // rien à montrer : ni zone ni dessin
        let mut pm = Pixmap::new(800, 600).unwrap();
        e.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, 0.0, &cfg, 1.0);
        assert!(e.rect().is_none() && pm.data().iter().all(|&b| b == 0));
        // un morceau, pointeur au milieu : le texte est à droite et sous le pointeur
        e.debut = Some(0.0);
        e.ecrire("Titre du morceau", "Artiste", cfg.etiquette.taille, 1.0);
        e.placer((400.0, 300.0), 1.0, Some(ECRAN));
        assert_eq!(e.coin, (416.0, 324.0));
        let zone = e.rect().unwrap();
        e.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, 1.0, &cfg, 1.0);
        let mut peints = 0;
        for (j, p) in pm.data().chunks_exact(4).enumerate() {
            if p[3] > 0 {
                peints += 1;
                let (x, y) = ((j % 800) as i32, (j / 800) as i32);
                assert!(x >= zone.0 && x < zone.2 && y >= zone.1 && y < zone.3, "pixel ({x}, {y}) hors de la zone {zone:?}");
            }
        }
        assert!(peints > 300, "{peints} pixels");
        // rien sur le pointeur
        assert_eq!(pm.pixel(400, 300).unwrap().alpha(), 0);
        // pointeur dans le coin bas droit : le texte passe à gauche et au-dessus, dans l'écran
        e.placer((790.0, 590.0), 1.0, Some(ECRAN));
        let z = e.rect().unwrap();
        assert!(z.2 <= 790 && z.3 <= 600 && z.0 >= -10, "{z:?}");
        // après sa durée, plus rien
        let mut pm = Pixmap::new(800, 600).unwrap();
        e.dessiner(&mut crate::canvas::PixCanvas { pm: &mut pm, origin: (0.0, 0.0) }, (cfg.etiquette.duree + SORTIE + 0.1) as f64, &cfg, 1.0);
        assert!(pm.data().iter().all(|&b| b == 0));
        // éteinte : tout est oublié
        cfg.etiquette.actif = false;
        e.avance(9.0, 0.01, (400.0, 300.0), 1.0, &AudioView::default(), &cfg, Some(ECRAN));
        assert!(e.rect().is_none() && e.texte.is_none());
    }
}

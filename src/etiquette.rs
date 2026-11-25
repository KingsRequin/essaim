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
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, ExtTextOutW, GdiFlush, GetTextExtentExPointW, GetTextExtentPoint32W, SelectObject, SetBkMode,
    SetTextColor, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, ETO_OPTIONS, ETO_RTLREADING, HDC, HFONT,
    OUT_DEFAULT_PRECIS, TRANSPARENT,
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
/// Largeur d'une ligne au plus (px à 100 %) : au-delà, coupée entre deux caractères visibles,
/// avec des points de suspension. Elle borne aussi le calcul du cerne, sur le fil de rendu.
const LARGEUR_MAX: f32 = 480.0;
/// Caractères au plus confiés à la mesure : un titre démesuré n'est pas mesuré en entier.
const CARACTERES_MAX: usize = 512;
const POINTS: &str = "…";
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

// --- Coupure du texte -------------------------------------------------------------------------
//
// Sans dépendance de segmentation Unicode : une table des caractères qui se rattachent au
// précédent suffit à ne jamais couper au milieu d'un emoji composé, d'un drapeau ou d'une lettre
// accentuée par marques combinantes.

/// Le caractère se rattache au précédent (ne commence pas un groupe visible) : marques
/// combinantes des principales écritures, ZWJ et ZWNJ, sélecteurs de variante, modificateurs de
/// teint, étiquettes des drapeaux régionaux, voyelles et finales du hangûl décomposé.
fn se_rattache(c: char) -> bool {
    let u = c as u32;
    // écritures indiennes (U+0900 à U+0D7F) : même disposition dans chaque bloc de 128
    if (0x0900..0x0D80).contains(&u) {
        let o = u & 0x7F;
        return o <= 0x03 || ((0x3A..=0x4F).contains(&o) && o != 0x3D) || (0x51..=0x57).contains(&o) || (0x62..=0x63).contains(&o);
    }
    matches!(u,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2 | 0x05C4..=0x05C5 | 0x05C7
        | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7..=0x06E8 | 0x06EA..=0x06ED
        | 0x0711 | 0x0730..=0x074A | 0x07A6..=0x07B0 | 0x07EB..=0x07F3 | 0x0816..=0x082D | 0x0859..=0x085B | 0x0898..=0x089F
        | 0x08CA..=0x08E1 | 0x08E3..=0x08FF
        | 0x0D81..=0x0D83 | 0x0DCA..=0x0DDF | 0x0DF2..=0x0DF3
        | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E | 0x0EB1 | 0x0EB4..=0x0EBC | 0x0EC8..=0x0ECE
        | 0x0F18..=0x0F19 | 0x0F35 | 0x0F37 | 0x0F39 | 0x0F3E..=0x0F3F | 0x0F71..=0x0F84 | 0x0F86..=0x0F87 | 0x0F8D..=0x0FBC
        | 0x102B..=0x103E | 0x1056..=0x1059 | 0x105E..=0x1060 | 0x1062..=0x1064 | 0x1067..=0x106D | 0x1071..=0x1074
        | 0x1082..=0x108D | 0x108F | 0x109A..=0x109D
        | 0x1160..=0x11FF | 0x17B4..=0x17D3 | 0x17DD
        | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x200C..=0x200D | 0x20D0..=0x20FF | 0x302A..=0x302F | 0x3099..=0x309A
        | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F
        | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F | 0xE0100..=0xE01EF)
}

fn indicateur_regional(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

/// Débuts des groupes de caractères visibles de `s` (en octets), puis `s.len()` : les seuls
/// endroits où le couper. Un caractère se joint au précédent s'il s'y rattache ou suit un ZWJ ;
/// les indicateurs régionaux (drapeaux) vont par deux.
fn limites(s: &str) -> Vec<usize> {
    let mut v = Vec::new();
    let (mut prec, mut regionaux) = (None::<char>, 0usize);
    for (i, c) in s.char_indices() {
        let joint = prec.is_some() && (se_rattache(c) || prec == Some('\u{200D}') || (indicateur_regional(c) && regionaux % 2 == 1));
        if !joint {
            v.push(i);
        }
        regionaux = if indicateur_regional(c) { regionaux + 1 } else { 0 };
        prec = Some(c);
    }
    v.push(s.len());
    v
}

/// Le texte s'écrit de droite à gauche : son premier caractère fort (lettre) est hébreu, arabe,
/// syriaque, thâna, n'ko… Les chiffres, signes et emojis ne comptent pas.
fn droite_a_gauche(s: &str) -> bool {
    for c in s.chars() {
        if c.is_numeric() || !c.is_alphabetic() {
            continue;
        }
        let u = c as u32;
        return matches!(u, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF);
    }
    false
}

/// Début de `s` gardé pour la mesure : `CARACTERES_MAX` caractères au plus, coupé entre deux
/// groupes ; vrai s'il a fallu couper.
fn debut_mesurable(s: &str) -> (&str, bool) {
    match s.char_indices().nth(CARACTERES_MAX) {
        None => (s, false),
        Some((fin, _)) => {
            let fin = limites(s).into_iter().take_while(|&b| b <= fin).last().unwrap_or(0);
            (&s[..fin], true)
        }
    }
}

/// Où couper `s` pour que, suivi de « … » (`points` px), il tienne dans `max` px : `cumul[i]` est
/// la largeur des `i + 1` premiers caractères UTF-16 (`GetTextExtentExPointW`). `None` : `s` tient
/// en entier (et `forcee` est faux). Toujours entre deux groupes visibles, espaces de fin ôtés.
fn coupure(s: &str, cumul: &[i32], max: i32, points: i32, forcee: bool) -> Option<usize> {
    if !forcee && cumul.last().is_none_or(|&l| l <= max) {
        return None;
    }
    let largeur = |octet: usize| {
        let n = s[..octet].encode_utf16().count();
        if n == 0 {
            0
        } else {
            cumul.get(n - 1).copied().unwrap_or(i32::MAX)
        }
    };
    let fin = limites(s).into_iter().filter(|&b| b < s.len() || forcee).take_while(|&b| largeur(b) + points <= max).last().unwrap_or(0);
    Some(s[..fin].trim_end().len())
}

/// Ligne prête à écrire : texte UTF-16, coupé au besoin pour tenir dans `max` px avec la police
/// choisie dans `dc`, sa largeur, et son sens (droite à gauche).
///
/// # Safety
/// `dc` est un contexte de périphérique valide.
unsafe fn ajuster(dc: HDC, s: &str, max: i32) -> (Vec<u16>, i32, bool) {
    let rtl = droite_a_gauche(s);
    let (s, forcee) = debut_mesurable(s);
    let mesure = |t: &[u16]| {
        let mut taille = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, t, &mut taille);
        taille.cx
    };
    let u: Vec<u16> = s.encode_utf16().collect();
    let mut cumul = vec![0i32; u.len()];
    let mut taille = SIZE::default();
    let _ = GetTextExtentExPointW(dc, PCWSTR(u.as_ptr()), u.len() as i32, 0, None, Some(cumul.as_mut_ptr()), &mut taille);
    let points: Vec<u16> = POINTS.encode_utf16().collect();
    let Some(mut fin) = coupure(s, &cumul, max, mesure(&points), forcee) else {
        let l = mesure(&u);
        return (u, l, rtl);
    };
    // la mise en forme (ligatures, écritures liées) peut élargir un peu : un groupe de moins
    loop {
        let texte: Vec<u16> = format!("{}{POINTS}", &s[..fin]).encode_utf16().collect();
        let l = mesure(&texte);
        if l <= max || fin == 0 {
            return (texte, l.min(max), rtl);
        }
        fin = limites(&s[..fin]).into_iter().rev().nth(1).unwrap_or(0);
        fin = s[..fin].trim_end().len();
    }
}

/// Texte écrit : son image (RGBA prémultiplié), un numéro qui change à chaque écriture (la carte
/// graphique garde l'image tant qu'il ne change pas), le filet sous la première ligne (début et
/// largeur dans l'image, sens) et la marge autour des lettres (px).
struct Texte {
    image: Pixmap,
    id: u64,
    filet: (f32, f32),
    rtl: bool,
    marge: f32,
}

/// Écrit `titre` (taille `taille` px, gras) et dessous `artiste` (plus petit, plus pâle, s'il y en
/// a un) ; sans titre, l'artiste seul, à la place du titre. Chaque ligne tient dans `largeur_max`
/// px, alignée à droite si elle s'écrit de droite à gauche. Blanc cerné de sombre. `None` si les
/// deux sont vides ou si Windows refuse.
fn ecrire(titre: &str, artiste: &str, taille: f32, largeur_max: f32) -> Option<Texte> {
    static NUMERO: AtomicU64 = AtomicU64::new(1);
    let (titre, artiste) = (titre.trim(), artiste.trim());
    let lignes: Vec<(&str, f32, i32, f32)> = if titre.is_empty() {
        vec![(artiste, taille, GRAISSE.0, 1.0)]
    } else {
        vec![(titre, taille, GRAISSE.0, 1.0), (artiste, taille * ARTISTE.0, GRAISSE.1, ARTISTE.1)]
    }
    .into_iter()
    .filter(|l| !l.0.is_empty())
    .collect();
    if lignes.is_empty() {
        return None;
    }
    let max = largeur_max.round().max(1.0) as i32;
    let marge = CERNE + 1;
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let police = |px: f32, graisse: i32| -> HFONT {
            CreateFontW(-(px.round().max(6.0) as i32), 0, 0, 0, graisse, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, ANTIALIASED_QUALITY, 0, w!("Segoe UI"))
        };
        // chaque ligne coupée et mesurée : (police, texte, largeur, hauteur, droite à gauche)
        let mesures: Vec<(HFONT, Vec<u16>, i32, i32, bool)> = lignes
            .iter()
            .map(|(texte, px, graisse, _)| {
                let f = police(*px, *graisse);
                let ancien = SelectObject(dc, f.into());
                let (u, largeur, rtl) = ajuster(dc, texte, max);
                let mut s = SIZE::default();
                let _ = GetTextExtentPoint32W(dc, &u, &mut s);
                SelectObject(dc, ancien);
                (f, u, largeur.clamp(1, max), s.cy.max(1), rtl)
            })
            .collect();
        // largeur bornée par `max` : le cerne, calculé plus bas pixel par pixel, l'est aussi
        let largeur = mesures.iter().map(|m| m.2).max().unwrap_or(1);
        let (l, h) = (largeur + 2 * marge, mesures.iter().map(|m| m.3).sum::<i32>() + 2 * marge);
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
                for ((_, _, _, opacite), (f, texte, large, haut, rtl)) in lignes.iter().zip(&mesures) {
                    let avant = SelectObject(dc, (*f).into());
                    // de droite à gauche : sens de lecture (« … » à gauche), ligne alignée à droite
                    let (x, sens) = if *rtl { (marge + largeur - large, ETO_RTLREADING) } else { (marge, ETO_OPTIONS(0)) };
                    let _ = ExtTextOutW(dc, x, y, sens, None, PCWSTR(texte.as_ptr()), texte.len() as u32, None);
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
        let (premiere, rtl) = (mesures[0].2, mesures[0].4);
        let filet = (if rtl { marge + largeur - premiere } else { marge } as f32, premiere as f32);
        Some(Texte { image, id: NUMERO.fetch_add(1, Ordering::Relaxed), filet, rtl, marge: marge as f32 })
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
    /// Côté du pointeur où le texte est posé (à gauche, au-dessus), choisi au début de
    /// l'affichage et gardé tant qu'il tient (`placer`) ; `None` avant.
    cote: Option<(bool, bool)>,
}

/// Le texte, passé à gauche (ou au-dessus) du pointeur faute de place, ne revient à droite (ou
/// dessous) que s'il y tient avec cette marge en plus (px à 100 %) : un pointeur qui hésite au
/// bord ne le fait pas sauter d'un côté à l'autre.
const HYSTERESE: f32 = 48.0;

impl Etiquette {
    pub fn new() -> Self {
        Self { pistes: None, kicks: None, debut: None, texte: None, echelle: 1.0, kick: 0.0, coin: (0.0, 0.0), cote: None }
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
            (self.debut, self.texte, self.cote) = (Some(t), None, None);
        }
        self.pistes = Some(pistes);
        if self.debut.is_some_and(|d| (t - d) as f32 >= c.duree + SORTIE) {
            (self.debut, self.texte, self.cote) = (None, None, None);
        }
        if self.debut.is_some() && (self.texte.is_none() || self.echelle != scale) {
            let morceau = crate::media::morceau().unwrap_or_default();
            self.ecrire(&morceau.titre, &morceau.artiste, c.taille, scale);
        }
        self.kick = if self.kicks.is_some_and(|k| k != au.kicks) { 1.0 } else { self.kick * (-dt * 7.0).exp() };
        self.kicks = Some(au.kicks);
        self.placer(cursor, scale, ecran);
    }

    /// Écrit le morceau à montrer ; sans titre ni artiste, rien n'est montré.
    fn ecrire(&mut self, titre: &str, artiste: &str, taille: f32, scale: f32) {
        (self.texte, self.echelle) = (ecrire(titre, artiste, taille * scale, LARGEUR_MAX * scale), scale);
        if self.texte.is_none() {
            self.debut = None;
        }
    }

    /// Place le texte près du pointeur, du côté où il tient dans l'écran : à droite et dessous
    /// par défaut, à gauche ou au-dessus faute de place. Le côté choisi est gardé tant qu'il
    /// tient ; il ne revient à droite ou dessous qu'avec `HYSTERESE` de marge.
    fn placer(&mut self, cursor: (f32, f32), scale: f32, ecran: Option<IRect>) {
        let (dx, dy) = (DECALAGE.0 * scale, DECALAGE.1 * scale);
        self.coin = (cursor.0 + dx, cursor.1 + dy);
        if let (Some(e), Some(tx)) = (ecran, &self.texte) {
            let (l, h) = (tx.image.width() as f32, tx.image.height() as f32 + (FILET_ECART + FILET.0 + FILET.1) * scale);
            // déjà de l'autre côté : il faut la marge en plus pour revenir
            let (avant_g, avant_d) = self.cote.unwrap_or((false, false));
            let marge = |deja: bool| if deja { HYSTERESE * scale } else { 0.0 };
            let gauche = self.coin.0 + l + marge(avant_g) > e.2 as f32;
            let dessus = self.coin.1 + h + marge(avant_d) > e.3 as f32;
            if gauche {
                self.coin.0 = (cursor.0 - dx - l).max(e.0 as f32);
            }
            if dessus {
                self.coin.1 = (cursor.1 - dy - h).max(e.1 as f32);
            }
            self.cote = Some((gauche, dessus));
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
        // filet sous le texte : il se déploie à l'entrée dans le sens de lecture, et bat avec le kick
        let epais = (FILET.0 + FILET.1 * self.kick) * scale;
        let yf = y + tx.image.height() as f32 - tx.marge + FILET_ECART * scale + epais / 2.0;
        let (debut, long) = (x + tx.filet.0, tx.filet.1 * entree);
        let x0 = if tx.rtl { debut + tx.filet.1 - long } else { debut };
        cv.line((x0, yf), (x0 + long, yf), epais, prisme(cfg, t, 1.0), a, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ECRAN: IRect = (0, 0, 800, 600);

    /// Coupe `s` à `max` « px », chaque unité UTF-16 comptant 10 px, « … » 10 px.
    fn coupe_fictive(s: &str, max: i32) -> String {
        let cumul: Vec<i32> = (1..=s.encode_utf16().count() as i32).map(|n| n * 10).collect();
        match coupure(s, &cumul, max, 10, false) {
            None => s.to_string(),
            Some(fin) => format!("{}{POINTS}", &s[..fin]),
        }
    }

    #[test]
    fn groupes_de_caracteres_visibles() {
        // famille (emojis liés par ZWJ), drapeau, teint, lettre et marques combinantes, variante
        let famille = "👨\u{200D}👩\u{200D}👧";
        let s = format!("a{famille}🇫🇷🇩🇪👍🏽e\u{301}\u{323}❤\u{FE0F}क्षि");
        let l = limites(&s);
        let groupes: Vec<&str> = l.windows(2).map(|w| &s[w[0]..w[1]]).collect();
        assert_eq!(groupes, ["a", famille, "🇫🇷", "🇩🇪", "👍🏽", "e\u{301}\u{323}", "❤\u{FE0F}", "क्", "षि"]);
        // trois indicateurs régionaux : une paire, puis un seul
        assert_eq!(limites("🇫🇷🇩").len(), 3);
        assert_eq!(limites(""), [0]);
    }

    #[test]
    fn coupure_entre_deux_groupes() {
        // tient : intact
        assert_eq!(coupe_fictive("abc", 30), "abc");
        // trop long : coupé avec « … », dans la largeur
        assert_eq!(coupe_fictive("abcdef", 40), "abc…");
        // jamais au milieu d'un emoji composé (8 unités UTF-16), d'un drapeau, d'un combinant
        let famille = "👨\u{200D}👩\u{200D}👧";
        for max in 10..=200 {
            for s in [format!("ab{famille}cd"), "a🇫🇷🇩🇪b".to_string(), "ae\u{301}\u{301}e\u{301}bc".to_string(), format!("{famille}{famille}")] {
                let c = coupe_fictive(&s, max);
                let garde = c.strip_suffix(POINTS).unwrap_or(&c);
                assert!(limites(&s).contains(&garde.len()), "« {s} » coupé en « {c} » ({max})");
                if c != s {
                    assert!(garde.encode_utf16().count() as i32 * 10 + 10 <= max || garde.is_empty(), "« {c} » dépasse {max}");
                }
            }
        }
        // espaces de fin ôtés avant « … »
        assert_eq!(coupe_fictive("ab cdefgh", 40), "ab…");
        assert_eq!(coupe_fictive("ab cdefgh", 50), "ab c…");
        // titre démesuré : début gardé entre deux groupes, « … » obligatoire même s'il tient
        let long = "é".repeat(CARACTERES_MAX + 10);
        let (debut, coupe) = debut_mesurable(&long);
        assert!(coupe && debut.chars().count() == CARACTERES_MAX);
        let drapeaux = "🇫🇷".repeat(CARACTERES_MAX);
        let (debut, _) = debut_mesurable(&drapeaux);
        assert!(limites(&drapeaux).contains(&debut.len()) && debut.chars().count() % 2 == 0);
        let cumul = vec![1; 4];
        assert_eq!(coupure("abcd", &cumul, 100, 1, true), Some(4));
    }

    #[test]
    fn sens_de_lecture() {
        assert!(droite_a_gauche("שלום"));
        assert!(droite_a_gauche("مرحبا بالعالم"));
        assert!(droite_a_gauche("2024 — «سلام» feat. X"));
        assert!(!droite_a_gauche("Hello سلام"));
        assert!(!droite_a_gauche("👍 123 !"));
        assert!(!droite_a_gauche("Ünïcödé"));
    }

    #[test]
    fn ecriture_de_droite_a_gauche_et_largeur_bornee() {
        // de droite à gauche : écrit, filet sous la première ligne, aligné à droite
        let tx = ecrire("أغنية طويلة جدا", "فنان", 12.0, 480.0).expect("texte écrit");
        assert!(tx.rtl);
        assert!((tx.filet.0 + tx.filet.1 - (tx.image.width() as f32 - tx.marge)).abs() < 0.5, "filet {:?}, largeur {}", tx.filet, tx.image.width());
        // titre interminable, dans toutes les écritures : jamais plus large que la largeur maximale
        let marges = 2 * (CERNE + 1) as u32;
        for titre in ["W".repeat(300), "ש".repeat(300), "👨\u{200D}👩\u{200D}👧".repeat(200), "e\u{301}".repeat(400), "🇫🇷".repeat(300)] {
            for max in [480.0, 120.0, 30.0] {
                let tx = ecrire(&titre, "Artiste", 16.0, max).expect("texte écrit");
                assert!(tx.image.width() <= max as u32 + marges, "{} px pour {max}", tx.image.width());
            }
        }
    }

    #[test]
    fn le_texte_est_blanc_cerne_de_sombre() {
        let tx = ecrire("Titre du morceau", "Artiste", 12.0, LARGEUR_MAX).expect("texte écrit");
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
        assert!(!tx.rtl && tx.filet.0 == tx.marge && tx.filet.1 > 40.0 && tx.filet.1 <= l as f32);
        let long = ecrire("Titre du morceau, version longue", "Artiste", 12.0, LARGEUR_MAX).unwrap();
        assert!(long.image.width() > l && long.id != tx.id);
        // sans artiste : une seule ligne ; sans titre : l'artiste seul, en titre ; sans rien : rien
        assert!(ecrire("Titre", "", 12.0, LARGEUR_MAX).unwrap().image.height() < h);
        let artiste = ecrire("  ", "Artiste", 12.0, LARGEUR_MAX).expect("artiste seul montré");
        assert!(artiste.image.height() < h && artiste.filet.1 > 20.0);
        assert!(ecrire("  ", " ", 12.0, LARGEUR_MAX).is_none());
        // un titre interminable est coupé à la largeur maximale
        let coupe = ecrire(&"a".repeat(400), "", 12.0, LARGEUR_MAX).unwrap();
        assert!(coupe.image.width() <= LARGEUR_MAX as u32 + 2 * (CERNE + 1) as u32, "{} px", coupe.image.width());
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
        // le pointeur recule un peu : le texte reste à gauche et au-dessus (pas de saut), puis
        // revient à droite et dessous quand il y tient largement
        let l = e.texte.as_ref().unwrap().image.width() as f32;
        let juste = 800.0 - DECALAGE.0 - l - 4.0;
        e.placer((juste, 590.0 - 40.0), 1.0, Some(ECRAN));
        assert!(e.coin.0 < juste, "le texte a sauté à droite : {:?}", e.coin);
        e.placer((juste - HYSTERESE - 4.0, 300.0), 1.0, Some(ECRAN));
        assert_eq!(e.coin, ((juste - HYSTERESE - 4.0 + DECALAGE.0).round(), 300.0 + DECALAGE.1));
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

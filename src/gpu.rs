//! Moteur carte graphique : un calque DirectComposition par écran (fenêtre immobile de la taille
//! de l'écran, sans surface GDI), contenu = une surface virtuelle dessinée par Direct2D. Seule
//! la zone qui change est redessinée (`BeginDraw` avec rectangle), directement sur la carte
//! graphique : ni dessin des pixels par le processeur, ni recopie vers le compositeur.
//!
//! Tout est dessiné par un **lot de sprites** (`ID2D1SpriteBatch`) : une pastille ronde lissée,
//! préparée une fois, recopiée des milliers de fois en un seul appel. Un trait devient une rangée
//! de pastilles serrées. Mesuré : tracer chaque trait avec `DrawLine` coûtait plus de processeur
//! que le moteur tiny-skia (Direct2D découpe chaque trait lissé en triangles sur le processeur),
//! le lot de sprites ne coûte presque rien.
//!
//! Le lot de sprites n'est pas lissé par Direct2D (il exige le mode crénelé) : le lissage vient
//! de l'image de la pastille. Elle est prérendue à plusieurs tailles (64, 32, 16, 8 et 4 px, une
//! « planche ») et chaque pastille prend celle qui lui va (`sourceRectangles`) : une petite
//! pastille tirée de l'image de 64 px ne serait lue qu'en quelques points et clignoterait selon
//! sa position. Aucune pastille n'a moins de `R_MIN_PX` de rayon : une plus petite est agrandie
//! et rendue plus transparente d'autant (même quantité de lumière).
//!
//! Perte du périphérique (pilote réinitialisé, sortie de veille) : `backend.rs` refait un `Gpu`
//! et lui rattache les mêmes fenêtres (`Gpu::rattacher`), sans en créer ni en détruire sous la
//! souris ; il ne repasse au moteur processeur (`layer.rs`) que si cela échoue.

use crate::canvas::{Canvas, Rgb};
use crate::layer::{union, IRect};
use std::time::{Duration, Instant};
use windows::core::{Interface, HRESULT};
use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, E_FAIL, HMODULE, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_RECT_U, D2D_SIZE_U};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION};
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM};
use windows::Win32::Graphics::Dxgi::{IDXGIDevice, DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_DRIVER_INTERNAL_ERROR};
use windows_numerics::{Matrix3x2, Vector2};

/// L'erreur dit-elle que le périphérique est perdu (à refaire entièrement) ?
pub fn perte(e: &windows::core::Error) -> bool {
    const PERTES: [HRESULT; 5] = [D2DERR_RECREATE_TARGET, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DRIVER_INTERNAL_ERROR];
    PERTES.contains(&e.code())
}

pub struct Gpu {
    d3d: ID3D11Device,
    dcomp: IDCompositionDesktopDevice,
    /// Pastilles blanches lissées (prémultipliées) à plusieurs tailles, teintées sprite par sprite.
    planche: ID2D1Bitmap1,
}

impl Gpu {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let mut d3d = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let d3d: ID3D11Device = d3d.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
            let dxgi: IDXGIDevice = d3d.cast()?;
            let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d = factory.CreateDevice(&dxgi)?;
            // créé depuis le périphérique Direct2D : BeginDraw rend directement un contexte Direct2D
            let dcomp: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d)?;
            let dc = d2d.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let planche = planche_pastilles(&dc)?;
            Ok(Self { d3d, dcomp, planche })
        }
    }

    /// Le périphérique Direct3D a-t-il été perdu (pilote réinitialisé, carte retirée) ?
    pub fn perdu(&self) -> bool {
        unsafe { self.d3d.GetDeviceRemovedReason() }.is_err()
    }

    /// Calque de l'écran `rect` (coordonnées écran). La fenêtre est créée cachée et montrée après
    /// la première remise réussie au compositeur (`GpuLayer::montrer`).
    pub fn layer(&self, mon: isize, rect: IRect) -> windows::core::Result<GpuLayer> {
        let hwnd = crate::win::overlay_window(rect, true)?;
        match self.composition(hwnd, rect) {
            Ok(comp) => Ok(GpuLayer { mon, rect, hwnd, montre: false, comp: Some(comp), prev: None, empty_since: None }),
            Err(e) => {
                // encore cachée : la détruire ne fait rien apparaître ni disparaître
                crate::win::destroy_window(hwnd);
                Err(e)
            }
        }
    }

    /// Rattache le calque `l` (sa fenêtre, déjà affichée ou non) à ce périphérique : nouvelle cible
    /// DirectComposition sur la même fenêtre, nouvelle surface vide. L'ancienne composition doit
    /// avoir été lâchée avec son périphérique (`GpuLayer::detacher`, puis le `Gpu` perdu libéré) :
    /// une fenêtre n'a qu'une cible.
    pub fn rattacher(&self, l: &mut GpuLayer) -> windows::core::Result<()> {
        l.comp = None;
        l.comp = Some(self.composition(l.hwnd, l.rect)?);
        // surface neuve, transparente : rien à effacer
        l.prev = None;
        Ok(())
    }

    fn composition(&self, hwnd: HWND, rect: IRect) -> windows::core::Result<Composition> {
        unsafe {
            let target = self.dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = self.dcomp.CreateVisual()?;
            let (w, h) = ((rect.2 - rect.0).max(1) as u32, (rect.3 - rect.1).max(1) as u32);
            let surface = self.dcomp.CreateVirtualSurface(w, h, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
            visual.SetContent(&surface)?;
            target.SetRoot(&visual)?;
            Ok(Composition { _target: target, _visual: visual, surface, pinceau: Pinceau::default() })
        }
    }

    /// Rend visibles d'un coup toutes les modifications de l'image.
    pub fn commit(&self) -> windows::core::Result<()> {
        unsafe { self.dcomp.Commit() }
    }
}

/// Objets DirectComposition d'un calque (et pinceau, lié au même périphérique) : refaits sur un
/// nouveau périphérique sans toucher à la fenêtre.
struct Composition {
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual2,
    surface: IDCompositionVirtualSurface,
    pinceau: Pinceau,
}

pub struct GpuLayer {
    pub mon: isize,
    pub rect: IRect,
    hwnd: HWND,
    /// Fenêtre déjà montrée (après la première remise réussie).
    montre: bool,
    comp: Option<Composition>,
    /// Zone affichée sur la surface (à effacer à l'image suivante) ; mise à jour seulement après
    /// un dessin réussi.
    prev: Option<IRect>,
    pub empty_since: Option<Instant>,
}

impl GpuLayer {
    /// Même contrat que `layer::Layer::frame` : efface la zone précédente et `dirty`, laisse
    /// `draw` dessiner (coordonnées écran), renvoie le temps de remise au compositeur.
    pub fn frame(&mut self, gpu: &Gpu, dirty: Option<IRect>, draw: impl FnOnce(&mut dyn Canvas)) -> windows::core::Result<Duration> {
        let zone = union(self.prev, dirty);
        self.empty_since = match (dirty, self.empty_since) {
            (Some(_), _) => None,
            (None, None) => Some(Instant::now()),
            (None, since) => since,
        };
        let (ox, oy) = (self.rect.0, self.rect.1);
        let (w, h) = (self.rect.2 - ox, self.rect.3 - oy);
        let local = zone.map(|z| ((z.0 - ox).max(0), (z.1 - oy).max(0), (z.2 - ox).min(w), (z.3 - oy).min(h)));
        let Some(local) = local.filter(|l| l.2 > l.0 && l.3 > l.1) else {
            self.prev = dirty;
            return Ok(Duration::ZERO);
        };
        let comp = self.comp.as_mut().ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
        unsafe {
            let r = RECT { left: local.0, top: local.1, right: local.2, bottom: local.3 };
            let mut off = POINT::default();
            let dc: ID2D1DeviceContext = comp.surface.BeginDraw(Some(&r), &mut off)?;
            // écran → surface : la zone demandée commence en `off` dans la texture rendue
            let tx = (off.x - local.0 - ox) as f32;
            let ty = (off.y - local.1 - oy) as f32;
            dc.SetTransform(&Matrix3x2::translation(tx, ty));
            let clip = D2D_RECT_F {
                left: (local.0 + ox) as f32,
                top: (local.1 + oy) as f32,
                right: (local.2 + ox) as f32,
                bottom: (local.3 + oy) as f32,
            };
            dc.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
            // le contenu de la zone rendue n'est pas conservé : tout est effacé puis redessiné
            dc.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let peint = comp.pinceau.peindre(&dc, &gpu.planche, draw);
            // découpe et dessin toujours refermés, même après une erreur
            dc.PopAxisAlignedClip();
            let t0 = Instant::now();
            let fin = comp.surface.EndDraw();
            peint?;
            fin?;
            self.prev = dirty;
            Ok(t0.elapsed())
        }
    }

    /// Efface ce qui est affiché (pause, curseur masqué). À suivre d'un `commit`.
    pub fn clear(&mut self, gpu: &Gpu) -> windows::core::Result<()> {
        // sans zone à dessiner, `frame` efface la précédente et n'en garde aucune
        self.frame(gpu, None, |_| {}).map(|_| ())
    }

    /// Montre la fenêtre si elle ne l'est pas encore (après une remise réussie au compositeur).
    pub fn montrer(&mut self) {
        if !self.montre {
            crate::win::show_window(self.hwnd);
            self.montre = true;
        }
    }

    /// Lâche les objets DirectComposition (périphérique perdu) ; la fenêtre reste.
    pub fn detacher(&mut self) {
        self.comp = None;
    }
}

impl Drop for GpuLayer {
    fn drop(&mut self) {
        crate::win::destroy_window(self.hwnd);
    }
}

/// Pastilles d'une image et lot de sprites qui les dessine, gardés d'une image à l'autre : un
/// par calque, et un pour la fenêtre pour le stream (`stream.rs`). Toutes ses ressources sont
/// liées au périphérique Direct2D : un nouveau périphérique demande un nouveau pinceau.
#[derive(Default)]
pub struct Pinceau {
    sprites: SpriteCanvas,
    /// Lot de sprites, créé une fois (ressource du périphérique Direct2D) puis vidé à chaque image.
    batch: Option<ID2D1SpriteBatch>,
    /// Vignette téléversée (son numéro, son image Direct2D), gardée tant que le numéro ne change pas.
    vignette: Option<(u64, ID2D1Bitmap1)>,
    /// Brosse du voile, gardée tant que sa couleur et la forme de son dégradé ne changent pas.
    voile: Option<(CleVoile, ID2D1RadialGradientBrush)>,
}

impl Pinceau {
    /// Laisse `draw` poser ses pastilles et ses voiles, puis les dessine sur `dc` (entre
    /// `BeginDraw` et `EndDraw`, avec la transformation et la découpe en place). `planche` vient
    /// de `planche_pastilles` sur le même périphérique.
    pub unsafe fn peindre(&mut self, dc: &ID2D1DeviceContext, planche: &ID2D1Bitmap1, draw: impl FnOnce(&mut dyn Canvas)) -> windows::core::Result<()> {
        let sprites = &mut self.sprites;
        sprites.vider();
        // pixels par unité de dessin (la fenêtre pour le stream agrandit l'essaim)
        let mut m = Matrix3x2::default();
        unsafe { dc.GetTransform(&mut m) };
        let echelle = (m.M11 * m.M22 - m.M12 * m.M21).abs().sqrt();
        sprites.echelle = if echelle.is_finite() && echelle > 1e-4 { echelle } else { 1.0 };
        draw(sprites);
        unsafe {
            // voiles (projecteur) d'abord : tout le reste est dessiné par-dessus
            for v in &sprites.voiles {
                voile(dc, v, &mut self.voile)?;
            }
            if !sprites.rects.is_empty() {
                let dc3: ID2D1DeviceContext3 = dc.cast()?;
                let batch = match &self.batch {
                    Some(b) => {
                        b.Clear();
                        b.clone()
                    }
                    None => {
                        let b = dc3.CreateSpriteBatch()?;
                        self.batch = Some(b.clone());
                        b
                    }
                };
                batch.AddSprites(
                    sprites.rects.len() as u32,
                    sprites.rects.as_ptr(),
                    Some(sprites.sources.as_ptr()),
                    Some(sprites.colors.as_ptr()),
                    None,
                    std::mem::size_of::<D2D_RECT_F>() as u32,
                    std::mem::size_of::<D2D_RECT_U>() as u32,
                    std::mem::size_of::<D2D1_COLOR_F>() as u32,
                    0,
                )?;
                dc3.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED); // exigé par DrawSpriteBatch
                dc3.DrawSpriteBatch(&batch, 0, sprites.rects.len() as u32, planche, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_SPRITE_OPTIONS_NONE);
            }
            // vignette (texte) par-dessus : téléversée quand elle change, puis recopiée telle quelle
            let v = &sprites.vignette;
            if let Some((x, y, alpha)) = v.place {
                if self.vignette.as_ref().map(|g| g.0) != Some(v.id) {
                    let image = dc.CreateBitmap(D2D_SIZE_U { width: v.l, height: v.h }, Some(v.pixels.as_ptr() as *const _), v.l * 4, &proprietes_image())?;
                    self.vignette = Some((v.id, image));
                }
                if let Some((_, image)) = &self.vignette {
                    let zone = D2D_RECT_F { left: x, top: y, right: x + v.l as f32, bottom: y + v.h as f32 };
                    let cible: &ID2D1RenderTarget = dc;
                    cible.DrawBitmap(image, Some(&zone), alpha, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, None);
                }
            }
        }
        Ok(())
    }
}

/// Image Direct2D en BGRA prémultiplié, à l'échelle de l'écran.
fn proprietes_image() -> D2D1_BITMAP_PROPERTIES1 {
    D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
        ..Default::default()
    }
}

/// Pastilles de la planche : (côté de l'image, abscisse dans la planche), de la plus grande à la
/// plus petite, séparées de 2 px vides. Le disque de chacune a un rayon de côté / 2 − 1 (un pixel
/// transparent autour : la lecture bilinéaire ne déborde pas sur la voisine).
const NIVEAUX: [(u32, u32); 5] = [(64, 0), (32, 66), (16, 100), (8, 118), (4, 128)];
const PLANCHE: (u32, u32) = (132, 64);
/// Rayon minimal d'une pastille, en pixels de l'écran.
const R_MIN_PX: f32 = 1.0;
/// Pastilles au plus par trait ou anneau : un trait démesuré (coordonnées aberrantes) garde sa
/// longueur, ses pastilles plus espacées, au lieu de remplir la mémoire.
const MAX_PAR_TRAIT: usize = 20_000;
/// Garde-fou de mémoire sur l'image entière (~24 Mo de pastilles) : très au-dessus d'une image
/// réelle (un halo sur une fenêtre agrandie en 4K en pose ~33 000), il ne coupe jamais l'essaim,
/// dessiné après l'habillage.
const MAX_PASTILLES: usize = 500_000;

/// Rapport entre le demi-côté de l'image d'une pastille et le rayon de son disque.
fn marge(cote: u32) -> f32 {
    (cote as f32 / 2.0) / (cote as f32 / 2.0 - 1.0)
}

/// Planche des pastilles blanches lissées, prémultipliées, en image Direct2D.
pub unsafe fn planche_pastilles(dc: &ID2D1DeviceContext) -> windows::core::Result<ID2D1Bitmap1> {
    let mut pm = tiny_skia::Pixmap::new(PLANCHE.0, PLANCHE.1).expect("taille valide");
    let blanc = crate::canvas::paint((255, 255, 255), 1.0);
    for (cote, x0) in NIVEAUX {
        let c = cote as f32 / 2.0;
        crate::canvas::circle(&mut pm, x0 as f32 + c, c, c - 1.0, &blanc);
    }
    // RGBA → BGRA (le blanc prémultiplié a les trois mêmes composantes)
    let data: Vec<u8> = pm.data().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    unsafe { dc.CreateBitmap(D2D_SIZE_U { width: PLANCHE.0, height: PLANCHE.1 }, Some(data.as_ptr() as *const _), PLANCHE.0 * 4, &proprietes_image()) }
}

/// Pastille de la planche pour un disque de `r_px` pixels de rayon : la plus grande lue au plus
/// deux fois plus finement que l'écran (au-delà, la lecture bilinéaire saute des pixels de
/// l'image), sinon la plus petite.
fn niveau(r_px: f32) -> (u32, u32) {
    NIVEAUX.into_iter().find(|&(cote, _)| cote as f32 <= 4.0 * r_px * marge(cote)).unwrap_or(NIVEAUX[NIVEAUX.len() - 1])
}

/// Voile percé d'un disque clair (voir `Canvas::voile`), dessiné en dégradé radial Direct2D.
struct Voile {
    zone: D2D_RECT_F,
    x: f32,
    y: f32,
    r: f32,
    flou: f32,
    rgb: Rgb,
    alpha: f32,
}

/// Ce qui fixe les arrêts du dégradé d'un voile : couleur, opacité (sur 255) et bord du disque
/// clair rapporté au rayon extérieur (sur 1024). Centre et rayon se changent sur la brosse.
#[derive(Clone, Copy, PartialEq)]
struct CleVoile {
    rgb: Rgb,
    alpha: u8,
    bord: u16,
}

/// Remplit la zone du voile d'un dégradé radial : transparent dans le disque, la couleur au-delà
/// du bord adouci (le dégradé se prolonge par sa dernière couleur). Un seul rectangle, rempli
/// par la carte graphique ; la brosse est gardée dans `garde` d'une image à l'autre.
unsafe fn voile(dc: &ID2D1DeviceContext, v: &Voile, garde: &mut Option<(CleVoile, ID2D1RadialGradientBrush)>) -> windows::core::Result<()> {
    let r = v.r.max(0.0);
    let ext = r + v.flou.max(1.0);
    if !(ext.is_finite() && v.x.is_finite() && v.y.is_finite()) {
        return Ok(());
    }
    let cle = CleVoile { rgb: v.rgb, alpha: (v.alpha.clamp(0.0, 1.0) * 255.0).round() as u8, bord: ((r / ext).clamp(0.0, 1.0) * 1024.0).round() as u16 };
    if cle.alpha == 0 {
        return Ok(());
    }
    let rt: &ID2D1RenderTarget = dc;
    unsafe {
        let brosse = match garde {
            Some((c, b)) if *c == cle => {
                b.SetCenter(Vector2 { X: v.x, Y: v.y });
                b.SetRadiusX(ext);
                b.SetRadiusY(ext);
                b.clone()
            }
            _ => {
                let c = |k: f32| color(cle.rgb, cle.alpha as f32 / 255.0 * k);
                let bord = cle.bord as f32 / 1024.0;
                let stops = [
                    D2D1_GRADIENT_STOP { position: 0.0, color: c(0.0) },
                    D2D1_GRADIENT_STOP { position: bord, color: c(0.0) },
                    D2D1_GRADIENT_STOP { position: bord + (1.0 - bord) * 0.5, color: c(0.4) },
                    D2D1_GRADIENT_STOP { position: 1.0, color: c(1.0) },
                ];
                let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
                let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                    center: Vector2 { X: v.x, Y: v.y },
                    gradientOriginOffset: Vector2 { X: 0.0, Y: 0.0 },
                    radiusX: ext,
                    radiusY: ext,
                };
                let b = rt.CreateRadialGradientBrush(&props, None, &coll)?;
                *garde = Some((cle, b.clone()));
                b
            }
        };
        rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED);
        rt.FillRectangle(&v.zone, &brosse);
    }
    Ok(())
}

/// Vignette à recopier (voir `Canvas::vignette`) : ses pixels en BGRA prémultiplié, gardés tant
/// que son numéro ne change pas, et où la poser à cette image (x, y, opacité).
#[derive(Default)]
struct Vignette {
    id: u64,
    pixels: Vec<u8>,
    l: u32,
    h: u32,
    place: Option<(f32, f32, f32)>,
}

/// Accumule des pastilles (rectangle écran, pastille de la planche, teinte) ; dessinées en un
/// seul lot à la fin.
struct SpriteCanvas {
    rects: Vec<D2D_RECT_F>,
    sources: Vec<D2D_RECT_U>,
    colors: Vec<D2D1_COLOR_F>,
    voiles: Vec<Voile>,
    vignette: Vignette,
    /// Pixels de l'écran par unité de dessin.
    echelle: f32,
}

impl Default for SpriteCanvas {
    fn default() -> Self {
        Self { rects: Vec::new(), sources: Vec::new(), colors: Vec::new(), voiles: Vec::new(), vignette: Vignette::default(), echelle: 1.0 }
    }
}

fn fini(p: (f32, f32)) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

/// Opacité de chaque pastille pour qu'un point couvert `cover` fois ait l'opacité `alpha`.
fn opacite_recouverte(alpha: f32, cover: f32) -> f32 {
    1.0 - (1.0 - alpha.clamp(0.0, 0.999)).powf(1.0 / cover)
}

/// Vide `v` pour l'image suivante ; si sa place dépasse de loin ce que l'image d'avant a pris
/// (une image pathologique, un halo démesuré), elle est rendue : la mémoire ne reste pas au
/// niveau d'une pointe pour toute la session.
fn vider<T>(v: &mut Vec<T>) {
    let n = v.len();
    v.clear();
    if v.capacity() > RENDRE_AU_DELA && v.capacity() > 4 * n {
        v.shrink_to(n);
    }
}

/// Place (en éléments) en deçà de laquelle un `Vec` des pastilles n'est jamais rendu.
const RENDRE_AU_DELA: usize = 100_000;

impl SpriteCanvas {
    /// Vide les pastilles et les voiles de l'image précédente (`vider`).
    fn vider(&mut self) {
        vider(&mut self.rects);
        vider(&mut self.sources);
        vider(&mut self.colors);
        vider(&mut self.voiles);
        self.vignette.place = None;
    }

    /// Rayon minimal d'une pastille, en unités de dessin.
    fn r_min(&self) -> f32 {
        R_MIN_PX / self.echelle
    }

    /// Disque de rayon `r` : plus petit que `r_min`, il est agrandi et son opacité réduite
    /// d'autant (même quantité de lumière, sans clignoter selon sa position).
    fn dot(&mut self, x: f32, y: f32, r: f32, mut c: D2D1_COLOR_F) {
        if !(r.is_finite() && r > 0.0) {
            return;
        }
        let r_min = self.r_min();
        let r = if r < r_min {
            c.a *= (r / r_min).powi(2);
            r_min
        } else {
            r
        };
        self.poser(x, y, r, c);
    }

    /// Pose une pastille de rayon `r` (au moins `r_min`), tirée de la planche à la bonne taille.
    fn poser(&mut self, x: f32, y: f32, r: f32, c: D2D1_COLOR_F) {
        if !(fini((x, y)) && r.is_finite()) || c.a <= 0.0 || self.rects.len() >= MAX_PASTILLES {
            return;
        }
        let (cote, x0) = niveau(r * self.echelle);
        // le disque occupe 1 / marge de l'image : on élargit pour qu'il ait le rayon r
        let d = r * marge(cote);
        self.rects.push(D2D_RECT_F { left: x - d, top: y - d, right: x + d, bottom: y + d });
        self.sources.push(D2D_RECT_U { left: x0, top: 0, right: x0 + cote, bottom: cote });
        self.colors.push(c);
    }

    /// Largeur d'un trait : un trait de largeur nulle est un filet d'un pixel (comme tiny-skia).
    fn largeur(&self, width: f32) -> f32 {
        if width > 0.0 {
            width
        } else {
            1.0 / self.echelle
        }
    }

    /// Rangée de pastilles de diamètre `width` de a à b (au moins `2 r_min`, l'opacité réduite
    /// d'autant). Les pastilles se chevauchent (un point est couvert deux fois) : leur opacité est
    /// réduite d'autant pour garder celle du trait.
    fn stroke(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32, with_start: bool) {
        if !(fini(a) && fini(b) && width.is_finite()) {
            return;
        }
        let w = self.largeur(width);
        let d = w.max(2.0 * self.r_min());
        let step = d * 0.5;
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let n = (dx.hypot(dy) / step).ceil().clamp(1.0, MAX_PAR_TRAIT as f32) as usize;
        let c = color(rgb, opacite_recouverte(alpha * w / d, d / step));
        for i in usize::from(!with_start)..=n {
            let k = i as f32 / n as f32;
            self.poser(a.0 + dx * k, a.1 + dy * k, d * 0.5, c);
        }
    }
}

fn color(rgb: Rgb, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: rgb.0 as f32 / 255.0, g: rgb.1 as f32 / 255.0, b: rgb.2 as f32 / 255.0, a: alpha.clamp(0.0, 1.0) }
}

impl Canvas for SpriteCanvas {
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32, _smooth: bool) {
        self.stroke(a, b, width, rgb, alpha, true);
    }

    fn polyline(&mut self, pts: &[(f32, f32)], width: f32, rgb: Rgb, alpha: f32, _smooth: bool) {
        for (i, w) in pts.windows(2).enumerate() {
            // le premier point d'un segment est le dernier du précédent : pas de doublon
            self.stroke(w[0], w[1], width, rgb, alpha, i == 0);
        }
    }

    fn circle(&mut self, x: f32, y: f32, r: f32, rgb: Rgb, alpha: f32) {
        self.dot(x, y, r, color(rgb, alpha));
    }

    fn ring(&mut self, x: f32, y: f32, r: f32, width: f32, rgb: Rgb, alpha: f32) {
        if !(fini((x, y)) && r.is_finite() && width.is_finite()) || r <= 0.0 {
            return;
        }
        let w = self.largeur(width);
        let d = w.max(2.0 * self.r_min());
        if 2.0 * r <= d {
            // trop petit ou trop large pour un anneau de pastilles : un disque de même lumière,
            // aussi grand que le trait de tiny-skia (jusqu'à r + w/2, creux sous r − w/2)
            let e = r + w * 0.5;
            let a = alpha * (1.0 - ((r - w * 0.5).max(0.0) / e).powi(2));
            self.dot(x, y, e, color(rgb, a));
            return;
        }
        let step = d * 0.5;
        let n = (std::f32::consts::TAU * r / step).ceil().clamp(8.0, MAX_PAR_TRAIT as f32) as usize;
        let c = color(rgb, opacite_recouverte(alpha * w / d, d / step));
        for i in 0..n {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            self.poser(x + t.cos() * r, y + t.sin() * r, d * 0.5, c);
        }
    }

    fn voile(&mut self, zone: (f32, f32, f32, f32), x: f32, y: f32, r: f32, flou: f32, rgb: Rgb, alpha: f32) {
        if alpha > 0.0 && zone.2 > zone.0 && zone.3 > zone.1 {
            let zone = D2D_RECT_F { left: zone.0, top: zone.1, right: zone.2, bottom: zone.3 };
            self.voiles.push(Voile { zone, x, y, r, flou, rgb, alpha: alpha.min(1.0) });
        }
    }

    fn vignette(&mut self, img: &tiny_skia::Pixmap, id: u64, x: f32, y: f32, alpha: f32) {
        let v = &mut self.vignette;
        if v.id != id || v.pixels.is_empty() {
            v.pixels.clear();
            v.pixels.extend(img.data().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]));
            (v.id, v.l, v.h) = (id, img.width(), img.height());
        }
        v.place = Some((x.round(), y.round(), alpha.clamp(0.0, 1.0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn petites_pastilles_agrandies_a_lumiere_egale() {
        let mut s = SpriteCanvas::default();
        s.circle(10.0, 10.0, 0.25, (255, 255, 255), 0.8);
        assert_eq!(s.rects.len(), 1);
        let r = &s.rects[0];
        let (cote, _) = niveau(R_MIN_PX);
        assert!(((r.right - r.left) / 2.0 - R_MIN_PX * marge(cote)).abs() < 1e-4, "rayon minimal");
        assert!((s.colors[0].a - 0.8 * 0.0625).abs() < 1e-5, "opacité × (r / r_min)²");
        // agrandi deux fois (fenêtre pour le stream) : le rayon minimal est de 1 px de l'écran
        s.echelle = 2.0;
        s.circle(0.0, 0.0, 0.25, (255, 255, 255), 1.0);
        assert!((s.colors[1].a - 0.25).abs() < 1e-5);
    }

    #[test]
    fn la_planche_suit_la_taille() {
        assert_eq!(niveau(31.0).0, 64);
        assert_eq!(niveau(1000.0).0, 64);
        assert_eq!(niveau(8.0).0, 32);
        assert_eq!(niveau(2.0).0, 8);
        assert_eq!(niveau(1.0).0, 4);
        // les pastilles ne se touchent pas dans la planche
        for w in NIVEAUX.windows(2) {
            assert!(w[0].1 + w[0].0 + 2 <= w[1].1);
        }
        let (cote, x0) = NIVEAUX[NIVEAUX.len() - 1];
        assert!(x0 + cote <= PLANCHE.0 && NIVEAUX[0].0 <= PLANCHE.1);
    }

    #[test]
    fn coordonnees_aberrantes_ignorees_et_pastilles_plafonnees() {
        let mut s = SpriteCanvas::default();
        s.circle(f32::NAN, 0.0, 3.0, (255, 0, 0), 1.0);
        s.circle(0.0, f32::INFINITY, 3.0, (255, 0, 0), 1.0);
        s.line((0.0, 0.0), (f32::INFINITY, 0.0), 2.0, (255, 0, 0), 1.0, true);
        s.ring(0.0, 0.0, f32::INFINITY, 2.0, (255, 0, 0), 1.0);
        assert!(s.rects.is_empty());
        // un trait ou un anneau démesuré : borné à lui seul, ses pastilles espacées sur sa longueur
        s.line((0.0, 0.0), (1.0e9, 0.0), 2.0, (255, 0, 0), 1.0, true);
        assert_eq!(s.rects.len(), MAX_PAR_TRAIT + 1);
        assert!(s.rects.last().unwrap().right > 0.99e9, "le trait va jusqu'au bout");
        s.ring(0.0, 0.0, 1.0e9, 2.0, (255, 0, 0), 1.0);
        assert_eq!(s.rects.len(), 2 * MAX_PAR_TRAIT + 1);
        assert_eq!(s.sources.len(), s.rects.len());
        assert_eq!(s.colors.len(), s.rects.len());
        // garde-fou de mémoire sur l'image entière
        for _ in 0..MAX_PASTILLES / MAX_PAR_TRAIT + 1 {
            s.line((0.0, 0.0), (1.0e9, 0.0), 2.0, (255, 0, 0), 1.0, true);
        }
        assert_eq!(s.rects.len(), MAX_PASTILLES);
        // image suivante ordinaire : la place de la pointe reste le temps d'une image, puis est rendue
        s.vider();
        assert!(s.rects.capacity() >= MAX_PASTILLES, "image d'avant pleine : rien n'est rendu");
        s.circle(0.0, 0.0, 3.0, (255, 0, 0), 1.0);
        s.vider();
        assert!(s.rects.capacity() < RENDRE_AU_DELA && s.colors.capacity() < RENDRE_AU_DELA, "{}", s.rects.capacity());
        // une place modeste n'est jamais rendue (pas de réallocation à chaque image)
        let mut v: Vec<u8> = Vec::with_capacity(RENDRE_AU_DELA);
        vider(&mut v);
        assert_eq!(v.capacity(), RENDRE_AU_DELA);
    }

    #[test]
    fn un_grand_halo_ne_coupe_pas_l_essaim() {
        // halo sur une fenêtre agrandie en 4K (12 000 px de tour) : un filet au bord et quatre
        // nappes, soit bien plus de 20 000 pastilles ; l'essaim, dessiné après, est là
        let mut s = SpriteCanvas::default();
        let coins = [(0.0, 0.0), (3840.0, 0.0), (3840.0, 2160.0), (0.0, 2160.0), (0.0, 0.0)];
        for n in 0..5 {
            let r = n as f32 * 12.0;
            let pts: Vec<(f32, f32)> = coins.iter().map(|&(x, y)| (x + if x > 0.0 { -r } else { r }, y + if y > 0.0 { -r } else { r })).collect();
            for w in pts.windows(2) {
                s.bande(w[0], w[1], 1.0, (255, 255, 255), 0.5);
            }
        }
        assert!(s.rects.len() > 50_000, "{} pastilles", s.rects.len());
        s.circle(1920.0, 1080.0, 3.0, (255, 0, 0), 1.0);
        let p = s.rects.last().unwrap();
        assert_eq!(((p.left + p.right) / 2.0, (p.top + p.bottom) / 2.0), (1920.0, 1080.0));
        assert_eq!(s.colors.last().unwrap().g, 0.0, "le point d'essaim est dessiné");
    }

    #[test]
    fn anneau_borne_comme_un_trait() {
        let mut s = SpriteCanvas::default();
        // pastilles de la largeur du trait, posées tous les demi-diamètres
        s.ring(0.0, 0.0, 10.0, 4.0, (255, 255, 255), 0.5);
        let n = s.rects.len();
        assert_eq!(n, (std::f32::consts::TAU * 10.0 / 2.0).ceil() as usize);
        let (cote, _) = niveau(2.0);
        assert!(s.rects.iter().all(|r| ((r.right - r.left) - 4.0 * marge(cote)).abs() < 1e-3));
        // au moins aussi large que son diamètre : un seul disque, jusqu'à r + w/2 comme tiny-skia
        s.ring(0.0, 0.0, 10.0, 50.0, (255, 255, 255), 0.5);
        assert_eq!(s.rects.len(), n + 1);
        let (cote, _) = niveau(35.0);
        assert!(((s.rects[n].right - s.rects[n].left) - 70.0 * marge(cote)).abs() < 1e-3);
        assert!((s.colors[n].a - 0.5).abs() < 1e-5, "plein : r − w/2 < 0");
        // minuscule : un seul disque
        s.ring(0.0, 0.0, 0.3, 0.2, (255, 255, 255), 0.5);
        assert_eq!(s.rects.len(), n + 2);
    }
}

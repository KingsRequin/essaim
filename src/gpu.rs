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
//! En cas d'échec (pas de carte graphique utilisable, pilote réinitialisé), `main.rs` repasse
//! au moteur processeur (`layer.rs`).

use crate::canvas::{Canvas, Rgb};
use crate::layer::{union, IRect};
use std::time::{Duration, Instant};
use windows::core::Interface;
use windows::Win32::Foundation::{HMODULE, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION};
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows_numerics::Matrix3x2;

pub struct Gpu {
    _d3d: ID3D11Device,
    dcomp: IDCompositionDesktopDevice,
    /// Pastille blanche lissée (prémultipliée), teintée sprite par sprite.
    disc: ID2D1Bitmap1,
}

/// Côté de l'image de la pastille ; le disque y a un rayon de `DISC_R`.
const DISC: u32 = 64;
const DISC_R: f32 = 31.0;

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
            let d3d: ID3D11Device = d3d.ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
            let dxgi: IDXGIDevice = d3d.cast()?;
            let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d = factory.CreateDevice(&dxgi)?;
            // créé depuis le périphérique Direct2D : BeginDraw rend directement un contexte Direct2D
            let dcomp: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d)?;
            let dc = d2d.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let disc = make_disc(&dc)?;
            Ok(Self { _d3d: d3d, dcomp, disc })
        }
    }

    /// Calque de l'écran `rect` (coordonnées écran).
    pub fn layer(&self, mon: isize, rect: IRect) -> windows::core::Result<GpuLayer> {
        unsafe {
            let hwnd = crate::win::overlay_window(rect, true)?;
            let make = || -> windows::core::Result<_> {
                let target = self.dcomp.CreateTargetForHwnd(hwnd, true)?;
                let visual = self.dcomp.CreateVisual()?;
                let (w, h) = ((rect.2 - rect.0).max(1) as u32, (rect.3 - rect.1).max(1) as u32);
                let surface = self.dcomp.CreateVirtualSurface(w, h, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
                visual.SetContent(&surface)?;
                target.SetRoot(&visual)?;
                Ok((target, visual, surface))
            };
            match make() {
                Ok((target, visual, surface)) => {
                    Ok(GpuLayer {
                        mon,
                        rect,
                        hwnd,
                        _target: target,
                        _visual: visual,
                        surface,
                        prev: None,
                        empty_since: None,
                        sprites: SpriteCanvas::default(),
                        batch: None,
                    })
                }
                Err(e) => {
                    crate::win::destroy_window(hwnd);
                    Err(e)
                }
            }
        }
    }

    /// Rend visibles d'un coup toutes les modifications de l'image.
    pub fn commit(&self) -> windows::core::Result<()> {
        unsafe { self.dcomp.Commit() }
    }
}

pub struct GpuLayer {
    pub mon: isize,
    pub rect: IRect,
    hwnd: HWND,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual2,
    surface: IDCompositionVirtualSurface,
    prev: Option<IRect>,
    pub empty_since: Option<Instant>,
    /// Pastilles de l'image, tampons gardés d'une image à l'autre.
    sprites: SpriteCanvas,
    /// Lot de sprites, créé une fois (ressource du périphérique Direct2D) puis vidé à chaque image.
    batch: Option<ID2D1SpriteBatch>,
}

impl GpuLayer {
    /// Même contrat que `layer::Layer::frame` : efface la zone précédente et `dirty`, laisse
    /// `draw` dessiner (coordonnées écran), renvoie le temps de remise au compositeur.
    pub fn frame(&mut self, gpu: &Gpu, dirty: Option<IRect>, draw: impl FnOnce(&mut dyn Canvas)) -> windows::core::Result<Duration> {
        let zone = union(self.prev, dirty);
        self.prev = dirty;
        self.empty_since = match (dirty, self.empty_since) {
            (Some(_), _) => None,
            (None, None) => Some(Instant::now()),
            (None, since) => since,
        };
        let Some(z) = zone else { return Ok(Duration::ZERO) };
        let (ox, oy) = (self.rect.0, self.rect.1);
        let (w, h) = (self.rect.2 - ox, self.rect.3 - oy);
        let local = ((z.0 - ox).max(0), (z.1 - oy).max(0), (z.2 - ox).min(w), (z.3 - oy).min(h));
        if local.2 <= local.0 || local.3 <= local.1 {
            return Ok(Duration::ZERO);
        }
        unsafe {
            let r = RECT { left: local.0, top: local.1, right: local.2, bottom: local.3 };
            let mut off = POINT::default();
            let dc: ID2D1DeviceContext = self.surface.BeginDraw(Some(&r), &mut off)?;
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
            let sprites = &mut self.sprites;
            sprites.rects.clear();
            sprites.colors.clear();
            draw(sprites);
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
                    None,
                    Some(sprites.colors.as_ptr()),
                    None,
                    std::mem::size_of::<D2D_RECT_F>() as u32,
                    0,
                    std::mem::size_of::<D2D1_COLOR_F>() as u32,
                    0,
                )?;
                dc3.SetAntialiasMode(D2D1_ANTIALIAS_MODE_ALIASED); // exigé par DrawSpriteBatch
                dc3.DrawSpriteBatch(
                    &batch,
                    0,
                    sprites.rects.len() as u32,
                    &gpu.disc,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    D2D1_SPRITE_OPTIONS_NONE,
                );
            }
            dc.PopAxisAlignedClip();
            let t0 = Instant::now();
            self.surface.EndDraw()?;
            Ok(t0.elapsed())
        }
    }

    /// Efface ce qui est affiché (pause, curseur masqué). À suivre d'un `commit`.
    pub fn clear(&mut self, gpu: &Gpu) -> windows::core::Result<()> {
        self.frame(gpu, None, |_| {})?;
        self.prev = None;
        Ok(())
    }
}

impl Drop for GpuLayer {
    fn drop(&mut self) {
        crate::win::destroy_window(self.hwnd);
    }
}

/// Pastille blanche lissée, prémultipliée, en image Direct2D.
unsafe fn make_disc(dc: &ID2D1DeviceContext) -> windows::core::Result<ID2D1Bitmap1> {
    let mut pm = tiny_skia::Pixmap::new(DISC, DISC).expect("taille valide");
    let c = DISC as f32 / 2.0;
    crate::canvas::circle(&mut pm, c, c, DISC_R, &crate::canvas::paint((255, 255, 255), 1.0));
    // RGBA → BGRA (le blanc prémultiplié a les trois mêmes composantes)
    let data: Vec<u8> = pm.data().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    let props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
        ..Default::default()
    };
    unsafe { dc.CreateBitmap(D2D_SIZE_U { width: DISC, height: DISC }, Some(data.as_ptr() as *const _), DISC * 4, &props) }
}

/// Accumule des pastilles (rectangle écran + teinte) ; dessinées en un seul lot à la fin.
#[derive(Default)]
struct SpriteCanvas {
    rects: Vec<D2D_RECT_F>,
    colors: Vec<D2D1_COLOR_F>,
}

impl SpriteCanvas {
    fn dot(&mut self, x: f32, y: f32, r: f32, c: D2D1_COLOR_F) {
        // le disque occupe DISC_R / (DISC / 2) de l'image : on élargit pour qu'il ait le rayon r
        let r = r * (DISC as f32 / 2.0) / DISC_R;
        self.rects.push(D2D_RECT_F { left: x - r, top: y - r, right: x + r, bottom: y + r });
        self.colors.push(c);
    }

    /// Rangée de pastilles de diamètre `width` de a à b. Les pastilles se chevauchent (un point
    /// est couvert ~deux fois) : leur opacité est réduite d'autant pour garder celle du trait.
    fn stroke(&mut self, a: (f32, f32), b: (f32, f32), width: f32, rgb: Rgb, alpha: f32, with_start: bool) {
        let w = width.max(0.8);
        let step = (w * 0.5).max(0.6);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let n = ((dx * dx + dy * dy).sqrt() / step).ceil().max(1.0) as usize;
        let cover = w / step;
        let a_dot = 1.0 - (1.0 - alpha.clamp(0.0, 0.999)).powf(1.0 / cover);
        let c = color(rgb, a_dot);
        for i in usize::from(!with_start)..=n {
            let k = i as f32 / n as f32;
            self.dot(a.0 + dx * k, a.1 + dy * k, w * 0.5, c);
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
        let step = (width * 0.5).max(0.6);
        let n = (std::f32::consts::TAU * r / step).ceil().max(8.0) as usize;
        let a_dot = 1.0 - (1.0 - alpha.clamp(0.0, 0.999)).powf(step / width.max(0.8));
        let c = color(rgb, a_dot);
        for i in 0..n {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            self.dot(x + t.cos() * r, y + t.sin() * r, width * 0.5, c);
        }
    }
}

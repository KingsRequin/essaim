//! Contours d'une image, pour en faire le trajet des comètes : une image choisie ou glissée dans
//! les réglages, un texte ou un emoji dessinés par la page, l'icône de l'application au premier
//! plan (`[orbite] forme = "icone"`).
//!
//! 1. Masque de la forme : la transparence quand l'image en a (logo en PNG), sinon la luminosité
//!    séparée par le seuil d'Otsu (ou celui demandé), du côté qui n'est pas le fond (le fond est
//!    ce que montrent la plupart des bords de l'image) ; « inverser » prend l'autre côté.
//! 2. Filtre majoritaire 3 × 3, deux fois : les pixels isolés et les bords en escalier s'en vont.
//! 3. Régions d'un seul tenant (8 voisins) ; les plus petites (moins de 0,2 % de l'image) sont
//!    écartées ; les plus grandes d'abord.
//! 4. Bord extérieur de chaque région, suivi pixel à pixel (voisinage de Moore, arrêt de Jacob).
//! 5. Simplifié (Ramer-Douglas-Peucker, coupé en deux aux points les plus éloignés : un contour
//!    fermé n'a pas d'extrémités), puis adouci (Chaikin, deux fois) : `POINTS_MAX` points au plus.
//!
//! Le moteur rééchantillonne ensuite chaque contour à pas égaux et les met tous à la même
//! échelle (`render::set_traces`).

/// Contours gardés au plus (un par anneau de comètes), et points par contour.
pub const CONTOURS_MAX: usize = 4;
pub const POINTS_MAX: usize = 512;

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Seuil de luminosité (ou d'opacité, pour une image transparente) ; `None` : calculé.
    pub seuil: Option<u8>,
    /// La forme est de l'autre côté du seuil.
    pub inverser: bool,
    /// Tous les contours (jusqu'à `CONTOURS_MAX`), sinon le plus grand seulement.
    pub tous: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resultat {
    /// Contours, en pixels de l'image (y vers le bas), les plus grands d'abord.
    pub contours: Vec<Vec<[f32; 2]>>,
    /// Seuil employé (calculé ou demandé), et s'il porte sur la transparence.
    pub seuil: u8,
    pub transparence: bool,
}

/// Contours de l'image `rgba` (`l` × `h` pixels, 4 octets par pixel : rouge, vert, bleu, opacité).
pub fn contours(rgba: &[u8], l: usize, h: usize, o: &Options) -> Resultat {
    if l < 3 || h < 3 || rgba.len() < l * h * 4 {
        return Resultat::default();
    }
    let (masque, seuil, transparence) = masque(rgba, l, h, o);
    let masque = majoritaire(&majoritaire(&masque, l, h), l, h);
    let mut regions = regions(&masque, l, h);
    let aire_min = 12.max(l * h / 500);
    regions.retain(|r| r.len() >= aire_min);
    regions.sort_by_key(|r| std::cmp::Reverse(r.len()));
    regions.truncate(if o.tous { CONTOURS_MAX } else { 1 });
    let contours = regions
        .iter()
        .filter_map(|r| {
            let bord = bord(r, l, h);
            (bord.len() >= 4).then(|| adoucir(&simplifier(&bord)))
        })
        .collect();
    Resultat { contours, seuil, transparence }
}

/// Masque de la forme, seuil employé, et s'il porte sur la transparence.
fn masque(rgba: &[u8], l: usize, h: usize, o: &Options) -> (Vec<bool>, u8, bool) {
    let n = l * h;
    let opaques = rgba.chunks_exact(4).take(n).filter(|p| p[3] >= 128).count();
    // transparence utile : ni (presque) tout opaque, ni (presque) tout transparent
    if opaques > n / 50 && opaques < n - n / 50 {
        let s = o.seuil.unwrap_or(128).max(1);
        let m: Vec<bool> = rgba.chunks_exact(4).take(n).map(|p| (p[3] >= s) != o.inverser).collect();
        return (m, s, true);
    }
    // luminosité, sur fond blanc pour les pixels un peu transparents
    let lum: Vec<u8> = rgba
        .chunks_exact(4)
        .take(n)
        .map(|p| {
            let a = p[3] as f32 / 255.0;
            let v = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
            (v * a + 255.0 * (1.0 - a)).round().clamp(0.0, 255.0) as u8
        })
        .collect();
    let s = o.seuil.unwrap_or_else(|| otsu(&lum));
    // le fond : le côté du seuil que montrent la plupart des bords
    let bords = (0..l).flat_map(|x| [x, (h - 1) * l + x]).chain((0..h).flat_map(|y| [y * l, y * l + l - 1]));
    let (mut sombres, mut total) = (0usize, 0usize);
    for i in bords {
        total += 1;
        sombres += (lum[i] < s) as usize;
    }
    let fond_sombre = sombres * 2 > total;
    let m = lum.iter().map(|&v| ((v < s) != fond_sombre) != o.inverser).collect();
    (m, s, false)
}

/// Seuil d'Otsu : celui qui sépare le mieux les deux classes de luminosité (variance entre les
/// classes la plus grande).
fn otsu(lum: &[u8]) -> u8 {
    let mut hist = [0u64; 256];
    for &v in lum {
        hist[v as usize] += 1;
    }
    let total = lum.len() as f64;
    let somme: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut poids, mut somme_bas, mut meilleur, mut seuil) = (0.0, 0.0, -1.0, 128u8);
    for (t, &c) in hist.iter().enumerate() {
        poids += c as f64;
        if poids == 0.0 || poids == total {
            continue;
        }
        somme_bas += t as f64 * c as f64;
        let (m0, m1) = (somme_bas / poids, (somme - somme_bas) / (total - poids));
        let v = poids * (total - poids) * (m0 - m1) * (m0 - m1);
        if v > meilleur {
            meilleur = v;
            // le seuil sépare « jusqu'à t » de « au-dessus de t »
            seuil = (t + 1).min(255) as u8;
        }
    }
    seuil
}

/// Filtre majoritaire 3 × 3 (hors de l'image : vide).
fn majoritaire(m: &[bool], l: usize, h: usize) -> Vec<bool> {
    let mut sortie = vec![false; l * h];
    for y in 0..h {
        for x in 0..l {
            let mut n = 0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                    if xx >= 0 && yy >= 0 && (xx as usize) < l && (yy as usize) < h && m[yy as usize * l + xx as usize] {
                        n += 1;
                    }
                }
            }
            sortie[y * l + x] = n >= 5;
        }
    }
    sortie
}

/// Régions d'un seul tenant (8 voisins) : les pixels de chacune, dans l'ordre de l'image (le
/// premier est le plus haut, puis le plus à gauche).
fn regions(m: &[bool], l: usize, h: usize) -> Vec<Vec<usize>> {
    let mut vu = vec![false; l * h];
    let mut regions = Vec::new();
    for depart in 0..l * h {
        if !m[depart] || vu[depart] {
            continue;
        }
        let mut region = Vec::new();
        let mut pile = vec![depart];
        vu[depart] = true;
        while let Some(i) = pile.pop() {
            region.push(i);
            let (x, y) = ((i % l) as i32, (i / l) as i32);
            for (dx, dy) in VOISINS {
                let (xx, yy) = (x + dx, y + dy);
                if xx >= 0 && yy >= 0 && (xx as usize) < l && (yy as usize) < h {
                    let j = yy as usize * l + xx as usize;
                    if m[j] && !vu[j] {
                        vu[j] = true;
                        pile.push(j);
                    }
                }
            }
        }
        region.sort_unstable();
        regions.push(region);
    }
    regions
}

/// Les 8 voisins, dans le sens des aiguilles d'une montre en partant de l'ouest (y vers le bas).
const VOISINS: [(i32, i32); 8] = [(-1, 0), (-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1)];

/// Bord extérieur d'une région (ses pixels, triés), suivi pixel à pixel : voisinage de Moore,
/// arrêt de Jacob (revenu au départ, entré du même côté). Centres des pixels.
fn bord(region: &[usize], l: usize, h: usize) -> Vec<(i32, i32)> {
    let mut dedans = vec![false; l * h];
    for &i in region {
        dedans[i] = true;
    }
    let est = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < l && (y as usize) < h && dedans[y as usize * l + x as usize];
    let depart = ((region[0] % l) as i32, (region[0] / l) as i32);
    // le premier pixel de la région dans l'ordre de l'image : son voisin de l'ouest est dehors
    let (mut p, mut retour) = (depart, 0usize);
    let mut contour = vec![p];
    for _ in 0..4 * region.len() + 8 {
        let suivant = (1..=8).map(|i| (retour + i) % 8).find(|&d| est(p.0 + VOISINS[d].0, p.1 + VOISINS[d].1));
        let Some(d) = suivant else { break };
        let q = (p.0 + VOISINS[d].0, p.1 + VOISINS[d].1);
        // nouveau retour : le voisin examiné juste avant q, vu depuis q
        let avant = (p.0 + VOISINS[(d + 7) % 8].0, p.1 + VOISINS[(d + 7) % 8].1);
        retour = VOISINS.iter().position(|&(dx, dy)| (q.0 + dx, q.1 + dy) == avant).unwrap_or(0);
        p = q;
        if p == depart && retour == 0 {
            break;
        }
        contour.push(p);
    }
    contour
}

/// Ramer-Douglas-Peucker sur un contour fermé : coupé en deux au point le plus éloigné du
/// premier, chaque moitié simplifiée à `EPSILON` près ; plus grossier si trop de points restent.
fn simplifier(bord: &[(i32, i32)]) -> Vec<[f32; 2]> {
    let pts: Vec<[f32; 2]> = bord.iter().map(|&(x, y)| [x as f32 + 0.5, y as f32 + 0.5]).collect();
    let d2 = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    let loin = (0..pts.len()).max_by(|&i, &j| d2(pts[0], pts[i]).total_cmp(&d2(pts[0], pts[j]))).unwrap_or(0);
    let mut epsilon = 0.8f32;
    loop {
        let mut garde = vec![false; pts.len()];
        garde[0] = true;
        garde[loin] = true;
        // les deux moitiés : 0 → loin, loin → 0 (le contour se referme)
        let mut pile = vec![(0usize, loin), (loin, pts.len())];
        while let Some((a, b)) = pile.pop() {
            if b <= a + 1 {
                continue;
            }
            let (pa, pb) = (pts[a], pts[b % pts.len()]);
            let (dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
            let long = (dx * dx + dy * dy).sqrt().max(1e-6);
            let (mut i_max, mut d_max) = (a, 0.0f32);
            for (i, p) in pts.iter().enumerate().take(b).skip(a + 1) {
                let d = ((p[0] - pa[0]) * dy - (p[1] - pa[1]) * dx).abs() / long;
                if d > d_max {
                    (i_max, d_max) = (i, d);
                }
            }
            if d_max > epsilon {
                garde[i_max] = true;
                pile.push((a, i_max));
                pile.push((i_max, b));
            }
        }
        let sortie: Vec<[f32; 2]> = pts.iter().zip(&garde).filter(|(_, &g)| g).map(|(p, _)| *p).collect();
        if sortie.len() * 4 <= POINTS_MAX || epsilon > 64.0 {
            return sortie;
        }
        epsilon *= 1.5;
    }
}

/// Chaikin sur un contour fermé, deux fois : chaque côté donne deux points, au quart et aux
/// trois quarts ; les angles s'arrondissent.
fn adoucir(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut p = pts.to_vec();
    for _ in 0..2 {
        if p.len() < 3 {
            break;
        }
        let n = p.len();
        p = (0..n)
            .flat_map(|i| {
                let (a, b) = (p[i], p[(i + 1) % n]);
                [[0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]], [0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]]
            })
            .collect();
    }
    if p.len() > POINTS_MAX {
        let pas = p.len().div_ceil(POINTS_MAX);
        p = p.into_iter().step_by(pas).collect();
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Image `l` × `h` dessinée par `f(x, y) -> [r, g, b, a]`.
    fn image(l: usize, h: usize, f: impl Fn(f32, f32) -> [u8; 4]) -> Vec<u8> {
        (0..l * h).flat_map(|i| f((i % l) as f32 + 0.5, (i / l) as f32 + 0.5)).collect()
    }

    fn disque(cx: f32, cy: f32, r: f32) -> impl Fn(f32, f32) -> bool {
        move |x, y| (x - cx).powi(2) + (y - cy).powi(2) < r * r
    }

    /// Distance moyenne et extrêmes des points au centre.
    fn rayons(c: &[[f32; 2]], cx: f32, cy: f32) -> (f32, f32, f32) {
        let d: Vec<f32> = c.iter().map(|p| ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt()).collect();
        (d.iter().sum::<f32>() / d.len() as f32, d.iter().cloned().fold(f32::MAX, f32::min), d.iter().cloned().fold(0.0, f32::max))
    }

    #[test]
    fn disque_noir_sur_blanc() {
        let d = disque(50.0, 50.0, 30.0);
        let img = image(100, 100, |x, y| if d(x, y) { [20, 20, 20, 255] } else { [240, 240, 240, 255] });
        let r = contours(&img, 100, 100, &Options::default());
        assert!(!r.transparence && r.seuil > 20 && r.seuil < 240, "seuil {}", r.seuil);
        assert_eq!(r.contours.len(), 1);
        let (moy, min, max) = rayons(&r.contours[0], 50.0, 50.0);
        assert!((moy - 30.0).abs() < 1.5 && min > 27.0 && max < 32.0, "{moy} {min} {max}");
        assert!(r.contours[0].len() >= 16 && r.contours[0].len() <= POINTS_MAX);
        // inverser : le fond devient la forme (le bord de l'image)
        let r = contours(&img, 100, 100, &Options { inverser: true, ..Default::default() });
        let (moy, ..) = rayons(&r.contours[0], 50.0, 50.0);
        assert!(moy > 40.0, "{moy}");
    }

    #[test]
    fn forme_claire_sur_fond_sombre_et_seuil_impose() {
        let d = disque(40.0, 30.0, 15.0);
        let img = image(80, 60, |x, y| if d(x, y) { [230, 200, 30, 255] } else { [10, 10, 40, 255] });
        let r = contours(&img, 80, 60, &Options::default());
        let (moy, ..) = rayons(&r.contours[0], 40.0, 30.0);
        assert!((moy - 15.0).abs() < 1.5, "le fond sombre n'est pas la forme : {moy}");
        // seuil au-dessus de tout : plus de forme
        assert!(contours(&img, 80, 60, &Options { seuil: Some(255), ..Default::default() }).contours.is_empty());
    }

    #[test]
    fn transparence_d_un_logo_et_plusieurs_contours() {
        let (a, b, c) = (disque(30.0, 40.0, 14.0), disque(90.0, 40.0, 10.0), disque(60.0, 90.0, 6.0));
        let img = image(120, 120, |x, y| if a(x, y) || b(x, y) || c(x, y) { [255, 255, 255, 255] } else { [255, 255, 255, 0] });
        let r = contours(&img, 120, 120, &Options { tous: true, ..Default::default() });
        assert!(r.transparence);
        assert_eq!(r.contours.len(), 3, "trois formes");
        // les plus grandes d'abord
        let centre = |c: &[[f32; 2]]| c.iter().fold([0.0, 0.0], |s, p| [s[0] + p[0] / c.len() as f32, s[1] + p[1] / c.len() as f32]);
        assert!((centre(&r.contours[0])[0] - 30.0).abs() < 2.0 && (centre(&r.contours[2])[0] - 60.0).abs() < 2.0);
        // le plus grand seulement
        assert_eq!(contours(&img, 120, 120, &Options::default()).contours.len(), 1);
    }

    #[test]
    fn grains_ecartes_et_images_vides() {
        // un carré et du bruit : un seul contour
        let img = image(100, 100, |x, y| {
            let carre = (20.0..80.0).contains(&x) && (20.0..80.0).contains(&y);
            let grain = (x as usize * 7 + y as usize * 13).is_multiple_of(97);
            if carre || grain { [0, 0, 0, 255] } else { [255, 255, 255, 255] }
        });
        let r = contours(&img, 100, 100, &Options { tous: true, ..Default::default() });
        assert_eq!(r.contours.len(), 1, "{:?}", r.contours.iter().map(Vec::len).collect::<Vec<_>>());
        // image unie, trop petite, tronquée : rien, sans panique
        assert!(contours(&image(50, 50, |_, _| [128, 128, 128, 255]), 50, 50, &Options::default()).contours.is_empty());
        assert!(contours(&[0; 16], 2, 2, &Options::default()).contours.is_empty());
        assert!(contours(&[0; 10], 50, 50, &Options::default()).contours.is_empty());
    }

    /// Contours d'un visage souriant (contour du visage, deux yeux, bouche), pour l'image du README
    /// `trace.gif` : `cargo test --release exporter_contours -- --ignored`.
    #[test]
    #[ignore]
    fn exporter_contours() {
        let sortie = std::env::var("ESSAIM_CONTOURS").expect("ESSAIM_CONTOURS : fichier de sortie");
        let img = image(200, 200, |x, y| {
            let r = ((x - 100.0).powi(2) + (y - 100.0).powi(2)).sqrt();
            let visage = (78.0..92.0).contains(&r);
            let oeil = disque(72.0, 78.0, 11.0)(x, y) || disque(128.0, 78.0, 11.0)(x, y);
            let bouche = (40.0..52.0).contains(&r) && y > 112.0;
            if visage || oeil || bouche { [255, 255, 255, 255] } else { [0, 0, 0, 0] }
        });
        let r = contours(&img, 200, 200, &Options { tous: true, ..Default::default() });
        assert_eq!(r.contours.len(), 4);
        std::fs::write(sortie, serde_json::to_string(&r.contours).unwrap()).unwrap();
    }

    #[test]
    fn otsu_entre_deux_paquets() {
        let lum: Vec<u8> = (0..1000).map(|i| if i % 3 == 0 { 40 } else { 200 }).collect();
        let s = otsu(&lum);
        assert!(s > 40 && s <= 200, "{s}");
    }
}

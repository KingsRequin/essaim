//! Contours d'une image, pour en faire le trajet des comètes : une image choisie ou glissée dans
//! les réglages, un texte ou un emoji dessinés par la page, l'icône de l'application au premier
//! plan (`[orbite] forme = "icone"`).
//!
//! 1. Valeur de « forme » de chaque pixel : l'opacité quand l'image a de la transparence (logo en
//!    PNG, texte), sinon la luminosité séparée par le seuil d'Otsu (ou celui demandé), du côté
//!    qui n'est pas le fond (le fond est ce que montrent la plupart des bords de l'image) ;
//!    « inverser » prend l'autre côté. Un léger flou (noyau 1-2-1) efface le bruit d'une photo et
//!    les escaliers d'une image en gros pixels.
//! 2. Isocontour au seuil, entre les pixels (marching squares) : le bord tombe là où passe le
//!    seuil, au dixième de pixel près : les bords droits restent droits (l'anticrénelage d'une
//!    lettre donne la vraie position de son bord). Chaque bout de contour est
//!    orienté : les bords extérieurs tournent dans un sens, les trous dans l'autre (signe de
//!    l'aire). Les trous sont laissés de côté.
//! 3. Les formes trop petites (moins de 0,025 % de l'image ; le point d'un i reste) sont
//!    écartées ; les plus grandes d'abord, `CONTOURS_MAX` au plus (une lettre chacune, pour un
//!    texte).
//! 4. Simplifié (Ramer-Douglas-Peucker à un quart de pixel, plus grossier s'il reste plus de
//!    `POINTS_MAX` points) : les coins restent des coins.
//!
//! Le moteur rééchantillonne ensuite chaque contour à pas égaux et les met tous à la même
//! échelle (`render::set_traces`).

/// Contours gardés au plus (un par anneau de comètes), et points par contour.
pub const CONTOURS_MAX: usize = 24;
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
    let (valeurs, niveau, seuil, transparence) = forme(rgba, l, h, o);
    let valeurs = flou(&valeurs, l, h);
    let aire_min = 12.0f32.max((l * h) as f32 / 4000.0);
    let mut formes: Vec<(f32, Vec<[f32; 2]>)> = isocontours(&valeurs, l, h, niveau)
        .into_iter()
        .map(|c| (-aire(&c), c))
        // un bord extérieur tourne dans le sens inverse des aiguilles d'une montre (y vers le
        // bas : aire négative), un trou dans l'autre
        .filter(|(a, _)| *a >= aire_min)
        .collect();
    formes.sort_by(|a, b| b.0.total_cmp(&a.0));
    formes.truncate(if o.tous { CONTOURS_MAX } else { 1 });
    let contours = formes.into_iter().map(|(_, c)| simplifier(&c)).filter(|c| c.len() >= 3).collect();
    Resultat { contours, seuil, transparence }
}

/// Valeur de « forme » de chaque pixel (0 à 255) et niveau au-dessus duquel un point est dans la
/// forme ; seuil employé, et s'il porte sur la transparence.
fn forme(rgba: &[u8], l: usize, h: usize, o: &Options) -> (Vec<f32>, f32, u8, bool) {
    let n = l * h;
    let opaques = rgba.chunks_exact(4).take(n).filter(|p| p[3] >= 128).count();
    // transparence utile : ni (presque) tout opaque, ni (presque) tout transparent
    if opaques > n / 50 && opaques < n - n / 50 {
        let s = o.seuil.unwrap_or(128).max(1);
        let valeurs = rgba.chunks_exact(4).take(n).map(|p| p[3] as f32).collect();
        // l'opacité « ≥ s » sur des entiers : le niveau passe entre s − 1 et s
        return inverser(valeurs, s as f32 - 0.5, s, true, o);
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
    if sombres * 2 > total {
        // fond sombre : la forme est claire (luminosité ≥ s)
        inverser(lum.iter().map(|&v| v as f32).collect(), s as f32 - 0.5, s, false, o)
    } else {
        // fond clair : la forme est sombre (luminosité < s, soit 255 − luminosité > 255,5 − s)
        inverser(lum.iter().map(|&v| 255.0 - v as f32).collect(), 255.5 - s as f32, s, false, o)
    }
}

/// « Inverser » : la forme est de l'autre côté du niveau.
fn inverser(valeurs: Vec<f32>, niveau: f32, seuil: u8, transparence: bool, o: &Options) -> (Vec<f32>, f32, u8, bool) {
    if o.inverser {
        (valeurs.into_iter().map(|v| 255.0 - v).collect(), 255.0 - niveau, seuil, transparence)
    } else {
        (valeurs, niveau, seuil, transparence)
    }
}

/// Seuil d'Otsu : la coupure qui sépare le mieux les deux classes de luminosité (variance entre
/// les classes la plus grande), placée au milieu des moyennes des deux classes (le bord adouci
/// d'une forme passe alors à mi-chemin entre elle et le fond).
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
            seuil = ((m0 + m1) / 2.0).ceil().clamp(1.0, 255.0) as u8;
        }
    }
    seuil
}

/// Flou 1-2-1, horizontal puis vertical (au bord : le pixel du bord répété).
fn flou(v: &[f32], l: usize, h: usize) -> Vec<f32> {
    let passe = |v: &[f32], pas: usize, long: usize, lignes: usize, ecart: usize| {
        let mut sortie = vec![0.0; v.len()];
        for ligne in 0..lignes {
            let base = ligne * ecart;
            for i in 0..long {
                let at = |j: usize| v[base + j * pas];
                sortie[base + i * pas] = (at(i.saturating_sub(1)) + 2.0 * at(i) + at((i + 1).min(long - 1))) / 4.0;
            }
        }
        sortie
    };
    let horizontal = passe(v, 1, l, h, l);
    passe(&horizontal, l, h, l, 1)
}

/// Isocontours fermés du champ `v` (`l` × `h`) au `niveau` : marching squares sur les centres des
/// pixels, l'image entourée d'une marge hors de la forme (un contour qui touche le bord se
/// referme). Chaque contour est orienté : bord extérieur d'aire négative, trou d'aire positive
/// (y vers le bas). Points en pixels de l'image.
fn isocontours(v: &[f32], l: usize, h: usize, niveau: f32) -> Vec<Vec<[f32; 2]>> {
    // grille avec une marge d'un pixel, toujours hors de la forme
    let (gl, gh) = (l + 2, h + 2);
    let val = |x: usize, y: usize| if x == 0 || y == 0 || x > l || y > h { f32::NEG_INFINITY } else { v[(y - 1) * l + x - 1] };
    let dedans = |x: usize, y: usize| val(x, y) > niveau;
    // une arête par point de la grille vers la droite (2k) et vers le bas (2k + 1)
    let arete = |x: usize, y: usize, bas: bool| 2 * (y * gl + x) + bas as usize;
    let mut suivant = vec![u32::MAX; 2 * gl * gh];
    let mut position = vec![[0.0f32; 2]; 2 * gl * gh];
    let croise = |x0: usize, y0: usize, x1: usize, y1: usize| -> [f32; 2] {
        let (a, b) = (val(x0, y0), val(x1, y1));
        // un bout dans la marge : le bord tombe au bord du pixel
        let t = if a.is_finite() && b.is_finite() && (b - a).abs() > 1e-6 { ((niveau - a) / (b - a)).clamp(0.0, 1.0) } else { 0.5 };
        // centre du pixel (x, y) de la grille : (x − 0,5 ; y − 0,5) dans l'image
        [x0 as f32 - 0.5 + (x1 as f32 - x0 as f32) * t, y0 as f32 - 0.5 + (y1 as f32 - y0 as f32) * t]
    };
    for y in 0..gh - 1 {
        for x in 0..gl - 1 {
            // coins dans le sens des aiguilles d'une montre (y vers le bas) : haut gauche, haut
            // droit, bas droit, bas gauche ; l'arête i va du coin i au coin i + 1
            let coins = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)];
            let d = coins.map(|(cx, cy)| dedans(cx, cy));
            if d.iter().all(|&b| b) || d.iter().all(|&b| !b) {
                continue;
            }
            let aretes = [arete(x, y, false), arete(x + 1, y, true), arete(x, y + 1, false), arete(x, y, true)];
            // passages, dans l'ordre du tour : (arête, entrée dans la forme ?)
            let mut passages = [(0usize, false); 4];
            let mut n = 0;
            for i in 0..4 {
                let (a, b) = (d[i], d[(i + 1) % 4]);
                if a != b {
                    let ((x0, y0), (x1, y1)) = (coins[i], coins[(i + 1) % 4]);
                    // la même arête est vue de deux cellules : position calculée dans le même sens
                    let p = if (x0, y0) < (x1, y1) { croise(x0, y0, x1, y1) } else { croise(x1, y1, x0, y0) };
                    position[aretes[i]] = p;
                    passages[n] = (aretes[i], b);
                    n += 1;
                }
            }
            // segments de l'entrée vers la sortie : tous les contours sont orientés pareil
            let entree = (0..n).find(|&i| passages[i].1).unwrap_or(0);
            let p = |k: usize| passages[(entree + k) % n].0;
            if n == 2 {
                suivant[p(0)] = p(1) as u32;
            } else {
                // col : le centre de la cellule dit si les deux parties de la forme se touchent
                let centre = coins.iter().map(|&(cx, cy)| val(cx, cy)).sum::<f32>() / 4.0;
                if centre > niveau {
                    suivant[p(0)] = p(3) as u32;
                    suivant[p(2)] = p(1) as u32;
                } else {
                    suivant[p(0)] = p(1) as u32;
                    suivant[p(2)] = p(3) as u32;
                }
            }
        }
    }
    let mut vu = vec![false; suivant.len()];
    let mut sortie = Vec::new();
    for depart in 0..suivant.len() {
        if suivant[depart] == u32::MAX || vu[depart] {
            continue;
        }
        let mut c = Vec::new();
        let mut i = depart;
        while !vu[i] && suivant[i] != u32::MAX {
            vu[i] = true;
            c.push(position[i]);
            i = suivant[i] as usize;
        }
        if c.len() >= 3 {
            sortie.push(c);
        }
    }
    sortie
}

/// Aire orientée d'un contour fermé (formule du lacet).
fn aire(c: &[[f32; 2]]) -> f32 {
    let n = c.len();
    (0..n).map(|i| c[i][0] * c[(i + 1) % n][1] - c[(i + 1) % n][0] * c[i][1]).sum::<f32>() / 2.0
}

/// Ramer-Douglas-Peucker sur un contour fermé : coupé en deux au point le plus éloigné du
/// premier, chaque moitié simplifiée à `epsilon` près ; plus grossier si trop de points restent.
fn simplifier(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let d2 = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    let loin = (0..pts.len()).max_by(|&i, &j| d2(pts[0], pts[i]).total_cmp(&d2(pts[0], pts[j]))).unwrap_or(0);
    let mut epsilon = 0.25f32;
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
        if sortie.len() <= POINTS_MAX || epsilon > 64.0 {
            return sortie;
        }
        epsilon *= 1.5;
    }
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
    fn mot_long_toutes_les_lettres_et_les_points() {
        // « KingsRequin » en blocs : 11 lettres de 30 × 60 et 2 points de 12 × 12, transparent
        let img = image(512, 512, |x, y| {
            let lettre = (0..11).any(|i| {
                let x0 = 20.0 + i as f32 * 44.0;
                (x0..x0 + 30.0).contains(&x) && (230.0..290.0).contains(&y)
            });
            let point = [64.0, 416.0].iter().any(|&x0| (x0..x0 + 12.0).contains(&x) && (205.0..217.0).contains(&y));
            if lettre || point { [0, 0, 0, 255] } else { [0, 0, 0, 0] }
        });
        let r = contours(&img, 512, 512, &Options { tous: true, ..Default::default() });
        assert_eq!(r.contours.len(), 13, "{:?}", r.contours.iter().map(Vec::len).collect::<Vec<_>>());
        // bords droits : un rectangle simplifié garde ses 4 coins, à peu près rien d'autre
        assert!(r.contours[0].len() <= 12, "{}", r.contours[0].len());
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

    /// Banc sur de vraies images : pour chaque `NN.rgba` (et `NN.taille` : « largeur hauteur ») du
    /// dossier `ESSAIM_BANC`, écrit `NN.json` (le plus grand contour) et `NN-tous.json` :
    /// `cargo test --release banc_images -- --ignored`.
    #[test]
    #[ignore]
    fn banc_images() {
        let dossier = std::path::PathBuf::from(std::env::var("ESSAIM_BANC").expect("ESSAIM_BANC : dossier"));
        for f in std::fs::read_dir(&dossier).unwrap().flatten() {
            let p = f.path();
            if p.extension().is_none_or(|e| e != "rgba") {
                continue;
            }
            let taille = std::fs::read_to_string(p.with_extension("taille")).unwrap();
            let [l, h]: [usize; 2] = taille.split_whitespace().map(|v| v.parse().unwrap()).collect::<Vec<_>>().try_into().unwrap();
            let px = std::fs::read(&p).unwrap();
            for (suffixe, tous) in [("", false), ("-tous", true)] {
                let r = contours(&px, l, h, &Options { tous, ..Default::default() });
                let nom = format!("{}{suffixe}.json", p.file_stem().unwrap().to_string_lossy());
                std::fs::write(dossier.join(nom), serde_json::to_string(&r.contours).unwrap()).unwrap();
            }
        }
    }

    #[test]
    fn otsu_entre_deux_paquets() {
        let lum: Vec<u8> = (0..1000).map(|i| if i % 3 == 0 { 40 } else { 200 }).collect();
        let s = otsu(&lum);
        assert!(s > 40 && s <= 200, "{s}");
    }
}

// Aperçu d'un preset : le même calcul que l'essaim autour du pointeur (src/render.rs et
// src/modulation.rs), dessiné dans un canevas. Les événements du son (kick, caisse claire, drop…)
// et de la souris font agir les déclencheurs ; les modulations font varier rotation, vitesse,
// rayon… Le pointeur de l'essaim est au centre ; les petites figures suivent la souris quand elle
// survole le grand aperçu. Toute formule de render.rs (Swarm::update, agir,
// Swarm::draw, draw_trail…) est reprise ici telle quelle : la modifier là oblige à la reporter ici.
"use strict";

const TAU = Math.PI * 2;
const SAT_R_MAX = 1.9;
// Dépassement des satellites en mode ressorts, marge, retard du centre au plus et étendue de la
// fenêtre (px à 100 %) : comme SAT_DEPASSEMENT, MARGIN, CENTER_LAG_MAX et MAX_EXTENT de render.rs.
const SAT_DEPASSEMENT = 1.4, MARGE = 12, RETARD_CENTRE_MAX = 30, ETENDUE_MAX = 2400;
// Point le plus loin du centre de chaque trajet de rayon 1 (comme render::etendue_forme) ; 1,2
// pour des contours (image, icône) et une forme inconnue.
const ETENDUES_FORMES = {
  cercle: 1, hexagone: 1.05, pentagone: 1.1, losange: 1.15, lissajous: 1.19, triangle: 1.2, huit: 1.2, trefle: 1.2,
  coeur: 1.22, etoile: 1.25, fleur: 1.25, carre: 1.25, ellipse: 1.3, astroide: 1.3,
};
const etendueForme = (forme) => ETENDUES_FORMES[forme] || 1.2;
// Points de toutes les traînées au plus, tours de trajet qu'une traînée couvre au plus : comme
// POINTS_TRAINEES et TOURS_TRAINEE.
const POINTS_TRAINEES = 60000, TOURS_TRAINEE = 5;
const SAT_TRAINEE = 0.25;
const PAS_TRAINEE = 1 / 90;
// Finesse des traînées (angle maximal en radians, écart maximal en px entre deux points),
// morceaux de traînée des comètes, particules vivantes au plus : comme render.rs.
const FINESSE_COMETES = [0.045, 4];
const FINESSE_SATELLITES = [0.1, 8];
const TRAIL_CHUNK = 4;
const MAX_PARTICULES = 400;
// Onde d'un changement de piste : taille (px à 100 %) et durée (s), comme ONDE_PISTE.
const ONDE_PISTE = [90, 0.8];
// Caisse claire, drop, constellation : comme CARRE_CAISSE, ONDE_DROP, ETINCELLES_DROP, LIENS_MAX,
// LIENS_PORTEE.
const CARRE_CAISSE = [30, 0.32];
const ONDE_DROP = [130, 0.9];
const ETINCELLES_DROP = 28;
const LIENS_MAX = 160;
const LIENS_PORTEE = 1.1;
// Réticule et anneau de spectre : comme src/effets.rs.
const RETICULE_LUEUR = 2.8;
const SPECTRE_BARRE_MIN = 1.5;
const SPECTRE_MONTEE = 30;
const SPECTRE_DESCENTE = 8;
const BANDES_MAX = 32;
// Modulations : comme src/modulation.rs (effet à fond de chaque cible, retombée des coups :
// kick, caisse claire, charleston, drop, contretemps ; suivi de la montée ; lissage par défaut).
const MODULATION = {
  rotation: 90, vitesse: 40, acceleration: 2, rayon: 4, taille: 1.5, teinte: 180, decalage: 1.6, luminosite: 2, trainee: 3, opacite: 1,
};
// Bornes des totaux, réglage compris (comme modulation::ROTATION_MAX, VITESSE_MAX) : rotation
// (tr/min, de chaque côté), vitesse sur le trajet (radians/s).
const ROTATION_MAX = 180, VITESSE_MAX = 60;
const borneRotation = (tr) => (Number.isFinite(tr) ? Math.min(ROTATION_MAX, Math.max(-ROTATION_MAX, tr)) : 0);
// Ressorts et rubans : comme REBOND, POUSSEE, FREIN, TREMBLE, SAT_RESSORT, SAT_ENTREE_SORTIE,
// SAT_FONTE, SAT_TRAIL_RESSORTS, ETIREMENT, ANNEAU_RESSORTS, VRILLE et RUBAN_MORCEAU de render.rs.
const REBOND = 6.67, REBOND_MAX = 3 * REBOND, POUSSEE = [5, 4], FREIN = [0.8, 9], TREMBLE = [0.053, 46];
const SAT_RESSORT = [10, 16, 0.3], SAT_ENTREE_SORTIE = [0.3, 0.35], SAT_FONTE = 0.6, SAT_TRAIL_RESSORTS = 0.68;
const ETIREMENT = [0.02, 8.7], ANNEAU_RESSORTS = [5, 4.5], VRILLE = [0.58, 2.4, 1.4], RUBAN_MORCEAU = 2;
// Halo blanc de l'éclat du kick : rayon du vide laissé sous la pointe (render::HALO_VIDE).
const HALO_VIDE = 6;
// Petites figures (arcs, quatre temps, anneau) : comme les constantes de src/figures.rs.
const RETOMBEE_GESTES = [7, 8, 10], FIG_SUIVI_MONTEE = 6, TEMPO_FONDU = [0.15, 0.8], SOUS_LE_POINTEUR = [7, 27];
// Centre des figures : au bout d'un ressort accroché au pointeur (pulsation, amortissement),
// jamais à plus de FIG_RETARD_MAX px (à 100 %) de lui (figures::SUIVI, figures::RETARD_MAX).
const FIG_SUIVI = [14, 0.55], FIG_RETARD_MAX = 40;
const ARC_CRAN = 0.6, ARC_BOND = 90, ARC_RECUL = 13, ARC_BORNES = [-4, 8], ARC_PETIT = [0.675, 0.76];
const ARC_GRAND_OUVERTURE = [0.14, 5.5], ARC_PETIT_OUVERTURE = [0.12, 4.8];
const SAUT_BORNES = [-12, 6], TAILLE_BORNES = [0.3, 1.8], TIRET_BORNES = [-1, 5], POINT_ACTIF = 1.45, POINT_ETEINT = 0.4, VAGUE = 0.06;
// Anneau : bornes de son échelle, écho (durée, élargissement), étirement au plus et vitesse qui
// le donne (px/s à 100 %) ; il s'étire aussi avec son retard (Apercu.etirementFigures).
const ANNEAU_BORNES = [0.2, 1.7], ANNEAU_ECHO = [0.42, 1.2], ANNEAU_ECHOS_MAX = 6, ANNEAU_VITESSE = 900, ETIREMENT_DEFAUT = 0.45, ETIREMENT_MAX = 0.7;
// Cibles propres modulées au plus (comme figures::ECART_MAX, TAILLE_POINTS_MAX, plus::RELIEF_MAX, FIL_MAX).
const ECART_MAX = 2, TAILLE_POINTS_MAX = 2.5, RELIEF_MAX = 2, FIL_MAX = 2.5;
// Orbe, ligne de voix, trame, trait de pinceau et pendule : comme les constantes de src/figures/plus.rs.
const ORBE_POINTS = 96, ORBE_SOUFFLE = [-6, 12], ORBE_ECHO = [0.45, 0.32], ORBE_ELAN = 110, ORBE_ECHOS_MAX = 8;
const VOIX_POINTS = 40, VOIX_MORCEAUX = 8, VOIX_ATTENTE = 1.5, VOIX_FONDU = [0.15, 0.8], VOIX_LARGE = [0.5, 1.6], VOIX_ONDES = [0.62, 0.4];
const TRAME_VIDE = [7, 16], TRAME_VAGUE = [220, 10], TRAME_ONDE_MAX = 2.5, TRAME_DEPLACE = 3, TRAME_LOUPE_MAX = 1.5, TRAME_VAGUES_MAX = 12, TRAME_ECLAIRS_MAX = 48;
const PINCEAU_PAS = 0.5, PINCEAU_VITESSE = [120, 700], PINCEAU_PERLE = [1.4, 0.4, 2.4], PENDULE_GRAVITE = 900, PENDULE_SAUT = [250, 200];
// Gouttes en contour : comme les constantes de src/figures/gouttes.rs.
const GOUTTE_DEMI = 72, GOUTTE_NOEUDS = 73, GOUTTE_NOYAU = [8, 2.5, 4], GOUTTE_NOYAU_ELAN = [40, 70], GOUTTE_NOYAU_BORNES = [2, 18];
const GOUTTE_PERLES = [[17, 2.1, 4.8], [23, -1.6, 4.2], [29, 1.2, 5.2], [34, -0.9, 4.2], [20, -1.9, 3.8], [26, 1.4, 4.4], [31, -1.1, 3.9], [14, 2.6, 3.6]];
// Perles et noyau ajoutés par leur modulation à fond, bulles au plus, goutte détachée par l'accent
// (vitesse, part du rayon de sa perle, durée), traîne liquide (retard au plus, part du noyau,
// pulsation et amortissement du rappel) : comme figures/gouttes.rs.
const GOUTTE_PERLES_MODULE = 4, GOUTTE_NOYAU_MODULE = 8, GOUTTE_BULLES_MAX = 12, GOUTTE_DETACHEE = [110, 0.7, 0.8], GOUTTE_TRAINE = [26, 0.35, 18, 0.3];
const GOUTTE_PERLE_ELAN = [[80, 140], 200], GOUTTE_PERLE_ECART_MAX = 24, GOUTTE_PERLE_MONTEE = 0.9;
const GOUTTE_BULLES = 5, GOUTTE_BULLE_VITESSE = [90, 160], GOUTTE_BULLE_RAYON = [2.2, 3.2], GOUTTE_BULLE_VIE = [0.6, 0.9], GOUTTE_BULLE_FREIN = 3.5;
const GOUTTE_DEGRADE = 45;
const champGouttes = new Float32Array(GOUTTE_NOEUDS * GOUTTE_NOEUDS);

// Les neuf figures autour du pointeur, dans l'ordre de config::FIGURES (celui des effets de
// modulation qui les visent), et l'ordre de leur dessin, les premières dessous (figures::ORDRE).
const FIGURES = ["arcs", "quatre_temps", "anneau", "orbe", "voix", "trame", "pinceau", "pendule", "gouttes"];
const [F_ARCS, F_QUATRE, F_ANNEAU, F_ORBE, F_VOIX, F_TRAME, F_TRAIT, F_PENDULE, F_GOUTTES] = FIGURES.keys();
const ORDRE_FIGURES = [F_GOUTTES, F_TRAME, F_ORBE, F_VOIX, F_TRAIT, F_PENDULE, F_ARCS, F_QUATRE, F_ANNEAU];
// Le trait suit le trajet du pointeur et le pendule y est accroché : ils ne tournent, ne grandissent
// ni ne se déplacent (comme figures::geometrique).
const geometrique = (i) => i !== F_TRAIT && i !== F_PENDULE;
// Seuls les arcs, l'anneau, l'orbe et les gouttes tournent (comme figures::tourne) : les quatre
// temps et la voix sont sous la flèche, la trame est fixe à l'écran.
const tourne = (i) => i === F_ARCS || i === F_ANNEAU || i === F_ORBE || i === F_GOUTTES;
// Part de la vitesse des modulations prise par la rotation d'une figure, décalage d'une figure par
// « rayon » de décalage (px à 100 %) : comme FIG_VITESSE et FIG_DECALAGE.
const FIG_VITESSE = 0.1, FIG_DECALAGE = 30;
// Échelle d'une figure, modulations comprises, de 0,5 à 2,5 (comme figures::GEO_MIN, GEO_MAX).
const GEO_MIN = 0.5, GEO_MAX = 2.5;
// Fissures : rayon le plus long en tailles et portée la plus longue (px à 100 %), comme
// render::FISSURE_RAYON et FISSURE_PORTEE_MAX.
const FISSURE_RAYON_MAX = 44, FISSURE_PORTEE_MAX = 400;
// Allure d'une figure (comme config::Allure) : défaut et bornes de chaque réglage.
const ALLURE = {
  echelle: [1, 0.5, 2], opacite: [1, 0.1, 1], teinte: [0, -180, 180], teinte_fixe: [200, 0, 360], saturation: [1, 0, 1], luminosite: [0.66, 0, 1], degrade: [40, 0, 180], rotation: [0, -30, 30], decalage_x: [0, -80, 80], decalage_y: [0, -80, 80],
};
// Valeur d'un réglage, bornée ; `d` si elle manque.
const regle = (v, lo, hi, d) => Math.min(hi, Math.max(lo, finiOu(+v, d)));
// Allure de la section d'une figure : une clé absente prend sa valeur par défaut.
function allureDe(sec) {
  const a = {};
  for (const [cle, [d, lo, hi]] of Object.entries(ALLURE)) a[cle] = regle((sec || {})[cle], lo, hi, d);
  return a;
}
// Gestes des figures (comme config::GESTES) : rang dans les gestes reçus par une figure.
const [BATTEMENT, ACCENT, SCINTILLEMENT, APOGEE] = [0, 1, 2, 3];
const GESTES_FIGURE = ["battement", "accent", "scintillement", "apogee"];
// Gestes que fait chaque figure (comme config::gestes_de_figure) : un geste qui ne ferait que
// pousser un paramètre déjà cible de ses modulations est une modulation (MODULATIONS_DES_FIGURES).
const GESTES_DES_FIGURES = {
  voix: GESTES_FIGURE, trame: GESTES_FIGURE, arcs: ["battement", "accent", "scintillement"],
  quatre_temps: ["battement", "accent", "apogee"], pendule: ["battement", "accent", "apogee"], gouttes: ["battement", "accent", "apogee"],
  orbe: ["battement", "apogee"], anneau: ["scintillement", "apogee"], pinceau: ["battement"],
};
// Réactions de toujours faites par une modulation (comme config::modulations_de_figure) : le geste
// qu'elles remplacent depuis la 0.28, source, cible et quantité (aussitôt, pour la figure seule).
const MODULATIONS_DES_FIGURES = {
  arcs: [["apogee", "drop", "ouverture", 1]],
  anneau: [["battement", "kick", "rayon", 0.05], ["accent", "caisse", "forme", 1]],
  pinceau: [["apogee", "drop", "duree", 0.35]],
};
// Gestes reçus par une figure depuis l'image d'avant : combien de chaque, et leur force cumulée
// (comme figures::Gestes, 4 au plus de chaque sorte) ; force d'un geste (moyenne), 0 sans geste.
const gestesVides = () => ({ n: [0, 0, 0, 0], force: [0, 0, 0, 0] });
function ajouterGeste(g, k, force) {
  if (g.n[k] >= 4 || !Number.isFinite(force)) return;
  g.n[k]++;
  g.force[k] += Math.min(2, Math.max(0, force));
}
const forceGeste = (g, k) => (g.n[k] > 0 ? g.force[k] / g.n[k] : 0);
// Facteurs des cibles propres modulées (comme figures::ecart_module, plus::relief_module,
// duree_module, fil_module).
const ecartModule = (x) => (Number.isFinite(x) ? Math.min(ECART_MAX, Math.max(0.3, 1 + x)) : 1);
const reliefModule = (x) => (Number.isFinite(x) ? Math.min(RELIEF_MAX, Math.max(0, 1 + x)) : 1);
const dureeModule = (x) => (Number.isFinite(x) ? Math.min(2.5, Math.max(0.3, 1 + x)) : 1);
const filModule = (x) => (Number.isFinite(x) ? Math.min(FIL_MAX, Math.max(0.5, x)) : 1);
// Zone d'une figure au repos autour du pointeur, avant sa pose : [x0, y0, x1, y1], en unités de
// l'aperçu (× S), d'après ses réglages et les bornes de son dessin plus bas, sans les élans du son.
// null pour le trait, qui suit la souris. Sert seulement au repère de place (dessinerRepere).
function zoneFigure(i, s) {
  const carre = (r) => [-r, -r, r, r];
  if (i === F_ARCS) return carre(regle(s.rayon, 10, 80, 20));
  if (i === F_ANNEAU) return carre(regle(s.rayon, 6, 60, 13));
  if (i === F_ORBE) return carre(regle(s.rayon, 10, 60, 21) + regle(s.relief, 2, 30, 9));
  if (i === F_TRAME) return carre(regle(s.rayon, 20, 120, 46));
  if (i === F_GOUTTES) return carre(Math.max(...GOUTTE_PERLES.map(([orbite, , r]) => orbite + r)) * regle(s.taille, 0.6, 1.6, 1));
  if (i === F_QUATRE) {
    const t = regle(s.taille, 1, 5, 1.6) * POINT_ACTIF, demi = 1.5 * regle(s.ecart, 4, 20, 7.5) + t;
    return [SOUS_LE_POINTEUR[0] - demi, SOUS_LE_POINTEUR[1] - t, SOUS_LE_POINTEUR[0] + demi, SOUS_LE_POINTEUR[1] + t];
  }
  if (i === F_VOIX) {
    const h = regle(s.hauteur, 2, 20, 6.5), demi = regle(s.largeur, 20, 120, 46) / 2, y = SOUS_LE_POINTEUR[1] + 1.4 * h;
    return [SOUS_LE_POINTEUR[0] - demi, y - h, SOUS_LE_POINTEUR[0] + demi, y + h];
  }
  if (i === F_PENDULE) {
    const l = regle(s.longueur, 8, 60, 22), t = regle(s.taille, 1, 6, 2.6);
    return [-l - t, -t, l + t, l + t];
  }
  return null;
}
// La pose ne change rien : la figure est dessinée directement (comme figures::Pose::neutre).
const poseNeutre = (p) => !p || (p.angle === 0 && p.geo === 1 && p.traits === 1 && p.decalage[0] === 0 && p.decalage[1] === 0 && p.alpha === 1);
// Couleur CSS d'opacité multipliée par `alpha`, plafonnée à 1 (comme figures::Posee::alpha).
function styleAlpha(style, alpha) {
  const m = typeof style === "string" && /^rgba?\(([^,]+),([^,]+),([^,)]+)(?:,([^)]+))?\)$/.exec(style.replace(/\s+/g, ""));
  if (!m) return style;
  const a = m[4] === undefined ? 1 : +m[4];
  return `rgba(${m[1]},${m[2]},${m[3]},${Math.max(0, Math.min(1, a * alpha))})`;
}
// Contexte d'une figure posée (comme figures::Posee) : le canevas, déjà tourné et agrandi de `geo`,
// garde des traits de l'épaisseur demandée × `traits` (l'agrandissement est annulé), et l'opacité
// de chaque couleur est multipliée par `alpha`.
function contextePose(g, pose) {
  const fonctions = new Map();
  return new Proxy(g, {
    get(cible, nom) {
      const v = cible[nom];
      if (typeof v !== "function") return v;
      if (!fonctions.has(nom)) fonctions.set(nom, v.bind(cible));
      return fonctions.get(nom);
    },
    set(cible, nom, v) {
      // une épaisseur nulle serait ignorée par le canevas (il garderait la précédente)
      if (nom === "lineWidth") cible.lineWidth = Math.max(1e-4, (v * pose.traits) / pose.geo);
      else if (nom === "strokeStyle" || nom === "fillStyle") cible[nom] = styleAlpha(v, pose.alpha);
      else cible[nom] = v;
      return true;
    },
  });
}
// Couleur CSS d'une couleur [r, g, b] (arrondie) et d'une opacité ; mélange de deux couleurs.
const plein = (rgb, a = 1) => `rgba(${Math.round(rgb[0])},${Math.round(rgb[1])},${Math.round(rgb[2])},${Math.max(0, Math.min(1, a))})`;
const melange = (a, b, k) => a.map((x, i) => x + (b[i] - x) * Math.min(1, Math.max(0, k)));
// Passage adouci de 0 (x ≤ a) à 1 (x ≥ b), et cloche de Gauss (comme figures::lisse et gauss).
const lisse = (a, b, x) => { const k = Math.min(1, Math.max(0, (x - a) / (b - a))); return k * k * (3 - 2 * k); };
const gauss = (x) => Math.exp(-x * x);

// Contour au niveau 1 du champ des gouttes ([x, y, rayon], px autour du centre), en segments
// (comme figures::gouttes::contour) : marching squares sur la partie utile de la grille.
function contourGouttes(gouttes) {
  const N = GOUTTE_NOEUDS, D = GOUTTE_DEMI, pas = (2 * D) / (N - 1), champ = champGouttes, segments = [];
  const noeud = (v) => Math.min(N - 1, Math.max(0, (v + D) / pas));
  let i0 = N, j0 = N, i1 = 0, j1 = 0;
  for (const [x, y, r] of gouttes) {
    const m = r * 3.2 + 3;
    i0 = Math.min(i0, Math.floor(noeud(x - m))); i1 = Math.max(i1, Math.ceil(noeud(x + m)));
    j0 = Math.min(j0, Math.floor(noeud(y - m))); j1 = Math.max(j1, Math.ceil(noeud(y + m)));
  }
  if (i0 >= i1 || j0 >= j1) return segments;
  for (let j = j0; j <= j1; j++) {
    const y = -D + j * pas;
    for (let i = i0; i <= i1; i++) {
      const x = -D + i * pas;
      let f = 0;
      // le bord de la grille est toujours dehors : un contour qui l'atteint s'y referme
      if (i > 0 && j > 0 && i < N - 1 && j < N - 1) for (const [gx, gy, r] of gouttes) f += (r * r) / ((x - gx) ** 2 + (y - gy) ** 2 + 1);
      champ[j * N + i] = f;
    }
  }
  const croise = ([xa, ya, va], [xb, yb, vb]) => {
    const k = Math.abs(vb - va) > 1e-6 ? Math.min(1, Math.max(0, (1 - va) / (vb - va))) : 0.5;
    return [xa + (xb - xa) * k, ya + (yb - ya) * k];
  };
  for (let j = j0; j < j1; j++) {
    for (let i = i0; i < i1; i++) {
      const coin = (di, dj) => [-D + (i + di) * pas, -D + (j + dj) * pas, champ[(j + dj) * N + i + di]];
      // coins dans le sens des aiguilles d'une montre : haut gauche, haut droit, bas droit, bas gauche
      const c = [coin(0, 0), coin(1, 0), coin(1, 1), coin(0, 1)], dedans = c.map((q) => q[2] > 1);
      const nombre = dedans.filter(Boolean).length;
      if (nombre === 0 || nombre === 4) continue;
      const passages = [0, 1, 2, 3].map((a) => (dedans[a] !== dedans[(a + 1) % 4] ? croise(c[a], c[(a + 1) % 4]) : null));
      if (passages.every(Boolean)) {
        // col : le milieu de la case dit quels coins sont reliés
        const milieu = (c[0][2] + c[1][2] + c[2][2] + c[3][2]) / 4 > 1;
        if (milieu === dedans[0]) segments.push([passages[0], passages[1]], [passages[2], passages[3]]);
        else segments.push([passages[3], passages[0]], [passages[1], passages[2]]);
      } else {
        const bouts = passages.filter(Boolean);
        if (bouts.length >= 2) segments.push([bouts[0], bouts[1]]);
      }
    }
  }
  return segments;
}

// Équerres et liseré : comme les constantes de src/loin.rs.
const EQUERRE_ECART = 4, EQUERRE_ELAN = [110, 260], EQUERRE_BASSE = 1.3, EQUERRE_ELAN_BRAS = 38, EQUERRE_BORNES = [[0, 26], [0.4, 3.2]];
const FILET_PAS = 6, FILET_CRANS = 16, FILET_LUEUR = [0.1, 0.75], FILET_PLEIN = [0.8, 0.25], FILET_KICK = [1, 0.6, 560], FILET_DROP = [2, 1.1, 760], FILET_ECLAT = 40, FILET_ECLATS_MAX = 8;
const FILET_ATTENTE = 1.5, FILET_FONDU = [0.15, 0.8], LISERE_LUEUR = [36, 0.26];
const HALO_LUEUR = 0.11, HALO_FRISSON = 0.2, HALO_NAPPES = 4, HALO_OPACITE = 0.45;

// Suites de pas voisins de même force et de même teinte (comme loin::suites), et trait à bouts
// droits (comme Canvas::bande) : des traits bout à bout ne se recouvrent pas.
function suitesFilet(long, pas, niveau, trace) {
  const n = Math.ceil(long / pas), cran = (v) => Math.round(Math.min(1, Math.max(0, v)) * FILET_CRANS);
  let cours = null;
  for (let i = 0; i <= n; i++) {
    const a = Math.min(long, i * pas);
    let cle = null;
    if (i < n) {
      const [f, k] = niveau((a + Math.min(long, (i + 1) * pas)) / 2);
      cle = [f < 0.03 ? 0 : Math.max(1, cran(f)), cran(k)];
    }
    if (cours && cle && cours[1][0] === cle[0] && cours[1][1] === cle[1]) continue;
    if (cours && cours[1][0] > 0) trace(cours[0], a, cours[1][0] / FILET_CRANS, cours[1][1] / FILET_CRANS);
    cours = cle && [a, cle];
  }
}
function bande(g, a, b, largeur, couleur) {
  g.lineCap = "butt";
  g.lineWidth = largeur;
  g.strokeStyle = couleur;
  g.beginPath();
  g.moveTo(a[0], a[1]);
  g.lineTo(b[0], b[1]);
  g.stroke();
  g.lineCap = "round";
}

// Étiquette du morceau : comme les constantes de src/etiquette.rs.
const ETIQUETTE_DECALAGE = [16, 24], ETIQUETTE_ENTREE = 0.3, ETIQUETTE_SORTIE = 0.4, ETIQUETTE_GLISSE = 6, ETIQUETTE_CERNE = 2, ETIQUETTE_CERNE_OPACITE = 0.6;
const ETIQUETTE_ARTISTE = [0.85, 0.9], ETIQUETTE_FILET = [1.4, 1.6], ETIQUETTE_FILET_ECART = 2;

// Mode sans flashs (comme render::ECART_FLASH, win::sans_flashs) : au plus un éclat tous les
// ECART_FLASH secondes, à moitié de force. « auto » suit Windows (animations réduites), que la
// page voit par prefers-reduced-motion.
const ECART_FLASH = 1 / 3;
// une seule requête, suivie (lue à chaque image) : le réglage de Windows peut changer en route
const animationsReduitesApercu = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;
let animationsReduitesVoulu = !!(animationsReduitesApercu && animationsReduitesApercu.matches);
if (animationsReduitesApercu) animationsReduitesApercu.addEventListener("change", (e) => { animationsReduitesVoulu = e.matches; });
function sansFlashs(c) {
  const mode = (c.rendu || {}).sans_flashs;
  if (mode === "oui") return true;
  if (mode === "non") return false;
  return animationsReduitesVoulu;
}

// Filet de lumière du liseré et du halo (comme loin::Filet) : état, avance, force en un point.
// `doux` : ni embrasement ni pleine force, un éclat tous les ECART_FLASH secondes au plus.
const nouveauFilet = () => ({ eclats: [], embrase: 0, silence: FILET_ATTENTE, presence: 0, dernierEclat: -Infinity });
function avancerFilet(fl, t, dt, u, kick, drop, son, doux) {
  const eclat = (e, k) => ({ u, ne: t, force: e[0] * k, vie: e[1], vitesse: e[2] });
  if (!doux) {
    if (kick) fl.eclats.push(eclat(FILET_KICK, 1));
    if (drop) { fl.eclats.push(eclat(FILET_DROP, 1)); fl.embrase = 1; }
  } else if ((kick || drop) && t - fl.dernierEclat >= ECART_FLASH) {
    fl.dernierEclat = t;
    fl.eclats.push(eclat(drop ? FILET_DROP : FILET_KICK, 0.5));
  }
  fl.eclats = fl.eclats.filter((e) => t - e.ne < e.vie).slice(-FILET_ECLATS_MAX);
  fl.embrase *= Math.exp(-dt * 1.5);
  fl.silence = son || fl.eclats.length > 0 || fl.embrase > 0.02 ? 0 : fl.silence + dt;
  fl.presence = fl.silence >= FILET_ATTENTE ? Math.max(0, fl.presence - dt / FILET_FONDU[1]) : Math.min(1, fl.presence + dt / FILET_FONDU[0]);
}
function forceFilet(fl, t, S, basse, montee, lueur, auPointeur, ecart) {
  let f = (FILET_LUEUR[0] + FILET_LUEUR[1] * basse) * Math.exp(-((auPointeur / lueur) ** 2)) + FILET_PLEIN[0] * fl.embrase + FILET_PLEIN[1] * montee;
  for (const e of fl.eclats) {
    const age = t - e.ne;
    f += e.force * Math.max(0, 1 - age / e.vie) ** 1.2 * Math.exp(-(((ecart(e.u) - e.vitesse * S * age) / (FILET_ECLAT * S)) ** 2));
  }
  return f;
}
const RETOMBEE = [7, 8, 10, 0.9, 10];
const SUIVI_MONTEE = 6;
const LISSAGE = 0.35;
const ADDITIONNEES = new Set(["rotation", "vitesse", "teinte", "decalage"]);
// Cibles propres des figures : parts ajoutées (comme modulation::Propres).
const PROPRES = ["ouverture", "ecart", "forme", "relief", "duree", "nombre", "noyau"];
const effetNeutre = () => ({ rotation: 0, vitesse: 0, teinte: 0, decalage: 0, acceleration: 1, rayon: 1, taille: 1, luminosite: 1, trainee: 1, opacite: 1, propres: Object.fromEntries(PROPRES.map((c) => [c, 0])) });
const finiOu = (v, d) => (Number.isFinite(v) ? v : d);

// Valeur visée d'une modulation (comme modulation::visee) : quantité × source, la source à
// l'envers si demandé, rien sous le seuil.
function visee(m, source) {
  const stereo = m.source === "stereo";
  let signe = stereo ? Math.sign(source) : 1;
  let v = stereo ? Math.abs(source) : Math.max(0, source);
  if (m.inverser && !stereo) v = 1 - Math.min(1, v);
  if (m.inverser && stereo) signe = -signe;
  const s = Math.min(0.95, Math.max(0, m.seuil || 0));
  v = Math.max(0, (v - s) / (1 - s));
  const q = Math.max(-1, Math.min(1, finiOu(+m.quantite, 0)));
  return finiOu(q * v * signe, 0);
}

// Part du chemin vers la valeur visée en dt secondes (comme modulation::part_lissage).
function partLissage(lissage, dt) {
  const tau = 0.4 * Math.min(1, Math.max(0, finiOu(+lissage, LISSAGE))) ** 2;
  return tau < 1e-4 ? 1 : 1 - Math.exp(-dt / tau);
}

// Effets des modulations (comme modulation::effets) : sur les comètes et les satellites
// (`groupes`), sur chaque figure (`figures`, dans l'ordre de FIGURES) et teinte de tout ce qui est
// dessiné (`teinte`, la teinte « sur tout »). « tout » vise les comètes et les satellites,
// « figures » toutes les figures, le nom d'une figure celle-là seule.
function effetsModulations(liste, valeurs) {
  const r = { groupes: [effetNeutre(), effetNeutre()], figures: FIGURES.map(effetNeutre), teinte: 0 };
  (Array.isArray(liste) ? liste : []).forEach((m, i) => {
    const v = valeurs[i];
    if (!Number.isFinite(v)) return;
    const sur = m.sur || "tout";
    const figures = r.figures.filter((_, i) => sur === "figures" || sur === FIGURES[i]);
    // cible propre d'une figure : chacune lit celles qui la concernent
    if (PROPRES.includes(m.cible)) { for (const e of figures) e.propres[m.cible] += v; return; }
    if (!(m.cible in MODULATION)) return;
    if (m.cible === "teinte" && sur === "tout") { r.teinte += v * MODULATION.teinte; return; }
    const groupes = r.groupes.filter((_, g) => sur === "tout" || (g === 0 && sur === "cometes") || (g === 1 && sur === "satellites"));
    for (const e of groupes.concat(figures)) {
      if (ADDITIONNEES.has(m.cible)) e[m.cible] += v * MODULATION[m.cible];
      else e[m.cible] *= Math.max(0, 1 + v * MODULATION[m.cible]);
    }
  });
  const borne = (x, a, b) => Math.min(b, Math.max(a, x));
  for (const e of r.groupes.concat(r.figures)) {
    e.acceleration = borne(e.acceleration, 0, 5);
    e.rayon = borne(e.rayon, 0.1, 5);
    e.taille = borne(e.taille, 0, 4);
    e.luminosite = borne(e.luminosite, 0, 4);
    e.trainee = borne(e.trainee, 0.1, 5);
    e.opacite = borne(e.opacite, 0, 1);
  }
  return r;
}

// Ressort amorti avancé de dt (comme render::ressort) : x son écart à la cible, v sa vitesse, w sa
// pulsation, z son amortissement ; en petits pas (w × pas ≤ 0,25). Renvoie [x, v].
function ressort(x, v, w, z, dt) {
  const n = Math.min(16, Math.max(1, Math.ceil((w * dt) / 0.25))), h = dt / n;
  for (let i = 0; i < n; i++) {
    v += (-w * w * x - 2 * z * w * v) * h;
    x += v * h;
  }
  return Number.isFinite(x) && Number.isFinite(v) ? [x, v] : [0, 0];
}

// Ressort borné des figures (comme figures::Ressort::vers) : `r` = [position, vitesse], avancé de
// dt vers `cible` ; en butée, l'élan est perdu.
function vers(r, cible, [w, z], dt, [lo, hi]) {
  const [ecart, v] = ressort(r[0] - cible, r[1], w, z, dt), x = cible + ecart;
  r[0] = Math.min(hi, Math.max(lo, x));
  r[1] = r[0] === x ? v : 0;
}

// Retard d'une goutte sur le centre (px à 100 %), au bout d'un ressort par axe.
const traineVide = () => ({ x: [0, 0], y: [0, 0] });

// État des petites figures (comme figures::Figures::new).
function nouvellesFigures() {
  return {
    // centre (null avant la première image) et sa vitesse, écart au pointeur et retard au plus (px)
    centre: null, vitesse: [0, 0], retard: [0, 0], retardMax: FIG_RETARD_MAX,
    horloge: 0, montee: 0, basse: 0, compte: 0, mesure: 0, tempo: 0,
    // gestes reçus des déclencheurs pour la prochaine image, enveloppes du battement, de l'accent
    // et du scintillement de chaque figure
    gestes: FIGURES.map(gestesVides), enveloppes: FIGURES.map(() => [0, 0, 0]),
    arcs: { tour: [0, 0], cran: 0, bond: [0, 0], recul: [0, 0], grave: 0, aigu: 0 },
    // vague du drop : son début et sa force
    temps: { pts: [0, 1, 2, 3].map(() => ({ saut: [0, 0], taille: [1, 0], tiret: [0, 0], arme: false })), actif: null, vague: -Infinity, force: 1 },
    anneau: { echelle: [1, 0], echos: [] },
    // figures/plus.rs
    orbe: { souffle: [0, 0], contour: [], echos: [] },
    voix: { niveaux: [0, 0, 0], large: [1, 0], silence: VOIX_ATTENTE, visible: 0 },
    trame: { loupe: [0, 0], vagues: [], eclairs: [] },
    pinceau: { trajet: [], perles: [], vie: [0, 0] },
    pendule: { poids: null, vitesse: [0, 0], fil: [0, 0], ancre: [0, 0], mou: 0, cote: 1 },
    // figures/gouttes.rs
    gouttes: { noyau: [GOUTTE_NOYAU[0], 0], traine: traineVide(), perles: GOUTTE_PERLES.map((_, i) => ({ angle: i * 2.4, ecart: [0, 0], traine: traineVide() })), bulles: [], segments: [] },
    // loin.rs
    loin: { kick: 0, ecart: [EQUERRE_ECART, 0], bras: [1, 0], diagonales: [0, 0], tour: 0, halo: nouveauFilet(), frisson: 0, lisere: nouveauFilet() },
    // pose de chaque figure (figures::Figures::poser) : angle propre, pose et teinte à cette image
    angles: FIGURES.map(() => 0), poses: FIGURES.map(() => null), teintes: FIGURES.map(() => 0),
  };
}

// Sortie avec dépassement, de 0 à 1 (comme render::depasse).
function depasse(k) {
  const c = 1.70158, u = Math.min(1, Math.max(0, k)) - 1;
  return 1 + (c + 1) * u * u * u + c * u * u;
}

// Étirement d'une tête en mode ressorts (comme render::etirement) : de la tête au bout de son
// étirement, à l'opposé de sa vitesse. `S` : l'échelle de l'aperçu.
function etirement(tr, S, force) {
  const n = tr.length;
  if (n < 2 || !(force > 0)) return [0, 0];
  const a = tr[n - 2], b = tr[n - 1], dx = b[1] - a[1], dy = b[2] - a[2], dt = b[0] - a[0], d = Math.hypot(dx, dy);
  if (dt < 1e-4 || d < 1e-3) return [0, 0];
  const l = Math.min((d / dt) * ETIREMENT[0], ETIREMENT[1] * S) * Math.min(2, force);
  return [(-dx / d) * l, (-dy / d) * l];
}

// Événements du son depuis la lecture précédente (comme render::Ecoute) : kicks, caisses
// claires, drops, attaques des aigus (4 au plus chacun), temps (pendant un break ou non) et
// contretemps quand le tempo est régulier.
class Ecoute {
  constructor() { this.vus = {}; this.phase = null; }
  lire(son) {
    const lu = (nom) => {
      const n = son[nom] || 0, avant = this.vus[nom];
      this.vus[nom] = n;
      if (avant === undefined) return 0;
      const d = (n - avant) >>> 0;
      return d > 0x7fffffff ? 0 : Math.min(4, d);
    };
    const kicks = lu("kicks"), caisses = lu("caisses"), drops = lu("drops"), aigus = lu("aigus");
    const phase = son.bpm ? son.phase || 0 : null;
    let temps = false, contretemps = false;
    if (phase !== null && this.phase !== null) {
      temps = phase < this.phase - 0.5;
      contretemps = this.phase < 0.5 && phase >= 0.5 && phase - this.phase < 0.4;
    }
    this.phase = phase;
    return { kicks, caisses, drops, aigus, temps, pause: !!son.relais, contretemps };
  }
}

function hsl(h, s, l) {
  const c = (1 - Math.abs(2 * l - 1)) * s, x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
}

// Teinte : k = 0 en queue de traînée, 1 en tête (comme render::prisme). `imposee` : teinte de
// l'application au premier plan ou de l'éclairage, quand le preset la suit ; `decalage` : teinte
// ajoutée à tout ce qui est dessiné (modulations « sur tout ») ; `j` : teinte de la palette (comme
// render::prisme_palette) ; `plus` : degrés ajoutés par une modulation de la teinte d'un groupe.
function prisme(c, t, k, imposee, decalage = 0, j = 0, plus = 0) {
  const n = Math.min(3, Math.max(1, Math.round(c.palette || 1)));
  const palette = (j % n) * (c.ecart_palette ?? 120);
  const base = (imposee ?? (((c.teinte || 0) + t * c.vitesse_teinte) % 360 + 360) % 360) + finiOu(decalage, 0) + palette + finiOu(plus, 0);
  const queue = base + c.ecart_teinte;
  let dh = base - queue;
  if (dh > 180) dh -= 360;
  if (dh < -180) dh += 360;
  return hsl((((queue + dh * k) % 360) + 360) % 360, luminositeDe(c.saturation, 1), luminositeDe(c.luminosite, LUMINOSITE));
}
// Luminosité par défaut des couleurs (comme config::LUMINOSITE) ; réglage de 0 à 1, `d` s'il manque.
const LUMINOSITE = 0.66;
const luminositeDe = (v, d) => (Number.isFinite(+v) && v !== null && v !== undefined ? Math.min(1, Math.max(0, +v)) : d);
// Blanc des reflets (cœurs, halo de l'éclat), assombri avec la luminosité des couleurs (comme
// render::blanc).
function blancReflet(c) {
  const v = Math.round(255 * Math.min(1, luminositeDe((c || {}).luminosite, LUMINOSITE) / LUMINOSITE));
  return [v, v, v];
}

// Contour fermé parcouru à vitesse constante, coins gardés (comme render::Contour) : ses
// sommets, la longueur parcourue jusqu'à chacun et le tour ; null s'il est trop court.
function contour(points) {
  const p = points.filter((q) => Array.isArray(q) && Number.isFinite(q[0]) && Number.isFinite(q[1])).slice(0, 2000);
  const n = p.length;
  if (n < 3) return null;
  const cumul = [];
  let total = 0;
  for (let i = 0; i < n; i++) {
    cumul.push(total);
    total += Math.hypot(p[(i + 1) % n][0] - p[i][0], p[(i + 1) % n][1] - p[i][1]);
  }
  return total > 1e-6 ? { points: p, cumul, total } : null;
}

// Point à la fraction u (0 à 1) du tour d'un contour (comme Contour::point).
function pointContour(c, u) {
  const s = (((u % 1) + 1) % 1) * c.total;
  let bas = 0, haut = c.cumul.length;
  while (bas < haut) { const m = (bas + haut) >> 1; if (c.cumul[m] <= s) bas = m + 1; else haut = m; }
  const i = Math.max(0, bas - 1), a = c.points[i], b = c.points[(i + 1) % c.points.length];
  const long = (i + 1 < c.cumul.length ? c.cumul[i + 1] : c.total) - c.cumul[i];
  const f = long > 1e-9 ? Math.min(1, Math.max(0, (s - c.cumul[i]) / long)) : 0;
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
}

// Contours (24 au plus) centrés sur le milieu du cadre qui les englobe et mis à la même échelle
// (point le plus loin : 1,2), comme render::mettre_a_l_echelle ; `echelle` faux : déjà à
// l'échelle (ceux de l'icône, venus de l'appli). Gardés par liste.
const CONTOURS_MAX = 24;
const contoursVus = new WeakMap();
function mettreALEchelle(liste, echelle = true) {
  if (!Array.isArray(liste) || !liste.length) return [];
  if (contoursVus.has(liste)) return contoursVus.get(liste);
  const tous = liste.slice(0, CONTOURS_MAX).filter(Array.isArray).map((c) => c.filter((q) => Array.isArray(q) && Number.isFinite(q[0]) && Number.isFinite(q[1])).slice(0, 2000)).filter((c) => c.length >= 3);
  let res = tous;
  if (echelle) {
    const pts = tous.flat();
    const xs = pts.map((q) => q[0]), ys = pts.map((q) => q[1]);
    const cx = (Math.min(...xs) + Math.max(...xs)) / 2, cy = (Math.min(...ys) + Math.max(...ys)) / 2;
    const r = Math.max(1e-6, ...pts.map((q) => Math.hypot(q[0] - cx, q[1] - cy)));
    res = tous.map((c) => c.map((q) => [((q[0] - cx) / r) * 1.2, ((q[1] - cy) / r) * 1.2]));
  }
  res = res.map(contour).filter(Boolean);
  contoursVus.set(liste, res);
  return res;
}

// Contours du trajet d'un réglage d'orbite : ceux d'une image, sinon le trajet dessiné d'avant
// (comme Orbite::chemins) ; ceux de l'icône de l'application au premier plan, que la page reçoit
// de l'appli (`window.contoursIcone`, déjà mis à l'échelle).
function contoursDe(o, forme) {
  if (forme === "icone") return mettreALEchelle(window.contoursIcone, false);
  if (forme !== "trace" || !o) return [];
  if (Array.isArray(o.contours) && o.contours.length) return mettreALEchelle(o.contours);
  if (Array.isArray(o.trace) && o.trace.length >= 3) {
    if (!contoursVus.has(o.trace)) contoursVus.set(o.trace, mettreALEchelle([o.trace]));
    return contoursVus.get(o.trace);
  }
  return [];
}

// Longueur parcourue par radian sur le contour i (comme render::longueur_par_radian) : 1,2 pour
// les formes calculées.
function longueurParRadian(contours, i) {
  const c = contours.length ? contours[i % contours.length] : null;
  return c ? Math.max(1.2, c.total / TAU) : 1.2;
}

// Point du trajet à l'angle a, pour un rayon de 1 (comme render::chemin). `contours` : ceux de la
// forme « trace » ou « icone » (contoursDe), `i` : le contour suivi.
function chemin(forme, a, contours, i = 0) {
  if (forme === "trace" || forme === "icone") {
    const c = Array.isArray(contours) && contours.length ? contours[i % contours.length] : null;
    return c ? pointContour(c, a / TAU) : [Math.cos(a), Math.sin(a)];
  }
  const poly = (k, r, r2, depart = -Math.PI / 2) => {
    const u = (((a / TAU) * k) % k + k) % k, j = Math.floor(u), f = u - j;
    const v = (j) => { const th = depart + (j * TAU) / k, rr = j % 2 === 0 ? r : r2; return [Math.cos(th) * rr, Math.sin(th) * rr]; };
    const p = v(j), q = v(j + 1);
    return [p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f];
  };
  switch (forme) {
    case "triangle": return poly(3, 1.2, 1.2);
    case "losange": return poly(4, 1.15, 1.15);
    case "hexagone": return poly(6, 1.05, 1.05);
    case "huit": return [1.2 * Math.cos(a), 0.6 * Math.sin(2 * a)];
    case "etoile": return poly(10, 1.25, 0.55);
    case "fleur": { const r = 0.95 + 0.3 * Math.cos(5 * a); return [Math.cos(a) * r, Math.sin(a) * r]; }
    case "carre": return poly(4, 1.25, 1.25, (-3 * Math.PI) / 4);
    case "pentagone": return poly(5, 1.1, 1.1);
    case "ellipse": return [1.3 * Math.cos(a), 0.7 * Math.sin(a)];
    case "coeur": {
      const s = Math.sin(a), y = 13 * Math.cos(a) - 5 * Math.cos(2 * a) - 2 * Math.cos(3 * a) - Math.cos(4 * a);
      return [(16 * s * s * s) / 15, -(y + 2.5) / 15];
    }
    case "trefle": { const r = 0.75 + 0.45 * Math.cos(3 * a); return [Math.cos(a) * r, Math.sin(a) * r]; }
    case "lissajous": return [0.9 * Math.cos(3 * a), 0.85 * Math.sin(2 * a)];
    case "astroide": return [1.3 * Math.cos(a) ** 3, 1.3 * Math.sin(a) ** 3];
    default: return [Math.cos(a), Math.sin(a)];
  }
}

// Épaisseur des barres du spectre (comme effets::spectre_largeur).
const spectreLargeur = (n, rayon, S) => Math.min(6 * S, Math.max(1.5 * S, ((TAU * rayon) / Math.max(1, n)) * 0.5));

// Bandes simulées pour l'aperçu (l'aperçu n'a pas l'analyse en bandes du vrai son) : le kick et
// la basse dans le grave, un médium qui ondule, la brillance et les attaques d'aigus dans
// l'aigu. Bande i sur n, centres de 40 Hz à 12 kHz en échelle logarithmique (comme audio::Bandes).
function bandesSimulees(n, t, son, env, eclatAigus) {
  const cloche = (x, c, l) => Math.exp(-(((x - c) / l) ** 2));
  return Array.from({ length: n }, (_, i) => {
    const x = i / Math.max(1, n - 1);
    const v = 0.8 * env * cloche(x, 0.08, 0.12) + 0.6 * Math.min(1, son.basse || 0) * cloche(x, 0.15, 0.18)
      + 0.4 * (0.6 + 0.4 * Math.sin(t * 2.3 + i * 1.7)) * cloche(x, 0.5, 0.22)
      + (0.35 * Math.min(1, son.brillance || 0) + 0.5 * eclatAigus) * cloche(x, 0.85, 0.15);
    return Math.max(0, Math.min(1, v));
  });
}

// Rayon et vitesse relatifs de l'anneau k (comme render::anneau).
const anneau = (k) => [1 + 0.55 * k, 1 / (1 + 0.45 * k)];

function rgba([r, g, b], a) {
  return `rgba(${r},${g},${b},${Math.max(0, Math.min(1, a))})`;
}

const reste = (x, m) => ((x % m) + m) % m;
const entre = (a, b) => a + Math.random() * (b - a);

// Ajoute la position courante (comme render::push_trail) : le dernier point suit la tête ; un
// nouveau point fixe n'est ajouté que tous les PAS_TRAINEE ou tous les `ecart` pixels, mesurés
// depuis l'avant-dernier.
// Au-delà de VIE_FINE, pas et écart sont multipliés par vie / VIE_FINE (une traînée de 12 s ne
// compte pas plus de points qu'une de 3 s).
// Propre à l'aperçu : `max` points au plus (Apercu.plafondTrainee, selon la taille du canevas et
// le nombre de traînées). Au-delà, la traînée est éclaircie (un point sur deux, la tête gardée) et
// ses pas suivants doublés (`tr.espace`), pour une densité égale sur toute sa longueur ; ils
// redeviennent plus fins quand elle n'a plus qu'un quart de son plafond.
const VIE_FINE = 3, TRAINEE_POINTS_MAX = 4000;
// Plafonds de l'aperçu : points par traînée et pour toutes, par pixel de largeur du canevas ; une
// traînée en garde toujours POINTS_TRAINEE_MIN.
const POINTS_PAR_PX = [0.6, 16], POINTS_TRAINEE_MIN = 8;
function pousser(tr, t, x, y, vie, ecart, max = TRAINEE_POINTS_MAX) {
  const espace = tr.espace || 1;
  const n = tr.length, p = n >= 2 ? tr[n - 2] : null, k = Math.max(1, vie / VIE_FINE) * espace, pas = PAS_TRAINEE * k, gap = ecart * k;
  if (p && t - p[0] < pas && (p[1] - x) ** 2 + (p[2] - y) ** 2 < gap * gap) tr[n - 1] = [t, x, y];
  else tr.push([t, x, y]);
  let debut = 0;
  while (debut < tr.length && t - tr[debut][0] > vie) debut++;
  if (debut) tr.splice(0, debut);
  if (tr.length > max) {
    let j = 0;
    for (let i = (tr.length - 1) % 2; i < tr.length; i += 2) tr[j++] = tr[i];
    tr.length = j;
    tr.espace = espace * 2;
  } else if (espace > 1 && tr.length < max / 4) {
    tr.espace = espace / 2;
  }
}

// Vie utile d'une traînée de vie `vie` (s) qui tourne à `w` radians/s (lissée) : au plus
// TOURS_TRAINEE tours de son trajet (comme render::vie_utile).
const vieUtile = (vie, w) => (Number.isFinite(w) && w > 1e-3 ? Math.min(vie, (TOURS_TRAINEE * TAU) / w) : vie);

// Facteur du rayon et décalage (en rayons) d'un groupe, ramenés ensemble pour que son point le
// plus loin du centre (`portee` rayons : étendue de la forme × anneau extérieur ou satellite le
// plus écarté) reste à moins de `dispo` (comme render::borne_etendue).
function borneEtendue(facteur, decalage, rayon, portee, dispo) {
  const total = Math.abs(decalage) * rayon + portee * rayon * facteur;
  if (!(Number.isFinite(total) && total > dispo && total > 0)) return [facteur, decalage];
  const k = dispo / total;
  return [facteur * k, decalage * k];
}

// Pas intermédiaires pour un déplacement d'angle `da` sur un rayon de `r` px (comme
// render::sous_pas) : la traînée reste une courbe quand l'essaim tourne vite.
// Redressement du trajet (comme render::REDRESSE, render::pas_rotation) : sous 2 tours/min, il
// revient à l'angle 0, par le plus court, à 3 par seconde.
const REDRESSE = [2, 3];
function pasRotation(rot, toursMin, dt) {
  const ecart = reste(Math.PI - rot, TAU) - Math.PI;
  const poids = Math.min(1, Math.max(0, 1 - Math.abs(toursMin) / REDRESSE[0]));
  return ((toursMin * TAU) / 60) * dt + ecart * (1 - Math.exp(-REDRESSE[1] * dt)) * poids;
}

function sousPas(da, r, [angle, ecart]) {
  return Math.min(24, Math.max(1, Math.ceil(Math.max(Math.abs(da) / angle, (Math.abs(da) * r) / ecart))));
}

// Une traînée repart de zéro.
function viderTrainee(tr) {
  tr.length = 0;
  tr.espace = 1;
}

// Nombre de valeurs (section.clé) qui diffèrent entre deux jeux de réglages, compté jusqu'à
// `jusqua` (inutile d'aller plus loin). Une section partagée telle quelle est passée sans la lire.
function changementsReglages(a, b, jusqua) {
  let n = 0;
  for (const s of new Set(Object.keys(a).concat(Object.keys(b)))) {
    const sa = a[s], sb = b[s];
    if (sa === sb) continue;
    const objets = sa && sb && typeof sa === "object" && typeof sb === "object" && !Array.isArray(sa) && !Array.isArray(sb);
    const cles = objets ? new Set(Object.keys(sa).concat(Object.keys(sb))) : [null];
    for (const k of cles) {
      const va = k === null ? sa : sa[k], vb = k === null ? sb : sb[k];
      if (va !== vb && JSON.stringify(va) !== JSON.stringify(vb) && ++n >= jusqua) return n;
    }
  }
  return n;
}

// Contours suivis (`orbite.contours`) différents de `a` à `b` : même liste (référence) d'abord,
// sinon même contenu.
function autresContours(a, b) {
  const ca = (a.orbite || {}).contours, cb = (b.orbite || {}).contours;
  return ca !== cb && JSON.stringify(ca) !== JSON.stringify(cb);
}

class Apercu {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.reglages = null;
    this.dir = 1;
    this.inverseA = -Infinity;
    // vitesse du trajet des comètes et de celui des satellites, lissée (render::Swarm::spd)
    this.spd = [3.5, 3.5];
    this.angles = [];
    this.rot = 0;
    this.rotSats = 0;
    // ressorts (render::Swarm) : écart et vitesse du ressort du rayon de chaque groupe, rayon visé
    // et facteur du rayon à l'image précédente, poussée de la rotation, coup de frein ; vrille des
    // rubans (saut visé, saut lissé)
    this.rebond = [[0, 0], [0, 0]];
    this.cibles = [1, 1];
    this.facteurs = [1, 1];
    this.pousse = 0;
    this.frein = 0;
    this.vrille = [0, 0];
    // petites figures (figures.rs), et coups du son entendus pendant l'image en cours
    this.figures = nouvellesFigures();
    // étiquette du morceau : début de l'affichage, enveloppe du kick, déjà allumée
    this.etiquette = { debut: -Infinity, kick: 0, allumee: false };
    this.coupsImage = { kicks: 0, caisses: 0, aigus: 0, drops: 0, temps: false };
    // modulations : enveloppes des coups et montée suivie (modulation::Sources), valeur lissée de
    // chaque modulation, effets sur les comètes et les satellites à cette image et à la
    // précédente, teinte de tout ce qui est dessiné
    this.sources = { kick: 0, caisse: 0, charleston: 0, drop: 0, contretemps: 0, montee: 0 };
    this.valeurs = [];
    this.mods = [effetNeutre(), effetNeutre()];
    this.modsAvant = [effetNeutre(), effetNeutre()];
    // effets sur chaque figure (render::Swarm::mods_figures), pris par les figures
    this.modsFigures = FIGURES.map(effetNeutre);
    this.teinte = 0;
    // déclencheurs : dernier déclenchement de chacun (délai minimal)
    this.derniers = [];
    this.ecoute = new Ecoute();
    this.sonVu = null;
    this.grave = 0;
    this.presence = 0;
    this.hauteur = 50;
    this.env = 0;
    // mode sans flashs (Swarm::doux, relu à chaque image : sansFlashs) et instant du dernier éclat
    this.doux = false;
    this.dernierFlash = -Infinity;
    this.trainees = [];
    // plafonds de points des traînées (fixés par dimensionner)
    this.pointsTrainee = TRAINEE_POINTS_MAX;
    this.pointsTotal = POINTS_TRAINEES;
    // vitesse angulaire des comètes et des satellites, lissée (Swarm::w_lisse), vie utile des
    // traînées des comètes par anneau (Swarm::vies), formes des deux trajets à l'image d'avant
    // (Swarm::formes) et modulations (source, cible, groupe) des valeurs lissées
    // (Swarm::cles_modulations)
    this.wLisse = [0, 0];
    this.vies = [];
    this.formes = null;
    this.clesModulations = [];
    this.sats = [];
    this.fixes = [];
    this.teinteImposee = null;
    this.curseur = null;
    this.particules = [];
    // fissures (l'aperçu les dessine, plus simplement que render::Crack)
    this.fissures = [];
    // part fractionnaire des grains du sillage, reportée à l'image suivante (Swarm::reste_sillage)
    this.resteSillage = 0;
    this.prochaineCouleur = 0;
    // souris sur le canevas (pixels du canevas), pour le sillage
    this.pointeur = null;
    this.pointeurAvant = null;
    this.t = 0;
    // anneau de spectre : bandes affichées (lissées) et éclat des attaques d'aigus
    this.bandes = new Array(BANDES_MAX).fill(0);
    this.eclatAigus = 0;
    this.aigusSpectre = null;
  }

  // Réglages changés d'un coup (preset appliqué, « Annuler », vignette qui montre un autre
  // preset) : toutes les traînées repartent de zéro, sinon un trait relierait l'ancien trajet au
  // nouveau (comme render::Swarm::vider_trainees). Une glissière ne change qu'une valeur à la
  // fois : le trajet y glisse. D'autres contours suivis (boîte « Trajet d'une image » : seuil,
  // texte) sont un autre trajet : les traînées repartent aussi de zéro.
  regler(reglages) {
    if (this.reglages && reglages !== this.reglages
      && (changementsReglages(this.reglages, reglages, 2) >= 2 || autresContours(this.reglages, reglages))) this.viderTrainees();
    this.reglages = reglages;
  }

  viderTrainees() {
    this.trainees.forEach(viderTrainee);
    for (const q of this.sats.concat(this.fixes)) viderTrainee(q.trainee);
  }

  // Taille du canevas en pixels physiques, et échelle pour que l'orbite la plus large tienne.
  dimensionner() {
    const r = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(r.width * dpr));
    // la hauteur aussi : un canevas neuf fait 300 × 150, et une largeur de 300 ne changerait rien
    if (this.canvas.width !== w || this.canvas.height !== w) {
      this.canvas.width = w;
      this.canvas.height = w;
    }
    const o = this.reglages.orbite;
    // une modulation du rayon (note grave…) élargit l'orbite : 70 % de la plus grande qu'elle
    // permet tient aussi
    const liste = (this.reglages.modulations || {}).liste || [];
    const fond = (v) => effetsModulations(liste, liste.map((m) => visee(m, v)));
    const rayonMax = fond(1).groupes.concat(fond(-1).groupes).reduce((m, e) => Math.max(m, e.rayon), 1);
    // plusieurs contours : un anneau chacun, tous au rayon 1
    const anneaux = contoursDe(o, o.forme).length > 1 ? 1 : o.anneaux;
    // l'essaim ne va jamais au-delà de la moitié de l'étendue (Apercu.pas, borneEtendue)
    const portee = Math.min(ETENDUE_MAX / 2, o.rayon * Math.max(SAT_R_MAX, anneau(anneaux - 1)[0]) * 1.3 * (1 + (rayonMax - 1) * 0.7) + 6);
    this.echelle = Math.min(2.2 * dpr, (w * 0.46) / portee);
    // points des traînées au plus, selon la taille du canevas (une vignette n'a pas besoin des
    // centaines de points d'une traînée à l'écran) : par traînée et pour toutes ensemble
    this.pointsTrainee = Math.min(TRAINEE_POINTS_MAX, Math.max(POINTS_TRAINEE_MIN, Math.round(w * POINTS_PAR_PX[0])));
    this.pointsTotal = Math.min(POINTS_TRAINEES, Math.round(w * POINTS_PAR_PX[1]));
  }

  // Points au plus d'une traînée à cette image : son plafond, et la part de chacune dans le
  // plafond de toutes (render::part_trainee : POINTS_TRAINEES partagés également entre toutes
  // celles que le réglage permet, satellites au maximum : une salve ne raccourcit pas les
  // comètes), jamais sous POINTS_TRAINEE_MIN.
  plafondTrainee() {
    const maximum = Math.max(0, finiOu(+((this.reglages || {}).essaim || {}).maximum, 0));
    const n = this.trainees.length + Math.max(maximum, this.sats.length) + this.fixes.length;
    return Math.max(POINTS_TRAINEE_MIN, Math.min(this.pointsTrainee, Math.floor(this.pointsTotal / Math.max(1, n))));
  }

  // Souris sur le canevas (coordonnées de la fenêtre), ou `survoler(null)` quand elle en sort.
  // L'essaim reste au centre ; le sillage, le trait, le pendule et le centre des figures suivent
  // la souris.
  survoler(clientX, clientY) {
    if (clientX === null) {
      this.pointeur = this.pointeurAvant = null;
      return;
    }
    const r = this.canvas.getBoundingClientRect();
    if (r.width <= 0) return;
    this.pointeur = [((clientX - r.left) / r.width) * this.canvas.width, ((clientY - r.top) / r.height) * this.canvas.height];
  }

  // Les vignettes avancent par pas de 1/30 s : découpés en pas de 1/90 s au plus (une image du
  // vrai essaim), avec dans chacun les points intermédiaires de render.rs. Les compteurs ne
  // changent pas d'un pas à l'autre : les événements ne comptent qu'une fois.
  avancer(dt, son) {
    if (!this.reglages) return;
    this.doux = sansFlashs(this.reglages);
    this.sillage(dt);
    this.coupsImage = { kicks: 0, caisses: 0, aigus: 0, drops: 0, temps: false };
    const n = Math.max(1, Math.ceil(dt / PAS_TRAINEE));
    for (let i = 0; i < n; i++) this.pas(dt / n, son);
    this.spectre(dt, son);
    this.avancerFigures(dt, son, this.coupsImage);
  }

  // Le pointeur des figures : la souris sur le canevas, sinon son centre.
  pointeurFigures() {
    return this.pointeur || [this.canvas.width / 2, this.canvas.height / 2];
  }

  // Petites figures (comme figures::Figures::avance) ; leur centre suit le pointeur des figures
  // au bout d'un ressort, avec un retard borné.
  avancerFigures(dt, son, coups) {
    const c = this.reglages, f = this.figures, t = this.t, S = this.echelle || 1;
    this.etiquette.kick = coups.kicks > 0 ? 1 : this.etiquette.kick * Math.exp(-dt * 7);
    const actives = FIGURES.map((s) => !!(c[s] && c[s].actif));
    const plus = [F_ORBE, F_VOIX, F_TRAME, F_TRAIT, F_PENDULE].some((i) => actives[i]);
    const loin = ['equerres', 'halo', 'lisere'].some((s) => c[s] && c[s].actif);
    if (!actives.some(Boolean) && !loin) { this.figures = nouvellesFigures(); return; }
    // centre : au bout d'un ressort accroché au pointeur, retard borné (en butée, l'élan est perdu)
    const cur = this.pointeurFigures(), avant = f.centre, [px, py] = f.centre || cur;
    let [ex, vx] = ressort(px - cur[0], f.vitesse[0], FIG_SUIVI[0], FIG_SUIVI[1], dt);
    let [ey, vy] = ressort(py - cur[1], f.vitesse[1], FIG_SUIVI[0], FIG_SUIVI[1], dt);
    const d = Math.hypot(ex, ey), lim = FIG_RETARD_MAX * S;
    if (d > lim) { ex = (ex / d) * lim; ey = (ey / d) * lim; vx = vy = 0; }
    f.vitesse = [vx, vy];
    f.centre = [cur[0] + ex, cur[1] + ey];
    f.retard = [ex, ey];
    f.retardMax = lim;
    f.horloge = reste(f.horloge + dt, TAU * 40);
    // ce que les figures écoutent : la basse et la montée (communes), leurs gestes (déclencheurs)
    const gestes = f.gestes;
    f.gestes = FIGURES.map(gestesVides);
    f.enveloppes = f.enveloppes.map((env, i) => env.map((e, k) => {
      const retombee = e * Math.exp(-dt * RETOMBEE_GESTES[k]);
      return gestes[i].n[k] > 0 ? Math.max(retombee, Math.min(1, forceGeste(gestes[i], k))) : retombee;
    }));
    f.montee += (Math.min(1, Math.max(0, finiOu(son.tension, 0))) - f.montee) * Math.min(1, dt * FIG_SUIVI_MONTEE);
    f.basse += (Math.min(1, Math.max(0, finiOu(son.basse, 0))) - f.basse) * (1 - Math.exp(-dt * 10));
    if (coups.temps) f.compte++;
    if (coups.drops > 0) f.compte = 0;
    f.tempo = son.bpm ? Math.min(1, f.tempo + dt / TEMPO_FONDU[0]) : Math.max(0, f.tempo - dt / TEMPO_FONDU[1]);
    if (son.bpm) f.mesure = ((f.compte % 4) + Math.min(1, Math.max(0, son.phase || 0))) / 4;
    this.poserFigures(dt, actives);
    const ecoutes = FIGURES.map((s, i) => ({ p: this.partageFigure(i), g: gestes[i] }));
    if (plus) this.avancerPlus(dt, son, coups, ecoutes);
    if (loin) this.avancerLoin(dt, coups);
    // les gouttes traînent derrière le centre quand il bouge (px de l'aperçu depuis l'image d'avant)
    const bouge = avant ? [f.centre[0] - avant[0], f.centre[1] - avant[1]] : [0, 0];
    if (actives[F_GOUTTES]) this.avancerGouttes(dt, ecoutes[F_GOUTTES], bouge);
    else { f.gouttes.segments = []; f.gouttes.bulles = []; }
    if (actives[F_ARCS]) {
      const a = f.arcs, { p, g } = ecoutes[F_ARCS];
      if (g.n[BATTEMENT] > 0) { a.cran += ARC_CRAN * g.force[BATTEMENT]; a.bond[1] += ARC_BOND * g.force[BATTEMENT]; }
      a.recul[1] -= ARC_RECUL * forceGeste(g, ACCENT);
      while (a.cran > TAU) { a.cran -= TAU; a.tour[0] -= TAU; }
      vers(a.tour, a.cran, [14, 0.4], dt, [-TAU, 2 * TAU]);
      vers(a.bond, 0, [18, 0.3], dt, ARC_BORNES);
      vers(a.recul, 0, [12, 0.35], dt, [-Math.PI, Math.PI]);
      const aigu = Math.min(1, 0.9 * p.scintillement + 0.8 * p.montee + 0.45 * p.accent);
      a.grave = p.basse;
      a.aigu += (aigu - a.aigu) * (1 - Math.exp(-dt * (aigu > a.aigu ? 30 : 6)));
    }
    if (actives[F_QUATRE]) {
      const q = f.temps, actif = son.bpm ? f.compte % 4 : null, { g } = ecoutes[F_QUATRE];
      if (actif !== q.actif) {
        q.actif = actif;
        if (actif !== null) q.pts[actif].taille[1] += 9;
      }
      if (q.actif !== null) {
        const pt = q.pts[q.actif];
        pt.saut[1] -= 150 * forceGeste(g, BATTEMENT);
        pt.tiret[1] += 60 * forceGeste(g, ACCENT);
      }
      if (g.n[APOGEE] > 0) { q.vague = t; q.force = forceGeste(g, APOGEE); for (const pt of q.pts) pt.arme = true; }
      q.pts.forEach((pt, i) => {
        if (pt.arme && t >= q.vague + i * VAGUE) { pt.saut[1] -= 230 * q.force; pt.arme = false; }
        vers(pt.saut, 0, [20, 0.3], dt, SAUT_BORNES);
        vers(pt.taille, 1, [18, 0.35], dt, TAILLE_BORNES);
        vers(pt.tiret, 0, [16, 0.4], dt, TIRET_BORNES);
      });
    }
    if (actives[F_ANNEAU]) {
      const a = f.anneau, { p, g } = ecoutes[F_ANNEAU];
      if (g.n[APOGEE] > 0) { a.echelle[1] += 10 * forceGeste(g, APOGEE); a.echos.push(t); }
      vers(a.echelle, 1 - 0.42 * p.montee, [18, 0.28], dt, ANNEAU_BORNES);
      a.echos = a.echos.filter((ne) => t - ne < ANNEAU_ECHO[0]).slice(-ANNEAU_ECHOS_MAX);
    }
  }

  // Ce qu'entend la figure `i` (comme figures::Figures::partage) : la basse et la montée, communes,
  // et l'enveloppe de ses battements, accents et scintillements.
  partageFigure(i) {
    const f = this.figures, [battement, accent, scintillement] = f.enveloppes[i];
    return { horloge: f.horloge, basse: f.basse, montee: f.montee, battement, accent, scintillement };
  }

  // Pose de chaque figure active à cette image (comme figures::Figures::poser) : son allure et ses
  // modulations. Une figure qui tourne (`tourne`) tourne de sa rotation propre plus celle des
  // modulations, le tout multiplié par leur accélération ; le trait et le pendule gardent seulement
  // épaisseur, opacité et teinte ; la trame applique elle-même la sienne (loupe, points, place).
  poserFigures(dt, actives) {
    const c = this.reglages, f = this.figures, S = this.echelle || 1;
    FIGURES.forEach((s, i) => {
      if (!actives[i]) { f.angles[i] = 0; f.poses[i] = null; f.teintes[i] = 0; return; }
      const a = allureDe(c[s]), m = this.modsFigures[i] || effetNeutre(), geo = geometrique(i);
      if (tourne(i)) {
        // rotation totale bornée comme celle du trajet de l'essaim
        const libre = (((a.rotation + m.rotation) * TAU) / 60 + m.vitesse * FIG_VITESSE) * m.acceleration;
        const w = (borneRotation((libre * 60) / TAU) * TAU) / 60;
        if (w !== 0) f.angles[i] = reste(f.angles[i] + w * dt, TAU);
      } else {
        f.angles[i] = 0;
      }
      f.poses[i] = {
        angle: f.angles[i],
        geo: geo ? Math.min(GEO_MAX, Math.max(GEO_MIN, a.echelle * m.rayon)) : 1,
        traits: m.taille,
        decalage: geo ? [(a.decalage_x + m.decalage * FIG_DECALAGE) * S, a.decalage_y * S] : [0, 0],
        alpha: a.opacite * m.opacite * m.luminosite,
      };
      f.teintes[i] = m.teinte;
    });
  }

  // Orbe, ligne de voix, trame, trait de pinceau et pendule (comme figures::plus::Plus::avance) :
  // la trame autour du centre des figures (plus sa place), le trait et le pendule au pointeur des
  // figures. `ecoutes` : ce que chaque figure entend et les gestes qu'elle a reçus.
  avancerPlus(dt, son, coups, ecoutes) {
    const c = this.reglages, f = this.figures, t = this.t, S = this.echelle || 1;
    const cur = this.pointeurFigures(), mods = (i) => this.modsFigures[i] || effetNeutre();
    const orbe = c.orbe || {}, voix = c.voix || {}, trame = c.trame || {}, pinceau = c.pinceau || {}, pendule = c.pendule || {};
    if (orbe.actif) {
      const o = f.orbe, { p, g } = ecoutes[F_ORBE], relief = mods(F_ORBE).propres.relief, rayons = () => this.rayonsOrbe(orbe, p, relief);
      if (g.n[BATTEMENT] > 0) { const k = forceGeste(g, BATTEMENT); o.souffle[1] += ORBE_ELAN * k; o.echos.push([t + 0.03, Math.min(1.5, k), rayons()]); }
      if (g.n[APOGEE] > 0) { const k = forceGeste(g, APOGEE); o.souffle[1] += 2 * ORBE_ELAN * k; for (const retard of [0.12, 0.24]) o.echos.push([t + retard, Math.min(1.5, 1.5 * k), rayons()]); }
      vers(o.souffle, 0, [17, 0.3], dt, ORBE_SOUFFLE);
      o.echos = o.echos.filter((e) => t - e[0] < ORBE_ECHO[0]).slice(-ORBE_ECHOS_MAX);
      o.contour = rayons();
    } else {
      f.orbe.contour = [];
      f.orbe.echos = [];
    }
    if (voix.actif) {
      const v = f.voix, { p, g } = ecoutes[F_VOIX];
      // grandes ondulations : la basse et le battement ; moyennes : l'accent ; fines : le
      // scintillement et la montée
      const cibles = [0.3 * p.basse + 0.85 * p.battement, (p.basse > 0.02 ? 0.15 : 0) + 0.9 * p.accent, 0.9 * p.scintillement + 0.6 * p.montee];
      v.niveaux = v.niveaux.map((n, i) => { const cible = Math.min(1, cibles[i]); return n + (cible - n) * (1 - Math.exp(-dt * (cible > n ? 30 : 9))); });
      v.large[1] += 5 * forceGeste(g, APOGEE);
      vers(v.large, 1, [12, 0.35], dt, VOIX_LARGE);
      v.silence = v.niveaux[0] + v.niveaux[1] + v.niveaux[2] < 0.03 ? v.silence + dt : 0;
      v.visible = v.silence >= VOIX_ATTENTE ? Math.max(0, v.visible - dt / VOIX_FONDU[1]) : Math.min(1, v.visible + dt / VOIX_FONDU[0]);
    } else {
      f.voix.silence = VOIX_ATTENTE;
      f.voix.visible = 0;
    }
    if (trame.actif) {
      // la loupe est à l'échelle de la figure (allure, modulation du rayon), à sa place
      const tr = f.trame, { p, g } = ecoutes[F_TRAME], pose = f.poses[F_TRAME], pas = regle(trame.pas, 6, 24, 10) * S;
      const rayon = regle(trame.rayon, 20, 120, 46) * pose.geo, cx = f.centre[0] + pose.decalage[0], cy = f.centre[1] + pose.decalage[1];
      // quelques points scintillent autour du pointeur
      for (let i = 0; i < Math.round(4 * forceGeste(g, SCINTILLEMENT)); i++) {
        const a = entre(0, TAU), d = (14 + 28 * Math.random()) * S;
        tr.eclairs.push([Math.round((cx + Math.cos(a) * d) / pas), Math.round((cy + Math.sin(a) * d) / pas), t]);
      }
      if (g.n[BATTEMENT] > 0) { const k = forceGeste(g, BATTEMENT); tr.vagues.push({ x: cx, y: cy, ne: t, force: k, vie: 0.4, losange: false }); tr.loupe[1] += 1.96 * rayon * k; }
      if (g.n[ACCENT] > 0) tr.vagues.push({ x: cx, y: cy, ne: t, force: 0.6 * forceGeste(g, ACCENT), vie: 0.34, losange: true });
      if (g.n[APOGEE] > 0) tr.vagues.push({ x: cx, y: cy, ne: t + 0.05, force: 1.8 * forceGeste(g, APOGEE), vie: 0.6, losange: false });
      tr.vagues = tr.vagues.filter((v) => t - v.ne < v.vie).slice(-TRAME_VAGUES_MAX);
      tr.eclairs = tr.eclairs.filter((e) => t - e[2] < 0.4).slice(-TRAME_ECLAIRS_MAX);
      vers(tr.loupe, rayon * (1 + 0.3 * p.basse), [15, 0.4], dt, [0, rayon * TRAME_LOUPE_MAX]);
    } else {
      f.trame.vagues = [];
      f.trame.loupe = [0, 0];
    }
    if (pinceau.actif) {
      // durée modulée (cible `duree`) : de × 0,3 à × 2,5
      const pi = f.pinceau, { g } = ecoutes[F_TRAIT], duree = regle(pinceau.duree, 0.2, 1, 0.42) * dureeModule(mods(F_TRAIT).propres.duree);
      if (g.n[BATTEMENT] > 0 && pi.perles.length < 6) pi.perles.push(t);
      if (pi.vie[0] <= 0) pi.vie = [duree, 0];
      vers(pi.vie, duree, [9, 0.45], dt, [duree * 0.5, duree * 2.2]);
      pi.perles = pi.perles.filter((ne) => t - ne < 0.5);
      // la souris sur le canevas, sinon son centre (rien à tracer)
      // un point n'est ajouté que si le pointeur a bougé d'au moins PINCEAU_PAS (comme plus.rs)
      const dernier = pi.trajet[pi.trajet.length - 1];
      if (!dernier || Math.hypot(dernier[1] - cur[0], dernier[2] - cur[1]) >= PINCEAU_PAS) pi.trajet.push([t, cur[0], cur[1]]);
      pi.trajet = pi.trajet.filter((q) => t - q[0] <= pi.vie[0]);
    } else {
      f.pinceau.trajet = [];
    }
    if (pendule.actif && dt > 1e-5) {
      const pe = f.pendule, { p, g } = ecoutes[F_PENDULE], longueurReglee = regle(pendule.longueur, 8, 60, 22);
      // la modulation du rayon allonge ou raccourcit le fil (de × 0,5 à × 2,5)
      const longueur = longueurReglee * filModule(mods(F_PENDULE).rayon);
      if (pe.fil[0] <= 0) pe.fil = [longueurReglee, 0];
      pe.fil[1] += 7.3 * longueur * forceGeste(g, APOGEE);
      vers(pe.fil, longueur * (1 + 0.32 * p.basse - 0.6 * p.montee), [12, 0.4], dt, [4, longueurReglee * FIL_MAX]);
      const l = pe.fil[0] * S, [ax, ay] = cur;
      let [qx, qy] = pe.poids || [ax, ay + l];
      if (g.n[BATTEMENT] > 0) { const k = forceGeste(g, BATTEMENT); pe.vitesse[1] -= PENDULE_SAUT[0] * S * k; pe.vitesse[0] += entre(-70, 70) * S * k; }
      if (g.n[ACCENT] > 0) { pe.cote = -pe.cote; pe.vitesse[0] += PENDULE_SAUT[1] * pe.cote * S * forceGeste(g, ACCENT); }
      if (g.n[APOGEE] > 0) pe.vitesse[1] -= 1.5 * PENDULE_SAUT[0] * S * forceGeste(g, APOGEE);
      // gravité et frottement, puis le fil : il ne s'étire pas, mais peut se détendre
      const avant = [qx, qy], frein = Math.exp(-dt * regle(pendule.frottement, 0.3, 6, 1.8));
      pe.vitesse = [pe.vitesse[0] * frein, (pe.vitesse[1] + PENDULE_GRAVITE * S * dt) * frein];
      qx += pe.vitesse[0] * dt;
      qy += pe.vitesse[1] * dt;
      const dx = qx - ax, dy = qy - ay, d = Math.hypot(dx, dy);
      if (d > l) { qx = ax + (dx / d) * l; qy = ay + (dy / d) * l; }
      pe.vitesse = [(qx - avant[0]) / dt, (qy - avant[1]) / dt];
      if (![qx, qy, pe.vitesse[0], pe.vitesse[1]].every(Number.isFinite)) { qx = ax; qy = ay + l; pe.vitesse = [0, 0]; }
      pe.poids = [qx, qy];
      pe.ancre = [ax, ay];
      pe.mou = Math.max(0, l - Math.min(d, l));
    } else if (!pendule.actif) {
      f.pendule.poids = null;
    }
  }

  // Étirement de l'anneau et de l'orbe (comme figures::Figures::etirement) : [étirement (0 à `max`),
  // cosinus, sinus de sa direction à l'écran], le plus grand de celui de la vitesse du centre et de
  // celui de son retard sur le pointeur.
  etirementFigures(S, max) {
    const f = this.figures, [vx, vy] = f.vitesse, [rx, ry] = f.retard;
    const m = Math.min(ETIREMENT_MAX, Math.max(0, max));
    const v = Math.hypot(vx, vy), d = Math.hypot(rx, ry);
    const parVitesse = Math.min(1, v / (ANNEAU_VITESSE * S)) * m;
    const parRetard = Math.min(1, d / Math.max(1, f.retardMax)) * m;
    if (parRetard > parVitesse && d > 0.5) return [parRetard, -rx / d, -ry / d];
    if (v > 1) return [parVitesse, vx / v, vy / v];
    return [0, 1, 0];
  }

  // Étirement de la figure `i`, sa direction ramenée dans le repère de la figure tournée (comme
  // figures::Figures::etirement_pose) : l'anneau reste étiré vers le pointeur même s'il tourne.
  etirementPose(i, S, max) {
    const [e, cos, sin] = this.etirementFigures(S, max), pose = this.figures.poses[i], a = pose ? -pose.angle : 0;
    const s = Math.sin(a), c = Math.cos(a);
    return [e, cos * c - sin * s, cos * s + sin * c];
  }

  // Rayons de l'orbe (comme figures::plus::Plus::rayons), en px à 100 % ; `p` : ce qu'entend l'orbe,
  // `relief` : part ajoutée à son relief par les modulations.
  rayonsOrbe(orbe, p, relief) {
    const f = this.figures, n = this.nBandes || 0, bandes = this.bandes;
    const fond = regle(orbe.rayon, 10, 60, 21) * (1 - 0.35 * p.montee) + f.orbe.souffle[0];
    const r = regle(orbe.relief, 2, 30, 9) * reliefModule(relief);
    return Array.from({ length: ORBE_POINTS }, (_, i) => {
      const th = (i / ORBE_POINTS) * TAU;
      let bande = 0;
      if (n >= 2) {
        const x = (1 - Math.abs(th / Math.PI - 1)) * (n - 1), j = Math.min(n - 2, Math.floor(x)), k = (1 - Math.cos((x - j) * Math.PI)) / 2;
        bande = Math.min(1, Math.max(0, bandes[j] + (bandes[j + 1] - bandes[j]) * k));
      }
      const lente = (0.4 + 0.7 * p.basse) * (1.3 * Math.sin(3 * th + p.horloge * 0.9) + 0.9 * Math.sin(5 * th - p.horloge * 1.3));
      return Math.max(2, fond + r * bande + lente + 1.4 * p.montee * Math.sin(14 * th + p.horloge * 22));
    });
  }

  // Dessin de la figure `i` (comme figures::Figures::dessiner_figure), autour de (cx, cy) ; `p` :
  // ce qu'elle entend, `teinte` : degrés ajoutés à sa couleur par les modulations (pour le trait).
  dessinerFigure(i, g, cx, cy, S, tete, queue, p, teinte) {
    const c = this.reglages, f = this.figures, t = this.t, propres = (this.modsFigures[i] || effetNeutre()).propres;
    g.lineCap = "round";
    g.lineJoin = "round";
    if (i === F_GOUTTES) this.dessinerGouttes(g, cx, cy, S, tete, queue);
    else if (i === F_TRAME) this.dessinerTrame(g, cx, cy, S, tete, p, f.poses[F_TRAME]);
    else if (i === F_ORBE) this.dessinerOrbe(g, cx, cy, S, tete, queue);
    else if (i === F_VOIX) this.dessinerVoix(g, cx, cy, S, tete, queue, propres.relief);
    else if (i === F_TRAIT) this.dessinerTrait(g, S, p, teinte);
    else if (i === F_PENDULE) this.dessinerPendule(g, S, tete, queue, p, f.poses[F_PENDULE]);
    else if (i === F_ARCS) {
      const a = f.arcs, arcs = c.arcs, rayon = regle(arcs.rayon, 10, 80, 20), epais = regle(arcs.epaisseur, 0.5, 6, 2.1);
      const arc = (r, depart, ouverture, largeur, rgb) => {
        g.strokeStyle = plein(rgb);
        g.lineWidth = largeur;
        g.beginPath();
        g.arc(cx, cy, Math.max(2, r), depart, depart + ouverture);
        g.stroke();
      };
      // la modulation de l'ouverture ouvre (ou ferme) les deux arcs
      const ouvre = (x) => Math.min(1, Math.max(0, x + finiOu(propres.ouverture, 0)));
      arc((rayon + a.bond[0]) * S, -Math.PI / 2 + a.tour[0] + f.horloge * 0.25, ARC_GRAND_OUVERTURE[0] + ARC_GRAND_OUVERTURE[1] * ouvre(a.grave), epais * S, tete);
      arc((rayon * ARC_PETIT[0] + a.bond[0] * 0.5) * S, Math.PI / 2 + a.recul[0] - f.horloge * 0.4, ARC_PETIT_OUVERTURE[0] + ARC_PETIT_OUVERTURE[1] * ouvre(a.aigu), epais * ARC_PETIT[1] * S, queue);
    } else if (i === F_QUATRE) {
      if (f.tempo <= 0.01) return;
      const q = f.temps, quatre = c.quatre_temps, taille = regle(quatre.taille, 1, 5, 1.6), ax = cx + SOUS_LE_POINTEUR[0] * S, ay = cy + SOUS_LE_POINTEUR[1] * S;
      const ecart = regle(quatre.ecart, 4, 20, 7.5) * ecartModule(propres.ecart) * S * (1 - 0.35 * p.montee);
      const pose = f.poses[F_QUATRE], tiret = pose ? (pose.traits > 1e-3 ? pose.geo / pose.traits : 0) : 1;
      // la modulation de la taille grossit les points
      const grossi = pose ? Math.min(TAILLE_POINTS_MAX, pose.traits) : 1;
      q.pts.forEach((pt, j) => {
        const actif = q.actif === j;
        const x = ax + (j - 1.5) * ecart + Math.sin(f.horloge * 40 + j * 2) * 0.8 * p.montee * S, y = ay + pt.saut[0] * S;
        const r = taille * S * pt.taille[0] * grossi * (actif ? POINT_ACTIF : 1), l = Math.max(0, pt.tiret[0]) * 1.1 * S, al = f.tempo * (actif ? 1 : POINT_ETEINT);
        if (l > 0.3 && tiret > 0) {
          g.strokeStyle = plein(tete, al);
          // le tiret, un point allongé, garde l'échelle du point (`geo`) et non celle des traits
          // (comme figures.rs : 2·r·geo/traits, que la pose multiplie par `traits`)
          g.lineWidth = 2 * r * tiret;
          g.beginPath();
          g.moveTo(x - l, y);
          g.lineTo(x + l, y);
          g.stroke();
        } else {
          g.fillStyle = plein(tete, al);
          g.beginPath();
          g.arc(x, y, Math.max(0.1, r), 0, TAU);
          g.fill();
        }
      });
    } else if (i === F_ANNEAU) {
      const a = f.anneau, anneau = c.anneau, r = regle(anneau.rayon, 6, 60, 13) * S * a.echelle[0], epais = regle(anneau.epaisseur, 0.5, 6, 1.4);
      for (const ne of a.echos) {
        const k = (t - ne) / ANNEAU_ECHO[0];
        if (k < 0 || k >= 1) continue;
        g.strokeStyle = plein(queue, 0.7 * (1 - k));
        g.lineWidth = 1.2 * S;
        g.beginPath();
        g.arc(cx, cy, r * (1 + ANNEAU_ECHO[1] * (1 - (1 - k) ** 3)), 0, TAU);
        g.stroke();
      }
      // étiré dans le sens où il file ; carré un instant sur l'accent, ou par la modulation de la
      // forme (superellipse : cercle à 2, presque carré à 8)
      const [e, cos, sin] = this.etirementPose(F_ANNEAU, S, regle(anneau.etirement, 0, 0.7, ETIREMENT_DEFAUT)), ra = r * (1 + e), rb = r * (1 - e);
      const puissance = 2 / (2 + 6 * Math.min(1, Math.max(0, finiOu(propres.forme, 0))));
      const point = (th) => {
        const ct = Math.cos(th), st = Math.sin(th);
        const x = ra * Math.sign(ct) * Math.abs(ct) ** puissance, y = rb * Math.sign(st) * Math.abs(st) ** puissance;
        return [cx + x * cos - y * sin, cy + x * sin + y * cos];
      };
      g.beginPath();
      for (let j = 0; j <= 48; j++) {
        const [x, y] = point((j / 48) * TAU);
        if (j) g.lineTo(x, y); else g.moveTo(x, y);
      }
      g.strokeStyle = plein(tete);
      g.lineWidth = epais * (1 + 1.07 * p.basse + 0.86 * p.montee) * S;
      g.stroke();
      if (f.tempo > 0.01) {
        const [x, y] = point(-Math.PI / 2 + f.mesure * TAU);
        g.fillStyle = plein(queue, f.tempo);
        g.beginPath();
        g.arc(x, y, (1.9 + 1.9 * p.scintillement) * S, 0, TAU);
        g.fill();
      }
    }
  }

  // Trame (comme figures::plus::Plus::dessiner_trame) autour du centre de sa loupe (centre des
  // figures plus sa place) ; `p` : ce qu'elle entend ; ses points grossissent avec la taille.
  dessinerTrame(g, cx0, cy0, S, tete, p, pose) {
    const tr = this.figures.trame, t = this.t, trame = this.reglages.trame;
    const cx = cx0 + pose.decalage[0], cy = cy0 + pose.decalage[1];
    const pas = regle(trame.pas, 6, 24, 10) * S, taille = regle(trame.taille, 0.3, 3, 1) * Math.min(TAILLE_POINTS_MAX, Math.max(0, pose.traits));
    const l = Math.max(4, tr.loupe[0]) * S, marge = l * 1.25 + 1;
    // chaque vague une fois par image (et non à chaque point) : centre, front, force restante
    const vagues = tr.vagues.map((v) => { const age = t - v.ne; return [v.x, v.y, TRAME_VAGUE[0] * S * age, v.force * Math.max(0, 1 - age / v.vie), v.losange, age]; }).filter((v) => v[5] >= 0 && v[3] > 0);
    const epaisseur = TRAME_VAGUE[1] * S;
    for (let j = Math.floor((cy - marge) / pas); j <= Math.ceil((cy + marge) / pas); j++) {
      for (let i = Math.floor((cx - marge) / pas); i <= Math.ceil((cx + marge) / pas); i++) {
        let x = i * pas, y = j * pas;
        const dx = x - cx, dy = y - cy, d = Math.hypot(dx, dy);
        // le vide sous le pointeur, puis la loupe qui s'estompe vers son bord
        const vide = lisse(TRAME_VIDE[0] * S, TRAME_VIDE[1] * S, d), dedans = (1 - lisse(l * 0.35, l, d)) * vide, large = (1 - lisse(l * 0.3, l * 1.25, d)) * vide;
        if (large <= 0.01) continue;
        let onde = 0, px = 0, py = 0;
        for (const [vx0, vy0, front, force, losange] of vagues) {
          const vx = x - vx0, vy = y - vy0, droit = Math.hypot(vx, vy), dist = losange ? (Math.abs(vx) + Math.abs(vy)) / 1.25 : droit;
          const k = gauss((dist - front) / epaisseur) * force;
          onde += k;
          if (droit > 1e-3) { px += (vx / droit) * k; py += (vy / droit) * k; }
        }
        onde = Math.min(TRAME_ONDE_MAX, onde);
        const pousse = Math.hypot(px, py);
        if (pousse > TRAME_ONDE_MAX) { px = (px / pousse) * TRAME_ONDE_MAX; py = (py / pousse) * TRAME_ONDE_MAX; }
        const eclair = tr.eclairs.reduce((m, e) => (e[0] === i && e[1] === j ? Math.max(m, Math.exp(-(t - e[2]) * 12)) : m), 0);
        x += px * TRAME_DEPLACE * S - dx * 0.3 * p.montee * dedans;
        y += py * TRAME_DEPLACE * S - dy * 0.3 * p.montee * dedans;
        const r = (dedans * (1 + 0.9 * p.basse) + 2 * onde * large + 1.5 * eclair * large) * taille * S;
        if (r < 0.25) continue;
        g.fillStyle = plein(tete, Math.min(1, 0.3 + (0.35 * r) / S));
        g.beginPath();
        g.arc(x, y, r, 0, TAU);
        g.fill();
      }
    }
  }

  // Orbe (comme figures::plus::Plus::dessiner_orbe), étirée comme l'anneau.
  dessinerOrbe(g, cx, cy, S, tete, queue) {
    const o = this.figures.orbe, t = this.t;
    if (o.contour.length !== ORBE_POINTS) return;
    const [e, ec, es] = this.etirementPose(F_ORBE, S, regle(this.reglages.orbe.etirement, 0, 0.7, ETIREMENT_DEFAUT));
    const tracer = (rayons, echelle, largeur, rgb, al) => {
      g.beginPath();
      for (let i = 0; i <= ORBE_POINTS; i++) {
        const th = -Math.PI / 2 + (i / ORBE_POINTS) * TAU, r = rayons[i % ORBE_POINTS] * echelle * S;
        let x = Math.cos(th) * r, y = Math.sin(th) * r;
        if (e !== 0) {
          // étirée le long de (ec, es), resserrée en travers
          const a = (x * ec + y * es) * (1 + e), b = (-x * es + y * ec) * (1 - e);
          [x, y] = [a * ec - b * es, a * es + b * ec];
        }
        if (i) g.lineTo(cx + x, cy + y); else g.moveTo(cx + x, cy + y);
      }
      g.strokeStyle = plein(rgb, al);
      g.lineWidth = largeur;
      g.stroke();
    };
    for (const [ne, force, rayons] of o.echos) {
      const k = (t - ne) / ORBE_ECHO[0];
      if (k < 0 || k >= 1) continue;
      tracer(rayons, 1 + ORBE_ECHO[1] * force * (1 - (1 - k) ** 3), (1.3 * (1 - k) + 0.4) * S, melange(tete, queue, k), 0.55 * (1 - k));
    }
    tracer(o.contour, 1, regle(this.reglages.orbe.epaisseur, 0.5, 5, 1.6) * S, tete, 1);
  }

  // Ligne de voix (comme figures::plus::Plus::dessiner_voix) ; `relief` : part ajoutée à la hauteur
  // de ses ondulations par les modulations.
  dessinerVoix(g, cx, cy, S, tete, queue, relief) {
    const f = this.figures, v = f.voix, voix = this.reglages.voix;
    if (v.visible <= 0.01) return;
    const hauteur = regle(voix.hauteur, 2, 20, 6.5), w = regle(voix.largeur, 20, 120, 46) * S * v.large[0], h = hauteur * reliefModule(relief) * S;
    const ax = cx + SOUS_LE_POINTEUR[0] * S, ay = cy + (SOUS_LE_POINTEUR[1] + 1.4 * hauteur) * S, [grave, medium, aigu] = v.niveaux;
    const point = (i) => {
      const u = i / VOIX_POINTS, fenetre = Math.sin(Math.PI * u) ** 2;
      const onde = grave * Math.sin(TAU * 1.5 * u + f.horloge * 5) + VOIX_ONDES[0] * medium * Math.sin(TAU * 3 * u - f.horloge * 8) + VOIX_ONDES[1] * aigu * Math.sin(TAU * 6.5 * u + f.horloge * 13);
      return [ax + (u - 0.5) * w, ay + fenetre * h * onde];
    };
    const al = v.visible * (0.5 + 0.5 * Math.min(1, grave + medium + aigu)), par = VOIX_POINTS / VOIX_MORCEAUX;
    g.lineWidth = regle(voix.epaisseur, 0.5, 5, 1.7) * S;
    for (let m = 0; m < VOIX_MORCEAUX; m++) {
      g.strokeStyle = plein(melange(tete, queue, (m + 0.5) / VOIX_MORCEAUX), al);
      g.beginPath();
      for (let i = m * par; i <= (m + 1) * par; i++) { const [x, y] = point(i); if (i > m * par) g.lineTo(x, y); else g.moveTo(x, y); }
      g.stroke();
    }
  }

  // Trait de pinceau (comme figures::plus::Plus::dessiner_trait) : le dégradé de sa couleur, de la
  // tête à la queue (`teinte` : degrés ajoutés par les modulations) ; `p` : ce qu'il entend.
  dessinerTrait(g, S, p, teinte) {
    const c = this.reglages, pi = this.figures.pinceau, t = this.t;
    if (pi.trajet.length < 2) return;
    const vie = Math.max(0.05, pi.vie[0]), largeur = regle(c.pinceau.largeur, 2, 12, 5.5);
    for (let i = 1; i < pi.trajet.length; i++) {
      const a = pi.trajet[i - 1], b = pi.trajet[i], duree = Math.max(1e-3, b[0] - a[0]);
      const v = Math.hypot(b[1] - a[1], b[2] - a[2]) / duree / S, vif = Math.min(1, Math.max(0, (v - PINCEAU_VITESSE[0]) / PINCEAU_VITESSE[1])) ** 0.8;
      if (vif < 0.02) continue;
      const k = Math.min(1, Math.max(0, 1 - (t - b[0]) / vie)), profil = Math.max(0, Math.sin(Math.PI * k ** 0.6)) ** 0.7;
      const perle = pi.perles.reduce((s, ne) => s + PINCEAU_PERLE[0] * gauss((k - (1 - (t - ne) / PINCEAU_PERLE[1])) / 0.1), 0);
      const trait = (0.6 + largeur * vif * (0.6 + 0.6 * p.basse)) * profil * Math.min(PINCEAU_PERLE[2], 1 + perle) * S;
      if (trait < 0.4) continue;
      g.strokeStyle = plein(this.couleurFigure(c.pinceau, k, teinte));
      g.lineWidth = trait;
      g.beginPath();
      g.moveTo(a[1], a[2]);
      g.lineTo(b[1], b[2]);
      g.stroke();
    }
  }

  // Pendule (comme figures::plus::Plus::dessiner_pendule) : le poids grossit au battement et avec la
  // modulation de la taille ; le fil part de son ancre (le pointeur des figures).
  dessinerPendule(g, S, tete, queue, p, pose) {
    const f = this.figures, pe = f.pendule, pendule = this.reglages.pendule;
    if (!pe.poids) return;
    const [qx, qy] = pe.poids, [ax, ay] = pe.ancre, grossi = pose ? Math.min(TAILLE_POINTS_MAX, Math.max(0, pose.traits)) : 1;
    // le fil : droit quand il est tendu, il pend quand le poids est remonté
    g.strokeStyle = plein(queue, 0.75);
    g.lineWidth = S;
    g.beginPath();
    g.moveTo(ax, ay);
    g.quadraticCurveTo((ax + qx) / 2, (ay + qy) / 2 + pe.mou * 0.6, qx, qy);
    g.stroke();
    g.fillStyle = plein(tete);
    g.beginPath();
    g.arc(qx, qy, (regle(pendule.taille, 1, 6, 2.6) + 1.8 * p.battement) * grossi * S, 0, TAU);
    g.fill();
  }

  // Couleur d'une figure de section `sec`, de sa queue (k = 0) à sa tête (1) : celle de l'essaim
  // (sa teinte plus la teinte de l'allure), une teinte fixe (la queue `degrade` degrés plus loin) ou
  // du blanc ; `decale` : degrés ajoutés par les modulations (comme figures::couleur_figure).
  couleurFigure(sec, k, decale) {
    const a = allureDe(sec), mode = sec && sec.couleur, d = finiOu(decale, 0);
    if (mode === "blanc") return [255, 255, 255];
    if (mode === "fixe") return hsl(reste(a.teinte_fixe + d + a.degrade * (1 - Math.min(1, Math.max(0, k))), 360), a.saturation, a.luminosite);
    return prisme(this.reglages.couleur, this.t, k, this.teinteImposee, this.teinte, 0, a.teinte + d);
  }

  // Équerres, halo et liseré (comme loin::Loin::avance). Dans l'aperçu, la fenêtre et l'écran sont
  // le cadre de l'aperçu : équerres à ses coins, halo le long de la fenêtre, liseré à son bord bas.
  avancerLoin(dt, coups) {
    const c = this.reglages, f = this.figures, l = f.loin, t = this.t;
    const equerres = c.equerres || {}, halo = c.halo || {}, lisere = c.lisere || {};
    const kicks = coups.kicks, caisse = coups.caisses > 0, drop = coups.drops > 0;
    l.kick = kicks > 0 ? 1 : l.kick * Math.exp(-dt * 7);
    if (equerres.actif) {
      if (kicks > 0) l.ecart[1] += EQUERRE_ELAN[0];
      if (caisse) { l.tour ^= 1; l.diagonales[l.tour] = 1; }
      if (drop) { l.ecart[1] += EQUERRE_ELAN[1]; l.bras[1] += EQUERRE_ELAN_BRAS; }
      l.diagonales = l.diagonales.map((d) => d * Math.exp(-dt * 8));
      vers(l.ecart, EQUERRE_ECART * (1 - 0.75 * f.montee) + Math.sin(t * 44) * 0.9 * f.montee, [17, 0.3], dt, EQUERRE_BORNES[0]);
      vers(l.bras, 1 + EQUERRE_BASSE * f.basse, [14, 0.4], dt, EQUERRE_BORNES[1]);
    }
    const sonLa = f.basse > 0.02 || f.montee > 0.02;
    if (halo.actif) {
      // le pointeur de l'aperçu est au centre : au plus près du haut de la fenêtre, en son milieu
      const [x0, , x1] = this.fenetreApercu();
      l.frisson = caisse ? 1 : l.frisson * Math.exp(-dt * 12);
      avancerFilet(l.halo, t, dt, (x1 - x0) / 2, kicks > 0, drop, sonLa || l.frisson > 0.02, this.doux);
    } else {
      l.halo = nouveauFilet();
      l.frisson = 0;
    }
    if (lisere.actif) avancerFilet(l.lisere, t, dt, this.canvas.width / 2, kicks > 0, drop, sonLa, this.doux);
    else l.lisere = nouveauFilet();
  }

  // La « fenêtre » de l'aperçu : son cadre, rentré de quoi laisser les équerres s'écarter.
  fenetreApercu() {
    const m = (EQUERRE_BORNES[0][1] + 6) * (this.echelle || 1);
    return [m, m, this.canvas.width - m, this.canvas.height - m];
  }

  // Dessin des équerres, du halo et du liseré (comme loin::Loin::dessiner), dans le cadre de l'aperçu.
  dessinerLoin(g, S, tete, queue) {
    const c = this.reglages, f = this.figures, l = f.loin, t = this.t, w = this.canvas.width, h = this.canvas.height;
    const equerres = c.equerres || {}, halo = c.halo || {}, lisere = c.lisere || {};
    const [x0, y0, x1, y1] = this.fenetreApercu();
    if (equerres.actif) {
      const e = l.ecart[0] * S, bras = regle(equerres.bras, 4, 30, 10) * l.bras[0] * S;
      g.lineWidth = regle(equerres.epaisseur, 1, 5, 2) * S;
      g.lineCap = "round";
      g.lineJoin = "round";
      [[x0, y0, -1, -1, 0], [x1, y0, 1, -1, 1], [x1, y1, 1, 1, 0], [x0, y1, -1, 1, 1]].forEach(([x, y, sx, sy, diag]) => {
        const px = x + sx * e, py = y + sy * e;
        g.strokeStyle = plein(tete, 0.5 + (this.doux ? 0.25 : 0.5) * Math.max(l.diagonales[diag], l.kick));
        g.beginPath();
        g.moveTo(px - sx * bras, py);
        g.lineTo(px, py);
        g.lineTo(px, py - sy * bras);
        g.stroke();
      });
    }
    // filets : la teinte va de celle des têtes à celle des queues le long du filet, et glisse
    const teinte = (part) => 0.5 + 0.5 * Math.sin(part * TAU + reste(t * 0.5, TAU));
    const couleur = (k, alpha) => plein(tete.map((v, i) => v + (queue[i] - v) * k), alpha);
    const pas = FILET_PAS * S;
    g.lineCap = "round";
    if (halo.actif && l.halo.presence > 0) {
      const W = x1 - x0, H = y1 - y0, tour = 2 * (W + H), u = W / 2;
      const ep = regle(halo.epaisseur, 1, 4, 2) * S, profondeur = regle(halo.profondeur, 6, 30, 14) * S;
      const ecart = (a, b) => { const d = Math.abs(a - b) % tour; return Math.min(d, tour - d); };
      // les quatre côtés : départ, direction, longueur, sens vers l'intérieur, abscisse du départ
      const cotes = [[[x0, y0], [1, 0], W, [0, 1], 0], [[x1, y0], [0, 1], H, [-1, 0], W], [[x1, y1], [-1, 0], W, [0, -1], W + H], [[x0, y1], [0, -1], H, [1, 0], 2 * W + H]];
      for (const [depart, dir, long, dedans, u0] of cotes) {
        const point = (s, r) => [depart[0] + dir[0] * s + dedans[0] * r, depart[1] + dir[1] * s + dedans[1] * r];
        const niveau = (s) => {
          const m = u0 + s;
          const force = forceFilet(l.halo, t, S, f.basse, f.montee, HALO_LUEUR * tour, ecart(m, u), (v) => ecart(m, v)) + HALO_FRISSON * l.frisson;
          return [force * l.halo.presence, teinte(m / tour)];
        };
        suitesFilet(long, pas, niveau, (a, b, force, k) => {
          bande(g, point(a, ep / 2), point(b, ep / 2), ep, couleur(k, Math.min(1, 0.15 + force)));
          const fond = profondeur;
          for (let n = 0; n < HALO_NAPPES; n++) {
            // coupée en onglet aux coins : les nappes de deux côtés ne s'y superposent pas
            const r = (n + 0.5) / HALO_NAPPES, rentre = ep + r * fond, a1 = Math.max(a, rentre), b1 = Math.min(b, long - rentre);
            if (b1 > a1) bande(g, point(a1, rentre), point(b1, rentre), fond / HALO_NAPPES + 1, couleur(k, HALO_OPACITE * force * (1 - r) * (1 - r)));
          }
        });
      }
    }
    if (lisere.actif && l.lisere.presence > 0) {
      const ep = regle(lisere.epaisseur, 1, 8, 3) * S, y = h - ep / 2, px = w / 2;
      const lueur = LISERE_LUEUR[0] * S + LISERE_LUEUR[1] * w * f.basse;
      const niveau = (m) => [forceFilet(l.lisere, t, S, f.basse, f.montee, lueur, m - px, (u) => Math.abs(m - u)) * l.lisere.presence, teinte(m / w)];
      suitesFilet(w, pas, niveau, (a, b, force, k) => bande(g, [a, y], [b, y], ep, couleur(k, Math.min(1, 0.15 + force))));
    }
  }

  // Gouttes en contour (comme figures::gouttes::Gouttes::avance) ; `p`, `g` : ce qu'elles entendent
  // et les gestes reçus ; `bouge` : déplacement du centre depuis l'image d'avant (px de l'aperçu).
  avancerGouttes(dt, { p, g }, bouge) {
    const go = this.figures.gouttes, t = this.t, c = this.reglages.gouttes || {}, S = this.echelle || 1;
    const m = this.modsFigures[F_GOUTTES] || effetNeutre();
    const tire = ([a, b]) => a + (b - a) * Math.random();
    const nombre = Math.min(GOUTTE_PERLES.length, Math.max(0, regle(c.perles, 0, 8, 4) + finiOu(m.propres.nombre, 0) * GOUTTE_PERLES_MODULE));
    const visible = (j) => Math.min(1, Math.max(0, nombre - j));
    for (let i = 0; i < g.n[BATTEMENT]; i++) {
      const k = forceGeste(g, BATTEMENT);
      go.noyau[1] += GOUTTE_NOYAU_ELAN[0] * k;
      for (const q of go.perles) q.ecart[1] += tire(GOUTTE_PERLE_ELAN[0]) * k;
    }
    if (g.n[APOGEE] > 0) {
      const k = forceGeste(g, APOGEE);
      go.noyau[1] += GOUTTE_NOYAU_ELAN[1] * k;
      for (const q of go.perles) q.ecart[1] += GOUTTE_PERLE_ELAN[1] * k;
      for (let i = 0; i < GOUTTE_BULLES; i++) {
        const a = (i * TAU) / GOUTTE_BULLES + tire([-0.3, 0.3]), v = tire(GOUTTE_BULLE_VITESSE) * Math.min(k, 1);
        go.bulles.push({ x: 0, y: 0, vx: Math.cos(a) * v, vy: Math.sin(a) * v, r: tire(GOUTTE_BULLE_RAYON), ne: t, vie: tire(GOUTTE_BULLE_VIE) });
      }
    }
    // accent : une goutte se détache d'une perle visible et file vers le large
    if (g.n[ACCENT] > 0 && nombre >= 1) {
      const j = Math.min(Math.floor(nombre) - 1, Math.floor(Math.random() * Math.floor(nombre))), [orbite, , r] = GOUTTE_PERLES[j], q = go.perles[j];
      const d = orbite + q.ecart[0], x = Math.cos(q.angle) * d + q.traine.x[0], y = Math.sin(q.angle) * d + q.traine.y[0];
      const v = GOUTTE_DETACHEE[0] * Math.min(1, forceGeste(g, ACCENT));
      q.ecart[1] += 60 * forceGeste(g, ACCENT);
      go.bulles.push({ x, y, vx: Math.cos(q.angle) * v, vy: Math.sin(q.angle) * v, r: r * GOUTTE_DETACHEE[1], ne: t, vie: GOUTTE_DETACHEE[2] });
    }
    go.bulles = go.bulles.slice(-GOUTTE_BULLES_MAX);
    const noyau = GOUTTE_NOYAU[0] + GOUTTE_NOYAU[1] * p.basse + GOUTTE_NOYAU[2] * p.montee + GOUTTE_NOYAU_MODULE * finiOu(m.propres.noyau, 0);
    vers(go.noyau, noyau, [16, 0.35], dt, GOUTTE_NOYAU_BORNES);
    const vitesse = regle(c.vitesse, 0, 3, 1) * (1 + 1.5 * p.basse);
    go.perles.forEach((q, i) => {
      const [orbite, w] = GOUTTE_PERLES[i];
      q.angle = reste(q.angle + w * vitesse * dt, TAU);
      vers(q.ecart, 0, [9, 0.32], dt, [-orbite, GOUTTE_PERLE_ECART_MAX]);
    });
    // traîne liquide : le centre a bougé, les gouttes en gardent une part en retard (les perles du
    // large plus que celles près du noyau), les bulles restent où elles sont
    const k = Math.max(1e-3, S * regle(c.taille, 0.6, 1.6, 1)), u = [bouge[0] / k, bouge[1] / k];
    const souplesse = regle(c.souplesse, 0, 1, 0.5), max = GOUTTE_TRAINE[0] * souplesse;
    const trainer = (tr, part, w, borne) => {
      tr.x[0] -= u[0] * part;
      tr.y[0] -= u[1] * part;
      vers(tr.x, 0, [w, GOUTTE_TRAINE[3]], dt, [-borne, borne]);
      vers(tr.y, 0, [w, GOUTTE_TRAINE[3]], dt, [-borne, borne]);
    };
    trainer(go.traine, GOUTTE_TRAINE[1] * souplesse, GOUTTE_TRAINE[2], max * GOUTTE_TRAINE[1]);
    go.perles.forEach((q, i) => { const orbite = GOUTTE_PERLES[i][0]; trainer(q.traine, souplesse * (0.5 + orbite / 70), GOUTTE_TRAINE[2] - orbite / 4, max); });
    const frein = Math.exp(-dt * GOUTTE_BULLE_FREIN);
    for (const b of go.bulles) { b.x += b.vx * dt - u[0]; b.y += b.vy * dt - u[1]; b.vx *= frein; b.vy *= frein; }
    go.bulles = go.bulles.filter((b) => t - b.ne < b.vie && Math.max(Math.abs(b.x), Math.abs(b.y)) < GOUTTE_DEMI);
    // les gouttes de l'image : [x, y, rayon] autour du centre, gardées dans la grille, puis leur
    // contour commun
    const dans = (x, y, r) => { const lim = Math.max(0, GOUTTE_DEMI - r - 3); return [Math.min(lim, Math.max(-lim, x)), Math.min(lim, Math.max(-lim, y)), r]; };
    const ecart = ecartModule(m.propres.ecart);
    const gouttes = [dans(go.traine.x[0], go.traine.y[0], Math.max(GOUTTE_NOYAU_BORNES[0], go.noyau[0]))];
    go.perles.forEach((q, i) => {
      const [orbite, , rayon] = GOUTTE_PERLES[i], r = rayon * visible(i);
      if (r < 0.5) return;
      const d = Math.max(0, orbite * ecart * (1 - GOUTTE_PERLE_MONTEE * p.montee) + q.ecart[0]);
      gouttes.push(dans(Math.cos(q.angle) * d + q.traine.x[0], Math.sin(q.angle) * d + q.traine.y[0], r));
    });
    for (const b of go.bulles) gouttes.push(dans(b.x, b.y, b.r * (1 - Math.min(1, Math.max(0, (t - b.ne) / b.vie))) ** 0.7));
    go.segments = contourGouttes(gouttes);
  }

  // Dessin des gouttes (comme figures::gouttes::Gouttes::dessiner) : couleur des têtes au centre,
  // des queues au large.
  dessinerGouttes(g, cx, cy, S, tete, queue) {
    const c = this.reglages.gouttes;
    const k = regle(c.taille, 0.6, 1.6, 1) * S;
    g.lineWidth = regle(c.epaisseur, 1, 4, 1.4) * S;
    g.lineCap = "round";
    for (const [a, b] of this.figures.gouttes.segments) {
      const loin = Math.min(1, Math.hypot(a[0] + b[0], a[1] + b[1]) / 2 / GOUTTE_DEGRADE);
      g.strokeStyle = `rgb(${tete.map((v, i) => Math.round(v + (queue[i] - v) * loin)).join(",")})`;
      g.beginPath();
      g.moveTo(cx + a[0] * k, cy + a[1] * k);
      g.lineTo(cx + b[0] * k, cy + b[1] * k);
      g.stroke();
    }
  }

  // Étiquette du morceau (comme etiquette::Etiquette::dessiner) : dans l'aperçu, un titre d'exemple,
  // montré par « Piste » et quand l'étiquette vient d'être allumée.
  dessinerEtiquette(g, cx, cy, S, tete) {
    const c = this.reglages.etiquette || {};
    if (!c.actif) { this.etiquette.allumee = false; return; }
    if (!this.etiquette.allumee) { this.etiquette.allumee = true; this.etiquette.debut = this.t; }
    const duree = regle(c.duree, 1.5, 8, 3), age = this.t - this.etiquette.debut;
    const entree = lisse(0, ETIQUETTE_ENTREE, age), a = entree * (1 - lisse(duree, duree + ETIQUETTE_SORTIE, age));
    if (!(a > 0)) return;
    const T = regle(c.taille, 9, 20, 12) * S, titre = `700 ${T}px "Segoe UI", sans-serif`;
    // à droite du pointeur ; à gauche si le titre sortirait du cadre (comme Etiquette::placer)
    g.font = titre;
    const large = g.measureText("Titre du morceau").width, droite = cx + ETIQUETTE_DECALAGE[0] * S;
    const x = (droite + large > this.canvas.width ? Math.max(0, cx - ETIQUETTE_DECALAGE[0] * S - large) : droite) - ETIQUETTE_GLISSE * (1 - entree) * S;
    const y = cy + ETIQUETTE_DECALAGE[1] * S;
    // blanc cerné de sombre : lisible sur un fond clair comme sur un fond sombre
    const ecrire = (texte, fonte, yy, alpha) => {
      g.font = fonte;
      g.lineWidth = 2 * ETIQUETTE_CERNE * S;
      g.strokeStyle = `rgba(0,0,0,${ETIQUETTE_CERNE_OPACITE * alpha})`;
      g.strokeText(texte, x, yy);
      g.fillStyle = `rgba(255,255,255,${alpha})`;
      g.fillText(texte, x, yy);
      return g.measureText(texte).width;
    };
    g.textAlign = "left";
    g.textBaseline = "top";
    g.lineJoin = "round";
    const largeur = ecrire("Titre du morceau", titre, y, a);
    ecrire("Artiste", `600 ${T * ETIQUETTE_ARTISTE[0]}px "Segoe UI", sans-serif`, y + T * 1.33, ETIQUETTE_ARTISTE[1] * a);
    // filet sous le texte : il se déploie à l'entrée et bat avec le kick
    const epais = (ETIQUETTE_FILET[0] + ETIQUETTE_FILET[1] * this.etiquette.kick) * S, yf = y + T * (1.33 + 1.33 * ETIQUETTE_ARTISTE[0]) + ETIQUETTE_FILET_ECART * S + epais / 2;
    g.lineCap = "round";
    g.lineWidth = epais;
    g.strokeStyle = `rgba(${tete.map(Math.round).join(",")},${a})`;
    g.beginPath();
    g.moveTo(x, yf);
    g.lineTo(x + largeur * entree, yf);
    g.stroke();
  }

  // Dessin des figures autour de leur centre (comme figures::Figures::dessiner) : chacune dans sa
  // pose (tournée, agrandie et déplacée autour du centre, traits épaissis, opacité) et sa couleur.
  dessinerFigures(g, S) {
    const c = this.reglages, f = this.figures, t = this.t;
    if (!f.centre) return;
    const [cx, cy] = f.centre;
    for (const i of ORDRE_FIGURES) {
      const sec = c[FIGURES[i]];
      if (!sec || !sec.actif) continue;
      const teinte = f.teintes[i] || 0, p = this.partageFigure(i);
      // la trame applique elle-même sa pose (loupe, points, place) : elle reste fixe à l'écran
      const pose = i === F_TRAME && f.poses[i] ? { angle: 0, geo: 1, traits: 1, decalage: [0, 0], alpha: f.poses[i].alpha } : f.poses[i];
      const couleur = (k) => this.couleurFigure(sec, k, teinte);
      if (poseNeutre(pose)) {
        this.dessinerFigure(i, g, cx, cy, S, couleur(1), couleur(0), p, teinte);
        continue;
      }
      g.save();
      g.translate(cx + pose.decalage[0], cy + pose.decalage[1]);
      g.rotate(pose.angle);
      g.scale(pose.geo, pose.geo);
      g.translate(-cx, -cy);
      this.dessinerFigure(i, contextePose(g, pose), cx, cy, S, couleur(1), couleur(0), p, teinte);
      g.restore();
    }
  }

  // Repère de place (grand aperçu, panneau d'une figure active) : une croix au pointeur de l'essaim
  // et, en pointillés fins, le cadre de la zone de la figure une fois posée (échelle, décalage ; si elle
  // tourne, le carré que balaie sa zone). Dessiné par-dessus l'image déjà faite, sans rien changer
  // au calcul : ni les vignettes ni l'export GIF (pris avant) ne l'ont. `couleur` : couleur CSS.
  dessinerRepere(section, couleur) {
    const c = this.reglages, i = FIGURES.indexOf(section);
    if (!c || i < 0 || !(c[section] && c[section].actif)) return;
    const g = this.ctx, S = this.echelle || 1, d = window.devicePixelRatio || 1;
    // croix ouverte au centre : ses bras partent du bord du point qui figure le pointeur
    // traits fins posés au milieu des pixels : nets, sans être fondus sur deux
    const net = (v) => Math.round(v - d / 2) + d / 2;
    const cx = net(this.canvas.width / 2), cy = net(this.canvas.height / 2), vide = 3.5 * S, bras = vide + 6 * d;
    g.save();
    g.strokeStyle = couleur;
    g.lineWidth = d;
    g.lineCap = "butt";
    g.setLineDash([]);
    g.beginPath();
    for (const [ux, uy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
      g.moveTo(cx + ux * vide, cy + uy * vide);
      g.lineTo(cx + ux * bras, cy + uy * bras);
    }
    g.stroke();
    let z = zoneFigure(i, c[section]);
    if (z) {
      if (geometrique(i)) {
        const a = allureDe(c[section]);
        if (a.rotation !== 0) {
          const r = Math.max(...[z[0], z[2]].flatMap((x) => [z[1], z[3]].map((y) => Math.hypot(x, y))));
          z = [-r, -r, r, r];
        }
        z = [z[0] * a.echelle + a.decalage_x, z[1] * a.echelle + a.decalage_y, z[2] * a.echelle + a.decalage_x, z[3] * a.echelle + a.decalage_y];
      }
      // autour du centre des figures, qui suit la souris avec du retard ; le pendule, à son ancre
      const [zx, zy] = (geometrique(i) && this.figures.centre) || this.pointeurFigures();
      g.setLineDash([3 * d, 3 * d]);
      const x0 = net(zx + z[0] * S), y0 = net(zy + z[1] * S);
      g.strokeRect(x0, y0, net(zx + z[2] * S) - x0, net(zy + z[3] * S) - y0);
    }
    g.restore();
  }

  // Bandes de l'anneau de spectre : simulées, puis lissées comme effets::lisser_bandes.
  spectre(dt, son) {
    const s = this.reglages.spectre || {}, orbe = this.reglages.orbe || {};
    if (!s.actif && !orbe.actif) { this.nBandes = 0; return; }
    if (this.aigusSpectre !== null && (son.aigus || 0) !== this.aigusSpectre) this.eclatAigus = 1;
    this.aigusSpectre = son.aigus || 0;
    this.eclatAigus *= Math.exp(-dt * 12);
    const n = s.actif ? Math.min(BANDES_MAX, Math.max(8, Math.round(s.bandes))) : 16;
    this.nBandes = n;
    const lues = bandesSimulees(n, this.t, son, this.env, this.eclatAigus);
    const km = Math.min(1, dt * SPECTRE_MONTEE), kd = Math.min(1, dt * SPECTRE_DESCENTE);
    for (let i = 0; i < n; i++) this.bandes[i] += (lues[i] - this.bandes[i]) * (lues[i] > this.bandes[i] ? km : kd);
  }

  // Sillage : poussière le long du trajet de la souris quand elle file vite (comme
  // Swarm::update), une fois par image ; les grains naissent à la fin de l'image.
  sillage(dt) {
    const c = this.reglages, S = this.echelle || 1, cur = this.pointeur, avant = this.pointeurAvant;
    this.pointeurAvant = cur;
    if (!c.sillage || !c.sillage.actif || !cur || !avant || !(dt > 0)) { this.resteSillage = 0; return; }
    const dx = cur[0] - avant[0], dy = cur[1] - avant[1], dist = Math.hypot(dx, dy);
    if (!(dist / Math.max(dt, 1e-3) > 700 * S && dist < 600 * S)) { this.resteSillage = 0; return; }
    // la part fractionnaire des grains passe à l'image suivante (comme `reste_sillage`) : à haute
    // cadence, chaque image n'en voit qu'une fraction ; au plus 14 grains par image
    this.resteSillage = (this.resteSillage || 0) + (Math.min(10, Math.max(1, c.sillage.densite)) * dist) / (60 * S);
    const n = Math.min(14, Math.max(0, Math.floor(this.resteSillage)));
    this.resteSillage = Math.min(this.resteSillage - n, 1);
    for (let i = 0; i < n; i++) {
      const f = (i + Math.random()) / Math.max(n, 1);
      const a = entre(0, TAU), v = entre(5, 30) * S;
      const vie = entre(0.35, 0.7), taille = entre(0.7, 1.7) * S;
      this.particules.push({
        x: avant[0] + dx * f, y: avant[1] + dy * f, vx: Math.cos(a) * v - (dx / dt) * 0.05, vy: Math.sin(a) * v - (dy / dt) * 0.05,
        ne: this.t + dt, vie, taille, genre: "poussiere",
      });
    }
  }

  // Satellites nés à `t` (comme Swarm::ajouter_satellites), dans la limite du maximum. En mode
  // ressorts, chacun naît au centre, au bout d'un ressort à lui.
  ajouterSatellites(n, t, maximum) {
    const ressorts = !!(this.reglages.ressorts || {}).actif;
    for (let j = 0; j < n && this.sats.length < maximum; j++) {
      const r = entre(0.75, SAT_R_MAX);
      this.sats.push({
        a: entre(0, TAU), r, mul: entre(0.7, 1.3), ne: t, trainee: [], couleur: this.prochaineCouleur++,
        rr: ressorts ? 0 : r, rv: 0, w: ressorts ? entre(SAT_RESSORT[0], SAT_RESSORT[1]) : SAT_RESSORT[0],
      });
    }
  }

  // Une image du vrai essaim (Swarm::update), dans le même ordre ; le pointeur est au centre.
  pas(dt, son) {
    const c = this.reglages, e = c.essaim, o = c.orbite;
    this.t += dt;
    const t = this.t;
    const S = this.echelle || 1;
    const cx = this.canvas.width / 2, cy = this.canvas.height / 2, p = [cx, cy];
    // événements du son : chacun fait agir ses déclencheurs, dans l'ordre de render.rs
    const coups = this.ecoute.lire(son);
    // gardés pour les petites figures, qui avancent une fois par image
    const ci = this.coupsImage;
    ci.kicks += coups.kicks; ci.caisses += coups.caisses; ci.aigus += coups.aigus; ci.drops += coups.drops;
    ci.temps = ci.temps || coups.temps;
    for (let i = 0; i < coups.kicks; i++) this.declencher("kick", p, t);
    this.env *= Math.exp(-dt * 9);
    if (coups.temps) {
      this.declencher("temps", p, t);
      if (coups.pause) this.declencher("break", p, t);
    }
    if (coups.caisses > 0) this.declencher("caisse", p, t);
    if (coups.drops > 0) this.declencher("drop", p, t);
    if (coups.contretemps) this.declencher("contretemps", p, t);
    for (let i = 0; i < coups.aigus; i++) this.declencher("charleston", p, t);
    // particules : avancent en ralentissant, puis s'éteignent (les plus vieilles en trop s'en vont)
    const frein = Math.exp(-dt * 3);
    for (const q of this.particules) { q.x += q.vx * dt; q.y += q.vy * dt; q.vx *= frein; q.vy *= frein; }
    this.particules = this.particules.filter((q) => t - q.ne < q.vie);
    if (this.particules.length > MAX_PARTICULES) this.particules.splice(0, this.particules.length - MAX_PARTICULES);
    const vieFissure = Math.max(0.2, (c.fissures || {}).duree || 3);
    this.fissures = this.fissures.filter((f) => t - f.ne <= vieFissure);
    // note de la basse ; ce que la basse, la note grave, la montée… changent passe par les
    // modulations
    if (son.hauteur > 0) this.hauteur += (son.hauteur - this.hauteur) * Math.min(dt * 25, 1);
    const grave = Math.max(0, Math.min(1, (80 - this.hauteur) / 50));
    const presence = Math.max(0, Math.min(1, son.basse * 2.5));
    const liste = (c.modulations || {}).liste || [];
    // forme d'un trajet changée : les traînées de ce groupe repartent de zéro ; un rayon qu'on
    // glisse, une modulation ajoutée les laissent (comme render::Swarm::formes)
    const formeSats = !e.forme || e.forme === "cometes" ? o.forme : e.forme;
    const formesAvant = this.formes || [];
    if (formesAvant[0] !== o.forme) this.trainees.forEach(viderTrainee);
    if (formesAvant[1] !== formeSats) for (const q of this.sats.concat(this.fixes)) viderTrainee(q.trainee);
    this.formes = [o.forme, formeSats];
    // modulation ajoutée, retirée ou déplacée : les valeurs lissées repartent de zéro (comme
    // render::Swarm::cles_modulations), comparées sur place
    const cles = this.clesModulations;
    const memes = cles.length === liste.length && liste.every((m, i) => cles[i][0] === m.source && cles[i][1] === m.cible && cles[i][2] === (m.sur || "tout"));
    if (!memes) {
      this.clesModulations = liste.map((m) => [m.source, m.cible, m.sur || "tout"]);
      this.valeurs = [];
    }
    // modulations : valeur visée de chacune (source, seuil, à l'envers), lissée à son rythme
    this.avancerSources(dt, [coups.kicks, coups.caisses, coups.aigus, coups.drops, coups.contretemps ? 1 : 0], son.tension || 0);
    this.sonVu = son;
    this.grave = grave;
    this.presence = presence;
    this.valeurs.length = liste.length;
    liste.forEach((m, i) => {
      const vise = visee(m, this.valeurSource(m.source));
      this.valeurs[i] = finiOu((this.valeurs[i] || 0) + (vise - (this.valeurs[i] || 0)) * partLissage(m.lissage ?? LISSAGE, dt), 0);
    });
    const effets = effetsModulations(liste, this.valeurs);
    this.modsAvant = this.mods;
    this.mods = effets.groupes;
    this.modsFigures = effets.figures;
    this.teinte = effets.teinte;
    // vitesse du trajet de chaque groupe, accélérée (montée…), lissée
    this.spd = this.spd.map((v, g) => finiOu(v + (o.vitesse_base * this.mods[g].acceleration - v) * Math.min(dt * 10, 1), o.vitesse_base));
    // ressorts : la poussée et le frein retombent ; le rayon de chaque groupe est son rayon modulé
    // plus l'écart d'un ressort, que les rebonds lancent ; en mode ressorts, tout le rayon passe
    // par ce ressort (il suit sa cible, tremblement de la montée compris)
    const re = c.ressorts || {}, ru = c.rubans || {};
    this.pousse *= Math.exp(-dt * POUSSEE[1]);
    this.frein *= Math.exp(-dt * FREIN[1]);
    const raideur = Math.min(30, Math.max(6, finiOu(+re.raideur, 19))), amorti = Math.min(1, Math.max(0.1, finiOu(+re.amorti, 0.26)));
    const facteursAvant = this.facteurs;
    this.facteurs = [0, 1].map((g) => {
      let [ecart, v] = this.rebond[g], cible = this.mods[g].rayon;
      if (re.actif) {
        cible += TREMBLE[0] * Math.min(1, Math.max(0, son.tension || 0)) * Math.sin(t * TREMBLE[1]);
        ecart += this.cibles[g] - cible;
      }
      this.rebond[g] = ressort(ecart, v, raideur, amorti, dt);
      this.cibles[g] = cible;
      return Math.max(0.05, cible + this.rebond[g][0]);
    });
    // plusieurs contours (une image, une icône, un texte) : un anneau par contour, à sa place
    const contoursCometes = contoursDe(o, o.forme), nContours = Math.max(1, contoursCometes.length);
    const nAnneaux = nContours > 1 ? nContours : Math.max(1, o.anneaux), nombre = Math.max(1, o.nombre);
    // étendue bornée (comme Swarm::update) : le point le plus loin de chaque groupe, décalage et
    // retard du centre compris, reste à moins de la moitié de l'étendue ; à l'échelle 100 %,
    // l'aperçu n'ayant pas celle de l'écran
    const rayon = o.rayon * S;
    const lim = RETARD_CENTRE_MAX * (1 + 5 * Math.min(1, Math.max(0, finiOu(+o.inertie, 0))));
    const dispo = Math.max(0, ETENDUE_MAX / 2 - MARGE - lim) * S;
    const portees = [
      etendueForme(o.forme) * (nContours > 1 ? 1 : anneau(nAnneaux - 1)[0]),
      etendueForme(formeSats) * SAT_R_MAX * ((c.ressorts || {}).actif ? SAT_DEPASSEMENT : 1),
    ];
    portees.forEach((portee, g) => {
      [this.facteurs[g], this.mods[g].decalage] = borneEtendue(this.facteurs[g], this.mods[g].decalage, rayon, portee, dispo);
    });
    // vrille des rubans : un cran de plus à chaque caisse claire, rejoint en douceur
    if (coups.caisses > 0 && ru.actif) this.vrille[0] += VRILLE[2];
    this.vrille[1] += (this.vrille[0] - this.vrille[1]) * (1 - Math.exp(-dt * 12));
    if (this.vrille[1] > TAU) this.vrille = [this.vrille[0] - TAU, this.vrille[1] - TAU];
    // comètes : `nombre` par anneau, réparties sur le trajet
    while (this.angles.length < nAnneaux) this.angles.push(0);
    this.angles.length = nAnneaux;
    // anneaux ou comètes par anneau changés : chaque traînée appartiendrait à une autre comète ;
    // elles repartent de zéro (comme Swarm::update)
    if (this.trainees.length !== nAnneaux * nombre) this.trainees = Array.from({ length: nAnneaux * nombre }, () => []);
    const vie = Math.max(0.01, o.trainee) * this.mods[0].trainee;
    // une pose par groupe (render::Pose) : rotation du trajet, rayon et décalage au début et à la
    // fin de l'image ; les points intermédiaires suivent la vraie forme, coins compris
    // ce qui tourne avec le trajet (comme Groupe::inclut ; un autre groupe vaut « tout », comme
    // Config::clamped)
    const sur = ["tout", "cometes", "satellites"].includes(o.rotation_sur) ? o.rotation_sur : "tout";
    const pose = (g, rot) => {
      const base = sur === "tout" || sur === (g === 1 ? "satellites" : "cometes") ? o.rotation || 0 : 0;
      const drot = finiOu(pasRotation(finiOu(rot, 0), borneRotation(base + this.mods[g].rotation), dt), 0);
      return {
        rot0: finiOu(rot, 0), drot, rayon0: rayon * facteursAvant[g], rayon: rayon * this.facteurs[g],
        dx0: this.modsAvant[g].decalage * rayon, dx: this.mods[g].decalage * rayon,
      };
    };
    const poses = [pose(0, this.rot), pose(1, this.rotSats)];
    this.rot = reste(poses[0].rot0 + poses[0].drot, TAU);
    this.rotSats = reste(poses[1].rot0 + poses[1].drot, TAU);
    const point = (q, forme, contour, a, rel, f) => {
      const [x, y] = chemin(forme, a, contoursDe(o, forme), contour);
      const th = q.rot0 + q.drot * f, cos = Math.cos(th), sin = Math.sin(th);
      const rr = (q.rayon0 + (q.rayon - q.rayon0) * f) * rel;
      return [cx + q.dx0 + (q.dx - q.dx0) * f + (x * cos - y * sin) * rr, cy + (x * sin + y * cos) * rr];
    };
    const duree = Math.max(0.1, e.duree);
    this.sats = this.sats.filter((s) => t - s.ne < duree);
    // satellites permanents : répartis une fois pour toutes (nombre d'or), toujours les mêmes
    if (this.fixes.length !== e.fixes) {
      this.fixes = Array.from({ length: e.fixes }, (_, j) => {
        const u = (j * 0.618034) % 1;
        const r = 0.6 + u * (SAT_R_MAX - 0.6);
        return { a: j * 2.399963, r, mul: 0.6 + 0.8 * (1 - u), ne: Infinity, trainee: [], couleur: j, rr: r, rv: 0, w: SAT_RESSORT[0] };
      });
    }
    // part de chaque traînée dans le budget de points commun
    const plafond = this.plafondTrainee();
    // vitesse sur le trajet ; lissée, elle borne la vie utile des traînées (TOURS_TRAINEE tours de trajet)
    const wCometes = this.dir * this.vitesse(0), wSats = this.dir * this.vitesse(1);
    this.wLisse = this.wLisse.map((w, g) => {
      const v = Math.abs(g === 0 ? wCometes : wSats), lisse = w + (v - w) * (1 - Math.exp(-dt * 2));
      return Number.isFinite(lisse) ? lisse : v;
    });
    this.vies.length = nAnneaux;
    // les écarts de render.rs sont en pixels d'écran : ici, à l'échelle de l'aperçu
    const finCometes = [FINESSE_COMETES[0], FINESSE_COMETES[1] * S], finSats = [FINESSE_SATELLITES[0], FINESSE_SATELLITES[1] * S];
    for (let k = 0; k < nAnneaux; k++) {
      const [rk, wk] = nContours > 1 ? [1, 1] : anneau(k);
      const contour = k % nContours;
      const a0 = this.angles[k], da = wCometes * wk * dt;
      this.angles[k] = reste(a0 + da, TAU);
      const vieK = vieUtile(vie, this.wLisse[0] * wk);
      this.vies[k] = vieK;
      const n = sousPas(da, poses[0].rayon * rk * longueurParRadian(contoursCometes, contour), finCometes);
      for (let i = 0; i < nombre; i++) {
        const decale = (i * TAU) / nombre;
        for (let j = 1; j <= n; j++) {
          const f = j / n, [x, y] = point(poses[0], o.forme, contour, a0 + da * f + decale, rk, f);
          pousser(this.trainees[k * nombre + i], t - dt * (1 - f), x, y, vieK, finCometes[1], plafond);
        }
      }
    }
    // satellites : sur le trajet des comètes, ou le leur, à des distances variées
    const forme = formeSats;
    const vieSats = SAT_TRAINEE * this.mods[1].trainee * (re.actif ? SAT_TRAIL_RESSORTS : 1);
    // plusieurs contours : chaque satellite suit l'un d'eux, de près
    const listeSats = contoursDe(o, forme), contoursSats = Math.max(1, listeSats.length);
    for (const s of this.sats.concat(this.fixes)) {
      // mode ressorts : le satellite est au bout de son propre ressort, un peu en retard
      if (re.actif) {
        const [ecart, v] = ressort(finiOu(s.rr, s.r) - s.r, s.rv || 0, s.w || SAT_RESSORT[0], SAT_RESSORT[2], dt);
        // dépassement borné (~37 % au plus) : l'étendue reste celle prévue
        s.rr = Math.min(SAT_R_MAX * SAT_DEPASSEMENT, Math.max(0, s.r + ecart));
        s.rv = v;
      } else {
        s.rr = s.r;
        s.rv = 0;
      }
      const contour = contoursSats > 1 ? (s.couleur || 0) % contoursSats : 0, rel = contoursSats > 1 ? 1 + (s.r - 1.3) * 0.25 : s.rr;
      const a0 = s.a, da = wSats * s.mul * dt;
      s.a = reste(a0 + da, TAU);
      const vieS = vieUtile(vieSats, this.wLisse[1] * s.mul);
      const n = sousPas(da, poses[1].rayon * rel * longueurParRadian(listeSats, contour), finSats);
      for (let j = 1; j <= n; j++) {
        const f = j / n, [x, y] = point(poses[1], forme, contour, a0 + da * f, rel, f);
        pousser(s.trainee, t - dt * (1 - f), x, y, vieS, finSats[1], plafond);
      }
    }
  }

  // Vitesse sur le trajet du groupe g, sans le sens (comme Swarm::vitesse) : la poussée s'y
  // ajoute, jamais négative ni au-delà de VITESSE_MAX ; le coup de frein en retire une part.
  vitesse(g) {
    const v = this.spd[g] + this.mods[g].vitesse * this.mods[g].acceleration + this.pousse;
    return (Number.isFinite(v) ? Math.min(VITESSE_MAX, Math.max(0, v)) : 0) * (1 - FREIN[0] * this.frein);
  }

  // Enveloppes des coups (`coups` : nouveaux kicks, caisses claires, attaques des aigus, drops,
  // contretemps) et montée suivie, comme modulation::Sources::avancer.
  avancerSources(dt, coups, tension) {
    const s = this.sources;
    ["kick", "caisse", "charleston", "drop", "contretemps"].forEach((nom, i) => {
      s[nom] = coups[i] > 0 ? 1 : finiOu(s[nom] * Math.exp(-dt * RETOMBEE[i]), 0);
    });
    const vise = Math.max(0, Math.min(1, finiOu(tension, 0)));
    s.montee = finiOu(s.montee + (vise - s.montee) * Math.min(1, dt * SUIVI_MONTEE), 0);
  }

  // Valeur d'une source de modulation en ce moment (comme modulation::Sources::valeur) : 0 à 1,
  // stéréo de -1 à 1. Sert aussi aux jauges des modulations dans la page.
  valeurSource(source) {
    const son = this.sonVu || {}, s = this.sources;
    const v = {
      basse: Math.max(0, Math.min(1, son.basse || 0)),
      grave: this.grave * this.presence,
      aigu: (1 - this.grave) * this.presence,
      kick: s.kick, caisse: s.caisse, charleston: s.charleston, montee: s.montee, drop: s.drop, contretemps: s.contretemps,
      brillance: Math.max(0, Math.min(1, son.brillance || 0)),
      stereo: Math.max(-1, Math.min(1, son.balance || 0)),
      tempo: son.bpm ? 0.5 + 0.5 * Math.cos(TAU * (son.phase || 0)) : 0,
    }[source];
    return finiOu(v, 0);
  }

  // Un événement de la souris ou du morceau (clic, clic droit, piste), au centre : ses
  // déclencheurs agissent tout de suite (comme Swarm::evenement).
  evenement(nom) {
    if (!this.reglages) return;
    if (nom === "piste") this.etiquette.debut = this.t;
    this.declencher(nom, [this.canvas.width / 2, this.canvas.height / 2], this.t);
  }

  // Joue un déclencheur tout de suite, au centre (bouton « Tester » des réglages).
  tester(d) {
    if (!this.reglages) return;
    // essai à la main : une inversion à chaque clic, même aperçu figé
    this.inverseA = -Infinity;
    this.agir(d, [this.canvas.width / 2, this.canvas.height / 2], this.t);
  }

  // Les déclencheurs de `nom`, dans l'ordre de la liste, chacun après son délai et avec sa
  // chance (comme Swarm::declencher).
  declencher(nom, p, t) {
    const liste = (this.reglages.declencheurs || {}).liste || [];
    // délais rattachés à chaque déclencheur (quand, faire), pas à sa place : une liste changée
    // (ajout, retrait, ordre, autre preset) les remet tous à zéro ; seuls force ou délai changés,
    // on les garde (comme `cles_declencheurs` de Swarm::declencher)
    const cles = liste.map((d) => `${d.quand}\u0000${d.faire}`);
    if (!this.clesDeclencheurs || cles.length !== this.clesDeclencheurs.length || cles.some((k, i) => k !== this.clesDeclencheurs[i])) {
      this.clesDeclencheurs = cles;
      this.derniers = cles.map(() => -Infinity);
    }
    liste.forEach((d, i) => {
      if (d.quand !== nom || ((d.delai || 0) > 0 && t - this.derniers[i] < d.delai)) return;
      if ((d.probabilite ?? 1) < 1 && Math.random() >= d.probabilite) return;
      this.derniers[i] = t;
      this.agir(d, p, t);
    });
  }

  // Une action (comme Swarm::agir) ; la bordure n'a pas d'aperçu.
  agir(d, [x, y], t) {
    const c = this.reglages, S = this.echelle || 1;
    const force = Math.min(3, Math.max(0, finiOu(d.force ?? 1, 1)));
    const taille = Math.min(3, Math.max(0.3, (c.particules || {}).taille || 1)) * force;
    const lo = Math.min(d.nombre ?? 3, d.nombre_max ?? 3), hi = Math.max(d.nombre ?? 3, d.nombre_max ?? 3);
    const nombre = () => (lo === hi ? lo : Math.floor(entre(lo, hi + 0.999)));
    const maximum = c.essaim.maximum;
    switch (d.faire) {
      case "eclat": this.eclater(Math.min(1, force)); break;
      case "satellites": this.ajouterSatellites(Math.floor(entre(lo, hi + 0.999)), t, maximum); break;
      // une seule inversion par image (comme Swarm::agir) : deux s'annuleraient
      case "inversion": if (this.inverseA !== t) { this.dir = -this.dir; this.inverseA = t; } break;
      // élan borné à celui d'un rebond de force 3 (comme render::Action::Rebond)
      case "rebond": for (const r of this.rebond) r[1] = Math.min(REBOND_MAX, Math.max(-REBOND_MAX, r[1] + REBOND * force)); break;
      case "poussee": this.pousse += POUSSEE[0] * force; break;
      case "frein": this.frein = Math.max(this.frein, Math.min(1, force)); break;
      // gestes des figures : chaque figure visée qui fait ce geste le recevra à l'image suivante
      // (comme Swarm::agir) ; « tout » (absent) ou « figures » : toutes
      case "battement": case "accent": case "scintillement": case "apogee": {
        const k = GESTES_FIGURE.indexOf(d.faire), sur = d.sur || "tout";
        FIGURES.forEach((s, i) => {
          if ((sur === "tout" || sur === "figures" || sur === s) && GESTES_DES_FIGURES[s].includes(d.faire)) ajouterGeste(this.figures.gestes[i], k, Math.min(2, force));
        });
        break;
      }
      case "onde": this.particules.push({ x, y, vx: 0, vy: 0, ne: t, vie: 0.55, taille: 46 * taille * S, genre: "onde" }); break;
      case "eclats":
        for (let i = 0; i < 16; i++) {
          const a = entre(0, TAU), v = entre(160, 380) * taille * S;
          this.particules.push({ x, y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, ne: t, vie: entre(0.45, 0.8), taille: entre(1.2, 2.4) * S, genre: "etincelle" });
        }
        break;
      case "fissure": {
        // une fissure simple : quelques branches brisées depuis le point (render::Crack est plus fin)
        // taille × force bornée comme render::taille_fissure : ses rayons ne vont pas au-delà de
        // FISSURE_PORTEE_MAX px
        const tailleFissure = Math.min(FISSURE_PORTEE_MAX / FISSURE_RAYON_MAX, Math.max(0.2, (c.fissures || {}).taille || 1.4) * force);
        const branches = [], long = 7 * tailleFissure * S;
        for (let b = 0; b < 5; b++) {
          let a = entre(0, TAU), px = x, py = y;
          const pts = [[px, py]];
          for (let k = 0; k < 6; k++) { a += entre(-0.5, 0.5); px += Math.cos(a) * long; py += Math.sin(a) * long; pts.push([px, py]); }
          branches.push(pts);
        }
        this.fissures.push({ ne: t, branches });
        break;
      }
      case "implosion": {
        const r = 52 * taille * S;
        this.particules.push({ x, y, vx: 0, vy: 0, ne: t, vie: 0.45, taille: r, genre: "implosion" });
        for (let i = 0; i < 12; i++) {
          const a = (i * TAU) / 12 + entre(-0.2, 0.2), cos = Math.cos(a), sin = Math.sin(a), v = entre(2.2, 3) * r;
          this.particules.push({ x: x + cos * r, y: y + sin * r, vx: -cos * v, vy: -sin * v, ne: t, vie: 0.4, taille: entre(1, 1.8) * S, genre: "etincelle" });
        }
        break;
      }
      case "carre": this.particules.push({ x, y, vx: 0, vy: 0, ne: t, vie: CARRE_CAISSE[1], taille: CARRE_CAISSE[0] * force * S, genre: "carre" }); break;
      case "etincelles": {
        // près des têtes (ou sur l'orbite s'il n'y en a pas)
        const n = nombre();
        const tetes = this.trainees.concat(this.sats.map((q) => q.trainee), this.fixes.map((q) => q.trainee)).map((tr) => tr[tr.length - 1]).filter(Boolean);
        for (let k = 0; k < Math.min(n, MAX_PARTICULES); k++) {
          let px, py;
          if (tetes.length) {
            [, px, py] = tetes[Math.floor(Math.random() * tetes.length) % tetes.length];
          } else {
            const a = entre(0, TAU);
            px = x + Math.cos(a) * c.orbite.rayon * S;
            py = y + Math.sin(a) * c.orbite.rayon * S;
          }
          const a = entre(0, TAU), v = entre(30, 90) * S * Math.max(0.2, force);
          this.particules.push({ x: px, y: py, vx: Math.cos(a) * v, vy: Math.sin(a) * v, ne: t, vie: entre(0.3, 0.55), taille: entre(1.1, 2.1) * S, genre: "etincelle" });
        }
        break;
      }
      case "explosion": {
        // l'éclat d'un kick, des satellites, une grande onde et une gerbe d'étincelles
        this.eclater(1);
        this.ajouterSatellites(nombre(), t, maximum);
        this.particules.push({ x, y, vx: 0, vy: 0, ne: t, vie: ONDE_DROP[1], taille: ONDE_DROP[0] * force * S, genre: "onde" });
        for (let i = 0; i < ETINCELLES_DROP; i++) {
          const a = (i * TAU) / ETINCELLES_DROP + entre(-0.1, 0.1), v = entre(220, 420) * S;
          this.particules.push({ x, y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, ne: t, vie: entre(0.5, 0.9), taille: entre(1.2, 2.2) * S, genre: "etincelle" });
        }
        break;
      }
      case "rafale":
        this.eclater(1);
        this.ajouterSatellites(nombre(), t, maximum);
        this.particules.push({ x, y, vx: 0, vy: 0, ne: t, vie: ONDE_PISTE[1], taille: ONDE_PISTE[0] * force * S, genre: "onde" });
        break;
      default: break;
    }
  }

  // Éclat de force `force` (comme Swarm::eclater) : plein, ou en mode sans flashs, à moitié et au
  // plus trois fois par seconde.
  eclater(force) {
    if (force <= 0) return;
    if (!this.doux) this.env = Math.max(this.env, force);
    else if (this.t - this.dernierFlash >= ECART_FLASH) {
      this.dernierFlash = this.t;
      this.env = Math.max(this.env, 0.5 * force);
    }
  }

  // Dessin, dans l'ordre de l'écran (main.rs, puis Swarm::draw pour l'essaim) ; le pointeur (absent
  // du vrai essaim : c'est celui de Windows) est ajouté au centre, par-dessus.
  dessiner() {
    if (!this.reglages) return;
    const g = this.ctx, c = this.reglages, t = this.t, S = this.echelle || 1;
    const couleur = (k) => prisme(c.couleur, t, k, this.teinteImposee, this.teinte);
    g.clearRect(0, 0, this.canvas.width, this.canvas.height);
    // essaim sombre (luminosité basse), ou figure d'une couleur fixe sombre : la scène s'éclaircit,
    // sinon on ne verrait rien sur son noir
    const sombre = (v) => luminositeDe(v, LUMINOSITE) < 0.35;
    const claire = sombre((c.couleur || {}).luminosite) || FIGURES.some((s) => c[s] && c[s].actif && c[s].couleur === "fixe" && sombre(c[s].luminosite));
    if (claire !== this.sceneClaire) { this.sceneClaire = claire; this.canvas.classList.toggle("scene-claire", claire); }
    g.lineCap = "round";
    g.lineJoin = "round";
    const teinte = (k, j, plus = 0) => prisme(c.couleur, t, k, this.teinteImposee, this.teinte, j, plus);
    const ligne = (tr, debut, fin) => {
      g.beginPath();
      g.moveTo(tr[debut][1], tr[debut][2]);
      for (let j = debut + 1; j <= fin; j++) g.lineTo(tr[j][1], tr[j][2]);
      g.stroke();
    };
    // traînée par morceaux (render::draw_trail) : comètes par TRAIL_CHUNK segments, satellites en
    // deux morceaux ; chaque morceau prend la couleur, l'épaisseur et l'opacité de son milieu
    const trainee = (tr, vie, largeur, al, halo, j, plus) => {
      if (tr.length < 2) return;
      const q = tr.map((p) => Math.max(0, 1 - (t - p[0]) / vie));
      if (halo) {
        g.strokeStyle = rgba(teinte(0.7, j, plus), 0.12 * al);
        g.lineWidth = (0.8 + largeur * 0.7) * 2.6;
        ligne(tr, 0, tr.length - 1);
      }
      const morceau = halo ? TRAIL_CHUNK : Math.max(1, Math.ceil((tr.length - 1) / 2));
      for (let i = 0; i + 1 < tr.length;) {
        const fin = Math.min(i + morceau, tr.length - 1), k = (q[i] + q[fin]) * 0.5;
        g.strokeStyle = rgba(teinte(k, j, plus), k * 0.9 * al);
        g.lineWidth = 0.8 + k * largeur;
        ligne(tr, i, fin);
        i = fin;
      }
    };
    // traînée en ruban (render::draw_ruban) : large au milieu, affinée aux deux bouts, sa largeur
    // ondulant le long de la traînée ; par morceaux de RUBAN_MORCEAU segments
    const ruban = (tr, vie, large, fin, al, j, plus, [profondeur, phase]) => {
      if (tr.length < 2) return;
      const q = tr.map((p) => Math.min(1, Math.max(0, 1 - (t - p[0]) / vie)));
      for (let i = 0; i + 1 < tr.length;) {
        const bout = Math.min(i + RUBAN_MORCEAU, tr.length - 1), k = (q[i] + q[bout]) * 0.5;
        const profil = Math.max(0, Math.sin(Math.PI * k ** 0.75)) ** 0.8, onde = 0.5 + 0.5 * Math.sin(5.5 * k + phase);
        g.strokeStyle = rgba(teinte(k, j, plus), al);
        g.lineWidth = Math.max(fin, large * profil * (1 - profondeur * (1 - onde)));
        ligne(tr, i, bout);
        i = bout;
      }
    };
    const disque = (x, y, r, rgb, a) => {
      g.fillStyle = rgba(rgb, a);
      g.beginPath();
      g.arc(x, y, Math.max(0.1, r), 0, TAU);
      g.fill();
    };
    const cx = this.canvas.width / 2, cy = this.canvas.height / 2;
    const lueur = 0.4 + 0.6 * this.env, tete = couleur(1), duree = Math.max(0.1, c.essaim.duree);
    const trait = (a, b, largeur, rgb, al) => {
      g.strokeStyle = rgba(rgb, al);
      g.lineWidth = largeur;
      g.beginPath();
      g.moveTo(a[0], a[1]);
      g.lineTo(b[0], b[1]);
      g.stroke();
    };
    // dans l'ordre de l'écran (main.rs) : équerres, halo et liseré, fissures, figures, spectre et
    // réticule (Effets::dessiner), l'essaim, puis l'étiquette par-dessus tout
    this.dessinerLoin(g, S, tete, couleur(0));
    // fissures : trait sombre, reflet, lueur colorée au début, fondu sur la fin
    const vieFissure = Math.max(0.2, (c.fissures || {}).duree || 3);
    g.lineCap = "round";
    g.lineJoin = "round";
    for (const f of this.fissures) {
      const age = t - f.ne, al = Math.min(1, vieFissure - age), lueurF = Math.max(0, 1 - age / 0.45, this.env * 0.6);
      // largeurs à l'échelle de l'écran (Crack::a_l_echelle) : éclat 3,5 puis 1,2, reflet décalé de
      // 0,8, au moins 0,5 de large
      for (const [rgb, largeur, a, d] of [[[20, 20, 28], 2.4 * S, 0.8, 0], [tete, 3.5 * S, 0.3 * lueurF, 0], [tete, 1.2 * S, lueurF, 0], [[170, 185, 250], Math.max(0.84 * S, 0.5 * S), 0.25, 0.8 * S]]) {
        g.strokeStyle = rgba(rgb, a * al);
        g.lineWidth = largeur;
        for (const b of f.branches) {
          g.beginPath();
          g.moveTo(b[0][0] + d, b[0][1] + d);
          for (const q of b.slice(1)) g.lineTo(q[0] + d, q[1] + d);
          g.stroke();
        }
      }
    }
    this.dessinerFigures(g, S);
    g.lineCap = "round";
    g.lineJoin = "round";
    const sp = c.spectre;
    if (sp && sp.actif) {
      const n = Math.min(BANDES_MAX, Math.max(8, Math.round(sp.bandes))), r = sp.rayon * S, l = spectreLargeur(n, r, S);
      for (let i = 0; i < n; i++) {
        const v = this.bandes[i], a = -Math.PI / 2 + (i / n) * TAU, cos = Math.cos(a), sin = Math.sin(a);
        const long = (SPECTRE_BARRE_MIN + sp.hauteur * v) * S;
        trait([cx + cos * r, cy + sin * r], [cx + cos * (r + long), cy + sin * (r + long)], l, couleur(v), 0.35 + 0.6 * v);
      }
    }
    const re = c.reticule;
    if (re && re.actif) {
      const vide = re.vide * S, long = (re.vide + re.taille) * S, env = Math.min(1, Math.max(0, this.env));
      for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        const a = [cx + dx * vide, cy + dy * vide], b = [cx + dx * long, cy + dy * long];
        if (env > 0.02) trait(a, b, (re.epaisseur + RETICULE_LUEUR) * S, tete, 0.3 * env * re.opacite);
        trait(a, b, re.epaisseur * S, tete, re.opacite * (0.6 + 0.4 * env));
      }
    }
    const tetes = [];
    // modulations : taille (têtes et traînées), teinte, lueur, traînée et opacité de chaque groupe
    const [mc, ms] = this.mods;
    // mode ressorts : têtes étirées dans leur élan, satellites qui entrent en dépassant leur taille
    // et sortent en fondant, traînées de satellites plus courtes
    const ressorts = c.ressorts || {}, rubans = c.rubans || {};
    const queue = (tr) => (ressorts.actif ? etirement(tr, S, finiOu(+ressorts.etirement, 1)) : [0, 0]);
    const vieSats = SAT_TRAINEE * ms.trainee * (ressorts.actif ? SAT_TRAIL_RESSORTS : 1);
    // satellites (sans halo) ; les permanents restent à 0,8
    for (const s of this.sats.concat(this.fixes)) {
      let k = 0.8;
      if (Number.isFinite(s.ne)) {
        const age = t - s.ne;
        k = 1 - age / duree;
        if (ressorts.actif) {
          const entree = age < SAT_ENTREE_SORTIE[0] ? depasse(age / SAT_ENTREE_SORTIE[0]) : 1;
          k = (1 - (SAT_FONTE * age) / duree) * entree * Math.min(1, Math.max(0, (duree - age) / SAT_ENTREE_SORTIE[1])) ** 2;
        }
      }
      const al = Math.min(1, k * 1.5) * ms.opacite;
      trainee(s.trainee, vieUtile(vieSats, this.wLisse[1] * s.mul), 2.6 * k * S * ms.taille, al, false, s.couleur || 0, ms.teinte);
      const p = s.trainee[s.trainee.length - 1];
      if (p) tetes.push({ x: p[1], y: p[2], s: (0.6 + 1.6 * k) * S * ms.taille, al, lueur: ms.luminosite, devant: p[2] > cy, rgb: teinte(1, s.couleur || 0, ms.teinte), queue: queue(s.trainee) });
    }
    // les comètes (devant : sous le centre de l'orbite, comme vues de biais), chacune dans une
    // teinte de la palette. En rubans : une traînée large, vrillée, sans tête
    if (c.orbite.cometes) {
      const vieReglee = Math.max(0.01, c.orbite.trainee) * mc.trainee, phase = reste(-VRILLE[1] * t, TAU) + this.vrille[1];
      const nombre = Math.max(1, c.orbite.nombre);
      this.trainees.forEach((tr, i) => {
        // vie utile de son anneau (TOURS_TRAINEE tours de trajet au plus), comme dans `pas`
        const vie = finiOu(this.vies[Math.floor(i / nombre)], vieReglee);
        if (rubans.actif) {
          const large = Math.min(14, Math.max(1, finiOu(+rubans.largeur, 4))) * S * mc.taille;
          ruban(tr, vie, large, 0.6 * S, mc.opacite, i, mc.teinte, [VRILLE[0] * Math.min(1, Math.max(0, finiOu(+rubans.vrille, 1))), phase + 2 * i]);
        } else {
          trainee(tr, vie, 3.5 * S * mc.taille, mc.opacite, true, i, mc.teinte);
        }
        const p = tr[tr.length - 1];
        if (p) tetes.push({ x: p[1], y: p[2], s: 2.4 * S * mc.taille, al: mc.opacite, lueur: mc.luminosite, devant: p[2] > cy, rgb: teinte(1, i, mc.teinte), queue: queue(tr), cachee: !!rubans.actif });
      });
    }
    // constellation : fines lignes entre les têtes proches (comme render::liens), sous les têtes
    if (c.essaim.constellation) {
      const portee = LIENS_PORTEE * c.orbite.rayon * S * Math.max(0.2, this.mods[0].rayon), lien = couleur(0.85);
      let n = 0;
      g.lineWidth = 1.2 * S;
      for (let i = 0; i < tetes.length && n < LIENS_MAX; i++) {
        for (let j = i + 1; j < tetes.length && n < LIENS_MAX; j++) {
          const d = Math.hypot(tetes[i].x - tetes[j].x, tetes[i].y - tetes[j].y);
          if (d >= portee) continue;
          n++;
          g.strokeStyle = rgba(lien, (0.25 + 0.5 * (1 - d / portee)) * Math.min(tetes[i].al, tetes[j].al));
          g.beginPath();
          g.moveTo(tetes[i].x, tetes[i].y);
          g.lineTo(tetes[j].x, tetes[j].y);
          g.stroke();
        }
      }
    }
    const dessinerTete = (h) => {
      if (h.cachee) return;
      if (Math.abs(h.queue[0]) + Math.abs(h.queue[1]) > 0.5) {
        // tête étirée : les mêmes quatre couches, en traits aux bouts ronds
        const a = [h.x, h.y], b = [h.x + h.queue[0], h.y + h.queue[1]];
        trait(a, b, h.s * 5.2, h.rgb, 0.12 * h.al * lueur * h.lueur);
        trait(a, b, h.s * 3.2, h.rgb, 0.3 * h.al * lueur * h.lueur);
        trait(a, b, h.s * 2, h.rgb, h.al);
        trait(a, [h.x + h.queue[0] * 0.55, h.y + h.queue[1] * 0.55], h.s * 0.9, blancReflet(c.couleur), h.al);
        return;
      }
      disque(h.x, h.y, h.s * 2.6, h.rgb, 0.12 * h.al * lueur * h.lueur);
      disque(h.x, h.y, h.s * 1.6, h.rgb, 0.3 * h.al * lueur * h.lueur);
      disque(h.x, h.y, h.s, h.rgb, h.al);
      disque(h.x, h.y, h.s * 0.45, blancReflet(c.couleur), h.al);
    };
    for (const h of tetes) if (!h.devant) dessinerTete(h);
    const eclat = c.eclat.taille * S;
    // mode sans flashs : ni halo blanc, ni anneau ou carré trop vifs (comme Swarm::draw)
    const vif = this.doux ? 0.45 : 1;
    if (this.env > 0.02 && c.eclat.taille > 0) {
      if (ressorts.actif) {
        // mode ressorts : un anneau épais qui sort en dépassant puis s'affine, sans halo blanc
        const k = Math.min(1, Math.max(0, -Math.log(Math.min(1, this.env)) / 3.78));
        g.strokeStyle = rgba(tete, 0.9 * (1 - k) * vif);
        g.lineWidth = (ANNEAU_RESSORTS[1] * (1 - k) ** 2 + 0.6) * S;
        g.beginPath();
        g.arc(cx, cy, Math.max(1, eclat * ANNEAU_RESSORTS[0] * depasse(k)), 0, TAU);
        g.stroke();
      } else {
        // éclat du kick : halo blanc et anneau coloré qui s'élargit sous le pointeur ; le halo
        // laisse un vide sous la pointe (render::HALO_VIDE), réduit à la moitié du halo pour un
        // petit éclat
        const dehors = eclat * (2 + 2.5 * this.env), vide = Math.min(HALO_VIDE * S, 0.5 * dehors);
        if (!this.doux && dehors > vide) {
          g.strokeStyle = rgba(blancReflet(c.couleur), 0.35 * this.env);
          g.lineWidth = dehors - vide;
          g.beginPath();
          g.arc(cx, cy, (dehors + vide) / 2, 0, TAU);
          g.stroke();
        }
        g.strokeStyle = rgba(tete, 0.8 * this.env * vif);
        g.lineWidth = 1.5 * S;
        g.beginPath();
        g.arc(cx, cy, eclat * (2.5 + 5 * (1 - this.env)), 0, TAU);
        g.stroke();
      }
    }
    // particules : étincelles, ondes de choc, poussière du sillage
    const poussiere = couleur(0.5);
    for (const q of this.particules) {
      const k = Math.min(1, Math.max(0, (t - q.ne) / q.vie));
      if (q.genre === "onde") {
        g.strokeStyle = rgba(tete, 0.85 * (1 - k) * vif);
        g.lineWidth = (2.5 * (1 - k) + 0.6) * S;
        g.beginPath();
        g.arc(q.x, q.y, Math.max(1, q.taille * (1 - (1 - k) ** 3)), 0, TAU);
        g.stroke();
      } else if (q.genre === "poussiere") {
        disque(q.x, q.y, q.taille * (1 - 0.5 * k), poussiere, 0.7 * (1 - k));
      } else if (q.genre === "carre") {
        // carré posé sur la pointe, qui s'ouvre vite puis s'efface
        const r = q.taille * (0.35 + 0.65 * (1 - (1 - k) ** 2));
        g.strokeStyle = rgba(tete, 0.8 * (1 - k) * vif);
        g.lineWidth = (2.2 * (1 - k) + 0.8) * S;
        g.beginPath();
        g.moveTo(q.x, q.y - r);
        g.lineTo(q.x + r, q.y);
        g.lineTo(q.x, q.y + r);
        g.lineTo(q.x - r, q.y);
        g.closePath();
        g.stroke();
      } else if (q.genre === "implosion") {
        // anneau qui se referme
        g.strokeStyle = rgba(tete, 0.8 * (1 - k * k) * vif);
        g.lineWidth = (0.8 + 1.8 * k) * S;
        g.beginPath();
        g.arc(q.x, q.y, Math.max(1, q.taille * (1 - k) ** 2), 0, TAU);
        g.stroke();
      } else {
        disque(q.x, q.y, q.taille * 2.2, tete, 0.25 * (1 - k));
        disque(q.x, q.y, q.taille * (1 - 0.5 * k), blancReflet(c.couleur), (1 - k) * vif);
      }
    }
    for (const h of tetes) if (h.devant) dessinerTete(h);
    this.dessinerEtiquette(g, cx, cy, S, tete);
    // le pointeur, au centre : le curseur Windows du preset (point chaud au centre), sinon un point
    const cur = this.curseur;
    if (cur) {
      const taille = 32 * S, k = cur.images > 1 ? imageA(cur, performance.now()) : 0;
      g.drawImage(cur.img, k * cur.cote, 0, cur.cote, cur.cote, cx - (cur.chaud[0] / cur.cote) * taille, cy - (cur.chaud[1] / cur.cote) * taille, taille, taille);
    } else {
      disque(cx, cy, 2.2 * S, [255, 255, 255], 0.95);
    }
  }
}

// Image (indice) d'un curseur animé à l'instant `ms`, d'après la durée de chaque image.
function imageA(image, ms) {
  let t = ms % (image.durees.reduce((a, b) => a + b, 0) || 1), k = 0;
  while (k < image.durees.length - 1 && t >= image.durees[k]) { t -= image.durees[k]; k++; }
  return k;
}

window.Apercu = Apercu;
window.chemin = chemin;
window.contoursDe = contoursDe;
window.imageA = imageA;

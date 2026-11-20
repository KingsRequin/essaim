// Aperçu d'un preset : le même calcul que l'essaim autour du pointeur (src/render.rs et
// src/modulation.rs), dessiné dans un canevas. Les événements du son (kick, caisse claire, drop…)
// et de la souris font agir les déclencheurs ; les modulations font varier rotation, vitesse,
// rayon… Le pointeur est au centre. Toute formule de render.rs (Swarm::update, agir,
// Swarm::draw, draw_trail…) est reprise ici telle quelle : la modifier là oblige à la reporter ici.
"use strict";

const TAU = Math.PI * 2;
const SAT_R_MAX = 1.9;
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
const RETICULE_VIDE = 12;
const RETICULE_LARGEUR = 1.2;
const RETICULE_LUEUR = 4;
const SPECTRE_BARRE_MIN = 1.5;
const SPECTRE_MONTEE = 30;
const SPECTRE_DESCENTE = 8;
const BANDES_MAX = 32;
// Modulations : comme src/modulation.rs (effet à fond de chaque cible, retombée des coups :
// kick, caisse claire, charleston, drop, contretemps ; suivi de la montée ; lissage par défaut).
const MODULATION = {
  rotation: 90, vitesse: 40, acceleration: 2, rayon: 4, taille: 1.5, teinte: 180, decalage: 1.6, luminosite: 2, trainee: 3, opacite: 1,
};
const RETOMBEE = [7, 8, 10, 0.9, 10];
const SUIVI_MONTEE = 6;
const LISSAGE = 0.35;
const ADDITIONNEES = new Set(["rotation", "vitesse", "teinte", "decalage"]);
const effetNeutre = () => ({ rotation: 0, vitesse: 0, teinte: 0, decalage: 0, acceleration: 1, rayon: 1, taille: 1, luminosite: 1, trainee: 1, opacite: 1 });
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
// (`groupes`), et teinte de tout ce qui est dessiné (`teinte`, la teinte « sur tout »).
function effetsModulations(liste, valeurs) {
  const r = { groupes: [effetNeutre(), effetNeutre()], teinte: 0 };
  (Array.isArray(liste) ? liste : []).forEach((m, i) => {
    const v = valeurs[i];
    if (!Number.isFinite(v) || !(m.cible in MODULATION)) return;
    const sur = m.sur || "tout";
    if (m.cible === "teinte" && sur === "tout") { r.teinte += v * MODULATION.teinte; return; }
    r.groupes.forEach((e, g) => {
      if ((g === 1 && sur === "cometes") || (g === 0 && sur === "satellites")) return;
      if (ADDITIONNEES.has(m.cible)) e[m.cible] += v * MODULATION[m.cible];
      else e[m.cible] *= Math.max(0, 1 + v * MODULATION[m.cible]);
    });
  });
  const borne = (x, a, b) => Math.min(b, Math.max(a, x));
  for (const e of r.groupes) {
    e.acceleration = borne(e.acceleration, 0, 5);
    e.rayon = borne(e.rayon, 0.1, 5);
    e.taille = borne(e.taille, 0, 4);
    e.luminosite = borne(e.luminosite, 0, 4);
    e.trainee = borne(e.trainee, 0.1, 5);
    e.opacite = borne(e.opacite, 0, 1);
  }
  return r;
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
  return hsl((((queue + dh * k) % 360) + 360) % 360, 1, 0.66);
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

// Contours (8 au plus) centrés sur le milieu du cadre qui les englobe et mis à la même échelle
// (point le plus loin : 1,2), comme render::mettre_a_l_echelle ; `echelle` faux : déjà à
// l'échelle (ceux de l'icône, venus de l'appli). Gardés par liste.
const CONTOURS_MAX = 8;
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
function pousser(tr, t, x, y, vie, ecart) {
  const n = tr.length, p = n >= 2 ? tr[n - 2] : null;
  if (p && t - p[0] < PAS_TRAINEE && (p[1] - x) ** 2 + (p[2] - y) ** 2 < ecart * ecart) tr[n - 1] = [t, x, y];
  else tr.push([t, x, y]);
  while (tr.length && t - tr[0][0] > vie) tr.shift();
}

// Pas intermédiaires pour un déplacement d'angle `da` sur un rayon de `r` px (comme
// render::sous_pas) : la traînée reste une courbe quand l'essaim tourne vite.
function sousPas(da, r, [angle, ecart]) {
  return Math.min(24, Math.max(1, Math.ceil(Math.max(Math.abs(da) / angle, (Math.abs(da) * r) / ecart))));
}

class Apercu {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.reglages = null;
    this.dir = 1;
    // vitesse du trajet des comètes et de celui des satellites, lissée (render::Swarm::spd)
    this.spd = [3.5, 3.5];
    this.angles = [];
    this.rot = 0;
    this.rotSats = 0;
    // modulations : enveloppes des coups et montée suivie (modulation::Sources), valeur lissée de
    // chaque modulation, effets sur les comètes et les satellites à cette image et à la
    // précédente, teinte de tout ce qui est dessiné
    this.sources = { kick: 0, caisse: 0, charleston: 0, drop: 0, contretemps: 0, montee: 0 };
    this.valeurs = [];
    this.mods = [effetNeutre(), effetNeutre()];
    this.modsAvant = [effetNeutre(), effetNeutre()];
    this.teinte = 0;
    // déclencheurs : dernier déclenchement de chacun (délai minimal)
    this.derniers = [];
    this.ecoute = new Ecoute();
    this.sonVu = null;
    this.grave = 0;
    this.presence = 0;
    this.hauteur = 50;
    this.env = 0;
    this.trainees = [];
    this.sats = [];
    this.fixes = [];
    this.teinteImposee = null;
    this.curseur = null;
    this.particules = [];
    // fissures (l'aperçu les dessine, plus simplement que render::Crack)
    this.fissures = [];
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

  regler(reglages) {
    this.reglages = reglages;
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
    const portee = o.rayon * Math.max(SAT_R_MAX, anneau(anneaux - 1)[0]) * 1.3 * (1 + (rayonMax - 1) * 0.7) + 6;
    this.echelle = Math.min(2.2 * dpr, (w * 0.46) / portee);
  }

  // Souris sur le canevas (coordonnées de la fenêtre), ou `survoler(null)` quand elle en sort.
  // L'essaim reste au centre ; seul le sillage suit la souris.
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
    this.sillage(dt);
    const n = Math.max(1, Math.ceil(dt / PAS_TRAINEE));
    for (let i = 0; i < n; i++) this.pas(dt / n, son);
    this.spectre(dt, son);
  }

  // Bandes de l'anneau de spectre : simulées, puis lissées comme effets::lisser_bandes.
  spectre(dt, son) {
    const s = this.reglages.spectre;
    if (!s || !s.actif) return;
    if (this.aigusSpectre !== null && (son.aigus || 0) !== this.aigusSpectre) this.eclatAigus = 1;
    this.aigusSpectre = son.aigus || 0;
    this.eclatAigus *= Math.exp(-dt * 12);
    const n = Math.min(BANDES_MAX, Math.max(8, Math.round(s.bandes)));
    const lues = bandesSimulees(n, this.t, son, this.env, this.eclatAigus);
    const km = Math.min(1, dt * SPECTRE_MONTEE), kd = Math.min(1, dt * SPECTRE_DESCENTE);
    for (let i = 0; i < n; i++) this.bandes[i] += (lues[i] - this.bandes[i]) * (lues[i] > this.bandes[i] ? km : kd);
  }

  // Sillage : poussière le long du trajet de la souris quand elle file vite (comme
  // Swarm::update), une fois par image ; les grains naissent à la fin de l'image.
  sillage(dt) {
    const c = this.reglages, S = this.echelle || 1, cur = this.pointeur, avant = this.pointeurAvant;
    this.pointeurAvant = cur;
    if (!c.sillage || !c.sillage.actif || !cur || !avant || !(dt > 0)) return;
    const dx = cur[0] - avant[0], dy = cur[1] - avant[1], dist = Math.hypot(dx, dy);
    if (!(dist / Math.max(dt, 1e-3) > 700 * S && dist < 600 * S)) return;
    const n = Math.min(14, Math.round((Math.min(10, Math.max(1, c.sillage.densite)) * dist) / (60 * S)));
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

  // Satellites nés à `t` (comme Swarm::ajouter_satellites), dans la limite du maximum.
  ajouterSatellites(n, t, maximum) {
    for (let j = 0; j < n && this.sats.length < maximum; j++) {
      this.sats.push({ a: entre(0, TAU), r: entre(0.75, SAT_R_MAX), mul: entre(0.7, 1.3), ne: t, trainee: [], couleur: this.prochaineCouleur++ });
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
    // modulations : valeur visée de chacune (source, seuil, à l'envers), lissée à son rythme
    this.avancerSources(dt, [coups.kicks, coups.caisses, coups.aigus, coups.drops, coups.contretemps ? 1 : 0], son.tension || 0);
    this.sonVu = son;
    this.grave = grave;
    this.presence = presence;
    const liste = (c.modulations || {}).liste || [];
    this.valeurs.length = liste.length;
    liste.forEach((m, i) => {
      const vise = visee(m, this.valeurSource(m.source));
      this.valeurs[i] = finiOu((this.valeurs[i] || 0) + (vise - (this.valeurs[i] || 0)) * partLissage(m.lissage ?? LISSAGE, dt), 0);
    });
    const effets = effetsModulations(liste, this.valeurs);
    this.modsAvant = this.mods;
    this.mods = effets.groupes;
    this.teinte = effets.teinte;
    // vitesse du trajet de chaque groupe, accélérée (montée…), lissée
    this.spd = this.spd.map((v, g) => finiOu(v + (o.vitesse_base * this.mods[g].acceleration - v) * Math.min(dt * 10, 1), o.vitesse_base));
    // comètes : `nombre` par anneau, réparties sur le trajet ; plusieurs contours (une image, une
    // icône, un texte) : un anneau par contour, à sa place
    const contoursCometes = contoursDe(o, o.forme), nContours = Math.max(1, contoursCometes.length);
    const nAnneaux = nContours > 1 ? nContours : Math.max(1, o.anneaux), nombre = Math.max(1, o.nombre);
    while (this.angles.length < nAnneaux) this.angles.push(0);
    this.angles.length = nAnneaux;
    while (this.trainees.length < nAnneaux * nombre) this.trainees.push([]);
    this.trainees.length = nAnneaux * nombre;
    const vie = Math.max(0.01, o.trainee) * this.mods[0].trainee;
    // une pose par groupe (render::Pose) : rotation du trajet, rayon et décalage au début et à la
    // fin de l'image ; les points intermédiaires suivent la vraie forme, coins compris
    const rayon = o.rayon * S;
    const sur = o.rotation_sur || "tout";
    const pose = (g, rot) => {
      const base = (g === 1 ? sur !== "cometes" : sur !== "satellites") ? o.rotation || 0 : 0;
      const drot = finiOu((((base + this.mods[g].rotation) * TAU) / 60) * dt, 0);
      return {
        rot0: finiOu(rot, 0), drot, rayon0: rayon * this.modsAvant[g].rayon, rayon: rayon * this.mods[g].rayon,
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
    const wCometes = this.dir * this.vitesse(0), wSats = this.dir * this.vitesse(1);
    // les écarts de render.rs sont en pixels d'écran : ici, à l'échelle de l'aperçu
    const finCometes = [FINESSE_COMETES[0], FINESSE_COMETES[1] * S], finSats = [FINESSE_SATELLITES[0], FINESSE_SATELLITES[1] * S];
    for (let k = 0; k < nAnneaux; k++) {
      const [rk, wk] = nContours > 1 ? [1, 1] : anneau(k);
      const contour = k % nContours;
      const a0 = this.angles[k], da = wCometes * wk * dt;
      this.angles[k] = reste(a0 + da, TAU);
      const n = sousPas(da, poses[0].rayon * rk * longueurParRadian(contoursCometes, contour), finCometes);
      for (let i = 0; i < nombre; i++) {
        const decale = (i * TAU) / nombre;
        for (let j = 1; j <= n; j++) {
          const f = j / n, [x, y] = point(poses[0], o.forme, contour, a0 + da * f + decale, rk, f);
          pousser(this.trainees[k * nombre + i], t - dt * (1 - f), x, y, vie, finCometes[1]);
        }
      }
    }
    const duree = Math.max(0.1, e.duree);
    this.sats = this.sats.filter((s) => t - s.ne < duree);
    // satellites permanents : répartis une fois pour toutes (nombre d'or), toujours les mêmes
    if (this.fixes.length !== e.fixes) {
      this.fixes = Array.from({ length: e.fixes }, (_, j) => {
        const u = (j * 0.618034) % 1;
        return { a: j * 2.399963, r: 0.6 + u * (SAT_R_MAX - 0.6), mul: 0.6 + 0.8 * (1 - u), ne: Infinity, trainee: [], couleur: j };
      });
    }
    // satellites : sur le trajet des comètes, ou le leur, à des distances variées
    const forme = !e.forme || e.forme === "cometes" ? o.forme : e.forme;
    const vieSats = SAT_TRAINEE * this.mods[1].trainee;
    // plusieurs contours : chaque satellite suit l'un d'eux, de près
    const listeSats = contoursDe(o, forme), contoursSats = Math.max(1, listeSats.length);
    for (const s of this.sats.concat(this.fixes)) {
      const contour = contoursSats > 1 ? (s.couleur || 0) % contoursSats : 0, rel = contoursSats > 1 ? 1 + (s.r - 1.3) * 0.25 : s.r;
      const a0 = s.a, da = wSats * s.mul * dt;
      s.a = reste(a0 + da, TAU);
      const n = sousPas(da, poses[1].rayon * rel * longueurParRadian(listeSats, contour), finSats);
      for (let j = 1; j <= n; j++) {
        const f = j / n, [x, y] = point(poses[1], forme, contour, a0 + da * f, rel, f);
        pousser(s.trainee, t - dt * (1 - f), x, y, vieSats, finSats[1]);
      }
    }
  }

  // Vitesse sur le trajet du groupe g, sans le sens (comme Swarm::vitesse).
  vitesse(g) {
    return Math.max(0, this.spd[g] + this.mods[g].vitesse * this.mods[g].acceleration);
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
    this.declencher(nom, [this.canvas.width / 2, this.canvas.height / 2], this.t);
  }

  // Joue un déclencheur tout de suite, au centre (bouton « Tester » des réglages).
  tester(d) {
    if (!this.reglages) return;
    this.agir(d, [this.canvas.width / 2, this.canvas.height / 2], this.t);
  }

  // Les déclencheurs de `nom`, dans l'ordre de la liste, chacun après son délai et avec sa
  // chance (comme Swarm::declencher).
  declencher(nom, p, t) {
    const liste = (this.reglages.declencheurs || {}).liste || [];
    if (this.derniers.length !== liste.length) this.derniers = liste.map(() => -Infinity);
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
      case "inversion": this.dir = -this.dir; break;
      case "onde": this.particules.push({ x, y, vx: 0, vy: 0, ne: t, vie: 0.55, taille: 46 * taille * S, genre: "onde" }); break;
      case "eclats":
        for (let i = 0; i < 16; i++) {
          const a = entre(0, TAU), v = entre(160, 380) * taille * S;
          this.particules.push({ x, y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, ne: t, vie: entre(0.45, 0.8), taille: entre(1.2, 2.4) * S, genre: "etincelle" });
        }
        break;
      case "fissure": {
        // une fissure simple : quelques branches brisées depuis le point (render::Crack est plus fin)
        const branches = [], long = 7 * Math.max(0.2, (c.fissures || {}).taille || 1.4) * force * S;
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

  // Éclat de force `force` (comme Swarm::eclater ; l'aperçu n'a pas le mode sans flashs).
  eclater(force) {
    if (force > 0) this.env = Math.max(this.env, force);
  }

  // Dessin, dans l'ordre de Swarm::draw ; le pointeur (absent du vrai essaim : c'est celui de
  // Windows) est ajouté au centre, par-dessus.
  dessiner() {
    if (!this.reglages) return;
    const g = this.ctx, c = this.reglages, t = this.t, S = this.echelle || 1;
    const couleur = (k) => prisme(c.couleur, t, k, this.teinteImposee, this.teinte);
    g.clearRect(0, 0, this.canvas.width, this.canvas.height);
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
    const disque = (x, y, r, rgb, a) => {
      g.fillStyle = rgba(rgb, a);
      g.beginPath();
      g.arc(x, y, Math.max(0.1, r), 0, TAU);
      g.fill();
    };
    const cx = this.canvas.width / 2, cy = this.canvas.height / 2;
    const lueur = 0.4 + 0.6 * this.env, tete = couleur(1), duree = Math.max(0.1, c.essaim.duree);
    // anneau de spectre puis réticule, sous l'essaim (comme Effets::dessiner)
    const trait = (a, b, largeur, rgb, al) => {
      g.strokeStyle = rgba(rgb, al);
      g.lineWidth = largeur;
      g.beginPath();
      g.moveTo(a[0], a[1]);
      g.lineTo(b[0], b[1]);
      g.stroke();
    };
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
      const vide = RETICULE_VIDE * S, long = (RETICULE_VIDE + re.taille) * S, env = Math.min(1, Math.max(0, this.env));
      for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        const a = [cx + dx * vide, cy + dy * vide], b = [cx + dx * long, cy + dy * long];
        if (env > 0.02) trait(a, b, RETICULE_LUEUR * S, tete, 0.3 * env * re.opacite);
        trait(a, b, RETICULE_LARGEUR * S, tete, re.opacite * (0.6 + 0.4 * env));
      }
    }
    // fissures, sous l'essaim : trait sombre, reflet, lueur colorée au début, fondu sur la fin
    const vieFissure = Math.max(0.2, (c.fissures || {}).duree || 3);
    for (const f of this.fissures) {
      const age = t - f.ne, al = Math.min(1, vieFissure - age), lueurF = Math.max(0, 1 - age / 0.45, this.env * 0.6);
      for (const [rgb, largeur, a] of [[[20, 20, 28], 2.4 * S, 0.8], [tete, 3.2 * S, 0.5 * lueurF], [[170, 185, 250], 0.9 * S, 0.5]]) {
        g.strokeStyle = rgba(rgb, a * al);
        g.lineWidth = largeur;
        for (const b of f.branches) {
          g.beginPath();
          g.moveTo(b[0][0], b[0][1]);
          for (const q of b.slice(1)) g.lineTo(q[0], q[1]);
          g.stroke();
        }
      }
    }
    const tetes = [];
    // modulations : taille (têtes et traînées), teinte, lueur, traînée et opacité de chaque groupe
    const [mc, ms] = this.mods;
    // satellites (sans halo) ; les permanents restent à 0,8
    for (const s of this.sats.concat(this.fixes)) {
      const k = Number.isFinite(s.ne) ? 1 - (t - s.ne) / duree : 0.8, al = Math.min(1, k * 1.5) * ms.opacite;
      trainee(s.trainee, SAT_TRAINEE * ms.trainee, 2.6 * k * S * ms.taille, al, false, s.couleur || 0, ms.teinte);
      const p = s.trainee[s.trainee.length - 1];
      if (p) tetes.push({ x: p[1], y: p[2], s: (0.6 + 1.6 * k) * S * ms.taille, al, lueur: ms.luminosite, devant: p[2] > cy, rgb: teinte(1, s.couleur || 0, ms.teinte) });
    }
    // les comètes (devant : sous le centre de l'orbite, comme vues de biais), chacune dans une
    // teinte de la palette
    if (c.orbite.cometes) {
      this.trainees.forEach((tr, i) => {
        trainee(tr, Math.max(0.01, c.orbite.trainee) * mc.trainee, 3.5 * S * mc.taille, mc.opacite, true, i, mc.teinte);
        const p = tr[tr.length - 1];
        if (p) tetes.push({ x: p[1], y: p[2], s: 2.4 * S * mc.taille, al: mc.opacite, lueur: mc.luminosite, devant: p[2] > cy, rgb: teinte(1, i, mc.teinte) });
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
      disque(h.x, h.y, h.s * 2.6, h.rgb, 0.12 * h.al * lueur * h.lueur);
      disque(h.x, h.y, h.s * 1.6, h.rgb, 0.3 * h.al * lueur * h.lueur);
      disque(h.x, h.y, h.s, h.rgb, h.al);
      disque(h.x, h.y, h.s * 0.45, [255, 255, 255], h.al);
    };
    for (const h of tetes) if (!h.devant) dessinerTete(h);
    // éclat du kick : halo blanc et anneau coloré qui s'élargit sous le pointeur
    const eclat = c.eclat.taille * S;
    if (this.env > 0.02 && c.eclat.taille > 0) {
      disque(cx, cy, eclat * (2 + 2.5 * this.env), [255, 255, 255], 0.35 * this.env);
      g.strokeStyle = rgba(tete, 0.8 * this.env);
      g.lineWidth = 1.5 * S;
      g.beginPath();
      g.arc(cx, cy, eclat * (2.5 + 5 * (1 - this.env)), 0, TAU);
      g.stroke();
    }
    // particules : étincelles, ondes de choc, poussière du sillage
    const poussiere = couleur(0.5);
    for (const q of this.particules) {
      const k = Math.min(1, Math.max(0, (t - q.ne) / q.vie));
      if (q.genre === "onde") {
        g.strokeStyle = rgba(tete, 0.85 * (1 - k));
        g.lineWidth = (2.5 * (1 - k) + 0.6) * S;
        g.beginPath();
        g.arc(q.x, q.y, Math.max(1, q.taille * (1 - (1 - k) ** 3)), 0, TAU);
        g.stroke();
      } else if (q.genre === "poussiere") {
        disque(q.x, q.y, q.taille * (1 - 0.5 * k), poussiere, 0.7 * (1 - k));
      } else if (q.genre === "carre") {
        // carré posé sur la pointe, qui s'ouvre vite puis s'efface
        const r = q.taille * (0.35 + 0.65 * (1 - (1 - k) ** 2));
        g.strokeStyle = rgba(tete, 0.8 * (1 - k));
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
        g.strokeStyle = rgba(tete, 0.8 * (1 - k * k));
        g.lineWidth = (0.8 + 1.8 * k) * S;
        g.beginPath();
        g.arc(q.x, q.y, Math.max(1, q.taille * (1 - k) ** 2), 0, TAU);
        g.stroke();
      } else {
        disque(q.x, q.y, q.taille * 2.2, tete, 0.25 * (1 - k));
        disque(q.x, q.y, q.taille * (1 - 0.5 * k), [255, 255, 255], 1 - k);
      }
    }
    for (const h of tetes) if (h.devant) dessinerTete(h);
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

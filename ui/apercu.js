// Aperçu d'un preset : le même calcul que l'essaim autour du pointeur (src/render.rs et
// src/modulation.rs), dessiné
// dans un canevas. Kick : inverse le sens et ajoute des satellites ; basse : vitesse ; note
// grave : orbites plus larges. Le pointeur est au centre. Toute formule de render.rs (Swarm::update,
// Swarm::draw, draw_trail…) est reprise ici telle quelle : la modifier là oblige à la reporter ici.
"use strict";

const TAU = Math.PI * 2;
const SAT_R_MAX = 1.9;
// Éclat de la pulsation au tempo pendant un break sans kick (FORCE_PULSATION de render.rs).
const FORCE_PULSATION = 0.35;
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
// Modulations : comme src/modulation.rs (effet à fond de chaque cible : tr/min, radians/s,
// parts du rayon et de la taille, degrés ; retombée des coups : kick, caisse claire, charleston,
// drop ; suivi de la montée). La basse qui accélère et la note grave qui éloigne l'orbite sont
// deux modulations, présentes par défaut.
const MODULATION = { rotation: 90, vitesse: 40, rayon: 4, taille: 1.5, teinte: 180 };
const RETOMBEE = [7, 8, 10, 0.9];
const SUIVI_MONTEE = 6;
const effetNeutre = () => ({ rotation: 0, vitesse: 0, rayon: 1, taille: 1, teinte: 0 });
const finiOu = (v, d) => (Number.isFinite(v) ? v : d);

// Effets des modulations sur les comètes ([0]) et les satellites ([1]), comme
// modulation::effets ; `valeur(source)` donne la valeur de chaque source.
function effetsModulations(liste, valeur) {
  const somme = [[0, 0, 0, 0, 0], [0, 0, 0, 0, 0]];
  const cibles = ["rotation", "vitesse", "rayon", "taille", "teinte"];
  for (const m of Array.isArray(liste) ? liste : []) {
    const i = cibles.indexOf(m && m.cible);
    if (i < 0) continue;
    const q = Math.max(-1, Math.min(1, finiOu(+m.quantite, 0)));
    const v = q * valeur(m.source);
    const sur = m.sur || "tout";
    if (sur !== "satellites") somme[0][i] += v;
    if (sur !== "cometes") somme[1][i] += v;
  }
  return somme.map((s) => ({
    rotation: s[0] * MODULATION.rotation,
    vitesse: s[1] * MODULATION.vitesse,
    rayon: Math.min(5, Math.max(0.1, 1 + s[2] * MODULATION.rayon)),
    taille: Math.min(4, Math.max(0, 1 + s[3] * MODULATION.taille)),
    teinte: s[4] * MODULATION.teinte,
  }));
}

function hsl(h, s, l) {
  const c = (1 - Math.abs(2 * l - 1)) * s, x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
}

// Teinte : k = 0 en queue de traînée, 1 en tête (comme render::prisme). `imposee` : teinte de
// l'application au premier plan ou de l'éclairage, quand le preset la suit ; `decalage` :
// glissement musical ; `j` : teinte de la palette (comme render::prisme_palette) ; `plus` :
// degrés ajoutés par une modulation de la teinte.
function prisme(c, t, k, imposee, decalage = 0, j = 0, plus = 0) {
  const n = Math.min(3, Math.max(1, Math.round(c.palette || 1)));
  const palette = (j % n) * (c.ecart_palette ?? 120);
  const base = (imposee ?? (((c.teinte || 0) + t * c.vitesse_teinte) % 360 + 360) % 360) + (c.musique ? decalage : 0) + palette + finiOu(plus, 0);
  const queue = base + c.ecart_teinte;
  let dh = base - queue;
  if (dh > 180) dh -= 360;
  if (dh < -180) dh += 360;
  return hsl((((queue + dh * k) % 360) + 360) % 360, 1, 0.66);
}

// Trajet dessiné : rééchantillonné à 256 pas égaux, centré, rayon le plus grand 1,2 (comme
// render::reechantillonner) ; gardé par tableau de points.
const traces = new WeakMap();
function reechantillonner(points) {
  if (!Array.isArray(points) || points.length < 3) return null;
  if (traces.has(points)) return traces.get(points);
  const p = points.filter((q) => Array.isArray(q) && Number.isFinite(q[0]) && Number.isFinite(q[1])).slice(0, 2000);
  const n = p.length;
  const long = (i) => Math.hypot(p[(i + 1) % n][0] - p[i][0], p[(i + 1) % n][1] - p[i][1]);
  let total = 0;
  for (let i = 0; i < n; i++) total += long(i);
  if (n < 3 || total <= 1e-6) { traces.set(points, null); return null; }
  const out = [];
  let i = 0, fait = 0;
  for (let k = 0; k < 256; k++) {
    const cible = (total * k) / 256;
    while (i < n - 1 && fait + long(i) < cible) { fait += long(i); i++; }
    const f = long(i) > 0 ? Math.min(1, Math.max(0, (cible - fait) / long(i))) : 0;
    const a = p[i], b = p[(i + 1) % n];
    out.push([a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]);
  }
  const cx = out.reduce((s, q) => s + q[0], 0) / 256, cy = out.reduce((s, q) => s + q[1], 0) / 256;
  const r = Math.max(1e-6, ...out.map((q) => Math.hypot(q[0] - cx, q[1] - cy)));
  const res = out.map((q) => [((q[0] - cx) / r) * 1.2, ((q[1] - cy) / r) * 1.2]);
  traces.set(points, res);
  return res;
}

// Point du trajet à l'angle a, pour un rayon de 1 (comme render::chemin). `trace` : points du
// trajet dessiné, pour la forme « trace ».
function chemin(forme, a, trace) {
  if (forme === "trace") {
    const t = reechantillonner(trace);
    if (!t) return [Math.cos(a), Math.sin(a)];
    const u = (((a / TAU) * 256) % 256 + 256) % 256, i = Math.floor(u), f = u - i;
    const p = t[i % 256], q = t[(i + 1) % 256];
    return [p[0] + (q[0] - p[0]) * f, p[1] + (q[1] - p[1]) * f];
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
    this.spd = 3.5;
    this.far = 1;
    this.angles = [];
    this.rot = 0;
    this.rotSats = 0;
    // modulations : enveloppes des coups et montée suivie (modulation::Sources), effets lissés
    // sur les comètes et les satellites, à cette image et à la précédente
    this.sources = { kick: 0, caisse: 0, charleston: 0, drop: 0, montee: 0 };
    this.mods = [effetNeutre(), effetNeutre()];
    this.modsAvant = [effetNeutre(), effetNeutre()];
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
    this.aigusVus = null;
    this.decalage = 0;
    this.kicksVus = null;
    // caisses claires et drops vus, montée suivie, contretemps, décalage stéréo du centre
    this.caissesVues = null;
    this.dropsVus = null;
    this.tension = 0;
    this.contre = 0;
    this.phaseAvant = null;
    this.penche = 0;
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
    const liste = (this.reglages.modulations || {}).liste;
    const rayonMax = effetsModulations(liste, () => 1).concat(effetsModulations(liste, () => -1)).reduce((m, e) => Math.max(m, e.rayon), 1);
    const portee = o.rayon * Math.max(SAT_R_MAX, anneau(o.anneaux - 1)[0]) * 1.3 * (1 + (rayonMax - 1) * 0.7) + 6;
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
  // vrai essaim), avec dans chacun les points intermédiaires de render.rs.
  avancer(dt, son) {
    if (!this.reglages) return;
    this.sillage(dt);
    const n = Math.max(1, Math.ceil(dt / PAS_TRAINEE));
    for (let i = 0; i < n; i++) {
      this.pas(dt / n, i === 0 ? son : { ...son, kicks: this.kicksVus ?? son.kicks, caisses: this.caissesVues ?? son.caisses, drops: this.dropsVus ?? son.drops });
    }
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
    const c = this.reglages, e = c.essaim, o = c.orbite, m = c.musique || {};
    this.t += dt;
    const t = this.t;
    const S = this.echelle || 1;
    // centre de l'orbite : penche vers le côté qui joue le plus fort (le pointeur reste au centre)
    const penche = Math.max(-1, Math.min(1, son.balance || 0)) * Math.max(0, Math.min(1, m.stereo || 0)) * o.rayon * 1.6 * S;
    this.penche += (penche - this.penche) * (1 - Math.exp(-dt * 14));
    const cx = this.canvas.width / 2 + this.penche, cy = this.canvas.height / 2;
    const far0 = this.far;
    // kicks : sens inversé + satellites
    if (this.kicksVus === null) this.kicksVus = son.kicks;
    const nouveaux = Math.min(4, (son.kicks - this.kicksVus) >>> 0);
    this.kicksVus = son.kicks;
    for (let i = 0; i < nouveaux; i++) {
      if (o.inverse_au_kick !== false) this.dir = -this.dir;
      this.env = 1;
      const bas = Math.min(e.par_kick_min, e.par_kick_max), haut = Math.max(e.par_kick_min, e.par_kick_max);
      this.ajouterSatellites(Math.floor(entre(bas, haut + 0.999)), t, e.maximum);
    }
    this.env *= Math.exp(-dt * 9);
    // relais du tempo (break sans kick) : l'éclat pulse doucement sur chaque temps (comme
    // pulsation_tempo de render.rs)
    if (son.relais && c.audio && c.audio.pulsation_tempo !== false) {
      this.env = Math.max(this.env, FORCE_PULSATION * Math.pow(1 - Math.min(1, Math.max(0, son.phase || 0)), 4));
    }
    // caisse claire : un carré s'ouvre sous le pointeur
    if (this.caissesVues === null) this.caissesVues = son.caisses || 0;
    const caisses = Math.min(4, ((son.caisses || 0) - this.caissesVues) >>> 0);
    this.caissesVues = son.caisses || 0;
    const px = this.canvas.width / 2, py = this.canvas.height / 2;
    if (m.caisse_claire && caisses > 0) {
      this.particules.push({ x: px, y: py, vx: 0, vy: 0, ne: t, vie: CARRE_CAISSE[1], taille: CARRE_CAISSE[0] * S, genre: "carre" });
    }
    // montée suivie en douceur ; drop : l'essaim explose
    if (this.dropsVus === null) this.dropsVus = son.drops || 0;
    const drops = Math.min(4, ((son.drops || 0) - this.dropsVus) >>> 0);
    this.dropsVus = son.drops || 0;
    const tensionVisee = m.montee ? Math.max(0, Math.min(1, son.tension || 0)) : 0;
    this.tension += (tensionVisee - this.tension) * Math.min(1, dt * 6);
    if (m.montee && drops > 0) this.exploser(px, py, cx, cy, t);
    // contretemps (milieu du temps, tempo connu) : les têtes pulsent
    this.contre *= Math.exp(-dt * 10);
    const phase = son.bpm ? son.phase || 0 : null;
    if (m.contretemps && phase !== null && this.phaseAvant !== null && this.phaseAvant < 0.5 && phase >= 0.5 && phase - this.phaseAvant < 0.4) this.contre = 1;
    this.phaseAvant = phase;
    // aigus : étincelles près des têtes (ou sur l'orbite s'il n'y en a pas)
    if (this.aigusVus === null) this.aigusVus = son.aigus || 0;
    const aigus = Math.min(4, ((son.aigus || 0) - this.aigusVus) >>> 0);
    this.aigusVus = son.aigus || 0;
    const sc = c.scintillement;
    if (sc && sc.actif && aigus > 0) {
      const tetes = this.trainees.concat(this.sats.map((x) => x.trainee), this.fixes.map((x) => x.trainee)).map((tr) => tr[tr.length - 1]).filter(Boolean);
      const quantite = Math.round(Math.min(10, Math.max(1, sc.quantite)));
      for (let n = 0; n < Math.min(aigus * quantite, MAX_PARTICULES); n++) {
        let x, y;
        if (tetes.length) {
          [, x, y] = tetes[Math.floor(Math.random() * tetes.length) % tetes.length];
        } else {
          const a = entre(0, TAU);
          x = cx + Math.cos(a) * o.rayon * S;
          y = cy + Math.sin(a) * o.rayon * S;
        }
        const a = entre(0, TAU), v = entre(30, 90) * S;
        const vie = entre(0.3, 0.55), taille = entre(1.1, 2.1) * S;
        this.particules.push({ x, y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, ne: t, vie, taille, genre: "etincelle" });
      }
    }
    // particules : avancent en ralentissant, puis s'éteignent (les plus vieilles en trop s'en vont)
    const frein = Math.exp(-dt * 3);
    for (const q of this.particules) { q.x += q.vx * dt; q.y += q.vy * dt; q.vx *= frein; q.vy *= frein; }
    this.particules = this.particules.filter((q) => t - q.ne < q.vie);
    if (this.particules.length > MAX_PARTICULES) this.particules.splice(0, this.particules.length - MAX_PARTICULES);
    // basse : vitesse ; note grave : éloignement
    if (son.hauteur > 0) this.hauteur += (son.hauteur - this.hauteur) * Math.min(dt * 25, 1);
    const grave = Math.max(0, Math.min(1, (80 - this.hauteur) / 50));
    // montée : plus vite et plus serré ; contretemps : les orbites s'écartent un instant. La
    // basse (vitesse) et la note grave (rayon) agissent par les modulations, plus bas.
    const tension = Math.max(0, Math.min(1, this.tension));
    const accel = 1 + 1.2 * tension;
    this.spd += (o.vitesse_base * accel - this.spd) * Math.min(dt * 10, 1);
    const presence = Math.max(0, Math.min(1, son.basse * 2.5));
    const loin = (1 - 0.45 * tension) * (1 + 0.06 * this.contre);
    this.far += (loin - this.far) * Math.min(dt * 16, 1);
    // modulations : sources du moment, effets lissés (~50 ms) sur les comètes et les satellites
    this.avancerSources(dt, [nouveaux, caisses, aigus, drops], son.tension || 0);
    this.sonVu = son;
    this.grave = grave;
    this.presence = presence;
    const viseMods = effetsModulations((c.modulations || {}).liste, (src) => this.valeurSource(src));
    this.modsAvant = this.mods.map((m) => ({ ...m }));
    const km = Math.min(1, dt * 20);
    this.mods = this.mods.map((m, g) => {
      const r = {};
      for (const cle of Object.keys(m)) r[cle] = m[cle] + (viseMods[g][cle] - m[cle]) * km;
      return Object.values(r).every(Number.isFinite) ? r : effetNeutre();
    });
    // comètes : `nombre` par anneau, réparties sur le trajet
    const nAnneaux = Math.max(1, o.anneaux), nombre = Math.max(1, o.nombre);
    while (this.angles.length < nAnneaux) this.angles.push(0);
    this.angles.length = nAnneaux;
    while (this.trainees.length < nAnneaux * nombre) this.trainees.push([]);
    this.trainees.length = nAnneaux * nombre;
    const vie = Math.max(0.01, o.trainee);
    // position sur le trajet à la fraction `f` de l'image (rayon et rotation interpolés) : les
    // points intermédiaires suivent la vraie forme, coins compris
    // rotation du trajet (`rotation_sur` : comètes, satellites ou tout) et modulations : une pose
    // par groupe (rotation et rayon au début et à la fin de l'image), comme render::Pose
    const rayon = o.rayon * S * this.far, rayon0 = o.rayon * S * far0;
    const sur = o.rotation_sur || "tout";
    const pose = (g, rot) => {
      const base = (g === 1 ? sur !== "cometes" : sur !== "satellites") ? o.rotation || 0 : 0;
      const drot = finiOu((((base + this.mods[g].rotation) * TAU) / 60) * dt, 0);
      return { rot0: finiOu(rot, 0), drot, rayon0: rayon0 * this.modsAvant[g].rayon, rayon: rayon * this.mods[g].rayon };
    };
    const poses = [pose(0, this.rot), pose(1, this.rotSats)];
    this.rot = reste(poses[0].rot0 + poses[0].drot, TAU);
    this.rotSats = reste(poses[1].rot0 + poses[1].drot, TAU);
    const point = (p, forme, a, rel, f) => {
      const [x, y] = chemin(forme, a, o.trace);
      const th = p.rot0 + p.drot * f, cos = Math.cos(th), sin = Math.sin(th);
      const rr = (p.rayon0 + (p.rayon - p.rayon0) * f) * rel;
      return [cx + (x * cos - y * sin) * rr, cy + (x * sin + y * cos) * rr];
    };
    // vitesse : celle du trajet plus celle des modulations (accélérée par la montée), jamais négative
    const vitesse = (g) => this.dir * Math.max(0, this.spd + this.mods[g].vitesse * accel);
    const wCometes = vitesse(0), wSats = vitesse(1);
    // les écarts de render.rs sont en pixels d'écran : ici, à l'échelle de l'aperçu
    const finCometes = [FINESSE_COMETES[0], FINESSE_COMETES[1] * S], finSats = [FINESSE_SATELLITES[0], FINESSE_SATELLITES[1] * S];
    for (let k = 0; k < nAnneaux; k++) {
      const [rk, wk] = anneau(k);
      const a0 = this.angles[k], da = wCometes * wk * dt;
      this.angles[k] = reste(a0 + da, TAU);
      const n = sousPas(da, poses[0].rayon * rk * 1.2, finCometes);
      for (let i = 0; i < nombre; i++) {
        const decale = (i * TAU) / nombre;
        for (let j = 1; j <= n; j++) {
          const f = j / n, [x, y] = point(poses[0], o.forme, a0 + da * f + decale, rk, f);
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
    for (const s of this.sats.concat(this.fixes)) {
      const a0 = s.a, da = wSats * s.mul * dt;
      s.a = reste(a0 + da, TAU);
      const n = sousPas(da, poses[1].rayon * s.r * 1.2, finSats);
      for (let j = 1; j <= n; j++) {
        const f = j / n, [x, y] = point(poses[1], forme, a0 + da * f, s.r, f);
        pousser(s.trainee, t - dt * (1 - f), x, y, SAT_TRAINEE, finSats[1]);
      }
    }
    // glissement de teinte avec la musique (render::decalage_vise, lissé comme dans main.rs)
    const vise = 160 * (0.6 * Math.max(0, Math.min(1, son.basse)) + 0.4 * Math.max(0, Math.min(1, son.brillance || 0)));
    this.decalage += (vise - this.decalage) * Math.min(dt * 6, 1);
  }

  // Enveloppes des coups (`coups` : nouveaux kicks, caisses claires, attaques des aigus, drops)
  // et montée suivie, comme modulation::Sources::avancer.
  avancerSources(dt, coups, tension) {
    const s = this.sources;
    ["kick", "caisse", "charleston", "drop"].forEach((nom, i) => {
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
      kick: s.kick, caisse: s.caisse, charleston: s.charleston, montee: s.montee, drop: s.drop,
      stereo: Math.max(-1, Math.min(1, son.balance || 0)),
      tempo: son.bpm ? 0.5 + 0.5 * Math.cos(TAU * (son.phase || 0)) : 0,
    }[source];
    return finiOu(v, 0);
  }

  // Effets d'un clic au centre (onde de choc, éclats), comme Swarm::clic.
  clic() {
    if (!this.reglages || !this.reglages.clic) return;
    const k = this.reglages.clic, S = this.echelle || 1, t = this.t;
    const cx = this.canvas.width / 2, cy = this.canvas.height / 2, taille = Math.min(3, Math.max(0.3, k.taille));
    if (k.onde) this.particules.push({ x: cx, y: cy, vx: 0, vy: 0, ne: t, vie: 0.55, taille: 46 * taille * S, genre: "onde" });
    if (k.eclats) {
      for (let i = 0; i < 16; i++) {
        const a = entre(0, TAU), v = entre(160, 380) * taille * S;
        this.particules.push({ x: cx, y: cy, vx: Math.cos(a) * v, vy: Math.sin(a) * v, ne: t, vie: entre(0.45, 0.8), taille: entre(1.2, 2.4) * S, genre: "etincelle" });
      }
    }
  }

  // Clic droit au centre, comme Swarm::clic_droit : un anneau qui se referme et des étincelles
  // aspirées vers le pointeur.
  clicDroit() {
    if (!this.reglages || !this.reglages.clic || this.reglages.clic.droit === false) return;
    const S = this.echelle || 1, t = this.t, cx = this.canvas.width / 2, cy = this.canvas.height / 2;
    const r = 52 * Math.min(3, Math.max(0.3, this.reglages.clic.taille)) * S;
    this.particules.push({ x: cx, y: cy, vx: 0, vy: 0, ne: t, vie: 0.45, taille: r, genre: "implosion" });
    for (let i = 0; i < 12; i++) {
      const a = (i * TAU) / 12 + entre(-0.2, 0.2), cos = Math.cos(a), sin = Math.sin(a), v = entre(2.2, 3) * r;
      this.particules.push({ x: cx + cos * r, y: cy + sin * r, vx: -cos * v, vy: -sin * v, ne: t, vie: 0.4, taille: entre(1, 1.8) * S, genre: "etincelle" });
    }
  }

  // Rafale d'un changement de piste au centre, comme Swarm::rafale : l'éclat d'un kick, deux fois
  // les satellites d'un kick fort (dans la limite du maximum) et une grande onde.
  rafale() {
    if (!this.reglages) return;
    const e = this.reglages.essaim, S = this.echelle || 1, t = this.t;
    const cx = this.canvas.width / 2, cy = this.canvas.height / 2;
    this.env = 1;
    this.ajouterSatellites(2 * Math.max(2, e.par_kick_min, e.par_kick_max), t, e.maximum);
    this.particules.push({ x: cx, y: cy, vx: 0, vy: 0, ne: t, vie: ONDE_PISTE[1], taille: ONDE_PISTE[0] * S, genre: "onde" });
  }

  // Drop, comme Swarm::exploser : l'éclat d'un kick, deux fois les satellites d'un kick fort, une
  // grande onde sous le pointeur (px, py) et des étincelles projetées depuis le centre de l'orbite.
  exploser(px, py, cx, cy, t) {
    const e = this.reglages.essaim, S = this.echelle || 1;
    this.env = 1;
    this.ajouterSatellites(2 * Math.max(2, e.par_kick_min, e.par_kick_max), t, e.maximum);
    this.particules.push({ x: px, y: py, vx: 0, vy: 0, ne: t, vie: ONDE_DROP[1], taille: ONDE_DROP[0] * S, genre: "onde" });
    for (let i = 0; i < ETINCELLES_DROP; i++) {
      const a = (i * TAU) / ETINCELLES_DROP + entre(-0.1, 0.1), v = entre(220, 420) * S;
      this.particules.push({ x: cx, y: cy, vx: Math.cos(a) * v, vy: Math.sin(a) * v, ne: t, vie: entre(0.5, 0.9), taille: entre(1.2, 2.2) * S, genre: "etincelle" });
    }
  }

  // Dessin, dans l'ordre de Swarm::draw ; le pointeur (absent du vrai essaim : c'est celui de
  // Windows) est ajouté au centre, par-dessus.
  dessiner() {
    if (!this.reglages) return;
    const g = this.ctx, c = this.reglages, t = this.t, S = this.echelle || 1;
    const couleur = (k) => prisme(c.couleur, t, k, this.teinteImposee, this.decalage);
    g.clearRect(0, 0, this.canvas.width, this.canvas.height);
    g.lineCap = "round";
    g.lineJoin = "round";
    const teinte = (k, j, plus = 0) => prisme(c.couleur, t, k, this.teinteImposee, this.decalage, j, plus);
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
    const tetes = [];
    // contretemps : les têtes grossissent un instant
    const pulse = 1 + 0.5 * this.contre;
    // modulations : taille (têtes et traînées) et teinte de chaque groupe
    const [mc, ms] = this.mods;
    // satellites (sans halo) ; les permanents restent à 0,8
    for (const s of this.sats.concat(this.fixes)) {
      const k = Number.isFinite(s.ne) ? 1 - (t - s.ne) / duree : 0.8, al = Math.min(1, k * 1.5);
      trainee(s.trainee, SAT_TRAINEE, 2.6 * k * S * ms.taille, al, false, s.couleur || 0, ms.teinte);
      const p = s.trainee[s.trainee.length - 1];
      if (p) tetes.push({ x: p[1], y: p[2], s: (0.6 + 1.6 * k) * S * pulse * ms.taille, al, devant: p[2] > cy, rgb: teinte(1, s.couleur || 0, ms.teinte) });
    }
    // les comètes (devant : sous le centre de l'orbite, comme vues de biais), chacune dans une
    // teinte de la palette
    if (c.orbite.cometes) {
      this.trainees.forEach((tr, i) => {
        trainee(tr, Math.max(0.01, c.orbite.trainee), 3.5 * S * mc.taille, 1, true, i, mc.teinte);
        const p = tr[tr.length - 1];
        if (p) tetes.push({ x: p[1], y: p[2], s: 2.4 * S * pulse * mc.taille, al: 1, devant: p[2] > cy, rgb: teinte(1, i, mc.teinte) });
      });
    }
    // constellation : fines lignes entre les têtes proches (comme render::liens), sous les têtes
    if (c.essaim.constellation) {
      const portee = LIENS_PORTEE * c.orbite.rayon * S * Math.max(0.2, this.far), lien = couleur(0.85);
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
      disque(h.x, h.y, h.s * 2.6, h.rgb, 0.12 * h.al * lueur);
      disque(h.x, h.y, h.s * 1.6, h.rgb, 0.3 * h.al * lueur);
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
window.imageA = imageA;

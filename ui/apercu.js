// Aperçu d'un preset : le même calcul que l'essaim autour du pointeur (src/render.rs), dessiné
// dans un canevas. Kick : inverse le sens et ajoute des satellites ; basse : vitesse ; note
// grave : orbites plus larges. Le pointeur est au centre. Toute formule de render.rs (Swarm::update,
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

function hsl(h, s, l) {
  const c = (1 - Math.abs(2 * l - 1)) * s, x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
}

// Teinte : k = 0 en queue de traînée, 1 en tête (comme render::prisme). `imposee` : teinte de
// l'application au premier plan ou de l'éclairage, quand le preset la suit ; `decalage` :
// glissement musical.
function prisme(c, t, k, imposee, decalage = 0) {
  const base = (imposee ?? (((c.teinte || 0) + t * c.vitesse_teinte) % 360 + 360) % 360) + (c.musique ? decalage : 0);
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
    // souris sur le canevas (pixels du canevas), pour le sillage
    this.pointeur = null;
    this.pointeurAvant = null;
    this.t = 0;
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
    const portee = o.rayon * Math.max(SAT_R_MAX, anneau(o.anneaux - 1)[0]) * 1.3 * (1 + o.eloignement_grave * 0.7) + 6;
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
    for (let i = 0; i < n; i++) this.pas(dt / n, i === 0 ? son : { ...son, kicks: this.kicksVus ?? son.kicks });
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

  // Une image du vrai essaim (Swarm::update), dans le même ordre ; le pointeur est au centre.
  pas(dt, son) {
    const c = this.reglages, e = c.essaim, o = c.orbite;
    this.t += dt;
    const t = this.t;
    const cx = this.canvas.width / 2, cy = this.canvas.height / 2, S = this.echelle || 1;
    const far0 = this.far;
    // kicks : sens inversé + satellites
    if (this.kicksVus === null) this.kicksVus = son.kicks;
    const nouveaux = Math.min(4, (son.kicks - this.kicksVus) >>> 0);
    this.kicksVus = son.kicks;
    for (let i = 0; i < nouveaux; i++) {
      this.dir = -this.dir;
      this.env = 1;
      const bas = Math.min(e.par_kick_min, e.par_kick_max), haut = Math.max(e.par_kick_min, e.par_kick_max);
      const n = Math.floor(entre(bas, haut + 0.999));
      for (let j = 0; j < n && this.sats.length < e.maximum; j++) {
        this.sats.push({ a: entre(0, TAU), r: entre(0.75, SAT_R_MAX), mul: entre(0.7, 1.3), ne: t, trainee: [] });
      }
    }
    this.env *= Math.exp(-dt * 9);
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
    this.spd += (o.vitesse_base + o.vitesse_basse * son.basse - this.spd) * Math.min(dt * 10, 1);
    const presence = Math.max(0, Math.min(1, son.basse * 2.5));
    this.far += (1 + o.eloignement_grave * grave * presence - this.far) * Math.min(dt * 16, 1);
    const w = this.dir * this.spd;
    // comètes : `nombre` par anneau, réparties sur le trajet
    const nAnneaux = Math.max(1, o.anneaux), nombre = Math.max(1, o.nombre);
    while (this.angles.length < nAnneaux) this.angles.push(0);
    this.angles.length = nAnneaux;
    while (this.trainees.length < nAnneaux * nombre) this.trainees.push([]);
    this.trainees.length = nAnneaux * nombre;
    const vie = Math.max(0.01, o.trainee);
    // position sur le trajet à la fraction `f` de l'image (rayon et rotation interpolés) : les
    // points intermédiaires suivent la vraie forme, coins compris
    const rayon = o.rayon * S * this.far, rayon0 = o.rayon * S * far0;
    const rot0 = this.rot, drot = (((o.rotation || 0) * TAU) / 60) * dt;
    this.rot = reste(rot0 + drot, TAU);
    const point = (forme, a, rel, f) => {
      const [x, y] = chemin(forme, a, o.trace);
      const th = rot0 + drot * f, cos = Math.cos(th), sin = Math.sin(th);
      const rr = (rayon0 + (rayon - rayon0) * f) * rel;
      return [cx + (x * cos - y * sin) * rr, cy + (x * sin + y * cos) * rr];
    };
    // les écarts de render.rs sont en pixels d'écran : ici, à l'échelle de l'aperçu
    const finCometes = [FINESSE_COMETES[0], FINESSE_COMETES[1] * S], finSats = [FINESSE_SATELLITES[0], FINESSE_SATELLITES[1] * S];
    for (let k = 0; k < nAnneaux; k++) {
      const [rk, wk] = anneau(k);
      const a0 = this.angles[k], da = w * wk * dt;
      this.angles[k] = reste(a0 + da, TAU);
      const n = sousPas(da, rayon * rk * 1.2, finCometes);
      for (let i = 0; i < nombre; i++) {
        const decale = (i * TAU) / nombre;
        for (let j = 1; j <= n; j++) {
          const f = j / n, [x, y] = point(o.forme, a0 + da * f + decale, rk, f);
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
        return { a: j * 2.399963, r: 0.6 + u * (SAT_R_MAX - 0.6), mul: 0.6 + 0.8 * (1 - u), ne: Infinity, trainee: [] };
      });
    }
    // satellites : sur le trajet des comètes, ou le leur, à des distances variées
    const forme = !e.forme || e.forme === "cometes" ? o.forme : e.forme;
    for (const s of this.sats.concat(this.fixes)) {
      const a0 = s.a, da = w * s.mul * dt;
      s.a = reste(a0 + da, TAU);
      const n = sousPas(da, rayon * s.r * 1.2, finSats);
      for (let j = 1; j <= n; j++) {
        const f = j / n, [x, y] = point(forme, a0 + da * f, s.r, f);
        pousser(s.trainee, t - dt * (1 - f), x, y, SAT_TRAINEE, finSats[1]);
      }
    }
    // glissement de teinte avec la musique (render::decalage_vise, lissé comme dans main.rs)
    const vise = 160 * (0.6 * Math.max(0, Math.min(1, son.basse)) + 0.4 * Math.max(0, Math.min(1, son.brillance || 0)));
    this.decalage += (vise - this.decalage) * Math.min(dt * 6, 1);
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

  // Dessin, dans l'ordre de Swarm::draw ; le pointeur (absent du vrai essaim : c'est celui de
  // Windows) est ajouté au centre, par-dessus.
  dessiner() {
    if (!this.reglages) return;
    const g = this.ctx, c = this.reglages, t = this.t, S = this.echelle || 1;
    const couleur = (k) => prisme(c.couleur, t, k, this.teinteImposee, this.decalage);
    g.clearRect(0, 0, this.canvas.width, this.canvas.height);
    g.lineCap = "round";
    g.lineJoin = "round";
    const ligne = (tr, debut, fin) => {
      g.beginPath();
      g.moveTo(tr[debut][1], tr[debut][2]);
      for (let j = debut + 1; j <= fin; j++) g.lineTo(tr[j][1], tr[j][2]);
      g.stroke();
    };
    // traînée par morceaux (render::draw_trail) : comètes par TRAIL_CHUNK segments, satellites en
    // deux morceaux ; chaque morceau prend la couleur, l'épaisseur et l'opacité de son milieu
    const trainee = (tr, vie, largeur, al, halo) => {
      if (tr.length < 2) return;
      const q = tr.map((p) => Math.max(0, 1 - (t - p[0]) / vie));
      if (halo) {
        g.strokeStyle = rgba(couleur(0.7), 0.12 * al);
        g.lineWidth = (0.8 + largeur * 0.7) * 2.6;
        ligne(tr, 0, tr.length - 1);
      }
      const morceau = halo ? TRAIL_CHUNK : Math.max(1, Math.ceil((tr.length - 1) / 2));
      for (let i = 0; i + 1 < tr.length;) {
        const fin = Math.min(i + morceau, tr.length - 1), k = (q[i] + q[fin]) * 0.5;
        g.strokeStyle = rgba(couleur(k), k * 0.9 * al);
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
    const tetes = [];
    // satellites (sans halo) ; les permanents restent à 0,8
    for (const s of this.sats.concat(this.fixes)) {
      const k = Number.isFinite(s.ne) ? 1 - (t - s.ne) / duree : 0.8, al = Math.min(1, k * 1.5);
      trainee(s.trainee, SAT_TRAINEE, 2.6 * k * S, al, false);
      const p = s.trainee[s.trainee.length - 1];
      if (p) tetes.push({ x: p[1], y: p[2], s: (0.6 + 1.6 * k) * S, al, devant: p[2] > cy });
    }
    // les comètes (devant : sous le centre de l'orbite, comme vues de biais)
    if (c.orbite.cometes) {
      for (const tr of this.trainees) {
        trainee(tr, Math.max(0.01, c.orbite.trainee), 3.5 * S, 1, true);
        const p = tr[tr.length - 1];
        if (p) tetes.push({ x: p[1], y: p[2], s: 2.4 * S, al: 1, devant: p[2] > cy });
      }
    }
    const dessinerTete = (h) => {
      disque(h.x, h.y, h.s * 2.6, tete, 0.12 * h.al * lueur);
      disque(h.x, h.y, h.s * 1.6, tete, 0.3 * h.al * lueur);
      disque(h.x, h.y, h.s, tete, h.al);
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

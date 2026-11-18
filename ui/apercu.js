// Aperçu d'un preset : le même calcul que l'essaim autour du pointeur (src/render.rs), dessiné
// dans un canevas. Kick : inverse le sens et ajoute des satellites ; basse : vitesse ; note
// grave : orbites plus larges. Le pointeur est au centre.
"use strict";

const TAU = Math.PI * 2;
const SAT_R_MAX = 1.9;
const SAT_TRAINEE = 0.25;
const PAS_TRAINEE = 1 / 90;

function hsl(h, s, l) {
  const c = (1 - Math.abs(2 * l - 1)) * s, x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
}

// Teinte : k = 0 en queue de traînée, 1 en tête (comme render::prisme). `imposee` : teinte de
// l'application au premier plan, quand le preset la suit.
function prisme(c, t, k, imposee) {
  const base = imposee ?? (((c.teinte || 0) + t * c.vitesse_teinte) % 360 + 360) % 360;
  const queue = base + c.ecart_teinte;
  let dh = base - queue;
  if (dh > 180) dh -= 360;
  if (dh < -180) dh += 360;
  return hsl((((queue + dh * k) % 360) + 360) % 360, 1, 0.66);
}

// Point du trajet à l'angle a, pour un rayon de 1 (comme render::chemin).
function chemin(forme, a) {
  const poly = (k, r, r2) => {
    const u = (((a / TAU) * k) % k + k) % k, j = Math.floor(u), f = u - j;
    const v = (j) => { const th = -Math.PI / 2 + (j * TAU) / k, rr = j % 2 === 0 ? r : r2; return [Math.cos(th) * rr, Math.sin(th) * rr]; };
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
    default: return [Math.cos(a), Math.sin(a)];
  }
}

// Rayon et vitesse relatifs de l'anneau k (comme render::anneau).
const anneau = (k) => [1 + 0.55 * k, 1 / (1 + 0.45 * k)];

function rgba([r, g, b], a) {
  return `rgba(${r},${g},${b},${Math.max(0, Math.min(1, a))})`;
}

function pousser(tr, t, x, y, vie) {
  const n = tr.length;
  if (n >= 2 && t - tr[n - 2][0] < PAS_TRAINEE) tr[n - 1] = [t, x, y];
  else tr.push([t, x, y]);
  while (tr.length && t - tr[0][0] > vie) tr.shift();
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
    this.hauteur = 60;
    this.env = 0;
    this.trainees = [];
    this.sats = [];
    this.fixes = [];
    this.teinteImposee = null;
    this.curseur = null;
    this.kicksVus = null;
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
    if (this.canvas.width !== w) {
      this.canvas.width = w;
      this.canvas.height = w;
    }
    const o = this.reglages.orbite;
    const portee = o.rayon * Math.max(SAT_R_MAX, anneau(o.anneaux - 1)[0]) * 1.3 * (1 + o.eloignement_grave * 0.7) + 6;
    this.echelle = Math.min(2.2 * dpr, (w * 0.46) / portee);
  }

  // Les vignettes avancent par pas de 1/30 s : découpés en pas de 1/90 s (celui des traînées
  // du vrai essaim), sinon les orbites deviennent des polygones.
  avancer(dt, son) {
    const n = Math.max(1, Math.ceil(dt / PAS_TRAINEE));
    for (let i = 0; i < n; i++) this.pas(dt / n, i === 0 ? son : { ...son, kicks: this.kicksVus ?? son.kicks });
  }

  pas(dt, son) {
    if (!this.reglages) return;
    const c = this.reglages, e = c.essaim, o = c.orbite;
    this.t += dt;
    const t = this.t;
    if (this.kicksVus === null) this.kicksVus = son.kicks;
    const nouveaux = Math.min(4, (son.kicks - this.kicksVus) >>> 0);
    this.kicksVus = son.kicks;
    for (let i = 0; i < nouveaux; i++) {
      this.dir = -this.dir;
      this.env = 1;
      const bas = Math.min(e.par_kick_min, e.par_kick_max), haut = Math.max(e.par_kick_min, e.par_kick_max);
      const n = Math.floor(bas + Math.random() * (haut - bas + 0.999));
      for (let j = 0; j < n && this.sats.length < e.maximum; j++) {
        this.sats.push({ a: Math.random() * TAU, r: 0.75 + Math.random() * (SAT_R_MAX - 0.75), mul: 0.7 + Math.random() * 0.6, ne: t, trainee: [] });
      }
    }
    this.env *= Math.exp(-dt * 9);
    if (son.hauteur > 0) this.hauteur += (son.hauteur - this.hauteur) * Math.min(dt * 25, 1);
    const grave = Math.max(0, Math.min(1, (80 - this.hauteur) / 50));
    this.spd += (o.vitesse_base + o.vitesse_basse * son.basse - this.spd) * Math.min(dt * 10, 1);
    const presence = Math.max(0, Math.min(1, son.basse * 2.5));
    this.far += (1 + o.eloignement_grave * grave * presence - this.far) * Math.min(dt * 16, 1);
    const w = this.dir * this.spd;
    const cx = this.canvas.width / 2, cy = this.canvas.height / 2, S = this.echelle || 1;
    const r = o.rayon * S * this.far;
    const nAnneaux = Math.max(1, o.anneaux), nombre = Math.max(1, o.nombre);
    // rotation du trajet lui-même (comme render.rs)
    this.rot = ((this.rot || 0) + ((o.rotation || 0) * TAU) / 60 * dt) % TAU;
    const tourne = ([x, y]) => [x * Math.cos(this.rot) - y * Math.sin(this.rot), x * Math.sin(this.rot) + y * Math.cos(this.rot)];
    while (this.angles.length < nAnneaux) this.angles.push(0);
    this.angles.length = nAnneaux;
    while (this.trainees.length < nAnneaux * nombre) this.trainees.push([]);
    this.trainees.length = nAnneaux * nombre;
    for (let k = 0; k < nAnneaux; k++) {
      const [rk, wk] = anneau(k);
      this.angles[k] = (this.angles[k] + w * wk * dt) % TAU;
      for (let i = 0; i < nombre; i++) {
        const [x, y] = tourne(chemin(o.forme, this.angles[k] + (i * TAU) / nombre));
        pousser(this.trainees[k * nombre + i], t, cx + x * r * rk, cy + y * r * rk, Math.max(0.01, o.trainee));
      }
    }
    const duree = Math.max(0.1, e.duree);
    this.sats = this.sats.filter((s) => t - s.ne < duree);
    if (this.fixes.length !== e.fixes) {
      this.fixes = Array.from({ length: e.fixes }, (_, j) => {
        const u = (j * 0.618034) % 1;
        return { a: j * 2.399963, r: 0.6 + u * (SAT_R_MAX - 0.6), mul: 0.6 + 0.8 * (1 - u), ne: Infinity, trainee: [] };
      });
    }
    // satellites : sur le trajet des comètes, ou le leur (comme render.rs)
    const forme = !e.forme || e.forme === "cometes" ? o.forme : e.forme;
    for (const s of this.sats.concat(this.fixes)) {
      s.a = (s.a + w * s.mul * dt) % TAU;
      const rr = o.rayon * S * s.r * this.far;
      const [x, y] = tourne(chemin(forme, s.a));
      pousser(s.trainee, t, cx + x * rr, cy + y * rr, SAT_TRAINEE);
    }
  }

  dessiner() {
    if (!this.reglages) return;
    const g = this.ctx, c = this.reglages, t = this.t, S = this.echelle || 1;
    g.clearRect(0, 0, this.canvas.width, this.canvas.height);
    g.lineCap = "round";
    g.lineJoin = "round";
    const trainee = (tr, vie, largeur, al, halo) => {
      if (tr.length < 2) return;
      if (halo) {
        g.strokeStyle = rgba(prisme(c.couleur, t, 0.7, this.teinteImposee), 0.12 * al);
        g.lineWidth = (0.8 + largeur * 0.7) * 2.6;
        g.beginPath();
        tr.forEach(([, x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
        g.stroke();
      }
      for (let i = 0; i + 1 < tr.length; i++) {
        const k = Math.max(0, 1 - (t - tr[i + 1][0]) / vie);
        g.strokeStyle = rgba(prisme(c.couleur, t, k, this.teinteImposee), k * 0.9 * al);
        g.lineWidth = 0.8 + k * largeur;
        g.beginPath();
        g.moveTo(tr[i][1], tr[i][2]);
        g.lineTo(tr[i + 1][1], tr[i + 1][2]);
        g.stroke();
      }
    };
    const disque = (x, y, r, couleur, a) => {
      g.fillStyle = rgba(couleur, a);
      g.beginPath();
      g.arc(x, y, Math.max(0.1, r), 0, TAU);
      g.fill();
    };
    const lueur = 0.4 + 0.6 * this.env, tete = prisme(c.couleur, t, 1, this.teinteImposee), duree = Math.max(0.1, c.essaim.duree);
    const tetes = [];
    for (const s of this.sats.concat(this.fixes)) {
      const k = Number.isFinite(s.ne) ? 1 - (t - s.ne) / duree : 0.8, al = Math.min(1, k * 1.5);
      trainee(s.trainee, SAT_TRAINEE, 2.6 * k * S, al, false);
      const p = s.trainee[s.trainee.length - 1];
      if (p) tetes.push({ x: p[1], y: p[2], s: (0.6 + 1.6 * k) * S, al, devant: p[2] > this.canvas.height / 2 });
    }
    if (c.orbite.cometes) {
      this.trainees.forEach((tr) => {
        trainee(tr, Math.max(0.01, c.orbite.trainee), 3.5 * S, 1, true);
        const p = tr[tr.length - 1];
        if (p) tetes.push({ x: p[1], y: p[2], s: 2.4 * S, al: 1, devant: p[2] > this.canvas.height / 2 });
      });
    }
    const dessinerTete = (h) => {
      disque(h.x, h.y, h.s * 2.6, tete, 0.12 * h.al * lueur);
      disque(h.x, h.y, h.s * 1.6, tete, 0.3 * h.al * lueur);
      disque(h.x, h.y, h.s, tete, h.al);
      disque(h.x, h.y, h.s * 0.45, [255, 255, 255], h.al);
    };
    tetes.filter((h) => !h.devant).forEach(dessinerTete);
    const cx = this.canvas.width / 2, cy = this.canvas.height / 2;
    const eclat = c.eclat.taille * S;
    if (this.env > 0.02 && eclat > 0) {
      disque(cx, cy, eclat * (2 + 2.5 * this.env), [255, 255, 255], 0.35 * this.env);
      g.strokeStyle = rgba(tete, 0.8 * this.env);
      g.lineWidth = 1.5 * S;
      g.beginPath();
      g.arc(cx, cy, eclat * (2.5 + 5 * (1 - this.env)), 0, TAU);
      g.stroke();
    }
    tetes.filter((h) => h.devant).forEach(dessinerTete);
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
window.imageA = imageA;

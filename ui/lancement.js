// Essaim — le logo qui s'assemble : ouverture de la fenêtre, et GIF du README
// (`scripts/images-readme/logo.html`).
//
// Un germe éclate en orbes ; ils se rangent en anneau ; l'anneau se dévide dans le logo et chaque
// orbe se soude à ses voisins sitôt posé. Avec le nom, un second anneau de petits orbes écrit
// ESSAIM en même temps. Deux fins : tout se défait en orbes qui filent (`sortie`, le GIF), ou le
// logo seul se défait pendant que le nom et son filet vont se poser ailleurs (`passage` : dans
// l'en-tête de la fenêtre, qui se dessine au même moment).
//
// Tout se dessine dans la grille du logo (48 unités). Le temps est mené par des segments : chacun
// conduit quelques valeurs d'un état à un autre selon une courbe, jamais linéaire. Rien n'est
// chargé d'ailleurs, ni bibliothèque ni police.
"use strict";

const Lancement = (() => {
  const NS = "http://www.w3.org/2000/svg";
  const TOUR = 2 * Math.PI;
  const TRACE = "M21 6A18 18 0 0 1 21 42A9 9 0 0 0 21 24A9 9 0 0 1 21 6Z";
  const ORBE = { cx: 21, cy: 33, r: 6 };

  // ---- Outils -----------------------------------------------------------------------------
  function N(balise, attrs, parent) {
    const e = document.createElementNS(NS, balise);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(e);
    return e;
  }
  const graine = (g) => { let x = g; return () => { x = (x * 1664525 + 1013904223) % 4294967296; return x / 4294967296; }; };
  const hasard = (a, b) => { const x = Math.sin(a * 127.1 + b * 311.7 + 1.3) * 43758.5453; return x - Math.floor(x); };
  const echelle = (cx, cy, e) => `translate(${cx} ${cy}) scale(${e}) translate(${-cx} ${-cy})`;
  const borne = (x, a, b) => Math.max(a, Math.min(b, x));

  // ---- Le temps ---------------------------------------------------------------------------
  const puissance = (n) => ({
    in: (k) => k ** n,
    out: (k) => 1 - (1 - k) ** n,
    inOut: (k) => (k < 0.5 ? (2 * k) ** n / 2 : 1 - (2 * (1 - k)) ** n / 2),
  });
  const FAMILLES = {
    power2: puissance(3),
    power3: puissance(4),
    sine: { out: (k) => Math.sin((k * Math.PI) / 2), inOut: (k) => (1 - Math.cos(Math.PI * k)) / 2 },
    expo: {
      out: (k) => (k >= 1 ? 1 : 1 - 2 ** (-10 * k)),
      inOut: (k) => (k <= 0 ? 0 : k >= 1 ? 1 : k < 0.5 ? 2 ** (20 * k - 10) / 2 : 1 - 2 ** (10 - 20 * k) / 2),
    },
  };
  // « back.out(1.4) » : dépasse sa cible puis y revient ; le nombre règle le dépassement
  function courbe(nom) {
    const m = /^back\.out\(([\d.]+)\)$/.exec(nom);
    if (m) { const s = +m[1]; return (k) => { const p = k - 1; return p * p * ((s + 1) * p + s) + 1; }; }
    const [famille, sens] = nom.split(".");
    return FAMILLES[famille][sens];
  }
  function horloge() {
    const segments = [], departs = [];
    return {
      // de l'instant t, pendant d secondes, les valeurs de `cible` vont de `de` à `vers`
      seg(cible, t, d, de, vers, nomCourbe) {
        for (const p in vers) departs.push({ cible, p, v: cible[p] });
        segments.push({ cible, t, d, de, vers, courbe: courbe(nomCourbe) });
      },
      // les segments se jouent dans l'ordre de leur début
      trier() { segments.sort((a, b) => a.t - b.t); },
      // remet chaque valeur à son départ, puis joue les segments déjà commencés
      avancer(t) {
        for (let i = departs.length - 1; i >= 0; i--) departs[i].cible[departs[i].p] = departs[i].v;
        for (const s of segments) {
          if (t < s.t) continue;
          const k = s.courbe(s.d > 0 ? Math.min(1, (t - s.t) / s.d) : 1);
          for (const p in s.vers) s.cible[p] = s.de[p] + (s.vers[p] - s.de[p]) * k;
        }
      },
    };
  }

  // ---- Le logo ----------------------------------------------------------------------------
  // un point est-il dans le logo, à au moins `m` unités du bord ?
  function dansLogo(x, y, m) {
    const dx = x - 21, a = dx * dx + (y - 24) * (y - 24), b = dx * dx + (y - 15) * (y - 15), c = dx * dx + (y - 33) * (y - 33);
    if (m < 6 && c <= (6 - m) * (6 - m)) return true;
    if (m < 9 && b <= (9 - m) * (9 - m)) return true;
    return m < 18 && a <= (18 - m) * (18 - m) && dx >= m && c >= (9 + m) * (9 + m);
  }
  // distance d'un point du logo à son bord (0 s'il est dehors)
  function bord(x, y) {
    if (!dansLogo(x, y, 0)) return 0;
    let a = 0, b = 9;
    for (let i = 0; i < 12; i++) { const m = (a + b) / 2; if (dansLogo(x, y, m)) a = m; else b = m; }
    return a;
  }
  // Sème les orbes du logo, puis les range le long du logo : de la pointe de la queue vers la
  // tête, l'orbe en dernier. Chaque orbe a son rayon au repos (r) et ce dont il gonfle en se
  // soudant (gonfle).
  function semer() {
    const r = graine(11), P = [];
    const orbe = (x, y, rayon) => ({ x, y, r: rayon, gonfle: rayon * 1.3 + 1, orbe: (x - 21) * (x - 21) + (y - 33) * (y - 33) <= 36.5 });
    // 74 orbes de taille pleine, jetés au hasard sans se toucher
    for (let essais = 0; P.length < 74 && essais < 60000; essais++) {
      const x = 11 + r() * 29, y = 5 + r() * 38;
      if (dansLogo(x, y, 0.945) && P.every((p) => (p.x - x) ** 2 + (p.y - y) ** 2 >= 4.41)) P.push(orbe(x, y, 1.2));
    }
    // La pointe de la queue est trop fine pour un orbe ordinaire : sur ses cinq dernières unités
    // elle fait moins d'une demi-unité d'épaisseur. On y pose une file de grains, de plus en
    // plus petits, sur sa ligne médiane (à l'angle `a` depuis la pointe, la queue va du rayon
    // 18·cos a au rayon 18 autour du centre du cercle). En gonflant, ils la remplissent jusqu'au bout.
    for (let a = 0.06; a < 0.4; a += 0.06) {
      const milieu = 9 * (1 + Math.cos(a));
      P.push(orbe(21 + milieu * Math.sin(a), 24 + milieu * Math.cos(a), Math.max(0.1, 8.1 * (1 - Math.cos(a)))));
    }
    // Puis on comble : tant qu'un point du logo resterait hors d'atteinte de la matière une fois
    // les orbes gonflés, on y pose un orbe de plus, à la taille que le bord permet. La soudure
    // remplit alors toute la forme, au lieu de laisser le tracé net finir le travail.
    const grille = [];
    for (let gx = 11.25; gx <= 40; gx += 0.5) {
      for (let gy = 5.25; gy <= 43; gy += 0.5) { const d = bord(gx, gy); if (d > 0) grille.push({ x: gx, y: gy, d, fait: false }); }
    }
    const atteint = (g) => P.some((p) => (p.x - g.x) ** 2 + (p.y - g.y) ** 2 <= ((p.r + p.gonfle) * 0.88) ** 2);
    for (let tour = 0; tour < 160; tour++) {
      let pire = null;
      for (const g of grille) {
        if (g.fait) continue;
        if (g.d < 0.25 || atteint(g)) { g.fait = true; continue; }
        if (!pire || g.d > pire.d) pire = g;
      }
      if (!pire) break;
      P.push(orbe(pire.x, pire.y, Math.min(1.2, Math.max(0.3, pire.d * 0.9))));
    }
    for (const p of P) {
      const ox = p.x - 24, oy = p.y - 24, d = Math.sqrt(ox * ox + oy * oy) || 1;
      p.dx = ox / d; p.dy = oy / d;
      let a = Math.atan2(p.x - 21, p.y - 24);
      if (a < 0) a += TOUR;
      p.cle = p.orbe ? 10 + Math.atan2(p.y - 33, p.x - 21) : a;
    }
    P.sort((a, b) => a.cle - b.cle);
    // dans l'anneau, chaque orbe prend la place de sa taille
    const total = P.reduce((s, p) => s + 2 * p.r + 0.5, 0);
    let cumul = 0;
    P.forEach((p, i) => {
      p.rang = i / (P.length - 1);
      p.t0 = (-TOUR * (cumul + p.r + 0.25)) / total;
      cumul += 2 * p.r + 0.5;
    });
    return { P, r };
  }

  // ---- Le nom, sous le logo. Avec lui, le logo est posé 7 unités plus haut dès le départ. --
  // Le nom est composé comme le titre de la fenêtre (`.t-affiche` : Archivo 900, étirée à 125 %,
  // interlettrage de -0,035 em) : agrandi, il s'y superpose lettre pour lettre.
  const MOT = "ESSAIM", BASE = 51, CORPS = 12.5, MONTE = -7, MILIEU_X = 25.5;
  const FILET = { y: BASE + 2.1, height: 1.4 };
  const POLICE = "Archivo, 'Arial Narrow', Arial, sans-serif";
  function ecrire(parent, x) {
    const e = N("text", { x, y: BASE, "font-size": CORPS, "font-weight": 900, "letter-spacing": -0.035 * CORPS, "text-rendering": "geometricPrecision" }, parent);
    e.style.fontFamily = POLICE;
    e.style.fontStretch = "125%";
    e.textContent = MOT;
    return e;
  }
  function poserNom(groupe, defs) {
    const net = ecrire(groupe, 0);
    // chaque lettre est mesurée dans la police réellement affichée
    let places = [], large = 0;
    try {
      for (let i = 0; i < MOT.length; i++) { const b = net.getExtentOfChar(i); places.push({ x: b.x, w: b.width }); large = b.x + b.width; }
    } catch { places = []; }
    if (!places.length || !(large > 0)) { large = 57; places = [...MOT].map((_, i) => ({ x: i * 9.5, w: 9.5 })); }
    const x0 = MILIEU_X - large / 2;
    net.setAttribute("x", x0);
    // le dessin des lettres sert aussi de contour à la soudure liquide
    ecrire(N("clipPath", { id: "lancement-clip-nom" }, defs), x0);
    return { net, large, x0, places };
  }
  // Sème des places dans le dessin des lettres : chaque lettre est peinte hors écran, puis on
  // garde les points tombés dans l'encre. Rangées de gauche à droite.
  function semerNom(nom) {
    const Q = [], r = graine(23), T = 96;
    try {
      const toile = document.createElement("canvas"), ctx = toile.getContext("2d", { willReadFrequently: true });
      for (let i = 0; i < MOT.length; i++) {
        toile.width = 2 * T; toile.height = Math.ceil(1.4 * T);
        ctx.font = `900 ${T}px ${POLICE}`;
        if ("fontStretch" in ctx) ctx.fontStretch = "extra-expanded";
        ctx.textBaseline = "alphabetic";
        ctx.fillStyle = "#fff";
        const c = MOT[i], avance = ctx.measureText(c).width, place = nom.places[i];
        if (!(avance > 0)) continue;
        ctx.fillText(c, 0.3 * T, T);
        const L = toile.width, H = toile.height, pixels = ctx.getImageData(0, 0, L, H).data;
        // la lettre peinte est ramenée à la case mesurée dans la page
        const kx = avance / place.w, ky = T / CORPS;
        const encre = (ux, uy) => {
          const px = Math.round(0.3 * T + ux * kx), py = Math.round(T + uy * ky);
          return px >= 0 && py >= 0 && px < L && py < H && pixels[(py * L + px) * 4 + 3] > 127;
        };
        const debut = Q.length;
        for (let essais = 0; Q.length - debut < 34 && essais < 5000; essais++) {
          const ux = r() * place.w, uy = -r() * CORPS * 0.8;
          // le point et ses quatre voisins proches doivent être dans l'encre : pas d'orbe à cheval sur un bord
          if (!encre(ux, uy) || !encre(ux - 0.35, uy) || !encre(ux + 0.35, uy) || !encre(ux, uy - 0.35) || !encre(ux, uy + 0.35)) continue;
          const x = nom.x0 + place.x + ux, y = BASE + uy;
          if (Q.slice(debut).every((q) => (q.x - x) ** 2 + (q.y - y) ** 2 >= 1.5)) Q.push({ x, y, lettre: i });
        }
      }
    } catch { Q.length = 0; }
    Q.sort((a, b) => a.lettre - b.lettre || a.x - b.x);
    return Q;
  }

  // ---- L'animation ------------------------------------------------------------------------
  // Chaque orbe a quatre avancements : e (du germe à sa place dans l'anneau), u (de l'anneau à
  // sa place dans le logo), g (soudure : il gonfle et se fond dans ses voisins), o (sortie).
  const POSE = 0.42, ATTENTE = 0.1, SOUDURE = 0.4;
  /** Instant où le dernier orbe est fondu : le logo et le nom sont entiers (2,67 s). */
  const SOUDE = 1.0 + 0.75 + POSE + ATTENTE + SOUDURE;
  /** Durée du passage du nom vers sa place, et de la sortie. */
  const PASSAGE = 0.9, SORTIE = 0.6;
  function vie(seg, p, r, ordre, sortie) {
    p.e = 0; p.u = 0; p.g = 0; p.o = 0; p.fondu = false;
    const depart = 1.0 + ordre * 0.75;
    seg(p, 0.4 + r() * 0.06, 0.45, { e: 0 }, { e: 1 }, "expo.out");
    seg(p, depart, POSE, { u: 0 }, { u: 1 }, "back.out(1.4)");
    // la soudure commence à peine l'orbe posé : elle suit le déversement au lieu de l'attendre
    seg(p, depart + POSE + ATTENTE, SOUDURE, { g: 0 }, { g: 1 }, "sine.inOut");
    if (sortie === null) return;
    // sortie : la matière se défait en orbes, qui filent
    seg(p, sortie, 0.14, { g: 1 }, { g: 0 }, "power2.in");
    seg(p, sortie + 0.1 + hasard(p.x, p.y) * 0.08, 0.42, { o: 0 }, { o: 1 }, "power3.in");
  }
  // place un orbe : `libres` porte les orbes en vol, `fondus` ceux qui se soudent (filtre
  // liquide, contenus par le contour)
  function poser(p, x, y, rayon, gonfle, libres, fondus) {
    const fondu = p.g > 0.001;
    if (fondu !== p.fondu) { p.fondu = fondu; (fondu ? fondus : libres).appendChild(p.el); }
    p.el.setAttribute("cx", x);
    p.el.setAttribute("cy", y);
    p.el.setAttribute("r", Math.max((rayon * (0.3 + 0.7 * Math.min(p.e, 1)) + p.g * gonfle) * (1 - 0.7 * p.o), 0));
    p.el.style.opacity = p.e > 0.001 && p.o < 1 ? 1 : 0;
  }
  // Soudure liquide : les orbes floutés puis durcis se rejoignent dès qu'ils se touchent ; le
  // contour du logo (ou des lettres) les contient. `zone` borne le calcul du filtre.
  function liquide(defs, id, flou, zone) {
    const f = N("filter", { id, filterUnits: "userSpaceOnUse", ...zone }, defs);
    N("feGaussianBlur", { in: "SourceGraphic", stdDeviation: flou, result: "flou" }, f);
    N("feColorMatrix", { in: "flou", mode: "matrix", values: "1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 18 -7" }, f);
  }
  // un groupe d'orbes soudés n'est calculé que tant qu'il se voit
  function montrer(groupe, opacite) {
    groupe.style.opacity = opacite;
    groupe.style.display = opacite > 0 ? "" : "none";
  }

  /**
   * Monte l'animation dans `racine` (un groupe SVG, dans la page) et rend `{ duree, dessiner(t) }`.
   *
   * `fin` dit comment elle s'achève, à l'instant `fin.t` (la pose dure jusque-là) :
   * - `{ genre: "sortie", t }` : tout se défait en orbes, qui filent ;
   * - `{ genre: "passage", t, cible }` (avec le nom) : le logo seul se défait ; le nom et son filet
   *   vont où le dit `cible({ x0, base, corps })`, appelée au départ. Elle rend, dans les unités de
   *   `racine`, la transformation du nom (`tx`, `ty`, `s` : translation puis échelle) et le
   *   rectangle du filet (`filet` : x, y, width, height).
   */
  function monter(racine, { nom: avecNom = false, rouge = "#ff3b2f", encre = "#ecebe4", fin }) {
    racine.textContent = "";
    const { seg, trier, avancer } = horloge();
    const passage = fin.genre === "passage", sortie = fin.t, monte = avecNom ? MONTE : 0;
    const { P, r } = semer();
    const st = { pouls: 1, poulsNom: 1, net: 0, liq: 1, netNom: 0, liqNom: 1, w: 0, s: 0, rot: 0, filet: 0, va: 0 };

    const defs = N("defs", {}, racine);
    liquide(defs, "lancement-liquide", 1, { x: 6, y: 0, width: 40, height: 48 });
    const contour = N("clipPath", { id: "lancement-clip-logo" }, defs);
    N("path", { d: TRACE }, contour);
    N("circle", ORBE, contour);

    // le logo (germe, onde, orbes, tracé net) bat d'un bloc, autour du centre de son cercle
    const logo = N("g", {}, racine);
    logo.style.fill = rouge;
    const germe = N("circle", { cx: 21, cy: 33, r: 0 }, logo);
    // l'onde part du cercle dans lequel le logo s'inscrit (centre 21, 24 ; rayon 18) ; avec le
    // nom elle reste courte, pour s'éteindre avant de passer sur les lettres
    const onde = N("circle", { cx: 21, cy: 24, fill: "none", opacity: 0 }, logo), portee = avecNom ? 8 : 26;
    onde.style.stroke = rouge;
    const fondus = N("g", { filter: "url(#lancement-liquide)", "clip-path": "url(#lancement-clip-logo)" }, logo);
    const libres = N("g", {}, logo);
    const net = N("g", { opacity: 0 }, logo);
    N("path", { d: TRACE }, net);
    N("circle", ORBE, net);

    // 1. Le germe gonfle, se contracte (anticipation), puis éclate
    seg(st, 0, 0.3, { s: 0 }, { s: 1 }, "back.out(2.5)");
    seg(st, 0.3, 0.1, { s: 1 }, { s: 0.7 }, "power2.in");
    seg(st, 0.4, 0.08, { s: 0.7 }, { s: 0 }, "power3.out");
    seg(st, 0.4, 0.06, { pouls: 1, poulsNom: 1 }, { pouls: 1.07, poulsNom: 1.07 }, "power2.out");
    seg(st, 0.46, 0.26, { pouls: 1.07, poulsNom: 1.07 }, { pouls: 1, poulsNom: 1 }, "power2.out");
    // 2. Les orbes se rangent en anneau, qui prend de la vitesse puis ralentit en se vidant
    seg(st, 0.4, 0.6, { rot: 0 }, { rot: 2.2 }, "power2.in");
    seg(st, 1.0, 1.25, { rot: 2.2 }, { rot: 9.5 }, "sine.out");
    // 3. L'anneau se dévide dans le logo (la queue d'abord, la tête, l'orbe en dernier) et
    //    chaque orbe se soude à ses voisins sitôt posé
    for (const p of P) {
      p.el = N("circle", { r: 0, opacity: 0 }, libres);
      p.v = 0.6 + r() * 0.6;
      vie(seg, p, r, p.rang, sortie);
    }

    // Avec le nom : le germe lâche aussi de petits orbes clairs. Ils forment un second anneau,
    // plus serré et à contresens, qui se dévide dans les lettres, de gauche à droite.
    let nom = null, Q = [], titre, groupeNom, fondusNom, libresNom, filet, cible = null;
    if (avecNom) {
      titre = N("g", {}, racine);      // le nom et son filet : battent avec le logo
      titre.style.fill = encre;
      groupeNom = N("g", {}, titre);   // le nom : c'est lui qui se déplace au passage
      nom = poserNom(groupeNom, defs);
      Q = semerNom(nom);
      liquide(defs, "lancement-liquide-nom", 0.45, { x: nom.x0 - 3, y: BASE - CORPS - 1, width: nom.large + 6, height: CORPS + 6 });
      fondusNom = N("g", { filter: "url(#lancement-liquide-nom)", "clip-path": "url(#lancement-clip-nom)" }, groupeNom);
      libresNom = N("g", {}, groupeNom);
      filet = N("rect", { x: nom.x0, width: 0, ...FILET }, titre);
      Q.forEach((q, j) => {
        q.el = N("circle", { r: 0, opacity: 0 }, libresNom);
        q.t0 = (j * TOUR) / Q.length; q.v = 0.6 + r() * 0.6;
        const ox = q.x - MILIEU_X, oy = q.y - (BASE - 4.3), d = Math.sqrt(ox * ox + oy * oy) || 1;
        q.dx = ox / d; q.dy = oy / d;
        vie(seg, q, r, j / Math.max(1, Q.length - 1), passage ? null : sortie);
      });
      // le filet se trace pendant que les dernières lettres se soudent
      seg(st, 2.2, 0.5, { filet: 0 }, { filet: 1 }, "expo.out");
    }

    // 4. Fin de la soudure : la matière remplit exactement le contour, le tracé net la relaie
    //    sans que rien ne change à l'œil. Le logo gonfle un instant puis retombe : c'est en
    //    retombant qu'il lâche l'onde.
    seg(st, SOUDE, 0.1, { net: 0, netNom: 0 }, { net: 1, netNom: 1 }, "power2.out");
    seg(st, SOUDE + 0.06, 0.08, { liq: 1, liqNom: 1 }, { liq: 0, liqNom: 0 }, "power2.out");
    seg(st, SOUDE - 0.1, 0.16, { pouls: 1, poulsNom: 1 }, { pouls: 1.045, poulsNom: 1.045 }, "sine.out");
    seg(st, SOUDE + 0.06, 0.22, { pouls: 1.045, poulsNom: 1.045 }, { pouls: 1, poulsNom: 1 }, "power3.out");
    seg(st, SOUDE + 0.06, 0.6, { w: 0 }, { w: 1 }, "power3.out");
    // 5. La fin : le logo se ramasse, sa matière se défait en orbes, qui filent. Le nom fait de
    //    même (sortie), ou va se poser à sa place avec son filet (passage).
    seg(st, sortie, 0.03, { net: 1, liq: 0 }, { net: 0, liq: 1 }, "power2.out");
    seg(st, sortie, 0.1, { pouls: 1 }, { pouls: 0.95 }, "power2.out");
    seg(st, sortie + 0.1, 0.2, { pouls: 0.95 }, { pouls: 1 }, "power2.out");
    if (passage) {
      seg(st, sortie, PASSAGE, { va: 0 }, { va: 1 }, "power3.inOut");
    } else if (avecNom) {
      seg(st, sortie, 0.03, { netNom: 1, liqNom: 0 }, { netNom: 0, liqNom: 1 }, "power2.out");
      seg(st, sortie, 0.1, { poulsNom: 1 }, { poulsNom: 0.95 }, "power2.out");
      seg(st, sortie + 0.1, 0.2, { poulsNom: 0.95 }, { poulsNom: 1 }, "power2.out");
      seg(st, sortie, 0.25, { filet: 1 }, { filet: 0 }, "expo.inOut");
    }
    trier();

    function dessiner(t) {
      avancer(t);
      logo.setAttribute("transform", `translate(21 ${24 + monte}) scale(${st.pouls}) translate(-21 -24)`);
      net.setAttribute("opacity", st.net);
      montrer(fondus, st.liq);
      germe.setAttribute("r", 5 * st.s);
      const w = st.w, visible = w > 0 && w < 1;
      onde.style.opacity = visible ? 0.6 * (1 - w) ** 0.7 : 0;
      if (visible) { onde.setAttribute("r", 18 + portee * w); onde.setAttribute("stroke-width", 1.8 * (1 - w) + 0.1); }
      for (const p of P) {
        const a = p.t0 + st.rot, rx = 24 + Math.cos(a) * 36, ry = 24 + Math.sin(a) * 36;
        const bx = 21 + (rx - 21) * p.e, by = 33 + (ry - 33) * p.e, fuite = p.o * 70 * p.v;
        poser(p, bx + (p.x - bx) * p.u + p.dx * fuite, by + (p.y - by) * p.u + p.dy * fuite, p.r, p.gonfle, libres, fondus);
      }
      if (!nom) return;
      // le nom vit hors du bloc du logo, mais bat avec lui ; ses orbes partent du même germe
      const cy0 = 24 + monte, gy = 33 + monte;
      titre.setAttribute("transform", echelle(21, cy0, st.poulsNom));
      nom.net.setAttribute("opacity", st.netNom);
      montrer(fondusNom, st.liqNom);
      for (let j = 0; j < Q.length; j++) {
        const q = Q[j], b = q.t0 - st.rot * 0.85, qx = 24 + Math.cos(b) * 27, qy = cy0 + Math.sin(b) * 27;
        const ax = 21 + (qx - 21) * q.e, ay = gy + (qy - gy) * q.e, f = q.o * 60 * q.v;
        poser(q, ax + (q.x - ax) * q.u + q.dx * f, ay + (q.y - ay) * q.u + q.dy * f, 0.62, 1.5, libresNom, fondusNom);
      }
      // au passage, le nom et le filet sur lequel il est posé vont ensemble à leur place
      const va = st.va, repos = { x: nom.x0, width: nom.large * st.filet, ...FILET };
      if (va > 0 && !cible) cible = fin.cible({ x0: nom.x0, base: BASE, corps: CORPS });
      if (va > 0) groupeNom.setAttribute("transform", `translate(${cible.tx * va} ${cible.ty * va}) scale(${1 + (cible.s - 1) * va})`);
      else groupeNom.removeAttribute("transform");
      for (const a in repos) filet.setAttribute(a, va > 0 ? repos[a] + (cible.filet[a] - repos[a]) * va : repos[a]);
    }
    return { duree: sortie + (passage ? PASSAGE : SORTIE), dessiner };
  }

  // ---- L'ouverture de la fenêtre ----------------------------------------------------------
  // Le logo s'assemble au milieu de la fenêtre, sur un fond qui la couvre. Puis le fond s'efface :
  // le logo se défait, le nom va se poser sur le titre de l'en-tête, son filet s'allonge en
  // filet de l'en-tête, et le reste de la fenêtre se dessine par blocs décalés. Un clic ou une
  // touche l'abrège.
  //
  // La page ne la montre qu'à l'adresse `index.html#ouverture` (style.css, `.ouverture:target`),
  // et jamais si Windows demande moins d'animations.

  /** L'ouverture va plus vite que le GIF : secondes de l'animation par seconde réelle. */
  const VITESSE = 1.35;
  /** Unités de la grille du logo au milieu du bloc qu'il forme avec le nom. */
  const MILIEU_Y = 26.75;
  /** Instant du passage : le logo est entier et retombé de son battement. */
  const DEPART = SOUDE + 0.45;

  // Fait entrer les blocs de la fenêtre, les uns après les autres. Rend les animations lancées.
  function dessinerFenetre(fenetre) {
    const anims = [], douce = "cubic-bezier(0.16, 1, 0.3, 1)";
    // un bloc se lève en place ; un filet (titre de la grille, pied) se tire de gauche à droite
    const lever = (el, delai) => anims.push(el.animate({ opacity: [0, 1], transform: ["translateY(10px)", "none"] }, { duration: 450, delay: delai, easing: douce, fill: "backwards" }));
    const tirer = (el, delai) => anims.push(el.animate({ clipPath: ["inset(0 100% 0 0)", "inset(0 0 0 0)"] }, { duration: 600, delay: delai, easing: douce, fill: "backwards" }));
    // l'en-tête : la pause, la sonde, puis chaque onglet
    const entete = [...fenetre.querySelectorAll(".entete-droite > :not(.onglets), .entete-droite .onglet")];
    entete.forEach((el, i) => lever(el, 120 + 45 * i));
    // la vue affichée sort du fond d'un bloc (avec ses barres de défilement) ; par-dessus, ses
    // blocs se lèvent l'un après l'autre, puis les tuiles de la grille visibles
    const vue = fenetre.querySelector(".vue:not([hidden])");
    if (!vue) return anims;
    anims.push(vue.animate({ opacity: [0, 1] }, { duration: 300, delay: 150, easing: "ease-out", fill: "backwards" }));
    // un bloc sans boîte (`display: contents`) ne s'anime pas : ses enfants le font à sa place
    const blocs = [...vue.querySelectorAll(":scope > * > *")].flatMap((b) => (getComputedStyle(b).display === "contents" ? [...b.children] : [b]));
    blocs.filter((b) => !b.hidden).forEach((bloc, rang) => {
      const delai = 200 + 40 * rang;
      if (bloc.matches(".barre-titre, .pied")) tirer(bloc, delai);
      else if (!bloc.matches(".defile")) lever(bloc, delai);
    });
    const tuiles = [...vue.querySelectorAll(".defile > .grille > *")].filter((t) => t.getBoundingClientRect().top < innerHeight);
    tuiles.slice(0, 40).forEach((t, i) => lever(t, 320 + 22 * i));
    return anims;
  }

  function ouvrir() {
    const voile = document.getElementById("ouverture");
    if (!voile) return;
    const fenetre = document.querySelector(".fenetre"), entete = fenetre && fenetre.querySelector(".entete");
    const titre = entete && entete.querySelector(".t-affiche");
    if (location.hash !== "#ouverture" || matchMedia("(prefers-reduced-motion: reduce)").matches || !titre) { voile.remove(); return; }
    const svg = voile.querySelector("svg"), fond = voile.querySelector(".fond");
    let image = 0, anims = [], fini = false;

    function finir(abrege) {
      if (fini) return;
      fini = true;
      cancelAnimationFrame(image);
      removeEventListener("keydown", touche, true);
      removeEventListener("resize", taille);
      if (abrege) for (const a of anims) a.cancel();
      // le voile parti, l'en-tête montre son vrai titre et son vrai filet, à la place exacte de
      // ceux du voile (style.css)
      voile.remove();
      const version = entete.querySelector(".version");
      if (version && !abrege) version.animate({ opacity: [0, 1] }, { duration: 300, easing: "ease-out" });
    }
    const abreger = () => finir(true);
    // une erreur ne doit jamais laisser le voile sur la fenêtre
    const garder = (f) => (...args) => {
      try { f(...args); } catch (e) { console.error("Ouverture :", e); abreger(); }
    };
    // la touche qui abrège ne fait rien d'autre
    function touche(e) {
      if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
      e.stopPropagation();
      e.preventDefault();
      abreger();
    }

    // Le bloc du logo et du nom, au milieu de la fenêtre : k pixels par unité de la grille, origine
    // en (ox, oy). Tant que le nom n'est pas parti, il suit la taille de la fenêtre (WebView2 la
    // règle encore pendant le chargement) ; après, un changement de taille abrège.
    const racine = N("g", {}, svg);
    let k = 1, ox = 0, oy = 0, passe = false;
    function placer() {
      k = borne(Math.min(innerWidth, innerHeight) / 130, 3.5, 7);
      ox = innerWidth / 2 - MILIEU_X * k;
      oy = innerHeight / 2 - MILIEU_Y * k;
      racine.setAttribute("transform", `translate(${ox} ${oy}) scale(${k})`);
    }
    const taille = () => (passe ? abreger() : placer());

    // Monte le logo sur sa première image (le germe pas encore né), puis rend de quoi le lancer.
    function monterScene() {
      placer();
      // Où le nom doit arriver : sur le titre de l'en-tête. Sa ligne de base est le bas d'un bloc
      // sans hauteur posé dans sa ligne.
      const cible = (nom) => {
        const sonde = document.createElement("span");
        sonde.style.cssText = "display:inline-block;width:0;height:0";
        titre.append(sonde);
        const gauche = titre.getBoundingClientRect().left, base = sonde.getBoundingClientRect().bottom;
        sonde.remove();
        const cadre = entete.getBoundingClientRect(), trait = parseFloat(getComputedStyle(entete).borderBottomWidth);
        const s = parseFloat(getComputedStyle(titre).fontSize) / (nom.corps * k);
        return {
          s, tx: (gauche - ox) / k - nom.x0 * s, ty: (base - oy) / k - nom.base * s,
          filet: { x: (cadre.left - ox) / k, y: (cadre.bottom - trait - oy) / k, width: cadre.width / k, height: trait / k },
        };
      };
      const scene = monter(racine, { nom: true, rouge: "var(--signal)", encre: "var(--encre)", fin: { genre: "passage", t: DEPART, cible } });
      scene.dessiner(0);
      let debut = null;
      const pas = garder((maintenant) => {
        if (fini) return;
        if (debut === null) debut = maintenant;
        const t = ((maintenant - debut) / 1000) * VITESSE;
        if (!passe && t >= DEPART) {
          passe = true;
          anims = dessinerFenetre(fenetre);
          anims.push(fond.animate({ opacity: [1, 0] }, { duration: 220, easing: "ease-out", fill: "forwards" }));
        }
        scene.dessiner(Math.min(t, scene.duree));
        if (t >= scene.duree) finir(false);
        else image = requestAnimationFrame(pas);
      });
      return () => { if (!fini) image = requestAnimationFrame(pas); };
    }

    voile.addEventListener("pointerdown", abreger);
    addEventListener("keydown", touche, true);
    addEventListener("resize", taille);
    // Le logo est monté dès que la police est là (les lettres y sont mesurées et semées : on
    // l'attend 1,5 s au plus). L'animation part quand la fenêtre se montre : app.js l'annonce
    // (« essaim-prete ») en demandant à l'appli de la montrer. Sans nouvelles, elle part seule.
    const attendre = (ms) => new Promise((ok) => setTimeout(ok, ms));
    const police = Promise.race([document.fonts.load("900 20px Archivo").catch(() => {}), attendre(1500)]);
    const montree = Promise.race([new Promise((ok) => addEventListener("essaim-prete", ok, { once: true })), attendre(2500)]);
    police.then(garder(() => {
      if (fini) return;
      const lancer = monterScene();
      montree.then(lancer);
    }));
  }

  return { monter, ouvrir };
})();

Lancement.ouvrir();

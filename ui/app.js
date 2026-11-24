// Fenêtre des réglages d'Essaim. La page ne touche à rien elle-même : elle envoie des demandes
// à l'appli (window.ipc.postMessage) et reçoit son état par essaim.recevoir(…).
"use strict";

const $ = (id) => document.getElementById(id);
const el = (tag, cls, texte) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (texte !== undefined) e.textContent = texte;
  return e;
};
// Remplace les enfants d'un conteneur sans perdre le focus clavier : l'élément qui l'avait le
// retrouve par sa clé (data-cle) parmi les nouveaux. Rien n'est touché si les enfants sont déjà
// les mêmes, dans le même ordre (retirer un élément du document lui ôte le focus).
function remplacer(conteneur, enfants) {
  const avant = conteneur.children;
  if (avant.length === enfants.length && enfants.every((e, i) => avant[i] === e)) return;
  const actif = document.activeElement;
  const cle = actif && actif !== conteneur && conteneur.contains(actif) ? actif.dataset.cle : undefined;
  conteneur.replaceChildren(...enfants);
  if (cle === undefined || conteneur.contains(actif)) return;
  const cible = [...conteneur.querySelectorAll("[data-cle]")].find((e) => e.dataset.cle === cle);
  if (cible) cible.focus({ preventScroll: true });
}
const envoyer = (demande) => window.ipc && window.ipc.postMessage(JSON.stringify(demande));
// La fenêtre ne se montre qu'à ce message (reglages.rs) : elle s'ouvre sur la page dessinée, police
// chargée, et non sur le blanc du chargement. L'ouverture animée part au même moment
// (lancement.js). Si les images ne viennent pas (fenêtre encore cachée), on n'attend pas plus de 0,4 s.
function annoncerPrete() {
  const image = () => new Promise((ok) => requestAnimationFrame(ok));
  const dessinee = document.fonts.ready.then(image).then(image);
  Promise.race([dessinee, new Promise((ok) => setTimeout(ok, 400))]).then(() => {
    envoyer({ cmd: "prete" });
    dispatchEvent(new Event("essaim-prete"));
  });
}
const memoire = {
  lire(cle, defaut) {
    try { const v = localStorage.getItem("essaim." + cle); return v === null ? defaut : JSON.parse(v); } catch { return defaut; }
  },
  ecrire(cle, v) {
    try { localStorage.setItem("essaim." + cle, JSON.stringify(v)); } catch { /* préférence perdue, sans gravité */ }
  },
};
const virgule = (x, d) => x.toFixed(d).replace(".", ",");
const deux = (n) => String(n).padStart(2, "0");
const borne = (x, a, b) => Math.max(a, Math.min(b, x));
const estObjet = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
// Égalité de deux valeurs de réglage à la tolérance de profils::same (nombres : 1e-3 relatif,
// au moins 1e-3), en descendant dans les listes et les objets : les f32 de l'appli, élargis en
// f64 dans le JSON (0.30000001…), ne comptent pas comme des différences.
function memeValeur(a, b) {
  if (typeof a === "number" && typeof b === "number") return Math.abs(a - b) <= 1e-3 * Math.max(Math.abs(a), Math.abs(b), 1);
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => memeValeur(x, b[i]));
  if (estObjet(a) && estObjet(b)) return [...new Set([...Object.keys(a), ...Object.keys(b)])].every((k) => memeValeur(a[k], b[k]));
  return a === b;
}
const animationsReduites = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
const enSaisie = () => ["INPUT", "TEXTAREA"].includes(document.activeElement && document.activeElement.tagName) && document.activeElement.type !== "range";

const etat = {
  reglages: null,
  defauts: null,
  presets: [],
  actif: null,
  proche: null,
  depot: "",
  curseurs: "",
  demarrage: false,
  pause: false,
  jeux: null,
  detailDemande: null,
  vue: "presets",
  source: "tout",
  ordre: memoire.lire("ordre", "defaut"),
  favoris: new Set(memoire.lire("favoris", [])),
  recherche: "",
  mode: memoire.lire("mode", animationsReduites() ? "survol" : "direct"),
  avance: memoire.lire("avance", false),
  commu: { etat: "rien", liste: [], message: "" },
  son: { kicks: 0, basse: 0, hauteur: 0, aigus: 0, brillance: 0, bpm: null, phase: 0, relais: false, caisses: 0, drops: 0, tension: 0, balance: 0, recu: 0 },
  teinte: null,
  dernierSon: -1e9,
  demo: { kicks: 0, prochain: 0 },
  sourceSon: null, // son donné aux aperçus : "musique", "demo" ou "muet" (écoute coupée)
  prisme: null, // Prisme (éclairage RGB) est-il là ? null : l'appli ne le dit pas
  survol: null,
  menu: null, // tuile dont le menu « Gérer » est ouvert
  renomme: null, // tuile en cours de renommage
  filtreForme: null,
  comparer: null, // premier preset choisi pour une comparaison
  applis: [],
  gif: null, // enregistrement d'un GIF en cours
  renommages: [], // { ancien, nouveau } demandés, suivis à l'état suivant (favoris)
  capture: null, // écoute du son : "active", "coupee", "erreur" (absent : inconnu)
  sorties: null, // sorties son de Windows [{ id, nom }] (null : pas encore reçues)
  veille: false,
  morceau: null, // morceau en cours { titre, artiste } (API média de Windows), ou null
  lecture: null, // le morceau joue (vrai), est en pause (faux) ; null : aucune session
  pistes: null, // compteur des changements de piste de l'appli (rafale dans le grand aperçu)
  listePresets: "", // noms et dates des presets au dernier état (la grille n'est refaite que s'ils changent)
  raison: "", // ce qui retient l'essaim (« Plein écran », « Ne pas déranger », « Batterie : 30 i/s »…)
  recente: "", // dernière application vue au premier plan, proposée pour la liste des pauses
};

// ---------------------------------------------------------------------------------------------
// Son : le vrai son de Windows, ou une démonstration (un kick par demi-seconde) en silence.

// Le vrai son, tant qu'un morceau joue (panneau multimédia de Windows) ou que du son arrive
// depuis moins de 2,5 s : un passage calme ne fait pas repartir la démo, on veut voir comment
// l'essaim réagit à la vraie musique.
function musique() {
  return etat.lecture === true || performance.now() - etat.dernierSon < 2500;
}

// Écoute coupée (« Écoute » inactive) : un son muet, compteurs figés, sans tempo ni basse.
const SON_MUET = Object.freeze({ kicks: 0, basse: 0, hauteur: 50, aigus: 0, brillance: 0, caisses: 0, drops: 0, tension: 0, balance: 0, bpm: null, phase: 0, relais: false });
const sourceSon = () => (etat.reglages && !etat.reglages.audio.actif ? "muet" : musique() ? "musique" : "demo");

// Au passage d'une source à l'autre, les compteurs (kicks, caisses, drops, aigus) sautent : chaque
// aperçu repart d'une écoute neuve, qui prend les nouveaux compteurs pour point de départ au lieu
// d'y voir jusqu'à quatre coups d'un coup.
function nouvelleEcoute() {
  const tous = [grand, loupe.apercu, image.apercu, ...comparaison.apercus, ...[...tuiles.values()].map((t) => t.apercu)];
  for (const ap of tous) if (ap && ap.ecoute) ap.ecoute = new ap.ecoute.constructor();
}

function sonPourApercus(maintenant) {
  const source = sourceSon();
  if (source !== etat.sourceSon) {
    if (etat.sourceSon !== null) nouvelleEcoute();
    etat.sourceSon = source;
  }
  if (source === "muet") return SON_MUET;
  if (source === "musique") {
    const s = etat.son;
    // relais du tempo (break sans kick) : la phase avance d'elle-même entre deux messages
    if (s.relais && s.bpm) return { ...s, phase: (s.phase + ((maintenant - s.recu) / 1000) * (s.bpm / 60)) % 1 };
    return s;
  }
  if (maintenant >= etat.demo.prochain) {
    etat.demo.kicks++;
    etat.demo.prochain = maintenant + 500;
  }
  // démo : un charleston entre deux kicks, une caisse claire un kick sur deux ; son tempo est
  // connu (120 BPM, la phase repart de 0 à chaque kick), comme l'analyse le donnerait sur ce son :
  // ce qui suit les temps (déclencheurs « temps », quatre temps, point de l'anneau) vit aussi
  const phase = Math.min(1, Math.max(0, 1 - (etat.demo.prochain - maintenant) / 500));
  return { kicks: etat.demo.kicks, basse: 0.35, hauteur: 50, aigus: Math.floor((maintenant + 250) / 500), brillance: 0.3, caisses: Math.floor(etat.demo.kicks / 2), bpm: 120, phase };
}

// Simulation dans le grand aperçu (boutons sous l'aperçu) : une montée de 3 s (roulement de
// caisse claire qui s'accélère, tension qui monte, kicks suspendus), puis le drop ; ou un kick,
// une caisse claire. Ajoutée au son des aperçus : les compteurs ne reculent jamais.
const simulation = { debut: null, coups: 0, kicks: null, ajout: { kicks: 0, caisses: 0, drops: 0 } };
function sonSimule(son, maintenant) {
  const a = simulation.ajout;
  const r = { ...son, kicks: (son.kicks || 0) + a.kicks, caisses: (son.caisses || 0) + a.caisses, drops: (son.drops || 0) + a.drops };
  if (simulation.debut === null) return r;
  const x = (maintenant - simulation.debut) / 1000;
  if (x >= 3) {
    // le drop : le roulement reste compté, le kick repart
    a.caisses += simulation.coups;
    a.drops += 1;
    simulation.debut = null;
    return { ...r, caisses: r.caisses + simulation.coups, drops: r.drops + 1 };
  }
  if (simulation.kicks === null) simulation.kicks = r.kicks;
  simulation.coups = Math.floor(4 * x + 3 * x * x);
  return { ...r, kicks: simulation.kicks, caisses: r.caisses + simulation.coups, tension: Math.min(1, (x / 3) * 1.2), basse: 0.2, brillance: Math.min(1, (son.brillance || 0.3) + 0.3 * (x / 3)) };
}

// La sonde de l'en-tête : kick, niveau de la basse (faible → forte), note de la basse
// (grave → aiguë, 25 à 85 Hz sur une échelle logarithmique).
function recevoirSon(m) {
  const kick = m.kicks !== etat.son.kicks;
  const aigus = (m.aigus || 0) !== etat.son.aigus;
  // du son : un kick, de la basse, une attaque d'aigus, une caisse claire, ou un son brillant
  if (kick || aigus || m.basse > 0.03 || (m.caisses || 0) !== etat.son.caisses || (m.brillance || 0) > 0.05) etat.dernierSon = performance.now();
  // tempo : champs absents d'une appli plus ancienne
  const bpm = typeof m.bpm === "number" && m.bpm > 0 ? m.bpm : null;
  const nombre = (v) => (typeof v === "number" && Number.isFinite(v) ? v : 0);
  etat.son = {
    kicks: m.kicks, basse: m.basse, hauteur: m.hauteur, aigus: m.aigus || 0, brillance: m.brillance || 0,
    bpm, phase: nombre(m.phase), relais: m.relais === true, recu: performance.now(),
    // caisses claires, drops, montée et côté qui joue le plus fort (absents d'une appli plus ancienne)
    caisses: nombre(m.caisses), drops: nombre(m.drops), tension: nombre(m.tension), balance: nombre(m.balance),
  };
  etat.teinte = typeof m.teinte === "number" ? m.teinte : null;
  const actif = musique();
  $("sonde").classList.toggle("silence", !actif);
  // tempo des kicks, seulement quand il est stable
  const tempo = $("bpm");
  tempo.hidden = !(actif && bpm);
  if (actif && bpm) tempo.textContent = Math.round(bpm) + " BPM";
  if (actif) {
    $("jauge-niveau").style.width = Math.round(borne(m.basse, 0, 1) * 100) + "%";
    $("detail-niveau").textContent = m.basse < 0.25 ? "faible" : m.basse < 0.6 ? "moyenne" : "forte";
    if (m.hauteur > 0) {
      const p = borne((Math.log(m.hauteur) - Math.log(25)) / (Math.log(85) - Math.log(25)), 0, 1);
      $("jauge-note").style.left = `calc(${(p * 100).toFixed(1)}% - 2px)`;
      $("detail-note").textContent = `${Math.round(m.hauteur)} Hz · ${m.hauteur < 45 ? "grave" : m.hauteur < 65 ? "médium" : "aiguë"}`;
    }
  } else {
    $("detail-niveau").textContent = "silence";
    $("detail-note").textContent = sourceSon() === "muet" ? "aperçus muets" : "aperçus en démo";
  }
  for (const [coup, id] of [[kick, "temoin"], [aigus, "temoin-aigus"]]) {
    if (!coup || !actif || animationsReduites()) continue;
    const t = $(id);
    t.classList.remove("coup");
    void t.offsetWidth;
    t.classList.add("coup");
  }
}

// ---------------------------------------------------------------------------------------------
// Presets : fournis, à moi, communauté

const FORMES = [
  ["cercle", "Cercle"], ["ellipse", "Ellipse"], ["triangle", "Triangle"], ["carre", "Carré"], ["losange", "Losange"],
  ["pentagone", "Pentagone"], ["hexagone", "Hexagone"], ["etoile", "Étoile"], ["astroide", "Astroïde"], ["fleur", "Fleur"],
  ["trefle", "Trèfle"], ["coeur", "Cœur"], ["huit", "Huit"], ["lissajous", "Lissajous"],
];
// Dans les réglages, en plus : les contours d'une image (ouvre la boîte « Trajet d'une
// image ») et ceux de l'icône de l'application au premier plan.
const FORMES_REGLAGE = [...FORMES, ["trace", "Image…"], ["icone", "Icône de l'appli"]];
const FORMES_SATELLITES = [["cometes", "Comme les comètes"], ...FORMES_REGLAGE];
const NOMS_FORMES = { ...Object.fromEntries(FORMES), trace: "Image", icone: "Icône de l'appli" };
// Sections enregistrées dans un preset (comme profils::SECTIONS). L'habillage de la fenêtre et
// de l'écran (equerres, lisere) n'en fait pas partie : il reste d'un preset à l'autre.
const SECTIONS_PRESET = [
  "orbite", "essaim", "couleur", "eclat", "fissures", "particules", "sillage", "projecteur", "reticule", "spectre", "rubans", "ressorts", "arcs",
  "quatre_temps", "anneau", "orbe", "voix", "trame", "pinceau", "pendule", "gouttes", "audio", "bordure", "souris", "modulations", "declencheurs",
];
// Réglages de ces sections propres à la machine ou à la main de l'utilisateur, hors presets
// (comme profils::in_profile).
const HORS_PRESET = new Set(["souris.avec_preset", "audio.sortie", "audio.applis", "projecteur.secousse", "projecteur.sensibilite"]);

const FORMES_CONNUES = new Set(FORMES_SATELLITES.map(([id]) => id));
// point d'un trajet : fini et pas démesuré (comme Config::clamped)
const pointValide = (q) => Array.isArray(q) && q.length === 2 && q.every((v) => Number.isFinite(v) && Math.abs(v) < 1e6);
const POINTS_MAX = 2000; // par trajet ou par contour (comme Config::clamped)
// comme contours::CONTOURS_MAX (nom distinct du CONTOURS_MAX d'apercu.js, chargé dans la même page)
const CONTOURS_DISTANTS_MAX = 24;
// Réglages entiers du moteur (u32 dans config.rs) : un nombre distant y est arrondi.
const ENTIERS = new Set(["orbite.nombre", "orbite.anneaux", "essaim.maximum", "essaim.fixes", "couleur.palette", "spectre.bandes"]);
// Bornes sûres des réglages sans glissière (comme config::limits) : l'allure du trait et du
// pendule (échelle, rotation, décalage, sans glissière), puis la borne de la section.
const BORNES_ALLURE = {
  echelle: [0.5, 2], opacite: [0.1, 1], teinte: [-180, 180], rotation: [-30, 30], decalage_x: [-80, 80], decalage_y: [-80, 80],
  reaction_kick: [0, 2], reaction_caisse: [0, 2], reaction_charleston: [0, 2], reaction_basse: [0, 2], reaction_montee: [0, 2], reaction_drop: [0, 2],
};
const BORNES_SECTION = { orbite: [0, 100], essaim: [0, 10], couleur: [0, 720], bordure: [0, 1], rendu: [0, 500] };
// Champs numériques d'une modulation ou d'un déclencheur (comme Config::clamped) ; `nombre` et
// `nombre_max` sont entiers.
const BORNES_ELEMENT = { quantite: [-1, 1], seuil: [0, 0.95], lissage: [0, 1], force: [0, 3], nombre: [0, 40], nombre_max: [0, 40], delai: [0, 10], probabilite: [0, 1] };

// Bornes d'un réglage numérique : celles de sa glissière (alignées sur config::limits), sinon
// les bornes sûres ci-dessus.
let bornesGlissieres = null; // « section.cle » → [min, max]
function bornesReglage(section, cle) {
  if (!bornesGlissieres) {
    bornesGlissieres = new Map();
    for (const d of ELEMENTS) if (typeof d.min === "number" && typeof d.max === "number") bornesGlissieres.set(d.section + "." + d.cle, [d.min, d.max]);
  }
  return bornesGlissieres.get(section + "." + cle)
    || (section in NOMS_FIGURES && BORNES_ALLURE[cle])
    || BORNES_SECTION[section] || [0, 1000];
}

// Modulation ou déclencheur d'un preset distant : textes, nombres finis, booléen (une source,
// une cible, un événement ou une action inconnus, d'une version plus récente, sont acceptés :
// Essaim les ignore).
const elementValide = (m) => estObjet(m)
  && ["source", "cible", "sur", "quand", "faire"].every((k) => m[k] === undefined || typeof m[k] === "string")
  && Object.keys(BORNES_ELEMENT).every((k) => m[k] === undefined || Number.isFinite(m[k]))
  && (m.inverser === undefined || typeof m.inverser === "boolean");
// Ses nombres ramenés dans leurs bornes (des étincelles par millions figeraient les aperçus).
function elementBorne(m) {
  const r = { ...m };
  for (const [k, [a, b]] of Object.entries(BORNES_ELEMENT)) {
    if (typeof r[k] === "number") r[k] = k === "nombre" || k === "nombre_max" ? Math.round(borne(r[k], a, b)) : borne(r[k], a, b);
  }
  return r;
}

// Valeur distante prise à la place de `defaut`, ou undefined si elle est refusée : même type
// (nombre fini, booléen, texte, liste) ; un nombre est ramené dans les bornes du réglage (arrondi
// s'il est entier), une liste tronquée à son maximum ; une forme inconnue ou un trajet mal formé
// est refusé.
function valeurDistante(section, cle, v, defaut) {
  if (typeof defaut === "number") {
    if (!Number.isFinite(v)) return undefined;
    const [a, b] = bornesReglage(section, cle);
    const x = borne(v, a, b);
    return ENTIERS.has(section + "." + cle) ? Math.round(x) : x;
  }
  if (typeof defaut === "boolean") return typeof v === "boolean" ? v : undefined;
  if (typeof defaut === "string") return typeof v === "string" && (cle !== "forme" || FORMES_CONNUES.has(v)) ? v : undefined;
  if (!Array.isArray(defaut) || !Array.isArray(v)) return undefined;
  if (cle === "trace") return v.every(pointValide) ? v.slice(0, POINTS_MAX) : undefined;
  if (cle === "contours") {
    return v.every((c) => Array.isArray(c) && c.every(pointValide)) ? v.slice(0, CONTOURS_DISTANTS_MAX).map((c) => c.slice(0, POINTS_MAX)) : undefined;
  }
  if (cle === "liste") {
    const max = section === "modulations" ? MODULATIONS_MAX : DECLENCHEURS_MAX;
    return v.every(elementValide) ? v.slice(0, max).map(elementBorne) : undefined;
  }
  return v;
}

// Réglages d'un preset distant posés sur `base` : seules les clés connues de `base`, de même
// type, dans les sections d'un preset (pas `[rendu]`, propre à la machine), sont reprises ; le
// reste garde la valeur de base. Renvoie null si une section est là sans être un objet (preset
// mal formé, ignoré).
function fusion(base, partiel) {
  if (!estObjet(partiel)) return null;
  // preset d'avant les déclencheurs : ses réactions d'alors en deviennent (comme migration::migrer)
  if (partiel.declencheurs === undefined) {
    const a = reactionsAvant(partiel);
    const deja = estObjet(partiel.modulations) && Array.isArray(partiel.modulations.liste) ? partiel.modulations.liste : null;
    const mods = deja || a.basse;
    const plus = a.mods.filter((x) => !mods.some((y) => y.source === x.source && y.cible === x.cible));
    partiel = { ...partiel, modulations: { liste: mods.concat(plus) }, declencheurs: { liste: a.decl }, particules: { ...a.particules, ...(estObjet(partiel.particules) ? partiel.particules : {}) } };
  }
  const r = {};
  for (const [s, sec] of Object.entries(base)) {
    const distant = partiel[s];
    if (distant === undefined || !estObjet(sec) || !SECTIONS_PRESET.includes(s)) { r[s] = sec; continue; }
    if (!estObjet(distant)) return null;
    r[s] = { ...sec };
    for (const [k, defaut] of Object.entries(sec)) {
      if (HORS_PRESET.has(s + "." + k) || !(k in distant)) continue;
      const v = valeurDistante(s, k, distant[k], defaut);
      if (v !== undefined) r[s][k] = v;
    }
  }
  return r;
}

function reglagesDeBase() {
  const d = etat.presets.find((p) => p.fourni && p.nom === "Défaut");
  return d ? d.reglages : etat.reglages;
}

function tousLesPresets() {
  const locaux = etat.presets.map((p) => ({
    id: "p:" + p.nom, nom: p.nom, origine: p.fourni ? "fourni" : "mien", reglages: p.reglages, etiquettes: [], auteur: "", date: p.date || 0,
  }));
  const commu = etat.commu.liste.map((p) => ({ ...p, id: "c:" + p.nom, origine: "commu", date: 0 }));
  return locaux.concat(commu);
}

// Favoris : gardés par identifiant (« p:nom »). Un renommage demandé par la page est suivi quand
// l'état reçu ne contient plus l'ancien nom mais contient le nouveau (à la casse près : l'appli
// peut l'ajuster).
function suivreRenommages() {
  if (!etat.renommages.length) return;
  const noms = new Set(etat.presets.filter((p) => !p.fourni).map((p) => p.nom));
  let change = false;
  etat.renommages = etat.renommages.filter(({ ancien, nouveau, quand }) => {
    // pas encore fait : attendu quelques secondes (un renommage refusé est oublié)
    if (noms.has(ancien)) return performance.now() - quand < 5000;
    const fait = [...noms].find((n) => n.toLowerCase() === nouveau.toLowerCase());
    if (fait && etat.favoris.delete("p:" + ancien)) { etat.favoris.add("p:" + fait); change = true; }
    if (fait && etat.comparer === "p:" + ancien) etat.comparer = "p:" + fait;
    return false;
  });
  if (change) memoire.ecrire("favoris", [...etat.favoris]);
}

function installe(nom) {
  return etat.presets.some((p) => !p.fourni && p.nom === nom);
}

function filtrer(liste) {
  const q = etat.recherche.trim().toLowerCase();
  const libelle = { fourni: "fourni", mien: "mes presets", commu: "communauté" };
  const montres = liste.filter((p) => {
    if (etat.source === "favoris" ? !etat.favoris.has(p.id) : etat.source !== "tout" && p.origine !== etat.source) return false;
    if (etat.filtreForme && p.reglages.orbite.forme !== etat.filtreForme) return false;
    if (!q) return true;
    return [p.nom, p.auteur, libelle[p.origine], NOMS_FORMES[p.reglages.orbite.forme], ...(p.etiquettes || [])].join(" ").toLowerCase().includes(q);
  });
  if (etat.ordre === "nom") montres.sort((a, b) => a.nom.localeCompare(b.nom, "fr"));
  if (etat.ordre === "recents") montres.sort((a, b) => b.date - a.date);
  return montres;
}

// Image d'aperçu d'un preset de la bibliothèque (champ `apercu` de l'index, facultatif) : chemin
// relatif d'un PNG ou d'un GIF du dépôt, sans « .. » ; adresse sur raw.githubusercontent.com.
function imageCommu(chemin) {
  if (typeof chemin !== "string" || chemin.length > 200) return "";
  const c = chemin.trim().replace(/^\.\//, "");
  const parts = c.split("/");
  if (!/^[\w\-. /]+\.(png|gif)$/i.test(c) || parts.some((x) => !x || x === "." || x === "..")) return "";
  return `https://raw.githubusercontent.com/${etat.depot}/main/${parts.map(encodeURIComponent).join("/")}`;
}

const INDEX_MAX = 1 << 20;
const PRESETS_COMMU_MAX = 500;

async function chargerCommu(forcer) {
  if (!etat.depot || (!forcer && etat.commu.etat !== "rien")) return;
  etat.commu = { etat: "charge", liste: [], message: "" };
  dessinerPresets();
  const arret = new AbortController();
  const delai = setTimeout(() => arret.abort(), 8000);
  try {
    const r = await fetch(`https://raw.githubusercontent.com/${etat.depot}/main/index.json`, { signal: arret.signal, cache: "no-store" });
    if (r.status === 404) throw new Error("absent");
    if (!r.ok) throw new Error("http " + r.status);
    // index d'un dépôt tiers : taille et nombre de presets bornés
    const texte = await r.text();
    if (texte.length > INDEX_MAX) throw new Error("trop grand");
    const index = JSON.parse(texte);
    const base = reglagesDeBase();
    const liste = (estObjet(index) && Array.isArray(index.presets) ? index.presets : [])
      .slice(0, PRESETS_COMMU_MAX)
      .filter((p) => estObjet(p) && typeof p.nom === "string" && p.nom.trim() && p.nom.length <= 40)
      .map((p) => ({
        nom: p.nom.trim(),
        auteur: typeof p.auteur === "string" ? "@" + p.auteur.replace(/^@/, "").slice(0, 39) : "",
        etiquettes: Array.isArray(p.etiquettes) ? p.etiquettes.filter((e) => typeof e === "string").slice(0, 8) : [],
        reglages: fusion(base, p.reglages),
        image: imageCommu(p.apercu),
      }))
      .filter((p) => p.reglages);
    etat.commu = { etat: "ok", liste, message: "" };
  } catch (e) {
    etat.commu = {
      etat: "erreur", liste: [],
      message: e.message === "absent"
        ? `La bibliothèque n'est pas encore en ligne : le dépôt ${etat.depot} n'a pas d'index.json.`
        : e.message === "trop grand"
          ? `L'index du dépôt ${etat.depot} est trop grand (1 Mo au plus) : il n'est pas chargé.`
          : "Bibliothèque injoignable. Vérifie ta connexion, puis réessaie.",
    };
  } finally {
    clearTimeout(delai);
  }
  dessinerPresets();
}

// ---------------------------------------------------------------------------------------------
// Grille des presets

const tuiles = new Map(); // id → { racine, apercu, … }
const visibles = new Set();
const observateur = new IntersectionObserver((entrees) => {
  for (const e of entrees) {
    const id = e.target.dataset.id;
    if (e.isIntersecting) visibles.add(id); else visibles.delete(id);
  }
}, { root: $("defile-grille"), rootMargin: "80px" });

function detail(p) {
  const c = p.reglages, o = c.orbite;
  const parts = [];
  // rubans, ressorts et petites figures : ce qui fait l'allure du preset passe en tête
  const figures = [["arcs", "arcs"], ["quatre_temps", "quatre temps"], ["anneau", "anneau lisse"], ["orbe", "anneau spectre"], ["voix", "ligne de voix"], ["trame", "trame"], ["pinceau", "trait"], ["pendule", "pendule"], ["gouttes", "gouttes en contour"]].filter(([s]) => c[s] && c[s].actif).map(([, nom]) => nom);
  if (o.cometes) parts.push(`${NOMS_FORMES[o.forme] || "Cercle"} · ${o.nombre * o.anneaux} ${c.rubans && c.rubans.actif ? "ruban" : "comète"}${o.nombre * o.anneaux > 1 ? "s" : ""}`);
  else if (!figures.length) parts.push("sans comètes");
  if (figures.length) parts.push(figures.join(", ").replace(/^./, (l) => l.toUpperCase()));
  if (c.ressorts && c.ressorts.actif) parts.push("sur ressorts");
  const sats = c.essaim.fixes + c.essaim.maximum;
  if (sats) parts.push(sats + " satellites" + (c.essaim.forme && c.essaim.forme !== "cometes" ? " en " + (NOMS_FORMES[c.essaim.forme] || "").toLowerCase() : ""));
  if (o.rotation) parts.push("trajet qui tourne");
  if (c.souris && c.souris.jeu) parts.push("curseurs " + c.souris.jeu);
  if (p.origine === "commu" && p.etiquettes.length) parts.push(p.etiquettes.slice(0, 2).join(", "));
  return parts.join(" · ");
}

function creerTuile(p) {
  const racine = el("div", "tuile");
  racine.dataset.id = p.id;
  const bouton = el("button");
  bouton.dataset.cle = p.id;
  const tete = el("span", "tete");
  const numero = el("span", "t-numero");
  const origine = el("span", "t-label");
  tete.append(numero, origine);
  const canvas = el("canvas", "scene");
  const nom = el("span", "t-nom", p.nom);
  const ligne = el("span", "t-petit");
  bouton.append(tete, canvas, nom, ligne);
  const actions = el("div", "actions");
  racine.append(bouton, actions);
  // preset de la bibliothèque avec une image : montrée quand l'aperçu n'est pas animé
  const image = el("img", "scene");
  Object.assign(image, { alt: "", loading: "lazy", decoding: "async", hidden: true });
  canvas.after(image);
  const apercu = new window.Apercu(canvas);
  // boutons d'action gardés d'un affichage à l'autre (seuls leurs attributs changent) ; menu « Gérer »
  const menu = el("div", "menu-tuile");
  const t = { racine, bouton, numero, origine, nom, ligne, actions, apercu, canvas, image, p, signature: null, pret: false, erreur: false, confirmer: 0, boutons: new Map(), menu };
  image.addEventListener("error", () => { t.imageKo = true; image.hidden = true; canvas.hidden = false; });
  bouton.addEventListener("click", () => { if (etat.renomme !== t.p.id) choisir(t.p); });
  bouton.addEventListener("mouseenter", () => { etat.survol = p.id; montrerLoupe(t); });
  bouton.addEventListener("mouseleave", () => { if (etat.survol === p.id) etat.survol = null; cacherLoupe(); });
  observateur.observe(racine);
  return t;
}

function choisir(p) {
  if (p.origine === "commu") {
    if (installe(p.nom)) envoyer({ cmd: "appliquer", nom: p.nom });
    else envoyer({ cmd: "installer", nom: p.nom, reglages: p.reglages, appliquer: true });
  } else {
    envoyer({ cmd: "appliquer", nom: p.nom });
  }
}

// Petite puce d'action ; `cle` la retrouve après une reconstruction (focus clavier gardé).
function puce(libelle, action, classe, cle) {
  const b = el("button", "puce petite" + (classe ? " " + classe : ""), libelle);
  if (cle) b.dataset.cle = cle;
  b.addEventListener("click", (e) => { e.stopPropagation(); action(b); });
  return b;
}

function renommer(t) {
  etat.renomme = t.p.id;
  const champ = el("input", "renommer");
  Object.assign(champ, { value: t.p.nom, maxLength: 40, spellcheck: false });
  champ.setAttribute("aria-label", "Nouveau nom du preset");
  const fin = (valider) => {
    if (etat.renomme !== t.p.id) return;
    etat.renomme = null;
    const nouveau = champ.value.trim();
    if (valider && nouveau && nouveau !== t.p.nom) {
      etat.renommages.push({ ancien: t.p.nom, nouveau, quand: performance.now() });
      envoyer({ cmd: "renommer", ancien: t.p.nom, nouveau });
    }
    champ.replaceWith(t.nom);
  };
  champ.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") fin(true);
    if (e.key === "Escape") fin(false);
  });
  champ.addEventListener("click", (e) => e.stopPropagation());
  champ.addEventListener("blur", () => fin(true));
  t.nom.replaceWith(champ);
  champ.focus();
  champ.select();
}

// Signature des réglages d'un preset : une vignette n'est préchauffée de nouveau que si elle change
// (tousLesPresets recrée les objets à chaque message). Gardée par objet de réglages : un même
// message n'est sérialisé qu'une fois, quel que soit le nombre d'affichages.
const signatures = new WeakMap();
function signature(p) {
  let s = signatures.get(p.reglages);
  if (s === undefined) { s = JSON.stringify(p.reglages); signatures.set(p.reglages, s); }
  return s;
}

// Habillage hors presets (fenêtre, écran, étiquette du morceau) : il reste quand on applique un
// preset, donc les vignettes, la loupe et la comparaison le montrent tel qu'il est en place.
const HABILLAGE = ["equerres", "halo", "lisere", "etiquette"];
function avecHabillage(r) {
  if (!etat.reglages || !r) return r;
  const plus = {};
  for (const s of HABILLAGE) if (etat.reglages[s]) plus[s] = etat.reglages[s];
  return { ...r, ...plus };
}
const signatureHabillage = () => JSON.stringify(HABILLAGE.map((s) => (etat.reglages ? etat.reglages[s] : null)));

function majTuile(t, p, rang, habillage) {
  const sig = signature(p) + "|" + habillage;
  if (sig !== t.signature) { t.signature = sig; t.pret = false; t.erreur = false; }
  t.p = p;
  if ((p.image || "") !== t.image.getAttribute("src") && (p.image || t.image.hasAttribute("src"))) {
    t.imageKo = false;
    if (p.image) t.image.src = p.image; else t.image.removeAttribute("src");
  }
  t.apercu.regler(avecHabillage(p.reglages));
  t.numero.textContent = deux(rang);
  t.nom.textContent = p.nom;
  const proche = etat.proche && p.origine !== "commu" && etat.proche.nom === p.nom;
  t.origine.textContent = proche ? "modifié" : p.origine === "commu" ? p.auteur || "communauté" : p.origine === "fourni" ? "fourni" : "à toi";
  t.ligne.textContent = detail(p);
  const actif = p.origine === "commu" ? installe(p.nom) && etat.actif === p.nom : etat.actif === p.nom;
  t.racine.classList.toggle("active", actif);
  t.racine.classList.toggle("proche", !!proche);
  t.racine.classList.toggle("menu-ouvert", etat.menu === p.id);
  t.bouton.setAttribute("aria-pressed", actif);
  t.bouton.setAttribute("aria-label", `${p.nom}, ${t.origine.textContent}${actif ? ", actif" : ""}`);
  if (proche) t.bouton.title = `Les réglages en cours sont ceux de « ${p.nom} » avec ${etat.proche.ecarts} réglage${etat.proche.ecarts > 1 ? "s" : ""} changé${etat.proche.ecarts > 1 ? "s" : ""}.`;
  else t.bouton.removeAttribute("title");

  // actions : favori, et un menu « Gérer » selon l'origine
  remplacer(t.actions, actionsTuile(t));
}

// Attributs d'un élément mis à jour seulement s'ils changent (null : retiré).
function attributs(e, liste) {
  for (const [k, v] of Object.entries(liste)) {
    if (v === null) e.removeAttribute(k);
    else if (e.getAttribute(k) !== String(v)) e.setAttribute(k, v);
  }
  return e;
}

// Actions d'une tuile : favori, comparer, et un menu « Gérer » selon l'origine. Chaque bouton est
// créé une fois (son action lit le preset au moment du clic) ; ensuite seuls son texte et ses
// attributs changent.
const ACTIONS_TUILE = {
  favori: (p) => {
    if (etat.favoris.has(p.id)) etat.favoris.delete(p.id); else etat.favoris.add(p.id);
    memoire.ecrire("favoris", [...etat.favoris]);
    dessinerPresets();
  },
  comparer: (p) => comparer(p),
  installer: (p) => envoyer({ cmd: "installer", nom: p.nom, reglages: p.reglages, appliquer: false }),
  gerer: (p) => { etat.menu = etat.menu === p.id ? null : p.id; dessinerPresets(); },
  dupliquer: (p) => envoyer({ cmd: "dupliquer", nom: p.nom }),
  renommer: (p, t) => renommer(t),
  exporter: (p) => envoyer({ cmd: "exporter", nom: p.nom }),
  supprimer: (p, t) => {
    if (t.confirmer > performance.now()) {
      envoyer({ cmd: "supprimer", nom: p.nom });
      t.confirmer = 0;
    } else {
      t.confirmer = performance.now() + 4000;
      setTimeout(dessinerPresets, 4100);
    }
    dessinerPresets();
  },
};
function actionsTuile(t) {
  const p = t.p;
  const bouton = (nom, texte, attrs = {}) => {
    let b = t.boutons.get(nom);
    if (!b) {
      b = puce("", () => ACTIONS_TUILE[nom](t.p, t), "", p.id + "|" + nom);
      t.boutons.set(nom, b);
    }
    if (b.textContent !== texte) b.textContent = texte;
    return attributs(b, attrs);
  };
  const fav = bouton("favori", "Favori", { "aria-pressed": etat.favoris.has(p.id), "aria-label": `Favori : ${p.nom}` });
  const comp = bouton("comparer", etat.comparer === p.id ? "Comparer…" : "Comparer", {
    "aria-pressed": etat.comparer === p.id, title: "Compare deux presets côte à côte, avec leurs réglages différents",
  });
  if (p.origine === "commu") {
    const fait = installe(p.nom);
    const b = bouton("installer", fait ? "Installé" : "+ Installer", { "aria-pressed": fait ? "true" : null });
    b.disabled = fait;
    return [fav, comp, b];
  }
  const ouvert = etat.menu === p.id;
  const gerer = bouton("gerer", ouvert ? "Gérer ▴" : "Gérer ▾", { "aria-expanded": ouvert });
  if (!ouvert) return [fav, comp, gerer];
  const items = [bouton("dupliquer", "Dupliquer")];
  if (p.origine === "mien") {
    const confirmer = t.confirmer > performance.now();
    const suppr = bouton("supprimer", confirmer ? "Confirmer la suppression" : "Supprimer");
    suppr.classList.toggle("danger", confirmer);
    items.push(bouton("renommer", "Renommer"), bouton("exporter", "Exporter"), suppr);
  }
  remplacer(t.menu, items);
  return [fav, comp, gerer, t.menu];
}

// Boutons des sources et du filtre par forme : créés une fois chacun, gardés ensuite.
const boutonsSources = new Map(); // id → { b, compte }
const boutonsFormes = new Map(); // id → bouton

function dessinerSources(liste) {
  const sources = [["tout", "Tout"], ["fourni", "Fournis"], ["mien", "Mes presets"], ["favoris", "Favoris"], ["commu", "Communauté (GitHub)"]];
  remplacer($("sources"), sources.map(([id, libelle]) => {
    let s = boutonsSources.get(id);
    if (!s) {
      const b = el("button", "ligne");
      b.dataset.cle = id;
      const compte = el("span", "compte");
      b.append(el("span", "", libelle), compte);
      b.addEventListener("click", () => {
        etat.source = id;
        if (id === "commu" || id === "tout") chargerCommu(false);
        dessinerPresets();
      });
      s = { b, compte };
      boutonsSources.set(id, s);
    }
    attributs(s.b, { "aria-pressed": etat.source === id });
    const n = id === "tout" ? liste.length : id === "favoris" ? liste.filter((p) => etat.favoris.has(p.id)).length : liste.filter((p) => p.origine === id).length;
    const compte = id === "commu" && etat.commu.etat !== "ok" ? (etat.commu.etat === "erreur" ? "—" : "··") : deux(n);
    if (s.compte.textContent !== compte) s.compte.textContent = compte;
    return s.b;
  }));
}

function groupeChoix(conteneur, choix, actuel, choisir) {
  remplacer($(conteneur), choix.map(([id, libelle]) => {
    const b = el("button", "puce", libelle);
    b.dataset.cle = id;
    b.setAttribute("aria-pressed", actuel === id);
    b.addEventListener("click", () => choisir(id));
    return b;
  }));
}

function dessinerModes() {
  groupeChoix("modes", [["direct", "Direct"], ["survol", "Survol"], ["fige", "Figé"]], etat.mode, (id) => {
    etat.mode = id;
    memoire.ecrire("mode", id);
    dessinerModes();
  });
  $("mode-aide").textContent = {
    direct: "Toutes les vignettes visibles bougent avec la musique.",
    survol: "Seule la vignette sous la souris bouge.",
    fige: "Aucune vignette ne bouge : le plus léger.",
  }[etat.mode];
  groupeChoix("ordres", [["defaut", "Par défaut"], ["nom", "Nom"], ["recents", "Récents"]], etat.ordre, (id) => {
    etat.ordre = id;
    memoire.ecrire("ordre", id);
    dessinerModes();
    dessinerPresets();
  });
}

function dessinerPresets() {
  if (!etat.reglages) return;
  const liste = tousLesPresets();
  dessinerSources(liste);
  const titres = { tout: "Tous les presets", fourni: "Fournis avec Essaim", mien: "Mes presets", favoris: "Favoris", commu: "Bibliothèque communautaire" };
  $("titre-grille").textContent = titres[etat.source];
  dessinerFiltreFormes(liste);
  const montres = filtrer(liste);
  $("compte").textContent = montres.length + (montres.length > 1 ? " résultats" : " résultat");

  const note = $("note-commu");
  note.hidden = etat.source !== "commu";
  if (!note.hidden) {
    note.replaceChildren(
      el("strong", "", "Hébergé sur GitHub"),
      document.createTextNode(` — dépôt ${etat.depot}. Installer copie le preset dans Mes presets ; publier le tien passe par une relecture avant d'apparaître ici. `),
    );
    const lien = el("button", "puce petite", "Voir le dépôt");
    lien.addEventListener("click", () => envoyer({ cmd: "ouvrir_depot" }));
    note.append(lien);
  }

  const habillage = signatureHabillage();
  const rangs = new Map(liste.map((p, i) => [p, i + 1]));
  const enfants = montres.map((p) => {
    let t = tuiles.get(p.id);
    if (!t) { t = creerTuile(p); tuiles.set(p.id, t); }
    if (etat.renomme !== p.id) majTuile(t, p, rangs.get(p), habillage);
    return t.racine;
  });
  const ids = new Set(liste.map((p) => p.id));
  for (const [id, t] of tuiles) {
    if (!ids.has(id)) { observateur.unobserve(t.racine); tuiles.delete(id); visibles.delete(id); }
  }
  remplacer($("grille"), enfants);

  const vide = $("vide");
  const attente = etat.source === "commu" && etat.commu.etat === "charge";
  const erreurCommu = etat.source === "commu" && etat.commu.etat === "erreur";
  vide.hidden = montres.length > 0;
  if (!vide.hidden) {
    vide.replaceChildren();
    if (attente) {
      vide.append(el("span", "t-titre", "Chargement de la bibliothèque…"));
    } else if (erreurCommu) {
      vide.append(el("span", "zero", "—"), el("span", "t-texte", etat.commu.message));
      const g = el("div", "groupe-puces");
      const re = el("button", "puce", "Réessayer");
      re.addEventListener("click", () => chargerCommu(true));
      const voir = el("button", "puce", "Voir le dépôt");
      voir.addEventListener("click", () => envoyer({ cmd: "ouvrir_depot" }));
      g.append(re, voir);
      vide.append(g);
    } else {
      const texte = etat.recherche ? `Aucun preset pour « ${etat.recherche} » ici.`
        : etat.source === "favoris" ? "Aucun favori : la puce Favori sous un preset l'ajoute ici." : "Aucun preset ici pour l'instant.";
      vide.append(el("span", "zero", "0"), el("span", "t-texte", texte));
      const eff = el("button", "puce", "Tout afficher");
      eff.addEventListener("click", () => { etat.recherche = ""; $("recherche").value = ""; etat.source = "tout"; dessinerPresets(); });
      vide.append(eff);
    }
  }
}

// Filtre par forme : seulement les formes qu'ont les presets de la liste.
function dessinerFiltreFormes(liste) {
  const presentes = new Set(liste.map((p) => p.reglages.orbite.forme));
  if (etat.filtreForme && !presentes.has(etat.filtreForme)) etat.filtreForme = null;
  const choix = [["", "Toutes"], ...FORMES_REGLAGE.filter(([id]) => presentes.has(id))];
  remplacer($("filtre-formes"), choix.map(([id, libelle]) => {
    let b = boutonsFormes.get(id);
    if (!b) {
      b = el("button", "puce petite forme");
      b.dataset.cle = id || "toutes";
      if (id) b.append(picto(id));
      b.append(document.createTextNode(id ? NOMS_FORMES[id] : libelle));
      b.addEventListener("click", () => { etat.filtreForme = id || null; dessinerPresets(); });
      boutonsFormes.set(id, b);
    }
    return attributs(b, { "aria-pressed": (etat.filtreForme || "") === id });
  }));
}

// Flèches du clavier dans la grille : d'une tuile à sa voisine.
function deplacerDansLaGrille(e) {
  const boutons = [...$("grille").querySelectorAll(".tuile > button")];
  const i = boutons.indexOf(document.activeElement);
  if (i < 0) return false;
  const colonnes = Math.max(1, Math.round($("grille").clientWidth / boutons[0].getBoundingClientRect().width));
  const pas = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -colonnes, ArrowDown: colonnes }[e.key];
  if (pas === undefined) return false;
  const cible = boutons[borne(i + pas, 0, boutons.length - 1)];
  cible.focus();
  cible.scrollIntoView({ block: "nearest" });
  return true;
}

// ---------------------------------------------------------------------------------------------
// Réglages

// Pictogramme d'une forme : son trajet, calculé comme l'essaim (window.chemin), en SVG ; pour
// une image ou l'icône, chacun de ses contours (`orbite` : le réglage qui les porte).
function picto(forme, orbite) {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "-1.45 -1.45 2.9 2.9");
  svg.setAttribute("class", "picto");
  svg.setAttribute("aria-hidden", "true");
  const contours = window.contoursDe(orbite || (etat.reglages && etat.reglages.orbite), forme);
  for (let c = 0; c < Math.max(1, contours.length); c++) {
    const pts = [];
    for (let i = 0; i <= 120; i++) {
      const [x, y] = window.chemin(forme, (i / 120) * Math.PI * 2, contours, c);
      pts.push(x.toFixed(3) + "," + y.toFixed(3));
    }
    const ligne = document.createElementNS(NS, "polyline");
    ligne.setAttribute("points", pts.join(" "));
    svg.append(ligne);
  }
  return svg;
}

const val = (section, cle) => etat.reglages[section][cle];
// Valeurs changées dans la page, envoyées 60 ms après le dernier changement : « section.cle » →
// { section, cle, valeur, minuterie, envoyee }. Tant que l'appli ne les a pas reprises, elles
// restent posées sur l'état qu'elle envoie (sinon un état parti avant l'envoi ferait revenir
// la glissière en arrière).
const attente = new Map();
const poser = (r, section, cle, valeur) => ({ ...r, [section]: { ...r[section], [cle]: valeur } });
function regler(section, cle, valeur) {
  etat.reglages = poser(etat.reglages, section, cle, valeur);
  grand.regler(etat.reglages);
  apercuChange();
  const k = section + "." + cle;
  const a = attente.get(k);
  if (a) clearTimeout(a.minuterie);
  const v = { section, cle, valeur, envoyee: 0 };
  v.minuterie = setTimeout(() => { v.envoyee = performance.now(); envoyer({ cmd: "regler", section, cle, valeur }); }, 60);
  attente.set(k, v);
}

// État reçu : les valeurs en attente par-dessus. Une valeur envoyée est oubliée dès que l'état la
// reprend (à la tolérance près : l'appli borne les valeurs), ou une seconde après l'envoi.
function reglagesRecus(recus) {
  let r = recus;
  for (const [k, a] of attente) {
    const vu = recus[a.section] && recus[a.section][a.cle];
    if (a.envoyee && (memeValeur(vu, a.valeur) || performance.now() - a.envoyee > 1000)) { attente.delete(k); continue; }
    r = poser(r, a.section, a.cle, a.valeur);
  }
  return r;
}
const enCours = () => [...attente.values()].some((a) => !a.envoyee);

// Impact sur la puissance, de 1 à 10. Mesuré par le banc `mesure_impact_reglages` (coût d'une
// image avec le réglage au minimum puis au maximum, carte graphique, 144 images/s) : score =
// 1 + écart en % d'un cœur / 1,5. « estime » : réglage hors du banc, score déduit de ce qu'il fait.
const IMPACT = {
  "orbite.nombre": [1], "orbite.anneaux": [1], "orbite.rayon": [4], "orbite.vitesse_base": [2],
  "orbite.trainee": [1], "orbite.cometes": [1], "orbite.forme": [1], "orbite.rotation": [1, "estime"],
  "essaim.maximum": [2], "essaim.fixes": [2], "essaim.duree": [1], "essaim.forme": [1, "estime"],
  "eclat.taille": [1], "couleur.teinte": [1], "couleur.vitesse_teinte": [1], "couleur.ecart_teinte": [1],
  "audio.sensibilite_kick": [2, "estime"], "audio.delai_kick": [1, "estime"], "fissures.taille": [1, "estime"], "fissures.duree": [1, "estime"],
  "bordure.repos": [1, "estime"],
  "rendu.cadence": [7, "estime"], "rendu.cadence_repos": [2, "estime"], "rendu.carte_graphique": [10], "rendu.veille_apres": [1, "estime"],
  "particules.taille": [1, "estime"], "particules.rebonds": [1, "estime"], "sillage.densite": [1, "estime"],
  // banc `mesure_effets` (carte graphique, 144 images/s)
  "projecteur.actif": [1], "projecteur.duree": [1, "estime"], "reticule.actif": [1], "reticule.taille": [1, "estime"], "reticule.opacite": [1, "estime"],
  "projecteur.secousse": [1, "estime"], "projecteur.sensibilite": [1, "estime"], "projecteur.taille": [1, "estime"], "projecteur.opacite": [1, "estime"],
  "reticule.epaisseur": [1, "estime"], "reticule.vide": [1, "estime"],
  "spectre.actif": [2], "spectre.bandes": [2], "spectre.rayon": [1, "estime"], "spectre.hauteur": [1, "estime"], "spectre.masquer_sans_musique": [1, "estime"],
  "orbite.inertie": [1, "estime"],
  "essaim.constellation": [2, "estime"], "couleur.palette": [1, "estime"], "couleur.ecart_palette": [1, "estime"],
  "orbite.rotation_sur": [1, "estime"],
  // rubans et ressorts : hors du banc (un ruban large pose plus de pastilles qu'une traînée fine)
  "rubans.actif": [2, "estime"], "rubans.largeur": [2, "estime"], "rubans.vrille": [1, "estime"],
  "ressorts.actif": [1, "estime"], "ressorts.raideur": [1, "estime"], "ressorts.amorti": [1, "estime"], "ressorts.etirement": [1, "estime"],
  "action.rebond": [1, "estime"], "action.poussee": [1, "estime"], "action.frein": [1, "estime"],
  // petites figures : quelques traits et points, hors du banc
  "arcs.actif": [1, "estime"], "arcs.rayon": [1, "estime"], "arcs.epaisseur": [1, "estime"],
  "quatre_temps.actif": [1, "estime"], "quatre_temps.taille": [1, "estime"], "quatre_temps.ecart": [1, "estime"],
  "anneau.actif": [1, "estime"], "anneau.rayon": [1, "estime"], "anneau.epaisseur": [1, "estime"],
  "orbe.actif": [2, "estime"], "orbe.rayon": [1, "estime"], "orbe.relief": [1, "estime"], "orbe.epaisseur": [1, "estime"],
  "voix.actif": [1, "estime"], "voix.largeur": [1, "estime"], "voix.hauteur": [1, "estime"], "voix.epaisseur": [1, "estime"],
  "trame.actif": [2, "estime"], "trame.rayon": [3, "estime"], "trame.pas": [2, "estime"], "trame.taille": [1, "estime"],
  "pinceau.actif": [1, "estime"], "pinceau.largeur": [1, "estime"], "pinceau.duree": [1, "estime"],
  "pendule.actif": [1, "estime"], "pendule.longueur": [1, "estime"], "pendule.taille": [1, "estime"],
  "gouttes.actif": [2, "estime"], "gouttes.taille": [1, "estime"], "gouttes.epaisseur": [1, "estime"],
  // loin du pointeur : leur zone agrandit celle que le moteur redessine
  "equerres.actif": [2, "estime"], "equerres.bras": [1, "estime"], "equerres.epaisseur": [1, "estime"],
  "lisere.actif": [3, "estime"], "lisere.epaisseur": [1, "estime"],
  "halo.actif": [3, "estime"], "halo.profondeur": [2, "estime"], "halo.epaisseur": [1, "estime"],
  // une image recopiée, quelques secondes par morceau
  "etiquette.actif": [1, "estime"], "etiquette.taille": [1, "estime"], "etiquette.duree": [1, "estime"],
  // actions des déclencheurs (estimé : une particule de plus ou de moins pèse peu, une gerbe d'étincelles un peu plus)
  "action.eclat": [1, "estime"], "action.satellites": [2, "estime"], "action.inversion": [1, "estime"], "action.bordure": [1, "estime"],
  "action.onde": [1, "estime"], "action.eclats": [2, "estime"], "action.fissure": [2, "estime"], "action.implosion": [1, "estime"],
  "action.carre": [1, "estime"], "action.etincelles": [2, "estime"], "action.explosion": [2, "estime"], "action.rafale": [2, "estime"],
};
const PRECISIONS = {
  "orbite.rayon": "Une orbite plus grande agrandit la zone redessinée à chaque image.",
  "rendu.cadence": "Multiplie le coût de tout le reste : 240 images/s coûtent huit fois plus que 30.",
  "rendu.carte_graphique": "Mesuré : 0,35 ms par image avec la carte graphique, 1,09 ms avec le processeur.",
  "projecteur.actif": "Rien hors de l'effet ; pendant l'effet, tout l'écran est redessiné : +0,13 ms par image avec la carte graphique, +9 ms avec le processeur.",
  "spectre.actif": "Analyse du son en bandes : 0,9 % d'un cœur à 32 bandes, rien quand l'anneau est coupé ; le dessin ne coûte presque rien.",
  "spectre.bandes": "L'analyse du son coûte en proportion : 0,2 % d'un cœur à 8 bandes, 0,9 % à 32.",
  "orbe.actif": "Demande l'analyse du son en bandes, comme le spectre (16 bandes s'il est coupé).",
  "trame.rayon": "Une loupe plus grande, ce sont plus de points à dessiner et une zone plus large redessinée à chaque image.",
  "trame.pas": "Un pas plus serré multiplie les points : quatre fois plus à moitié pas.",
  "equerres.actif": "Quand les équerres bougent (un kick, la basse), le moteur redessine un rectangle qui va du pointeur aux coins de la fenêtre ; au repos, rien.",
  "halo.actif": "Tant que la musique joue, le moteur redessine à chaque image le rectangle de la fenêtre au premier plan (et jusqu'au pointeur). Léger avec la carte graphique, lourd avec le dessin par le processeur ; les équerres en sont la version économe.",
  "lisere.actif": "Tant que la musique joue, le moteur redessine un rectangle qui va du pointeur au bord bas de l'écran. Léger avec la carte graphique, lourd avec le dessin par le processeur.",
  "action.fissure": "Une fissure redessine sa zone de l'écran pendant qu'elle s'efface.",
  "essaim.constellation": "Une ligne par paire de têtes proches, 160 au plus.",
};

// Jauge d'impact : dix cases, et son texte (score, précision) dans un élément lu par les lecteurs
// d'écran ; le contrôle qu'elle décrit s'y relie par `decrire` (aria-describedby).
let jauges = 0;
function jaugeImpact(cle) {
  const [score, estime] = IMPACT[cle] || [1, "estime"];
  const texte = `Impact sur la puissance : ${score} sur 10 (${estime ? "estimé" : "mesuré"}).` + (PRECISIONS[cle] ? " " + PRECISIONS[cle] : "");
  const j = el("span", "impact" + (score >= 7 ? " fort" : ""));
  j.id = "impact-" + ++jauges;
  for (let i = 1; i <= 10; i++) j.append(el("i", i <= score ? "plein" : ""));
  j.append(el("span", "sr", texte));
  j.title = texte;
  return j;
}
const decrire = (controle, ...ids) => controle.setAttribute("aria-describedby", ids.join(" "));

// Chaque réglage : bornes de la glissière, affichage (aff) et lecture d'une valeur tapée (vers,
// dans l'unité affichée ; par défaut le nombre lui-même).
const tpm = { aff: (v) => Math.round((v * 60) / (2 * Math.PI)) + " tr/min", vers: (x) => (x * 2 * Math.PI) / 60 };
const pourcent = { aff: (v) => Math.round(v * 100) + " %", vers: (x) => x / 100 };
// Délai de veille en secondes : « jamais » (0), « 45 s », « 5 min », « 1,5 min » ; une valeur
// tapée en minutes (« 5 min ») est convertie.
const dureeVeille = {
  aff: (v) => (+v === 0 ? "jamais" : v < 60 ? Math.round(v) + " s" : (Number.isInteger(v / 60) ? v / 60 : virgule(v / 60, 1)) + " min"),
  lire: (texte, x) => (/jamais/i.test(texte) ? 0 : /min/i.test(texte) ? x * 60 : x),
};
const TEINTES = [0, 25, 50, 95, 150, 190, 215, 260, 295, 330];

// Conditions : un réglage dont l'effet est coupé est estompé (il reste réglable).
const declDe = (r) => (r.declencheurs && Array.isArray(r.declencheurs.liste) ? r.declencheurs.liste : []);
const modsDe = (r) => (r.modulations && Array.isArray(r.modulations.liste) ? r.modulations.liste : []);
const fait = (r, ...actions) => declDe(r).some((d) => actions.includes(d.faire));
const SI = {
  cometes: (r) => r.orbite.cometes,
  son: (r) => r.audio.actif,
  bordure: (r) => r.bordure.active,
  sillage: (r) => r.sillage.actif,
  projecteur: (r) => !!(r.projecteur && r.projecteur.actif),
  secousse: (r) => !!(r.projecteur && r.projecteur.actif && r.projecteur.secousse),
  reticule: (r) => !!(r.reticule && r.reticule.actif),
  spectre: (r) => !!(r.spectre && r.spectre.actif) && r.audio.actif,
  palette: (r) => (r.couleur.palette || 1) > 1,
  rubans: (r) => !!(r.rubans && r.rubans.actif) && r.orbite.cometes,
  ressorts: (r) => !!(r.ressorts && r.ressorts.actif),
  // le ressort sert : mode ressorts, ou au moins un rebond dans les déclencheurs
  ressort: (r) => !!(r.ressorts && r.ressorts.actif) || fait(r, "rebond"),
  // le trajet tourne : rotation réglée, ou modulée
  rotation: (r) => r.orbite.rotation !== 0 || modsDe(r).some((m) => m.cible === "rotation" && m.quantite),
};

// Modulations : sources, cibles et groupes (comme config::Source, Cible, Groupe) ; effet d'une
// modulation à fond, dans l'unité de sa cible (comme modulation.rs).
const SOURCES_MOD = [
  ["basse", "Basse (niveau)"], ["grave", "Note grave"], ["aigu", "Note aiguë"], ["kick", "Kick"], ["caisse", "Caisse claire"],
  ["charleston", "Charleston (aigus)"], ["montee", "Montée"], ["drop", "Drop"], ["contretemps", "Contretemps"], ["tempo", "Battement du tempo"],
  ["brillance", "Brillance du son"], ["stereo", "Stéréo"],
];
const CIBLES_MOD = [
  ["rotation", "Rotation"], ["vitesse", "Vitesse"], ["acceleration", "Accélération"], ["rayon", "Rayon"], ["taille", "Taille"],
  ["teinte", "Teinte"], ["decalage", "Décalage"], ["luminosite", "Lueur"], ["trainee", "Traînée"], ["opacite", "Opacité"],
];
// Figures autour du pointeur (comme config::FIGURES ; nom distinct de celui de apercu.js, chargé dans la même page) : section et nom affiché.
const FIGURES_REGLAGES = [
  ["arcs", "Arcs"], ["quatre_temps", "Quatre temps"], ["anneau", "Anneau lisse"], ["orbe", "Anneau spectre"], ["voix", "Ligne de voix"],
  ["trame", "Trame"], ["pinceau", "Trait"], ["pendule", "Pendule"], ["gouttes", "Gouttes"],
];
const NOMS_FIGURES = Object.fromEntries(FIGURES_REGLAGES);
// Cibles qui ne font rien selon « sur » : la traînée sur une figure ; sur le trait et le pendule
// (ni rayon, ni rotation, ni place), en plus, ce qui fait tourner, grandir ou déplacer.
const SANS_EFFET_FIGURE = new Set(["trainee"]);
const SANS_EFFET_TRAIT = new Set(["trainee", "rotation", "vitesse", "acceleration", "rayon", "decalage"]);
function ciblesPour(sur) {
  const sans = sur === "pinceau" || sur === "pendule" ? SANS_EFFET_TRAIT : sur === "figures" || sur in NOMS_FIGURES ? SANS_EFFET_FIGURE : null;
  return sans ? CIBLES_MOD.filter(([id]) => !sans.has(id)) : CIBLES_MOD;
}
const GROUPES = [
  ["tout", "L'essaim"], ["cometes", "Comètes"], ["satellites", "Satellites"], ["figures", "Toutes les figures"],
  ...FIGURES_REGLAGES.map(([id, nom]) => [id, "Figure : " + nom]),
];
// sur une figure, la vitesse est dix fois moindre et le décalage se compte en px (comme
// figures::FIG_VITESSE, FIG_DECALAGE)
const UNITES_MOD_FIGURE = { vitesse: [(4 * 60) / (2 * Math.PI), " tr/min"], decalage: [48, " px"] };
// à fond (quantité 1) : valeur affichée et unité
const UNITES_MOD = {
  rotation: [90, " tr/min"], vitesse: [(40 * 60) / (2 * Math.PI), " tr/min"], acceleration: [200, " %"], rayon: [400, " %"], taille: [150, " %"],
  teinte: [180, "°"], decalage: [160, " % du rayon"], luminosite: [200, " %"], trainee: [300, " %"], opacite: [100, " %"],
};
const MODULATIONS_MAX = 16;
const LISSAGE_MOD = 0.35;
const affQuantite = (cible, q, sur) => {
  const figure = sur === "figures" || sur in NOMS_FIGURES;
  const [fond, unite] = (figure && UNITES_MOD_FIGURE[cible]) || UNITES_MOD[cible] || [100, " %"];
  const v = Math.round(q * fond);
  return (v > 0 ? "+" : v < 0 ? "−" : "") + Math.abs(v) + unite;
};

// Déclencheurs : événements et actions (comme config::Evenement, Action) ; réglages de chaque
// action (force, nombre) et ce qu'elle coûte.
const EVENEMENTS = [
  ["kick", "Kick"], ["caisse", "Caisse claire"], ["charleston", "Charleston (aigus)"], ["drop", "Drop"], ["temps", "Chaque temps"],
  ["break", "Temps d'un break"], ["contretemps", "Contretemps"], ["piste", "Changement de piste"], ["clic", "Clic"], ["clic_droit", "Clic droit"],
];
const ACTIONS = [
  ["eclat", "Éclat"], ["satellites", "Satellites"], ["inversion", "Inversion du sens"], ["bordure", "Bordure qui s'illumine"], ["onde", "Onde"],
  ["eclats", "Éclats"], ["fissure", "Fissure"], ["implosion", "Implosion"], ["carre", "Carré"], ["etincelles", "Étincelles"],
  ["explosion", "Explosion"], ["rafale", "Rafale"], ["rebond", "Rebond de l'orbite"], ["poussee", "Poussée de la rotation"], ["frein", "Coup de frein"],
];
const NOMS_EVENEMENTS = Object.fromEntries(EVENEMENTS);
const NOMS_ACTIONS = Object.fromEntries(ACTIONS);
// force : réglable, et son affichage (éclat, bordure et frein : 0 à 100 % ; tailles, rebond et
// poussée : × 0,2 à 3) ; nombre : satellites, étincelles
const ACTION_FORCE = {
  eclat: "pourcent", bordure: "pourcent", onde: "fois", eclats: "fois", fissure: "fois", implosion: "fois", carre: "fois", etincelles: "fois", explosion: "fois", rafale: "fois",
  rebond: "fois", poussee: "fois", frein: "pourcent",
};
const ACTION_NOMBRE = new Set(["satellites", "etincelles", "explosion", "rafale"]);
const IMPACT_ACTION = { etincelles: 2, explosion: 2, eclats: 2, fissure: 2 };
const DECLENCHEURS_MAX = 24;
const IDEES_DECLENCHEURS = [
  ["Étincelles sur la caisse claire", { quand: "caisse", faire: "etincelles", nombre: 6, nombre_max: 6 }],
  ["Fissure au drop", { quand: "drop", faire: "fissure", force: 2 }],
  ["Onde sur chaque temps", { quand: "temps", faire: "onde", force: 0.6 }],
  ["Carré sur les contretemps", { quand: "contretemps", faire: "carre", force: 0.7 }],
  ["Éclats sur un kick sur trois", { quand: "kick", faire: "eclats", probabilite: 0.33 }],
  ["Bordure sur la caisse claire", { quand: "caisse", faire: "bordure", force: 0.6 }],
  ["Rebond de l'orbite au kick", { quand: "kick", faire: "rebond" }],
  ["Coup de frein sur la caisse claire", { quand: "caisse", faire: "frein" }],
];

// Réactions d'avant (comme migration::Ancien), pour un preset de la communauté écrit avant les
// modulations et les déclencheurs : ses réglages d'alors deviennent ses listes.
function reactionsAvant(p) {
  const v = (s, k, d) => (p[s] && p[s][k] !== undefined ? p[s][k] : d);
  const q = (x) => Math.round(Math.max(-1, Math.min(1, x)) * 1e4) / 1e4;
  const m = (source, cible, quantite, lissage = LISSAGE_MOD) => ({ source, cible, sur: "tout", quantite: q(quantite), seuil: 0, lissage, inverser: false });
  const d = (quand, faire, plus = {}) => ({ quand, faire, force: 1, nombre: 3, nombre_max: 3, delai: 0, probabilite: 1, ...plus });
  // la basse (déjà en modulations depuis Essaim 0.4), puis le reste
  const basse = [], mods = [];
  if (v("orbite", "vitesse_basse", 11)) basse.push(m("basse", "vitesse", v("orbite", "vitesse_basse", 11) / 40));
  if (v("orbite", "eloignement_grave", 1.3)) basse.push(m("grave", "rayon", v("orbite", "eloignement_grave", 1.3) / 4));
  if (v("musique", "montee", true)) mods.push(m("montee", "acceleration", 0.6, 0), m("montee", "rayon", -0.1125, 0.4));
  if (v("musique", "contretemps", false)) mods.push(m("contretemps", "taille", 0.5 / 1.5, 0), m("contretemps", "rayon", 0.015, 0.4));
  if (v("couleur", "musique", false)) mods.push(m("basse", "teinte", (0.6 * 160) / 180, 0.65), m("brillance", "teinte", (0.4 * 160) / 180, 0.65));
  if (v("musique", "stereo", 0) > 0) mods.push(m("stereo", "decalage", v("musique", "stereo", 0), 0.42));
  const [mn, mx] = [v("essaim", "par_kick_min", 2), v("essaim", "par_kick_max", 3)];
  const double = 2 * Math.max(mn, mx, 2);
  const decl = [];
  if (v("orbite", "inverse_au_kick", true)) decl.push(d("kick", "inversion"));
  decl.push(d("kick", "eclat"), d("kick", "satellites", { nombre: Math.min(mn, mx), nombre_max: Math.max(mn, mx) }));
  if (v("bordure", "intensite", 1) > 0) decl.push(d("kick", "bordure", { force: Math.min(1, v("bordure", "intensite", 1)) }));
  if (v("audio", "pulsation_tempo", true)) decl.push(d("break", "eclat", { force: 0.35 }));
  if (v("musique", "caisse_claire", true)) decl.push(d("caisse", "carre"));
  if (v("musique", "montee", true)) decl.push(d("drop", "explosion", { nombre: double, nombre_max: double }));
  if (v("scintillement", "actif", true)) {
    const n = Math.round(Math.max(1, Math.min(10, v("scintillement", "quantite", 4))));
    decl.push(d("charleston", "etincelles", { nombre: n, nombre_max: n }));
  }
  if (v("clic", "onde", true)) decl.push(d("clic", "onde"));
  if (v("clic", "eclats", false)) decl.push(d("clic", "eclats"));
  if (v("fissures", "actives", true)) decl.push(d("clic", "fissure"));
  if (v("clic", "droit", true)) decl.push(d("clic_droit", "implosion"));
  if (v("musique", "rafale_piste", true)) decl.push(d("piste", "rafale", { nombre: double, nombre_max: double }));
  return { basse, mods, decl, particules: { rebonds: v("scintillement", "rebonds", true), taille: v("clic", "taille", 1) } };
}

// Allure commune des figures (comme config::Allure) : l'opacité avec les réglages courants, le
// reste sous « + Avancé » (l'échelle aussi : elle agrandit ce que « Rayon » ou « Taille », juste
// au-dessus, règlent déjà). Le trait et le pendule (`geo` faux) ne tournent, ne grandissent ni
// ne se déplacent.
const enPourcent = (v) => Math.round(v * 100) + " %";
// `nom` : titre des groupes quand un panneau montre deux figures ; `cache(r)` : réglages cachés
// quand la figure est éteinte (panneau « Anneau », deux formes).
function allureBase(s, cache) {
  return [{ libelle: "Opacité", section: s, cle: "opacite", min: 0.1, max: 1, pas: 0.05, aff: enPourcent, cache }];
}
function allureAvance(s, geo = true, nom, cache) {
  const reaction = (libelle, cle) => ({ libelle, section: s, cle: "reaction_" + cle, min: 0, max: 2, pas: 0.05, aff: enPourcent });
  const titre = (t) => (nom ? nom + " · " + t.charAt(0).toLowerCase() + t.slice(1) : t);
  // deux figures dans le panneau : chaque bouton nomme la sienne (« l'anneau lisse »)
  const quoi = nom ? "l'" + NOMS_FIGURES[s].toLowerCase() : null;
  return [
    { groupe: titre(geo ? "Couleur et place" : "Couleur") },
    ...(geo ? [{ libelle: "Échelle", section: s, cle: "echelle", min: 0.5, max: 2, pas: 0.05, aff: (v) => "× " + virgule(+v, 2) }] : []),
    { libelle: "Teinte", section: s, cle: "teinte", min: -180, max: 180, pas: 5, aff: (v) => (v > 0 ? "+" : v < 0 ? "−" : "") + Math.abs(Math.round(v)) + "°" },
    ...(geo ? [
      { libelle: "Rotation", section: s, cle: "rotation", min: -30, max: 30, pas: 0.5, aff: (v) => virgule(+v, 1) + " tr/min" },
      { libelle: "Décalage horizontal", section: s, cle: "decalage_x", min: -80, max: 80, pas: 1, aff: (v) => Math.round(v) + " px" },
      { libelle: "Décalage vertical", section: s, cle: "decalage_y", min: -80, max: 80, pas: 1, aff: (v) => Math.round(v) + " px" },
    ] : []),
    { groupe: titre("Réactions à la musique (0 % : aucune, 200 % : double)") },
    reaction("Kick", "kick"), reaction("Caisse claire", "caisse"), reaction("Charleston", "charleston"),
    reaction("Basse", "basse"), reaction("Montée", "montee"), reaction("Drop", "drop"),
    { copierAllure: s, quoi },
    { moduler: s, quoi, geo },
  ].map((d) => ({ ...d, cache }));
}

// Allure d'une figure copiée sur les huit autres : teinte, opacité et réactions (l'échelle, la
// rotation et le décalage dépendent de la forme, ils restent). Les valeurs remplacées sont gardées :
// « Annuler » (Ctrl+Z) les remet tant qu'aucun preset n'a été appliqué depuis.
const ALLURE_COPIEE = ["teinte", "opacite", "reaction_kick", "reaction_caisse", "reaction_charleston", "reaction_basse", "reaction_montee", "reaction_drop"];
let annulationLocale = null; // dernière action de la page que « Annuler » défait avant le dernier preset
function copierAllure(source) {
  const r = etat.reglages;
  if (!r || !r[source]) return;
  const avant = [];
  for (const [s] of FIGURES_REGLAGES) {
    if (s === source || !r[s]) continue;
    for (const cle of ALLURE_COPIEE) {
      const v = r[source][cle], ancien = r[s][cle];
      if (v === undefined || ancien === undefined || memeValeur(ancien, v)) continue;
      avant.push([s, cle, ancien]);
      regler(s, cle, v);
    }
  }
  majReglages();
  const nom = NOMS_FIGURES[source];
  if (!avant.length) { bandeau(`Les autres figures ont déjà l'allure de « ${nom} ».`); return; }
  const defaire = () => {
    if (annulationLocale !== defaire) return;
    annulationLocale = null;
    for (const [s, cle, v] of avant) regler(s, cle, v);
    majReglages();
    bandeau("Allure des figures d'avant remise.");
  };
  annulationLocale = defaire;
  bandeau(`Teinte, opacité et réactions de « ${nom} » copiées sur les autres figures.`, { libelle: "Annuler (Ctrl+Z)", faire: defaire });
}

// Panneaux de l'onglet Réglages, un par effet, rangés par familles (`famille`). `actif` :
// bascule de l'effet (section, clé) ; `present` : avec une bascule, l'effet est-il là (par
// défaut : la bascule) ; `voyant` : l'effet est-il là (sans bascule) ; `si` : le panneau est
// estompé sans… ; `base` : réglages courants ; `avance` : les autres, sous
// « + Avancé » ; `compact` : la base en lignes (réglages de la machine). Éléments : réglage
// numérique ({ libelle, section, cle, min, max, pas, aff }), rangée de bascules ({ rangee,
// puces: [[libellé, section, clé, info], …] }), trajet ({ trajet, section, cle, liste }), choix
// ({ choix, section, cle, valeurs }), bascule avec note ({ bascule, … }), déclencheurs d'une
// action ({ quand: [actions], libelle }), titre, et rangées construites à part (couleur,
// sorties, modulations, déclencheurs, boutons).
const PANNEAUX = [
  {
    id: "orbite", famille: "L'essaim", titre: "Orbite",
    note: "Le trajet autour du pointeur, que suivent les comètes et les satellites : sa taille, sa vitesse, sa rotation. Ce que le son y change (basse, note grave, montée, stéréo…) est dans Modulations.",
    base: [
      { libelle: "Rayon", section: "orbite", cle: "rayon", min: 4, max: 200, pas: 1, aff: (v) => String(Math.round(v)) },
      // les comètes et les satellites avancent sur le trajet ; le trajet lui-même tourne
      { libelle: "Vitesse sur le trajet", section: "orbite", cle: "vitesse_base", min: 0, max: 100, pas: 0.1, ...tpm },
      { libelle: "Rotation de la forme", section: "orbite", cle: "rotation", min: -60, max: 60, pas: 1, aff: (v) => Math.round(v) + " tr/min" },
      { choix: "Ce qui tourne avec le trajet", section: "orbite", cle: "rotation_sur", si: SI.rotation, valeurs: [["tout", "Comètes et satellites"], ["cometes", "Comètes"], ["satellites", "Satellites"]] },
      { quand: ["inversion"], libelle: "Sens inversé quand" },
    ],
    avance: [
      { libelle: "Inertie (traîne derrière la souris)", section: "orbite", cle: "inertie", min: 0, max: 1, pas: 0.05, aff: (v) => (+v === 0 ? "aucune" : Math.round(v * 100) + " %"), vers: (x) => x / 100 },
    ],
  },
  {
    id: "cometes", famille: "L'essaim", titre: "Comètes", actif: ["orbite", "cometes"],
    note: "Les têtes lumineuses et leur traînée, réparties sur le trajet.",
    base: [
      { trajet: "Trajet des comètes", section: "orbite", cle: "forme", liste: FORMES_REGLAGE },
      { libelle: "Comètes", section: "orbite", cle: "nombre", min: 1, max: 8, pas: 1, aff: (v) => String(v) },
      { libelle: "Traînée", section: "orbite", cle: "trainee", min: 0.01, max: 3, pas: 0.01, aff: (v) => virgule(+v, 2) + " s" },
    ],
    avance: [
      { libelle: "Anneaux de comètes", section: "orbite", cle: "anneaux", min: 1, max: 4, pas: 1, aff: (v) => String(v) },
    ],
  },
  {
    id: "rubans", famille: "L'essaim", titre: "Rubans", actif: ["rubans", "actif"], present: SI.rubans,
    note: "Les traînées des comètes deviennent des rubans : larges au milieu, affinés aux deux bouts, sans tête. Leur longueur est la traînée des comètes, leurs teintes celles de la palette ; une modulation de la taille les fait enfler (au kick, avec la basse). La caisse claire fait avancer leur vrille d'un cran.",
    base: [
      { libelle: "Largeur", section: "rubans", cle: "largeur", min: 1, max: 14, pas: 0.5, aff: (v) => virgule(+v, 1) },
      { libelle: "Vrille", section: "rubans", cle: "vrille", min: 0, max: 1, pas: 0.05, aff: (v) => (+v === 0 ? "plat" : Math.round(v * 100) + " %"), vers: (x) => x / 100 },
    ],
  },
  {
    id: "ressorts", famille: "L'essaim", titre: "Ressorts", actif: ["ressorts", "actif"], present: (r) => SI.ressorts(r) || fait(r, "rebond", "poussee", "frein"),
    note: "Tout prend de l'élan : l'orbite part, dépasse et revient au lieu de changer d'un coup, les têtes s'étirent dans leur course, les satellites naissent du centre et l'éclat devient un anneau épais, sans halo blanc. Les trois actions ci-dessous marchent aussi sans ce mode.",
    base: [
      { quand: ["rebond"], libelle: "L'orbite rebondit quand" },
      { quand: ["poussee"], libelle: "La rotation est poussée quand" },
      { quand: ["frein"], libelle: "Coup de frein quand" },
      { libelle: "Étirement des têtes", section: "ressorts", cle: "etirement", si: SI.ressorts, min: 0, max: 2, pas: 0.1, aff: (v) => (+v === 0 ? "aucun" : Math.round(v * 100) + " %"), vers: (x) => x / 100 },
    ],
    avance: [
      { libelle: "Raideur du ressort", section: "ressorts", cle: "raideur", si: SI.ressort, min: 6, max: 30, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Amorti", section: "ressorts", cle: "amorti", si: SI.ressort, min: 0.1, max: 1, pas: 0.02, aff: (v) => (+v >= 1 ? "sans rebond" : Math.round(v * 100) + " %"), vers: (x) => x / 100 },
    ],
  },
  {
    id: "satellites", famille: "L'essaim", titre: "Satellites",
    note: "Les petites traînées qui naissent sur les événements (le kick, par défaut) et vivent quelques secondes, et les satellites permanents, là même sans musique.",
    base: [
      { trajet: "Trajet des satellites", section: "essaim", cle: "forme", liste: FORMES_SATELLITES },
      // les permanents ne comptent pas dans ce maximum
      { libelle: "Ajoutés en même temps, au plus", section: "essaim", cle: "maximum", min: 0, max: 64, pas: 1, aff: (v) => deux(v) },
      { libelle: "Permanents", section: "essaim", cle: "fixes", min: 0, max: 32, pas: 1, aff: (v) => String(v) },
      { quand: ["satellites"], libelle: "Ajoutés quand" },
      { rangee: "Liens", puces: [["Constellation", "essaim", "constellation", "De fines lignes relient les satellites et les comètes proches"]] },
    ],
    avance: [
      { libelle: "Durée de vie", section: "essaim", cle: "duree", min: 0.1, max: 30, pas: 0.1, aff: (v) => virgule(+v, 1) + " s" },
    ],
  },
  {
    id: "couleur", famille: "L'essaim", titre: "Couleur",
    note: "Une teinte fixe ou qui défile, ou celle de ce qui est à l'écran : l'application, le site, la pochette du morceau. Pour qu'elle glisse avec la musique : une modulation de la teinte.",
    base: [{ special: "couleur" }],
    avance: [
      { libelle: "Teinte de départ", section: "couleur", cle: "teinte", min: 0, max: 360, pas: 1, aff: (v) => Math.round(v) + "°" },
      { libelle: "Défilement de la teinte", section: "couleur", cle: "vitesse_teinte", min: 0, max: 720, pas: 1, aff: (v) => (+v === 0 ? "fixe" : Math.round(v) + "°/s") },
      { libelle: "Écart tête-queue", section: "couleur", cle: "ecart_teinte", min: 0, max: 720, pas: 1, aff: (v) => Math.round(v) + "°" },
      { libelle: "Teintes de la palette", section: "couleur", cle: "palette", min: 1, max: 3, pas: 1, aff: (v) => (+v === 1 ? "une" : String(v)) },
      { libelle: "Écart entre les teintes", section: "couleur", cle: "ecart_palette", si: SI.palette, min: 0, max: 180, pas: 1, aff: (v) => Math.round(v) + "°" },
    ],
  },
  {
    id: "eclat", famille: "Les effets", titre: "Éclat", voyant: (r) => fait(r, "eclat", "explosion", "rafale") && r.eclat.taille > 0,
    note: "Un halo blanc et un anneau coloré sous le pointeur, qui font aussi briller les têtes. Avec Ressorts : un anneau épais qui sort en dépassant, sans halo. Bridé en mode sans flashs.",
    base: [
      { libelle: "Taille de l'éclat", section: "eclat", cle: "taille", min: 0, max: 20, pas: 0.1, aff: (v) => (+v === 0 ? "—" : virgule(+v, 1)) },
      { quand: ["eclat"], libelle: "Quand" },
    ],
  },
  {
    id: "particules", famille: "Les effets", titre: "Particules", voyant: (r) => fait(r, "onde", "eclats", "implosion", "carre", "etincelles", "explosion", "rafale") || r.sillage.actif,
    note: "Ondes de choc, éclats, étincelles, carrés, implosions et sillage de la souris.",
    base: [
      { quand: ["onde", "eclats", "implosion", "carre", "etincelles", "explosion", "rafale"], libelle: "Quand" },
      { libelle: "Taille des ondes et des éclats", section: "particules", cle: "taille", min: 0.3, max: 3, pas: 0.1, aff: (v) => virgule(+v, 1) },
      { rangee: "Rebonds", puces: [["Rebonds sur la fenêtre", "particules", "rebonds", "Étincelles, éclats et poussière rebondissent sur les bords de la fenêtre au premier plan"]] },
      { rangee: "Sillage", puces: [["Sillage de la souris", "sillage", "actif", "Quand la souris file vite, une poussière reste derrière elle"]] },
      { libelle: "Densité du sillage", section: "sillage", cle: "densite", si: SI.sillage, min: 1, max: 10, pas: 1, aff: (v) => String(Math.round(v)) },
    ],
  },
  {
    id: "fissures", famille: "Les effets", titre: "Fissures", voyant: (r) => fait(r, "fissure"),
    note: "Une fissure qui s'ouvre sous le pointeur, puis s'efface.",
    base: [
      { quand: ["fissure"], libelle: "Quand" },
      { libelle: "Taille", section: "fissures", cle: "taille", min: 0.2, max: 6, pas: 0.1, aff: (v) => virgule(+v, 1) },
      { libelle: "Durée", section: "fissures", cle: "duree", min: 0.2, max: 30, pas: 0.1, aff: (v) => virgule(+v, 1) + " s" },
    ],
  },
  {
    id: "pointeur", famille: "Les effets", titre: "Réticule et projecteur", voyant: (r) => SI.reticule(r) || SI.projecteur(r),
    note: "Pour retrouver le pointeur : le projecteur (secouer la souris, ou Ctrl+Alt+F) et le réticule. Si le projecteur part tout seul, baisse la sensibilité ; s'il faut trop secouer, monte-la.",
    base: [
      {
        rangee: "Retrouver le pointeur", puces: [
          ["Projecteur", "projecteur", "actif", "Secouer la souris (ou Ctrl+Alt+F) : l'écran s'assombrit autour du pointeur pour le retrouver"],
          ["Réticule", "reticule", "actif", "Deux fines lignes croisées centrées sur le pointeur, qui s'illuminent sur les éclats"],
        ],
      },
      {
        rangee: "Projecteur lancé par", si: SI.projecteur, puces: [
          ["La secousse de la souris", "projecteur", "secousse", "Quelques allers-retours rapides de gauche à droite lancent le projecteur ; coupé, seul Ctrl+Alt+F le lance. Propre à ta main et à ta souris, comme la sensibilité : ne change pas avec le preset"],
        ],
      },
      { libelle: "Sensibilité à la secousse", section: "projecteur", cle: "sensibilite", si: SI.secousse, min: 1, max: 10, pas: 0.5, aff: (v) => virgule(+v, 1) },
      { libelle: "Durée du projecteur", section: "projecteur", cle: "duree", si: SI.projecteur, min: 0.3, max: 5, pas: 0.1, aff: (v) => virgule(+v, 1) + " s" },
      { libelle: "Taille du réticule", section: "reticule", cle: "taille", si: SI.reticule, min: 10, max: 200, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Opacité du réticule", section: "reticule", cle: "opacite", si: SI.reticule, min: 0.05, max: 1, pas: 0.05, ...pourcent },
    ],
    avance: [
      { libelle: "Taille du projecteur", section: "projecteur", cle: "taille", si: SI.projecteur, min: 30, max: 300, pas: 5, aff: (v) => String(Math.round(v)) },
      { libelle: "Obscurité du projecteur", section: "projecteur", cle: "opacite", si: SI.projecteur, min: 0.2, max: 0.95, pas: 0.01, ...pourcent },
      { libelle: "Épaisseur du réticule", section: "reticule", cle: "epaisseur", si: SI.reticule, min: 0.5, max: 6, pas: 0.1, aff: (v) => virgule(+v, 1) },
      { libelle: "Vide du réticule", section: "reticule", cle: "vide", si: SI.reticule, min: 4, max: 60, pas: 1, aff: (v) => String(Math.round(v)) },
    ],
  },
  // un effet, pas une figure (sans allure) : à ne pas confondre avec l'anneau spectre des figures
  {
    id: "spectre", famille: "Les effets", titre: "Barres de spectre", actif: ["spectre", "actif"], si: SI.spectre,
    note: "Un anneau de barres autour du pointeur, une par bande de fréquences, plus longues quand la bande est forte. Il demande « Écoute » active.",
    base: [
      { libelle: "Bandes", section: "spectre", cle: "bandes", min: 8, max: 32, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Rayon", section: "spectre", cle: "rayon", min: 10, max: 150, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Hauteur des barres", section: "spectre", cle: "hauteur", min: 4, max: 100, pas: 1, aff: (v) => String(Math.round(v)) },
      { rangee: "Silence", puces: [["Masquer sans musique", "spectre", "masquer_sans_musique", "Les barres s'effacent quand plus aucun son ne joue, et reviennent avec la musique"]] },
    ],
  },
  {
    id: "arcs", famille: "Figures", titre: "Arcs", actif: ["arcs", "actif"],
    note: "Deux arcs fins autour du pointeur, comme les anneaux d'une montre : le grand s'ouvre avec la basse, le petit avec les aigus et la montée. Chaque kick fait avancer le grand d'un cran, la caisse claire fait reculer le petit, un drop les ferme en cercles. Leurs couleurs sont celles de l'essaim (tête et queue des traînées).",
    base: [
      { libelle: "Rayon", section: "arcs", cle: "rayon", min: 10, max: 80, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Épaisseur", section: "arcs", cle: "epaisseur", min: 0.5, max: 6, pas: 0.1, aff: (v) => virgule(+v, 1) },
      ...allureBase("arcs"),
    ],
    avance: allureAvance("arcs"),
  },
  {
    id: "quatre_temps", famille: "Figures", titre: "Quatre temps", actif: ["quatre_temps", "actif"],
    note: "Quatre points sous le pointeur, un par temps. Celui du temps en cours saute sur le kick et s'étire sur la caisse claire ; pendant une montée ils se rapprochent, au drop ils sautent en vague et le compte repart du premier. Ils s'effacent quand le tempo n'est pas régulier.",
    base: [
      { libelle: "Taille des points", section: "quatre_temps", cle: "taille", min: 1, max: 5, pas: 0.1, aff: (v) => virgule(+v, 1) },
      { libelle: "Écart", section: "quatre_temps", cle: "ecart", min: 4, max: 20, pas: 0.5, aff: (v) => virgule(+v, 1) },
      ...allureBase("quatre_temps"),
    ],
    avance: allureAvance("quatre_temps"),
  },
  // Anneau : deux formes, à cocher seules ou ensemble (deux anneaux) ; chacune garde sa section
  // dans les réglages (`anneau` : lisse, `orbe` : spectre)
  {
    id: "anneau", famille: "Figures", titre: "Anneau", voyant: (r) => !!(r.anneau.actif || r.orbe.actif),
    note: "Un anneau autour du pointeur, qui le suit avec un temps de retard et s'étire quand il file (d'autant plus qu'il est loin derrière). Deux formes, seules ou ensemble : lisse, un trait net qui bat avec le kick, devient carré un instant sur la caisse claire et lâche un écho au drop, un point y faisant un tour par mesure ; spectre, une ligne que les bandes de fréquences déforment (graves en haut, aigus en bas), dont chaque kick détache un écho.",
    base: [
      { rangee: "Forme", puces: [
        ["Lisse", "anneau", "actif", "Un trait net, qui bat avec le kick et devient carré sur la caisse claire"],
        ["Spectre", "orbe", "actif", "Une ligne déformée par les bandes de fréquences"],
      ] },
      { titre: "Lisse", cache: (r) => !r.anneau.actif },
      { libelle: "Rayon", section: "anneau", cle: "rayon", min: 6, max: 60, pas: 1, aff: (v) => String(Math.round(v)), cache: (r) => !r.anneau.actif },
      { libelle: "Épaisseur", section: "anneau", cle: "epaisseur", min: 0.5, max: 6, pas: 0.1, aff: (v) => virgule(+v, 1), cache: (r) => !r.anneau.actif },
      ...allureBase("anneau", (r) => !r.anneau.actif),
      { titre: "Spectre", cache: (r) => !r.orbe.actif },
      { libelle: "Rayon", section: "orbe", cle: "rayon", min: 10, max: 60, pas: 1, aff: (v) => String(Math.round(v)), cache: (r) => !r.orbe.actif },
      { libelle: "Relief", section: "orbe", cle: "relief", min: 2, max: 30, pas: 1, aff: (v) => String(Math.round(v)), cache: (r) => !r.orbe.actif },
      { libelle: "Épaisseur", section: "orbe", cle: "epaisseur", min: 0.5, max: 5, pas: 0.1, aff: (v) => virgule(+v, 1), cache: (r) => !r.orbe.actif },
      ...allureBase("orbe", (r) => !r.orbe.actif),
    ],
    avance: [...allureAvance("anneau", true, "Lisse", (r) => !r.anneau.actif), ...allureAvance("orbe", true, "Spectre", (r) => !r.orbe.actif)],
  },
  {
    id: "voix", famille: "Figures", titre: "Ligne de voix", actif: ["voix", "actif"], si: SI.son,
    note: "Une petite onde sous le pointeur, comme celle d'un message vocal : les grandes ondulations suivent la basse et le kick, les moyennes la caisse claire, les fines le charleston. Elle ne passe jamais sur le pointeur et s'efface quand plus rien ne joue.",
    base: [
      { libelle: "Largeur", section: "voix", cle: "largeur", min: 20, max: 120, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Hauteur des ondulations", section: "voix", cle: "hauteur", min: 2, max: 20, pas: 0.5, aff: (v) => virgule(+v, 1) },
      { libelle: "Épaisseur", section: "voix", cle: "epaisseur", min: 0.5, max: 5, pas: 0.1, aff: (v) => virgule(+v, 1) },
      ...allureBase("voix"),
    ],
    avance: allureAvance("voix"),
  },
  {
    id: "trame", famille: "Figures", titre: "Trame", actif: ["trame", "actif"],
    note: "Une grille de points fixe à l'écran, visible seulement autour du pointeur, comme sous une loupe ; un vide est gardé sous le pointeur. Les coups la traversent en vagues : ronde pour le kick, en losange pour la caisse claire, plus grande au drop ; le charleston fait scintiller quelques points.",
    base: [
      { libelle: "Rayon de la loupe", section: "trame", cle: "rayon", min: 20, max: 120, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Pas de la grille", section: "trame", cle: "pas", min: 6, max: 24, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Taille des points", section: "trame", cle: "taille", min: 0.3, max: 3, pas: 0.1, aff: (v) => virgule(+v, 1) },
      ...allureBase("trame"),
    ],
    avance: allureAvance("trame"),
  },
  {
    id: "pinceau", famille: "Figures", titre: "Trait", actif: ["pinceau", "actif"],
    note: "Rien au repos. Quand la souris file, elle laisse un trait de pinceau, plus épais quand elle va vite, qui s'efface en moins d'une seconde ; chaque kick y fait glisser une perle. Dans l'aperçu, passe vite la souris dessus pour le voir.",
    base: [
      { libelle: "Largeur à pleine vitesse", section: "pinceau", cle: "largeur", min: 2, max: 12, pas: 0.5, aff: (v) => virgule(+v, 1) },
      { libelle: "Durée", section: "pinceau", cle: "duree", min: 0.2, max: 1, pas: 0.02, aff: (v) => virgule(+v, 2) + " s" },
      ...allureBase("pinceau"),
    ],
    avance: allureAvance("pinceau", false),
  },
  {
    id: "pendule", famille: "Figures", titre: "Pendule", actif: ["pendule", "actif"],
    note: "Un petit poids pendu au pointeur par un fil. Il se balance quand la souris bouge ; chaque kick le fait sauter (le fil se détend puis se retend), la caisse claire lui donne une pichenette, le fil s'allonge avec la basse et se raccourcit pendant une montée.",
    base: [
      { libelle: "Longueur du fil", section: "pendule", cle: "longueur", min: 8, max: 60, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Taille du poids", section: "pendule", cle: "taille", min: 1, max: 6, pas: 0.1, aff: (v) => virgule(+v, 1) },
      ...allureBase("pendule"),
    ],
    avance: allureAvance("pendule", false),
  },
  {
    id: "gouttes", famille: "Figures", titre: "Gouttes", actif: ["gouttes", "actif"],
    note: "Une ligne liquide autour du pointeur : un noyau et quatre perles en orbite, dont on ne voit que le contour, qui se soude quand elles se touchent et se sépare quand elles s'éloignent. L'intérieur reste vide. Le kick projette les perles, une montée fond tout en un seul anneau, un drop lâche des bulles.",
    base: [
      { libelle: "Taille", section: "gouttes", cle: "taille", min: 0.6, max: 1.6, pas: 0.05, aff: (v) => "× " + virgule(+v, 2) },
      { libelle: "Épaisseur", section: "gouttes", cle: "epaisseur", min: 1, max: 4, pas: 0.1, aff: (v) => virgule(+v, 1) },
      ...allureBase("gouttes"),
    ],
    avance: allureAvance("gouttes"),
  },
  // Habillage de la fenêtre au premier plan et de l'écran : loin du pointeur, aux couleurs de
  // l'essaim. La bordure est réglée par le preset ; équerres, halo et liseré sont hors des presets.
  {
    id: "bordure", famille: "Fenêtre et écran", titre: "Bordure", actif: ["bordure", "active"],
    note: "La bordure de la fenêtre au premier plan prend la couleur de l'essaim, et s'illumine sur ses déclencheurs (Windows 11). Pas en mode sans flashs.",
    base: [
      { libelle: "Luminosité au repos", section: "bordure", cle: "repos", min: 0, max: 1, pas: 0.05, ...pourcent },
      { quand: ["bordure"], libelle: "S'illumine quand" },
    ],
  },
  {
    id: "equerres", famille: "Fenêtre et écran", titre: "Équerres", actif: ["equerres", "actif"], horsPresets: true,
    note: "Quatre petites équerres aux coins de la fenêtre au premier plan, à l'extérieur de son cadre, aux couleurs de l'essaim. Elles s'écartent sur le kick, leurs bras s'allongent avec la basse, la caisse claire allume une diagonale puis l'autre. Comme le halo et le liseré, et contrairement à la bordure, elles ne font pas partie des presets : elles restent quand tu en changes. Dans l'aperçu, la fenêtre est le cadre de l'aperçu.",
    base: [
      { libelle: "Longueur des bras", section: "equerres", cle: "bras", min: 4, max: 30, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Épaisseur", section: "equerres", cle: "epaisseur", min: 1, max: 5, pas: 0.1, aff: (v) => virgule(+v, 1) },
    ],
  },
  {
    id: "halo", famille: "Fenêtre et écran", titre: "Halo de fenêtre", actif: ["halo", "actif"], si: SI.son, horsPresets: true,
    note: "La lumière court le long du cadre de la fenêtre au premier plan : elle entre par le bord le plus proche du pointeur, s'élargit avec la basse, chaque kick en fait partir deux éclats le long du cadre, un drop l'embrase en entier. Elle déborde un peu vers l'intérieur de la fenêtre, jamais plus que la profondeur réglée, et s'efface quand plus rien ne joue. Comme les équerres, le halo ne fait pas partie des presets et reste quand tu en changes.",
    base: [
      { libelle: "Profondeur", section: "halo", cle: "profondeur", min: 6, max: 30, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Épaisseur", section: "halo", cle: "epaisseur", min: 1, max: 4, pas: 0.1, aff: (v) => virgule(+v, 1) },
    ],
  },
  {
    id: "lisere", famille: "Fenêtre et écran", titre: "Liseré", actif: ["lisere", "actif"], si: SI.son, horsPresets: true,
    note: "Un filet de lumière au bord bas de l'écran du pointeur, à son aplomb, aux couleurs de l'essaim. Il s'élargit avec la basse, chaque kick y court vers les deux côtés, un drop allume tout le bord ; il s'efface quand plus rien ne joue. Il ne fait pas partie des presets et reste quand tu en changes. Dans l'aperçu, l'écran est le cadre de l'aperçu.",
    base: [
      { libelle: "Épaisseur", section: "lisere", cle: "epaisseur", min: 1, max: 8, pas: 0.5, aff: (v) => virgule(+v, 1) },
      { rangee: "Écrans", puces: [["Sur tous les écrans", "lisere", "tous_ecrans", "Au bord bas de chaque écran, pas seulement de celui du pointeur ; les éclats du kick passent de l'un à l'autre"]] },
    ],
  },
  {
    id: "modulations", famille: "Réactions", titre: "Modulations", si: SI.son,
    note: "Un élément du son fait varier un paramètre, en continu, pour l'essaim (comètes, satellites ou les deux) ou pour les figures (toutes ou l'une d'elles) : la rotation qui s'emballe au drop, l'orbite qui s'ouvre sur les graves… Les quatre premières, de base, font accélérer l'essaim avec la basse, l'éloignent sur les notes graves, le resserrent et l'accélèrent pendant une montée. Elles sont gardées dans le preset et demandent « Écoute » active.",
    base: [{ modulations: true }],
  },
  {
    id: "declencheurs", famille: "Réactions", titre: "Déclencheurs",
    note: "Quand un événement arrive (un kick, une caisse claire, un drop, un clic…), une action : un éclat, une onde, des satellites, une fissure… Ceux de base font les réactions de toujours ; ajoute, change ou retire. Ils sont gardés dans le preset ; ceux du son demandent « Écoute » active.",
    base: [{ declencheurs: true }],
  },
  {
    id: "son", famille: "Écoute", titre: "Écoute", actif: ["audio", "actif"],
    note: "L'écoute du son de Windows, dont viennent les événements (kick, caisse claire, drop, temps…) et les sources des modulations.",
    base: [
      { libelle: "Sensibilité au kick", section: "audio", cle: "sensibilite_kick", min: 1, max: 10, pas: 0.5, aff: (v) => virgule(+v, 1) },
    ],
    avance: [
      { libelle: "Écart minimal entre deux kicks", section: "audio", cle: "delai_kick", min: 0.05, max: 1, pas: 0.01, aff: (v) => virgule(+v, 2) + " s" },
      { sorties: true },
      { applisSon: true },
    ],
  },
  {
    id: "etiquette", famille: "Écoute", titre: "Étiquette du morceau", actif: ["etiquette", "actif"], horsPresets: true,
    note: "Quand le morceau change, son titre et son artiste s'écrivent à côté du pointeur, soulignés d'un filet qui bat avec le kick, puis s'effacent. Le reste du temps, rien. Ce n'est pas une allure de l'essaim : l'étiquette ne fait pas partie des presets et reste quand tu en changes. Dans l'aperçu, « Piste » la montre avec un titre d'exemple.",
    base: [
      { libelle: "Taille du texte", section: "etiquette", cle: "taille", min: 9, max: 20, pas: 1, aff: (v) => String(Math.round(v)) },
      { libelle: "Temps d'affichage", section: "etiquette", cle: "duree", min: 1.5, max: 8, pas: 0.5, aff: (v) => virgule(+v, 1) + " s" },
    ],
  },
  {
    id: "machine", famille: "Cet ordinateur", titre: "Cet ordinateur", compact: true, horsPresets: true,
    note: "Le dessin de l'essaim sur ce PC : ces réglages ne font pas partie des presets. Mises à jour et démarrage sont dans la vue Application (l'engrenage, en haut).",
    // ailleurs, propres à ce PC eux aussi : un bouton y mène
    liens: [
      ["Pauses", "application", "groupe-pauses"],
      ["Captures et stream", "application", "groupe-captures"],
      ["Pause selon l'application (Auto)", "auto", "bloc-pauses"],
    ],
    base: [
      { libelle: "Images par seconde", section: "rendu", cle: "cadence", min: 0, max: 500, pas: 1, aff: (v) => (+v === 0 ? "écran" : String(v)) },
      { libelle: "Images par seconde au repos", section: "rendu", cle: "cadence_repos", min: 0, max: 500, pas: 1, aff: (v) => (+v === 0 ? "aucun repos" : String(v)) },
      // masqué tant que l'appli ne fournit pas ce réglage
      { libelle: "Mise en veille après", section: "rendu", cle: "veille_apres", cache: (r) => typeof r.rendu.veille_apres !== "number", min: 0, max: 3600, pas: 10, ...dureeVeille },
      {
        bascule: "Dessin par la carte graphique", section: "rendu", cle: "carte_graphique",
        note: "Mesuré sur Essaim : 0,35 ms de calcul par image avec la carte graphique, 1,09 ms avec le processeur (trois fois plus). À couper seulement si l'essaim s'affiche mal ; Essaim y revient seul en cas d'erreur.",
      },
      {
        bascule: "Qualité adaptative", section: "rendu", cle: "adaptatif",
        note: "Si une image demande plus de 4 ms de calcul (ordinateur chargé), Essaim baisse sa cadence par paliers (120, 60, puis 30 images/s au plus) et la remonte quand ça redevient léger.",
      },
      {
        choix: "Sans flashs (photosensibilité)", section: "rendu", cle: "sans_flashs",
        valeurs: [["auto", "Comme Windows"], ["oui", "Toujours"], ["non", "Jamais"]],
        note: "Tous les éclats adoucis et trois par seconde au plus, quels que soient les déclencheurs ; ni halo blanc ni bordure qui clignote. « Comme Windows » : quand les effets d'animation sont coupés (Accessibilité › Effets visuels).",
      },
    ],
  },
];
// Vue « Application » (l'engrenage) : ce qui tient à Essaim sur ce PC et non à l'allure de
// l'essaim, hors presets. `maj` : sous la version, dans la colonne ; `lignes` : le reste.
const APPLICATION = {
  maj: [
    {
      bascule: "Mises à jour automatiques", section: "mise_a_jour", cle: "auto",
      note: "Essaim cherche les nouvelles versions publiées sur GitHub, les télécharge, vérifie leur empreinte et te prévient par une notification. La nouvelle version s'applique au redémarrage.",
    },
  ],
  lignes: [
    { groupe: "Démarrage" },
    { demarrage: true },
    { accueil: true },
    { groupe: "Pauses", id: "groupe-pauses" },
    {
      bascule: "Pause quand Windows ne veut pas être dérangé", section: "pause", cle: "ne_pas_deranger",
      note: "Jeu ou application en plein écran au premier plan, mode présentation : Essaim se met en pause et reprend tout seul ensuite.",
    },
    {
      choix: "Sur batterie", section: "pause", cle: "batterie",
      valeurs: [["rien", "Rien ne change"], ["reduire", "30 images/s au plus"], ["pause", "Pause"]],
      note: "Sur batterie ou avec l'économiseur de batterie, Essaim peut ralentir pour durer plus longtemps, ou se mettre en pause.",
    },
    { groupe: "Captures et stream", id: "groupe-captures" },
    {
      bascule: "Masquer Essaim des captures et des streams", section: "rendu", cle: "masquer_captures",
      note: "L'essaim reste visible à l'écran, mais n'apparaît plus dans les captures d'écran, les enregistrements ni les partages d'écran (OBS, Discord, Teams…), y compris celles qu'on fait pour vérifier son rendu. Il faut Windows 10 version 2004 ou plus récent.",
    },
    {
      choix: "Fond de la fenêtre pour le stream", section: "stream", cle: "fond",
      valeurs: [["transparent", "Transparent"], ["vert", "Vert"], ["bleu", "Bleu"], ["magenta", "Magenta"], ["noir", "Noir"]],
      note: "La fenêtre pour le stream montre l'essaim au centre, à une taille qui tient toujours dans la fenêtre. Transparent : dans OBS, ajoute une « Capture de jeu » sur « Essaim — stream » et coche « Autoriser la transparence » et « Valeurs alpha prémultipliées » (à l'écran, la fenêtre paraît sur fond noir). Fond uni : « Capture de fenêtre » et filtre « Incrustation chromatique » (ou, sur fond noir, un mode de fusion « Écran »). Elle reste capturable même si Essaim est masqué des captures.",
    },
    { stream: true },
  ],
};
// Tous les éléments, pour retrouver le libellé d'un réglage (comparaison de presets).
const ELEMENTS = PANNEAUX.flatMap((p) => p.base.concat(p.avance || []));

const SUGGESTIONS_MOD = [
  ["La basse accélère (de base)", { source: "basse", cible: "vitesse", sur: "tout", quantite: 0.275 }],
  ["Les graves éloignent l'orbite (de base)", { source: "grave", cible: "rayon", sur: "tout", quantite: 0.325 }],
  ["La montée accélère (de base)", { source: "montee", cible: "acceleration", sur: "tout", quantite: 0.6, lissage: 0 }],
  ["Satellites qui s'emballent au drop", { source: "drop", cible: "rotation", sur: "satellites", quantite: 0.8 }],
  ["Comètes qui grossissent au kick", { source: "kick", cible: "taille", sur: "cometes", quantite: 0.6 }],
  ["La teinte suit la musique", { source: "basse", cible: "teinte", sur: "tout", quantite: 0.5333, lissage: 0.65 }],
  ["Penche vers le côté qui joue", { source: "stereo", cible: "decalage", sur: "tout", quantite: 0.6, lissage: 0.42 }],
  ["Lueur sur la caisse claire", { source: "caisse", cible: "luminosite", sur: "tout", quantite: 0.5 }],
  ["Traînées longues sur les graves", { source: "grave", cible: "trainee", sur: "cometes", quantite: 0.5 }],
];

const glissieres = []; // { def, input, saisie, els }
const bascules = []; // { section, cle, bouton }
const choix = []; // { section, cle, valeur, bouton } : réglages à quelques valeurs nommées
const formes = []; // { section, cle, id, b }
const conditionnels = []; // { si, cache, els } : estompés (si) ou masqués (cache) selon les réglages
const panneaux = []; // { def, onglet, section, corps, voyant }
// familles de la liste des sous-onglets : en-tête repliable, compte des effets actifs, liste de
// leurs onglets ; ouvertes (gardées d'une ouverture à l'autre)
const familles = new Map(); // nom → { bouton, signe, compte, compteLu, liste }
const famillesOuvertes = new Set(memoire.lire("familles", ["L'essaim"]));
const avances = []; // { bouton, signe, zone }
let teintes = [];
let boutonPrisme = null; // bascule « Synchro Prisme », cachée tant que Prisme n'est pas signalé

function hslCss(h) {
  const c = 1 - Math.abs(2 * 0.66 - 1), x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = 0.66 - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return `rgb(${Math.round((r + m) * 255)},${Math.round((g + m) * 255)},${Math.round((b + m) * 255)})`;
}

// Valeur tapée : nombre dans l'unité affichée (« 0,3 s », « 45 % », « écran » = 0) ; `lire`
// interprète le texte entier quand l'unité peut changer.
function lireSaisie(def, texte) {
  const m = texte.replace(",", ".").match(/-?\d+(\.\d+)?/);
  const x = m ? parseFloat(m[0]) : 0;
  const v = def.lire ? def.lire(texte, x) : def.vers ? def.vers(x) : x;
  return borne(v, def.min, def.max);
}

function reglageNumerique(def, id, conteneur, avance) {
  const input = el("input");
  Object.assign(input, { type: "range", min: def.min, max: def.max, step: def.pas, id });
  input.setAttribute("aria-label", def.libelle);
  input.title = "Double-clic : valeur par défaut";
  const saisie = el("input", "valeur-saisie" + (avance ? "" : " t-chiffre"));
  saisie.type = "text";
  saisie.spellcheck = false;
  saisie.setAttribute("aria-label", def.libelle + " (valeur)");
  saisie.title = "Clique pour taper une valeur";
  const appliquer = () => {
    const v = lireSaisie(def, saisie.value);
    const arrondi = def.pas >= 1 ? Math.round(v) : Math.round(v / def.pas) * def.pas;
    regler(def.section, def.cle, def.pas >= 1 ? arrondi : +(+arrondi).toFixed(4));
    majReglages();
  };
  saisie.addEventListener("focus", () => saisie.select());
  saisie.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { appliquer(); saisie.blur(); }
    if (e.key === "Escape") { saisie.value = def.aff(val(def.section, def.cle)); saisie.blur(); }
  });
  saisie.addEventListener("change", appliquer);
  input.addEventListener("input", () => {
    regler(def.section, def.cle, +input.value);
    saisie.value = def.aff(+input.value);
    majNomActif();
  });
  input.addEventListener("dblclick", () => {
    if (!etat.defauts) return;
    const v = etat.defauts[def.section][def.cle];
    regler(def.section, def.cle, v);
    input.value = v; // la glissière a le focus : majReglages ne la touche pas
    majReglages();
  });
  const g = { def, input, saisie, els: [] };
  glissieres.push(g);
  g.els = conteneur(input, saisie) || [];
  if (def.si || def.cache) conditionnels.push({ si: def.si, cache: def.cache, els: g.els });
}

// Bascule d'un réglage (booléen, ou nombre : allumée au-dessus de 0 ; `oui` donne la valeur
// suivante), avec son explication au survol.
function bascule(libelle, section, cle, oui, info) {
  const b = el("button", "puce", libelle);
  b.setAttribute("aria-pressed", "false");
  if (info) b.title = info;
  b.addEventListener("click", () => {
    const v = val(section, cle);
    regler(section, cle, oui ? oui(v) : !v);
    majReglages();
  });
  bascules.push({ section, cle, bouton: b });
  return b;
}

function choixForme(section, cle, liste) {
  return liste.map(([id, libelle]) => {
    const b = el("button", "puce forme");
    if (id !== "cometes") b.append(picto(id));
    b.append(document.createTextNode(libelle));
    b.title = id === "trace" ? "Le contour d'une image, d'un texte ou d'un emoji" : id === "icone" ? "Le contour de l'icône de l'application au premier plan, qui change avec elle" : libelle;
    b.addEventListener("click", () => {
      if (id === "trace") { ouvrirImage(section, cle); return; }
      regler(section, cle, id);
      majReglages();
    });
    formes.push({ section, cle, id, b });
    return b;
  });
}

function pucesChoix(def) {
  return def.valeurs.map(([valeur, libelle]) => {
    const b = el("button", "puce", libelle);
    b.setAttribute("aria-pressed", "false");
    b.addEventListener("click", () => { regler(def.section, def.cle, valeur); majReglages(); });
    choix.push({ section: def.section, cle: def.cle, valeur, bouton: b });
    return b;
  });
}

// Rangée de la couleur : nuancier et sources de la teinte.
function rangeeCouleur() {
  teintes = TEINTES.map((h) => {
    const b = el("button", "teinte");
    b.style.background = hslCss(h);
    b.setAttribute("aria-label", "Teinte " + h + "°");
    b.addEventListener("click", () => {
      regler("couleur", "teinte", h);
      regler("couleur", "vitesse_teinte", 0);
      regler("couleur", "application", false);
      regler("couleur", "site", false);
      regler("couleur", "pochette", false);
      regler("couleur", "synchro_prisme", false);
      majReglages();
    });
    return { h, b };
  });
  // une teinte choisie coupe la synchro avec l'éclairage : cette bascule la remet ; montrée
  // seulement quand l'appli dit que Prisme (outil d'éclairage RGB, rarement installé) est là
  boutonPrisme = bascule("Synchro Prisme", "couleur", "synchro_prisme", null, "Prend la teinte de l'éclairage RGB de Prisme quand il tourne");
  boutonPrisme.hidden = etat.prisme !== true;
  return {
    teintes: teintes.map((t) => t.b),
    sources: [
      bascule("Teinte qui défile", "couleur", "vitesse_teinte", (v) => (v > 0 ? 0 : 60)),
      bascule("Couleur de l'application", "couleur", "application"),
      bascule("Couleur du site (navigateur)", "couleur", "site"),
      bascule("Couleur de la pochette", "couleur", "pochette", null, "Prend la teinte de la pochette du morceau qui joue (Spotify, navigateur…), avant l'application et le site"),
      boutonPrisme,
    ],
  };
}

// Réglages courants d'un panneau : grille libellé, glissière, valeur en grands chiffres ; les
// rangées de boutons prennent la glissière et la valeur.
function construireBase(def, grille, corps) {
  const libelleAvecImpact = (texte, cle, pour) => {
    const l = el(pour ? "label" : "div", "libelle-reglage");
    if (pour) l.htmlFor = pour;
    l.append(el("span", "t-label", texte), jaugeImpact(cle));
    return l;
  };
  // rangée de boutons : un groupe nommé par son libellé et décrit par sa jauge
  const ligneLarge = (libelle, cle, contenu, note) => {
    const l = libelleAvecImpact(libelle, cle);
    const nom = l.firstChild, jauge = l.lastChild;
    nom.id = "libelle-" + jauge.id;
    const d = el("div", "large");
    d.setAttribute("role", "group");
    d.setAttribute("aria-labelledby", nom.id);
    decrire(d, jauge.id);
    d.append(...contenu);
    if (note) {
      const n = el("p", "note pleine", note);
      n.id = "note-" + jauge.id;
      d.append(n);
      decrire(d, jauge.id, n.id);
    }
    grille.append(l, d);
    return [l, d];
  };
  let els = null;
  if (def.titre) {
    const t = el("div", "titre-groupe", def.titre);
    grille.append(t);
    els = [t];
  } else if (def.trajet) {
    els = ligneLarge(def.trajet, def.section + "." + def.cle, choixForme(def.section, def.cle, def.liste));
  } else if (def.rangee) {
    const [, s, c] = def.puces[0];
    els = ligneLarge(def.rangee, s + "." + c, def.puces.map(([l, sec, cle, info]) => bascule(l, sec, cle, null, info)));
  } else if (def.choix) {
    els = ligneLarge(def.choix, def.section + "." + def.cle, pucesChoix(def), def.note);
  } else if (def.special === "couleur") {
    const { teintes: nuancier, sources } = rangeeCouleur();
    ligneLarge("Teinte", "couleur.teinte", nuancier);
    ligneLarge("Prendre la couleur de", "couleur.application", sources);
  } else if (def.modulations) {
    construireModulations(corps);
  } else if (def.declencheurs) {
    construireDeclencheurs(corps);
  } else if (def.quand) {
    const contenu = [];
    construireQuand(def, contenu);
    els = ligneLarge(def.libelle, "action." + def.quand[0], contenu);
  } else {
    reglageNumerique(def, "r-" + def.section + "-" + def.cle, (input, saisie) => {
      const zone = el("div", "glissiere");
      zone.append(input);
      const sortie = el("div", "valeur");
      sortie.append(saisie);
      const libelle = libelleAvecImpact(def.libelle, def.section + "." + def.cle, input.id);
      decrire(input, libelle.lastChild.id);
      grille.append(libelle, zone, sortie);
      return [libelle, zone, sortie];
    }, false);
  }
  if (els && (def.si || def.cache)) conditionnels.push({ si: def.si, cache: def.cache, els });
}

// Réglages en lignes, sur deux colonnes (avancés, machine) : nom, jauge et valeur au-dessus de la
// glissière ; bascules et choix sur toute la largeur, avec leur note.
function construireAvance(def, zone) {
  let els = null;
  if (def.copierAllure) {
    const l = el("div", "ligne-avance");
    const b = el("button", "puce debut", def.quoi ? `Copier ${def.quoi} sur toutes les figures` : "Copier sur toutes les figures");
    b.type = "button";
    b.addEventListener("click", () => copierAllure(def.copierAllure));
    const note = el("p", "note", `Donne la teinte, l'opacité et les réactions de ${def.quoi || "cette figure"} aux huit autres figures ; l'échelle, la rotation et le décalage restent propres à chacune.`);
    note.id = "note-copier-" + def.copierAllure;
    decrire(b, note.id);
    l.append(b, note);
    zone.append(l);
    els = [l];
  } else if (def.moduler) {
    // une modulation de plus, sur cette figure, puis le panneau des modulations
    const l = el("div", "ligne-avance");
    const b = el("button", "puce debut", def.quoi ? `+ Moduler ${def.quoi}` : "+ Moduler cette figure");
    b.type = "button";
    // le trait et le pendule (`geo` faux) n'ont ni rayon, ni rotation, ni place : la basse change leur taille
    b.addEventListener("click", () => {
      montrerPanneau("modulations");
      ajouterModulation({ source: "basse", cible: def.geo ? "rayon" : "taille", sur: def.moduler, quantite: 0.1 });
    });
    const note = el("p", "note", def.geo
      ? "Une source du son fait varier la figure : son rayon, l'épaisseur de ses traits, sa rotation, sa teinte, sa place, son opacité… Voir « Modulations »."
      : "Une source du son fait varier la figure : sa taille, sa teinte, sa lueur, son opacité… Voir « Modulations ».");
    note.id = "note-moduler-" + def.moduler;
    decrire(b, note.id);
    l.append(b, note);
    zone.append(l);
    els = [l];
  } else if (def.groupe) {
    const g = el("div", "t-label groupe-avance", def.groupe);
    if (def.id) g.id = def.id;
    zone.append(g);
    els = [g];
  } else if (def.choix || def.rangee) {
    const l = el("div", "ligne-avance");
    const groupe = el("div", "groupe-puces");
    groupe.setAttribute("role", "group");
    const nom = el("span", "t-label", def.choix || def.rangee);
    nom.id = "choix-" + (def.section || def.puces[0][1]) + "-" + (def.cle || def.puces[0][2]);
    groupe.setAttribute("aria-labelledby", nom.id);
    groupe.append(nom, ...(def.choix ? pucesChoix(def) : def.puces.map(([lb, s, c, info]) => bascule(lb, s, c, null, info))));
    l.append(groupe);
    if (def.note) {
      const note = el("p", "note", def.note);
      note.id = "note-" + nom.id;
      l.append(note);
      decrire(groupe, note.id);
    }
    zone.append(l);
    els = [l];
  } else if (def.sorties) {
    zone.append(rangeeSorties());
  } else if (def.applisSon) {
    zone.append(rangeeApplisSon());
  } else if (def.bascule || def.demarrage || def.accueil || def.stream) {
    const l = el("div", "ligne-avance");
    if (def.bascule) {
      const tete = el("div", "tete");
      const cle = def.section + "." + def.cle;
      const b = bascule(def.bascule, def.section, def.cle);
      tete.append(b);
      const decrits = [];
      // sans effet sur la puissance (mises à jour) : pas de jauge
      if (IMPACT[cle]) { const j = jaugeImpact(cle); tete.append(j); decrits.push(j.id); }
      l.append(tete);
      if (def.note) {
        const note = el("p", "note", def.note);
        note.id = "note-" + def.section + "-" + def.cle;
        l.append(note);
        decrits.push(note.id);
      }
      if (decrits.length) decrire(b, ...decrits);
    } else if (def.stream) {
      const b = el("button", "puce debut", "Ouvrir la fenêtre pour le stream");
      b.addEventListener("click", () => envoyer({ cmd: "stream" }));
      l.append(b);
    } else if (def.demarrage) {
      const b = el("button", "puce debut", "Démarrer Essaim avec Windows");
      b.id = "demarrage";
      b.addEventListener("click", () => envoyer({ cmd: "demarrage", actif: !etat.demarrage }));
      l.append(b);
    } else {
      const b = el("button", "puce debut", "Revoir l'accueil");
      b.addEventListener("click", () => ouvrirAccueil());
      l.append(b);
    }
    zone.append(l);
    els = [l];
  } else {
    reglageNumerique(def, "a-" + def.section + "-" + def.cle, (input, saisie) => {
      const l = el("label");
      const tete = el("span", "entete-reglage");
      const nom = el("span", "nom-reglage");
      const jauge = jaugeImpact(def.section + "." + def.cle);
      decrire(input, jauge.id);
      nom.append(el("span", "", def.libelle), jauge);
      tete.append(nom, saisie);
      l.append(tete, input);
      zone.append(l);
      return [l];
    }, true);
  }
  if (els && (def.si || def.cache)) conditionnels.push({ si: def.si, cache: def.cache, els });
}

// Va à un réglage d'une autre vue (liens du panneau « Cet ordinateur ») : la vue, puis le groupe,
// qui prend le focus (les lecteurs d'écran le lisent).
function allerA(vue, id) {
  montrerVue(vue);
  requestAnimationFrame(() => {
    const cible = $(id);
    if (!cible || cible.hidden) return;
    cible.tabIndex = -1;
    cible.scrollIntoView({ block: "start" });
    cible.focus({ preventScroll: true });
  });
}

function construireReglages() {
  const nav = $("sous-onglets"), zone = $("panneaux");
  let famille = null, liste = null, nFamilles = 0;
  for (const p of PANNEAUX) {
    if (p.famille !== famille) {
      famille = p.famille;
      // chaque famille : une liste d'onglets (tablist) ; repliable par son en-tête, qui la nomme,
      // sauf une famille d'un seul panneau (Cet ordinateur), nommée par aria-label
      const n = nFamilles++;
      liste = el("div", "onglets-famille");
      liste.id = "famille-" + n;
      liste.setAttribute("role", "tablist");
      liste.setAttribute("aria-orientation", "vertical");
      if (PANNEAUX.filter((x) => x.famille === famille).length > 1) {
        const nom = famille;
        const b = el("button", "t-label famille");
        b.type = "button";
        b.id = "tete-famille-" + n;
        b.setAttribute("aria-controls", liste.id);
        const signe = el("span", "signe", "");
        signe.setAttribute("aria-hidden", "true");
        // compte des effets actifs : chiffre à l'écran, phrase pour les lecteurs d'écran
        const compte = el("span", "compte-famille");
        compte.setAttribute("aria-hidden", "true");
        const compteLu = el("span", "sr");
        b.append(signe, el("span", "", nom), compte, compteLu);
        b.addEventListener("click", () => basculerFamille(nom));
        liste.setAttribute("aria-labelledby", b.id);
        nav.append(b);
        familles.set(nom, { bouton: b, signe, compte, compteLu, liste });
      } else {
        liste.setAttribute("aria-label", famille);
      }
      nav.append(liste);
    }
    // onglet : nom, et voyant de l'effet (allumé s'il est actif) ou nombre de modulations
    const o = el("button", "sous-onglet");
    Object.assign(o, { id: "sous-onglet-" + p.id, tabIndex: -1 });
    o.setAttribute("role", "tab");
    o.setAttribute("aria-controls", "panneau-" + p.id);
    o.setAttribute("aria-selected", "false");
    o.append(el("span", "nom", p.titre));
    const voyant = p.actif || p.voyant ? el("span", "voyant") : p.id === "modulations" || p.id === "declencheurs" ? el("span", "compte") : null;
    if (voyant) { voyant.setAttribute("aria-hidden", "true"); o.append(voyant); }
    o.addEventListener("click", () => montrerPanneau(p.id));
    liste.append(o);

    const sec = el("section", "panneau-reglages");
    sec.id = "panneau-" + p.id;
    sec.setAttribute("role", "tabpanel");
    sec.setAttribute("aria-labelledby", o.id);
    sec.hidden = true;
    const tete = el("div", "tete-panneau");
    const titres = el("div", "titres-panneau");
    titres.append(el("h2", "t-titre", p.titre));
    // réglages propres à cet ordinateur, ou habillage qui reste d'un preset à l'autre
    if (p.horsPresets) {
      const h = el("span", "hors-presets", "hors presets");
      h.title = "Ne fait pas partie des presets : reste tel quel quand tu en changes";
      titres.append(h);
    }
    tete.append(titres);
    if (p.actif) {
      const b = bascule("Actif", p.actif[0], p.actif[1]);
      b.setAttribute("aria-label", p.titre + " : actif");
      tete.append(b);
    }
    sec.append(tete, el("p", "note intro", p.note));
    if (p.liens) {
      const g = el("div", "groupe-puces liens-panneau");
      g.setAttribute("role", "group");
      const nom = el("span", "t-label", "Aussi propres à ce PC");
      nom.id = "liens-" + p.id;
      g.setAttribute("aria-labelledby", nom.id);
      g.append(nom, ...p.liens.map(([libelle, vue, id]) => {
        const b = el("button", "puce petite", libelle + " →");
        b.type = "button";
        b.addEventListener("click", () => allerA(vue, id));
        return b;
      }));
      sec.append(g);
    }
    const corps = el("div", "corps-panneau");
    if (p.compact) {
      const lignes = el("div", "avance");
      for (const def of p.base) construireAvance(def, lignes);
      corps.append(lignes);
    } else {
      const grille = el("div", "reglages");
      corps.append(grille);
      for (const def of p.base) construireBase(def, grille, corps);
      if (!grille.childElementCount) grille.remove();
    }
    if (p.avance && p.avance.length) {
      const b = el("button", "bascule-avance");
      const signe = el("span", "signe", "+");
      b.append(signe, document.createTextNode(" Avancé"));
      const av = el("div", "avance");
      av.id = "avance-" + p.id;
      b.setAttribute("aria-controls", av.id);
      for (const def of p.avance) construireAvance(def, av);
      b.addEventListener("click", () => { etat.avance = !etat.avance; memoire.ecrire("avance", etat.avance); majAvance(); });
      corps.append(b, av);
      avances.push({ bouton: b, signe, zone: av });
    }
    sec.append(corps);
    zone.append(sec);
    panneaux.push({ def: p, onglet: o, section: sec, corps, voyant });
  }
  // flèches haut et bas, Début, Fin : d'un onglet à l'autre (le panneau suit le focus)
  nav.addEventListener("keydown", (e) => {
    const i = panneaux.findIndex((p) => p.def.id === etat.panneau);
    const j = { ArrowDown: i + 1, ArrowRight: i + 1, ArrowUp: i - 1, ArrowLeft: i - 1, Home: 0, End: panneaux.length - 1 }[e.key];
    if (j === undefined) return;
    e.preventDefault();
    montrerPanneau(panneaux[(j + panneaux.length) % panneaux.length].def.id, true);
  });
  $("avec-preset").addEventListener("click", () => { regler("souris", "avec_preset", !val("souris", "avec_preset")); majReglages(); });
  montrerPanneau(memoire.lire("panneau", "orbite"));
  majFamilles();
  majAvance();
}

// Ouvre ou referme une famille de la liste des sous-onglets.
function basculerFamille(nom, ouvrir) {
  const ouverte = ouvrir ?? !famillesOuvertes.has(nom);
  if (ouverte) famillesOuvertes.add(nom); else famillesOuvertes.delete(nom);
  memoire.ecrire("familles", [...famillesOuvertes]);
  majFamilles();
}

// Familles : onglets montrés ou cachés (celle du panneau affiché reste ouverte), nombre d'effets
// actifs dans chacune.
function majFamilles() {
  const ici = (panneaux.find((p) => p.def.id === etat.panneau) || {}).def;
  for (const [nom, f] of familles) {
    const ouverte = famillesOuvertes.has(nom) || (ici && ici.famille === nom);
    f.bouton.setAttribute("aria-expanded", ouverte);
    f.signe.textContent = ouverte ? "−" : "+";
    f.liste.hidden = !ouverte;
    const actifs = panneaux.filter((p) => p.def.famille === nom && p.voyant && p.voyant.classList.contains("allume")).length;
    const phrase = actifs ? `${actifs} actif${actifs > 1 ? "s" : ""}` : "";
    f.compte.textContent = actifs ? String(actifs) : "";
    f.compteLu.textContent = phrase ? ", " + phrase : "";
    f.bouton.title = ouverte ? "Replier" : "Déplier" + (phrase ? ` (${phrase})` : "");
  }
}

function construireApplication() {
  for (const def of APPLICATION.maj) construireAvance(def, $("reglages-maj"));
  for (const def of APPLICATION.lignes) construireAvance(def, $("reglages-application"));
}

// Mise à jour : où en est la recherche (`m.etat`, envoyé par l'appli quand il change).
function recevoirMaj(m) {
  const heure = (s) => new Date(s * 1000).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  const prete = m.etat === "prete" && typeof m.version === "string";
  const textes = {
    jamais: "Pas encore de recherche depuis le lancement d'Essaim.",
    recherche: "Recherche en cours…",
    a_jour: `À jour : aucune version plus récente${typeof m.heure === "number" && m.heure > 0 ? ` (vérifié à ${heure(m.heure)})` : ""}.`,
    prete: prete ? `Version ${m.version} téléchargée et vérifiée : elle s'applique au redémarrage.` : "",
    echec: `Recherche impossible : ${String(m.texte || "erreur inconnue")}.`,
  };
  $("maj-etat").textContent = textes[m.etat] || "";
  $("chercher-maj").hidden = prete;
  $("chercher-maj").disabled = m.etat === "recherche";
  $("redemarrer-maj").hidden = !prete;
}

function montrerPanneau(id, focus) {
  if (!panneaux.some((p) => p.def.id === id)) id = panneaux[0].def.id;
  etat.panneau = id;
  memoire.ecrire("panneau", id);
  apercuChange(); // aperçu à l'arrêt : le repère de place suit le panneau
  majFamilles();
  // panneau Écoute : la liste des applis qui ont du son (et de celles qui jouent) est relue
  if (id === "son") envoyer({ cmd: "applis_sonores" });
  for (const p of panneaux) {
    const oui = p.def.id === id;
    p.section.hidden = !oui;
    p.onglet.setAttribute("aria-selected", oui);
    p.onglet.tabIndex = oui ? 0 : -1;
    if (oui && focus) p.onglet.focus();
  }
  $("panneaux").scrollTop = 0;
}

// ---------------------------------------------------------------------------------------------
// Modulations et déclencheurs : une ligne par élément, ses réglages, ses options (« + ») et,
// pour une modulation, la jauge de sa source en direct ; un bouton pour en ajouter, des idées.

const listeMods = () => (etat.reglages ? modsDe(etat.reglages) : []);
const listeDecl = () => (etat.reglages ? declDe(etat.reglages) : []);
// lignes affichées : clé de structure, lignes, options ouvertes (index)
const modUI = { cle: null, lignes: [], ouvertes: new Set() };
const declUI = { cle: null, lignes: [], ouvertes: new Set() };
const quandUI = []; // { actions, zone, cle }

function ecrireModulations(liste) {
  regler("modulations", "liste", liste);
  majReglages();
}
function ecrireDeclencheurs(liste) {
  regler("declencheurs", "liste", liste);
  majReglages();
}

function ajouterModulation(m) {
  const l = listeMods();
  if (l.length >= MODULATIONS_MAX) { bandeau(`${MODULATIONS_MAX} modulations au plus : retires-en une pour en ajouter.`); return; }
  ecrireModulations(l.concat([{ seuil: 0, lissage: LISSAGE_MOD, inverser: false, ...m }]));
  requestAnimationFrame(() => { const d = modUI.lignes[modUI.lignes.length - 1]; if (d) d.premier.focus(); });
}
function ajouterDeclencheur(d) {
  const l = listeDecl();
  if (l.length >= DECLENCHEURS_MAX) { bandeau(`${DECLENCHEURS_MAX} déclencheurs au plus : retires-en un pour en ajouter.`); return; }
  ecrireDeclencheurs(l.concat([{ force: 1, nombre: 3, nombre_max: 3, delai: 0, probabilite: 1, ...d }]));
  requestAnimationFrame(() => { const x = declUI.lignes[declUI.lignes.length - 1]; if (x) x.premier.focus(); });
}

// Liste, bouton « + Ajouter » et idées toutes prêtes.
function blocListe(corps, id, ajout, ajouter, idees, defaut) {
  const bloc = el("div", "modulations");
  const liste = el("div", "liste-modulations");
  liste.id = id;
  const plus = el("button", "puce debut", ajout);
  plus.id = "ajouter-" + id;
  plus.addEventListener("click", () => ajouter(defaut));
  const groupe = el("div", "groupe-puces");
  groupe.setAttribute("role", "group");
  const nom = el("span", "t-label", "Idées");
  nom.id = "idees-" + id;
  groupe.setAttribute("aria-labelledby", nom.id);
  groupe.append(nom, ...idees.map(([libelle, x]) => {
    const b = el("button", "puce", libelle);
    b.addEventListener("click", () => ajouter(x));
    return b;
  }));
  bloc.append(liste, plus, groupe);
  corps.append(bloc);
}

function construireModulations(corps) {
  blocListe(corps, "liste-modulations", "+ Ajouter une modulation", ajouterModulation, SUGGESTIONS_MOD, { source: "basse", cible: "rotation", sur: "tout", quantite: 0.5 });
}
function construireDeclencheurs(corps) {
  blocListe(corps, "liste-declencheurs", "+ Ajouter un déclencheur", ajouterDeclencheur, IDEES_DECLENCHEURS, { quand: "kick", faire: "onde" });
}

// `tous` : liste complète, quand `options` n'en propose qu'une partie (cibles selon « sur »).
function choixMod(options, valeur, libelle, change, tous = options) {
  const s = el("select", "choix");
  s.setAttribute("aria-label", libelle);
  s.replaceChildren(...options.map(([id, nom]) => { const o = el("option", "", nom); o.value = id; return o; }));
  // valeur gardée même si elle n'est pas proposée : connue mais sans effet ici (signalée), ou
  // d'une version plus récente (affichée telle quelle)
  if (!options.some(([id]) => id === valeur)) {
    const connue = tous.find(([id]) => id === valeur);
    const o = el("option", "", connue ? connue[1] + " (sans effet ici)" : String(valeur));
    o.value = valeur;
    s.append(o);
  }
  s.value = valeur;
  s.addEventListener("change", () => change(s.value));
  return s;
}

// Petite glissière avec sa valeur : `lire(x)` donne la valeur du réglage pour la position x,
// `aff(v)` son texte.
function plageMini(libelle, min, max, pas, v, aff, change, vers = (x) => x, depuis = (x) => x) {
  const l = el("label", "option-ligne");
  const plage = el("input");
  Object.assign(plage, { type: "range", min, max, step: pas, value: depuis(v) });
  plage.setAttribute("aria-label", libelle);
  const valeur = el("span", "valeur-mod", aff(v));
  plage.addEventListener("input", () => { const x = vers(+plage.value); valeur.textContent = aff(x); change(x); });
  // à l'écran, le nom du réglage seul (« Déclencheur 2 : force » → « Force »)
  const court = libelle.split(" : ").pop();
  l.append(el("span", "t-label", court.charAt(0).toUpperCase() + court.slice(1)), plage, valeur);
  return { l, plage, valeur };
}

function ligneModulation(m, i) {
  const ligne = el("div", "modulation");
  const autres = (champ, v) => listeMods().map((x, j) => (j === i ? { ...x, [champ]: v } : x));
  const changer = (champ) => (v) => ecrireModulations(autres(champ, v));
  const regle = (champ) => (v) => { regler("modulations", "liste", autres(champ, v)); majNomActif(); };
  const n = i + 1;
  const source = choixMod(SOURCES_MOD, m.source, `Modulation ${n} : source`, changer("source"));
  const cible = choixMod(ciblesPour(m.sur || "tout"), m.cible, `Modulation ${n} : paramètre`, changer("cible"), CIBLES_MOD);
  const sur = choixMod(GROUPES, m.sur || "tout", `Modulation ${n} : sur`, changer("sur"));
  const plage = el("input");
  Object.assign(plage, { type: "range", min: -100, max: 100, step: 1, value: Math.round((m.quantite || 0) * 100) });
  plage.setAttribute("aria-label", `Modulation ${n} : quantité`);
  plage.title = "Double-clic : remise à zéro. À gauche de zéro : dans l'autre sens";
  const valeur = el("span", "valeur-mod", affQuantite(m.cible, m.quantite || 0, m.sur));
  const quantite = (q) => {
    const x = listeMods()[i] || m;
    valeur.textContent = affQuantite(x.cible, q, x.sur);
    regle("quantite")(q);
  };
  plage.addEventListener("input", () => quantite(+plage.value / 100));
  plage.addEventListener("dblclick", () => { plage.value = 0; quantite(0); });
  const jauge = el("span", "jauge-mod");
  jauge.append(el("span"));
  jauge.title = "La source en ce moment, dans l'aperçu";
  jauge.setAttribute("aria-hidden", "true");
  const options = el("div", "options-ligne");
  options.id = "options-mod-" + i;
  options.hidden = !modUI.ouvertes.has(i);
  const pct = (v) => Math.round(v * 100) + " %";
  options.append(
    plageMini(`Modulation ${n} : seuil`, 0, 95, 5, m.seuil || 0, (v) => (v ? pct(v) : "aucun"), regle("seuil"), (x) => x / 100, (v) => v * 100).l,
    plageMini(`Modulation ${n} : lissage`, 0, 100, 5, m.lissage ?? LISSAGE_MOD, (v) => (v ? pct(v) : "aussitôt"), regle("lissage"), (x) => x / 100, (v) => v * 100).l,
  );
  const inverser = el("button", "puce", "À l'envers");
  inverser.setAttribute("aria-pressed", !!m.inverser);
  inverser.title = "La source à l'envers : forte quand elle est faible (stéréo : l'autre côté)";
  inverser.addEventListener("click", () => ecrireModulations(autres("inverser", !m.inverser)));
  options.append(inverser);
  const plus = boutonOptions(options, modUI.ouvertes, i, `Modulation ${n} : seuil, lissage, à l'envers`);
  const retirer = el("button", "puce petite", "Retirer");
  retirer.setAttribute("aria-label", `Retirer la modulation ${n}`);
  retirer.addEventListener("click", () => { modUI.ouvertes.clear(); ecrireModulations(listeMods().filter((_, j) => j !== i)); });
  ligne.append(source, el("span", "fleche", "→"), cible, el("span", "t-label", "sur"), sur, plage, valeur, jauge, plus, retirer, options);
  return { ligne, premier: source, plage, valeur, jauge: jauge.firstChild, source: m.source };
}

// Bouton « + » qui ouvre les options d'une ligne (gardées ouvertes d'un affichage à l'autre).
function boutonOptions(options, ouvertes, i, libelle) {
  const b = el("button", "puce petite plus-options", ouvertes.has(i) ? "−" : "+");
  b.setAttribute("aria-label", libelle);
  b.setAttribute("aria-expanded", ouvertes.has(i));
  b.setAttribute("aria-controls", options.id);
  b.title = libelle;
  b.addEventListener("click", () => {
    const oui = !ouvertes.has(i);
    if (oui) ouvertes.add(i); else ouvertes.delete(i);
    options.hidden = !oui;
    b.textContent = oui ? "−" : "+";
    b.setAttribute("aria-expanded", oui);
  });
  return b;
}

function ligneDeclencheur(d, i) {
  const ligne = el("div", "declencheur");
  const autres = (champ, v) => listeDecl().map((x, j) => (j === i ? { ...x, [champ]: v } : x));
  const changer = (champ) => (v) => ecrireDeclencheurs(autres(champ, v));
  const regle = (champ) => (v) => { regler("declencheurs", "liste", autres(champ, v)); majNomActif(); };
  const n = i + 1;
  const quand = choixMod(EVENEMENTS, d.quand, `Déclencheur ${n} : quand`, changer("quand"));
  const faire = choixMod(ACTIONS, d.faire, `Déclencheur ${n} : action`, changer("faire"));
  const reglages = el("div", "reglages-ligne");
  const genre = ACTION_FORCE[d.faire];
  if (genre === "pourcent") {
    reglages.append(plageMini(`Déclencheur ${n} : force`, 5, 100, 5, Math.min(1, d.force ?? 1), (v) => Math.round(v * 100) + " %", regle("force"), (x) => x / 100, (v) => v * 100).l);
  } else if (genre === "fois") {
    reglages.append(plageMini(`Déclencheur ${n} : taille`, 20, 300, 10, d.force ?? 1, (v) => "× " + virgule(v, 1), regle("force"), (x) => x / 100, (v) => v * 100).l);
  }
  if (ACTION_NOMBRE.has(d.faire)) {
    const lo = d.nombre ?? 3, hi = d.nombre_max ?? lo;
    const nombre = (cle, v, nom) => {
      const s = el("input", "champ nombre");
      Object.assign(s, { type: "number", min: 0, max: 40, step: 1, value: v });
      s.setAttribute("aria-label", `Déclencheur ${n} : ${nom}`);
      s.addEventListener("change", () => changer(cle)(Math.max(0, Math.min(40, Math.round(+s.value || 0)))));
      return s;
    };
    const l = el("span", "option-ligne");
    l.append(el("span", "t-label", "Nombre"), nombre("nombre", lo, "au moins"), el("span", "t-label", "à"), nombre("nombre_max", hi, "au plus"));
    reglages.append(l);
  }
  const options = el("div", "options-ligne");
  options.id = "options-decl-" + i;
  options.hidden = !declUI.ouvertes.has(i);
  options.append(
    plageMini(`Déclencheur ${n} : délai minimal`, 0, 100, 1, d.delai || 0, (v) => (v ? virgule(v, 1) + " s" : "aucun"), regle("delai"), (x) => x / 10, (v) => v * 10).l,
    plageMini(`Déclencheur ${n} : chance`, 5, 100, 5, d.probabilite ?? 1, (v) => Math.round(v * 100) + " %", regle("probabilite"), (x) => x / 100, (v) => v * 100).l,
  );
  const plus = boutonOptions(options, declUI.ouvertes, i, `Déclencheur ${n} : délai minimal, chance`);
  const tester = el("button", "puce petite", "Tester");
  tester.setAttribute("aria-label", `Tester le déclencheur ${n} dans l'aperçu`);
  tester.title = d.faire === "bordure" ? "La bordure s'illumine sur la vraie fenêtre, pas dans l'aperçu" : "Le joue tout de suite dans le grand aperçu";
  tester.disabled = d.faire === "bordure";
  tester.addEventListener("click", () => grand.tester(listeDecl()[i] || d));
  const retirer = el("button", "puce petite", "Retirer");
  retirer.setAttribute("aria-label", `Retirer le déclencheur ${n}`);
  retirer.addEventListener("click", () => { declUI.ouvertes.clear(); ecrireDeclencheurs(listeDecl().filter((_, j) => j !== i)); });
  const jauge = jaugeImpact("action." + d.faire);
  ligne.append(quand, el("span", "fleche", "→"), faire, reglages, jauge, tester, plus, retirer, options);
  return { ligne, premier: quand };
}

// Lignes d'une liste : refaites quand sa structure change, sinon laissées (une glissière en
// cours de réglage garde son focus).
function majListe(ui, id, liste, cle, ligne, vide, max) {
  const zone = $(id);
  if (!zone) return;
  if (cle !== ui.cle) {
    ui.cle = cle;
    ui.lignes = liste.map(ligne);
    remplacer(zone, ui.lignes.length ? ui.lignes.map((d) => d.ligne) : [el("p", "note vide-mod", vide)]);
  }
  const plus = $("ajouter-" + id);
  if (plus) plus.disabled = liste.length >= max;
}

function majModulations() {
  if (!etat.reglages) return;
  const l = listeMods();
  majListe(modUI, "liste-modulations", l, JSON.stringify(l.map((m) => [m.source, m.cible, m.sur, !!m.inverser])), ligneModulation,
    "Aucune modulation : ajoute-en une, ou pars d'une idée ci-dessous.", MODULATIONS_MAX);
  l.forEach((m, i) => {
    const d = modUI.lignes[i];
    if (d && document.activeElement !== d.plage) {
      d.plage.value = Math.round((m.quantite || 0) * 100);
      d.valeur.textContent = affQuantite(m.cible, m.quantite || 0, m.sur);
    }
  });
  const ld = listeDecl();
  majListe(declUI, "liste-declencheurs", ld, JSON.stringify(ld.map((d) => [d.quand, d.faire, d.nombre, d.nombre_max])), ligneDeclencheur,
    "Aucun déclencheur : l'essaim ne réagit plus aux événements. Ajoute-en un, ou pars d'une idée ci-dessous.", DECLENCHEURS_MAX);
  for (const q of quandUI) majQuand(q, ld);
  for (const p of panneaux) {
    if (p.def.id === "modulations" && p.voyant) p.voyant.textContent = l.length ? String(l.length) : "";
    if (p.def.id === "declencheurs" && p.voyant) p.voyant.textContent = ld.length ? String(ld.length) : "";
  }
}

// Déclencheurs d'un effet (ligne « Quand » de son panneau) : un bouton par déclencheur (le
// retirer), un choix pour en ajouter un, un lien vers tous les déclencheurs.
function construireQuand(def, contenu) {
  const zone = el("span", "quand");
  contenu.push(zone);
  const q = { actions: def.quand, zone, cle: null };
  quandUI.push(q);
  return q;
}
function majQuand(q, liste) {
  const les = liste.map((d, i) => [d, i]).filter(([d]) => q.actions.includes(d.faire));
  const cle = JSON.stringify(les.map(([d, i]) => [d.quand, d.faire, i]));
  if (cle === q.cle) return;
  q.cle = cle;
  const seul = q.actions.length === 1;
  const puces = les.map(([d, i]) => {
    const b = el("button", "puce evenement", (seul ? NOMS_EVENEMENTS[d.quand] : `${NOMS_EVENEMENTS[d.quand]} → ${NOMS_ACTIONS[d.faire]}`) || d.quand);
    b.append(el("span", "croix", "×"));
    b.title = "Retirer";
    b.setAttribute("aria-label", `Retirer : ${NOMS_EVENEMENTS[d.quand] || d.quand} → ${NOMS_ACTIONS[d.faire] || d.faire}`);
    b.addEventListener("click", () => ecrireDeclencheurs(listeDecl().filter((_, j) => j !== i)));
    return b;
  });
  const ajout = el("select", "choix petit");
  ajout.setAttribute("aria-label", "Ajouter un événement");
  ajout.append(Object.assign(el("option", "", seul ? "+ Ajouter quand…" : `+ ${NOMS_ACTIONS[q.actions[0]]} quand…`), { value: "" }));
  for (const [id, nom] of EVENEMENTS) ajout.append(Object.assign(el("option", "", nom), { value: id }));
  ajout.addEventListener("change", () => { if (ajout.value) ajouterDeclencheur({ quand: ajout.value, faire: q.actions[0] }); });
  const tous = el("button", "puce petite", "Tous les déclencheurs →");
  tous.addEventListener("click", () => montrerPanneau("declencheurs", true));
  remplacer(q.zone, [...(puces.length ? puces : [el("span", "note", "jamais")]), ajout, tous]);
}

// Jauges des sources, à chaque image du grand aperçu (qui suit le vrai son, ou la démo).
function majJaugesModulations() {
  if (etat.panneau !== "modulations") return;
  for (const d of modUI.lignes) {
    const v = grand.valeurSource(d.source);
    // stéréo : de -1 à 1, depuis le milieu
    if (d.source === "stereo") {
      d.jauge.style.left = 50 + Math.min(0, v) * 50 + "%";
      d.jauge.style.width = Math.abs(v) * 50 + "%";
    } else {
      d.jauge.style.left = "0";
      d.jauge.style.width = Math.max(0, Math.min(1, v)) * 100 + "%";
    }
  }
}

// Sortie son écoutée (`[audio] sortie`, propre à cet ordinateur) : liste demandée à l'appli à la
// construction et à chaque ouverture de la liste (un casque a pu être branché entre-temps).
function rangeeSorties() {
  const l = el("label", "ligne-avance");
  const tete = el("span", "tete");
  tete.append(el("span", "t-label", "Sortie son écoutée"));
  const choix = el("select", "choix");
  choix.id = "sortie-son";
  const note = el("span", "note", "Propre à cet ordinateur (hors presets). Si la sortie choisie est débranchée, Essaim écoute la sortie par défaut en attendant son retour.");
  note.id = "note-sortie-son";
  decrire(choix, note.id);
  choix.addEventListener("focus", () => envoyer({ cmd: "sorties" }));
  choix.addEventListener("change", () => regler("audio", "sortie", choix.value));
  l.append(tete, choix, note);
  envoyer({ cmd: "sorties" });
  majSorties(choix);
  return l;
}

// Applications écoutées (`[audio] applis`, propre à cet ordinateur) : une puce par appli qui a du
// son ouvert, plus celles déjà choisies ; aucune cochée = tout le son. Liste redemandée à
// l'ouverture du panneau et par le bouton « Actualiser ».
function rangeeApplisSon() {
  const l = el("div", "ligne-avance");
  const groupe = el("div", "groupe-puces");
  groupe.id = "applis-son";
  groupe.setAttribute("role", "group");
  const nom = el("span", "t-label", "Applications écoutées");
  nom.id = "nom-applis-son";
  groupe.setAttribute("aria-labelledby", nom.id);
  const actualiser = el("button", "puce", "Actualiser");
  actualiser.type = "button";
  actualiser.addEventListener("click", () => envoyer({ cmd: "applis_sonores" }));
  const note = el("p", "note", "Coche une ou plusieurs applications pour n'écouter que leur son (ta musique, sans les notifications ni les vidéos d'à côté) ; celles qui jouent en ce moment sont en tête, marquées « en lecture ». Aucune cochée : tout le son de la sortie. Propre à cet ordinateur (hors presets) ; il faut Windows 10 2004 ou plus récent, sinon tout le son est écouté.");
  note.id = "note-applis-son";
  decrire(groupe, note.id);
  l.append(nom, groupe, actualiser, note);
  envoyer({ cmd: "applis_sonores" });
  majApplisSon();
  return l;
}

function majApplisSon() {
  const groupe = $("applis-son");
  if (!groupe || !etat.reglages) return;
  const choisies = val("audio", "applis") || [];
  // celles qui jouent en ce moment d'abord, marquées
  const liste = (etat.applisSon || []).map((a) => [a.exe, a.nom, !!a.joue]).sort((a, b) => b[2] - a[2]);
  for (const c of choisies) if (!liste.some(([exe]) => nomExe(exe) === nomExe(c))) liste.push([c, c.replace(/\.exe$/i, ""), false]);
  const cle = JSON.stringify([liste, choisies]);
  if (groupe.dataset.cle === cle) return;
  groupe.dataset.cle = cle;
  if (!liste.length) { remplacer(groupe, [videApplisSon]); return; }
  // un bouton par application, gardé d'une fois à l'autre (le focus clavier reste sur celui qu'on
  // vient de cocher) ; seuls son texte et son état changent
  remplacer(groupe, liste.map(([exe, nom, joue]) => {
    const id = nomExe(exe);
    let b = boutonsApplisSon.get(id);
    if (!b) {
      b = el("button", "puce");
      b.type = "button";
      b.dataset.cle = id;
      b.addEventListener("click", () => {
        const actuelles = val("audio", "applis") || [];
        const deja = actuelles.some((c) => nomExe(c) === id);
        regler("audio", "applis", deja ? actuelles.filter((c) => nomExe(c) !== id) : actuelles.concat([b.dataset.exe]));
        majApplisSon();
      });
      boutonsApplisSon.set(id, b);
    }
    b.dataset.exe = exe;
    const contenu = [nom];
    if (joue) {
      // « ▶ » décoratif : les lecteurs d'écran lisent « en lecture »
      const marque = el("span", "joue", " ");
      const fleche = el("span", "", "▶");
      fleche.setAttribute("aria-hidden", "true");
      marque.append(fleche, " en lecture");
      contenu.push(marque);
    }
    b.replaceChildren(...contenu);
    b.title = exe + (joue ? " : joue en ce moment" : "");
    return attributs(b, { "aria-pressed": choisies.some((c) => nomExe(c) === id) });
  }));
}
const boutonsApplisSon = new Map(); // nom de l'exe (minuscules) → bouton
const videApplisSon = el("span", "note", "Aucune application n'a de son ouvert en ce moment.");

function majSorties(choix = $("sortie-son")) {
  if (!choix || !etat.reglages) return;
  const voulue = val("audio", "sortie") || "";
  const options = [["", "Sortie par défaut de Windows"], ...(etat.sorties || []).map((s) => [s.id, s.nom])];
  // sortie choisie absente (débranchée) : gardée dans la liste, signalée
  if (voulue && !options.some(([id]) => id === voulue)) options.push([voulue, "Sortie choisie, absente (sortie par défaut en attendant)"]);
  const cle = JSON.stringify(options);
  if (choix.dataset.options !== cle) {
    choix.dataset.options = cle;
    choix.replaceChildren(...options.map(([id, nom]) => { const o = el("option", "", nom); o.value = id; return o; }));
  }
  if (choix.value !== voulue) choix.value = voulue;
}

function majAvance() {
  for (const a of avances) {
    a.zone.hidden = !etat.avance;
    a.signe.textContent = etat.avance ? "−" : "+";
    a.bouton.setAttribute("aria-expanded", etat.avance);
  }
}

function majNomActif() {
  const p = etat.presets.find((x) => x.nom === etat.actif);
  if (p && !enCours()) $("nom-actif").textContent = `Preset « ${p.nom} »`;
  else if (etat.proche && !enCours()) $("nom-actif").textContent = `« ${etat.proche.nom} » modifié (${etat.proche.ecarts} réglage${etat.proche.ecarts > 1 ? "s" : ""})`;
  else $("nom-actif").textContent = "Réglages personnalisés";
}

function majReglages() {
  if (!etat.reglages) return;
  for (const { def, input, saisie } of glissieres) {
    const v = val(def.section, def.cle);
    if (v === undefined) continue; // réglage que l'appli ne fournit pas (encore)
    if (document.activeElement !== input) input.value = v;
    if (document.activeElement !== saisie) saisie.value = def.aff(v);
  }
  for (const c of conditionnels) {
    const cache = !!c.cache && c.cache(etat.reglages), estompe = !!c.si && !c.si(etat.reglages);
    for (const e of c.els) { e.hidden = cache; e.classList.toggle("estompe", estompe); }
  }
  // panneau d'un effet coupé : estompé (il reste réglable), voyant de son onglet éteint
  // (éteint : effet coupé par sa bascule, ou sans ce dont il a besoin, `si`)
  for (const p of panneaux) {
    const r = etat.reglages;
    if (p.def.voyant) p.voyant.classList.toggle("allume", !!p.def.voyant(r));
    const actif = !p.def.actif || (p.def.present ? !!p.def.present(r) : !!val(p.def.actif[0], p.def.actif[1]));
    if (p.def.actif) p.voyant.classList.toggle("allume", actif);
    p.corps.classList.toggle("eteint", !actif || (!!p.def.si && !p.def.si(r)));
  }
  majFamilles();
  majModulations();
  for (const b of bascules) {
    const v = val(b.section, b.cle);
    b.bouton.setAttribute("aria-pressed", typeof v === "boolean" ? v : v > 0);
  }
  for (const c of choix) c.bouton.setAttribute("aria-pressed", val(c.section, c.cle) === c.valeur);
  for (const f of formes) {
    f.b.setAttribute("aria-pressed", (val(f.section, f.cle) || "cometes") === f.id);
    // le pictogramme d'une image ou de l'icône suit ses contours
    const source = f.id === "trace" ? etat.reglages.orbite.contours || etat.reglages.orbite.trace : f.id === "icone" ? window.contoursIcone : null;
    if (source && f.source !== source) {
      f.source = source;
      f.b.querySelector("svg").replaceWith(picto(f.id, etat.reglages.orbite));
    }
  }
  const c = etat.reglages.couleur;
  // sans Prisme, sa synchro ne change rien à la teinte
  const prisme = etat.prisme === true;
  if (boutonPrisme) boutonPrisme.hidden = !prisme;
  for (const t of teintes) {
    t.b.setAttribute("aria-pressed", !c.application && !c.site && !c.pochette && !(prisme && c.synchro_prisme) && c.vitesse_teinte === 0 && Math.abs(c.teinte - t.h) < 3);
  }
  $("avec-preset").setAttribute("aria-pressed", !!val("souris", "avec_preset"));
  const d = $("demarrage");
  if (d) d.setAttribute("aria-pressed", etat.demarrage);
  majSorties();
  majNomActif();
}

// « Surprends-moi » : des réglages tirés au hasard, dans des bornes qui donnent un essaim lisible.
function surprise() {
  const r = (a, b) => a + Math.random() * (b - a);
  const ri = (a, b) => Math.floor(r(a, b + 1));
  const parmi = (l) => l[Math.floor(Math.random() * l.length)];
  const c = JSON.parse(JSON.stringify(etat.reglages));
  Object.assign(c.orbite, {
    forme: parmi(FORMES)[0], cometes: Math.random() > 0.15, nombre: ri(1, 6), anneaux: Math.random() < 0.7 ? 1 : ri(2, 3),
    rayon: ri(14, 38), vitesse_base: +r(1.2, 6).toFixed(1), rotation: Math.random() < 0.6 ? 0 : ri(-25, 25), trainee: +r(0.15, 0.7).toFixed(2),
  });
  Object.assign(c.essaim, {
    maximum: ri(0, 30), fixes: c.orbite.cometes ? ri(0, 6) : ri(6, 18), forme: Math.random() < 0.75 ? "cometes" : parmi(FORMES)[0],
    constellation: Math.random() < 0.25,
  });
  Object.assign(c.couleur, {
    teinte: ri(0, 359), vitesse_teinte: Math.random() < 0.5 ? 0 : ri(20, 120), ecart_teinte: ri(20, 200),
    application: false, site: false, pochette: false, synchro_prisme: false,
    palette: Math.random() < 0.6 ? 1 : ri(2, 3), ecart_palette: parmi([60, 90, 120, 180]),
  });
  c.eclat.taille = +r(1.5, 6).toFixed(1);
  c.orbite.rotation_sur = parmi(["tout", "tout", "cometes", "satellites"]);
  // de temps en temps, des rubans ou des ressorts (si l'appli les connaît)
  if (c.rubans) Object.assign(c.rubans, { actif: Math.random() < 0.15, largeur: ri(3, 7), vrille: +r(0.4, 1).toFixed(2) });
  if (c.ressorts) c.ressorts.actif = Math.random() < 0.25;
  // réactions de toujours, plus ou moins fortes, et une fois sur trois une idée en plus pour les
  // modulations et pour les déclencheurs
  const avant = reactionsAvant({ orbite: { vitesse_basse: +r(4, 20).toFixed(1), eloignement_grave: +r(0.4, 2).toFixed(1) }, musique: { contretemps: Math.random() < 0.25 } });
  const idee = (l) => (Math.random() < 0.35 ? [{ seuil: 0, lissage: LISSAGE_MOD, inverser: false, force: 1, nombre: 3, nombre_max: 3, delai: 0, probabilite: 1, ...parmi(l)[1] }] : []);
  c.modulations = { liste: avant.basse.concat(avant.mods, idee(SUGGESTIONS_MOD.slice(3))).map(({ force, nombre, nombre_max, delai, probabilite, ...m }) => m) };
  // une fois sur trois, une ou deux figures (jamais plus : l'essaim reste discret), d'allure tirée
  // dans des bornes sages, et parfois une modulation sur l'une d'elles ; les autres sont coupées
  const figures = FIGURES_REGLAGES.map(([s]) => s).filter((s) => c[s]);
  for (const s of figures) c[s].actif = false;
  if (figures.length && Math.random() < 0.35) {
    const tirees = [];
    const n = Math.min(figures.length, Math.random() < 0.6 ? 1 : 2);
    while (tirees.length < n) { const s = parmi(figures); if (!tirees.includes(s)) tirees.push(s); }
    const reaction = () => +r(0.5, 1.5).toFixed(2);
    for (const s of tirees) {
      Object.assign(c[s], {
        actif: true, opacite: +r(0.7, 1).toFixed(2), teinte: 5 * ri(-36, 36),
        reaction_kick: reaction(), reaction_caisse: reaction(), reaction_charleston: reaction(),
        reaction_basse: reaction(), reaction_montee: reaction(), reaction_drop: reaction(),
      });
      // le trait et le pendule ne grandissent, ne tournent ni ne se déplacent
      if (s !== "pinceau" && s !== "pendule") {
        Object.assign(c[s], { echelle: +r(0.8, 1.3).toFixed(2), rotation: Math.random() < 0.3 ? +(r(3, 10) * parmi([-1, 1])).toFixed(1) : 0, decalage_x: 0, decalage_y: 0 });
      }
    }
    if (Math.random() < 0.4 && c.modulations.liste.length < MODULATIONS_MAX) {
      const sur = parmi(tirees), geo = sur !== "pinceau" && sur !== "pendule";
      const [cible, q] = parmi([["taille", [0.2, 0.5]], ["teinte", [0.2, 0.5]], ...(geo ? [["rayon", [0.05, 0.12]], ["rotation", [0.05, 0.12]]] : [])]);
      c.modulations.liste.push({ source: parmi(["basse", "kick", "caisse", "charleston", "montee"]), cible, sur, quantite: +r(q[0], q[1]).toFixed(2), seuil: 0, lissage: LISSAGE_MOD, inverser: false });
    }
  }
  c.declencheurs = { liste: avant.decl.concat(idee(IDEES_DECLENCHEURS).map(({ seuil, lissage, inverser, ...d }) => d)) };
  envoyer({ cmd: "appliquer_reglages", nom: "Surprise", reglages: c });
}

// En-tête : écoute du son (active, coupée, en erreur) et veille, sous le bouton Pause (la pause,
// elle, se lit sur le bouton). Rien n'est montré tant que l'appli n'envoie pas ces champs.
function majEtatEssaim() {
  const e = $("etat-essaim");
  // une ligne par état, courte : l'en-tête n'a pas de place en largeur
  const parts = [];
  if (etat.capture) parts.push({ active: "Son capté", coupee: "Son coupé", erreur: "Son : erreur" }[etat.capture]);
  if (etat.veille) parts.push("En veille");
  if (etat.raison) parts.push(etat.raison);
  // morceau en cours : son titre, tronqué ; titre et artiste en entier au survol
  const m = etat.morceau;
  const titre = m ? m.titre || m.artiste : "";
  e.hidden = !parts.length && !titre;
  const cle = parts.join("|") + "|" + (m ? m.titre + "|" + m.artiste : "");
  if (e.dataset.texte !== cle) {
    e.dataset.texte = cle;
    const spans = parts.map((t) => el("span", "", t));
    if (titre) {
      const s = el("span", "morceau", titre);
      s.title = m.titre && m.artiste ? `${m.titre} — ${m.artiste}` : titre;
      spans.push(s);
    }
    e.replaceChildren(...spans);
  }
  e.dataset.etat = etat.capture || "";
  e.title = { active: "Essaim entend le son de Windows.", coupee: "Écoute coupée : « Réagir au son » est désactivé.", erreur: "La capture du son de Windows a échoué (voir le journal)." }[etat.capture] || "";
}

// Puissance : temps de calcul par image × images par seconde = part d'un cœur du processeur.
function recevoirCharge(m) {
  etat.pause = !!m.pause;
  // champs ajoutés plus tard par l'appli : ignorés s'ils manquent
  etat.capture = ["active", "coupee", "erreur"].includes(m.capture) ? m.capture : null;
  etat.veille = m.veille === true;
  const mo = m.morceau;
  etat.morceau = mo && typeof mo === "object" ? { titre: String(mo.titre || ""), artiste: String(mo.artiste || "") } : null;
  etat.lecture = typeof m.lecture === "boolean" ? m.lecture : null;
  // changement de piste vu par l'appli : la même rafale dans le grand aperçu
  if (typeof m.pistes === "number") {
    if (etat.pistes !== null && m.pistes !== etat.pistes && etat.reglages) grand.evenement("piste");
    etat.pistes = m.pistes;
  }
  etat.raison = typeof m.raison === "string" ? m.raison.slice(0, 60) : "";
  // Prisme là ou non (absent d'une appli qui ne le signale pas : la synchro reste cachée)
  if (typeof m.prisme === "boolean" && m.prisme !== etat.prisme) {
    etat.prisme = m.prisme;
    majReglages();
  }
  $("pause").setAttribute("aria-pressed", etat.pause);
  $("pause").textContent = etat.pause ? "En pause" : "Pause";
  majEtatEssaim();
  const part = (m.ms * m.ips) / 1000;
  const niveau = m.ips < 1 ? "repos" : part < 0.03 ? "faible" : part < 0.1 ? "moyenne" : "eleve";
  $("puissance").dataset.niveau = niveau;
  $("puissance-niveau").textContent = { repos: "à l'arrêt", faible: "faible", moyenne: "moyenne", eleve: "élevée" }[niveau];
  $("puissance-barre").style.width = Math.round(borne(part / 0.15, 0, 1) * 100) + "%";
  $("puissance-detail").textContent = m.ips < 1
    ? (etat.pause ? "Essaim est en pause." : etat.raison && !/i\/s$/.test(etat.raison) ? `Essaim est en pause automatique (${etat.raison}).`
      : etat.veille ? "Essaim est en veille : rien n'est dessiné." : "Rien à dessiner en ce moment (plein écran ou curseur masqué).")
    : `${virgule(m.ms, 2)} ms de calcul par image × ${Math.round(m.ips)} images/s : ${virgule(part * 100, 1)} % d'un cœur du processeur, mesuré en direct.` +
      (niveau === "eleve" ? " Les cases à côté de chaque réglage montrent ce qui pèse le plus." : "");
}

// ---------------------------------------------------------------------------------------------
// Curseurs : chaque image est une bande d'images côte à côte (une seule s'il est fixe), animée
// selon les durées du fichier .ani.

const animes = new Set();
const apercusJeux = new Map(); // clé du jeu → [4 images] ; null = demandé
const imagesCanvas = new Map(); // png → HTMLImageElement, pour dessiner dans les aperçus

function creerAnim(image, taille) {
  const d = el("span", "anim");
  d.style.width = d.style.height = taille + "px";
  d.style.backgroundImage = `url("${image.png}")`;
  d.style.backgroundSize = `${image.images * taille}px ${taille}px`;
  if (image.images > 1) animes.add({ d, image, taille });
  return d;
}

// Les animations dont l'élément a quitté la page (cartes reconstruites) sont oubliées.
function purgerAnimes() {
  for (const a of animes) if (!a.d.isConnected) animes.delete(a);
}

function animer(maintenant) {
  for (const a of animes) {
    if (!a.d.isConnected) { animes.delete(a); continue; }
    a.d.style.backgroundPosition = `${-window.imageA(a.image, maintenant) * a.taille}px 0`;
  }
}

// Les quatre curseurs d'un jeu, demandés une fois ; `null` tant qu'ils ne sont pas arrivés.
function apercuJeu(cle) {
  if (!cle) return null;
  if (!apercusJeux.has(cle)) {
    apercusJeux.set(cle, null);
    envoyer({ cmd: "apercu_jeu", cle });
  }
  return apercusJeux.get(cle);
}

// Flèche du jeu, prête à dessiner dans un canevas (image chargée), ou rien.
function flecheJeu(cle) {
  const a = apercuJeu(cle);
  const f = a && a[0];
  if (!f) return null;
  let img = imagesCanvas.get(f.png);
  if (!img) { img = new Image(); img.src = f.png; imagesCanvas.set(f.png, img); }
  return img.complete ? { img, ...f } : null;
}

const observateurJeux = new IntersectionObserver((entrees) => {
  for (const e of entrees) if (e.isIntersecting) { apercuJeu(e.target.dataset.cle); observateurJeux.unobserve(e.target); }
}, { rootMargin: "200px" });

function remplirCarte(b, cle) {
  const zone = b.querySelector(".apercu-jeu");
  const a = apercusJeux.get(cle);
  if (!a || zone.dataset.pret) return;
  zone.dataset.pret = "1";
  zone.replaceChildren(...a.map((i) => (i ? creerAnim(i, 30) : el("span", "anim vide-role", "—"))));
}

function dessinerCurseurs() {
  const zone = $("curseurs");
  if (!etat.jeux) return;
  const actuel = etat.jeux.find((j) => j.cle === etat.curseurs);
  $("jeu-actuel").textContent = actuel ? actuel.nom : "Curseurs de Windows (non nommés)";
  $("compte-jeux").textContent = etat.jeux.length + " jeux";
  // détail du jeu en place : demandé une fois par jeu
  if (actuel && etat.detailDemande !== actuel.cle) {
    etat.detailDemande = actuel.cle;
    envoyer({ cmd: "detail_jeu", cle: actuel.cle });
  } else if (!actuel) {
    etat.detailDemande = null;
    $("roles").replaceChildren(el("p", "note", "Le jeu en place n'a pas de nom (curseurs par défaut ou choisis un par un) : choisis-en un à droite."));
    purgerAnimes();
  }
  if (!etat.jeux.length) {
    zone.replaceChildren(el("span", "t-petit", "Aucun jeu de curseurs trouvé dans Windows."));
    delete zone.dataset.liste;
    purgerAnimes();
    return;
  }
  // les cartes ne sont refaites que si la liste des jeux change (chaque message « etat » passe
  // par ici) : sinon seul le jeu en place change de cadre
  const liste = JSON.stringify(etat.jeux.map((j) => [j.cle, j.nom, j.systeme]));
  if (zone.dataset.liste === liste) {
    for (const b of zone.querySelectorAll(".jeu")) b.setAttribute("aria-pressed", b.dataset.cle === etat.curseurs);
    return;
  }
  zone.dataset.liste = liste;
  remplacer(zone, etat.jeux.map((j) => {
    const b = el("button", "jeu");
    b.dataset.cle = j.cle;
    b.setAttribute("aria-pressed", j.cle === etat.curseurs);
    b.append(el("span", "apercu-jeu"), el("span", "t-petit", j.nom), el("span", "t-label", j.systeme ? "Windows" : "installé"));
    b.addEventListener("click", () => envoyer({ cmd: "souris", jeu: j.cle }));
    remplirCarte(b, j.cle);
    if (!b.querySelector(".apercu-jeu").dataset.pret) observateurJeux.observe(b);
    return b;
  }));
  purgerAnimes();
}

function recevoirApercuJeu(m) {
  apercusJeux.set(m.cle, m.curseurs);
  // les vignettes déjà dessinées (mode figé) le sont de nouveau, avec le curseur
  for (const t of tuiles.values()) t.pret = false;
  for (const b of document.querySelectorAll(".jeu")) if (b.dataset.cle === m.cle) remplirCarte(b, m.cle);
}

function dessinerDetail(m) {
  if (m.cle !== etat.curseurs) return;
  $("roles").replaceChildren(...m.curseurs.map((c) => {
    const d = el("div", "role");
    d.append(c.image ? creerAnim(c.image, 40) : el("span", "vide-role", "—"), el("span", "", c.nom));
    if (c.image && c.image.images > 1) d.title = `Animé : ${c.image.images} images`;
    return d;
  }));
  purgerAnimes();
}

// ---------------------------------------------------------------------------------------------
// Automatique : règles par application et par heure, raccourcis

function remplirChoixPresets(id) {
  const s = $(id), avant = s.value;
  s.replaceChildren(...etat.presets.map((p) => { const o = el("option", "", p.nom); o.value = p.nom; return o; }));
  if (etat.presets.some((p) => p.nom === avant)) s.value = avant;
}

function ligneRegle(gauche, droite, retirer) {
  const d = el("div", "regle");
  const introuvable = !etat.presets.some((p) => p.nom === droite);
  d.append(el("span", "t-petit", gauche), el("span", "fleche", "→"), el("span", "t-petit", droite + (introuvable ? " (preset introuvable)" : "")));
  const b = el("button", "puce petite", "Retirer");
  b.addEventListener("click", retirer);
  d.append(b);
  return d;
}

function dessinerAuto() {
  if (!etat.reglages) return;
  const a = etat.reglages.automatique;
  $("auto-actif").setAttribute("aria-pressed", a.actif);
  $("raccourcis-actif").setAttribute("aria-pressed", etat.reglages.raccourcis.actif);
  remplirChoixPresets("appli-preset");
  remplirChoixPresets("horaire-preset");
  remplirChoixPresets("si-preset");
  const vide = (texte) => el("div", "regle", "").appendChild(el("span", "vide-regle t-petit", texte)).parentNode;
  // règles « si … alors … » (absentes d'une appli plus ancienne)
  const regles = a.regles || [];
  $("regles-si").replaceChildren(...(regles.length ? regles.map((r, i) => ligneRegle("si " + r.si, r.alors, () => {
    regler("automatique", "regles", regles.filter((_, j) => j !== i));
    dessinerAuto();
  })) : [vide("Aucune règle : par exemple « artiste = Skrillex » → un preset.")]));
  // raccourci : une règle pour l'artiste qui joue
  const artiste = etat.morceau && etat.morceau.artiste;
  $("si-artiste").hidden = !artiste;
  if (artiste) $("si-artiste").textContent = `Artiste en cours : ${artiste.length > 24 ? artiste.slice(0, 23) + "…" : artiste}`;
  $("regles-applis").replaceChildren(...(a.applis.length ? a.applis.map((r, i) => ligneRegle(r.appli, r.preset, () => {
    regler("automatique", "applis", a.applis.filter((_, j) => j !== i));
    dessinerAuto();
  })) : [vide("Aucune règle : ajoute une application ci-dessous.")]));
  $("regles-horaires").replaceChildren(...(a.horaires.length ? a.horaires.map((r, i) => ligneRegle(`${r.debut} à ${r.fin}`, r.preset, () => {
    regler("automatique", "horaires", a.horaires.filter((_, j) => j !== i));
    dessinerAuto();
  })) : [vide("Aucune plage horaire.")]));
  $("applis-ouvertes").replaceChildren(...etat.applis.map((n) => { const o = el("option"); o.value = n; return o; }));
  dessinerPausesApplis(vide);
}

// Applications qui mettent Essaim en pause au premier plan (`[pause] applis`, hors presets).
const nomExe = (texte) => { const n = texte.trim().toLowerCase(); return n && !n.endsWith(".exe") ? n + ".exe" : n; };
function dessinerPausesApplis(vide) {
  const p = etat.reglages.pause;
  $("bloc-pauses").hidden = !p; // réglage que l'appli ne fournit pas (encore)
  if (!p) return;
  $("pauses-applis").replaceChildren(...(p.applis.length ? p.applis.map((a, i) => {
    const d = el("div", "regle simple");
    const b = el("button", "puce petite", "Retirer");
    b.setAttribute("aria-label", "Retirer " + a);
    b.addEventListener("click", () => { regler("pause", "applis", p.applis.filter((_, j) => j !== i)); dessinerAuto(); });
    d.append(el("span", "t-petit", a), b);
    return d;
  }) : [vide("Aucune application : Essaim ne se met pas en pause selon l'application.")]));
  const r = $("pause-recente");
  r.hidden = !etat.recente || p.applis.some((a) => nomExe(a) === nomExe(etat.recente));
  r.textContent = `Ajouter « ${etat.recente} »`;
  r.title = "La dernière application que tu as utilisée avant d'ouvrir cette fenêtre";
}
function ajouterPauseAppli(texte) {
  const appli = nomExe(texte);
  if (!appli || appli === ".exe") { bandeau("Indique le nom d'une application (obs64.exe)."); return; }
  const liste = etat.reglages.pause.applis;
  if (!liste.some((a) => nomExe(a) === appli)) regler("pause", "applis", liste.concat([appli]));
  dessinerAuto();
}
$("ajout-pause").addEventListener("submit", (e) => {
  e.preventDefault();
  ajouterPauseAppli($("pause-nom").value);
  $("pause-nom").value = "";
});
$("pause-recente").addEventListener("click", () => ajouterPauseAppli(etat.recente));

$("auto-actif").addEventListener("click", () => { regler("automatique", "actif", !etat.reglages.automatique.actif); dessinerAuto(); });
$("raccourcis-actif").addEventListener("click", () => { regler("raccourcis", "actif", !etat.reglages.raccourcis.actif); dessinerAuto(); });
$("ajout-appli").addEventListener("submit", (e) => {
  e.preventDefault();
  let appli = $("appli-nom").value.trim().toLowerCase();
  const preset = $("appli-preset").value;
  if (!appli || !preset) { bandeau("Indique une application et un preset."); return; }
  if (!appli.endsWith(".exe")) appli += ".exe";
  const liste = etat.reglages.automatique.applis.filter((r) => r.appli !== appli).concat([{ appli, preset }]);
  regler("automatique", "applis", liste);
  $("appli-nom").value = "";
  dessinerAuto();
});
// Condition d'une règle : chaque partie (séparée par « et », hors guillemets) est « clé
// opérateur valeur » ; l'appli refait la vérification complète (une règle illisible est ignorée
// et notée au journal). Renvoie un message d'erreur, ou "" si elle se lit.
function verifierCondition(si) {
  const parties = si.split(/ et (?=(?:[^"]*"[^"]*")*[^"]*$)/i);
  for (const p of parties) {
    const m = p.trim().match(/^(artiste|artist|titre|morceau|title|appli|application|app|bpm|tempo|heure)\s*(<=|>=|!=|=|~|<|>)\s*(.+)$/i);
    if (!m) return `« ${p.trim()} » : écris « clé opérateur valeur », par exemple « artiste = Skrillex ».`;
    const cle = m[1].toLowerCase(), op = m[2], v = m[3].trim();
    const nombre = cle === "bpm" || cle === "tempo", heure = cle === "heure";
    if (nombre && !/^\d+([.,]\d+)?$/.test(v)) return `« ${v} » : un tempo s'écrit en nombre (bpm > 140).`;
    if (heure && !/^([01]?\d|2[0-3]):[0-5]\d$/.test(v)) return `« ${v} » : une heure s'écrit HH:MM (heure >= 22:00).`;
    if ((nombre || heure) && op === "~") return "« ~ » (contient) ne va qu'avec un texte.";
    if (!nombre && !heure && /[<>]/.test(op)) return "« < » et « > » ne vont qu'avec bpm ou heure.";
  }
  return "";
}
$("ajout-si").addEventListener("submit", (e) => {
  e.preventDefault();
  const si = $("si-condition").value.trim(), alors = $("si-preset").value;
  if (!si || !alors) { bandeau("Indique une condition et un preset."); return; }
  const erreur = verifierCondition(si);
  if (erreur) { bandeau(erreur); return; }
  regler("automatique", "regles", (etat.reglages.automatique.regles || []).concat([{ si, alors }]));
  $("si-condition").value = "";
  dessinerAuto();
});
$("si-artiste").addEventListener("click", () => {
  const a = etat.morceau && etat.morceau.artiste;
  if (a) $("si-condition").value = `artiste = "${a.split(/[,&;/]| feat\. | ft\. /)[0].trim()}"`;
  $("si-condition").focus();
});
$("ajout-horaire").addEventListener("submit", (e) => {
  e.preventDefault();
  const debut = $("horaire-debut").value, fin = $("horaire-fin").value, preset = $("horaire-preset").value;
  if (!/^\d\d:\d\d$/.test(debut) || !/^\d\d:\d\d$/.test(fin) || debut === fin || !preset) {
    bandeau("Choisis deux heures différentes et un preset.");
    return;
  }
  regler("automatique", "horaires", etat.reglages.automatique.horaires.concat([{ debut, fin, preset }]));
  dessinerAuto();
});

// ---------------------------------------------------------------------------------------------
// Loupe : un preset survolé, en grand, après un court arrêt de la souris.

const loupe = { apercu: new window.Apercu($("toile-loupe")), p: null, minuterie: 0 };
function montrerLoupe(t) {
  clearTimeout(loupe.minuterie);
  loupe.minuterie = setTimeout(() => {
    if (etat.vue !== "presets" || etat.renomme) return;
    const r = t.racine.getBoundingClientRect(), d = $("loupe"), largeur = 300;
    const gauche = r.right + 12 + largeur < innerWidth ? r.right + 12 : r.left - 12 - largeur;
    if (gauche < 0) return; // pas la place : la vignette suffit
    d.style.left = gauche + "px";
    d.style.top = borne(r.top - 40, 12, innerHeight - 360) + "px";
    $("nom-loupe").textContent = t.p.nom;
    loupe.p = t.p;
    loupe.apercu = new window.Apercu($("toile-loupe"));
    loupe.apercu.regler(avecHabillage(t.p.reglages));
    loupe.apercu.teinteImposee = teinteImposee(t.p.reglages);
    loupe.apercu.curseur = flecheJeu((t.p.reglages.souris && t.p.reglages.souris.jeu) || etat.curseurs);
    d.hidden = false;
    figer(loupe.apercu, sonPourApercus(performance.now()));
  }, 650);
}
function cacherLoupe() {
  clearTimeout(loupe.minuterie);
  loupe.p = null;
  $("loupe").hidden = true;
}
$("defile-grille").addEventListener("scroll", cacherLoupe, { passive: true });

// ---------------------------------------------------------------------------------------------
// Comparaison de deux presets

const comparaison = { apercus: [], fige: false };

function libelleReglage(section, cle) {
  const def = ELEMENTS.find((d) => d.section === section && d.cle === cle);
  if (def) return [def.libelle || def.bascule || def.choix || def.trajet, def.aff];
  for (const d of ELEMENTS) {
    const puce = (d.puces || []).find(([, s, c]) => s === section && c === cle);
    if (puce) return [puce[0]];
  }
  const effet = PANNEAUX.find((p) => p.actif && p.actif[0] === section && p.actif[1] === cle);
  if (effet) return [effet.titre];
  const noms = {
    "couleur.application": "Couleur de l'application", "couleur.site": "Couleur du site", "couleur.musique": "Couleur qui suit la musique",
    "couleur.synchro_prisme": "Synchro Prisme", "couleur.pochette": "Couleur de la pochette", "souris.jeu": "Curseurs", "modulations.liste": "Modulations",
    "declencheurs.liste": "Déclencheurs", "particules.rebonds": "Rebonds sur la fenêtre",
  };
  return [noms[section + "." + cle] || section + "." + cle];
}

function valeurLisible(v, aff, section) {
  if (Array.isArray(v)) {
    const nom = section === "declencheurs" ? "déclencheur" : "modulation";
    return v.length ? `${v.length} ${nom}${v.length > 1 ? "s" : ""}` : section === "declencheurs" ? "aucun" : "aucune";
  }
  if (typeof v === "boolean") return v ? "oui" : "non";
  if (aff && typeof v === "number") return aff(v);
  if (typeof v === "string") return NOMS_FORMES[v] || v || "—";
  return String(v);
}

function ecartsEntre(a, b) {
  const lignes = [];
  for (const s of SECTIONS_PRESET) {
    const x = a[s] || {}, y = b[s] || {};
    for (const k of new Set([...Object.keys(x), ...Object.keys(y)])) {
      // les trajets d'une image (points, contours) ne se lisent pas en liste ; la synchro Prisme
      // ne compte que si Prisme est là
      if (HORS_PRESET.has(s + "." + k) || k === "trace" || k === "contours" || memeValeur(x[k], y[k])) continue;
      if (s + "." + k === "couleur.synchro_prisme" && etat.prisme !== true) continue;
      const [libelle, aff] = libelleReglage(s, k);
      lignes.push([libelle, valeurLisible(x[k], aff, s), valeurLisible(y[k], aff, s)]);
    }
  }
  return lignes;
}

function comparer(p) {
  if (!etat.comparer || etat.comparer === p.id) {
    etat.comparer = etat.comparer === p.id ? null : p.id;
    if (etat.comparer) bandeau(`« ${p.nom} » retenu : choisis le preset à lui comparer (Comparer).`);
    dessinerPresets();
    return;
  }
  const a = tousLesPresets().find((x) => x.id === etat.comparer);
  etat.comparer = null;
  dessinerPresets();
  if (a) ouvrirComparaison(a, p);
}

function ouvrirComparaison(a, b) {
  cacherLoupe();
  const zone = $("comparaison");
  comparaison.apercus = [];
  comparaison.fige = false;
  const cotes = [a, b].map((p) => {
    const d = el("div", "cote");
    const canvas = el("canvas", "scene");
    const ap = new window.Apercu(canvas);
    ap.regler(avecHabillage(p.reglages));
    ap.teinteImposee = teinteImposee(p.reglages);
    suivreSouris(canvas, ap);
    comparaison.apercus.push(ap);
    const choisirCe = el("button", "puce debut", "Appliquer");
    choisirCe.addEventListener("click", () => { choisir(p); fermerComparaison(); });
    d.append(canvas, el("span", "t-nom", p.nom), choisirCe);
    return d;
  });
  const ecarts = ecartsEntre(a.reglages, b.reglages);
  const table = el("div", "regles pleine");
  if (!ecarts.length) table.append(el("div", "regle", "").appendChild(el("span", "vide-regle t-petit", "Mêmes réglages.")).parentNode);
  for (const [libelle, x, y] of ecarts) {
    const l = el("div", "regle ecart");
    l.append(el("span", "t-petit", libelle), el("span", "t-petit", x), el("span", "fleche", "≠"), el("span", "t-petit", y));
    table.append(l);
  }
  const titre = el("span", "t-label pleine", ecarts.length ? `${ecarts.length} réglage${ecarts.length > 1 ? "s" : ""} différent${ecarts.length > 1 ? "s" : ""}` : "");
  zone.replaceChildren(...cotes, titre, table);
  ouvrirBoite("voile-comparaison", $("fermer-comparaison"));
  requestAnimationFrame(() => comparaison.apercus.forEach((ap) => ap.dimensionner()));
}

// Souris sur un aperçu : le sillage la suit.
function suivreSouris(canvas, ap) {
  canvas.addEventListener("pointermove", (e) => ap.survoler(e.clientX, e.clientY));
  canvas.addEventListener("pointerleave", () => ap.survoler(null));
}

function fermerComparaison() {
  fermerBoite("voile-comparaison");
  comparaison.apercus = [];
}
$("fermer-comparaison").addEventListener("click", fermerComparaison);
$("voile-comparaison").addEventListener("click", (e) => { if (e.target === $("voile-comparaison")) fermerComparaison(); });

// ---------------------------------------------------------------------------------------------
// Trajet d'une image : les contours d'une image (choisie, glissée sur la fenêtre) ou d'un texte,
// d'un emoji ; l'appli les calcule (`contours.rs`), la page les montre sur l'image et sur un
// aperçu de l'essaim, puis les garde dans `[orbite] contours`.

const image = {
  cible: null, // réglage de forme à mettre sur « trace » (orbite ou essaim)
  source: null, // image chargée, ou null pour le texte
  nom: "",
  seuil: null, // null : calculé par l'appli
  inverser: false,
  tous: false,
  contours: [], // en pixels de l'image analysée
  taille: [0, 0],
  apercu: null,
  attente: 0, // minuterie de la prochaine analyse
};
const COTE_ANALYSE = 512;

// Couleur d'un jeton de style.css, pour dessiner dans un canevas.
function couleurJeton(nom) {
  return getComputedStyle(document.documentElement).getPropertyValue(nom).trim();
}

function ouvrirImage(section, cle) {
  image.cible = { section, cle };
  if (!image.apercu) image.apercu = new window.Apercu($("apercu-image"));
  ouvrirBoite("voile-image", $("choisir-image"));
  requestAnimationFrame(() => image.apercu.dimensionner());
  dessinerImage();
}

function fermerImage() {
  fermerBoite("voile-image");
}

// Image à analyser, par son adresse `data:` (la page n'accepte pas les adresses `blob:`).
function chargerImage(adresse, nom) {
  const img = new Image();
  img.onload = () => {
    image.source = img;
    image.nom = nom || "";
    $("texte-image").value = "";
    garderTous(false);
    analyser();
  };
  img.onerror = () => bandeau(`« ${nom || "Cette image"} » ne s'ouvre pas.`);
  img.src = adresse;
}

// L'image (ou le texte) dans un canevas de `COTE_ANALYSE` pixels de côté au plus, proportions
// gardées ; un texte est dessiné en noir sur transparent (sa forme : la transparence), cadré sur
// son encre (largeur et hauteur réelles des lettres).
function rasteriser() {
  const c = document.createElement("canvas"), texte = $("texte-image").value.trim();
  if (texte) {
    c.width = c.height = COTE_ANALYSE;
    const g = c.getContext("2d", { willReadFrequently: true });
    const police = (px) => `900 ${px}px Archivo, "Segoe UI Emoji", "Segoe UI Symbol", sans-serif`;
    g.textAlign = "left";
    g.textBaseline = "alphabetic";
    const encre = (px) => {
      g.font = police(px);
      const m = g.measureText(texte);
      return { g: m.actualBoundingBoxLeft, d: m.actualBoundingBoxRight, h: m.actualBoundingBoxAscent, b: m.actualBoundingBoxDescent };
    };
    const e0 = encre(100), cible = COTE_ANALYSE * 0.86;
    const px = Math.max(8, Math.floor(100 * Math.min(cible / Math.max(1, e0.g + e0.d), cible / Math.max(1, e0.h + e0.b))));
    const e = encre(px);
    g.fillStyle = "#000";
    g.fillText(texte, (COTE_ANALYSE - (e.d - e.g)) / 2, (COTE_ANALYSE + e.h - e.b) / 2);
    return { c, g };
  }
  const img = image.source;
  if (!img) return null;
  const l0 = img.naturalWidth || img.width || COTE_ANALYSE, h0 = img.naturalHeight || img.height || COTE_ANALYSE;
  const k = Math.min(1, COTE_ANALYSE / Math.max(l0, h0)) || 1;
  c.width = Math.max(8, Math.round(l0 * k));
  c.height = Math.max(8, Math.round(h0 * k));
  const g = c.getContext("2d", { willReadFrequently: true });
  g.drawImage(img, 0, 0, c.width, c.height);
  return { c, g };
}

// Octets en base64 (par paquets : `String.fromCharCode` refuse les très longs tableaux).
function base64(octets) {
  let s = "";
  for (let i = 0; i < octets.length; i += 0x8000) s += String.fromCharCode.apply(null, octets.subarray(i, i + 0x8000));
  return btoa(s);
}

// Demande les contours à l'appli (un peu après le dernier changement : la glissière du seuil).
function analyser() {
  clearTimeout(image.attente);
  image.attente = setTimeout(() => {
    const r = rasteriser();
    if (!r) { image.contours = []; dessinerImage(); return; }
    const px = r.g.getImageData(0, 0, r.c.width, r.c.height).data;
    image.taille = [r.c.width, r.c.height];
    image.rendu = r.c;
    envoyer({ cmd: "contours", largeur: r.c.width, hauteur: r.c.height, pixels: base64(px), seuil: image.seuil, inverser: image.inverser, tous: image.tous });
  }, 80);
}

function recevoirContours(m) {
  image.contours = Array.isArray(m.liste) ? m.liste : [];
  if (image.seuil === null) $("seuil-image").value = m.seuil;
  $("valeur-seuil").textContent = (image.seuil === null ? "auto · " : "") + m.seuil + (m.transparence ? " (transparence)" : "");
  const n = image.contours.length;
  $("info-image").textContent = !n
    ? "Aucune forme trouvée : change le seuil, inverse, ou prends une image plus contrastée."
    : n === 1 ? "Un contour." : `${n} contours : un par anneau de comètes.`;
  dessinerImage();
}

// L'image adoucie sur un papier clair (un logo sombre se voit), ses contours en couleur, dans la
// toile carrée.
function dessinerImage() {
  const c = $("toile-image"), g = c.getContext("2d");
  g.clearRect(0, 0, c.width, c.height);
  const [l, h] = image.taille;
  if (!image.rendu || !l || !h) {
    g.fillStyle = couleurJeton("--scene-guide");
    g.font = "600 14px Archivo, sans-serif";
    g.textAlign = "center";
    g.fillText("Choisis une image, glisse-la ici, ou tape un texte", c.width / 2, c.height / 2);
    return;
  }
  const k = Math.min(c.width / l, c.height / h) * 0.94, ox = (c.width - l * k) / 2, oy = (c.height - h * k) / 2;
  g.fillStyle = couleurJeton("--papier-image");
  g.fillRect(ox, oy, l * k, h * k);
  g.globalAlpha = 0.45;
  g.drawImage(image.rendu, ox, oy, l * k, h * k);
  g.globalAlpha = 1;
  g.strokeStyle = couleurJeton("--signal");
  g.lineWidth = 2.5;
  g.lineJoin = "round";
  for (const contour of image.contours) {
    g.beginPath();
    contour.forEach(([x, y], i) => (i ? g.lineTo(ox + x * k, oy + y * k) : g.moveTo(ox + x * k, oy + y * k)));
    g.closePath();
    g.stroke();
  }
}

// Réglages de l'aperçu de la boîte : ceux en place, avec ces contours.
function reglagesImage() {
  const r = etat.reglages;
  const orbite = { ...r.orbite, contours: image.contours };
  const essaim = { ...r.essaim };
  if (image.cible && image.cible.section === "essaim") essaim.forme = "trace";
  else orbite.forme = "trace";
  return { ...r, orbite, essaim };
}

// Garde les contours (arrondis au dixième de pixel) ; le plus grand aussi dans `trace`, pour les
// versions d'avant.
function validerImage() {
  if (!image.contours.length) { bandeau("Pas encore de contour : choisis une image ou tape un texte."); return; }
  const arrondi = image.contours.map((c) => c.map(([x, y]) => [Math.round(x * 10) / 10, Math.round(y * 10) / 10]));
  regler("orbite", "contours", arrondi);
  const pas = Math.ceil(arrondi[0].length / 200);
  regler("orbite", "trace", arrondi[0].filter((_, i) => i % pas === 0));
  regler(image.cible.section, image.cible.cle, "trace");
  majReglages();
  fermerImage();
}

$("choisir-image").addEventListener("click", () => $("fichier-image").click());
$("fichier-image").addEventListener("change", (e) => {
  const f = e.target.files && e.target.files[0];
  e.target.value = "";
  if (!f) return;
  const lecteur = new FileReader();
  lecteur.onload = () => chargerImage(lecteur.result, f.name);
  lecteur.onerror = () => bandeau(`« ${f.name} » illisible.`);
  lecteur.readAsDataURL(f);
});
// Garder tous les contours, ou le plus grand seulement.
function garderTous(tous) {
  image.tous = tous;
  for (const x of document.querySelectorAll("[data-contours]")) x.setAttribute("aria-pressed", (x.dataset.contours === "tous") === tous);
}
// un texte : une lettre par contour, toutes gardées
$("texte-image").addEventListener("input", () => {
  if ($("texte-image").value.trim()) garderTous(true);
  analyser();
});
$("seuil-image").addEventListener("input", (e) => {
  image.seuil = +e.target.value;
  $("seuil-auto").setAttribute("aria-pressed", "false");
  analyser();
});
$("seuil-auto").addEventListener("click", (e) => {
  image.seuil = null;
  e.currentTarget.setAttribute("aria-pressed", "true");
  analyser();
});
$("inverser-image").addEventListener("click", (e) => {
  image.inverser = !image.inverser;
  e.currentTarget.setAttribute("aria-pressed", image.inverser);
  analyser();
});
for (const b of document.querySelectorAll("[data-contours]")) {
  b.addEventListener("click", () => {
    garderTous(b.dataset.contours === "tous");
    analyser();
  });
}
$("valider-image").addEventListener("click", validerImage);
$("fermer-image").addEventListener("click", fermerImage);

// ---------------------------------------------------------------------------------------------
// Export GIF : trois secondes du grand aperçu, 20 images/s, palette fixe de 256 couleurs.

const GIF_COTE = 320, GIF_IMAGES = 60, GIF_DELAI = 5; // centièmes de seconde par image

function indexCouleur(r, g, b) {
  return Math.round(r / 51) * 42 + Math.round(g / 42.5) * 6 + Math.round(b / 51);
}

function paletteGif() {
  const p = new Uint8Array(768);
  for (let i = 0; i < 252; i++) {
    p[i * 3] = Math.floor(i / 42) * 51;
    p[i * 3 + 1] = Math.round((Math.floor(i / 6) % 7) * 42.5);
    p[i * 3 + 2] = (i % 6) * 51;
  }
  return p;
}

// Compression LZW du GIF (codes de 9 à 12 bits), en sous-blocs de 255 octets au plus.
function lzw(indices, sortie) {
  const min = 8, clear = 1 << min, eoi = clear + 1;
  let taille = min + 1, suivant = eoi + 1, dict = new Map();
  const octets = [];
  let acc = 0, nbits = 0;
  const ecrire = (code) => {
    acc |= code << nbits;
    nbits += taille;
    while (nbits >= 8) { octets.push(acc & 255); acc >>>= 8; nbits -= 8; }
  };
  ecrire(clear);
  let prefixe = indices[0];
  for (let i = 1; i < indices.length; i++) {
    const k = indices[i], cle = prefixe * 256 + k, c = dict.get(cle);
    if (c !== undefined) { prefixe = c; continue; }
    ecrire(prefixe);
    if (suivant === 4096) {
      ecrire(clear);
      dict = new Map();
      taille = min + 1;
      suivant = eoi + 1;
    } else {
      if (suivant >= 1 << taille) taille++;
      dict.set(cle, suivant++);
    }
    prefixe = k;
  }
  ecrire(prefixe);
  ecrire(eoi);
  if (nbits > 0) octets.push(acc & 255);
  sortie.push(min);
  for (let i = 0; i < octets.length; i += 255) {
    const bloc = octets.slice(i, i + 255);
    sortie.push(bloc.length, ...bloc);
  }
  sortie.push(0);
}

function encoderGif(images, cote) {
  const o = [];
  const mot = (n) => o.push(n & 255, (n >> 8) & 255);
  for (const c of "GIF89a") o.push(c.charCodeAt(0));
  mot(cote); mot(cote);
  o.push(0xf7, 0, 0);
  o.push(...paletteGif());
  o.push(0x21, 0xff, 11, ..."NETSCAPE2.0".split("").map((c) => c.charCodeAt(0)), 3, 1, 0, 0, 0);
  for (const indices of images) {
    o.push(0x21, 0xf9, 4, 0); mot(GIF_DELAI); o.push(0, 0);
    o.push(0x2c); mot(0); mot(0); mot(cote); mot(cote); o.push(0);
    lzw(indices, o);
  }
  o.push(0x3b);
  return new Uint8Array(o);
}

function versBase64(octets) {
  let s = "";
  for (let i = 0; i < octets.length; i += 0x8000) s += String.fromCharCode.apply(null, octets.subarray(i, i + 0x8000));
  return btoa(s);
}

function lancerGif() {
  if (etat.gif) return;
  const c = document.createElement("canvas");
  c.width = c.height = GIF_COTE;
  etat.gif = { c, g: c.getContext("2d", { willReadFrequently: true }), images: [], prochain: 0, fond: getComputedStyle($("grand-apercu")).backgroundColor };
  $("exporter-gif").textContent = "Enregistrement…";
  $("exporter-gif").disabled = true;
}

// Appelée après chaque image du grand aperçu pendant l'enregistrement.
function imageGif(maintenant) {
  const e = etat.gif;
  if (!e || maintenant < e.prochain) return;
  e.prochain = maintenant + GIF_DELAI * 10;
  const src = $("grand-apercu");
  e.g.fillStyle = e.fond;
  e.g.fillRect(0, 0, GIF_COTE, GIF_COTE);
  e.g.drawImage(src, 0, 0, src.width, src.height, 0, 0, GIF_COTE, GIF_COTE);
  const px = e.g.getImageData(0, 0, GIF_COTE, GIF_COTE).data, idx = new Uint8Array(GIF_COTE * GIF_COTE);
  for (let i = 0; i < idx.length; i++) idx[i] = indexCouleur(px[i * 4], px[i * 4 + 1], px[i * 4 + 2]);
  e.images.push(idx);
  if (e.images.length < GIF_IMAGES) return;
  etat.gif = null;
  $("exporter-gif").textContent = "Encodage…";
  setTimeout(() => {
    const nom = $("nom-actif").textContent.replace(/^Preset « (.*) »$/, "$1").replace(/[«»]/g, "").replace(/\s+/g, " ").trim() || "Essaim";
    envoyer({ cmd: "enregistrer_gif", nom, donnees: versBase64(encoderGif(e.images, GIF_COTE)) });
    $("exporter-gif").textContent = "Exporter en GIF";
    $("exporter-gif").disabled = false;
  }, 30);
}

// ---------------------------------------------------------------------------------------------
// Accueil : trois étapes au premier lancement, à revoir depuis les réglages avancés.

const ETAPES_ACCUEIL = [
  {
    titre: "Bienvenue",
    corps: () => [
      el("p", "t-texte", "Essaim dessine autour de ton pointeur des comètes qui suivent la musique : chaque kick les fait changer de sens et lâche des satellites, les aigus font jaillir des étincelles."),
      el("p", "note", "Ton pointeur reste celui de Windows : Essaim dessine seulement autour, dans une surcouche qui laisse passer les clics."),
    ],
  },
  {
    titre: "Choisis un style",
    corps: () => {
      const g = el("div", "choix-accueil");
      for (const p of etat.presets.filter((x) => x.fourni)) {
        const b = el("button", "puce forme");
        b.append(picto(p.reglages.orbite.forme, p.reglages.orbite), document.createTextNode(p.nom));
        b.setAttribute("aria-pressed", etat.actif === p.nom);
        b.addEventListener("click", () => {
          envoyer({ cmd: "appliquer", nom: p.nom });
          for (const x of g.children) x.setAttribute("aria-pressed", x === b);
        });
        g.append(b);
      }
      return [el("p", "note", "Un clic l'applique tout de suite : regarde autour de ta souris. Tu pourras tout régler ensuite."), g];
    },
  },
  {
    titre: "À savoir",
    corps: () => {
      const l = el("div", "raccourcis");
      for (const [k, t] of [["Ctrl + Alt + →", "preset suivant, partout dans Windows"], ["Ctrl + Alt + ←", "preset précédent"], ["Ctrl + Alt + P", "pause"], ["04 Auto", "un preset par application ou par heure"], ["Icône", "presets et pause dans la zone de notification"]]) {
        l.append(el("span", "touche", k), el("span", "", t));
      }
      return [l, el("p", "note", "Les cases à côté de chaque réglage notent sur 10 son impact sur la puissance utilisée.")];
    },
  },
];
let etapeAccueil = 0;

function ouvrirAccueil() {
  etapeAccueil = 0;
  majAccueil();
  ouvrirBoite("voile-accueil", $("suivant-accueil"));
}

function majAccueil() {
  const e = ETAPES_ACCUEIL[etapeAccueil];
  $("etape-accueil").textContent = `${etapeAccueil + 1} / ${ETAPES_ACCUEIL.length}`;
  $("titre-accueil").textContent = e.titre;
  $("corps-accueil").replaceChildren(...e.corps());
  $("suivant-accueil").textContent = etapeAccueil === ETAPES_ACCUEIL.length - 1 ? "Commencer" : "Suivant →";
}

function fermerAccueil() {
  fermerBoite("voile-accueil");
  memoire.ecrire("accueil", true);
}
$("suivant-accueil").addEventListener("click", () => {
  if (etapeAccueil === ETAPES_ACCUEIL.length - 1) { fermerAccueil(); return; }
  etapeAccueil++;
  majAccueil();
});
$("passer-accueil").addEventListener("click", fermerAccueil);

// ---------------------------------------------------------------------------------------------
// Vues, panneau, bandeau

// Boîtes modales : la fenêtre derrière devient inerte, le focus va dans la boîte et revient à
// l'élément d'origine à la fermeture ; Échap ferme la dernière ouverte (voir le clavier).
const ouvertes = []; // { id, retour }
const FERMETURES = {};
function majInerte() {
  document.querySelector(".fenetre").inert = ouvertes.length > 0;
  // une boîte ouverte par-dessus une autre rend celle de dessous inerte aussi
  ouvertes.forEach((o, i) => { $(o.id).inert = i < ouvertes.length - 1; });
}
function ouvrirBoite(id, cible) {
  const voile = $(id);
  if (voile.hidden) ouvertes.push({ id, retour: document.activeElement });
  voile.hidden = false;
  majInerte();
  const premier = cible || voile.querySelector("button, input, select, [tabindex]");
  if (premier) premier.focus();
}
function fermerBoite(id) {
  const voile = $(id);
  if (voile.hidden) return;
  voile.hidden = true;
  voile.inert = false;
  const i = ouvertes.findIndex((o) => o.id === id);
  const o = i >= 0 ? ouvertes.splice(i, 1)[0] : null;
  majInerte();
  if (o && o.retour && o.retour.isConnected && o.retour !== document.body) o.retour.focus({ preventScroll: true });
}

// Onglets : un seul dans l'ordre de tabulation (celui de la vue), flèches gauche / droite, Début
// et Fin pour passer de l'un à l'autre (la vue suit le focus).
const VUES = ["presets", "reglages", "curseurs", "auto", "application"];
function montrerVue(v, focus) {
  etat.vue = v;
  for (const id of VUES) {
    $("vue-" + id).hidden = v !== id;
    const onglet = $("onglet-" + id);
    onglet.setAttribute("aria-selected", v === id);
    onglet.tabIndex = v === id ? 0 : -1;
  }
  if (focus) $("onglet-" + v).focus();
  if (v === "presets") dessinerPresets();
  if (v === "reglages") requestAnimationFrame(() => grand.dimensionner());
  if (v === "curseurs" && !etat.jeux) envoyer({ cmd: "jeux" });
  if (v === "auto") { envoyer({ cmd: "applis" }); dessinerAuto(); }
  cacherLoupe();
}

// Bandeau : disparaît seul après quelques secondes, sauf tant que la souris ou le focus clavier
// est dessus (le délai repart en entier ensuite).
const minuterie = { id: 0, delai: 6000, suspendu: false };
function armerBandeau() {
  clearTimeout(minuterie.id);
  if (!minuterie.suspendu) minuterie.id = setTimeout(() => { $("bandeau").hidden = true; }, minuterie.delai);
}
function suspendreBandeau(oui) {
  minuterie.suspendu = oui;
  if (oui) clearTimeout(minuterie.id); else armerBandeau();
}
function bandeau(texte, action) {
  $("bandeau-texte").textContent = texte;
  const b = $("bandeau-action");
  b.hidden = !action;
  if (action) {
    b.textContent = action.libelle;
    b.onclick = () => { action.faire(); $("bandeau").hidden = true; };
  }
  $("bandeau").hidden = false;
  minuterie.delai = action ? 9000 : 6000;
  armerBandeau();
}
{
  const d = $("bandeau");
  d.addEventListener("mouseenter", () => suspendreBandeau(true));
  d.addEventListener("mouseleave", () => suspendreBandeau(d.contains(document.activeElement)));
  d.addEventListener("focusin", () => suspendreBandeau(true));
  d.addEventListener("focusout", (e) => { if (!d.contains(e.relatedTarget)) suspendreBandeau(d.matches(":hover")); });
}

function ouvrirPanneau() {
  const p = etat.presets.find((x) => x.nom === etat.actif && !x.fourni);
  $("nom-preset").value = p ? p.nom : "";
  $("publier").checked = false;
  $("erreur-panneau").hidden = true;
  ouvrirBoite("voile", $("nom-preset"));
}

function fermerPanneau() {
  fermerBoite("voile");
}

// Annuler : la dernière action de la page qui le permet (copie d'allure), sinon le dernier preset
// appliqué (l'appli garde les réglages d'avant).
const annuler = () => (annulationLocale ? annulationLocale() : envoyer({ cmd: "annuler" }));

// ---------------------------------------------------------------------------------------------
// Messages de l'appli

window.essaim = {
  recevoir(m) {
    switch (m.type) {
      case "contours":
        recevoirContours(m);
        break;
      case "image": {
        // image glissée sur la fenêtre : trajet des comètes
        if ($("voile-image").hidden) ouvrirImage("orbite", "forme");
        chargerImage(m.donnees, m.nom);
        break;
      }
      case "icone":
        window.contoursIcone = Array.isArray(m.liste) ? m.liste : [];
        apercuChange();
        majReglages();
        break;
      case "etat": {
        const premier = !etat.reglages;
        etat.reglages = reglagesRecus(m.reglages);
        etat.defauts = m.defauts;
        etat.presets = m.presets;
        suivreRenommages();
        etat.actif = m.actif;
        etat.proche = m.proche;
        etat.depot = m.depot;
        etat.curseurs = m.curseurs;
        etat.demarrage = m.demarrage;
        if (premier) {
          if (typeof m.version === "string") {
            $("version").textContent = "v" + m.version;
            $("version-installee").textContent = m.version;
          }
          construireReglages();
          construireApplication();
          chargerCommu(false);
          if (!memoire.lire("accueil", false)) ouvrirAccueil();
        }
        if (typeof m.prisme === "boolean") etat.prisme = m.prisme;
        // config.toml illisible (faute de frappe) : Essaim tourne avec les réglages d'avant, la
        // page montre les valeurs par défaut ; on le dit une fois par erreur
        if (m.illisible && m.illisible !== etat.illisible) {
          bandeau(`config.toml est illisible : la page montre les valeurs par défaut. ${m.illisible}`, { libelle: "Ouvrir le fichier", faire: () => envoyer({ cmd: "ouvrir_config" }) });
        }
        etat.illisible = m.illisible || null;
        grand.regler(etat.reglages);
        apercuChange();
        majReglages();
        // la grille n'est refaite que si elle est affichée ou si la liste des presets a changé
        // (un état arrive toutes les 0,7 s pendant qu'une glissière bouge) ; sinon, en revenant
        // sur la vue Presets
        const liste = JSON.stringify(etat.presets.map((p) => [p.nom, p.fourni, p.date]));
        if (etat.vue === "presets" || liste !== etat.listePresets) dessinerPresets();
        etat.listePresets = liste;
        dessinerCurseurs();
        if (etat.vue === "auto") dessinerAuto();
        if (premier) annoncerPrete();
        break;
      }
      case "applis":
        etat.applis = m.applis || [];
        etat.recente = typeof m.recente === "string" ? m.recente : "";
        dessinerAuto();
        break;
      case "son":
        recevoirSon(m);
        break;
      case "sorties":
        etat.sorties = Array.isArray(m.liste) ? m.liste.filter((s) => s && typeof s.id === "string" && typeof s.nom === "string") : [];
        majSorties();
        break;
      case "applis_sonores":
        etat.applisSon = Array.isArray(m.liste) ? m.liste.filter((a) => a && typeof a.exe === "string" && typeof a.nom === "string") : [];
        majApplisSon();
        break;
      case "maj":
        recevoirMaj(m);
        break;
      case "charge":
        recevoirCharge(m);
        break;
      case "jeux":
        etat.jeux = m.jeux;
        dessinerCurseurs();
        break;
      case "detail_jeu":
        dessinerDetail(m);
        break;
      case "apercu_jeu":
        recevoirApercuJeu(m);
        break;
      case "applique":
        annulationLocale = null; // Ctrl+Z défait maintenant ce preset
        bandeau(`« ${m.nom} » appliqué.`, { libelle: "Annuler (Ctrl+Z)", faire: annuler });
        break;
      case "enregistre":
        fermerPanneau();
        etat.source = "mien";
        montrerVue("presets");
        break;
      case "refus":
        $("erreur-panneau").textContent = m.texte;
        $("erreur-panneau").hidden = false;
        break;
      case "message":
      case "erreur":
        bandeau(m.texte);
        break;
    }
  },
};

// ---------------------------------------------------------------------------------------------
// Animation : une seule boucle ; vignettes à 30 images/s, grand aperçu à chaque image.

const grand = new window.Apercu($("grand-apercu"));
let derniere = performance.now(), cumul = 0;

// Teinte imposée à un aperçu : celle que l'appli suit en ce moment (pochette du morceau qui joue,
// sinon application ou site au premier plan, sinon éclairage de Prisme), si le preset suit la
// même source.
const sourceTeinte = (c) => (c.couleur.pochette && etat.lecture === true ? "pochette"
  : c.couleur.application || c.couleur.site ? "appli" : c.couleur.synchro_prisme ? "prisme" : null);
function teinteImposee(c) {
  if (!c || etat.teinte === null || !etat.reglages) return null;
  const s = sourceTeinte(c);
  return s && s === sourceTeinte(etat.reglages) ? etat.teinte : null;
}

// Une erreur dans un aperçu ne doit pas arrêter les autres : notée une fois dans la console.
const erreursVues = new Set();
function protege(nom, f) {
  try {
    f();
    return true;
  } catch (e) {
    if (!erreursVues.has(nom)) { erreursVues.add(nom); console.error(`Aperçu « ${nom} » :`, e); }
    return false;
  }
}

// Grands aperçus (réglages, loupe, comparaison) : à l'arrêt si Windows demande moins
// d'animations, avec un bouton Lecture ; à l'arrêt, ils sont redessinés figés quand un réglage
// ou leur taille change.
const mouvementReduit = matchMedia("(prefers-reduced-motion: reduce)");
let lecture = !mouvementReduit.matches, grandAFiger = true;
function apercuChange() { grandAFiger = true; }
// Repère de place de la figure dont le panneau est affiché (croix au pointeur, cadre de sa zone) :
// dessiné sur le grand aperçu après la prise d'une image du GIF, qui ne l'a donc pas.
function repereFigure() {
  // le panneau « Anneau » montre deux figures : ses deux formes (anneau lisse, orbe)
  const sections = etat.panneau === "anneau" ? ["anneau", "orbe"] : etat.panneau in NOMS_FIGURES ? [etat.panneau] : [];
  for (const s of sections) grand.dessinerRepere(s, couleurJeton("--scene-repere"));
}
function figer(ap, son) {
  ap.dimensionner();
  for (let i = 0; i < 40; i++) ap.avancer(1 / 30, son);
  ap.dessiner();
}
function majLecture() {
  for (const b of document.querySelectorAll("[data-lecture]")) {
    b.hidden = !mouvementReduit.matches;
    b.setAttribute("aria-pressed", lecture);
  }
}
mouvementReduit.addEventListener("change", () => {
  lecture = !mouvementReduit.matches;
  grandAFiger = true;
  comparaison.fige = false;
  majLecture();
});
for (const b of document.querySelectorAll("[data-lecture]")) b.addEventListener("click", () => { lecture = !lecture; majLecture(); });

function boucle(maintenant) {
  // en tête : une exception plus bas n'arrête pas l'animation
  requestAnimationFrame(boucle);
  const dt = Math.min(0.05, (maintenant - derniere) / 1000);
  derniere = maintenant;
  const son = sonPourApercus(maintenant);
  const bouge = lecture || !!etat.gif;
  if (etat.vue === "curseurs") protege("curseurs", () => animer(maintenant));
  if (etat.reglages && etat.vue === "reglages") {
    protege("réglages", () => {
      grand.teinteImposee = teinteImposee(etat.reglages);
      grand.curseur = flecheJeu(etat.curseurs);
      const largeur = grand.canvas.width;
      grand.dimensionner();
      const sonGrand = sonSimule(son, maintenant);
      // trajet « icône » : ses contours, redemandés de temps en temps (ils suivent l'application
      // au premier plan)
      const o = etat.reglages;
      if ((o.orbite.forme === "icone" || o.essaim.forme === "icone") && maintenant - (etat.icone || 0) > 3000) {
        etat.icone = maintenant;
        envoyer({ cmd: "icone" });
      }
      if (bouge) {
        grand.avancer(dt, sonGrand);
        grand.dessiner();
        imageGif(maintenant);
        repereFigure();
        majJaugesModulations();
      } else if (grandAFiger || largeur !== grand.canvas.width) {
        figer(grand, sonGrand);
        repereFigure();
      }
      grandAFiger = false;
    });
  }
  if (loupe.p && etat.vue === "presets" && bouge) {
    protege("loupe", () => {
      loupe.apercu.dimensionner();
      loupe.apercu.avancer(dt, son);
      loupe.apercu.dessiner();
    });
  }
  if (bouge || !comparaison.fige) {
    comparaison.apercus.forEach((ap, i) => protege("comparaison " + i, () => {
      if (bouge) {
        ap.dimensionner();
        ap.avancer(dt, son);
        ap.dessiner();
      } else {
        figer(ap, son);
      }
    }));
    comparaison.fige = !bouge && comparaison.apercus.length > 0;
  }
  // boîte « Trajet d'une image » : l'essaim sur les contours trouvés
  if (image.apercu && !$("voile-image").hidden && etat.reglages) {
    protege("image", () => {
      image.apercu.regler(reglagesImage());
      image.apercu.dimensionner();
      image.apercu.avancer(bouge ? dt : 1 / 30, son);
      image.apercu.dessiner();
    });
  }
  cumul += dt;
  if (etat.vue === "presets" && cumul >= 1 / 30) {
    const pas = Math.min(0.1, cumul);
    cumul = 0;
    for (const id of visibles) {
      const t = tuiles.get(id);
      if (!t || t.erreur || !t.p.reglages) continue;
      // une vignette en erreur est laissée de côté jusqu'à ce que ses réglages changent
      t.erreur = !protege(t.p.nom, () => {
        t.apercu.teinteImposee = teinteImposee(t.p.reglages);
        // le curseur du preset (ou celui en place, que le preset ne changera pas)
        t.apercu.curseur = flecheJeu((t.p.reglages.souris && t.p.reglages.souris.jeu) || etat.curseurs);
        if (!t.pret) {
          // première image : un peu d'histoire pour que les traînées existent déjà
          figer(t.apercu, son);
          t.pret = true;
        }
        const anime = etat.mode === "direct" || (etat.mode === "survol" && etat.survol === id);
        const image = !anime && !!t.p.image && !t.imageKo;
        if (t.image.hidden === image) { t.image.hidden = !image; t.canvas.hidden = image; }
        if (anime) {
          t.apercu.dimensionner();
          t.apercu.avancer(pas, son);
          t.apercu.dessiner();
        }
      });
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Démarrage

for (const v of VUES) $("onglet-" + v).addEventListener("click", () => montrerVue(v));
document.querySelector(".onglets").addEventListener("keydown", (e) => {
  const i = VUES.indexOf(etat.vue), n = VUES.length;
  const j = { ArrowLeft: (i + n - 1) % n, ArrowRight: (i + 1) % n, Home: 0, End: n - 1 }[e.key];
  if (j === undefined) return;
  e.preventDefault();
  montrerVue(VUES[j], true);
});
$("recherche").addEventListener("input", (e) => { etat.recherche = e.target.value; dessinerPresets(); });
Object.assign(FERMETURES, { voile: fermerPanneau, "voile-image": fermerImage, "voile-comparaison": fermerComparaison, "voile-accueil": fermerAccueil });
$("ouvrir-panneau").addEventListener("click", ouvrirPanneau);
$("ouvrir-panneau-2").addEventListener("click", ouvrirPanneau);
$("annuler-panneau").addEventListener("click", fermerPanneau);
$("voile").addEventListener("click", (e) => { if (e.target === $("voile")) fermerPanneau(); });
$("panneau").addEventListener("submit", (e) => {
  e.preventDefault();
  envoyer({ cmd: "enregistrer", nom: $("nom-preset").value, publier: $("publier").checked });
});
$("ouvrir-config").addEventListener("click", () => envoyer({ cmd: "ouvrir_config" }));
$("chercher-maj").addEventListener("click", () => envoyer({ cmd: "chercher_maj" }));
$("redemarrer-maj").addEventListener("click", () => envoyer({ cmd: "redemarrer" }));
$("importer").addEventListener("click", () => envoyer({ cmd: "importer" }));
$("surprise").addEventListener("click", surprise);
$("exporter-gif").addEventListener("click", lancerGif);
$("grand-apercu").addEventListener("pointerdown", (e) => grand.evenement(e.button === 2 ? "clic_droit" : "clic"));
$("simuler-montee").addEventListener("click", () => {
  simulation.debut = performance.now();
  simulation.kicks = null;
  simulation.coups = 0;
});
for (const b of document.querySelectorAll("[data-simuler]")) {
  b.addEventListener("click", () => {
    const quoi = b.dataset.simuler;
    if (quoi === "kick") simulation.ajout.kicks++;
    else if (quoi === "caisse") simulation.ajout.caisses++;
    else grand.evenement(quoi);
  });
}
$("grand-apercu").addEventListener("contextmenu", (e) => e.preventDefault());
suivreSouris($("grand-apercu"), grand);
$("pause").addEventListener("click", () => envoyer({ cmd: "pause" }));
$("bandeau-fermer").addEventListener("click", () => { $("bandeau").hidden = true; });

// Clavier : / chercher, Ctrl+1 à 5 onglets, Ctrl+S enregistrer, Ctrl+Z annuler le dernier preset,
// Ctrl+P pause, flèches dans la grille, Échap ferme la boîte ouverte.
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    const ouverte = ouvertes[ouvertes.length - 1];
    if (ouverte) { FERMETURES[ouverte.id](); return; }
    if (etat.comparer) { etat.comparer = null; dessinerPresets(); return; }
  }
  if (e.ctrlKey && ["1", "2", "3", "4", "5"].includes(e.key)) { e.preventDefault(); montrerVue(VUES[+e.key - 1]); return; }
  if (e.ctrlKey && e.key.toLowerCase() === "s") { e.preventDefault(); ouvrirPanneau(); return; }
  if (e.ctrlKey && e.key.toLowerCase() === "p") { e.preventDefault(); envoyer({ cmd: "pause" }); return; }
  if (enSaisie()) return;
  if (e.ctrlKey && e.key.toLowerCase() === "z") { e.preventDefault(); annuler(); return; }
  if (e.key === "/" && etat.vue === "presets") { e.preventDefault(); $("recherche").focus(); return; }
  if (etat.vue === "presets" && deplacerDansLaGrille(e)) e.preventDefault();
});
// Fenêtre redimensionnée : les vignettes figées sont redessinées une fois, à la fin du
// redimensionnement (et non à chaque événement).
let finRedimension = 0;
window.addEventListener("resize", () => {
  clearTimeout(finRedimension);
  finRedimension = setTimeout(() => { for (const t of tuiles.values()) t.pret = false; }, 200);
});

dessinerModes();
majLecture();
montrerVue("presets");
envoyer({ cmd: "etat" });
requestAnimationFrame(boucle);

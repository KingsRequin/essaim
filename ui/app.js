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
const envoyer = (demande) => window.ipc && window.ipc.postMessage(JSON.stringify(demande));
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

const etat = {
  reglages: null,
  presets: [],
  actif: null,
  depot: "",
  vue: "presets",
  source: "tout",
  recherche: "",
  mode: memoire.lire("mode", matchMedia("(prefers-reduced-motion: reduce)").matches ? "survol" : "direct"),
  avance: memoire.lire("avance", false),
  commu: { etat: "rien", liste: [], message: "" },
  son: { kicks: 0, basse: 0, hauteur: 0 },
  dernierSon: -1e9,
  demo: { kicks: 0, prochain: 0 },
  survol: null,
};

// ---------------------------------------------------------------------------------------------
// Son : le vrai son de Windows, ou une démonstration (un kick par demi-seconde) en silence.

function musique() {
  return performance.now() - etat.dernierSon < 2500;
}

function sonPourApercus(maintenant) {
  if (musique()) return etat.son;
  if (maintenant >= etat.demo.prochain) {
    etat.demo.kicks++;
    etat.demo.prochain = maintenant + 500;
  }
  return { kicks: etat.demo.kicks, basse: 0.35, hauteur: 50 };
}

function recevoirSon(m) {
  const kick = m.kicks !== etat.son.kicks;
  if (kick || m.basse > 0.03) etat.dernierSon = performance.now();
  etat.son = { kicks: m.kicks, basse: m.basse, hauteur: m.hauteur };
  const actif = musique();
  $("ecoute").classList.toggle("actif", actif);
  $("ecoute-texte").textContent = actif ? "Musique détectée" : "Silence · aperçus en démo";
  if (kick && actif && !matchMedia("(prefers-reduced-motion: reduce)").matches) {
    const t = $("temoin");
    t.classList.remove("coup");
    void t.offsetWidth;
    t.classList.add("coup");
  }
}

// ---------------------------------------------------------------------------------------------
// Presets : fournis, à moi, communauté

function fusion(base, partiel) {
  const r = Array.isArray(base) ? [...base] : { ...base };
  for (const [k, v] of Object.entries(partiel || {})) {
    r[k] = v && typeof v === "object" && !Array.isArray(v) && base && typeof base[k] === "object" ? fusion(base[k], v) : v;
  }
  return r;
}

function reglagesDeBase() {
  const d = etat.presets.find((p) => p.fourni && p.nom === "Défaut");
  return d ? d.reglages : etat.reglages;
}

function tousLesPresets() {
  const locaux = etat.presets.map((p) => ({
    id: "p:" + p.nom, nom: p.nom, origine: p.fourni ? "fourni" : "mien", reglages: p.reglages, etiquettes: [], auteur: "",
  }));
  const commu = etat.commu.liste.map((p) => ({ ...p, id: "c:" + p.nom, origine: "commu" }));
  return locaux.concat(commu);
}

function installe(nom) {
  return etat.presets.some((p) => !p.fourni && p.nom === nom);
}

function filtrer(liste) {
  const q = etat.recherche.trim().toLowerCase();
  const libelle = { fourni: "fourni", mien: "mes presets", commu: "communauté" };
  return liste.filter((p) => {
    if (etat.source !== "tout" && p.origine !== etat.source) return false;
    if (!q) return true;
    return [p.nom, p.auteur, libelle[p.origine], ...(p.etiquettes || [])].join(" ").toLowerCase().includes(q);
  });
}

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
    const index = await r.json();
    const base = reglagesDeBase();
    const liste = (Array.isArray(index.presets) ? index.presets : [])
      .filter((p) => p && typeof p.nom === "string" && p.nom.trim() && p.nom.length <= 40 && p.reglages && typeof p.reglages === "object")
      .map((p) => ({
        nom: p.nom.trim(),
        auteur: typeof p.auteur === "string" ? "@" + p.auteur.replace(/^@/, "").slice(0, 39) : "",
        etiquettes: Array.isArray(p.etiquettes) ? p.etiquettes.filter((e) => typeof e === "string").slice(0, 8) : [],
        reglages: fusion(base, p.reglages),
      }));
    etat.commu = { etat: "ok", liste, message: "" };
  } catch (e) {
    etat.commu = {
      etat: "erreur", liste: [],
      message: e.message === "absent"
        ? `La bibliothèque n'est pas encore en ligne : le dépôt ${etat.depot} n'a pas d'index.json.`
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
  const c = p.reglages;
  const parts = [c.orbite.cometes ? "2 comètes" : "sans comètes", c.essaim.maximum + " satellites"];
  if (p.origine === "commu" && p.etiquettes.length) parts.push(p.etiquettes.slice(0, 2).join(", "));
  return parts.join(" · ");
}

function creerTuile(p) {
  const racine = el("div", "tuile");
  racine.dataset.id = p.id;
  const bouton = el("button");
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
  const apercu = new window.Apercu(canvas);
  const t = { racine, bouton, numero, origine, ligne, actions, apercu, p, pret: false, confirmer: 0 };
  bouton.addEventListener("click", () => choisir(t.p));
  bouton.addEventListener("mouseenter", () => { etat.survol = p.id; });
  bouton.addEventListener("mouseleave", () => { if (etat.survol === p.id) etat.survol = null; });
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

function majTuile(t, p, rang) {
  t.p = p;
  t.apercu.regler(p.reglages);
  t.numero.textContent = deux(rang);
  t.origine.textContent = p.origine === "commu" ? p.auteur || "communauté" : p.origine === "fourni" ? "fourni" : "à toi";
  t.ligne.textContent = detail(p);
  const actif = p.origine === "commu" ? installe(p.nom) && etat.actif === p.nom : etat.actif === p.nom;
  t.racine.classList.toggle("active", actif);
  t.bouton.setAttribute("aria-pressed", actif);
  t.bouton.setAttribute("aria-label", `${p.nom}, ${t.origine.textContent}${actif ? ", actif" : ""}`);
  t.actions.replaceChildren();
  if (p.origine === "mien") {
    const confirmer = t.confirmer > performance.now();
    const b = el("button", "puce petite" + (confirmer ? " danger" : ""), confirmer ? "Confirmer la suppression" : "Supprimer");
    b.addEventListener("click", () => {
      if (t.confirmer > performance.now()) {
        envoyer({ cmd: "supprimer", nom: p.nom });
        t.confirmer = 0;
      } else {
        t.confirmer = performance.now() + 4000;
        setTimeout(dessinerPresets, 4100);
      }
      dessinerPresets();
    });
    t.actions.append(b);
  } else if (p.origine === "commu") {
    const fait = installe(p.nom);
    const b = el("button", "puce petite", fait ? "Installé ✓" : "+ Installer");
    b.setAttribute("aria-pressed", fait);
    b.disabled = fait;
    b.addEventListener("click", () => envoyer({ cmd: "installer", nom: p.nom, reglages: p.reglages, appliquer: false }));
    t.actions.append(b);
  }
}

function dessinerSources(liste) {
  const sources = [["tout", "Tout"], ["fourni", "Fournis"], ["mien", "Mes presets"], ["commu", "Communauté (GitHub)"]];
  const conteneur = $("sources");
  conteneur.replaceChildren(...sources.map(([id, libelle]) => {
    const b = el("button", "ligne");
    b.setAttribute("aria-pressed", etat.source === id);
    const n = id === "tout" ? liste.length : liste.filter((p) => p.origine === id).length;
    b.append(el("span", "", libelle), el("span", "compte", id === "commu" && etat.commu.etat !== "ok" ? (etat.commu.etat === "erreur" ? "—" : "··") : deux(n)));
    b.addEventListener("click", () => {
      etat.source = id;
      if (id === "commu" || id === "tout") chargerCommu(false);
      dessinerPresets();
    });
    return b;
  }));
}

function dessinerModes() {
  const modes = [["direct", "Direct"], ["survol", "Survol"], ["fige", "Figé"]];
  $("modes").replaceChildren(...modes.map(([id, libelle]) => {
    const b = el("button", "puce", libelle);
    b.setAttribute("aria-pressed", etat.mode === id);
    b.addEventListener("click", () => {
      etat.mode = id;
      memoire.ecrire("mode", id);
      dessinerModes();
    });
    return b;
  }));
  $("mode-aide").textContent = {
    direct: "Toutes les vignettes visibles bougent avec la musique.",
    survol: "Seule la vignette sous la souris bouge.",
    fige: "Aucune vignette ne bouge : le plus léger.",
  }[etat.mode];
}

function dessinerPresets() {
  if (!etat.reglages) return;
  const liste = tousLesPresets();
  dessinerSources(liste);
  const titres = { tout: "Tous les presets", fourni: "Fournis avec Essaim", mien: "Mes presets", commu: "Bibliothèque communautaire" };
  $("titre-grille").textContent = titres[etat.source];
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

  const vus = new Set();
  const enfants = montres.map((p) => {
    let t = tuiles.get(p.id);
    if (!t) { t = creerTuile(p); tuiles.set(p.id, t); }
    vus.add(p.id);
    majTuile(t, p, liste.indexOf(p) + 1);
    return t.racine;
  });
  for (const [id, t] of tuiles) {
    if (!liste.some((p) => p.id === id)) { observateur.unobserve(t.racine); tuiles.delete(id); visibles.delete(id); }
  }
  $("grille").replaceChildren(...enfants);

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
      vide.append(el("span", "zero", "0"), el("span", "t-texte", etat.recherche ? `Aucun preset pour « ${etat.recherche} » ici.` : "Aucun preset ici pour l'instant."));
      const eff = el("button", "puce", "Tout effacer");
      eff.addEventListener("click", () => { etat.recherche = ""; $("recherche").value = ""; etat.source = "tout"; dessinerPresets(); });
      vide.append(eff);
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Réglages

const val = (section, cle) => etat.reglages[section][cle];
const envois = new Map();
function regler(section, cle, valeur) {
  etat.reglages = { ...etat.reglages, [section]: { ...etat.reglages[section], [cle]: valeur } };
  grand.regler(etat.reglages);
  const k = section + "." + cle;
  clearTimeout(envois.get(k));
  envois.set(k, setTimeout(() => { envois.delete(k); envoyer({ cmd: "regler", section, cle, valeur }); }, 60));
}

const NORMAUX = [
  { libelle: "Rayon", section: "orbite", cle: "rayon", min: 4, max: 80, pas: 1, aff: (v) => String(Math.round(v)) },
  { libelle: "Tours / min", section: "orbite", cle: "vitesse_base", min: 0, max: 15, pas: 0.1, aff: (v) => String(Math.round((v * 60) / (2 * Math.PI))) },
  { libelle: "Satellites", section: "essaim", cle: "maximum", min: 0, max: 64, pas: 1, aff: (v) => deux(v) },
  { libelle: "Kick", section: "audio", cle: "sensibilite_kick", min: 1, max: 10, pas: 0.5, aff: (v) => virgule(+v, 1) },
  { libelle: "Éclat", section: "eclat", cle: "taille", min: 0, max: 12, pas: 0.1, aff: (v) => (+v === 0 ? "—" : virgule(+v, 1)) },
];
const EFFETS = [["Comètes", "orbite", "cometes"], ["Fissures", "fissures", "actives"], ["Bordure", "bordure", "active"], ["Réagir au son", "audio", "actif"]];
const TEINTES = [0, 25, 50, 95, 150, 190, 215, 260, 295, 330];
const AVANCES = [
  { groupe: "Orbite" },
  { libelle: "Vitesse ajoutée par la basse", section: "orbite", cle: "vitesse_basse", min: 0, max: 40, pas: 0.5, aff: (v) => virgule(+v, 1) },
  { libelle: "Écart des notes graves", section: "orbite", cle: "eloignement_grave", min: 0, max: 4, pas: 0.1, aff: (v) => "+" + Math.round(v * 100) + " %" },
  { libelle: "Traînée des comètes", section: "orbite", cle: "trainee", min: 0.05, max: 2, pas: 0.05, aff: (v) => virgule(+v, 2) + " s" },
  { groupe: "Satellites" },
  { libelle: "Ajoutés par kick, au moins", section: "essaim", cle: "par_kick_min", min: 0, max: 10, pas: 1, aff: (v) => String(v) },
  { libelle: "Ajoutés par kick, au plus", section: "essaim", cle: "par_kick_max", min: 0, max: 10, pas: 1, aff: (v) => String(v) },
  { libelle: "Durée de vie", section: "essaim", cle: "duree", min: 0.5, max: 10, pas: 0.1, aff: (v) => virgule(+v, 1) + " s" },
  { groupe: "Couleur" },
  { libelle: "Teinte de départ", section: "couleur", cle: "teinte", min: 0, max: 360, pas: 1, aff: (v) => Math.round(v) + "°" },
  { libelle: "Défilement de la teinte", section: "couleur", cle: "vitesse_teinte", min: 0, max: 360, pas: 1, aff: (v) => (+v === 0 ? "fixe" : Math.round(v) + "°/s") },
  { libelle: "Écart tête-queue", section: "couleur", cle: "ecart_teinte", min: 0, max: 360, pas: 1, aff: (v) => Math.round(v) + "°" },
  { groupe: "Fissures et bordure" },
  { libelle: "Taille des fissures", section: "fissures", cle: "taille", min: 0.2, max: 6, pas: 0.1, aff: (v) => virgule(+v, 1) },
  { libelle: "Durée des fissures", section: "fissures", cle: "duree", min: 0.2, max: 15, pas: 0.1, aff: (v) => virgule(+v, 1) + " s" },
  { libelle: "Éclat de la bordure sur le kick", section: "bordure", cle: "intensite", min: 0, max: 1, pas: 0.05, aff: (v) => Math.round(v * 100) + " %" },
  { libelle: "Luminosité de la bordure au repos", section: "bordure", cle: "repos", min: 0, max: 1, pas: 0.05, aff: (v) => Math.round(v * 100) + " %" },
  { groupe: "Son" },
  { libelle: "Écart minimal entre deux kicks", section: "audio", cle: "delai_kick", min: 0.05, max: 0.5, pas: 0.01, aff: (v) => virgule(+v, 2) + " s" },
  { groupe: "Cet ordinateur (hors presets)" },
  { libelle: "Images par seconde", section: "rendu", cle: "cadence", min: 0, max: 240, pas: 1, aff: (v) => (+v === 0 ? "écran" : String(v)) },
  { libelle: "Images par seconde au repos", section: "rendu", cle: "cadence_repos", min: 0, max: 120, pas: 1, aff: (v) => (+v === 0 ? "aucun repos" : String(v)) },
  { bascule: "Dessin par la carte graphique", section: "rendu", cle: "carte_graphique" },
];

const glissieres = []; // { def, input, sortie }
const bascules = []; // { section, cle, bouton }
let teintes = [];

function glissiere(def, id) {
  const input = el("input");
  Object.assign(input, { type: "range", min: def.min, max: def.max, step: def.pas, id });
  input.setAttribute("aria-label", def.libelle);
  return input;
}

function construireReglages() {
  const grille = $("reglages");
  const ligneLarge = (libelle, contenu) => {
    grille.append(el("span", "t-label", libelle));
    const d = el("div", "large");
    d.append(...contenu);
    grille.append(d);
  };
  const bascule = (libelle, section, cle) => {
    const b = el("button", "puce", libelle);
    b.addEventListener("click", () => { regler(section, cle, !val(section, cle)); majReglages(); });
    bascules.push({ section, cle, bouton: b });
    return b;
  };
  ligneLarge("Effets", EFFETS.map(([l, s, c]) => bascule(l, s, c)));
  for (const def of NORMAUX) {
    const input = glissiere(def, "r-" + def.section + "-" + def.cle);
    const sortie = el("span", "valeur t-chiffre");
    const zone = el("div", "glissiere");
    zone.append(input);
    const libelle = el("label", "t-label", def.libelle);
    libelle.htmlFor = input.id;
    grille.append(libelle, zone, sortie);
    input.addEventListener("input", () => { regler(def.section, def.cle, +input.value); sortie.textContent = def.aff(+input.value); majNomActif(); });
    glissieres.push({ def, input, sortie });
    if (def.cle === "vitesse_base") {
      teintes = TEINTES.map((h) => {
        const b = el("button", "teinte");
        const [r, g, bl] = hslCss(h);
        b.style.background = `rgb(${r},${g},${bl})`;
        b.setAttribute("aria-label", "Teinte " + h + "°");
        b.addEventListener("click", () => {
          regler("couleur", "teinte", h);
          regler("couleur", "vitesse_teinte", 0);
          regler("couleur", "synchro_prisme", false);
          majReglages();
        });
        return { h, b };
      });
      ligneLarge("Couleur", [...teintes.map((t) => t.b), (() => {
        const b = el("button", "puce", "Teinte qui défile");
        b.addEventListener("click", () => { regler("couleur", "vitesse_teinte", val("couleur", "vitesse_teinte") > 0 ? 0 : 60); majReglages(); });
        bascules.push({ section: "couleur", cle: "vitesse_teinte", bouton: b });
        return b;
      })(), bascule("Éclairage RGB (Prisme)", "couleur", "synchro_prisme")]);
    }
  }

  const avance = $("avance");
  for (const def of AVANCES) {
    if (def.groupe) {
      const g = el("div", "t-label", def.groupe);
      g.style.gridColumn = "1 / -1";
      g.style.padding = "18px 0 6px";
      avance.append(g);
    } else if (def.bascule) {
      const l = el("label");
      l.append(el("span", "entete-reglage", def.bascule));
      const b = el("button", "puce");
      b.style.alignSelf = "flex-start";
      b.addEventListener("click", () => { regler(def.section, def.cle, !val(def.section, def.cle)); majReglages(); });
      bascules.push({ section: def.section, cle: def.cle, bouton: b, texte: ["Oui", "Non"] });
      l.append(b);
      avance.append(l);
    } else {
      const l = el("label");
      const tete = el("span", "entete-reglage");
      const sortie = el("b");
      tete.append(el("span", "", def.libelle), sortie);
      const input = glissiere(def, "a-" + def.section + "-" + def.cle);
      input.addEventListener("input", () => { regler(def.section, def.cle, +input.value); sortie.textContent = def.aff(+input.value); majNomActif(); });
      l.append(tete, input);
      avance.append(l);
      glissieres.push({ def, input, sortie });
    }
  }
  $("bascule-avance").addEventListener("click", () => { etat.avance = !etat.avance; memoire.ecrire("avance", etat.avance); majAvance(); });
  majAvance();
}

function hslCss(h) {
  const c = 1 - Math.abs(2 * 0.66 - 1), x = c * (1 - Math.abs(((h / 60) % 2) - 1)), m = 0.66 - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
}

function majAvance() {
  $("avance").hidden = !etat.avance;
  $("signe-avance").textContent = etat.avance ? "−" : "+";
  $("bascule-avance").setAttribute("aria-expanded", etat.avance);
}

function majNomActif() {
  const p = etat.presets.find((x) => x.nom === etat.actif);
  $("nom-actif").textContent = p && !envois.size ? `Preset « ${p.nom} »` : "Réglages personnalisés";
}

function majReglages() {
  if (!etat.reglages) return;
  for (const { def, input, sortie } of glissieres) {
    const v = val(def.section, def.cle);
    if (document.activeElement !== input) input.value = v;
    sortie.textContent = def.aff(v);
  }
  for (const b of bascules) {
    const v = val(b.section, b.cle);
    const oui = typeof v === "boolean" ? v : v > 0;
    b.bouton.setAttribute("aria-pressed", oui);
    if (b.texte) b.bouton.textContent = oui ? b.texte[0] : b.texte[1];
  }
  const c = etat.reglages.couleur;
  for (const t of teintes) {
    t.b.setAttribute("aria-pressed", !c.synchro_prisme && c.vitesse_teinte === 0 && Math.abs(c.teinte - t.h) < 3);
  }
  majNomActif();
}

// ---------------------------------------------------------------------------------------------
// Vues, panneau, bandeau

function montrerVue(v) {
  etat.vue = v;
  for (const [id, nom] of [["presets", "presets"], ["reglages", "reglages"]]) {
    $("vue-" + id).hidden = v !== nom;
    $("onglet-" + id).setAttribute("aria-selected", v === nom);
  }
  if (v === "reglages") requestAnimationFrame(() => grand.dimensionner());
}

let minuterieBandeau = 0;
function bandeau(texte) {
  const b = $("bandeau");
  b.textContent = texte + "  ×";
  b.hidden = false;
  clearTimeout(minuterieBandeau);
  minuterieBandeau = setTimeout(() => { b.hidden = true; }, 6000);
}

function ouvrirPanneau() {
  const p = etat.presets.find((x) => x.nom === etat.actif && !x.fourni);
  $("nom-preset").value = p ? p.nom : "";
  $("publier").checked = false;
  $("erreur-panneau").hidden = true;
  $("voile").hidden = false;
  $("nom-preset").focus();
}

function fermerPanneau() {
  $("voile").hidden = true;
}

// ---------------------------------------------------------------------------------------------
// Messages de l'appli

window.essaim = {
  recevoir(m) {
    switch (m.type) {
      case "etat": {
        const premier = !etat.reglages;
        etat.reglages = m.reglages;
        etat.presets = m.presets;
        etat.actif = m.actif;
        etat.depot = m.depot;
        if (premier) {
          construireReglages();
          chargerCommu(false);
        }
        grand.regler(etat.reglages);
        majReglages();
        dessinerPresets();
        break;
      }
      case "son":
        recevoirSon(m);
        break;
      case "enregistre":
        fermerPanneau();
        etat.source = "mien";
        montrerVue("presets");
        dessinerPresets();
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

function boucle(maintenant) {
  const dt = Math.min(0.05, (maintenant - derniere) / 1000);
  derniere = maintenant;
  const son = sonPourApercus(maintenant);
  if (etat.reglages && etat.vue === "reglages") {
    grand.dimensionner();
    grand.avancer(dt, son);
    grand.dessiner();
  }
  cumul += dt;
  if (etat.vue === "presets" && cumul >= 1 / 30) {
    const pas = Math.min(0.1, cumul);
    cumul = 0;
    for (const id of visibles) {
      const t = tuiles.get(id);
      if (!t || !t.p.reglages) continue;
      if (!t.pret) {
        // première image : un peu d'histoire pour que les traînées existent déjà
        t.apercu.dimensionner();
        for (let i = 0; i < 40; i++) t.apercu.avancer(1 / 30, son);
        t.apercu.dessiner();
        t.pret = true;
      }
      const bouge = etat.mode === "direct" || (etat.mode === "survol" && etat.survol === id);
      if (bouge) {
        t.apercu.dimensionner();
        t.apercu.avancer(pas, son);
        t.apercu.dessiner();
      }
    }
  }
  requestAnimationFrame(boucle);
}

// ---------------------------------------------------------------------------------------------
// Démarrage

$("onglet-presets").addEventListener("click", () => montrerVue("presets"));
$("onglet-reglages").addEventListener("click", () => montrerVue("reglages"));
$("recherche").addEventListener("input", (e) => { etat.recherche = e.target.value; dessinerPresets(); });
$("ouvrir-panneau").addEventListener("click", ouvrirPanneau);
$("ouvrir-panneau-2").addEventListener("click", ouvrirPanneau);
$("annuler-panneau").addEventListener("click", fermerPanneau);
$("voile").addEventListener("click", (e) => { if (e.target === $("voile")) fermerPanneau(); });
$("panneau").addEventListener("submit", (e) => {
  e.preventDefault();
  envoyer({ cmd: "enregistrer", nom: $("nom-preset").value, publier: $("publier").checked });
});
$("ouvrir-config").addEventListener("click", () => envoyer({ cmd: "ouvrir_config" }));
$("bandeau").addEventListener("click", () => { $("bandeau").hidden = true; });
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !$("voile").hidden) fermerPanneau();
});
window.addEventListener("resize", () => { for (const t of tuiles.values()) t.pret = false; });

dessinerModes();
montrerVue("presets");
envoyer({ cmd: "etat" });
requestAnimationFrame(boucle);

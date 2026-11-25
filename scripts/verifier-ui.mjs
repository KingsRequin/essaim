// Vérifie les scripts de la fenêtre des réglages (`ui/`), embarqués tels quels dans Essaim : aucune
// commande cargo ne les lit. Ils sont chargés comme scripts classiques dans la même page : un nom
// déclaré au premier niveau dans deux d'entre eux empêche le second de se charger, et la fenêtre
// reste vide, alors que `node --check` passe sur chacun.
//
//   node scripts/verifier-ui.mjs
//
// 1. syntaxe de chaque script (`node --check`) ;
// 2. aucun nom de premier niveau (const, let, var, function, class) déclaré deux fois, dans un
//    même script ou entre deux scripts ;
// 3. messages entre la page et l'appli : chaque `cmd: "…"` envoyé par `ui/app.js` est une
//    variante de `enum Demande` (`src/reglages.rs`), sinon l'appli l'ignore en silence ; chaque
//    `case "…"` de `essaim.recevoir` est un `"type": "…"` envoyé par le Rust (`src/*.rs`), sinon
//    la page attend un message qui ne vient jamais.
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const racine = join(dirname(fileURLToPath(import.meta.url)), "..");
const ui = join(racine, "ui");
// dans l'ordre de chargement de `ui/index.html`
const SCRIPTS = ["lancement.js", "apercu.js", "app.js"];
// types reçus par la page que le Rust n'envoie pas sous la forme `"type": "…"` ou
// `m["type"] = json!("…")` (aucun pour l'instant ; un type écrit autrement s'ajoute ici, avec la
// raison)
const TYPES_HORS_RUST = new Set();

const ligneDe = (texte, index) => texte.slice(0, index).split("\n").length;
let erreurs = 0;
const erreur = (texte) => { erreurs++; console.error(texte); };

for (const s of SCRIPTS) {
  try {
    execFileSync(process.execPath, ["--check", join(ui, s)], { stdio: "pipe" });
  } catch (e) {
    erreur(`${s} : erreur de syntaxe\n${e.stderr}`);
  }
}

// déclarations de premier niveau : en début de ligne, sans indentation
const decl = /^(?:async\s+)?(?:const|let|var|function\*?|class)\s+([\p{L}_$][\p{L}\p{N}_$]*)/gmu;
const vus = new Map(); // nom → [fichier:ligne]
for (const s of SCRIPTS) {
  const texte = readFileSync(join(ui, s), "utf8");
  for (const m of texte.matchAll(decl)) {
    if (!vus.has(m[1])) vus.set(m[1], []);
    vus.get(m[1]).push(`${s}:${ligneDe(texte, m.index)}`);
  }
}
for (const [nom, ou] of vus) {
  if (ou.length > 1) erreur(`« ${nom} » déclaré plusieurs fois au premier niveau : ${ou.join(", ")}`);
}

// --- 3. messages entre la page et l'appli ---------------------------------------------------
const app = readFileSync(join(ui, "app.js"), "utf8");

// commandes : variantes de `enum Demande`, nommées par son `rename_all` ou leur `rename`
const reglages = readFileSync(join(racine, "src", "reglages.rs"), "utf8");
const enumDemande = /#\[serde\(([^\]]*)\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?enum\s+Demande\s*\{([\s\S]*?)\n\}/.exec(reglages);
if (!enumDemande) {
  erreur("src/reglages.rs : `enum Demande` (avec son #[serde(…)]) introuvable : commandes non vérifiées.");
} else {
  const snakeCase = /\brename_all\s*=\s*"snake_case"/.test(enumDemande[1]);
  const snake = (nom) => nom.replace(/(?<=[a-z0-9])([A-Z])/g, "_$1").toLowerCase();
  // variantes : au premier niveau du corps (les champs sont entre accolades), avec un éventuel
  // #[serde(rename = "…")] juste avant
  const demandes = new Set();
  let profondeur = 0, renomme = null;
  for (const ligne of enumDemande[2].split("\n")) {
    const l = ligne.trim();
    if (l.startsWith("//")) continue;
    if (profondeur === 0) {
      const r = /^#\[serde\(.*\brename\s*=\s*"([^"]+)"/.exec(l);
      if (r) renomme = r[1];
      const v = /^([A-Z][A-Za-z0-9]*)\b/.exec(l);
      if (v) {
        demandes.add(renomme || (snakeCase ? snake(v[1]) : v[1]));
        renomme = null;
      }
    }
    profondeur += (l.match(/\{/g) || []).length - (l.match(/\}/g) || []).length;
  }
  for (const m of app.matchAll(/\bcmd:\s*"([^"]*)"/g)) {
    if (!demandes.has(m[1])) {
      erreur(`ui/app.js:${ligneDe(app, m.index)} : commande « ${m[1]} » absente de enum Demande (src/reglages.rs) : l'appli l'ignorerait.`);
    }
  }
}

// messages reçus : chaque `case` de `essaim.recevoir` doit être envoyé par le Rust
const envoyes = new Set(TYPES_HORS_RUST);
const src = join(racine, "src");
for (const f of readdirSync(src, { recursive: true }).map(String).filter((f) => f.endsWith(".rs"))) {
  const texte = readFileSync(join(src, f), "utf8");
  for (const m of texte.matchAll(/"type"\s*:\s*"([^"]+)"|\["type"\]\s*=\s*json!\(\s*"([^"]+)"\s*\)/g)) envoyes.add(m[1] || m[2]);
}
const recevoir = /\brecevoir\s*\(\s*m\s*\)\s*\{([\s\S]*?)\n {2}\},?\n\};/.exec(app);
if (!recevoir) {
  erreur("ui/app.js : `essaim.recevoir(m)` introuvable : messages reçus non vérifiés.");
} else {
  const debut = ligneDe(app, recevoir.index + recevoir[0].indexOf("{"));
  for (const m of recevoir[1].matchAll(/\bcase\s+"([^"]+)"\s*:/g)) {
    if (!envoyes.has(m[1])) {
      erreur(`ui/app.js:${debut + ligneDe(recevoir[1], m.index) - 1} : message « ${m[1]} » attendu, mais aucun "type": "${m[1]}" dans src/*.rs.`);
    }
  }
}

if (erreurs) {
  console.error(`${erreurs} problème(s) dans la fenêtre des réglages.`);
  process.exit(1);
}
console.log(`ui : ${SCRIPTS.length} scripts, syntaxe correcte, aucun nom déclaré deux fois ; commandes et messages connus des deux côtés.`);

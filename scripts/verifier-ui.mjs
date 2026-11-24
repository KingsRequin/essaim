// Vérifie les scripts de la fenêtre des réglages (`ui/`), embarqués tels quels dans Essaim : aucune
// commande cargo ne les lit. Ils sont chargés comme scripts classiques dans la même page : un nom
// déclaré au premier niveau dans deux d'entre eux empêche le second de se charger, et la fenêtre
// reste vide, alors que `node --check` passe sur chacun.
//
//   node scripts/verifier-ui.mjs
//
// 1. syntaxe de chaque script (`node --check`) ;
// 2. aucun nom de premier niveau (const, let, var, function, class) déclaré deux fois, dans un
//    même script ou entre deux scripts.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ui = join(dirname(fileURLToPath(import.meta.url)), "..", "ui");
// dans l'ordre de chargement de `ui/index.html`
const SCRIPTS = ["lancement.js", "apercu.js", "app.js"];

let erreurs = 0;
for (const s of SCRIPTS) {
  try {
    execFileSync(process.execPath, ["--check", join(ui, s)], { stdio: "pipe" });
  } catch (e) {
    erreurs++;
    console.error(`${s} : erreur de syntaxe\n${e.stderr}`);
  }
}

// déclarations de premier niveau : en début de ligne, sans indentation
const decl = /^(?:async\s+)?(?:const|let|var|function\*?|class)\s+([\p{L}_$][\p{L}\p{N}_$]*)/gmu;
const vus = new Map(); // nom → [fichier:ligne]
for (const s of SCRIPTS) {
  const texte = readFileSync(join(ui, s), "utf8");
  for (const m of texte.matchAll(decl)) {
    const ligne = texte.slice(0, m.index).split("\n").length;
    if (!vus.has(m[1])) vus.set(m[1], []);
    vus.get(m[1]).push(`${s}:${ligne}`);
  }
}
for (const [nom, ou] of vus) {
  if (ou.length > 1) {
    erreurs++;
    console.error(`« ${nom} » déclaré plusieurs fois au premier niveau : ${ou.join(", ")}`);
  }
}

if (erreurs) {
  console.error(`${erreurs} problème(s) : la fenêtre des réglages ne se chargerait pas.`);
  process.exit(1);
}
console.log(`ui : ${SCRIPTS.length} scripts, syntaxe correcte, aucun nom déclaré deux fois.`);

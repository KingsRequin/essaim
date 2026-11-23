# Essaim — consignes du projet

Surcouche réactive à la musique autour du pointeur (Rust, Win32 pur, `tiny-skia`, WASAPI
loopback). Lire `docs/technique.md` (fonctionnement) et `docs/guide.md` (fonctions) avant de
modifier. `README.md` (et `README.en.md`, à tenir pareil) est la page de présentation : une
nouvelle fonction visible s'y ajoute en une ligne, le détail va dans `docs/guide.md`. Ses
images se refont avec `scripts/images-readme/rendre.py`. **Le README est mis à jour à chaque
changement** : tout ce qu'un changement rend faux ou inexact (texte, tableau, FAQ, image, texte
alternatif) se corrige dans le même travail, jamais laissé « hors périmètre » ; une image qui ne
montre plus la réalité se refait.

## Vérification (avant de dire « terminé »)

```
cargo test --release
cargo clippy --all-targets -- -D warnings
cargo build --release
node --check ui/app.js; node --check ui/apercu.js; node --check ui/lancement.js
```

Les scripts de `ui/` sont embarqués tels quels : aucune commande cargo ne les lit, et une seule
erreur de syntaxe laisse la fenêtre des réglages vide (0.8.0). Un nouveau `.js` s'ajoute ici.

Lancer cargo depuis le dépôt : `.cargo/config.toml` (local, non suivi) y choisit le dossier de
compilation et remplace `audio-commun` de GitHub par le dossier voisin. Consignes propres au PC
de l'auteur (chemins, exécutable) : `CLAUDE.local.md`. Exécutable :
`<dossier de compilation>\release\essaim.exe` (`cargo metadata` donne le dossier).

## Installation

Essaim est installé (`scripts\install.ps1`, démarrage avec la session) : après une
modification vérifiée, réinstaller avec ce script (il arrête l'instance installée).

## Publication

Un tag `vX.Y.Z` fait compiler le workflow, qui crée la version en brouillon. Elle n'est publiée
qu'une fois signée sur le PC de l'auteur (`pwsh scripts\signer-version.ps1`) : les mises à jour
automatiques refusent un `essaim.exe` sans `essaim.exe.sig` valide (`docs/technique.md`,
« Mises à jour et publication »). Ne jamais mettre de clé privée dans le dépôt ni sur GitHub.

## Essai en vrai

- Fermer l'instance avant de recompiler : `essaim.exe --quitter` (sinon l'exe est verrouillé).
- Vérifier le rendu par capture d'écran : `BitBlt` avec `CAPTUREBLT` (0x40CC0020), sinon les
  fenêtres superposées n'apparaissent pas ; processus en `PER_MONITOR_AWARE_V2`. Avec
  `[rendu] masquer_captures = true`, les captures ne montrent plus l'essaim.
- Fenêtre des réglages : `essaim.exe --reglages` (classe `EssaimReglages`), capture par
  `PrintWindow` avec `PW_RENDERFULLCONTENT` (2), qui rend aussi WebView2. On la pilote par de
  vrais clics (`SetCursorPos` + `mouse_event`) et `SendKeys`.
- Windows masque le curseur pendant la frappe (`CURSOR_SHOWING` absent) : l'essaim se cache
  aussi. Pour le réafficher depuis un script, un `SendInput` de mouvement relatif (structure
  `INPUT` de 40 octets) ; `SetCursorPos` ne suffit pas.
- Un clic simulé doit durer ~60 ms (bouton enfoncé, pause, relâché) : sinon `GetAsyncKeyState`
  le manque et ni l'onde ni les fissures n'apparaissent. Si les clics dans la page n'agissent
  plus, vérifier par `WindowFromPoint` qu'aucune fenêtre d'une autre appli (bulle de SharkApp,
  invisible dans `PrintWindow`) ne couvre le point.
- Le journal (`%APPDATA%\Essaim\essaim.log`) note chaque changement d'état et, toutes les
  minutes, la cadence et le temps de calcul par image.

## Règles

- La bordure colorée de la fenêtre au premier plan doit toujours revenir : tout chemin de sortie
  appelle `bordure::restore()` (fin normale, plantage), et `Border::reset` en pause et plein
  écran.
- Le premier plan ne peut pas être changé par script si un dialogue système (« Sécurité
  Windows », `Shell_SystemDim`) est ouvert : l'essai réel de la bordure
  (`bordure_reelle`, `--ignored`) pilote alors `Border` directement sur le Bloc-notes.
- Le pointeur est celui de Windows : l'overlay ne le dessine jamais (il aurait une image de
  retard), seulement ce qui l'entoure (orbes, éclat du kick, fissures). Essaim ne change le jeu
  de curseurs que sur demande (choix dans les réglages, preset appliqué avec l'option cochée),
  comme le panneau de la souris (`souris.rs`). Avant un essai qui en change, sauvegarder
  `reg export "HKCU\Control Panel\Cursors"` et le réimporter après.
- Réglages : la page (`ui/`) ne touche à aucun fichier ; elle envoie des demandes JSON à
  `reglages.rs`, qui écrit `config.toml` (`config::set_value`) puis `config::request_reload`.
  Un nouveau réglage s'ajoute à `Config`, au fichier par défaut, à `limits` et, pour la fenêtre,
  au panneau de son effet dans `PANNEAUX` de `ui/app.js` (`base` ou `avance`).
- Interface : toute couleur, taille ou police vient des jetons de `ui/style.css` (charte
  « Essaim » : un seul accent `--signal`, tout carré, Archivo). Les aperçus (`ui/apercu.js`)
  reproduisent le calcul de `render.rs` : le modifier là oblige à le reporter ici.
- Le kick (`../audio-commun/src/kick.rs`, bibliothèque partagée) est un modèle appris sur de vrais
  morceaux : tout changement de mesure ou de seuil passe par les tests sur signaux synthétiques
  (caisse claire seule = 0 kick, kicks trouvés avec et sans basse, délai médian < 30 ms) et par le
  banc sur vraie musique (`audio-commun`, README, section « Kick et caisse claire »).
- Aucune fenêtre ne doit bouger ni apparaître sous la souris : Windows redemande alors leur
  curseur aux applications (l'Explorateur répond « flèche »). Tout passe par les calques fixes
  de `layer.rs`.
- Pas de hook global souris/clavier (Defender a déjà bloqué une appli de l'utilisateur pour un
  comportement suspect) : interrogation `GetAsyncKeyState`.

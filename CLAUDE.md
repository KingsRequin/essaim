# Essaim — consignes du projet

Surcouche réactive à la musique autour du pointeur (Rust, Win32 pur, `tiny-skia`, WASAPI
loopback). Lire `README.md` avant de modifier.

## Vérification (avant de dire « terminé »)

```
cargo test --release
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Les compilations vont sur C: (`.cargo/config.toml`) : lancer cargo depuis le dépôt.
Exécutable : `C:\Users\KingsRequin\.cache\essaim-target\release\essaim.exe`.

## Installation

Essaim est installé (`scripts\install.ps1`, démarrage avec la session) : après une
modification vérifiée, réinstaller avec ce script (il arrête l'instance installée).

## Essai en vrai

- Fermer l'instance avant de recompiler : `essaim.exe --quitter` (sinon l'exe est verrouillé).
- Vérifier le rendu par capture d'écran : `BitBlt` avec `CAPTUREBLT` (0x40CC0020), sinon les
  fenêtres superposées n'apparaissent pas ; processus en `PER_MONITOR_AWARE_V2`.
- Fenêtre des réglages : `essaim.exe --reglages` (classe `EssaimReglages`), capture par
  `PrintWindow` avec `PW_RENDERFULLCONTENT` (2), qui rend aussi WebView2. On la pilote par de
  vrais clics (`SetCursorPos` + `mouse_event`) et `SendKeys`.
- Windows masque le curseur pendant la frappe (`CURSOR_SHOWING` absent) : l'essaim se cache
  aussi. Pour le réafficher depuis un script, un `SendInput` de mouvement relatif (structure
  `INPUT` de 40 octets) ; `SetCursorPos` ne suffit pas.
- Le journal (`%APPDATA%\Essaim\essaim.log`) note chaque changement d'état et, toutes les
  minutes, la cadence et le temps de calcul par image.

## Règles

- La bordure colorée de la fenêtre au premier plan doit toujours revenir : tout chemin de sortie
  appelle `bordure::restore()` (fin normale, plantage), et `Border::reset` en pause et plein
  écran.
- Le premier plan ne peut pas être changé par script si un dialogue système (« Sécurité
  Windows », `Shell_SystemDim`) est ouvert : l'essai réel de la bordure
  (`bordure_reelle`, `--ignored`) pilote alors `Border` directement sur le Bloc-notes.
- Essaim ne touche pas aux curseurs : le pointeur est celui de Windows (pack installé par les
  réglages de la souris). L'overlay ne dessine jamais le pointeur (il aurait une image de
  retard), seulement ce qui l'entoure (orbes, éclat du kick, fissures).
- Réglages : la page (`ui/`) ne touche à aucun fichier ; elle envoie des demandes JSON à
  `reglages.rs`, qui écrit `config.toml` (`config::set_value`) puis `config::request_reload`.
  Un nouveau réglage s'ajoute à `Config`, au fichier par défaut, à `limits` et, pour la fenêtre,
  aux listes `NORMAUX` ou `AVANCES` de `ui/app.js`.
- Interface : toute couleur, taille ou police vient des jetons de `ui/style.css` (charte
  « Essaim » : un seul accent `--signal`, tout carré, Archivo). Les aperçus (`ui/apercu.js`)
  reproduisent le calcul de `render.rs` : le modifier là oblige à le reporter ici.
- Le kick (`../audio-commun/src/kick.rs`, bibliothèque partagée) est un modèle appris sur de vrais
  morceaux : tout changement de mesure ou de seuil passe par les tests sur signaux synthétiques
  (caisse claire seule = 0 kick, kicks trouvés avec et sans basse, délai médian < 30 ms) et par le
  banc sur vraie musique (`audio-commun`, README, section « Kick »).
- Aucune fenêtre ne doit bouger ni apparaître sous la souris : Windows redemande alors leur
  curseur aux applications (l'Explorateur répond « flèche »). Tout passe par les calques fixes
  de `layer.rs`.
- Pas de hook global souris/clavier (Defender a déjà bloqué une appli de l'utilisateur pour un
  comportement suspect) : interrogation `GetAsyncKeyState`.

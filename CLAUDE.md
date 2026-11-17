# Essaim — consignes du projet

Curseur animé réactif à la musique (Rust, Win32 pur, `tiny-skia`, WASAPI loopback). Lire
`README.md` avant de modifier.

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
- Le curseur n'apparaît pas dans les captures `BitBlt` : pour voir les curseurs fabriqués,
  dessiner `LoadCursorW(IDC_*)` avec `DrawIconEx` (planche ; la transparence y paraît plus
  sombre qu'à l'écran).
- Windows masque le curseur pendant la frappe (`CURSOR_SHOWING` absent) : l'essaim se cache
  aussi. Pour le réafficher depuis un script, un `SendInput` de mouvement relatif (structure
  `INPUT` de 40 octets) ; `SetCursorPos` ne suffit pas.
- Le journal (`%APPDATA%\Essaim\essaim.log`) note chaque changement d'état et, toutes les
  minutes, la cadence et le temps de calcul par image.

## Règles

- Les curseurs de l'utilisateur doivent toujours revenir : tout chemin de sortie appelle
  `win::restore_cursors()` (fin normale, plantage, plein écran, pause).
- Le pointeur est un vrai curseur Windows (pas de dessin du pointeur dans l'overlay : il
  aurait une image de retard). Seul l'éclat du kick est dessiné sous lui.
- Les seuils du kick (`../audio-commun/src/kick.rs`, bibliothèque partagée) sont validés par les tests sur signaux synthétiques : tout changement
  de seuil ou de filtre passe par ces tests (caisse claire seule = 0 kick, kicks trouvés avec et
  sans basse, délai médian < 30 ms).
- Aucune fenêtre ne doit bouger ni apparaître sous la souris : Windows redemande alors leur
  curseur aux applications (l'Explorateur répond « flèche »). Tout passe par les calques fixes
  de `layer.rs`.
- Pas de hook global souris/clavier (Defender a déjà bloqué une appli de l'utilisateur pour un
  comportement suspect) : interrogation `GetAsyncKeyState`.

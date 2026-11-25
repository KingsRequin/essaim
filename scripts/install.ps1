# Installe (ou met à jour) Essaim pour l'utilisateur courant, sans droits administrateur :
#   - compile en release ;
#   - copie l'exécutable dans %LOCALAPPDATA%\Programs\Essaim ;
#   - lance Essaim à l'ouverture de session (clé Run de HKCU) ;
#   - ajoute au menu Démarrer « Essaim » et « Essaim - réglages ».
# Désinstaller : scripts\uninstall.ps1
param([switch]$NoBuild)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Essaim'
# Cargo lit .cargo/config.toml (dossier de compilation sur C:) depuis le dossier courant.
Push-Location $repo
try {
    $target = (& cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
    if (-not $NoBuild) {
        Write-Host 'Compilation…'
        & cargo build --release
        if ($LASTEXITCODE) { throw 'compilation échouée' }
    }
} finally { Pop-Location }
$built = Join-Path $target 'release\essaim.exe'

# Arrêt propre de toute instance (elle rend sa couleur à la bordure colorée), puis copie.
$installed = Join-Path $dest 'essaim.exe'
& $built --quitter
Wait-Process essaim -Timeout 10 -ErrorAction SilentlyContinue
Get-Process essaim -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item $built $dest -Force

# Démarrage avec la session. `--demarrage` : lancement automatique (une seconde instance lancée
# ainsi sort en silence ; sans lui, un double-clic ouvre les réglages de l'instance en cours).
Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Essaim' -Value "`"$installed`" --demarrage"
# Une coupure faite dans le Gestionnaire des tâches ne survit pas à une réinstallation voulue.
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run' -Name 'Essaim' -ErrorAction SilentlyContinue

# Raccourcis du menu Démarrer.
$programs = [Environment]::GetFolderPath('Programs')
$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut((Join-Path $programs 'Essaim.lnk'))
$lnk.TargetPath = $installed
$lnk.WorkingDirectory = $dest
$lnk.Description = 'Essaim réactif à la musique autour du pointeur'
$lnk.Save()
$lnk = $shell.CreateShortcut((Join-Path $programs 'Essaim - réglages.lnk'))
$lnk.TargetPath = $installed
$lnk.Arguments = '--reglages'
$lnk.WorkingDirectory = $dest
$lnk.Description = "Ouvre la fenêtre des réglages et des profils d'Essaim"
$lnk.Save()
# Restes des versions qui fabriquaient leurs curseurs (le pointeur est désormais celui de Windows).
Remove-Item (Join-Path $programs 'Essaim - restaurer les curseurs.lnk') -ErrorAction SilentlyContinue
Remove-Item (Join-Path $env:APPDATA 'Essaim\curseurs') -Recurse -Force -ErrorAction SilentlyContinue

Start-Process $installed
Write-Host "Essaim installé dans $dest et démarré."

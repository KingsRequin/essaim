# Installe (ou met à jour) Essaim pour l'utilisateur courant, sans droits administrateur :
#   - compile en release ;
#   - copie l'exécutable dans %LOCALAPPDATA%\Programs\Essaim ;
#   - lance Essaim à l'ouverture de session (clé Run de HKCU) ;
#   - ajoute au menu Démarrer « Essaim » et « Essaim - restaurer les curseurs ».
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

# Arrêt propre de toute instance (elle remet les curseurs de l'utilisateur), puis copie.
$installed = Join-Path $dest 'essaim.exe'
& $built --quitter
Wait-Process essaim -Timeout 10 -ErrorAction SilentlyContinue
Get-Process essaim -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item $built $dest -Force

# Démarrage avec la session.
Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Essaim' -Value "`"$installed`""

# Raccourcis du menu Démarrer.
$programs = [Environment]::GetFolderPath('Programs')
$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut((Join-Path $programs 'Essaim.lnk'))
$lnk.TargetPath = $installed
$lnk.WorkingDirectory = $dest
$lnk.Description = 'Curseur animé réactif à la musique'
$lnk.Save()
$lnk = $shell.CreateShortcut((Join-Path $programs 'Essaim - restaurer les curseurs.lnk'))
$lnk.TargetPath = $installed
$lnk.Arguments = '--restaurer-curseur'
$lnk.WorkingDirectory = $dest
$lnk.Description = "Remet les curseurs Windows d'origine (si Essaim a été arrêté de force)"
$lnk.Save()

Start-Process $installed
Write-Host "Essaim installé dans $dest et démarré."

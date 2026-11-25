# Désinstalle Essaim : arrêt (bordure colorée remise), retrait du démarrage avec la session,
# des raccourcis et du dossier d'installation. Les réglages (%APPDATA%\Essaim) sont conservés.
$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Essaim'
$installed = Join-Path $dest 'essaim.exe'
# essaim.exe est une appli fenêtrée : `&` ne l'attendrait pas, Start-Process -Wait si.
if (Test-Path $installed) { Start-Process $installed -ArgumentList '--quitter' -Wait }
Wait-Process essaim -Timeout 10 -ErrorAction SilentlyContinue
Get-Process essaim -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
# Une fois l'instance arrêtée (sinon elle recolorerait la bordure en quittant) : couleur rendue.
if (Test-Path $installed) { Start-Process $installed -ArgumentList '--restaurer-bordure' -Wait }
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Essaim' -ErrorAction SilentlyContinue
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run' -Name 'Essaim' -ErrorAction SilentlyContinue
$programs = [Environment]::GetFolderPath('Programs')
Remove-Item (Join-Path $programs 'Essaim.lnk'), (Join-Path $programs 'Essaim - réglages.lnk'), (Join-Path $programs 'Essaim - restaurer les curseurs.lnk') -ErrorAction SilentlyContinue
Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
# un fichier encore tenu (antivirus, Explorateur ouvert sur le dossier) reste : on le dit
if (Test-Path $dest) {
    Write-Warning "Le dossier $dest n'a pas pu être supprimé entièrement : ferme ce qui l'utilise, puis supprime-le à la main."
} else {
    Write-Host 'Essaim désinstallé (réglages conservés dans %APPDATA%\Essaim).'
}

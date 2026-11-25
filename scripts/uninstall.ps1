# Désinstalle Essaim : arrêt (bordure colorée remise), retrait du démarrage avec la session,
# des raccourcis et du dossier d'installation. Les réglages (%APPDATA%\Essaim) sont conservés.
$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Essaim'
$installed = Join-Path $dest 'essaim.exe'
if (Test-Path $installed) { & $installed --quitter }
Wait-Process essaim -Timeout 10 -ErrorAction SilentlyContinue
Get-Process essaim -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
# Une fois l'instance arrêtée (sinon elle recolorerait la bordure en quittant) : couleur rendue.
if (Test-Path $installed) { & $installed --restaurer-bordure }
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Essaim' -ErrorAction SilentlyContinue
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run' -Name 'Essaim' -ErrorAction SilentlyContinue
$programs = [Environment]::GetFolderPath('Programs')
Remove-Item (Join-Path $programs 'Essaim.lnk'), (Join-Path $programs 'Essaim - réglages.lnk'), (Join-Path $programs 'Essaim - restaurer les curseurs.lnk') -ErrorAction SilentlyContinue
Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
Write-Host 'Essaim désinstallé (réglages conservés dans %APPDATA%\Essaim).'

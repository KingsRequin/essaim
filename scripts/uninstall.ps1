# Désinstalle Essaim : arrêt (curseurs d'origine remis), retrait du démarrage avec la session,
# des raccourcis et du dossier d'installation. Les réglages (%APPDATA%\Essaim) sont conservés.
$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Essaim'
$installed = Join-Path $dest 'essaim.exe'
if (Test-Path $installed) {
    & $installed --quitter
    Wait-Process essaim -Timeout 10 -ErrorAction SilentlyContinue
    & $installed --restaurer-curseur
}
Get-Process essaim -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Essaim' -ErrorAction SilentlyContinue
$programs = [Environment]::GetFolderPath('Programs')
Remove-Item (Join-Path $programs 'Essaim.lnk'), (Join-Path $programs 'Essaim - restaurer les curseurs.lnk') -ErrorAction SilentlyContinue
Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
Write-Host 'Essaim désinstallé (réglages conservés dans %APPDATA%\Essaim).'

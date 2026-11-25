#Requires -Version 7.3
# Crée les deux clés qui signent les versions d'Essaim (une seule fois, sur le PC de l'auteur).
# Les mises à jour automatiques n'installent qu'un essaim.exe signé par l'une d'elles : un
# compte GitHub ou un workflow compromis ne suffit pas à faire passer un faux exécutable.
#
#   principale : %USERPROFILE%\.essaim-signature\principale.cle, chiffrée pour ta session
#                Windows (DPAPI) : illisible depuis un autre compte ou un autre PC.
#   secours    : %USERPROFILE%\.essaim-signature\secours.pem, EN CLAIR : à ranger hors du PC
#                (clé USB, gestionnaire de mots de passe), puis à effacer d'ici. Elle sert si la
#                principale est perdue (PC réinstallé) : signer avec elle une version qui
#                embarque une nouvelle clé principale.
#
# Affiche les clés publiques à mettre dans `CLES` (src/maj.rs).
#   pwsh scripts\creer-cles-signature.ps1 [-Remplacer]
param([switch]$Remplacer)
$ErrorActionPreference = 'Stop'

$dossier = Join-Path $env:USERPROFILE '.essaim-signature'
$principale = Join-Path $dossier 'principale.cle'
$secours = Join-Path $dossier 'secours.pem'
if ((Test-Path $principale) -and -not $Remplacer) {
    throw "Les clés existent déjà ($dossier). -Remplacer en crée d'autres : les versions installées n'accepteront plus que celles signées par les anciennes."
}

New-Item -ItemType Directory -Force $dossier | Out-Null
# accès réservé à l'utilisateur courant
$sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
icacls $dossier /inheritance:r /grant:r "*${sid}:(OI)(CI)F" | Out-Null
if ($LASTEXITCODE) { throw "droits du dossier $dossier non réglés" }

function New-CleSignature {
    [System.Security.Cryptography.ECDsa]::Create([System.Security.Cryptography.ECCurve+NamedCurves]::nistP256)
}

# clé publique : X puis Y (64 octets), forme de `CLES` dans src/maj.rs
function Format-ClePublique([string]$nom, $cle) {
    $q = $cle.ExportParameters($false).Q
    $octets = [byte[]]($q.X + $q.Y)
    "    // $nom`n    [" + (($octets | ForEach-Object { '0x{0:x2}' -f $_ }) -join ', ') + '],'
}

$p = New-CleSignature
$s = New-CleSignature
$pemPrincipale = $p.ExportPkcs8PrivateKeyPem()
Set-Content -NoNewline -Path $principale -Value (ConvertTo-SecureString $pemPrincipale -AsPlainText -Force | ConvertFrom-SecureString)
Set-Content -NoNewline -Path $secours -Value $s.ExportPkcs8PrivateKeyPem()

Write-Output "Clés créées dans $dossier."
Write-Output "Range secours.pem hors du PC puis efface-le d'ici."
Write-Output ''
Write-Output 'Clés publiques pour src/maj.rs (CLES) :'
Format-ClePublique 'principale' $p
Format-ClePublique 'secours' $s

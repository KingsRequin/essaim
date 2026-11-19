# Signe une version qu'a compilée le workflow « Publication » (brouillon), puis la publie : les
# mises à jour automatiques la proposent dès lors à tout le monde.
#
#   pwsh scripts\signer-version.ps1                       # version de Cargo.toml, clé principale
#   pwsh scripts\signer-version.ps1 -Secours E:\secours.pem   # clé de secours (principale perdue)
#
# Contrôles avant de signer : le tag sur GitHub désigne le même commit qu'ici (le code signé est
# celui que tu as), et le brouillon contient essaim.exe. Signature : ECDSA P-256 sur
# « essaim.exe <version> <sha256> », déposée à côté de l'exe (essaim.exe.sig, 64 octets).
param([string]$Version, [string]$Secours)
$ErrorActionPreference = 'Stop'
$depot = 'KingsRequin/essaim'
$racine = Split-Path $PSScriptRoot

if (-not $Version) {
    $ligne = Select-String -Path (Join-Path $racine 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    $Version = $ligne.Matches[0].Groups[1].Value
}
$tag = "v$Version"

# le tag publié est-il le commit d'ici ?
$local = git -C $racine rev-parse "$tag^{commit}"
if ($LASTEXITCODE) { throw "le tag $tag n'existe pas dans ce dépôt" }
# tag annoté : la ligne `^{}` donne le commit ; tag léger : la ligne du tag
$distant =(git -C $racine ls-remote origin "refs/tags/$tag^{}") -split '\s+' | Select-Object -First 1
if (-not $distant) { $distant = (git -C $racine ls-remote origin "refs/tags/$tag") -split '\s+' | Select-Object -First 1 }
if ($distant -ne $local) { throw "le tag $tag de GitHub ($distant) n'est pas le commit d'ici ($local) : rien n'est signé" }

$brouillon = gh release view $tag --repo $depot --json isDraft --jq .isDraft
if ($LASTEXITCODE) { throw "pas de version $tag sur GitHub (le workflow a-t-il fini ?)" }

$temp = Join-Path ([IO.Path]::GetTempPath()) "essaim-signature-$PID"
New-Item -ItemType Directory $temp | Out-Null
try {
    gh release download $tag --repo $depot --pattern essaim.exe --dir $temp
    if ($LASTEXITCODE) { throw "téléchargement de essaim.exe ($tag) impossible" }
    $exe = Join-Path $temp 'essaim.exe'
    $empreinte = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $message = [Text.Encoding]::ASCII.GetBytes("essaim.exe $Version $empreinte")

    if ($Secours) {
        $pem = Get-Content -Raw $Secours
    } else {
        $fichier = Join-Path $env:USERPROFILE '.essaim-signature\principale.cle'
        if (-not (Test-Path $fichier)) { throw "clé principale absente ($fichier) : scripts\creer-cles-signature.ps1, ou -Secours" }
        $pem = ConvertTo-SecureString (Get-Content -Raw $fichier) | ConvertFrom-SecureString -AsPlainText
    }
    $cle = [System.Security.Cryptography.ECDsa]::Create()
    $cle.ImportFromPem($pem)
    $signature = $cle.SignData($message, [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.DSASignatureFormat]::IeeeP1363FixedFieldConcatenation)
    $cle.Dispose()
    $sig = Join-Path $temp 'essaim.exe.sig'
    [IO.File]::WriteAllBytes($sig, $signature)

    gh release upload $tag $sig --repo $depot --clobber
    if ($LASTEXITCODE) { throw "dépôt de la signature impossible" }
    if ($brouillon -eq 'true') {
        gh release edit $tag --repo $depot --draft=false --latest
        if ($LASTEXITCODE) { throw "publication du brouillon impossible" }
        Write-Output "Essaim $Version signé et publié (essaim.exe $empreinte)."
    } else {
        Write-Output "Essaim $Version déjà publié : signature ajoutée (essaim.exe $empreinte)."
    }
} finally {
    Remove-Item -Recurse -Force $temp
}

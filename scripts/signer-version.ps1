# Signe une version et la publie : les mises à jour automatiques la proposent dès lors à tout le
# monde. Le workflow « Publication » a déjà vérifié le tag, passé les tests et créé le brouillon.
#
#   pwsh scripts\signer-version.ps1                           # version de Cargo.toml, clé principale
#   pwsh scripts\signer-version.ps1 -Secours E:\secours.pem   # clé de secours (principale perdue)
#
# Ce qui est signé n'est PAS l'exe compilé par GitHub : un runner, une action ou une dépendance
# compromis là-bas ne doit pas obtenir la signature. Le script vérifie que le tag de GitHub est
# le commit d'ici, compile ce commit sur ce PC dans un dossier propre (sans le `[patch]` local de
# .cargo/config.toml : le code publié est celui du dépôt), signe cet exe et le dépose à la place
# de celui du brouillon, avec essaim.exe.sig (ECDSA P-256 sur « essaim.exe <version> <sha256> »).
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
# (pas de `^{commit}` : PowerShell retire les accolades en passant l'argument à git)
$local = git -C $racine rev-list -n 1 "refs/tags/$tag"
if ($LASTEXITCODE -or -not $local) { throw "le tag $tag n'existe pas dans ce dépôt" }
# tag annoté : la ligne `^{}` donne le commit ; tag léger : la seule ligne du tag
$lignes = @(git -C $racine ls-remote --tags origin | ForEach-Object { , ($_ -split '\s+') } |
    Where-Object { $_[1] -in "refs/tags/$tag", "refs/tags/$tag^{}" })
$ligne = ($lignes | Where-Object { $_[1] -eq "refs/tags/$tag^{}" } | Select-Object -First 1) ?? ($lignes | Select-Object -First 1)
$distant = $ligne ? $ligne[0] : $null
if ($distant -ne $local) { throw "le tag $tag de GitHub ($distant) n'est pas le commit d'ici ($local) : rien n'est signé" }

$brouillon = gh release view $tag --repo $depot --json isDraft --jq .isDraft
if ($LASTEXITCODE) { throw "pas de version $tag sur GitHub (le workflow a-t-il fini ?)" }

# clé lue avant la compilation : une erreur de clé ne fait pas attendre la compilation
if ($Secours) {
    $pem = Get-Content -Raw $Secours
} else {
    $fichier = Join-Path $env:USERPROFILE '.essaim-signature\principale.cle'
    if (-not (Test-Path $fichier)) { throw "clé principale absente ($fichier) : scripts\creer-cles-signature.ps1, ou -Secours" }
    $pem = ConvertTo-SecureString (Get-Content -Raw $fichier) | ConvertFrom-SecureString -AsPlainText
}

$temp = Join-Path ([IO.Path]::GetTempPath()) "essaim-signature-$PID"
$source = Join-Path $temp 'source'
New-Item -ItemType Directory $temp | Out-Null
try {
    # copie propre du commit signé, hors du dépôt : ni fichiers locaux ni .cargo/config.toml
    git -C $racine worktree add --detach $source $local
    if ($LASTEXITCODE) { throw "copie du commit $local impossible" }
    $env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE '.cache\essaim-signature-target'
    Push-Location $source
    try {
        cargo build --release
        if ($LASTEXITCODE) { throw "compilation de $tag impossible" }
    } finally {
        Pop-Location
        Remove-Item Env:CARGO_TARGET_DIR
    }
    $exe = Join-Path $temp 'essaim.exe'
    Copy-Item (Join-Path $env:USERPROFILE '.cache\essaim-signature-target\release\essaim.exe') $exe
    $empreinte = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()

    $cle = [System.Security.Cryptography.ECDsa]::Create()
    $cle.ImportFromPem($pem)
    $message = [Text.Encoding]::ASCII.GetBytes("essaim.exe $Version $empreinte")
    $signature = $cle.SignData($message, [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.DSASignatureFormat]::IeeeP1363FixedFieldConcatenation)
    $cle.Dispose()
    $sig = Join-Path $temp 'essaim.exe.sig'
    [IO.File]::WriteAllBytes($sig, $signature)

    gh release upload $tag $exe $sig --repo $depot --clobber
    if ($LASTEXITCODE) { throw "dépôt de l'exe signé impossible" }
    if ($brouillon -eq 'true') {
        gh release edit $tag --repo $depot --draft=false --latest
        if ($LASTEXITCODE) { throw "publication du brouillon impossible" }
        Write-Output "Essaim $Version compilé ici, signé et publié (essaim.exe $empreinte)."
    } else {
        Write-Output "Essaim $Version déjà publié : exe compilé ici et signature déposés (essaim.exe $empreinte)."
    }
} finally {
    git -C $racine worktree remove --force $source 2>$null
    Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
}

//! Mises à jour automatiques depuis les versions publiées sur GitHub (Releases).
//!
//! Peu après le lancement, puis toutes les six heures, Essaim demande à GitHub la dernière
//! version publiée du dépôt. Si elle est plus récente, il télécharge son `essaim.exe` et en
//! vérifie l'empreinte SHA-256 donnée par GitHub, puis la signature `essaim.exe.sig` : ECDSA
//! P-256 sur « essaim.exe <version> <sha256> », par l'une des clés de `CLES` dont la partie
//! privée n'est jamais sur GitHub (`scripts/signer-version.ps1`). Un compte GitHub ou un
//! workflow compromis ne peut donc pas faire installer un autre exécutable, ni republier un
//! ancien sous un numéro plus grand. Il le met ensuite à la place de l'exécutable
//! en cours : Windows permet de renommer un exécutable lancé, l'ancien devient
//! `essaim.exe.ancien` et sera effacé au lancement suivant. Une bulle prévient au début du
//! téléchargement et quand la version est prête. Elle s'applique au redémarrage (clic sur la
//! bulle, menu de l'icône, fenêtre des réglages, ou prochaine session). La fenêtre des réglages
//! (vue Application) montre où en est la recherche (`Etat`) et peut en demander une tout de suite
//! (`chercher`), même quand la recherche automatique est coupée.
//!
//! Réseau : WinHTTP, fourni avec Windows (proxy du système, certificats du système).

use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

const DEPOT: &str = "KingsRequin/essaim";
/// Nom du fichier attendu parmi ceux de la version publiée.
const NOM_EXE: &str = "essaim.exe";
/// Sa signature, déposée à côté par `scripts/signer-version.ps1`.
const NOM_SIG: &str = "essaim.exe.sig";
/// Clés publiques ECDSA P-256 (X puis Y) qui signent les versions
/// (`scripts/creer-cles-signature.ps1`). La seconde, rangée hors du PC de l'auteur, sert si la
/// première est perdue.
const CLES: [[u8; 64]; 2] = [
    // principale
    [
        0x14, 0x89, 0x33, 0x66, 0x2e, 0x51, 0x1c, 0x91, 0x94, 0xf2, 0x53, 0x7d, 0x4e, 0x12, 0x9f, 0xf5, 0x50, 0xb9, 0x72, 0x35, 0x32, 0x48,
        0xb6, 0xb4, 0x86, 0x82, 0x35, 0xb1, 0x0a, 0x43, 0xa8, 0x95, 0x7b, 0xdf, 0x10, 0x12, 0x89, 0x49, 0x56, 0x1b, 0x4a, 0x1f, 0x9a, 0xde,
        0xbc, 0x59, 0x8c, 0x27, 0x1c, 0x76, 0xec, 0xb1, 0x02, 0x7f, 0x58, 0xfc, 0x0b, 0x0c, 0x14, 0xd6, 0x0b, 0x94, 0x05, 0xb9,
    ],
    // secours
    [
        0x79, 0x8c, 0x66, 0x09, 0x6e, 0x70, 0x42, 0xe1, 0x89, 0x10, 0xb1, 0x07, 0x70, 0xdf, 0xf6, 0x36, 0xb0, 0xc7, 0x0f, 0xb2, 0x7a, 0x46,
        0xad, 0xd6, 0x11, 0x6a, 0x33, 0xcd, 0x25, 0x5d, 0x6a, 0xf4, 0xef, 0x58, 0x8c, 0xf4, 0x76, 0xa5, 0x53, 0x96, 0x93, 0x39, 0x8b, 0xb9,
        0xd3, 0xe4, 0xc2, 0x15, 0x05, 0x4d, 0x75, 0xb5, 0xb1, 0x8c, 0xa0, 0x29, 0x87, 0x00, 0x5d, 0x09, 0x35, 0x8b, 0x6c, 0x04,
    ],
];
const PREMIERE_VERIFICATION: Duration = Duration::from_secs(30);
const INTERVALLE: Duration = Duration::from_secs(6 * 3600);
/// Au-delà, la réponse est refusée (l'exécutable fait quelques Mo).
const TAILLE_MAX: usize = 64 << 20;

/// Où en est la mise à jour, pour la fenêtre des réglages.
#[derive(Clone, Debug, PartialEq)]
pub enum Etat {
    /// Aucune recherche depuis le lancement.
    Jamais,
    Recherche,
    /// Rien de plus récent à la dernière recherche, faite à cette heure.
    AJour(SystemTime),
    /// Version téléchargée et mise en place, en attente d'un redémarrage.
    Prete(String),
    /// La dernière recherche a échoué (pas de réseau, fichier refusé…).
    Echec(String),
}

impl Etat {
    /// Pour la page des réglages : `etat`, et selon le cas `heure` (secondes depuis 1970),
    /// `version` ou `texte`.
    pub fn json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Etat::Jamais => json!({ "etat": "jamais" }),
            Etat::Recherche => json!({ "etat": "recherche" }),
            Etat::AJour(quand) => {
                let heure = quand.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                json!({ "etat": "a_jour", "heure": heure })
            }
            Etat::Prete(version) => json!({ "etat": "prete", "version": version }),
            Etat::Echec(texte) => json!({ "etat": "echec", "texte": texte }),
        }
    }
}

static ACTIF: AtomicBool = AtomicBool::new(true);
static ETAT: Mutex<Etat> = Mutex::new(Etat::Jamais);
/// Une recherche est en cours (automatique ou demandée) : une seule à la fois.
static EN_COURS: AtomicBool = AtomicBool::new(false);
/// Chemin de l'exécutable au lancement (il change de nom une fois la mise à jour en place).
static EXE: OnceLock<PathBuf> = OnceLock::new();

/// Active ou coupe la vérification (réglage `[mise_a_jour] auto`).
pub fn regler(actif: bool) {
    ACTIF.store(actif, Ordering::Relaxed);
}

pub fn etat() -> Etat {
    ETAT.lock().map_or(Etat::Jamais, |e| e.clone())
}

fn noter(etat: Etat) {
    if let Ok(mut e) = ETAT.lock() {
        *e = etat;
    }
}

/// Version prête à être lancée, s'il y en a une.
pub fn prete() -> Option<String> {
    match etat() {
        Etat::Prete(version) => Some(version),
        _ => None,
    }
}

/// Exécutable à relancer pour passer à la nouvelle version.
pub fn exe() -> Option<&'static Path> {
    EXE.get().map(PathBuf::as_path)
}

/// Efface l'ancienne version laissée par une mise à jour et lance la vérification périodique.
pub fn start(actif: bool) {
    regler(actif);
    let Ok(exe) = std::env::current_exe() else { return };
    let _ = std::fs::remove_file(ancien(&exe));
    let _ = std::fs::remove_file(nouveau(&exe));
    let _ = EXE.set(exe.clone());
    // une compilation de travail ne se remplace pas toute seule
    if cfg!(debug_assertions) {
        return;
    }
    let lance = std::thread::Builder::new().name("mise à jour".into()).spawn(move || {
        std::thread::sleep(PREMIERE_VERIFICATION);
        loop {
            if ACTIF.load(Ordering::Relaxed) && reserver() {
                recherche(&exe);
            }
            std::thread::sleep(INTERVALLE);
        }
    });
    if let Err(e) = lance {
        log::warn!("mise à jour : fil impossible à lancer ({e})");
    }
}

/// Cherche une mise à jour tout de suite, à la demande de l'utilisateur (fenêtre des réglages),
/// que la recherche automatique soit active ou non. Le résultat se lit dans `etat`.
pub fn chercher() {
    // une compilation de travail ne se remplace pas, même sur demande
    if cfg!(debug_assertions) {
        return noter(Etat::Echec("compilation de travail, jamais remplacée".into()));
    }
    let Some(exe) = exe() else { return noter(Etat::Echec("exécutable introuvable".into())) };
    // annoncée avant de lancer le fil : la fenêtre des réglages la montre dès le clic
    if !reserver() {
        return;
    }
    if let Err(e) = std::thread::Builder::new().name("mise à jour demandée".into()).spawn(move || recherche(exe)) {
        noter(Etat::Echec(format!("fil impossible à lancer ({e})")));
        EN_COURS.store(false, Ordering::SeqCst);
    }
}

/// Prend la main pour une recherche et l'annonce dans `ETAT`. Refusé si une autre est en cours
/// ou si une version attend déjà le redémarrage.
fn reserver() -> bool {
    if prete().is_some() || EN_COURS.swap(true, Ordering::SeqCst) {
        return false;
    }
    // une recherche a pu finir (version prête) entre la vérification et la réservation
    if prete().is_some() {
        EN_COURS.store(false, Ordering::SeqCst);
        return false;
    }
    noter(Etat::Recherche);
    true
}

/// Tenu pendant le remplacement de l'exécutable : la sortie d'Essaim l'attend
/// (`attendre_remplacement`), pour ne pas laisser l'exécutable à moitié déplacé.
static REMPLACEMENT: Mutex<()> = Mutex::new(());
/// Essaim se ferme : aucun remplacement ne commence plus.
static FERMETURE: AtomicBool = AtomicBool::new(false);

/// À la sortie d'Essaim : plus aucun remplacement ne commence, et celui en cours est attendu
/// `delai` au plus. Faux si le délai est passé avant sa fin.
pub fn attendre_remplacement(delai: Duration) -> bool {
    FERMETURE.store(true, Ordering::SeqCst);
    let fin = std::time::Instant::now() + delai;
    loop {
        match REMPLACEMENT.try_lock() {
            Ok(_) | Err(std::sync::TryLockError::Poisoned(_)) => return true,
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        if std::time::Instant::now() >= fin {
            log::warn!("mise à jour : remplacement de l'exécutable encore en cours à la sortie");
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// La recherche, une fois réservée (`reserver`) : son résultat reste dans `ETAT`.
fn recherche(exe: &Path) {
    noter(match verifier(exe) {
        Ok(Some(version)) => {
            log::info!("mise à jour : version {version} en place, active au redémarrage");
            crate::win::bulle_maj(&format!("Essaim {version} est prêt. Clique ici pour redémarrer (sinon, au prochain démarrage)."));
            Etat::Prete(version)
        }
        Ok(None) => Etat::AJour(SystemTime::now()),
        Err(e) => {
            log::warn!("mise à jour : {e:#}");
            Etat::Echec(format!("{e:#}"))
        }
    });
    EN_COURS.store(false, Ordering::SeqCst);
}

fn ancien(exe: &Path) -> PathBuf {
    exe.with_extension("exe.ancien")
}

fn nouveau(exe: &Path) -> PathBuf {
    exe.with_extension("exe.nouveau")
}

/// Cherche une version plus récente ; si elle existe, la télécharge et la met en place.
/// Renvoie son numéro.
fn verifier(exe: &Path) -> anyhow::Result<Option<String>> {
    let api = format!("https://api.github.com/repos/{DEPOT}/releases/latest");
    let reponse: serde_json::Value = serde_json::from_slice(&telecharger(&api)?).context("réponse de GitHub illisible")?;
    let tag = reponse["tag_name"].as_str().context("version publiée sans numéro")?;
    let version = tag.trim_start_matches('v');
    if !plus_recente(version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let fichier = |nom: &str| {
        reponse["assets"]
            .as_array()
            .and_then(|a| a.iter().find(|f| f["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(nom))))
            .with_context(|| format!("la version {version} ne contient pas {nom}"))
    };
    let adresse = |f: &serde_json::Value| -> anyhow::Result<String> {
        let url = f["browser_download_url"].as_str().context("adresse de téléchargement absente")?;
        if !url.starts_with(&format!("https://github.com/{DEPOT}/releases/download/")) {
            bail!("adresse de téléchargement inattendue : {url}");
        }
        Ok(url.to_owned())
    };
    let (fichier, signature) = (fichier(NOM_EXE)?, fichier(NOM_SIG)?);
    let url = adresse(fichier)?;
    let signature = telecharger(&adresse(signature)?)?;
    let empreinte = fichier["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .context("empreinte SHA-256 absente de la version publiée")?;

    log::info!("mise à jour : téléchargement de la version {version}");
    crate::win::bulle(&format!("Téléchargement de la mise à jour (version {version})…"));
    let octets = telecharger(&url)?;
    if !octets.starts_with(b"MZ") {
        bail!("le fichier téléchargé n'est pas un exécutable");
    }
    let obtenue = hex(&sha256(&octets)?);
    if !obtenue.eq_ignore_ascii_case(empreinte) {
        bail!("empreinte différente de celle publiée ({obtenue} au lieu de {empreinte})");
    }
    if !CLES.iter().any(|cle| signature_valide(cle, &message_signe(version, &obtenue), &signature)) {
        bail!("signature de la version {version} absente ou fausse : mise à jour refusée");
    }
    remplacer(exe, &octets)?;
    Ok(Some(version.to_owned()))
}

/// Attentes entre les essais d'un déplacement de fichier : un antivirus qui examine le nouvel
/// exécutable le garde ouvert quelques instants.
const ATTENTES_DEPLACEMENT: [Duration; 5] =
    [Duration::from_millis(100), Duration::from_millis(250), Duration::from_millis(500), Duration::from_secs(1), Duration::from_secs(2)];

/// Temps laissé au-delà des attentes (écriture et copies de l'exécutable sur un disque lent).
const MARGE_REMPLACEMENT: Duration = Duration::from_secs(5);

/// Durée maximale d'un remplacement en cours (`remplacer_avec`) : au pire quatre séries
/// d'attentes (nouvelle version libre, exécutable renommé, nouvelle version en place, ancienne
/// revenue), plus une marge. La sortie d'Essaim l'attend au plus ce temps-là.
pub fn duree_remplacement_max() -> Duration {
    4 * ATTENTES_DEPLACEMENT.iter().sum::<Duration>() + MARGE_REMPLACEMENT
}

// Déclaré ici : le paquet `windows` n'a pas la partie « Storage_FileSystem » activée.
#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    fn MoveFileExW(existant: *const u16, nouveau: *const u16, options: u32) -> i32;
}
const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

/// Déplace `de` vers `vers` (remplacé s'il existe), écrit sur le disque avant de rendre la main ;
/// réessaie après chacune des `attentes`.
fn deplacer(de: &Path, vers: &Path, attentes: &[Duration]) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let large = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(Some(0)).collect() };
    let (de, vers) = (large(de), large(vers));
    let mut attentes = attentes.iter();
    loop {
        if unsafe { MoveFileExW(de.as_ptr(), vers.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } != 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        match attentes.next() {
            Some(&d) => std::thread::sleep(d),
            None => return Err(e),
        }
    }
}

/// Met `octets` à la place de l'exécutable en cours ; l'ancien reste sous `.ancien`. Chaque
/// déplacement est réessayé (antivirus) ; si la nouvelle version ne peut pas prendre la place
/// libérée, l'ancienne y revient, par copie en dernier recours : il ne manque jamais d'exécutable.
/// Tenu pendant tout le remplacement (`REMPLACEMENT`) ; refusé si Essaim se ferme.
fn remplacer(exe: &Path, octets: &[u8]) -> anyhow::Result<()> {
    let _verrou = REMPLACEMENT.lock().unwrap_or_else(|e| e.into_inner());
    if FERMETURE.load(Ordering::SeqCst) {
        bail!("Essaim se ferme : remplacement remis au prochain lancement");
    }
    remplacer_avec(exe, octets, &ATTENTES_DEPLACEMENT)
}

/// Attend que `p` puisse être ouvert sans partage (un antivirus qui examine le fichier tout juste
/// écrit le garde ouvert), en réessayant après chacune des `attentes`. Faux s'il est encore tenu.
fn attendre_libre(p: &Path, attentes: &[Duration]) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    let mut attentes = attentes.iter();
    loop {
        if std::fs::OpenOptions::new().read(true).share_mode(0).open(p).is_ok() {
            return true;
        }
        match attentes.next() {
            Some(&d) => std::thread::sleep(d),
            None => return false,
        }
    }
}

fn remplacer_avec(exe: &Path, octets: &[u8], attentes: &[Duration]) -> anyhow::Result<()> {
    use std::io::Write;
    let (neuf, vieux) = (nouveau(exe), ancien(exe));
    // un `.nouveau` laissé là (ou un lien placé à sa place) est effacé, puis le fichier est créé
    // à neuf : jamais d'écriture à travers un fichier existant
    let ecrire = || -> std::io::Result<()> {
        match std::fs::remove_file(&neuf) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&neuf)?;
        f.write_all(octets)?;
        f.sync_all()
    };
    if let Err(e) = ecrire() {
        let _ = std::fs::remove_file(&neuf);
        bail!("écriture de {} impossible : {e}", neuf.display());
    }
    // la nouvelle version libre (antivirus fini) avant de toucher à l'exécutable en cours : moins
    // de temps où il n'est plus à sa place. Encore tenue, la suite réessaie quand même.
    if !attendre_libre(&neuf, attentes) {
        log::info!("mise à jour : {} encore ouvert par un autre programme", neuf.display());
    }
    let _ = std::fs::remove_file(&vieux);
    // Windows permet de renommer un exécutable lancé ; tant que ceci échoue, rien n'a bougé
    if let Err(e) = deplacer(exe, &vieux, attentes) {
        let _ = std::fs::remove_file(&neuf);
        bail!("renommage de l'exécutable en cours impossible : {e}");
    }
    let Err(e) = deplacer(&neuf, exe, attentes) else { return Ok(()) };
    // la place est libre : la nouvelle version (déjà vérifiée) par copie, sinon l'ancienne
    if std::fs::copy(&neuf, exe).is_ok() {
        let _ = std::fs::remove_file(&neuf);
        return Ok(());
    }
    let retour = deplacer(&vieux, exe, attentes).or_else(|_| std::fs::copy(&vieux, exe).map(|_| ()));
    let _ = std::fs::remove_file(&neuf);
    match retour {
        Ok(()) => bail!("mise en place de la nouvelle version impossible : {e}"),
        Err(r) => bail!("mise en place de la nouvelle version impossible ({e}), et l'ancienne n'a pas pu revenir ({r}) : elle reste sous {}", vieux.display()),
    }
}

/// `a` est-elle une version plus récente que `b` ? (« 1.10.0 » > « 1.9.2 » ; un suffixe
/// `-beta` ou `+build` est ignoré, un chiffre absent vaut 0).
fn plus_recente(a: &str, b: &str) -> bool {
    let nombres = |v: &str| -> Vec<u64> {
        let coeur = v.trim().split(['-', '+']).next().unwrap_or("");
        coeur.split('.').map(|n| n.parse().unwrap_or(0)).collect()
    };
    let (mut x, mut y) = (nombres(a), nombres(b));
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    x > y
}

/// Empreinte SHA-256 (BCrypt de Windows).
fn sha256(octets: &[u8]) -> anyhow::Result<[u8; 32]> {
    use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
    let mut sortie = [0u8; 32];
    let status = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, octets, &mut sortie) };
    if status.is_err() {
        bail!("calcul de l'empreinte impossible ({:#x})", status.0);
    }
    Ok(sortie)
}

fn hex(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

/// Ce que signe `scripts/signer-version.ps1` : le numéro de version et l'empreinte de l'exe.
fn message_signe(version: &str, empreinte_hex: &str) -> String {
    format!("{NOM_EXE} {version} {}", empreinte_hex.to_ascii_lowercase())
}

/// `signature` (r puis s, 64 octets) est-elle celle de `message` par la clé publique `cle`
/// (X puis Y) ? ECDSA P-256 avec SHA-256, par BCrypt de Windows.
fn signature_valide(cle: &[u8; 64], message: &str, signature: &[u8]) -> bool {
    use windows::Win32::Security::Cryptography::*;
    if signature.len() != 64 {
        return false;
    }
    let Ok(empreinte) = sha256(message.as_bytes()) else { return false };
    let mut blob = Vec::with_capacity(8 + 64);
    blob.extend_from_slice(&BCRYPT_ECDSA_PUBLIC_P256_MAGIC.to_le_bytes());
    blob.extend_from_slice(&32u32.to_le_bytes());
    blob.extend_from_slice(cle);
    unsafe {
        let mut alg = BCRYPT_ALG_HANDLE::default();
        if BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_ECDSA_P256_ALGORITHM, None, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0)).is_err() {
            return false;
        }
        let mut k = BCRYPT_KEY_HANDLE::default();
        let mut bonne = false;
        if BCryptImportKeyPair(alg, None, BCRYPT_ECCPUBLIC_BLOB, &mut k, &blob, 0).is_ok() {
            bonne = BCryptVerifySignature(k, None, &empreinte, signature, BCRYPT_FLAGS(0)).is_ok();
            let _ = BCryptDestroyKey(k);
        }
        let _ = BCryptCloseAlgorithmProvider(alg, 0);
        bonne
    }
}

/// Poignée WinHTTP, refermée en fin de portée.
struct Poignee(*mut std::ffi::c_void);

impl Poignee {
    fn new(h: *mut std::ffi::c_void, quoi: &str) -> anyhow::Result<Self> {
        if h.is_null() {
            bail!("{quoi} : {}", windows::core::Error::from_thread());
        }
        Ok(Self(h))
    }
}

impl Drop for Poignee {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0);
        }
    }
}

/// GET en HTTPS ; renvoie le corps d'une réponse 200 (les redirections sont suivies).
fn telecharger(url: &str) -> anyhow::Result<Vec<u8>> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Networking::WinHttp::*;
    let reste = url.strip_prefix("https://").with_context(|| format!("adresse non sécurisée : {url}"))?;
    let (hote, chemin) = match reste.split_once('/') {
        Some((h, c)) => (h, format!("/{c}")),
        None => (reste, "/".to_owned()),
    };
    unsafe {
        let session = Poignee::new(
            WinHttpOpen(w!("Essaim"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0),
            "ouverture de WinHTTP",
        )?;
        WinHttpSetTimeouts(session.0, 10_000, 10_000, 10_000, 30_000)?;
        let connexion = Poignee::new(WinHttpConnect(session.0, &HSTRING::from(hote), INTERNET_DEFAULT_HTTPS_PORT, 0), "connexion")?;
        let requete = Poignee::new(
            WinHttpOpenRequest(connexion.0, w!("GET"), &HSTRING::from(chemin), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE),
            "requête",
        )?;
        let entetes: Vec<u16> = "Accept: application/vnd.github+json, application/octet-stream\r\n".encode_utf16().collect();
        WinHttpSendRequest(requete.0, Some(&entetes), None, 0, 0, 0).with_context(|| format!("envoi vers {hote}"))?;
        WinHttpReceiveResponse(requete.0, std::ptr::null_mut()).with_context(|| format!("réponse de {hote}"))?;
        let (mut code, mut taille) = (0u32, 4u32);
        WinHttpQueryHeaders(
            requete.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut code as *mut u32 as *mut _),
            &mut taille,
            std::ptr::null_mut(),
        )?;
        if code != 200 {
            bail!("{hote} a répondu {code}");
        }
        let mut corps = Vec::new();
        let mut tampon = vec![0u8; 64 * 1024];
        loop {
            let mut lus = 0u32;
            WinHttpReadData(requete.0, tampon.as_mut_ptr() as *mut _, tampon.len() as u32, &mut lus)?;
            if lus == 0 {
                break;
            }
            corps.extend_from_slice(&tampon[..lus as usize]);
            if corps.len() > TAILLE_MAX {
                bail!("réponse trop grande");
            }
        }
        Ok(corps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_les_versions() {
        assert!(plus_recente("0.2.0", "0.1.0"));
        assert!(plus_recente("1.10.0", "1.9.2"));
        assert!(plus_recente("1.0", "0.9.9"));
        assert!(plus_recente("1.0.1", "1.0"));
        assert!(!plus_recente("0.1.0", "0.1.0"));
        assert!(!plus_recente("0.1.0", "0.2.0"));
        assert!(!plus_recente("1.0.0-beta", "1.0.0"));
        assert!(!plus_recente("n'importe quoi", "0.1.0"));
    }

    #[test]
    fn la_sortie_attend_le_pire_remplacement() {
        // quatre séries d'attentes complètes : 4 × 3,85 s
        assert!(duree_remplacement_max() > Duration::from_millis(15_400), "{:?}", duree_remplacement_max());
    }

    #[test]
    fn etat_pour_la_page() {
        use serde_json::json;
        assert_eq!(Etat::Jamais.json(), json!({ "etat": "jamais" }));
        assert_eq!(Etat::Recherche.json(), json!({ "etat": "recherche" }));
        let quand = SystemTime::UNIX_EPOCH + Duration::from_secs(1_795_000_000);
        assert_eq!(Etat::AJour(quand).json(), json!({ "etat": "a_jour", "heure": 1_795_000_000u64 }));
        assert_eq!(Etat::Prete("9.9.9".into()).json(), json!({ "etat": "prete", "version": "9.9.9" }));
        assert_eq!(Etat::Echec("api.github.com a répondu 403".into()).json(), json!({ "etat": "echec", "texte": "api.github.com a répondu 403" }));
    }

    #[test]
    fn empreinte_sha256() {
        // vecteur de test connu : SHA-256("abc")
        assert_eq!(hex(&sha256(b"abc").unwrap()), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// Clé et signature jetables produites par .NET comme dans `scripts/signer-version.ps1`.
    #[test]
    fn signature_ecdsa() {
        const CLE: [u8; 64] = [
            0x76, 0x70, 0x31, 0x6e, 0x77, 0xea, 0xaf, 0xac, 0xff, 0x25, 0xfc, 0x21, 0x14, 0xb6, 0x12, 0x83, 0x71, 0x98, 0x51, 0xef, 0x3b,
            0xae, 0x34, 0xfc, 0xf8, 0x82, 0x94, 0xf7, 0x5e, 0x2e, 0xb1, 0x16, 0x9a, 0x3a, 0x13, 0xdc, 0x18, 0x5d, 0x1c, 0x3d, 0x4e, 0xa7,
            0x3f, 0xb0, 0x08, 0x6d, 0x09, 0x06, 0x61, 0xcb, 0xf7, 0x18, 0x68, 0xf5, 0xea, 0xc9, 0x8f, 0x10, 0xc7, 0x59, 0xd1, 0x26, 0xe9,
            0xfa,
        ];
        let sig: [u8; 64] = [
            0xdb, 0xdc, 0x13, 0x26, 0xd0, 0x5d, 0x17, 0x82, 0x47, 0x5e, 0x6c, 0x78, 0xaf, 0xe0, 0xc5, 0xaf, 0x22, 0x6c, 0x1c, 0xeb, 0x25,
            0x86, 0x36, 0x60, 0x8d, 0x05, 0xa3, 0xba, 0x2c, 0x7a, 0x65, 0xb3, 0xd7, 0x0c, 0x3a, 0x2d, 0x57, 0x37, 0x86, 0x79, 0x3d, 0xac,
            0xcf, 0x00, 0x83, 0x2a, 0x65, 0xe3, 0x0f, 0xb1, 0x08, 0xa4, 0x4d, 0x9b, 0xf3, 0x95, 0x6f, 0xb0, 0xd5, 0xb4, 0x3a, 0xda, 0x4f,
            0x75,
        ];
        let empreinte = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        assert!(signature_valide(&CLE, &message_signe("9.9.9", empreinte), &sig));
        // autre version, autre exe, signature abîmée ou tronquée, autre clé : refusé
        assert!(!signature_valide(&CLE, &message_signe("9.9.10", empreinte), &sig));
        assert!(!signature_valide(&CLE, &message_signe("9.9.9", &empreinte.replace('A', "B")), &sig));
        let mut abimee = sig;
        abimee[10] ^= 1;
        assert!(!signature_valide(&CLE, &message_signe("9.9.9", empreinte), &abimee));
        assert!(!signature_valide(&CLE, &message_signe("9.9.9", empreinte), &sig[..63]));
        for cle in &CLES {
            assert!(!signature_valide(cle, &message_signe("9.9.9", empreinte), &sig));
        }
    }

    #[test]
    fn noms_des_fichiers_de_passage() {
        let exe = Path::new(r"C:\Essaim\essaim.exe");
        assert_eq!(ancien(exe), Path::new(r"C:\Essaim\essaim.exe.ancien"));
        assert_eq!(nouveau(exe), Path::new(r"C:\Essaim\essaim.exe.nouveau"));
    }

    #[test]
    fn remplace_et_garde_l_ancien() {
        let dossier = std::env::temp_dir().join(format!("essaim-maj-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let exe = dossier.join("essaim.exe");
        std::fs::write(&exe, b"MZ ancienne").unwrap();
        // un `.nouveau` laissé par un essai d'avant est remplacé, pas complété
        std::fs::write(nouveau(&exe), b"reste d'un essai d'avant, plus long que l'exe").unwrap();
        remplacer(&exe, b"MZ nouvelle").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ nouvelle");
        assert_eq!(std::fs::read(ancien(&exe)).unwrap(), b"MZ ancienne");
        assert!(!nouveau(&exe).exists());
        let _ = std::fs::remove_dir_all(&dossier);
    }

    /// Ouvre `p` sans partage (comme un antivirus qui l'examine) pendant `duree`, sur un autre fil.
    fn verrouiller(p: &Path, duree: Duration) -> std::thread::JoinHandle<()> {
        use std::os::windows::fs::OpenOptionsExt;
        let f = std::fs::OpenOptions::new().read(true).share_mode(0).open(p).unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(duree);
            drop(f);
        })
    }

    #[test]
    fn remplacement_reessaye_quand_le_fichier_est_tenu() {
        let dossier = std::env::temp_dir().join(format!("essaim-maj-essais-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let exe = dossier.join("essaim.exe");
        let attentes = [Duration::from_millis(100), Duration::from_millis(200), Duration::from_millis(300)];
        // tenu un instant : un nouvel essai réussit
        std::fs::write(&exe, b"MZ ancienne").unwrap();
        let fil = verrouiller(&exe, Duration::from_millis(150));
        remplacer_avec(&exe, b"MZ nouvelle", &attentes).unwrap();
        fil.join().unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ nouvelle");
        assert_eq!(std::fs::read(ancien(&exe)).unwrap(), b"MZ ancienne");
        // tenu tout du long : refusé, l'exécutable reste intact et rien ne traîne
        std::fs::write(&exe, b"MZ ancienne").unwrap();
        let fil = verrouiller(&exe, Duration::from_secs(2));
        assert!(remplacer_avec(&exe, b"MZ nouvelle", &attentes).is_err());
        fil.join().unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ ancienne");
        assert!(!nouveau(&exe).exists());
        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[test]
    fn attend_que_la_nouvelle_version_soit_libre() {
        let dossier = std::env::temp_dir().join(format!("essaim-maj-libre-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let f = dossier.join("essaim.exe.nouveau");
        std::fs::write(&f, b"MZ").unwrap();
        let attentes = [Duration::from_millis(100), Duration::from_millis(200), Duration::from_millis(300)];
        assert!(attendre_libre(&f, &attentes));
        // tenu un instant (antivirus) : attendu
        let fil = verrouiller(&f, Duration::from_millis(150));
        assert!(attendre_libre(&f, &attentes));
        fil.join().unwrap();
        // tenu tout du long : abandon après les essais
        let fil = verrouiller(&f, Duration::from_secs(2));
        assert!(!attendre_libre(&f, &attentes));
        fil.join().unwrap();
        let _ = std::fs::remove_dir_all(&dossier);
    }

    /// Interroge vraiment GitHub (réseau requis) : `cargo test --release -- --ignored maj_reelle`.
    #[test]
    #[ignore]
    fn maj_reelle() {
        let api = format!("https://api.github.com/repos/{DEPOT}/releases/latest");
        match telecharger(&api) {
            Ok(corps) => {
                let v: serde_json::Value = serde_json::from_slice(&corps).unwrap();
                assert!(v["tag_name"].is_string());
            }
            // aucune version publiée pour l'instant
            Err(e) => assert!(format!("{e:#}").contains("404"), "{e:#}"),
        }
    }
}

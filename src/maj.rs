//! Mises à jour automatiques depuis les versions publiées sur GitHub (Releases).
//!
//! Peu après le lancement, puis toutes les six heures, Essaim demande à GitHub la dernière
//! version publiée du dépôt. Si elle est plus récente, il télécharge son `essaim.exe` et en
//! vérifie l'empreinte SHA-256 donnée par GitHub. Il le met ensuite à la place de l'exécutable
//! en cours : Windows permet de renommer un exécutable lancé, l'ancien devient
//! `essaim.exe.ancien` et sera effacé au lancement suivant. Une bulle prévient au début du
//! téléchargement et quand la version est prête. Elle s'applique au redémarrage (clic sur la
//! bulle, menu de l'icône, ou prochaine session).
//!
//! Réseau : WinHTTP, fourni avec Windows (proxy du système, certificats du système).

use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const DEPOT: &str = "KingsRequin/essaim";
/// Nom du fichier attendu parmi ceux de la version publiée.
const NOM_EXE: &str = "essaim.exe";
const PREMIERE_VERIFICATION: Duration = Duration::from_secs(30);
const INTERVALLE: Duration = Duration::from_secs(6 * 3600);
/// Au-delà, la réponse est refusée (l'exécutable fait quelques Mo).
const TAILLE_MAX: usize = 64 << 20;

static ACTIF: AtomicBool = AtomicBool::new(true);
/// Version téléchargée et mise en place, en attente d'un redémarrage.
static PRETE: Mutex<Option<String>> = Mutex::new(None);
/// Chemin de l'exécutable au lancement (il change de nom une fois la mise à jour en place).
static EXE: OnceLock<PathBuf> = OnceLock::new();

/// Active ou coupe la vérification (réglage `[mise_a_jour] auto`).
pub fn regler(actif: bool) {
    ACTIF.store(actif, Ordering::Relaxed);
}

/// Version prête à être lancée, s'il y en a une.
pub fn prete() -> Option<String> {
    PRETE.lock().ok().and_then(|p| p.clone())
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
            if ACTIF.load(Ordering::Relaxed) && prete().is_none() {
                match verifier(&exe) {
                    Ok(Some(version)) => {
                        log::info!("mise à jour : version {version} en place, active au redémarrage");
                        if let Ok(mut p) = PRETE.lock() {
                            *p = Some(version.clone());
                        }
                        crate::win::bulle_maj(&format!(
                            "Essaim {version} est prêt. Clique ici pour redémarrer (sinon, au prochain démarrage)."
                        ));
                    }
                    Ok(None) => {}
                    Err(e) => log::warn!("mise à jour : {e:#}"),
                }
            }
            std::thread::sleep(INTERVALLE);
        }
    });
    if let Err(e) = lance {
        log::warn!("mise à jour : fil impossible à lancer ({e})");
    }
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
    let fichier = reponse["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|f| f["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(NOM_EXE))))
        .with_context(|| format!("la version {version} ne contient pas {NOM_EXE}"))?;
    let url = fichier["browser_download_url"].as_str().context("adresse de téléchargement absente")?;
    if !url.starts_with(&format!("https://github.com/{DEPOT}/releases/download/")) {
        bail!("adresse de téléchargement inattendue : {url}");
    }
    let empreinte = fichier["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .context("empreinte SHA-256 absente de la version publiée")?;

    log::info!("mise à jour : téléchargement de la version {version}");
    crate::win::bulle(&format!("Téléchargement de la mise à jour (version {version})…"));
    let octets = telecharger(url)?;
    if !octets.starts_with(b"MZ") {
        bail!("le fichier téléchargé n'est pas un exécutable");
    }
    let obtenue = sha256(&octets)?;
    if !obtenue.eq_ignore_ascii_case(empreinte) {
        bail!("empreinte différente de celle publiée ({obtenue} au lieu de {empreinte})");
    }
    remplacer(exe, &octets)?;
    Ok(Some(version.to_owned()))
}

/// Met `octets` à la place de l'exécutable en cours ; l'ancien reste sous `.ancien`.
fn remplacer(exe: &Path, octets: &[u8]) -> anyhow::Result<()> {
    let (neuf, vieux) = (nouveau(exe), ancien(exe));
    std::fs::write(&neuf, octets).with_context(|| format!("écriture de {} impossible", neuf.display()))?;
    let _ = std::fs::remove_file(&vieux);
    if let Err(e) = std::fs::rename(exe, &vieux) {
        let _ = std::fs::remove_file(&neuf);
        bail!("renommage de l'exécutable en cours impossible : {e}");
    }
    if let Err(e) = std::fs::rename(&neuf, exe) {
        let _ = std::fs::rename(&vieux, exe);
        let _ = std::fs::remove_file(&neuf);
        bail!("mise en place de la nouvelle version impossible : {e}");
    }
    Ok(())
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

/// Empreinte SHA-256 en hexadécimal (BCrypt de Windows).
fn sha256(octets: &[u8]) -> anyhow::Result<String> {
    use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
    let mut sortie = [0u8; 32];
    let status = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, octets, &mut sortie) };
    if status.is_err() {
        bail!("calcul de l'empreinte impossible ({:#x})", status.0);
    }
    Ok(sortie.iter().map(|o| format!("{o:02x}")).collect())
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
    fn empreinte_sha256() {
        // vecteur de test connu : SHA-256("abc")
        assert_eq!(sha256(b"abc").unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
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
        remplacer(&exe, b"MZ nouvelle").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"MZ nouvelle");
        assert_eq!(std::fs::read(ancien(&exe)).unwrap(), b"MZ ancienne");
        assert!(!nouveau(&exe).exists());
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

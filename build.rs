//! Intègre l'icône d'Essaim (`ressources/essaim.ico`) à l'exécutable : l'Explorateur, le menu
//! Démarrer et la barre des tâches la montrent (une fenêtre liée à un raccourci y prend l'icône
//! de l'exécutable). Le fichier de ressources (`.res`) est écrit ici, sans outil ni dépendance,
//! puis passé à l'éditeur de liens de MSVC, qui le lit tel quel.
//!
//! Format : une ressource vide d'en-tête, une `RT_ICON` (3) par image de l'icône, puis la
//! `RT_GROUP_ICON` (14) numéro 1 qui les liste.

use std::path::PathBuf;

/// En-tête d'une ressource numérotée (type et nom numériques), suivie de ses données alignées
/// sur 4 octets.
fn ressource(res: &mut Vec<u8>, type_: u16, nom: u16, drapeaux: u16, donnees: &[u8]) {
    let u16le = |v: u16| v.to_le_bytes();
    let u32le = |v: u32| v.to_le_bytes();
    res.extend(u32le(donnees.len() as u32)); // DataSize
    res.extend(u32le(32)); // HeaderSize
    res.extend(u16le(0xFFFF));
    res.extend(u16le(type_));
    res.extend(u16le(0xFFFF));
    res.extend(u16le(nom));
    res.extend(u32le(0)); // DataVersion
    res.extend(u16le(drapeaux)); // MemoryFlags
    res.extend(u16le(0)); // LanguageId : neutre
    res.extend(u32le(0)); // Version
    res.extend(u32le(0)); // Characteristics
    res.extend(donnees);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}

fn main() {
    println!("cargo:rerun-if-changed=ressources/essaim.ico");
    println!("cargo:rerun-if-changed=build.rs");
    // l'éditeur de liens de MSVC lit les .res ; ailleurs (autre cible) : rien à faire
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let ico = std::fs::read("ressources/essaim.ico").expect("ressources/essaim.ico");
    let n = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    assert!(ico[2..4] == [1, 0] && ico.len() >= 6 + 16 * n, "essaim.ico mal formé");
    let mut res = Vec::new();
    // ressource vide d'en-tête (le fichier .res commence ainsi)
    ressource(&mut res, 0, 0, 0, &[]);
    let mut groupe = vec![0, 0, 1, 0];
    groupe.extend((n as u16).to_le_bytes());
    for i in 0..n {
        let e = &ico[6 + 16 * i..6 + 16 * (i + 1)];
        let taille = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        let debut = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
        let id = i as u16 + 1;
        // MOVEABLE | DISCARDABLE
        ressource(&mut res, 3, id, 0x1010, &ico[debut..debut + taille]);
        // entrée du groupe : les 12 premiers octets de celle de l'.ico, puis le numéro
        groupe.extend(&e[..12]);
        groupe.extend(id.to_le_bytes());
    }
    // MOVEABLE | PURE | DISCARDABLE
    ressource(&mut res, 14, 1, 0x1030, &groupe);
    let sortie = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("essaim.res");
    std::fs::write(&sortie, res).expect("écriture de essaim.res");
    println!("cargo:rustc-link-arg-bins={}", sortie.display());
}

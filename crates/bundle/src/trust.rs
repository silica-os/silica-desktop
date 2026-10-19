//! Signing and verifying bundles, the developer CA and the revocation list (silica-os
//! `docs/app-bundles.md` § 4, silica-desktop `docs/spec/trust.md`).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use silica_apps::info::Info;

use crate::SIGNATURE_DIR;
use crate::manifest;
use crate::minisign::{KeyId, PublicKey, SecretKey};

/// The CA keys of the image.
pub const SYSTEM_CA_DIR: &str = "/usr/share/silica/trust/developer-ca";
/// The revocation list of the image.
pub const SYSTEM_REVOKED: &str = "/usr/share/silica/trust/revoked.list";

/// How far a bundle's developer is trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeveloperStatus {
    /// The key is certified by the Silica developer CA.
    Verified,
    SelfSigned,
    Unsigned,
    /// The key is in the revocation list.
    Revoked,
}

impl DeveloperStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DeveloperStatus::Verified => "verified",
            DeveloperStatus::SelfSigned => "self-signed",
            DeveloperStatus::Unsigned => "unsigned",
            DeveloperStatus::Revoked => "revoked",
        }
    }
}

/// Whether the bundle is what its developer signed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Integrity {
    Valid,
    /// No signature at all.
    Unsigned,
    /// Signed, but something differs (the reason).
    Tampered(String),
    /// Not a valid bundle (the reason).
    Invalid(String),
}

/// The result of verifying a bundle.
#[derive(Clone, Debug)]
pub struct Verification {
    pub integrity: Integrity,
    pub developer: DeveloperStatus,
    pub info: Option<Info>,
    pub key_id: Option<KeyId>,
    /// The SHA-256 of `_Signature/manifest` (what approvals are keyed on).
    pub manifest_sha256: Option<String>,
}

impl Verification {
    fn invalid(reason: String) -> Verification {
        Verification {
            integrity: Integrity::Invalid(reason),
            developer: DeveloperStatus::Unsigned,
            info: None,
            key_id: None,
            manifest_sha256: None,
        }
    }
}

/// The CA keys and the revoked key ids.
#[derive(Clone, Debug, Default)]
pub struct Roots {
    pub ca: Vec<PublicKey>,
    pub revoked: HashSet<KeyId>,
}

/// A parsed revocation list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revoked {
    pub serial: u64,
    pub ids: HashSet<KeyId>,
}

/// The text of a revocation list.
pub fn render_revoked(serial: u64, ids: &[KeyId]) -> String {
    let mut out = format!("silica-revoked 1\nserial {serial}\n");
    for id in ids {
        out.push_str(&format!("{id}\n"));
    }
    out
}

/// Parses a revocation list whose signature (`<list>.minisig` text) is valid for one of `ca`.
pub fn parse_revoked(text: &str, signature: &str, ca: &[PublicKey]) -> Option<Revoked> {
    let signed = ca.iter().any(|key| {
        key.verify(text.as_bytes(), signature)
            .is_ok_and(|c| c.starts_with("silica-revoked"))
    });
    if !signed {
        return None;
    }
    let mut lines = text.lines();
    if lines.next() != Some("silica-revoked 1") {
        return None;
    }
    let serial = lines.next()?.strip_prefix("serial ")?.trim().parse().ok()?;
    let ids = lines
        .filter(|l| !l.trim().is_empty())
        .filter_map(KeyId::parse)
        .collect();
    Some(Revoked { serial, ids })
}

impl Roots {
    /// The CA keys in `ca_dir` (`*.pub`) and the newest valid revocation list among
    /// `revocation_files` (each with its `.minisig` next to it).
    pub fn load(ca_dir: &Path, revocation_files: &[PathBuf]) -> Roots {
        let mut ca = Vec::new();
        if let Ok(read) = fs::read_dir(ca_dir) {
            let mut paths: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
            paths.sort();
            for path in paths {
                if path.extension().is_some_and(|e| e == "pub")
                    && let Ok(text) = fs::read_to_string(&path)
                    && let Ok(key) = PublicKey::parse(&text)
                {
                    ca.push(key);
                }
            }
        }
        let revoked = revocation_files
            .iter()
            .filter_map(|file| {
                let text = fs::read_to_string(file).ok()?;
                let mut sig = file.as_os_str().to_owned();
                sig.push(".minisig");
                let signature = fs::read_to_string(PathBuf::from(sig)).ok()?;
                parse_revoked(&text, &signature, &ca)
            })
            .max_by_key(|r| r.serial)
            .map(|r| r.ids)
            .unwrap_or_default();
        Roots { ca, revoked }
    }

    /// The image's roots, with the user's newer revocation list from `state_dir`.
    pub fn system(state_dir: Option<&Path>) -> Roots {
        let mut lists = vec![PathBuf::from(SYSTEM_REVOKED)];
        if let Some(dir) = state_dir {
            lists.push(dir.join("revoked.list"));
        }
        Roots::load(Path::new(SYSTEM_CA_DIR), &lists)
    }
}

/// The trusted comment of a bundle's manifest signature.
pub fn manifest_comment(info: &Info) -> String {
    format!("silica-bundle id={} build={}", info.id, info.build)
}

/// The trusted comment of a developer certificate.
pub fn cert_comment(name: &str) -> String {
    format!("silica-developer name={}", name.replace(['\n', '\r'], " "))
}

/// A developer certificate: the CA's signature over the developer's key line.
pub fn certify(ca: &SecretKey, developer: &PublicKey, name: &str) -> String {
    ca.sign(developer.to_base64().as_bytes(), &cert_comment(name))
}

/// Signs a bundle with `key`: writes `_Signature/` (manifest, signature, public key, and the
/// certificate when given). `Info.toml`'s `developer.key` must be this key.
pub fn sign_bundle(bundle: &Path, key: &SecretKey, cert: Option<&str>) -> Result<(), String> {
    let info = Info::read(bundle).map_err(|e| e.to_string())?;
    let public = key.public();
    if info.developer.key.trim() != public.to_base64() {
        return Err(format!(
            "Info.toml's developer.key is not the signing key (expected {})",
            public.to_base64()
        ));
    }
    let contents = bundle.join("Contents");
    let entries = manifest::scan(&contents)?;
    let text = manifest::render(&entries);
    let dir = contents.join(SIGNATURE_DIR);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let write = |name: &str, text: &str| {
        fs::write(dir.join(name), text).map_err(|e| format!("{}: {e}", dir.join(name).display()))
    };
    write("manifest", &text)?;
    write(
        "manifest.sig",
        &key.sign(text.as_bytes(), &manifest_comment(&info)),
    )?;
    write("developer.pub", &public.to_file())?;
    if let Some(cert) = cert {
        write("developer.cert", cert)?;
    }
    Ok(())
}

/// Verifies a bundle against `roots`.
pub fn verify_bundle(bundle: &Path, roots: &Roots) -> Verification {
    let info = match Info::read(bundle) {
        Ok(info) => info,
        Err(e) => return Verification::invalid(e.to_string()),
    };
    let contents = bundle.join("Contents");
    let dir = contents.join(SIGNATURE_DIR);
    let read = |name: &str| fs::read_to_string(dir.join(name));
    let mut result = Verification {
        integrity: Integrity::Valid,
        developer: DeveloperStatus::Unsigned,
        info: Some(info.clone()),
        key_id: None,
        manifest_sha256: None,
    };
    let (Ok(manifest_text), Ok(signature)) = (read("manifest"), read("manifest.sig")) else {
        result.integrity = Integrity::Unsigned;
        return result;
    };
    result.manifest_sha256 = Some(manifest::sha256(manifest_text.as_bytes()));
    let tampered = |mut r: Verification, why: String| {
        r.integrity = Integrity::Tampered(why);
        r.developer = DeveloperStatus::Unsigned;
        r
    };
    let public = match read("developer.pub").map(|t| PublicKey::parse(&t)) {
        Ok(Ok(key)) => key,
        _ => return tampered(result, "developer.pub is missing or damaged".to_owned()),
    };
    result.key_id = Some(public.id);
    if public.to_base64() != info.developer.key.trim() {
        return tampered(
            result,
            "developer.pub is not Info.toml's developer.key".to_owned(),
        );
    }
    match public.verify(manifest_text.as_bytes(), &signature) {
        Ok(comment) if comment == manifest_comment(&info) => {}
        Ok(_) => {
            return tampered(
                result,
                "the signature is for another app or build".to_owned(),
            );
        }
        Err(e) => return tampered(result, format!("manifest.sig: {e}")),
    }
    let entries = match manifest::parse(&manifest_text) {
        Ok(entries) => entries,
        Err(e) => return tampered(result, e),
    };
    if let Err(why) = manifest::check(&contents, &entries) {
        return tampered(result, why);
    }
    if roots.revoked.contains(&public.id) {
        result.developer = DeveloperStatus::Revoked;
        return result;
    }
    let certified = read("developer.cert").is_ok_and(|cert| {
        roots.ca.iter().any(|ca| {
            ca.verify(public.to_base64().as_bytes(), &cert)
                .is_ok_and(|c| c == cert_comment(&info.developer.name))
        })
    });
    // Signed by a CA key itself (Silica's own apps, signed with the release key).
    let is_ca = roots.ca.iter().any(|ca| ca == &public);
    result.developer = if certified || is_ca {
        DeveloperStatus::Verified
    } else {
        DeveloperStatus::SelfSigned
    };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trust-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn make_bundle(dir: &Path, key: &PublicKey, build: u64) -> PathBuf {
        let bundle = dir.join("Hello.app");
        let c = bundle.join("Contents");
        fs::create_dir_all(c.join("bin")).unwrap();
        fs::write(
            c.join("Info.toml"),
            format!(
                "format = 1\nid = \"dev.example.Hello\"\nname = \"Hello\"\nversion = \"1\"\nbuild = {build}\n\
                 kind = \"native\"\nruntime = \"none\"\nexec = \"bin/hello\"\n\
                 [developer]\nname = \"Example\"\nkey = \"{}\"\n",
                key.to_base64()
            ),
        )
        .unwrap();
        fs::write(c.join("bin/hello"), "#!/bin/sh\necho hi\n").unwrap();
        bundle
    }

    #[test]
    fn signed_bundles_verify_and_tampering_is_seen() {
        let root = tmp("sign");
        let dev = SecretKey::from_seed([1; 32], [1; 8]);
        let bundle = make_bundle(&root, &dev.public(), 1);
        let roots = Roots::default();
        assert_eq!(
            verify_bundle(&bundle, &roots).integrity,
            Integrity::Unsigned
        );
        sign_bundle(&bundle, &dev, None).unwrap();
        let v = verify_bundle(&bundle, &roots);
        assert_eq!(v.integrity, Integrity::Valid);
        assert_eq!(v.developer, DeveloperStatus::SelfSigned);
        assert_eq!(v.key_id, Some(dev.public().id));
        assert!(v.manifest_sha256.is_some());
        // A modified file.
        fs::write(bundle.join("Contents/bin/hello"), "#!/bin/sh\nrm -rf ~\n").unwrap();
        assert!(matches!(
            verify_bundle(&bundle, &roots).integrity,
            Integrity::Tampered(why) if why.contains("bin/hello")
        ));
        fs::write(bundle.join("Contents/bin/hello"), "#!/bin/sh\necho hi\n").unwrap();
        assert_eq!(verify_bundle(&bundle, &roots).integrity, Integrity::Valid);
        // A rebuilt Info.toml (another build) with the old signature.
        let info = bundle.join("Contents/Info.toml");
        let text = fs::read_to_string(&info).unwrap();
        fs::write(&info, text.replace("build = 1", "build = 2")).unwrap();
        assert!(matches!(
            verify_bundle(&bundle, &roots).integrity,
            Integrity::Tampered(_)
        ));
        fs::write(&info, &text).unwrap();
        // Another developer's key put in place of the signer's.
        let evil = SecretKey::from_seed([2; 32], [2; 8]);
        fs::write(
            bundle.join("Contents/_Signature/developer.pub"),
            evil.public().to_file(),
        )
        .unwrap();
        assert!(matches!(
            verify_bundle(&bundle, &roots).integrity,
            Integrity::Tampered(_)
        ));
        // Signing needs Info.toml to name the key.
        assert!(sign_bundle(&bundle, &evil, None).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_ca_verifies_and_revocation_wins() {
        let root = tmp("ca");
        let ca = SecretKey::from_seed([5; 32], [5; 8]);
        let dev = SecretKey::from_seed([6; 32], [6; 8]);
        let bundle = make_bundle(&root, &dev.public(), 3);
        let cert = certify(&ca, &dev.public(), "Example");
        sign_bundle(&bundle, &dev, Some(&cert)).unwrap();
        let ca_dir = root.join("ca");
        fs::create_dir_all(&ca_dir).unwrap();
        fs::write(ca_dir.join("silica.pub"), ca.public().to_file()).unwrap();
        let roots = Roots::load(&ca_dir, &[]);
        assert_eq!(
            verify_bundle(&bundle, &roots).developer,
            DeveloperStatus::Verified
        );
        // A certificate for another name does not count.
        let wrong = certify(&ca, &dev.public(), "Someone Else");
        fs::write(bundle.join("Contents/_Signature/developer.cert"), wrong).unwrap();
        assert_eq!(
            verify_bundle(&bundle, &roots).developer,
            DeveloperStatus::SelfSigned
        );
        // The revocation list, signed by the CA; a newer serial wins, an unsigned one is ignored.
        let lists = root.join("lists");
        fs::create_dir_all(&lists).unwrap();
        let old = render_revoked(1, &[]);
        fs::write(lists.join("a.list"), &old).unwrap();
        fs::write(
            lists.join("a.list.minisig"),
            ca.sign(old.as_bytes(), "silica-revoked serial=1"),
        )
        .unwrap();
        let new = render_revoked(2, &[dev.public().id]);
        fs::write(lists.join("b.list"), &new).unwrap();
        fs::write(
            lists.join("b.list.minisig"),
            ca.sign(new.as_bytes(), "silica-revoked serial=2"),
        )
        .unwrap();
        let roots = Roots::load(&ca_dir, &[lists.join("a.list"), lists.join("b.list")]);
        assert!(roots.revoked.contains(&dev.public().id));
        assert_eq!(
            verify_bundle(&bundle, &roots).developer,
            DeveloperStatus::Revoked
        );
        let forged = render_revoked(9, &[ca.public().id]);
        fs::write(lists.join("c.list"), &forged).unwrap();
        fs::write(
            lists.join("c.list.minisig"),
            dev.sign(forged.as_bytes(), "silica-revoked"),
        )
        .unwrap();
        let roots = Roots::load(&ca_dir, &[lists.join("c.list")]);
        assert!(roots.revoked.is_empty());
        // A bundle signed by a CA key itself is verified (Silica's own apps).
        let own = make_bundle(&root.join("own"), &ca.public(), 1);
        sign_bundle(&own, &ca, None).unwrap();
        assert_eq!(
            verify_bundle(&own, &Roots::load(&ca_dir, &[])).developer,
            DeveloperStatus::Verified
        );
        let _ = fs::remove_dir_all(&root);
    }
}

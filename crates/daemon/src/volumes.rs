//! Disk images: `dev.frascella.Silica.Volumes1` on `/dev/frascella/Silica/Volumes1`
//! (docs/spec/volumes.md). Verifies `.sxv` images, shows their license, mounts them with FUSE in
//! `$XDG_RUNTIME_DIR/silica-volumes/<name>` and ejects them.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use silica_bundle::minisign::PublicKey;
use silica_bundle::sxv::{self, Signature};
use silica_bundle::trust::{DeveloperStatus, Roots, cert_comment};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{fdo, interface};

use crate::trust::{Policy, read_policy};

pub const VOLUMES_PATH: &str = "/dev/frascella/Silica/Volumes1";
const ERROR: &str = "dev.frascella.Silica.Volumes1.Error";
/// The largest license shown.
const MAX_LICENSE: u64 = 256 * 1024;

fn error(kind: &str, text: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::Failed(format!("{ERROR}.{kind}: {text}"))
}

/// Where the service mounts and what it trusts.
#[derive(Clone, Debug)]
pub struct VolumesConfig {
    /// `$XDG_RUNTIME_DIR/silica-volumes`.
    pub dir: PathBuf,
    pub settings_dir: Option<PathBuf>,
    pub ca_dir: PathBuf,
    pub revocation_lists: Vec<PathBuf>,
    /// The FUSE tool for EROFS (`erofsfuse`), None: from `PATH`.
    pub erofsfuse: Option<PathBuf>,
}

impl VolumesConfig {
    pub fn from_env() -> VolumesConfig {
        let runtime = std::env::var("XDG_RUNTIME_DIR")
            .ok()
            .filter(|v| v.starts_with('/'))
            .unwrap_or_else(|| format!("/run/user/{}", rustix::process::getuid().as_raw()));
        let trust = crate::trust::TrustConfig::from_env(None);
        VolumesConfig {
            dir: PathBuf::from(runtime).join("silica-volumes"),
            settings_dir: trust.settings_dir,
            ca_dir: trust.ca_dir,
            revocation_lists: trust.revocation_lists,
            erofsfuse: std::env::var_os("SILICA_EROFSFUSE").map(PathBuf::from),
        }
    }
}

/// An attached volume.
#[derive(Clone, Debug)]
struct Volume {
    image: PathBuf,
    name: String,
    signature: &'static str,
    developer_status: DeveloperStatus,
    key: String,
}

/// The developer status of an image's key and certificate.
pub fn developer_status(key: &PublicKey, cert: Option<&str>, roots: &Roots) -> DeveloperStatus {
    if roots.revoked.contains(&key.id) {
        return DeveloperStatus::Revoked;
    }
    let certified = cert.is_some_and(|cert| {
        roots.ca.iter().any(|ca| {
            ca.verify(key.to_base64().as_bytes(), cert)
                .is_ok_and(|c| c.starts_with(&cert_comment("")))
        })
    });
    if certified || roots.ca.iter().any(|ca| ca == key) {
        DeveloperStatus::Verified
    } else {
        DeveloperStatus::SelfSigned
    }
}

/// The window layout of a mounted volume as `a{sv}`.
fn window_value(mount: &Path) -> HashMap<String, Value<'static>> {
    let text = std::fs::read_to_string(mount.join(sxv::WINDOW_FILE)).unwrap_or_default();
    let window = sxv::parse_window(&text)
        .or_else(|_| sxv::parse_window(""))
        .ok();
    let mut map: HashMap<String, Value<'static>> = HashMap::new();
    let Some(w) = window else {
        return map;
    };
    map.insert("width".to_owned(), Value::from(w.width));
    map.insert("height".to_owned(), Value::from(w.height));
    map.insert("view".to_owned(), Value::from(w.view));
    map.insert("icon_size".to_owned(), Value::from(w.icon_size));
    map.insert("text_size".to_owned(), Value::from(w.text_size));
    let background = w
        .background
        .map(|b| mount.join(".background").join(b))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    map.insert("background".to_owned(), Value::from(background));
    let items: Vec<(String, f64, f64)> = w.items.into_iter().map(|i| (i.name, i.x, i.y)).collect();
    map.insert("items".to_owned(), Value::from(items));
    map
}

struct State {
    attached: BTreeMap<PathBuf, Volume>,
}

/// The bus object.
pub struct Volumes {
    config: VolumesConfig,
    state: Arc<Mutex<State>>,
}

impl Volumes {
    pub fn new(config: VolumesConfig) -> Volumes {
        let volumes = Volumes {
            config,
            state: Arc::new(Mutex::new(State {
                attached: BTreeMap::new(),
            })),
        };
        volumes.adopt();
        volumes
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn roots(&self) -> Roots {
        Roots::load(&self.config.ca_dir, &self.config.revocation_lists)
    }

    fn record_file(&self) -> PathBuf {
        self.config.dir.join(".attached")
    }

    /// Writes the attached list (mount point and image per line) so that a restarted daemon
    /// adopts the mounts it left.
    fn save(&self) {
        let text: String = self
            .state()
            .attached
            .iter()
            .map(|(m, v)| format!("{}\t{}\n", m.display(), v.image.display()))
            .collect();
        let _ = std::fs::create_dir_all(&self.config.dir);
        let _ = std::fs::write(self.record_file(), text);
    }

    /// Adopts the volumes a previous daemon mounted and that are still mounted.
    fn adopt(&self) {
        let Ok(text) = std::fs::read_to_string(self.record_file()) else {
            return;
        };
        let roots = self.roots();
        let mut state = self.state();
        for line in text.lines() {
            let Some((mount, image)) = line.split_once('\t') else {
                continue;
            };
            let (mount, image) = (PathBuf::from(mount), PathBuf::from(image));
            if !mount.starts_with(&self.config.dir) || !sxv::is_mounted(&mount) {
                continue;
            }
            let Ok(found) = sxv::inspect(&image) else {
                continue;
            };
            let (signature, developer_status, key) = describe(&found.signature, &roots);
            state.attached.insert(
                mount,
                Volume {
                    image,
                    name: found.header.name,
                    signature,
                    developer_status,
                    key,
                },
            );
        }
    }

    fn info(&self, mount: &Path, v: &Volume) -> HashMap<String, Value<'static>> {
        let mut map: HashMap<String, Value<'static>> = HashMap::new();
        map.insert("name".to_owned(), Value::from(v.name.clone()));
        map.insert(
            "image".to_owned(),
            Value::from(v.image.to_string_lossy().into_owned()),
        );
        map.insert(
            "mountpoint".to_owned(),
            Value::from(mount.to_string_lossy().into_owned()),
        );
        map.insert("signature".to_owned(), Value::from(v.signature));
        map.insert(
            "developer_status".to_owned(),
            Value::from(v.developer_status.as_str()),
        );
        map.insert("key".to_owned(), Value::from(v.key.clone()));
        map.insert("window".to_owned(), Value::from(window_value(mount)));
        map
    }

    /// A free folder for `name` under the volumes folder (`Name`, `Name 2`…).
    fn free_mountpoint(&self, name: &str) -> PathBuf {
        let state = self.state();
        (1..1000)
            .map(|n| {
                if n == 1 {
                    self.config.dir.join(name)
                } else {
                    self.config.dir.join(format!("{name} {n}"))
                }
            })
            .find(|p| !state.attached.contains_key(p) && !sxv::is_mounted(p))
            .unwrap_or_else(|| {
                self.config
                    .dir
                    .join(format!("{name} {}", std::process::id()))
            })
    }

    /// Reads the license of an image through a short-lived mount.
    fn license_of(&self, image: &Path, header: &sxv::Header) -> Result<String, String> {
        let probe = self
            .config
            .dir
            .join(format!(".inspect-{}", std::process::id()));
        std::fs::create_dir_all(&probe).map_err(|e| e.to_string())?;
        sxv::mount(image, header, &probe, self.config.erofsfuse.as_deref())?;
        let file = probe.join(sxv::LICENSE_FILE);
        let text = match std::fs::metadata(&file) {
            Ok(m) if m.len() <= MAX_LICENSE => std::fs::read_to_string(&file).unwrap_or_default(),
            _ => String::new(),
        };
        let _ = sxv::unmount(&probe);
        let _ = std::fs::remove_dir(&probe);
        Ok(text)
    }
}

fn describe(signature: &Signature, roots: &Roots) -> (&'static str, DeveloperStatus, String) {
    match signature {
        Signature::Valid { key, cert } => (
            "valid",
            developer_status(key, cert.as_deref(), roots),
            key.id.to_string(),
        ),
        Signature::Invalid(_) => ("invalid", DeveloperStatus::Unsigned, String::new()),
        Signature::Unsigned => ("unsigned", DeveloperStatus::Unsigned, String::new()),
    }
}

type VolumeTuple = (String, String, HashMap<String, Value<'static>>);

#[interface(name = "dev.frascella.Silica.Volumes1")]
impl Volumes {
    /// Reads and verifies an image without mounting it (the license through a short mount).
    async fn inspect(&self, image: &str) -> fdo::Result<HashMap<String, Value<'static>>> {
        let path = PathBuf::from(image);
        let found = blocking::unblock({
            let path = path.clone();
            move || sxv::inspect(&path)
        })
        .await
        .map_err(|e| error("Invalid", e))?;
        let (signature, developer_status, key) = describe(&found.signature, &self.roots());
        let license = if found.header.has_license && signature == "valid" {
            self.license_of(&path, &found.header).unwrap_or_default()
        } else {
            String::new()
        };
        let mut map: HashMap<String, Value<'static>> = HashMap::new();
        map.insert("name".to_owned(), Value::from(found.header.name.clone()));
        map.insert("signature".to_owned(), Value::from(signature));
        map.insert(
            "developer_status".to_owned(),
            Value::from(developer_status.as_str()),
        );
        map.insert("key".to_owned(), Value::from(key));
        map.insert("license".to_owned(), Value::from(license));
        map.insert(
            "filesystem".to_owned(),
            Value::from(found.header.filesystem.as_str()),
        );
        Ok(map)
    }

    /// Verifies and mounts an image; returns the mount point. Options: `accept_license` (b).
    async fn attach(
        &self,
        image: &str,
        options: HashMap<String, OwnedValue>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let path = std::fs::canonicalize(image).map_err(|e| error("Invalid", e))?;
        if let Some((mount, _)) = self.state().attached.iter().find(|(_, v)| v.image == path) {
            return Ok(mount.to_string_lossy().into_owned());
        }
        let found = blocking::unblock({
            let path = path.clone();
            move || sxv::inspect(&path)
        })
        .await
        .map_err(|e| error("Invalid", e))?;
        let roots = self.roots();
        let (signature, developer_status, key) = describe(&found.signature, &roots);
        match &found.signature {
            Signature::Invalid(why) => return Err(error("BadSignature", why)),
            Signature::Unsigned
                if read_policy(self.config.settings_dir.as_deref()) != Policy::Anywhere =>
            {
                return Err(error(
                    "Unsigned",
                    "the image is not signed (allowed only with apps from anywhere)",
                ));
            }
            _ => {}
        }
        if developer_status == DeveloperStatus::Revoked {
            return Err(error("Revoked", "the developer's key was revoked"));
        }
        let accepted = options
            .get("accept_license")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        if found.header.has_license && !accepted {
            return Err(error("LicenseRequired", "the license must be accepted"));
        }
        let mount = self.free_mountpoint(&found.header.name);
        std::fs::create_dir_all(&mount).map_err(|e| error("Failed", e))?;
        let tool = self.config.erofsfuse.clone();
        let header = found.header.clone();
        let result = blocking::unblock({
            let (path, mount) = (path.clone(), mount.clone());
            move || sxv::mount(&path, &header, &mount, tool.as_deref())
        })
        .await;
        if let Err(why) = result {
            let _ = std::fs::remove_dir(&mount);
            return Err(error("Failed", why));
        }
        self.state().attached.insert(
            mount.clone(),
            Volume {
                image: path,
                name: found.header.name,
                signature,
                developer_status,
                key,
            },
        );
        self.save();
        let text = mount.to_string_lossy().into_owned();
        let _ = Self::attached(&emitter, &text).await;
        Ok(text)
    }

    /// Ejects a volume.
    async fn detach(
        &self,
        mountpoint: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let mount = PathBuf::from(mountpoint);
        if !self.state().attached.contains_key(&mount) {
            return Err(error("NotAttached", mountpoint));
        }
        let result = blocking::unblock({
            let mount = mount.clone();
            move || sxv::unmount(&mount)
        })
        .await;
        if let Err(why) = result {
            return Err(error("Busy", why));
        }
        let _ = std::fs::remove_dir(&mount);
        self.state().attached.remove(&mount);
        self.save();
        let _ = Self::detached(&emitter, mountpoint).await;
        Ok(())
    }

    /// The attached volumes: mount point, image, info.
    fn list(&self) -> Vec<VolumeTuple> {
        let attached = self.state().attached.clone();
        attached
            .iter()
            .map(|(m, v)| {
                (
                    m.to_string_lossy().into_owned(),
                    v.image.to_string_lossy().into_owned(),
                    self.info(m, v),
                )
            })
            .collect()
    }

    #[zbus(signal)]
    async fn attached(emitter: &SignalEmitter<'_>, mountpoint: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn detached(emitter: &SignalEmitter<'_>, mountpoint: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use silica_bundle::minisign::SecretKey;
    use silica_bundle::trust::certify;

    #[test]
    fn image_developers() {
        let ca = SecretKey::from_seed([1; 32], [1; 8]);
        let dev = SecretKey::from_seed([2; 32], [2; 8]);
        let roots = Roots {
            ca: vec![ca.public()],
            revoked: Default::default(),
        };
        let cert = certify(&ca, &dev.public(), "Example");
        assert_eq!(
            developer_status(&dev.public(), Some(&cert), &roots),
            DeveloperStatus::Verified
        );
        assert_eq!(
            developer_status(&dev.public(), None, &roots),
            DeveloperStatus::SelfSigned
        );
        assert_eq!(
            developer_status(&ca.public(), None, &roots),
            DeveloperStatus::Verified
        );
        let mut revoked = roots.clone();
        revoked.revoked.insert(dev.public().id);
        assert_eq!(
            developer_status(&dev.public(), Some(&cert), &revoked),
            DeveloperStatus::Revoked
        );
    }
}

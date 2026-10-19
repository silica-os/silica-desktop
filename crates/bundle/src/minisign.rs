//! Keys and signatures in the minisign formats (Ed25519): a key made here works with
//! `minisign`, and the other way round. Secret keys are read and written unencrypted (minisign
//! `-W`); signatures are made prehashed (`ED`, BLAKE2b-512) and both kinds are verified.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use blake2::digest::consts::U32;
use blake2::{Blake2b, Blake2b512, Digest};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

const ALG_ED: [u8; 2] = *b"Ed";
const ALG_PREHASHED: [u8; 2] = *b"ED";
const KDF_NONE: [u8; 2] = [0, 0];
const KDF_SCRYPT: [u8; 2] = *b"Sc";
const CHK_BLAKE2: [u8; 2] = *b"B2";

/// Why a key or a signature was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Format(String),
    Encrypted,
    WrongKey,
    BadSignature,
    BadTrustedComment,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Format(what) => write!(f, "not a minisign {what}"),
            Error::Encrypted => write!(
                f,
                "encrypted secret keys are not supported: make one with `silica-bundle keygen` or `minisign -G -W`"
            ),
            Error::WrongKey => write!(f, "signed with another key"),
            Error::BadSignature => write!(f, "the signature does not match"),
            Error::BadTrustedComment => write!(f, "the trusted comment was altered"),
        }
    }
}

/// A key id: 8 bytes, shown as 16 hexadecimal digits (minisign's notation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyId(pub [u8; 8]);

impl std::fmt::Display for KeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016X}", u64::from_le_bytes(self.0))
    }
}

impl KeyId {
    pub fn parse(text: &str) -> Option<KeyId> {
        let value = u64::from_str_radix(text.trim(), 16).ok()?;
        (text.trim().len() == 16).then(|| KeyId(value.to_le_bytes()))
    }
}

/// A public key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey {
    pub id: KeyId,
    key: VerifyingKey,
}

/// A secret key (with its public half).
pub struct SecretKey {
    pub id: KeyId,
    key: SigningKey,
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretKey({})", self.id)
    }
}

/// The base64 line of a file: the first non-comment line.
fn data_line(text: &str, skip: usize) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .nth(skip)
}

fn decode(line: &str, what: &str) -> Result<Vec<u8>, Error> {
    STANDARD
        .decode(line.trim())
        .map_err(|_| Error::Format(what.to_owned()))
}

fn arr<const N: usize>(bytes: &[u8], at: usize, what: &str) -> Result<[u8; N], Error> {
    bytes
        .get(at..at + N)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| Error::Format(what.to_owned()))
}

impl PublicKey {
    /// Parses the base64 line (`RWQ…`) of a public key.
    pub fn from_base64(line: &str) -> Result<PublicKey, Error> {
        let bytes = decode(line, "public key")?;
        if bytes.len() != 42 || bytes.get(..2) != Some(&ALG_ED) {
            return Err(Error::Format("public key".to_owned()));
        }
        let id = KeyId(arr(&bytes, 2, "public key")?);
        let pk: [u8; 32] = arr(&bytes, 10, "public key")?;
        let key =
            VerifyingKey::from_bytes(&pk).map_err(|_| Error::Format("public key".to_owned()))?;
        Ok(PublicKey { id, key })
    }

    /// Parses a public key file (`untrusted comment:` line, then the key), or a bare line.
    pub fn parse(text: &str) -> Result<PublicKey, Error> {
        let first = data_line(text, 0).ok_or_else(|| Error::Format("public key".to_owned()))?;
        let line = if first.starts_with("untrusted comment:") {
            data_line(text, 1).ok_or_else(|| Error::Format("public key".to_owned()))?
        } else {
            first
        };
        PublicKey::from_base64(line)
    }

    /// The base64 line.
    pub fn to_base64(&self) -> String {
        let mut bytes = Vec::with_capacity(42);
        bytes.extend_from_slice(&ALG_ED);
        bytes.extend_from_slice(&self.id.0);
        bytes.extend_from_slice(self.key.as_bytes());
        STANDARD.encode(bytes)
    }

    /// The public key file.
    pub fn to_file(&self) -> String {
        format!(
            "untrusted comment: minisign public key {}\n{}\n",
            self.id,
            self.to_base64()
        )
    }

    /// Verifies a prehashed (`ED`) signature file whose message has the BLAKE2b-512 `digest`
    /// (a large file hashed as it is read); returns its trusted comment.
    pub fn verify_prehashed(&self, digest: &[u8; 64], signature: &str) -> Result<String, Error> {
        let sig = SignatureFile::parse(signature)?;
        if sig.algorithm != ALG_PREHASHED {
            return Err(Error::Format("prehashed signature".to_owned()));
        }
        self.finish(digest, sig)
    }

    /// Verifies a signature file over `message`; returns its trusted comment.
    pub fn verify(&self, message: &[u8], signature: &str) -> Result<String, Error> {
        let sig = SignatureFile::parse(signature)?;
        if sig.algorithm == ALG_PREHASHED {
            let digest: [u8; 64] = Blake2b512::digest(message).into();
            return self.finish(&digest, sig);
        }
        if sig.key_id != self.id {
            return Err(Error::WrongKey);
        }
        let signature = Signature::from_bytes(&sig.signature);
        if self.key.verify(message, &signature).is_err() {
            return Err(Error::BadSignature);
        }
        self.global(&sig)
    }

    /// Checks the prehashed signature, then the trusted comment.
    fn finish(&self, digest: &[u8; 64], sig: SignatureFile) -> Result<String, Error> {
        if sig.key_id != self.id {
            return Err(Error::WrongKey);
        }
        let signature = Signature::from_bytes(&sig.signature);
        if self.key.verify(digest, &signature).is_err() {
            return Err(Error::BadSignature);
        }
        self.global(&sig)
    }

    /// Checks the global signature (over the signature and the trusted comment).
    fn global(&self, sig: &SignatureFile) -> Result<String, Error> {
        let mut global = sig.signature.to_vec();
        global.extend_from_slice(sig.trusted_comment.as_bytes());
        self.key
            .verify(&global, &Signature::from_bytes(&sig.global_signature))
            .map_err(|_| Error::BadTrustedComment)?;
        Ok(sig.trusted_comment.clone())
    }
}

/// A parsed signature file.
struct SignatureFile {
    algorithm: [u8; 2],
    key_id: KeyId,
    signature: [u8; 64],
    trusted_comment: String,
    global_signature: [u8; 64],
}

impl SignatureFile {
    fn parse(text: &str) -> Result<SignatureFile, Error> {
        let what = "signature";
        let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
        let first = lines.next().ok_or_else(|| Error::Format(what.to_owned()))?;
        if !first.starts_with("untrusted comment:") {
            return Err(Error::Format(what.to_owned()));
        }
        let sig = decode(lines.next().unwrap_or_default(), what)?;
        let comment = lines
            .next()
            .and_then(|l| l.strip_prefix("trusted comment: "))
            .ok_or_else(|| Error::Format(what.to_owned()))?
            .to_owned();
        let global = decode(lines.next().unwrap_or_default(), what)?;
        if sig.len() != 74 || global.len() != 64 {
            return Err(Error::Format(what.to_owned()));
        }
        let algorithm: [u8; 2] = arr(&sig, 0, what)?;
        if algorithm != ALG_ED && algorithm != ALG_PREHASHED {
            return Err(Error::Format(what.to_owned()));
        }
        Ok(SignatureFile {
            algorithm,
            key_id: KeyId(arr(&sig, 2, what)?),
            signature: arr(&sig, 10, what)?,
            trusted_comment: comment,
            global_signature: arr(&global, 0, what)?,
        })
    }
}

fn checksum(id: &KeyId, sk: &[u8; 64]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(ALG_ED);
    hasher.update(id.0);
    hasher.update(sk);
    hasher.finalize().into()
}

impl SecretKey {
    /// A new random key.
    pub fn generate() -> Result<SecretKey, String> {
        let mut seed = [0u8; 32];
        let mut id = [0u8; 8];
        getrandom::fill(&mut seed).map_err(|e| format!("random: {e}"))?;
        getrandom::fill(&mut id).map_err(|e| format!("random: {e}"))?;
        Ok(SecretKey {
            id: KeyId(id),
            key: SigningKey::from_bytes(&seed),
        })
    }

    /// A key from a 32-byte seed and an id (tests, reproducible fixtures).
    pub fn from_seed(seed: [u8; 32], id: [u8; 8]) -> SecretKey {
        SecretKey {
            id: KeyId(id),
            key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn public(&self) -> PublicKey {
        PublicKey {
            id: self.id,
            key: self.key.verifying_key(),
        }
    }

    fn sk_bytes(&self) -> [u8; 64] {
        self.key.to_keypair_bytes()
    }

    /// The secret key file (unencrypted).
    pub fn to_file(&self) -> String {
        let sk = self.sk_bytes();
        let mut bytes = Vec::with_capacity(158);
        bytes.extend_from_slice(&ALG_ED);
        bytes.extend_from_slice(&KDF_NONE);
        bytes.extend_from_slice(&CHK_BLAKE2);
        bytes.extend_from_slice(&[0u8; 32 + 8 + 8]);
        bytes.extend_from_slice(&self.id.0);
        bytes.extend_from_slice(&sk);
        bytes.extend_from_slice(&checksum(&self.id, &sk));
        format!(
            "untrusted comment: minisign secret key {} (unencrypted)\n{}\n",
            self.id,
            STANDARD.encode(bytes)
        )
    }

    /// Parses an unencrypted secret key file.
    pub fn parse(text: &str) -> Result<SecretKey, Error> {
        let what = "secret key";
        let first = data_line(text, 0).ok_or_else(|| Error::Format(what.to_owned()))?;
        let line = if first.starts_with("untrusted comment:") {
            data_line(text, 1).ok_or_else(|| Error::Format(what.to_owned()))?
        } else {
            first
        };
        let bytes = decode(line, what)?;
        if bytes.len() != 158 || bytes.get(..2) != Some(&ALG_ED) {
            return Err(Error::Format(what.to_owned()));
        }
        match bytes.get(2..4) {
            Some(kdf) if kdf == KDF_NONE => {}
            Some(kdf) if kdf == KDF_SCRYPT => return Err(Error::Encrypted),
            _ => return Err(Error::Format(what.to_owned())),
        }
        let id = KeyId(arr(&bytes, 54, what)?);
        let sk: [u8; 64] = arr(&bytes, 62, what)?;
        let chk: [u8; 32] = arr(&bytes, 126, what)?;
        // minisign leaves the checksum of an unencrypted key zero.
        if chk != [0; 32] && checksum(&id, &sk) != chk {
            return Err(Error::Format(what.to_owned()));
        }
        let key =
            SigningKey::from_keypair_bytes(&sk).map_err(|_| Error::Format(what.to_owned()))?;
        Ok(SecretKey { id, key })
    }

    /// Signs `message` (prehashed); `trusted_comment` must be one line.
    pub fn sign(&self, message: &[u8], trusted_comment: &str) -> String {
        let digest: [u8; 64] = Blake2b512::digest(message).into();
        self.sign_prehashed(&digest, trusted_comment)
    }

    /// Signs a message by its BLAKE2b-512 `digest` (see [`Hasher`]).
    pub fn sign_prehashed(&self, digest: &[u8; 64], trusted_comment: &str) -> String {
        let comment = trusted_comment.replace(['\n', '\r'], " ");
        let signature = self.key.sign(digest).to_bytes();
        let mut sig = Vec::with_capacity(74);
        sig.extend_from_slice(&ALG_PREHASHED);
        sig.extend_from_slice(&self.id.0);
        sig.extend_from_slice(&signature);
        let mut global = signature.to_vec();
        global.extend_from_slice(comment.as_bytes());
        let global = self.key.sign(&global).to_bytes();
        format!(
            "untrusted comment: signature from silica-bundle secret key\n{}\ntrusted comment: {comment}\n{}\n",
            STANDARD.encode(sig),
            STANDARD.encode(global)
        )
    }
}

/// The BLAKE2b-512 of a message fed in pieces, for [`SecretKey::sign_prehashed`] and
/// [`PublicKey::verify_prehashed`].
#[derive(Default)]
pub struct Hasher(Blake2b512);

impl Hasher {
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn finish(self) -> [u8; 64] {
        self.0.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key pair and a signature made by minisign 0.12 (`minisign -G -W`, then
    /// `minisign -S -m message.txt -t 'hello world'` on the 7 bytes `message`).
    const MINISIGN_PUB: &str = "untrusted comment: minisign public key F958A0852B43AED7\nRWTXrkMrhaBY+Sma75vhd8zGlcIeBYliyl0ACyjpdqcvJ/m8znvdabeb\n";
    const MINISIGN_KEY: &str = "untrusted comment: minisign encrypted secret key\nRWQAAEIyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA165DK4WgWPmN5wD9Gw/gpB2218HCKaylBltSTr+Tfw+z4UPc6cuBayma75vhd8zGlcIeBYliyl0ACyjpdqcvJ/m8znvdabebAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n";
    const MINISIGN_SIG: &str = "untrusted comment: signature from minisign secret key\nRUTXrkMrhaBY+WLJH0qOYVvjhzlyzCQ9BXwN/57zPsx66iJMleTjxfg/RLo+t5RgN9QePLZ5MXeR/duTkrXv5kxck/DzxI7sMQs=\ntrusted comment: hello world\ndxTSwoQd75/KpCYioqpfpC2hd9YTVSNn4F/FgpI8ntWe6qpA2VDfSJGi55nZgNLsGIB9kJPDxAr9g2VfoE+6Cg==\n";

    #[test]
    fn keys_round_trip() {
        let key = SecretKey::from_seed([7; 32], [1, 2, 3, 4, 5, 6, 7, 8]);
        let back = SecretKey::parse(&key.to_file()).unwrap();
        assert_eq!(back.id, key.id);
        assert_eq!(back.public(), key.public());
        let public = PublicKey::parse(&key.public().to_file()).unwrap();
        assert_eq!(public, key.public());
        assert_eq!(PublicKey::from_base64(&public.to_base64()).unwrap(), public);
        assert_eq!(key.id.to_string(), "0807060504030201");
        assert_eq!(KeyId::parse("0807060504030201"), Some(key.id));
        // A damaged secret key fails its checksum.
        let mut text = key.to_file();
        let at = text.len() - 10;
        text.replace_range(at..at + 1, if &text[at..at + 1] == "A" { "B" } else { "A" });
        assert!(SecretKey::parse(&text).is_err());
    }

    #[test]
    fn signatures_verify_and_refuse() {
        let key = SecretKey::from_seed([9; 32], [8; 8]);
        let other = SecretKey::from_seed([10; 32], [9; 8]);
        let sig = key.sign(b"message", "silica-bundle id=a build=1");
        assert_eq!(
            key.public().verify(b"message", &sig).unwrap(),
            "silica-bundle id=a build=1"
        );
        assert_eq!(
            key.public().verify(b"other", &sig),
            Err(Error::BadSignature)
        );
        assert_eq!(
            other.public().verify(b"message", &sig),
            Err(Error::WrongKey)
        );
        let altered = sig.replace("build=1", "build=9");
        assert_eq!(
            key.public().verify(b"message", &altered),
            Err(Error::BadTrustedComment)
        );
        assert!(key.public().verify(b"message", "garbage").is_err());
    }

    #[test]
    fn interoperates_with_minisign() {
        let public = PublicKey::parse(MINISIGN_PUB).unwrap();
        assert_eq!(public.id.to_string(), "F958A0852B43AED7");
        assert_eq!(
            public.verify(b"message", MINISIGN_SIG).unwrap(),
            "hello world"
        );
        assert_eq!(
            public.verify(b"messagE", MINISIGN_SIG),
            Err(Error::BadSignature)
        );
        // minisign's unencrypted secret key (it leaves the checksum zero) signs for its public
        // key, and our file of it reads back.
        let secret = SecretKey::parse(MINISIGN_KEY).unwrap();
        assert_eq!(secret.public(), public);
        let sig = secret.sign(b"ours", "t");
        assert_eq!(public.verify(b"ours", &sig).unwrap(), "t");
        assert_eq!(
            SecretKey::parse(&secret.to_file()).unwrap().public(),
            public
        );
    }

    #[test]
    fn legacy_signatures_verify() {
        // A non-prehashed ("Ed") signature, as older minisign makes.
        let key = SecretKey::from_seed([3; 32], [4; 8]);
        let raw = key.key.sign(b"legacy").to_bytes();
        let mut sig = Vec::new();
        sig.extend_from_slice(&ALG_ED);
        sig.extend_from_slice(&key.id.0);
        sig.extend_from_slice(&raw);
        let mut global = raw.to_vec();
        global.extend_from_slice(b"t");
        let global = key.key.sign(&global).to_bytes();
        let text = format!(
            "untrusted comment: x\n{}\ntrusted comment: t\n{}\n",
            STANDARD.encode(sig),
            STANDARD.encode(global)
        );
        assert_eq!(key.public().verify(b"legacy", &text).unwrap(), "t");
    }
}

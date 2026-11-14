//! Model files: what to download, where it lives and how it is verified.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

/// One file of a model, pinned by size and SHA-256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

/// A downloadable model, pinned to an exact upstream revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    /// Directory name under the models root.
    pub id: &'static str,
    pub base_url: &'static str,
    pub files: &'static [ModelFile],
}

impl ModelSpec {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    pub fn url(&self, file: &ModelFile) -> String {
        format!("{}/{}", self.base_url, file.name)
    }
}

/// NVIDIA Parakeet TDT 0.6B v3 (25 European languages incl. Portuguese),
/// int8-quantized ONNX export by istupakov — ~670 MB on disk.
pub const PARAKEET_TDT_V3_INT8: ModelSpec = ModelSpec {
    id: "parakeet-tdt-0.6b-v3-int8",
    base_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce",
    files: &[
        ModelFile {
            name: "vocab.txt",
            size: 93_939,
            sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
        },
        ModelFile {
            name: "decoder_joint-model.int8.onnx",
            size: 18_202_004,
            sha256: "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
        },
        ModelFile {
            name: "encoder-model.int8.onnx",
            size: 652_183_999,
            sha256: "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
        },
    ],
};

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("falha ao baixar {file}: {source}")]
    Network { file: String, source: io::Error },
    #[error("erro de disco em {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{file} chegou corrompido (checksum inválido)")]
    Checksum { file: String },
    #[error("{file} tem tamanho inesperado ({actual} bytes, esperado {expected})")]
    Size {
        file: String,
        actual: u64,
        expected: u64,
    },
}

/// Opens a byte stream for a URL. Abstracted so the installer is testable offline.
pub trait Fetch {
    fn open(&self, url: &str) -> io::Result<Box<dyn Read + Send>>;
}

/// Production fetcher over HTTPS.
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpFetch;

impl Fetch for HttpFetch {
    fn open(&self, url: &str) -> io::Result<Box<dyn Read + Send>> {
        let response = ureq::get(url).call().map_err(io::Error::other)?;
        Ok(Box::new(response.into_body().into_reader()))
    }
}

/// Where a model lives on disk.
#[derive(Debug, Clone)]
pub struct ModelStore {
    spec: ModelSpec,
    dir: PathBuf,
}

impl ModelStore {
    pub fn new(models_root: &Path, spec: ModelSpec) -> Self {
        Self {
            dir: models_root.join(spec.id),
            spec,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub const fn spec(&self) -> &ModelSpec {
        &self.spec
    }

    /// Cheap check (sizes only). Full hashes are verified at download time.
    pub fn is_installed(&self) -> bool {
        self.spec
            .files
            .iter()
            .all(|f| fs::metadata(self.dir.join(f.name)).is_ok_and(|m| m.len() == f.size))
    }

    /// Downloads every missing file, verifying size and SHA-256. `progress`
    /// receives `(bytes_done, bytes_total)` across all files.
    pub fn install(
        &self,
        fetch: &dyn Fetch,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<(), InstallError> {
        fs::create_dir_all(&self.dir).map_err(|source| InstallError::Io {
            path: self.dir.clone(),
            source,
        })?;
        let total = self.spec.total_size();
        let mut done = 0u64;
        for file in self.spec.files {
            let target = self.dir.join(file.name);
            if fs::metadata(&target).is_ok_and(|m| m.len() == file.size) {
                done += file.size;
                progress(done, total);
                continue;
            }
            let base = done;
            self.download_one(fetch, file, &target, |n| progress(base + n, total))?;
            done += file.size;
        }
        Ok(())
    }

    fn download_one(
        &self,
        fetch: &dyn Fetch,
        file: &ModelFile,
        target: &Path,
        mut progress: impl FnMut(u64),
    ) -> Result<(), InstallError> {
        let network = |source| InstallError::Network {
            file: file.name.to_owned(),
            source,
        };
        let part = target.with_extension("part");
        let io_err = |source| InstallError::Io {
            path: part.clone(),
            source,
        };

        let mut reader = fetch.open(&self.spec.url(file)).map_err(network)?;
        let mut out = File::create(&part).map_err(io_err)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut written = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(network)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(io_err)?;
            hasher.update(&buf[..n]);
            written += n as u64;
            progress(written);
        }
        out.sync_all().map_err(io_err)?;
        drop(out);

        let result = if written != file.size {
            Err(InstallError::Size {
                file: file.name.to_owned(),
                actual: written,
                expected: file.size,
            })
        } else if hex(&hasher.finalize()) != file.sha256 {
            Err(InstallError::Checksum {
                file: file.name.to_owned(),
            })
        } else {
            fs::rename(&part, target).map_err(io_err)
        };
        if result.is_err() {
            let _ = fs::remove_file(&part);
        }
        result
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Serves fixed bodies by URL and counts requests.
    #[derive(Default)]
    struct MemoryFetch {
        bodies: HashMap<String, Vec<u8>>,
        requests: Mutex<Vec<String>>,
    }

    impl Fetch for MemoryFetch {
        fn open(&self, url: &str) -> io::Result<Box<dyn Read + Send>> {
            self.requests.lock().unwrap().push(url.to_owned());
            self.bodies
                .get(url)
                .map(|b| Box::new(io::Cursor::new(b.clone())) as Box<dyn Read + Send>)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "404"))
        }
    }

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    /// Builds a spec whose files are the given bodies (leaks: test-only, tiny).
    fn spec_for(bodies: &[(&'static str, &[u8])]) -> ModelSpec {
        let files: Vec<ModelFile> = bodies
            .iter()
            .map(|(name, body)| ModelFile {
                name,
                size: body.len() as u64,
                sha256: Box::leak(sha(body).into_boxed_str()),
            })
            .collect();
        ModelSpec {
            id: "test-model",
            base_url: "https://example.test/m",
            files: Box::leak(files.into_boxed_slice()),
        }
    }

    fn fetch_serving(spec: &ModelSpec, bodies: &[(&str, &[u8])]) -> MemoryFetch {
        let mut fetch = MemoryFetch::default();
        for (name, body) in bodies {
            let file = spec.files.iter().find(|f| f.name == *name).unwrap();
            fetch.bodies.insert(spec.url(file), body.to_vec());
        }
        fetch
    }

    #[test]
    fn pinned_parakeet_spec_is_consistent() {
        let spec = PARAKEET_TDT_V3_INT8;
        assert!(spec.base_url.starts_with("https://"));
        assert!(spec.total_size() > 600_000_000);
        for f in spec.files {
            assert_eq!(f.sha256.len(), 64, "{}", f.name);
            assert!(f.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn installs_verified_files_and_reports_progress() {
        let bodies: &[(&str, &[u8])] = &[("vocab.txt", b"a b c"), ("enc.onnx", b"0123456789")];
        let spec = spec_for(bodies);
        let fetch = fetch_serving(&spec, bodies);
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path(), spec);
        assert!(!store.is_installed());

        let mut last = (0, 0);
        store
            .install(&fetch, |done, total| last = (done, total))
            .unwrap();

        assert!(store.is_installed());
        assert_eq!(last, (15, 15));
        assert_eq!(
            fs::read(store.dir().join("enc.onnx")).unwrap(),
            b"0123456789"
        );
    }

    #[test]
    fn skips_files_already_present() {
        let bodies: &[(&str, &[u8])] = &[("a", b"aaa"), ("b", b"bbbb")];
        let spec = spec_for(bodies);
        let fetch = fetch_serving(&spec, bodies);
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path(), spec);
        fs::create_dir_all(store.dir()).unwrap();
        fs::write(store.dir().join("a"), b"aaa").unwrap();

        store.install(&fetch, |_, _| {}).unwrap();

        let requests = fetch.requests.lock().unwrap();
        assert_eq!(requests.as_slice(), ["https://example.test/m/b"]);
    }

    #[test]
    fn rejects_corrupted_download_and_leaves_no_partial_file() {
        let spec = spec_for(&[("enc.onnx", b"good-bytes")]);
        let fetch = fetch_serving(&spec, &[("enc.onnx", b"evil-bytes")]);
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path(), spec);

        let err = store.install(&fetch, |_, _| {}).unwrap_err();

        assert!(matches!(err, InstallError::Checksum { .. }), "{err}");
        assert!(!store.dir().join("enc.onnx").exists());
        assert!(!store.dir().join("enc.part").exists());
    }

    #[test]
    fn rejects_truncated_download() {
        let spec = spec_for(&[("enc.onnx", b"full-length")]);
        let fetch = fetch_serving(&spec, &[("enc.onnx", b"full")]);
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path(), spec);

        let err = store.install(&fetch, |_, _| {}).unwrap_err();
        assert!(
            matches!(
                err,
                InstallError::Size {
                    actual: 4,
                    expected: 11,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn network_failure_is_reported_with_file_name() {
        let spec = spec_for(&[("enc.onnx", b"x")]);
        let fetch = MemoryFetch::default();
        let root = tempfile::tempdir().unwrap();
        let err = ModelStore::new(root.path(), spec)
            .install(&fetch, |_, _| {})
            .unwrap_err();
        assert!(err.to_string().contains("enc.onnx"));
    }
}

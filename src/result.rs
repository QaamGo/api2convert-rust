//! [`ConversionResult`] and [`FileDownload`] — the ergonomic handles returned by
//! [`convert`](crate::Api2Convert::convert) and
//! [`download`](crate::Api2Convert::download). **Hand-authored.**

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::errors::{Api2ConvertError, Result};
use crate::models::{Job, OutputFile};
use crate::transport::Transport;

const CHUNK: usize = 64 * 1024;

/// A handle to a single downloadable output. A download password given at
/// conversion time (or to [`Api2Convert::download`](crate::Api2Convert::download))
/// is remembered here and sent automatically; an explicit password argument to
/// [`save`](Self::save) / [`contents`](Self::contents) overrides it for that call.
pub struct FileDownload {
    transport: Arc<Transport>,
    output: OutputFile,
    download_password: Option<String>,
}

impl FileDownload {
    pub(crate) fn new(
        transport: Arc<Transport>,
        output: OutputFile,
        download_password: Option<String>,
    ) -> Self {
        FileDownload {
            transport,
            output,
            download_password,
        }
    }

    /// The output's download URL.
    pub fn url(&self) -> &str {
        &self.output.uri
    }

    /// The underlying output metadata.
    pub fn output(&self) -> &OutputFile {
        &self.output
    }

    /// Stream the download to disk. If `path_or_dir` is an existing directory
    /// (or ends with a path separator) the API filename is appended, sanitized
    /// against path traversal. Returns the path written.
    pub fn save(
        &self,
        path_or_dir: impl AsRef<Path>,
        download_password: Option<&str>,
    ) -> Result<PathBuf> {
        let pw = pick_password(download_password, &self.download_password);
        let mut resp = self.transport.open_download(&self.output.uri, pw)?;
        let target = resolve_target(path_or_dir.as_ref(), self.output.filename.as_deref());
        stream_to_file(&mut resp.body, &target, self.transport.max_download_bytes())?;
        Ok(target)
    }

    /// Download the output into memory.
    pub fn contents(&self, download_password: Option<&str>) -> Result<Vec<u8>> {
        let pw = pick_password(download_password, &self.download_password);
        let mut resp = self.transport.open_download(&self.output.uri, pw)?;
        read_capped(&mut resp.body, self.transport.max_download_bytes())
    }
}

/// The outcome of a completed [`convert`](crate::Api2Convert::convert): the
/// finished [`Job`] plus convenience download methods for the selected output.
pub struct ConversionResult {
    transport: Arc<Transport>,
    job: Job,
    index: usize,
    download_password: Option<String>,
}

impl ConversionResult {
    pub(crate) fn new(
        transport: Arc<Transport>,
        job: Job,
        index: usize,
        download_password: Option<String>,
    ) -> Self {
        ConversionResult {
            transport,
            job,
            index,
            download_password,
        }
    }

    /// The completed job.
    pub fn job(&self) -> &Job {
        &self.job
    }

    /// All output files.
    pub fn outputs(&self) -> &[OutputFile] {
        &self.job.output
    }

    /// The selected output (by the configured index).
    pub fn output(&self) -> Result<&OutputFile> {
        self.job.output.get(self.index).ok_or_else(|| {
            Api2ConvertError::Config(format!(
                "no output file at index {} (job produced {})",
                self.index,
                self.job.output.len()
            ))
        })
    }

    /// The selected output's download URL.
    pub fn url(&self) -> Result<String> {
        Ok(self.output()?.uri.clone())
    }

    /// Build a [`FileDownload`] for `output` (or the selected output), carrying
    /// the remembered download password.
    pub fn download(&self, output: Option<OutputFile>) -> Result<FileDownload> {
        let out = match output {
            Some(o) => o,
            None => self.output()?.clone(),
        };
        Ok(FileDownload::new(
            Arc::clone(&self.transport),
            out,
            self.download_password.clone(),
        ))
    }

    /// Stream the selected output to disk (see [`FileDownload::save`]).
    pub fn save(
        &self,
        path_or_dir: impl AsRef<Path>,
        download_password: Option<&str>,
    ) -> Result<PathBuf> {
        self.download(None)?.save(path_or_dir, download_password)
    }

    /// Download the selected output into memory.
    pub fn contents(&self, download_password: Option<&str>) -> Result<Vec<u8>> {
        self.download(None)?.contents(download_password)
    }
}

fn pick_password<'a>(explicit: Option<&'a str>, remembered: &'a Option<String>) -> Option<&'a str> {
    explicit.or(remembered.as_deref())
}

fn resolve_target(path_or_dir: &Path, filename: Option<&str>) -> PathBuf {
    let as_str = path_or_dir.to_string_lossy();
    let looks_like_dir = path_or_dir.is_dir()
        || as_str.ends_with('/')
        || as_str.ends_with(std::path::MAIN_SEPARATOR);
    if looks_like_dir {
        path_or_dir.join(safe_name(filename.unwrap_or("download")))
    } else {
        path_or_dir.to_path_buf()
    }
}

/// Reduce an API-supplied filename to a safe basename (no path traversal).
fn safe_name(name: &str) -> String {
    let normalized = name.replace('\\', "/").replace('\0', "");
    let base = Path::new(&normalized)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if base.is_empty() || base == "." || base == ".." {
        "download".to_string()
    } else {
        base.to_string()
    }
}

fn stream_to_file(body: &mut Box<dyn Read + Send>, target: &Path, max_bytes: u64) -> Result<()> {
    let mut file = fs::File::create(target).map_err(|e| {
        Api2ConvertError::Network(format!("failed to create {}: {}", target.display(), e))
    })?;
    let mut buf = [0u8; CHUNK];
    let mut total: u64 = 0;
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                let _ = fs::remove_file(target);
                return Err(Api2ConvertError::Network(
                    "failed while reading the download".to_string(),
                ));
            }
        };
        total += n as u64;
        if max_bytes > 0 && total > max_bytes {
            let _ = fs::remove_file(target);
            return Err(Api2ConvertError::Network(format!(
                "download exceeds the {max_bytes}-byte limit"
            )));
        }
        if file.write_all(&buf[..n]).is_err() {
            let _ = fs::remove_file(target);
            return Err(Api2ConvertError::Network(format!(
                "failed to write {}",
                target.display()
            )));
        }
    }
    if file.flush().is_err() {
        let _ = fs::remove_file(target);
        return Err(Api2ConvertError::Network(format!(
            "failed to finalize {}",
            target.display()
        )));
    }
    Ok(())
}

fn read_capped(body: &mut Box<dyn Read + Send>, max_bytes: u64) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; CHUNK];
    loop {
        let n = body.read(&mut buf).map_err(|_| {
            Api2ConvertError::Network("failed while reading the download".to_string())
        })?;
        if n == 0 {
            break;
        }
        if max_bytes > 0 && (out.len() as u64 + n as u64) > max_bytes {
            return Err(Api2ConvertError::Network(format!(
                "download exceeds the {max_bytes}-byte limit"
            )));
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

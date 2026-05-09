//! File upload and download helpers for Axum multipart forms.
//!
//! Provides convenient utilities for handling multipart uploads,
//! file size limits, and streaming file downloads.
//!
//! # Example
//!
//! ```no_run
//! use rszero::rest::upload::{save_upload, FileUpload, UploadConfig};
//!
//! # async fn example() -> rszero::error::RszeroResult<()> {
//! // In a handler:
//! // let config = UploadConfig { max_size: 10 * 1024 * 1024, allowed_types: vec!["image/jpeg".into()] };
//! // let file = save_upload(field, "/tmp/uploads", &config).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::{RszeroError, RszeroResult};
use axum::extract::Multipart;
use axum::response::Response;
use axum::http::{header, StatusCode};
use std::path::{Path, PathBuf};

/// Configuration for file uploads.
#[derive(Debug, Clone)]
pub struct UploadConfig {
    /// Maximum file size in bytes.
    pub max_size: usize,
    /// Allowed MIME types (empty = allow all).
    pub allowed_types: Vec<String>,
    /// Allowed file extensions (empty = allow all).
    pub allowed_extensions: Vec<String>,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            max_size: 10 * 1024 * 1024, // 10 MB
            allowed_types: vec![],
            allowed_extensions: vec![],
        }
    }
}

/// Represents a saved uploaded file.
#[derive(Debug, Clone)]
pub struct FileUpload {
    /// Original filename from the client.
    pub original_name: String,
    /// Saved path on disk.
    pub saved_path: PathBuf,
    /// File size in bytes.
    pub size: usize,
    /// MIME type.
    pub content_type: String,
    /// SHA-256 checksum (if computed).
    pub checksum: Option<String>,
}

/// Save a single file from a multipart field to disk.
///
/// Validates size limits and allowed types before writing.
pub async fn save_upload(
    field: &mut axum::extract::multipart::Field<'_>,
    dest_dir: &str,
    config: &UploadConfig,
) -> RszeroResult<FileUpload> {
    let file_name = field.file_name()
        .unwrap_or("unnamed")
        .to_string();

    let content_type = field.content_type()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // Validate MIME type
    if !config.allowed_types.is_empty() && !config.allowed_types.contains(&content_type) {
        return Err(RszeroError::Http {
            code: 415,
            msg: format!("content type '{}' not allowed", content_type),
        });
    }

    // Validate extension
    if !config.allowed_extensions.is_empty() {
        let ext = Path::new(&file_name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if !config.allowed_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
            return Err(RszeroError::Http {
                code: 415,
                msg: format!("file extension '{}' not allowed", ext),
            });
        }
    }

    // Read bytes and validate size
    let mut data = Vec::new();
    while let Some(chunk) = field.chunk().await
        .map_err(|e| RszeroError::Http { code: 400, msg: format!("upload read error: {}", e) })?
    {
        data.extend_from_slice(&chunk);
        if data.len() > config.max_size {
            return Err(RszeroError::Http {
                code: 413,
                msg: format!("file exceeds maximum size of {} bytes", config.max_size),
            });
        }
    }

    // Generate safe filename
    let safe_name = sanitize_filename(&file_name);
    let dest_path = Path::new(dest_dir).join(&safe_name);

    // Ensure directory exists
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| RszeroError::Internal { message: format!("failed to create upload dir: {}", e), source: None })?;
    }

    tokio::fs::write(&dest_path, &data).await
        .map_err(|e| RszeroError::Internal { message: format!("failed to write upload: {}", e), source: None })?;

    tracing::info!(path = %dest_path.display(), size = data.len(), "file uploaded");

    Ok(FileUpload {
        original_name: file_name,
        saved_path: dest_path,
        size: data.len(),
        content_type,
        checksum: None,
    })
}

/// Parse all files from a multipart request and save them.
pub async fn save_multipart(
    mut multipart: Multipart,
    dest_dir: &str,
    config: &UploadConfig,
) -> RszeroResult<Vec<FileUpload>> {
    let mut uploads = Vec::new();
    while let Some(mut field) = multipart.next_field().await
        .map_err(|e| RszeroError::Http { code: 400, msg: format!("multipart error: {}", e) })?
    {
        if field.file_name().is_some() {
            let upload = save_upload(&mut field, dest_dir, config).await?;
            uploads.push(upload);
        }
    }
    Ok(uploads)
}

/// Build a streaming file download response.
pub async fn file_download_response(
    path: &Path,
    filename: Option<&str>,
) -> RszeroResult<Response<String>> {
    let data = tokio::fs::read_to_string(path).await
        .map_err(|e| RszeroError::Http { code: 404, msg: format!("file not found: {}", e) })?;

    let disposition = match filename {
        Some(name) => format!("attachment; filename=\"{}\"", name),
        None => "inline".to_string(),
    };

    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(data)
        .map_err(|e| RszeroError::Internal { message: format!("response build error: {}", e), source: None })?;

    Ok(response)
}

/// Sanitize a filename to prevent path traversal attacks.
fn sanitize_filename(name: &str) -> String {
    let name = name.replace("..", "_");
    let name = name.replace('/', "_");
    let name = name.replace('\\', "_");
    if name.is_empty() || name.chars().all(|c| c == '_') {
        format!("upload_{}", uuid::Uuid::new_v4())
    } else {
        name
    }
}

/// Get file size and MIME type without reading the entire file.
pub async fn file_info(path: &Path) -> RszeroResult<(u64, String)> {
    let meta = tokio::fs::metadata(path).await
        .map_err(|e| RszeroError::Http { code: 404, msg: format!("file not found: {}", e) })?;
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    Ok((meta.len(), mime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("hello.txt"), "hello.txt");
        assert_eq!(sanitize_filename("../etc/passwd"), "__etc_passwd");
        assert_eq!(sanitize_filename("a/b\\c"), "a_b_c");
    }

    #[test]
    fn test_upload_config_default() {
        let cfg = UploadConfig::default();
        assert_eq!(cfg.max_size, 10 * 1024 * 1024);
        assert!(cfg.allowed_types.is_empty());
    }

    #[test]
    fn test_file_upload_struct() {
        let upload = FileUpload {
            original_name: "test.txt".into(),
            saved_path: PathBuf::from("/tmp/test.txt"),
            size: 1024,
            content_type: "text/plain".into(),
            checksum: None,
        };
        assert_eq!(upload.size, 1024);
    }
}

//! REST API gateway — Axum 0.7 wrapper replicating go-zero rest.

pub mod server;
pub mod types;
pub mod handler;
pub mod httpx;
pub mod websocket;
pub mod context;
pub mod param;
pub mod mock;
pub mod upload;

pub use server::{RszeroServer, CorsConfig, RouteGroup};
pub use types::*;
pub use handler::{Handler, FnHandler};
pub use httpx::{ok, error};
pub use websocket::{WsManager, HeartbeatConfig, AckConfig};
pub use param::{PathParam, QueryParam, ParamError, validate_required, validate_range};
pub use mock::{MockServer, MockConfig};
pub use upload::{UploadConfig, FileUpload, save_upload, save_multipart, file_download_response, file_info};

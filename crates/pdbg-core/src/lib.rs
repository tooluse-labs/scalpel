pub mod capabilities;
pub mod config;
pub mod dto;
pub mod egress;
pub mod panic_boundary;
pub mod registry;
pub mod search;
pub mod session;
pub mod shim;
mod wire;

pub use capabilities::{CapabilityDecision, CapabilityFeature};
pub use config::{RepairPolicy, SafeModeConfig};
pub use dto::*;
pub use egress::{escape_pdf_text, EgressFormat, EscapedText};
pub use panic_boundary::{catch_ffi_callback, CALLBACK_PANIC_MESSAGE};
pub use registry::{ChildContainer, NodeTokenRegistry};
pub use search::*;
pub use session::{DocumentSession, FakeSharedStore, FakeSharedStoreSnapshot, TaskQueueStats};
#[cfg(feature = "real-mupdf")]
pub use shim::RealMuPdfShim;
pub use shim::{CancelToken, FakeShim, OpenDocument, Shim, ShimDocument, ShimError};

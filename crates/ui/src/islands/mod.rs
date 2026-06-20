//! Interactive Leptos islands (#[island]): upload zone, modals, theme toggle, toasts, etc.
//! Islands are the only code compiled to WASM — keeps the bundle small (REQ-ui-theming).
//! Filled by Plan 04 (Wave 2).
//!
//! Security invariant (DEC-ui-ssr, criterion 5): NO signing/secret/presign/hmac/sigv4
//! code in ANY island or hydrate-gated source. NO ferrobucket-storage import in islands
//! (RESEARCH Pitfall 3 — they compile to WASM). localStorage holds ONLY the theme string.

pub mod confirm_modal;
pub mod copy_button;
pub mod create_bucket_modal;
pub mod slide_over;
pub mod theme_toggle;
pub mod toast;
pub mod upload_panel;
pub mod upload_zone;

pub use confirm_modal::{ConfirmAction, ConfirmModal};
pub use copy_button::CopyButton;
pub use create_bucket_modal::CreateBucketModal;
pub use slide_over::SlideOver;
pub use theme_toggle::ThemeToggle;
pub use toast::{Toast, ToastKind};
pub use upload_panel::{FileEntry, FileEntryName, ProgressInfo, UploadPanel, UploadStatus};
pub use upload_zone::{UploadZone, PART_SIZE_BYTES};

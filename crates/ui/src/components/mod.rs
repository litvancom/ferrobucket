//! SSR-only reusable UI components: bucket table, object table, breadcrumb, empty state, etc.
//! These components are never compiled to WASM — rendered server-side only.
//! Filled by Plan 05 (Wave 3).

pub mod breadcrumb;
pub mod bucket_table;
pub mod copy_field;
pub mod empty_state;
pub mod inline_preview;
pub mod loading_state;
pub mod object_table;
pub mod pagination_bar;
pub mod sidebar;
pub mod status_indicator;

pub use breadcrumb::Breadcrumb;
pub use bucket_table::BucketTable;
pub use copy_field::CopyField;
pub use empty_state::EmptyState;
pub use inline_preview::InlinePreview;
pub use loading_state::LoadingState;
pub use object_table::ObjectTable;
pub use pagination_bar::PaginationBar;
pub use sidebar::Sidebar;
pub use status_indicator::StatusIndicator;

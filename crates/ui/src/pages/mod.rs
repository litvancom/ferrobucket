//! Page-level SSR components: bucket list, object browser, settings.
//! Implemented by Plan 05 (Wave 3).

pub mod bucket_list;
pub mod object_browser;
pub mod settings;

pub use bucket_list::BucketListPage;
pub use object_browser::ObjectBrowserPage;
pub use settings::SettingsPage;

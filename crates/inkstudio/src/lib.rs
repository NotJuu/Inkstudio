//! Inkstudio application: book/page model, canvas, tools and UI.
pub mod app;
pub mod book;
pub mod canvas;
pub mod files;
pub mod layouts;
pub mod new_sheet;
pub mod pen;
pub mod project;
pub mod recovery;
pub mod settings;
pub mod tablet;

pub use app::InkApp;

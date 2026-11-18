pub mod client;
pub mod error;
pub mod extension;
pub mod mapping;
pub mod pages;

pub use client::CatalogClient;
pub use client::{SearchFilter, SearchOutcome};
pub use error::Error;
pub use pages::{AlbumPage, ArtistPage, MoodEntry, MoodListing, MoodSection, PlaylistPage};

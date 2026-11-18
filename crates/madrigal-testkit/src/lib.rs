slint::include_modules!();

use std::path::Path;
use std::time::Duration;

pub use i_slint_backend_testing::ElementHandle;
use i_slint_backend_testing::ElementRoot;

pub enum Error {
    ElementNotFound(String),
    Snapshot(String),
    Image(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ElementNotFound(id) => write!(f, "element not found: {id}"),
            Self::Snapshot(detail) => write!(f, "snapshot failed: {detail}"),
            Self::Image(detail) => write!(f, "image failed: {detail}"),
        }
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for Error {}

pub fn init_headless() {
    std::env::set_var("SLINT_BACKEND", "headless-software");
    std::env::set_var("SLINT_EMIT_DEBUG_INFO", "1");
    let backend = i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: true,
            renderer_name: Some(slint::SharedString::from("software")),
        },
    );
    slint::platform::set_platform(Box::new(backend)).expect("backend already initialized");
}

pub fn find_by_id(root: &impl ElementRoot, id: &str) -> Result<ElementHandle, Error> {
    let mut found = ElementHandle::find_by_element_id(root, id);
    found.next().ok_or_else(|| Error::ElementNotFound(id.to_string()))
}

pub fn click(handle: &ElementHandle) {
    handle.mock_single_click(slint::platform::PointerEventButton::Left);
}

pub fn click_by_id(root: &impl ElementRoot, id: &str) -> Result<(), Error> {
    let handle = find_by_id(root, id)?;
    click(&handle);
    Ok(())
}

pub fn mock_advance(total: Duration) {
    const STEP: Duration = Duration::from_millis(1);
    let mut rest = total;
    while rest > Duration::ZERO {
        let chunk = rest.min(STEP);
        i_slint_backend_testing::mock_elapsed_time(chunk);
        rest -= chunk;
    }
}

pub fn snapshot_to(window: &slint::Window, path: &Path) -> Result<(), Error> {
    let buffer = window.take_snapshot().map_err(|e| Error::Snapshot(e.to_string()))?;
    let pixels = buffer.as_bytes().to_vec();
    let width = buffer.width();
    let height = buffer.height();
    let picture = image::RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| Error::Snapshot("empty pixel buffer".to_string()))?;
    picture.save(path).map_err(|e| Error::Image(e.to_string()))?;
    Ok(())
}

use image::RgbaImage;
use slint::ComponentHandle;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub fn to_slint_image(img: &RgbaImage) -> slint::Image {
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        img.as_raw(),
        img.width(),
        img.height(),
    );
    slint::Image::from_rgba8(buffer)
}

fn generations() -> &'static Mutex<HashMap<String, u64>> {
    static GENERATIONS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    GENERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_generation(slot: &str) -> u64 {
    let mut map = generations().lock().expect("generation lock poisoned");
    let generation = map.entry(slot.to_string()).or_insert(0);
    *generation = generation.wrapping_add(1);
    *generation
}

fn is_current(slot: &str, generation: u64) -> bool {
    generations()
        .lock()
        .map(|map| map.get(slot) == Some(&generation))
        .unwrap_or(false)
}

pub fn apply_cover<C>(
    component: &C,
    slot: &str,
    image: RgbaImage,
    setter: impl FnOnce(&C, slint::Image) + Send + 'static,
) where
    C: ComponentHandle + 'static,
{
    let slot = slot.to_string();
    let generation = next_generation(&slot);
    let weak = component.as_weak();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(component) = weak.upgrade() {
            if is_current(&slot, generation) {
                setter(&component, to_slint_image(&image));
            }
        }
    });
}

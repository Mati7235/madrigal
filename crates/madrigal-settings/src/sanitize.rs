pub fn clamp_i32(value: i32, low: i32, high: i32) -> i32 {
    value.clamp(low, high)
}

pub fn clamp_f32(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        fallback
    }
}

pub fn non_empty_or(raw: &str, default: &str) -> String {
    let value = raw.trim();
    if value.is_empty() {
        default.to_owned()
    } else {
        value.to_owned()
    }
}

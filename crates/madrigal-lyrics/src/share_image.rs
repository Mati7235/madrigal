use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};

use crate::Lyrics;

#[derive(Debug, Clone)]
pub struct ShareOptions {
    pub width: u32,
    pub height: u32,
    pub base: [u8; 3],
    pub active: Option<usize>,
    pub title: String,
    pub artist: String,
}

impl Default for ShareOptions {
    fn default() -> Self {
        Self {
            width: 1080,
            height: 1350,
            base: [18, 42, 67],
            active: None,
            title: String::new(),
            artist: String::new(),
        }
    }
}

pub fn render(lyrics: &Lyrics, options: &ShareOptions) -> RgbImage {
    let width = options.width.max(64);
    let height = options.height.max(64);
    let mut image = RgbImage::new(width, height);
    paint_gradient(&mut image, options.base);
    let mut cursor = height / 12;
    if !options.title.trim().is_empty() {
        cursor = paint_bar(&mut image, cursor, width, bar_width(width, 26), 12, brighten(options.base, 0.85));
        cursor += 10;
    }
    if !options.artist.trim().is_empty() {
        cursor = paint_bar(&mut image, cursor, width, bar_width(width, 18), 10, brighten(options.base, 0.6));
        cursor += 26;
    }
    let rows = lyrics.lines.len().max(1) as u32;
    let available = height.saturating_sub(cursor + height / 12);
    let step = (available / rows).clamp(22, 56);
    let mut y = cursor + (available.saturating_sub(step * rows)) / 2;
    for (index, line) in lyrics.lines.iter().enumerate() {
        let lit = options.active.is_some_and(|active| active == index);
        let span = bar_width(width.saturating_sub(160), line.text.chars().count().min(48));
        let color = if lit {
            Rgb([240, 244, 248])
        } else {
            brighten(options.base, 0.45)
        };
        let bar_height = if lit { 14 } else { 9 };
        y = paint_bar(&mut image, y, width, span.max(48), bar_height, color);
        y += step.saturating_sub(bar_height);
    }
    image
}

pub fn render_png(lyrics: &Lyrics, options: &ShareOptions) -> Vec<u8> {
    let image = render(lyrics, options);
    let mut out = Vec::new();
    let encoder = PngEncoder::new(&mut out);
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .expect("png encode");
    out
}

fn paint_gradient(image: &mut RgbImage, base: [u8; 3]) {
    let height = image.height();
    for y in 0..height {
        let shade = 1.0 - 0.65 * (y as f32 / height.max(1) as f32);
        let pixel = Rgb([
            (base[0] as f32 * shade) as u8,
            (base[1] as f32 * shade) as u8,
            (base[2] as f32 * shade) as u8,
        ]);
        for x in 0..image.width() {
            image.put_pixel(x, y, pixel);
        }
    }
}

fn bar_width(limit: u32, chars: usize) -> u32 {
    (48 + chars as u32 * 14).min(limit.max(48))
}

fn paint_bar(
    image: &mut RgbImage,
    y: u32,
    width: u32,
    span: u32,
    bar_height: u32,
    color: Rgb<u8>,
) -> u32 {
    let start_x = width.saturating_sub(span) / 2;
    let stop_y = (y + bar_height).min(image.height());
    for row in y..stop_y {
        for x in start_x..(start_x + span).min(image.width()) {
            image.put_pixel(x, row, color);
        }
    }
    stop_y
}

fn brighten(base: [u8; 3], amount: f32) -> Rgb<u8> {
    Rgb([
        (base[0] as f32 + (255.0 - base[0] as f32) * amount) as u8,
        (base[1] as f32 + (255.0 - base[1] as f32) * amount) as u8,
        (base[2] as f32 + (255.0 - base[2] as f32) * amount) as u8,
    ])
}

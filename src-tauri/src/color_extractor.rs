use std::{collections::HashMap, path::Path};

use image::ImageReader;

use crate::models::ThemePalette;

#[derive(Clone, Copy)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
    count: u32,
}

#[derive(Clone, Copy, Default)]
struct ColorAccumulator {
    r: u64,
    g: u64,
    b: u64,
    count: u32,
}

pub fn extract_theme_palette(path: &Path) -> Result<ThemePalette, String> {
    let image = ImageReader::open(path)
        .map_err(|error| format!("无法打开图片用于取色: {error}"))?
        .decode()
        .map_err(|error| format!("无法解码图片用于取色: {error}"))?
        .to_rgba8();
    let sample_step = ((image.width() as u64 * image.height() as u64) / 40_000).max(1) as u32;
    let mut buckets: HashMap<(u8, u8, u8), ColorAccumulator> = HashMap::new();

    for (index, pixel) in image.pixels().enumerate() {
        if !(index as u32).is_multiple_of(sample_step) || pixel[3] < 128 {
            continue;
        }
        let brightness = (u16::from(pixel[0]) + u16::from(pixel[1]) + u16::from(pixel[2])) / 3;
        if !(10..=245).contains(&brightness) {
            continue;
        }
        let key = (pixel[0] >> 4, pixel[1] >> 4, pixel[2] >> 4);
        let bucket = buckets.entry(key).or_default();
        bucket.r += u64::from(pixel[0]);
        bucket.g += u64::from(pixel[1]);
        bucket.b += u64::from(pixel[2]);
        bucket.count += 1;
    }

    let mut colors = buckets
        .into_values()
        .map(|accum| Color {
            r: (accum.r / u64::from(accum.count)) as u8,
            g: (accum.g / u64::from(accum.count)) as u8,
            b: (accum.b / u64::from(accum.count)) as u8,
            count: accum.count,
        })
        .collect::<Vec<_>>();
    colors.sort_by(|left, right| right.count.cmp(&left.count));
    colors.truncate(8);
    if colors.is_empty() {
        colors = vec![
            Color {
                r: 26,
                g: 27,
                b: 38,
                count: 4,
            },
            Color {
                r: 122,
                g: 162,
                b: 247,
                count: 1,
            },
            Color {
                r: 158,
                g: 206,
                b: 106,
                count: 1,
            },
        ];
    }
    Ok(generate_palette(colors))
}

fn generate_palette(mut colors: Vec<Color>) -> ThemePalette {
    colors.sort_by(|left, right| luminance(*left).total_cmp(&luminance(*right)));
    let total = colors.iter().map(|color| color.count as f64).sum::<f64>();
    let average = colors
        .iter()
        .map(|color| luminance(*color) * color.count as f64)
        .sum::<f64>()
        / total;
    let dark = average < 0.18;
    let text = if dark { "#f5f5f5" } else { "#1a1a1a" };
    let target = if dark { "#000000" } else { "#ffffff" };
    let sources = [
        colors[0],
        *colors.get(1).unwrap_or(&colors[0]),
        *colors.get(colors.len() / 2).unwrap_or(&colors[0]),
    ];
    let mixes = if dark {
        [0.18, 0.12, 0.06]
    } else {
        [0.72, 0.76, 0.88]
    };
    let surfaces = sources.map(to_hex);
    let background = readable_surface(mix(&surfaces[0], target, mixes[0]), text, target);
    let panel = readable_surface(mix(&surfaces[1], target, mixes[1]), text, target);
    let panel_alt = readable_surface(mix(&surfaces[2], target, mixes[2]), text, target);
    let surface_set = [&background, &panel, &panel_alt];
    let accent = colors
        .iter()
        .max_by_key(|color| color.r.max(color.g).max(color.b) - color.r.min(color.g).min(color.b))
        .map(|color| to_hex(*color))
        .filter(|color| {
            surface_set
                .iter()
                .all(|surface| contrast(color, surface) >= 4.5)
        })
        .unwrap_or_else(|| {
            if dark {
                "#ffffff".to_string()
            } else {
                "#1a1a1a".to_string()
            }
        });
    let hover = readable_surface(mix(&panel, &accent, 0.1), text, target);
    let active = readable_surface(mix(&panel, &accent, 0.18), text, target);
    let subtle = readable_surface(mix(&panel_alt, &accent, 0.06), text, target);
    let muted = readable_surface(mix(text, &background, 0.32), &background, text);
    let border = mix(&panel_alt, &accent, 0.1);

    ThemePalette {
        background,
        panel,
        panel_alt,
        text: text.to_string(),
        muted,
        accent: accent.clone(),
        accent_text: readable_text(&accent).to_string(),
        border,
        hover,
        active,
        subtle,
    }
}

fn to_hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

pub(crate) fn channels(color: &str) -> [u8; 3] {
    [1, 3, 5].map(|index| u8::from_str_radix(&color[index..index + 2], 16).unwrap_or(0))
}

fn mix(left: &str, right: &str, amount: f64) -> String {
    let from = channels(left);
    let to = channels(right);
    format!(
        "#{:02x}{:02x}{:02x}",
        (f64::from(from[0]) * (1.0 - amount) + f64::from(to[0]) * amount).round() as u8,
        (f64::from(from[1]) * (1.0 - amount) + f64::from(to[1]) * amount).round() as u8,
        (f64::from(from[2]) * (1.0 - amount) + f64::from(to[2]) * amount).round() as u8,
    )
}

fn luminance(color: Color) -> f64 {
    luminance_channels([color.r, color.g, color.b])
}

pub(crate) fn contrast(left: &str, right: &str) -> f64 {
    let a = luminance_channels(channels(left));
    let b = luminance_channels(channels(right));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

pub(crate) fn luminance_channels(channels: [u8; 3]) -> f64 {
    let values = channels.map(|value| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722
}

fn readable_surface(color: String, text: &str, fallback: &str) -> String {
    if contrast(&color, text) >= 4.5 {
        color
    } else {
        fallback.to_string()
    }
}

fn readable_text(background: &str) -> &'static str {
    if contrast(background, "#1a1a1a") >= contrast(background, "#ffffff") {
        "#1a1a1a"
    } else {
        "#ffffff"
    }
}

#[cfg(test)]
mod tests {
    use super::{contrast, extract_theme_palette};
    use image::{Rgba, RgbaImage};

    #[test]
    fn extracts_a_valid_palette_from_a_local_png() {
        let path =
            std::env::temp_dir().join(format!("wbskin-color-test-{}.png", std::process::id()));
        let mut image = RgbaImage::new(2, 1);
        image.put_pixel(0, 0, Rgba([205, 85, 125, 255]));
        image.put_pixel(1, 0, Rgba([60, 40, 50, 255]));
        image.save(&path).expect("save fixture image");

        let palette = extract_theme_palette(&path).expect("extract palette");
        assert!(palette.background.starts_with('#'));
        assert!(palette.accent.starts_with('#'));
        assert_ne!(palette.text, palette.background);
        std::fs::remove_file(path).expect("remove fixture image");
    }

    #[test]
    fn keeps_muted_text_readable_on_a_light_palette() {
        let path =
            std::env::temp_dir().join(format!("wbskin-muted-test-{}.png", std::process::id()));
        let mut image = RgbaImage::new(2, 1);
        image.put_pixel(0, 0, Rgba([245, 225, 230, 255]));
        image.put_pixel(1, 0, Rgba([210, 160, 180, 255]));
        image.save(&path).expect("save fixture image");

        let palette = extract_theme_palette(&path).expect("extract palette");
        let ratio = contrast(&palette.muted, &palette.background);
        assert!(
            ratio >= 4.5,
            "muted 与 background 对比度不足: {ratio}，muted={} background={}",
            palette.muted,
            palette.background
        );
        assert_ne!(palette.muted, palette.background);
        std::fs::remove_file(path).expect("remove fixture image");
    }
}

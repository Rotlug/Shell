//! The color an image is mostly about, e.g. an album cover, for things shown next to it.

use crate::models::Rgba;

/// How many slices the hues are cut into: pixels of similar hues count towards the same color.
const HUE_BINS: usize = 12;

/// How colorful (on average, from 0.0 to 1.0) a cover has to be for it to have a dominant color
/// other than the average of all of its pixels. Black and white and gray covers don't.
const MIN_COLORFULNESS: f32 = 0.05;

/// Pixels that are this transparent (from 0 to 255) are not part of the cover.
const MIN_ALPHA: u8 = 128;

/// The text is shown on a dark background, the color is made to be at least this light (in
/// HSL, from 0.0 to 1.0), and no lighter than the other.
const MIN_LIGHTNESS: f32 = 0.62;
const MAX_LIGHTNESS: f32 = 0.85;

#[derive(Clone, Copy, Default)]
struct Bin {
    weight: f32,
    sum: [f32; 3],
}

/// The dominant color of the pixels of an image, made readable on a
/// dark background. `None` when there are no pixels to look at.
///
/// Colorful pixels count more than dull ones and dark ones, so that a cover that is mostly
/// black with a red picture on it is red. A cover without any color has the color of its
/// average pixel.
pub fn dominant_color(pixels: impl IntoIterator<Item = Rgba>) -> Option<Rgba> {
    let mut bins = [Bin::default(); HUE_BINS];
    let mut all = [0.0_f32; 3];
    let mut count = 0.0_f32;

    for Rgba { red, green, blue, alpha } in pixels {
        if alpha < MIN_ALPHA {
            continue;
        }

        let rgb = [f32::from(red), f32::from(green), f32::from(blue)];
        let (hue, saturation, value) = hsv(rgb);
        let weight = saturation * value;

        let bin = &mut bins[((hue / 360.0 * HUE_BINS as f32) as usize).min(HUE_BINS - 1)];
        bin.weight += weight;
        for (sum, channel) in bin.sum.iter_mut().zip(rgb) {
            *sum += channel * weight;
        }

        for (sum, channel) in all.iter_mut().zip(rgb) {
            *sum += channel;
        }
        count += 1.0;
    }

    if count == 0.0 {
        return None;
    }

    let best = bins.iter().max_by(|a, b| a.weight.total_cmp(&b.weight))?;
    let color = if best.weight / count >= MIN_COLORFULNESS {
        best.sum.map(|sum| sum / best.weight)
    } else {
        all.map(|sum| sum / count)
    };

    Some(readable(color))
}

/// Hue (in degrees), saturation and value, the last two from 0.0 to 1.0.
fn hsv([red, green, blue]: [f32; 3]) -> (f32, f32, f32) {
    let (red, green, blue) = (red / 255.0, green / 255.0, blue / 255.0);
    let max = red.max(green).max(blue);
    let delta = max - red.min(green).min(blue);

    let hue = if delta == 0.0 {
        0.0
    } else if max == red {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if max == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };

    (hue, if max == 0.0 { 0.0 } else { delta / max }, max)
}

/// Makes the color light enough to be read on a dark background, with the same hue.
fn readable([red, green, blue]: [f32; 3]) -> Rgba {
    let (red, green, blue) = (red / 255.0, green / 255.0, blue / 255.0);
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;

    let lightness = (max + min) / 2.0;
    let saturation = if delta == 0.0 { 0.0 } else { delta / (1.0 - (2.0 * lightness - 1.0).abs()) };
    let hue = hsv([red * 255.0, green * 255.0, blue * 255.0]).0;

    let lightness = lightness.clamp(MIN_LIGHTNESS, MAX_LIGHTNESS);

    // HSL back to RGB
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let offset = lightness - chroma / 2.0;
    let (red, green, blue) = match (hue / 60.0) as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };

    let [red, green, blue] = [red, green, blue].map(|channel| ((channel + offset) * 255.0).round().clamp(0.0, 255.0) as u8);

    Rgba { red, green, blue, alpha: 255 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(red: u8, green: u8, blue: u8, alpha: u8) -> Rgba {
        Rgba { red, green, blue, alpha }
    }

    fn solid(color: [u8; 3], count: usize) -> impl Iterator<Item = Rgba> {
        std::iter::repeat_n(pixel(color[0], color[1], color[2], 255), count)
    }

    fn lightness(color: Rgba) -> f32 {
        let max = color.red.max(color.green).max(color.blue);
        let min = color.red.min(color.green).min(color.blue);

        (f32::from(max) + f32::from(min)) / 510.0
    }

    #[test]
    fn a_red_cover_is_red() {
        let color = dominant_color(solid([200, 20, 20], 100)).unwrap();

        assert!(color.red > color.green && color.red > color.blue, "{color:?}");
    }

    #[test]
    fn the_most_colorful_hue_wins() {
        let color = dominant_color(solid([20, 20, 220], 70).chain(solid([220, 20, 20], 30))).unwrap();

        assert!(color.blue > color.red);
    }

    #[test]
    fn a_picture_on_black_is_the_color_of_the_picture() {
        let color = dominant_color(solid([0, 0, 0], 900).chain(solid([30, 200, 40], 100))).unwrap();

        assert!(color.green > color.red && color.green > color.blue, "{color:?}");
    }

    #[test]
    fn a_gray_cover_stays_gray() {
        let color = dominant_color(solid([100, 100, 100], 50)).unwrap();

        assert!(color.red.abs_diff(color.green) <= 1 && color.green.abs_diff(color.blue) <= 1);
    }

    #[test]
    fn the_color_can_be_read_on_a_dark_background() {
        for source in [[0, 0, 0], [30, 0, 60], [200, 20, 20], [255, 255, 255]] {
            let color = dominant_color(solid(source, 10)).unwrap();

            assert!((MIN_LIGHTNESS - 0.01..=MAX_LIGHTNESS + 0.01).contains(&lightness(color)), "{source:?} {color:?}");
        }
    }

    #[test]
    fn the_color_is_opaque() {
        assert_eq!(dominant_color(solid([10, 200, 10], 4)).unwrap().alpha, 255);
    }

    #[test]
    fn nothing_to_look_at() {
        assert!(dominant_color(std::iter::empty()).is_none());
        assert!(dominant_color(std::iter::repeat_n(pixel(255, 0, 0, 0), 10)).is_none());
    }
}

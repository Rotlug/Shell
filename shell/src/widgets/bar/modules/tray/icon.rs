use systray::item::Icon;

use crate::pixbuf;

// Rationale: Some icons have the possibility of being absurdly large (e.g. 1024x1024). The
// detail in icons that large is imperceptible when they're resized to a smaller size, and
// converting them is a lot of data to chew through for nothing.
const MAX_SIZE: i32 = 28;

pub const FALLBACK_ICON: &str = "emote-love";

/// Makes a pixbuf out of a tray item's icon.
pub fn make_icon_pixbuf(icon: Option<&Icon>) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    match icon {
        Some(Icon::System(name)) => pixbuf::get_pixbuf_or_fallback(name, FALLBACK_ICON),
        Some(Icon::Pixmap { width, height, pixels }) => pixmap_to_pixbuf(*width, *height, pixels),
        None => pixbuf::get_pixbuf_or_fallback("", FALLBACK_ICON),
    }
}

/// Tray pixmaps are ARGB32 in network byte order (so the bytes of a pixel are A, R, G, B),
/// pixbufs want R, G, B, A.
fn pixmap_to_pixbuf(width: i32, height: i32, pixels: &[u8]) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    if width <= 0 || height <= 0 || pixels.len() < (width * height * 4) as usize {
        return None;
    }

    let rgba: Vec<u8> = pixels
        .chunks_exact(4)
        .take((width * height) as usize)
        .flat_map(|argb| [argb[1], argb[2], argb[3], argb[0]])
        .collect();

    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_mut_slice(
        rgba,
        gtk::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        width,
        height,
        width * 4,
    );

    let pixbuf = if width > MAX_SIZE || height > MAX_SIZE {
        pixbuf.scale_simple(MAX_SIZE, MAX_SIZE, gtk::gdk_pixbuf::InterpType::Bilinear)?
    } else {
        pixbuf
    };

    Some(pixbuf)
}

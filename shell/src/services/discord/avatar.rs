//! Fetching the profile pictures of users.
use std::collections::HashMap;
use std::io::Read as _;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// How big avatars are made, in pixels
pub const SIZE: u32 = 22;

const TIMEOUT: Duration = Duration::from_secs(10);
/// Avatars are small, anything this big is not one
const MAX_DOWNLOAD: u64 = 8 * 1024 * 1024;
/// Users come and go all the time, but the same ones come back
const CACHE_CAPACITY: usize = 64;

/// A profile picture, `SIZE` by `SIZE` pixels of RGBA.
#[derive(Clone, Debug)]
pub struct Avatar {
    pub pixels: Vec<u8>,
}

static CACHE: LazyLock<Mutex<HashMap<String, Avatar>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Downloads and prepares the profile picture at `url`. `None` if that doesn't work out.
pub async fn load(url: String) -> Option<Avatar> {
    if let Some(avatar) = CACHE.lock().ok()?.get(&url) {
        return Some(avatar.clone());
    }

    let key = url.clone();
    let avatar = tokio::task::spawn_blocking(move || decode(&fetch(&url)?))
        .await
        .ok()??;

    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() >= CACHE_CAPACITY {
            cache.clear();
        }

        cache.insert(key, avatar.clone());
    }

    Some(avatar)
}

fn fetch(url: &str) -> Option<Vec<u8>> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return None;
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();

    let mut body = agent.get(url).call().ok()?.into_body();
    let mut bytes = Vec::new();
    body.as_reader().take(MAX_DOWNLOAD).read_to_end(&mut bytes).ok()?;

    Some(bytes)
}

/// Any image format, cropped to a square and scaled to `SIZE`.
fn decode(bytes: &[u8]) -> Option<Avatar> {
    let image = image::load_from_memory(bytes).ok()?;
    let image = image.resize_to_fill(SIZE, SIZE, image::imageops::FilterType::Lanczos3);

    Some(Avatar { pixels: image.to_rgba8().into_raw() })
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write as _};
    use std::net::TcpListener;

    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 10, 10, 255]));
        let mut bytes = Vec::new();
        image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    #[test]
    fn crops_and_scales_to_a_square() {
        let avatar = decode(&png(128, 64)).unwrap();

        assert_eq!(avatar.pixels.len(), (SIZE * SIZE * 4) as usize);
        assert_eq!(&avatar.pixels[..4], &[200, 10, 10, 255]);
    }

    #[test]
    fn rejects_what_is_not_an_image() {
        assert!(decode(b"definitely not an image").is_none());
    }

    #[test]
    fn only_fetches_over_http() {
        assert!(fetch("file:///etc/passwd").is_none());
        assert!(fetch("/etc/passwd").is_none());
    }

    #[test]
    fn fetches_over_http() {
        let image = png(32, 32);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request);

            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                image.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&image).unwrap();
        });

        let bytes = fetch(&format!("http://127.0.0.1:{port}/avatar.png")).unwrap();
        assert!(decode(&bytes).is_some());
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;

    /// Needs internet: `cargo test -p gray-meadows-shell -- --ignored`
    #[test]
    #[ignore]
    fn fetches_from_the_discord_cdn() {
        let bytes = fetch("https://cdn.discordapp.com/embed/avatars/0.png").expect("download");
        let avatar = decode(&bytes).expect("decode");

        assert_eq!(avatar.pixels.len(), (SIZE * SIZE * 4) as usize);
    }
}

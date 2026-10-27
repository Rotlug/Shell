//! Just enough of EDID, the data monitors describe themselves with, to tell them apart.

const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
const DESCRIPTORS: [usize; 4] = [54, 72, 90, 108];
const DESCRIPTOR_SIZE: usize = 18;
const MONITOR_NAME_TAG: u8 = 0xFC;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edid {
    /// The three letter id of the manufacturer, e.g. `DEL`
    pub manufacturer: String,
    pub product_code: u16,
    /// The model name, e.g. `DELL U2720Q`, if the monitor gives one
    pub name: Option<String>,
}

impl Edid {
    /// Parses the first block (128 bytes) of an EDID.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 128 || bytes[..8] != HEADER {
            return None;
        }

        // Three letters of five bits each, where 1 is `A`
        let id = u16::from_be_bytes([bytes[8], bytes[9]]);
        let manufacturer = [10, 5, 0]
            .map(|shift| char::from(b'@' + ((id >> shift) & 0x1F) as u8))
            .iter()
            .collect();

        let name = DESCRIPTORS.iter().find_map(|&start| {
            let descriptor = &bytes[start..start + DESCRIPTOR_SIZE];

            // Display descriptors start with three zeroes, then a tag and a zero
            if descriptor[..3] != [0, 0, 0] || descriptor[3] != MONITOR_NAME_TAG {
                return None;
            }

            let text = String::from_utf8_lossy(&descriptor[5..]);
            let name = text.split('\n').next().unwrap_or_default().trim();

            (!name.is_empty()).then(|| name.to_string())
        });

        Some(Self {
            manufacturer,
            product_code: u16::from_le_bytes([bytes[10], bytes[11]]),
            name,
        })
    }

    /// What to call the monitor.
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{} {:04X}", self.manufacturer, self.product_code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edid(name: Option<&str>) -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[..8].copy_from_slice(&HEADER);
        // "DEL": D = 4, E = 5, L = 12
        let id: u16 = (4 << 10) | (5 << 5) | 12;
        bytes[8..10].copy_from_slice(&id.to_be_bytes());
        bytes[10..12].copy_from_slice(&0xA0B1u16.to_le_bytes());

        if let Some(name) = name {
            // The first descriptor is often something else entirely
            bytes[54 + 3] = 0xFD;
            let start = 72;
            bytes[start + 3] = MONITOR_NAME_TAG;
            let mut text = name.as_bytes().to_vec();
            text.push(b'\n');
            text.resize(13, b' ');
            bytes[start + 5..start + 18].copy_from_slice(&text);
        }

        bytes
    }

    #[test]
    fn reads_the_model_name() {
        let parsed = Edid::parse(&edid(Some("U2720Q"))).unwrap();

        assert_eq!(parsed.manufacturer, "DEL");
        assert_eq!(parsed.product_code, 0xA0B1);
        assert_eq!(parsed.name.as_deref(), Some("U2720Q"));
        assert_eq!(parsed.display_name(), "U2720Q");
    }

    #[test]
    fn falls_back_to_manufacturer_and_product() {
        let parsed = Edid::parse(&edid(None)).unwrap();

        assert_eq!(parsed.name, None);
        assert_eq!(parsed.display_name(), "DEL A0B1");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(Edid::parse(&[0; 128]), None);
        assert_eq!(Edid::parse(&[0; 10]), None);
    }
}

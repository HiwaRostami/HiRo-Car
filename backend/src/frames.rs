use std::collections::HashMap;

pub(crate) struct Frame {
    pub ecu: String,
    pub can: bool,
    pub bytes: Vec<u8>,
}

struct Assembly {
    length: usize,
    sequence: u8,
    bytes: Vec<u8>,
}

pub(crate) fn decode(raw: &str) -> Vec<Frame> {
    let mut frames = Vec::new();
    let mut pending: HashMap<String, Assembly> = HashMap::new();
    for line in raw.replace("SEARCHING...", "").split(['\r', '\n']) {
        let mut tokens = line.trim_end_matches('>').split_whitespace();
        let Some(first) = tokens.next() else { continue };
        let header = matches!(first.len(), 3 | 8)
            && first.bytes().all(|byte| byte.is_ascii_hexdigit())
            && tokens.clone().next().is_some();
        let hex = if header {
            tokens.collect::<String>()
        } else {
            std::iter::once(first).chain(tokens).collect()
        };
        if hex.len() % 2 != 0 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let bytes: Vec<u8> = (0..hex.len()).step_by(2)
            .filter_map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
            .collect();
        let Some(&prefix) = bytes.first() else { continue };
        if !header {
            // ELM with headers enabled includes the three legacy header bytes and checksum.
            let legacy = bytes.len() >= 5
                && (matches!(prefix, 0x48 | 0x68) || prefix & 0xC0 == 0x80)
                && bytes[..bytes.len()-1].iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == bytes[bytes.len()-1];
            let (ecu, bytes) = if legacy {
                (format!("{:02X}", bytes[2]), bytes[3..bytes.len()-1].to_vec())
            } else {
                (String::new(), bytes)
            };
            frames.push(Frame { ecu, can: false, bytes });
            continue;
        }

        let ecu = first.to_ascii_uppercase();
        match prefix >> 4 {
            0 => {
                let length = usize::from(prefix & 0x0F);
                if let Some(payload) = bytes.get(1..1 + length).filter(|payload| !payload.is_empty()) {
                    frames.push(Frame { ecu, can: true, bytes: payload.to_vec() });
                }
            }
            1 if bytes.len() >= 3 => {
                let length = (usize::from(prefix & 0x0F) << 8) | usize::from(bytes[1]);
                if length > bytes.len() - 2 {
                    pending.insert(ecu, Assembly { length, sequence: 1, bytes: bytes[2..].to_vec() });
                }
            }
            2 => {
                let Some(mut assembly) = pending.remove(&ecu) else { continue };
                if prefix & 0x0F != assembly.sequence {
                    continue;
                }
                assembly.bytes.extend_from_slice(&bytes[1..]);
                if assembly.bytes.len() >= assembly.length {
                    assembly.bytes.truncate(assembly.length);
                    frames.push(Frame { ecu, can: true, bytes: assembly.bytes });
                } else {
                    assembly.sequence = (assembly.sequence + 1) & 0x0F;
                    pending.insert(ecu, assembly);
                }
            }
            _ => {}
        }
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn preserves_pid_and_data_across_adapter_formats() {
        for input in ["41 0C 17 E8", "410C17E8", "7E8 04 41 0C 17 E8 00", "48 6B 10 41 0C 17 E8 0F"] {
            assert_eq!(decode(input)[0].bytes, [0x41, 0x0C, 0x17, 0xE8], "{input}");
        }
    }

    #[test]
    fn reassembles_frames_by_ecu_and_checks_sequence() {
        let frames = decode("7E8 10 08 43 03 01 00 01 43\r7E9 04 43 01 02 00\r7E8 21 03 00 00");
        assert_eq!(frames[0].bytes, [0x43, 1, 2, 0]);
        assert_eq!(frames[1].bytes, [0x43, 3, 1, 0, 1, 0x43, 3, 0]);
        assert!(decode("7E8 10 08 43 03 01 00 01 43\r7E8 22 03 00").is_empty());
    }
}

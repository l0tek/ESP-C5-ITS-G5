//! Bounded framing for USB chunks and classic PCAP imports.
pub const MAX_FRAME: usize = 2304;
pub const MAX_LINE: usize = 5 + 2 * MAX_FRAME + 1;
pub const MAX_IMPORT: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub struct LineDecoder {
    pending: Vec<u8>,
    discarding: bool,
    pub errors: u64,
}
impl LineDecoder {
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for &byte in chunk {
            if byte == b'\n' {
                if !self.discarding {
                    match decode_line(&self.pending) {
                        Ok(Some(frame)) => frames.push(frame),
                        Err(_) => self.errors += 1,
                        _ => {}
                    }
                }
                self.pending.clear();
                self.discarding = false;
            } else if !self.discarding {
                if self.pending.len() >= MAX_LINE {
                    self.pending.clear();
                    self.discarding = true;
                    self.errors += 1;
                } else {
                    self.pending.push(byte);
                }
            }
        }
        frames
    }
}
pub fn decode_line(line: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let Some(hex) = line.strip_prefix(b"CITS:") else {
        return Ok(None);
    };
    if hex.len() % 2 != 0 || !(48..=MAX_FRAME * 2).contains(&hex.len()) {
        return Err("Ungültige Frame-Länge".into());
    }
    fn digit(b: u8) -> Result<u8, String> {
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            _ => Err("Ungültiges Hex-Zeichen".into()),
        }
    }
    let mut frame = Vec::with_capacity(hex.len() / 2);
    for p in hex.chunks_exact(2) {
        frame.push((digit(p[0])? << 4) | digit(p[1])?);
    }
    Ok(Some(frame))
}
pub fn import_frames(data: &[u8]) -> Result<(Vec<Vec<u8>>, u64), String> {
    if data.len() > MAX_IMPORT {
        return Err("Datei größer als 8 MiB".into());
    }
    let magic = data.get(..4).unwrap_or_default();
    let little = match magic {
        [0xd4, 0xc3, 0xb2, 0xa1] | [0x4d, 0x3c, 0xb2, 0xa1] => Some(true),
        [0xa1, 0xb2, 0xc3, 0xd4] | [0xa1, 0xb2, 0x3c, 0x4d] => Some(false),
        [0x0a, 0x0d, 0x0d, 0x0a] => {
            return Err("PCAPNG bitte zuerst in klassisches PCAP umwandeln".into());
        }
        _ => None,
    };
    let Some(le) = little else {
        let mut decoder = LineDecoder::default();
        let mut frames = decoder.feed(data);
        frames.extend(decoder.feed(b"\n"));
        return Ok((frames, decoder.errors));
    };
    let u32_at = |offset: usize| -> Result<u32, String> {
        let a: [u8; 4] = data
            .get(offset..offset + 4)
            .ok_or("Abgeschnittenes PCAP")?
            .try_into()
            .unwrap();
        Ok(if le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    };
    if data.len() < 24 {
        return Err("Abgeschnittener PCAP-Header".into());
    }
    let version = if le { [2, 0, 4, 0] } else { [0, 2, 0, 4] };
    if data[4..8] != version {
        return Err("PCAP-Version muss 2.4 sein".into());
    }
    if u32_at(20)? != 105 {
        return Err("PCAP benötigt LINKTYPE_IEEE802_11 (105)".into());
    }
    let mut offset = 24;
    let mut frames = Vec::new();
    let mut errors = 0;
    while offset < data.len() {
        if data.len() - offset < 16 {
            return Err("Abgeschnittener Paketheader".into());
        }
        let n = u32_at(offset + 8)? as usize;
        let original = u32_at(offset + 12)? as usize;
        offset += 16;
        let bytes = data
            .get(offset..offset.checked_add(n).ok_or("PCAP-Längenüberlauf")?)
            .ok_or("Abgeschnittenes Paket")?;
        if (24..=MAX_FRAME).contains(&n) && n == original {
            frames.push(bytes.to_vec());
        } else {
            errors += 1;
        }
        offset += n;
    }
    Ok((frames, errors))
}

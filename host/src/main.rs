//! Rust Host: liest CITS:<hex>-Zeilen von USB und schreibt 802.11 PCAP.
//! Build ohne Fremd-Crates: rustc --edition=2021 -O -o cits-pcap host/src/main.rs
use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
    time::{SystemTime, UNIX_EPOCH},
};
fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("odd hex length".into());
    }
    s.as_bytes()
        .chunks_exact(2)
        .map(|c| {
            let h = std::str::from_utf8(c).map_err(|e| e.to_string())?;
            u8::from_str_radix(h, 16).map_err(|e| e.to_string())
        })
        .collect()
}
fn write_global(w: &mut impl Write) -> io::Result<()> {
    w.write_all(&0xa1b2c3d4u32.to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&4u16.to_le_bytes())?;
    w.write_all(&0i32.to_le_bytes())?;
    w.write_all(&0u32.to_le_bytes())?;
    w.write_all(&65535u32.to_le_bytes())?;
    w.write_all(&105u32.to_le_bytes())?; // LINKTYPE_IEEE802_11
    Ok(())
}
fn packet(w: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    w.write_all(&(now.as_secs() as u32).to_le_bytes())?;
    w.write_all(&now.subsec_micros().to_le_bytes())?;
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(bytes)
}
fn consume<R: BufRead>(r: R, dst: &mut impl Write) -> io::Result<(u64, u64)> {
    let (mut ok, mut bad) = (0, 0);
    for line in r.lines() {
        let line = line?;
        if let Some(payload) = line.trim().strip_prefix("CITS:") {
            match hex_decode(payload) {
                Ok(bytes) if (24..=2304).contains(&bytes.len()) => {
                    packet(dst, &bytes)?;
                    dst.flush()?;
                    ok += 1;
                }
                _ => {
                    bad += 1;
                    eprintln!("invalid CITS packet");
                }
            }
        }
    }
    Ok((ok, bad))
}
fn run() -> io::Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 3 {
        eprintln!(
            "Usage: {} <serial-device-or-logfile|-> <capture.pcap>",
            args[0]
        );
        std::process::exit(2)
    }
    let input: Box<dyn BufRead> = if args[1] == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(&args[1])?))
    };
    let out = File::create_new(&args[2])?;
    let mut w = BufWriter::new(out);
    write_global(&mut w)?;
    w.flush()?;
    eprintln!("Capturing 802.11 frames to {}. Stop with Ctrl+C.", args[2]);
    let (good, bad) = consume(input, &mut w)?;
    w.flush()?;
    eprintln!("Frames written: {good}, invalid: {bad}");
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_hex() {
        assert_eq!(hex_decode("08ff00").unwrap(), vec![8, 255, 0]);
    }
    #[test]
    fn odd_hex() {
        assert!(hex_decode("f").is_err());
    }
    #[test]
    fn global_size() {
        let mut b = Vec::new();
        write_global(&mut b).unwrap();
        assert_eq!(b.len(), 24);
        assert_eq!(&b[20..24], &105u32.to_le_bytes());
    }
    #[test]
    fn packet_size() {
        let mut b = Vec::new();
        packet(&mut b, &[1, 2, 3]).unwrap();
        assert_eq!(b.len(), 19);
    }
}

//! A tiny, dependency-free WebSocket (RFC 6455) server helper: the handshake
//! (SHA-1 + base64) and minimal text-frame read/write. Enough to let browsers
//! connect to the authoritative game server over `ws://`.
//!
//! Scope: single text frames in/out, plus ping/close handling. No fragmentation,
//! no extensions, no TLS. Good for a localhost multiplayer demo.

use std::io::{self, Read, Write};

// ---------------------------------------------------------------------------
// SHA-1 (for the Sec-WebSocket-Accept handshake)
// ---------------------------------------------------------------------------

/// SHA-1 digest of `data` (20 bytes).
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let ml = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&ml.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, wi) in w.iter_mut().enumerate().take(16) {
            *wi = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Standard base64 encoding.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// Compute `Sec-WebSocket-Accept` from the client's `Sec-WebSocket-Key`.
pub fn accept_key(key: &str) -> String {
    const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    base64(&sha1(format!("{key}{GUID}").as_bytes()))
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// A message read from the socket.
pub enum Msg {
    Text(String),
    Close,
    /// A control frame we handled internally (e.g. ping->pong); caller ignores.
    Other,
}

/// Read one WebSocket frame. Unmasks client payloads. Returns `Ok(None)` on EOF.
pub fn read_frame<R: Read + Write>(s: &mut R) -> io::Result<Option<Msg>> {
    let mut hdr = [0u8; 2];
    if !read_exact_eof(s, &mut hdr)? {
        return Ok(None);
    }
    let opcode = hdr[0] & 0x0f;
    let masked = hdr[1] & 0x80 != 0;
    let mut len = (hdr[1] & 0x7f) as usize;
    if len == 126 {
        let mut e = [0u8; 2];
        s.read_exact(&mut e)?;
        len = u16::from_be_bytes(e) as usize;
    } else if len == 127 {
        let mut e = [0u8; 8];
        s.read_exact(&mut e)?;
        len = u64::from_be_bytes(e) as usize;
    }
    if len > 1 << 20 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut mask = [0u8; 4];
    if masked {
        s.read_exact(&mut mask)?;
    }
    let mut payload = vec![0u8; len];
    s.read_exact(&mut payload)?;
    if masked {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i & 3];
        }
    }
    match opcode {
        0x1 => Ok(Some(Msg::Text(String::from_utf8_lossy(&payload).into_owned()))),
        0x8 => Ok(Some(Msg::Close)),
        0x9 => {
            write_frame(s, 0xA, &payload)?; // ping -> pong
            Ok(Some(Msg::Other))
        }
        _ => Ok(Some(Msg::Other)),
    }
}

/// Write a server text frame (unmasked, single frame).
pub fn write_text<W: Write>(s: &mut W, text: &str) -> io::Result<()> {
    write_frame(s, 0x1, text.as_bytes())
}

/// Write a ping frame. Browsers reply with a pong automatically, so the server can use
/// this as a heartbeat to detect a peer that vanished without a clean close.
pub fn write_ping<W: Write>(s: &mut W) -> io::Result<()> {
    write_frame(s, 0x9, &[])
}

fn write_frame<W: Write>(s: &mut W, opcode: u8, payload: &[u8]) -> io::Result<()> {
    let mut frame = vec![0x80 | opcode]; // FIN + opcode
    let n = payload.len();
    if n < 126 {
        frame.push(n as u8);
    } else if n < 65536 {
        frame.push(126);
        frame.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(n as u64).to_be_bytes());
    }
    frame.extend_from_slice(payload);
    s.write_all(&frame)
}

fn read_exact_eof<R: Read>(s: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match s.read(&mut buf[filled..]) {
            Ok(0) => return Ok(false),
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_known_vector() {
        // FIPS test vector: SHA1("abc")
        let d = sha1(b"abc");
        let hex: String = d.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn base64_basic() {
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"a"), "YQ==");
    }

    #[test]
    fn rfc6455_accept_example() {
        // From RFC 6455 §1.3.
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }
}

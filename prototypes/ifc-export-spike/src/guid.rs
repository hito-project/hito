//! IfcGloballyUniqueId: a 128-bit UUID compressed to 22 characters.
//!
//! The first character holds the top 2 bits, then 21 characters of 6 bits each,
//! using IFC's own base-64 alphabet (not RFC 4648).

const ALPHABET: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

pub fn compress(uuid: u128) -> String {
    let mut out = String::with_capacity(22);
    out.push(ALPHABET[(uuid >> 126) as usize] as char);
    for i in (0..21).rev() {
        out.push(ALPHABET[((uuid >> (i * 6)) & 0x3F) as usize] as char);
    }
    out
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn expand(guid: &str) -> Option<u128> {
    if guid.len() != 22 {
        return None;
    }
    let mut v: u128 = 0;
    for (i, b) in guid.bytes().enumerate() {
        let d = ALPHABET.iter().position(|&a| a == b)? as u128;
        if i == 0 && d > 3 {
            return None;
        }
        v = (v << if i == 0 { 0 } else { 6 }) | d;
    }
    Some(v)
}

/// A deterministic UUID for the spike, so output files are reproducible.
/// HITO itself would use the element's stored UUID v7 (element model §2).
pub fn from_seed(seed: &str) -> u128 {
    fn fnv(seed: &str, basis: u64) -> u64 {
        let mut h = basis;
        for b in seed.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        // splitmix64 finaliser for better bit spread
        h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94d049bb133111eb);
        h ^ (h >> 31)
    }
    let hi = fnv(seed, 0xcbf29ce484222325) as u128;
    let lo = fnv(seed, 0x84222325cbf29ce4) as u128;
    let mut u = (hi << 64) | lo;
    // Mark it as a version 8 (custom) UUID, RFC 9562 variant.
    u = (u & !(0xF << 76)) | (0x8 << 76);
    u = (u & !(0x3 << 62)) | (0x2 << 62);
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(compress(0), "0000000000000000000000");
        assert_eq!(compress(u128::MAX), "3$$$$$$$$$$$$$$$$$$$$$");
        // Checked against ifcopenshell.guid.compress("0198c3a2-7b1e-7c4d-9f00-123456789abc").
        assert_eq!(compress(0x0198c3a2_7b1e_7c4d_9f00_123456789abc), KNOWN);
    }

    const KNOWN: &str = include_str!("../results/guid-known-value.txt").trim_ascii();

    #[test]
    fn round_trip() {
        for s in ["a", "column C1", "Planta baja"] {
            let u = from_seed(s);
            assert_eq!(expand(&compress(u)), Some(u));
        }
    }
}

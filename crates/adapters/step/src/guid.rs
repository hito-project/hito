//! The 22-character GUID that IFC files use (`IfcGloballyUniqueId`).
//!
//! A 128-bit UUID is written in base 64 with buildingSMART's own alphabet
//! (`0-9`, `A-Z`, `a-z`, `_`, `$`, not RFC 4648's): the first character
//! holds the top 2 bits, then 21 characters hold 6 bits each. This is the
//! same compression as `ifcopenshell.guid.compress`.
//!
//! The UUID is taken as a `u128` in its usual big-endian reading, so this
//! module needs no UUID crate: with the `uuid` crate, pass `uuid.as_u128()`,
//! or `u128::from_be_bytes(bytes)` from the 16 raw bytes.

/// buildingSMART's base-64 alphabet, in digit order.
const ALPHABET: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// The length of a compressed GUID.
pub const LEN: usize = 22;

/// Compresses a UUID into its 22-character IFC GUID.
pub fn compress(uuid: u128) -> String {
    let mut out = String::with_capacity(LEN);
    out.push(char::from(ALPHABET[(uuid >> 126) as usize]));
    for i in (0..LEN - 1).rev() {
        out.push(char::from(ALPHABET[((uuid >> (i * 6)) & 0x3F) as usize]));
    }
    out
}

/// Expands a 22-character IFC GUID back into its UUID. Returns `None` if it
/// isn't 22 characters of the alphabet, or the first character is above `3`
/// (more than 128 bits).
pub fn expand(guid: &str) -> Option<u128> {
    if guid.len() != LEN {
        return None;
    }
    let mut uuid: u128 = 0;
    for (i, byte) in guid.bytes().enumerate() {
        let digit = ALPHABET.iter().position(|&a| a == byte)? as u128;
        if i == 0 && digit > 3 {
            return None;
        }
        uuid = (uuid << 6) | digit;
    }
    Some(uuid)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ifcopenshell.guid.compress("0198c3a2-7b1e-7c4d-9f00-123456789abc")`,
    /// recorded in spike #3 (tag spike/3-ifc-export, results/guid-known-value.txt).
    const IFCOPENSHELL_KNOWN: (u128, &str) = (
        0x0198c3a2_7b1e_7c4d_9f00_123456789abc,
        "01cCEYUnvyJPy04ZHMU9gy",
    );

    // Carried over from spike #3 (tag spike/3-ifc-export, src/guid.rs).
    #[test]
    fn known_values() {
        assert_eq!(compress(0), "0000000000000000000000");
        assert_eq!(compress(u128::MAX), "3$$$$$$$$$$$$$$$$$$$$$");
        let (uuid, guid) = IFCOPENSHELL_KNOWN;
        assert_eq!(compress(uuid), guid);
        assert_eq!(expand(guid), Some(uuid));
    }

    #[test]
    fn from_raw_bytes() {
        let bytes = [
            0x01, 0x98, 0xc3, 0xa2, 0x7b, 0x1e, 0x7c, 0x4d, 0x9f, 0x00, 0x12, 0x34, 0x56, 0x78,
            0x9a, 0xbc,
        ];
        assert_eq!(compress(u128::from_be_bytes(bytes)), IFCOPENSHELL_KNOWN.1);
    }

    #[test]
    fn round_trip() {
        // A fixed xorshift sequence covers the bit patterns without a dependency.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..10_000 {
            let uuid = (u128::from(next()) << 64) | u128::from(next());
            let guid = compress(uuid);
            assert_eq!(guid.len(), LEN);
            assert!(guid.bytes().all(|b| ALPHABET.contains(&b)));
            assert_eq!(expand(&guid), Some(uuid));
        }
        for uuid in [0, 1, u128::MAX, 1 << 127, u128::MAX >> 1] {
            assert_eq!(expand(&compress(uuid)), Some(uuid));
        }
    }

    #[test]
    fn expand_rejects_bad_input() {
        assert_eq!(expand(""), None);
        assert_eq!(expand("000000000000000000000"), None); // 21 characters
        assert_eq!(expand("00000000000000000000000"), None); // 23 characters
        assert_eq!(expand("4000000000000000000000"), None); // over 128 bits
        assert_eq!(expand("000000000000000000000-"), None); // not in the alphabet
        assert_eq!(expand("00000000000000000000ñ"), None); // 22 bytes, not ASCII
    }
}

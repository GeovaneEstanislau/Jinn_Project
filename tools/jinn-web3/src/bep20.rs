//! BEP-20 / ERC-20 ABI Calldata Encoder and Decoder for Jinn OS
//! Designed for zero-overhead execution in Jinn userspace (Ring 3).

#![allow(dead_code)]

pub const SELECTOR_TOTAL_SUPPLY: [u8; 4] = [0x18, 0x16, 0x0d, 0xdd];
pub const SELECTOR_BALANCE_OF: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];
pub const SELECTOR_TRANSFER: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];
pub const SELECTOR_BURN: [u8; 4] = [0x42, 0x96, 0x6c, 0x68];
pub const SELECTOR_GET_OWNER: [u8; 4] = [0x89, 0x3d, 0x20, 0xe8];
pub const SELECTOR_DECIMALS: [u8; 4] = [0x31, 0x3c, 0xe5, 0x67];
pub const SELECTOR_SYMBOL: [u8; 4] = [0x95, 0xd8, 0x9b, 0x41];
pub const SELECTOR_NAME: [u8; 4] = [0x06, 0xfd, 0xde, 0x03];

/// Encodes `balanceOf(address)` calldata.
/// Output: 4 bytes selector + 32 bytes left-padded address = 36 bytes.
pub fn encode_balance_of(account: &[u8; 20]) -> [u8; 36] {
    let mut data = [0u8; 36];
    data[..4].copy_from_slice(&SELECTOR_BALANCE_OF);
    data[16..36].copy_from_slice(account);
    data
}

/// Encodes `transfer(address,uint256)` calldata.
/// Output: 4 bytes selector + 32 bytes address + 32 bytes amount = 68 bytes.
pub fn encode_transfer(recipient: &[u8; 20], amount_wei: &[u8; 32]) -> [u8; 68] {
    let mut data = [0u8; 68];
    data[..4].copy_from_slice(&SELECTOR_TRANSFER);
    data[16..36].copy_from_slice(recipient);
    data[36..68].copy_from_slice(amount_wei);
    data
}

/// Encodes `burn(uint256)` calldata.
/// Output: 4 bytes selector + 32 bytes amount = 36 bytes.
pub fn encode_burn(amount_wei: &[u8; 32]) -> [u8; 36] {
    let mut data = [0u8; 36];
    data[..4].copy_from_slice(&SELECTOR_BURN);
    data[4..36].copy_from_slice(amount_wei);
    data
}

/// Encodes `totalSupply()` calldata.
pub fn encode_total_supply() -> [u8; 4] {
    SELECTOR_TOTAL_SUPPLY
}

/// Parses a 20-byte address from a hex string (with or without '0x').
pub fn parse_address(hex_str: &str) -> Result<[u8; 20], &'static str> {
    let cleaned = if let Some(stripped) = hex_str.strip_prefix("0x") {
        stripped
    } else {
        hex_str
    };

    if cleaned.len() != 40 {
        return Err("Address must be exactly 40 hex characters");
    }

    let mut out = [0u8; 20];
    for (i, chunk) in cleaned.as_bytes().chunks(2).enumerate() {
        let hi = hex_val(chunk[0]).ok_or("Invalid hex character")?;
        let lo = hex_val(chunk[1]).ok_or("Invalid hex character")?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

/// Decodes an EVM 32-byte hexadecimal return data into a 32-byte big-endian buffer.
pub fn decode_hex_u256(hex_str: &str) -> Result<[u8; 32], &'static str> {
    let cleaned = if let Some(stripped) = hex_str.strip_prefix("0x") {
        stripped
    } else {
        hex_str
    };

    if cleaned.len() > 64 {
        return Err("Return hex string too long for U256");
    }

    let mut out = [0u8; 32];
    let pad_len = 64 - cleaned.len();
    let mut full_hex = String::with_capacity(64);
    for _ in 0..pad_len {
        full_hex.push('0');
    }
    full_hex.push_str(cleaned);

    for (i, chunk) in full_hex.as_bytes().chunks(2).enumerate() {
        let hi = hex_val(chunk[0]).ok_or("Invalid hex character")?;
        let lo = hex_val(chunk[1]).ok_or("Invalid hex character")?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

/// Converts a 64-bit integer into a 32-byte big-endian U256 representation.
pub fn u64_to_u256_be(val: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[24..32].copy_from_slice(&val.to_be_bytes());
    bytes
}

/// Converts a 128-bit integer into a 32-byte big-endian U256 representation.
pub fn u128_to_u256_be(val: u128) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[16..32].copy_from_slice(&val.to_be_bytes());
    bytes
}

/// Converts a 32-byte big-endian integer into a u128 (saturating on overflow).
pub fn u256_be_to_u128_saturating(bytes: &[u8; 32]) -> u128 {
    for &b in &bytes[..16] {
        if b != 0 {
            return u128::MAX;
        }
    }
    let mut low = [0u8; 16];
    low.copy_from_slice(&bytes[16..32]);
    u128::from_be_bytes(low)
}

/// Formats a wei balance (18 decimals) into a human readable string.
/// E.g. 1000 * 10^18 -> "1000.00"
pub fn format_jinn_amount(amount_wei: &[u8; 32]) -> String {
    let wei_val = u256_be_to_u128_saturating(amount_wei);
    let scale = 1_000_000_000_000_000_000u128; // 10^18
    let whole = wei_val / scale;
    let fraction = (wei_val % scale) / 10_000_000_000_000_000u128; // 2 decimal digits
    format!("{}.{:02} $JINN", whole, fraction)
}

/// Formats a byte slice as a hex string with '0x' prefix.
pub fn to_hex_string(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(2 + bytes.len() * 2);
    s.push_str("0x");
    for &b in bytes {
        let hi = (b >> 4) & 0x0f;
        let lo = b & 0x0f;
        s.push(hex_char(hi));
        s.push(hex_char(lo));
    }
    s
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_char(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        10..=15 => (b'a' + (v - 10)) as char,
        _ => '0',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_balance_of() {
        let addr = [0x11; 20];
        let calldata = encode_balance_of(&addr);
        assert_eq!(&calldata[..4], &SELECTOR_BALANCE_OF);
        assert_eq!(&calldata[4..16], &[0u8; 12]);
        assert_eq!(&calldata[16..36], &addr);
    }

    #[test]
    fn test_parse_address() {
        let addr_str = "0x71C634C26B331E0cd51a04579997424946344955";
        let parsed = parse_address(addr_str).expect("Valid address");
        assert_eq!(parsed[0], 0x71);
        assert_eq!(parsed[19], 0x55);
    }

    #[test]
    fn test_format_amount() {
        let one_token = u64_to_u256_be(1_000_000_000_000_000_000);
        assert_eq!(format_jinn_amount(&one_token), "1.00 $JINN");
    }
}

//! Pure native RFC 1951 Deflate / RFC 1950 Zlib inflate decompressor.
//!
//! Provides zero-dependency in-memory decompression for PDF `/FlateDecode` streams.

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
pub enum InflateError {
    UnexpectedEof,
    InvalidBlockType,
    InvalidHuffmanTree,
    InvalidDistance,
    CorruptedData,
}

struct BitReader<'a> {
    bytes: &'a [u8],
    byte_pos: usize,
    bit_buf: u32,
    bits_in_buf: u8,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            byte_pos: 0,
            bit_buf: 0,
            bits_in_buf: 0,
        }
    }

    fn read_bits(&mut self, n: u8) -> Result<u32, InflateError> {
        while self.bits_in_buf < n {
            if self.byte_pos >= self.bytes.len() {
                return Err(InflateError::UnexpectedEof);
            }
            self.bit_buf |= (self.bytes[self.byte_pos] as u32) << self.bits_in_buf;
            self.byte_pos += 1;
            self.bits_in_buf += 8;
        }
        let mask = (1u32 << n) - 1;
        let val = self.bit_buf & mask;
        self.bit_buf >>= n;
        self.bits_in_buf -= n;
        Ok(val)
    }

    fn drop_to_byte_boundary(&mut self) {
        let bits_to_drop = self.bits_in_buf % 8;
        self.bit_buf >>= bits_to_drop;
        self.bits_in_buf -= bits_to_drop;
    }

    fn read_u8(&mut self) -> Result<u8, InflateError> {
        Ok(self.read_bits(8)? as u8)
    }

    fn read_u16_le(&mut self) -> Result<u16, InflateError> {
        let b0 = self.read_u8()?;
        let b1 = self.read_u8()?;
        Ok((b0 as u16) | ((b1 as u16) << 8))
    }
}

#[derive(Clone)]
struct HuffmanTree {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl HuffmanTree {
    fn from_code_lengths(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            if l > 0 && (l as usize) < counts.len() {
                counts[l as usize] += 1;
            }
        }

        let mut offsets = [0usize; 16];
        for i in 1..15 {
            offsets[i + 1] = offsets[i] + (counts[i] as usize);
        }

        let total_symbols = offsets[15] + (counts[15] as usize);
        let mut symbols = vec![0u16; total_symbols];

        for (sym, &l) in lengths.iter().enumerate() {
            if l > 0 && (l as usize) < counts.len() {
                let off = offsets[l as usize];
                if off < symbols.len() {
                    symbols[off] = sym as u16;
                    offsets[l as usize] += 1;
                }
            }
        }

        Ok(Self { counts, symbols })
    }

    fn decode_symbol(&self, reader: &mut BitReader) -> Result<u16, InflateError> {
        let mut code = 0u32;
        let mut first = 0u32;
        let mut count_idx = 0usize;

        for len in 1..=15 {
            code |= reader.read_bits(1)?;
            let count = self.counts[len] as u32;
            if code < first + count {
                let sym_idx = count_idx + (code - first) as usize;
                if sym_idx < self.symbols.len() {
                    return Ok(self.symbols[sym_idx]);
                } else {
                    return Err(InflateError::InvalidHuffmanTree);
                }
            }
            first = (first + count) << 1;
            code <<= 1;
            count_idx += count as usize;
        }

        Err(InflateError::InvalidHuffmanTree)
    }
}

// Length extra bits and base values for LZ77
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

// Distance extra bits and base values
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
    13, 13,
];

const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

pub fn decompress_flate(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    // Check for optional ZLIB header (RFC 1950)
    let payload = if data.len() >= 2 && (data[0] & 0x0F) == 8 && ((data[0] as u16 * 256 + data[1] as u16) % 31 == 0) {
        &data[2..]
    } else {
        data
    };

    let mut reader = BitReader::new(payload);
    let mut out = Vec::new();
    let mut is_final = false;

    while !is_final {
        let bfinal = reader.read_bits(1)?;
        is_final = bfinal == 1;
        let btype = reader.read_bits(2)?;

        match btype {
            0 => {
                // Stored (uncompressed)
                reader.drop_to_byte_boundary();
                let len = reader.read_u16_le()? as usize;
                let _nlen = reader.read_u16_le()?;
                for _ in 0..len {
                    out.push(reader.read_u8()?);
                }
            }
            1 => {
                // Fixed Huffman
                let (lit_tree, dist_tree) = build_fixed_trees()?;
                inflate_block(&mut reader, &lit_tree, &dist_tree, &mut out)?;
            }
            2 => {
                // Dynamic Huffman
                let hlit = reader.read_bits(5)? as usize + 257;
                let hdist = reader.read_bits(5)? as usize + 1;
                let hclen = reader.read_bits(4)? as usize + 4;

                let mut cl_lengths = [0u8; 19];
                for i in 0..hclen {
                    cl_lengths[CODE_LENGTH_ORDER[i]] = reader.read_bits(3)? as u8;
                }
                let cl_tree = HuffmanTree::from_code_lengths(&cl_lengths)?;

                let mut lengths = Vec::with_capacity(hlit + hdist);
                while lengths.len() < hlit + hdist {
                    let sym = cl_tree.decode_symbol(&mut reader)?;
                    match sym {
                        0..=15 => lengths.push(sym as u8),
                        16 => {
                            let repeat = reader.read_bits(2)? as usize + 3;
                            let last = *lengths.last().ok_or(InflateError::CorruptedData)?;
                            for _ in 0..repeat {
                                lengths.push(last);
                            }
                        }
                        17 => {
                            let repeat = reader.read_bits(3)? as usize + 3;
                            for _ in 0..repeat {
                                lengths.push(0);
                            }
                        }
                        18 => {
                            let repeat = reader.read_bits(7)? as usize + 11;
                            for _ in 0..repeat {
                                lengths.push(0);
                            }
                        }
                        _ => return Err(InflateError::CorruptedData),
                    }
                }

                let lit_tree = HuffmanTree::from_code_lengths(&lengths[..hlit])?;
                let dist_tree = HuffmanTree::from_code_lengths(&lengths[hlit..])?;
                inflate_block(&mut reader, &lit_tree, &dist_tree, &mut out)?;
            }
            _ => return Err(InflateError::InvalidBlockType),
        }
    }

    Ok(out)
}

fn build_fixed_trees() -> Result<(HuffmanTree, HuffmanTree), InflateError> {
    let mut lit_lens = [0u8; 288];
    for i in 0..=143 {
        lit_lens[i] = 8;
    }
    for i in 144..=255 {
        lit_lens[i] = 9;
    }
    for i in 256..=279 {
        lit_lens[i] = 7;
    }
    for i in 280..=287 {
        lit_lens[i] = 8;
    }
    let lit_tree = HuffmanTree::from_code_lengths(&lit_lens)?;

    let dist_lens = [5u8; 32];
    let dist_tree = HuffmanTree::from_code_lengths(&dist_lens)?;
    Ok((lit_tree, dist_tree))
}

fn inflate_block(
    reader: &mut BitReader,
    lit_tree: &HuffmanTree,
    dist_tree: &HuffmanTree,
    out: &mut Vec<u8>,
) -> Result<(), InflateError> {
    loop {
        let sym = lit_tree.decode_symbol(reader)?;
        match sym {
            0..=255 => {
                out.push(sym as u8);
            }
            256 => {
                // End of block
                break;
            }
            257..=285 => {
                let len_idx = (sym - 257) as usize;
                let extra_bits = LENGTH_EXTRA[len_idx];
                let extra = if extra_bits > 0 {
                    reader.read_bits(extra_bits)? as u16
                } else {
                    0
                };
                let length = (LENGTH_BASE[len_idx] + extra) as usize;

                let dist_sym = dist_tree.decode_symbol(reader)?;
                if dist_sym > 29 {
                    return Err(InflateError::InvalidDistance);
                }
                let dist_idx = dist_sym as usize;
                let dist_extra_bits = DIST_EXTRA[dist_idx];
                let dist_extra = if dist_extra_bits > 0 {
                    reader.read_bits(dist_extra_bits)? as u16
                } else {
                    0
                };
                let distance = (DIST_BASE[dist_idx] + dist_extra) as usize;

                if distance > out.len() {
                    return Err(InflateError::InvalidDistance);
                }

                let start = out.len() - distance;
                for i in 0..length {
                    let val = out[start + i];
                    out.push(val);
                }
            }
            _ => return Err(InflateError::CorruptedData),
        }
    }
    Ok(())
}

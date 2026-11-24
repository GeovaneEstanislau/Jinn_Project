//! Descompressor DEFLATE / GZIP puro `#![no_std]` (RFC 1951 / RFC 1952)
//!
//! Implementação autônoma sem alocador (zero-heap) para descompressão de
//! respostas HTTP com `Content-Encoding: gzip` ou `deflate`.

pub fn is_gzip(data: &[u8]) -> bool {
    data.len() >= 10 && data[0] == 0x1f && data[1] == 0x8b && data[2] == 8
}

pub fn is_zlib(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x78 && (data[1] == 0x9c || data[1] == 0x01 || data[1] == 0xda || data[1] == 0x5e)
}

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u32,
    bits_count: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bits_count: 0,
        }
    }

    fn ensure_bits(&mut self, n: u8) -> bool {
        while self.bits_count < n {
            if self.byte_pos >= self.data.len() {
                return false;
            }
            self.bit_buf |= (self.data[self.byte_pos] as u32) << self.bits_count;
            self.byte_pos += 1;
            self.bits_count += 8;
        }
        true
    }

    fn read_bits(&mut self, n: u8) -> Result<u32, ()> {
        if n == 0 {
            return Ok(0);
        }
        if !self.ensure_bits(n) {
            return Err(());
        }
        let val = self.bit_buf & ((1 << n) - 1);
        self.bit_buf >>= n;
        self.bits_count -= n;
        Ok(val)
    }

    fn align_byte(&mut self) {
        let rem = self.bits_count % 8;
        if rem > 0 {
            self.bit_buf >>= rem;
            self.bits_count -= rem;
        }
    }

    fn read_u16(&mut self) -> Result<u16, ()> {
        self.align_byte();
        let b0 = self.read_bits(8)? as u16;
        let b1 = self.read_bits(8)? as u16;
        Ok(b0 | (b1 << 8))
    }
}

// Tabela de Huffman de busca direta (máx 15 bits de profundidade)
struct HuffmanTable {
    counts: [u16; 16],
    symbols: [u16; 320],
}

impl HuffmanTable {
    const fn new() -> Self {
        Self {
            counts: [0; 16],
            symbols: [0; 320],
        }
    }

    fn build(&mut self, lengths: &[u8]) -> Result<(), ()> {
        self.counts = [0; 16];
        self.symbols = [0; 320];

        for &len in lengths {
            if len > 0 && len < 16 {
                self.counts[len as usize] += 1;
            }
        }

        let mut offsets = [0usize; 16];
        let mut total = 0usize;
        for i in 1..16 {
            offsets[i] = total;
            total += self.counts[i] as usize;
        }

        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 && len < 16 {
                let off = offsets[len as usize];
                if off < self.symbols.len() {
                    self.symbols[off] = sym as u16;
                    offsets[len as usize] += 1;
                }
            }
        }

        Ok(())
    }

    fn decode(&self, reader: &mut BitReader) -> Result<u16, ()> {
        let mut code: u32 = 0;
        let mut first: u32 = 0;
        let mut count_idx = 0usize;

        for len in 1..16 {
            let bit = reader.read_bits(1)?;
            code = (code << 1) | bit;
            let count = self.counts[len] as u32;

            if code < first + count {
                let offset = count_idx + (code - first) as usize;
                if offset < self.symbols.len() {
                    return Ok(self.symbols[offset]);
                } else {
                    return Err(());
                }
            }

            first = (first + count) << 1;
            count_idx += count as usize;
        }

        Err(())
    }
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

const CODE_LEN_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn inflate_stream(mut reader: BitReader, out: &mut [u8]) -> Result<usize, ()> {
    let mut out_pos = 0usize;

    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;

        match btype {
            0 => {
                // Stored (sem compressão)
                reader.align_byte();
                let len = reader.read_u16()? as usize;
                let nlen = reader.read_u16()? as usize;
                if (len ^ nlen) != 0xFFFF {
                    return Err(());
                }
                for _ in 0..len {
                    let b = reader.read_bits(8)? as u8;
                    if out_pos < out.len() {
                        out[out_pos] = b;
                        out_pos += 1;
                    }
                }
            }
            1 => {
                // Huffman Estático (RFC 1951 Seção 3.2.6)
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

                let mut lit_table = HuffmanTable::new();
                lit_table.build(&lit_lens)?;

                let dist_lens = [5u8; 32];
                let mut dist_table = HuffmanTable::new();
                dist_table.build(&dist_lens)?;

                decode_block(&mut reader, &lit_table, &dist_table, out, &mut out_pos)?;
            }
            2 => {
                // Huffman Dinâmico (RFC 1951 Seção 3.2.7)
                let hlit = (reader.read_bits(5)? + 257) as usize;
                let hdist = (reader.read_bits(5)? + 1) as usize;
                let hclen = (reader.read_bits(4)? + 4) as usize;

                let mut code_lengths = [0u8; 19];
                for i in 0..hclen {
                    code_lengths[CODE_LEN_ORDER[i]] = reader.read_bits(3)? as u8;
                }

                let mut cl_table = HuffmanTable::new();
                cl_table.build(&code_lengths)?;

                let total_codes = hlit + hdist;
                let mut all_lengths = [0u8; 320];
                let mut idx = 0;

                while idx < total_codes {
                    let sym = cl_table.decode(&mut reader)?;
                    if sym <= 15 {
                        all_lengths[idx] = sym as u8;
                        idx += 1;
                    } else if sym == 16 {
                        if idx == 0 {
                            return Err(());
                        }
                        let prev = all_lengths[idx - 1];
                        let repeat = 3 + reader.read_bits(2)? as usize;
                        for _ in 0..repeat {
                            if idx < total_codes {
                                all_lengths[idx] = prev;
                                idx += 1;
                            }
                        }
                    } else if sym == 17 {
                        let repeat = 3 + reader.read_bits(3)? as usize;
                        for _ in 0..repeat {
                            if idx < total_codes {
                                all_lengths[idx] = 0;
                                idx += 1;
                            }
                        }
                    } else if sym == 18 {
                        let repeat = 11 + reader.read_bits(7)? as usize;
                        for _ in 0..repeat {
                            if idx < total_codes {
                                all_lengths[idx] = 0;
                                idx += 1;
                            }
                        }
                    } else {
                        return Err(());
                    }
                }

                let mut lit_table = HuffmanTable::new();
                lit_table.build(&all_lengths[..hlit])?;

                let mut dist_table = HuffmanTable::new();
                dist_table.build(&all_lengths[hlit..total_codes])?;

                decode_block(&mut reader, &lit_table, &dist_table, out, &mut out_pos)?;
            }
            _ => return Err(()),
        }

        if bfinal != 0 {
            break;
        }
    }

    Ok(out_pos)
}

fn decode_block(
    reader: &mut BitReader,
    lit_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    out: &mut [u8],
    out_pos: &mut usize,
) -> Result<(), ()> {
    loop {
        let sym = lit_table.decode(reader)?;
        if sym < 256 {
            if *out_pos < out.len() {
                out[*out_pos] = sym as u8;
                *out_pos += 1;
            }
        } else if sym == 256 {
            // Fim do bloco
            break;
        } else {
            // Referência LZ77 (comprimento + distância)
            let len_idx = (sym - 257) as usize;
            if len_idx >= LENGTH_BASE.len() {
                return Err(());
            }
            let extra_bits = LENGTH_EXTRA[len_idx];
            let length = LENGTH_BASE[len_idx] as usize + reader.read_bits(extra_bits)? as usize;

            let dist_sym = dist_table.decode(reader)? as usize;
            if dist_sym >= DIST_BASE.len() {
                return Err(());
            }
            let dist_extra = DIST_EXTRA[dist_sym];
            let distance = DIST_BASE[dist_sym] as usize + reader.read_bits(dist_extra)? as usize;

            if distance == 0 || distance > *out_pos {
                return Err(());
            }

            let start = *out_pos - distance;
            for i in 0..length {
                if *out_pos < out.len() {
                    let b = out[start + i];
                    out[*out_pos] = b;
                    *out_pos += 1;
                }
            }
        }
    }
    Ok(())
}

/// Descomprime payload completo em formato GZIP (RFC 1952)
pub fn decompress_gzip(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    if !is_gzip(input) {
        return Err(());
    }

    let flg = input[3];
    let mut offset = 10usize;

    // FEXTRA
    if (flg & 0x04) != 0 {
        if offset + 2 > input.len() {
            return Err(());
        }
        let xlen = (input[offset] as usize) | ((input[offset + 1] as usize) << 8);
        offset += 2 + xlen;
    }

    // FNAME
    if (flg & 0x08) != 0 {
        while offset < input.len() && input[offset] != 0 {
            offset += 1;
        }
        offset += 1;
    }

    // FCOMMENT
    if (flg & 0x10) != 0 {
        while offset < input.len() && input[offset] != 0 {
            offset += 1;
        }
        offset += 1;
    }

    // FHCRC
    if (flg & 0x02) != 0 {
        offset += 2;
    }

    if offset >= input.len() {
        return Err(());
    }

    // Os últimos 8 bytes são CRC32 e ISIZE
    let deflate_data = if input.len() > offset + 8 {
        &input[offset..input.len() - 8]
    } else {
        &input[offset..]
    };

    let reader = BitReader::new(deflate_data);
    inflate_stream(reader, out)
}

/// Descomprime stream Deflate bruta ou com header zlib
pub fn decompress_deflate(input: &[u8], out: &mut [u8]) -> Result<usize, ()> {
    let data = if is_zlib(input) {
        &input[2..]
    } else {
        input
    };
    let reader = BitReader::new(data);
    inflate_stream(reader, out)
}

/// Descomprime de forma automática se for GZIP, ZLIB ou retorna cópia direta se plano
pub fn auto_decompress(input: &[u8], out: &mut [u8]) -> usize {
    if is_gzip(input) {
        if let Ok(n) = decompress_gzip(input, out) {
            return n;
        }
    } else if is_zlib(input) {
        if let Ok(n) = decompress_deflate(input, out) {
            return n;
        }
    }

    let copy_len = input.len().min(out.len());
    out[..copy_len].copy_from_slice(&input[..copy_len]);
    copy_len
}

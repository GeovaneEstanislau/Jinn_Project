//! Pure Rust no_std RSA public key parsing and cryptographic signature verification (RFC 8017 / PKCS#1 v1.5 / RSA-PSS)

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::cmp::Ordering;
use crate::bignum::BigUint;
use crate::sha256::Sha256;

/// Chave pública RSA
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RSAPublicKey {
    pub n: BigUint, // Módulo N
    pub e: BigUint, // Expoente público e (geralmente 65537)
}

impl RSAPublicKey {
    pub fn new(n: BigUint, e: BigUint) -> Self {
        Self { n, e }
    }

    /// Retorna o tamanho do módulo em bytes (ex: 256 para RSA-2048, 512 para RSA-4096)
    pub fn modulus_len_bytes(&self) -> usize {
        (self.n.bit_length() + 7) / 8
    }

    /// Verifica assinatura RSA PKCS#1 v1.5 com resumo SHA-256 (RFC 8017 EMSA-PKCS1-v1_5)
    pub fn verify_pkcs1v15_sha256(&self, message_hash: &[u8; 32], signature: &[u8]) -> Result<(), &'static str> {
        let k = self.modulus_len_bytes();
        if signature.len() != k {
            return Err("RSA PKCS#1 v1.5: tamanho de assinatura incompativel com o modulo");
        }

        let s = BigUint::from_be_bytes(signature);
        if s.cmp(&self.n) != Ordering::Less {
            return Err("RSA PKCS#1 v1.5: assinatura maior ou igual ao modulo");
        }

        // Recupera EM = S^e mod N
        let em_uint = s.mod_exp(&self.e, &self.n);
        let em = em_uint.to_be_bytes_padded(k).ok_or("Falha ao alinhar EM RSA")?;

        // Validação da formatação PKCS#1 v1.5:
        // EM = 0x00 || 0x01 || PS (0xFF ... 0xFF) || 0x00 || DigestInfo
        // DigestInfo para SHA-256 (19 bytes):
        // 30 31 30 0d 06 09 60 86 48 01 65 03 04 02 01 05 00 04 20 || [32 bytes hash]
        const SHA256_DIGEST_INFO_PREFIX: [u8; 19] = [
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01,
            0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20,
        ];

        let t_len = 19 + 32; // 51 bytes
        if k < t_len + 11 {
            return Err("Modulo RSA muito curto para PKCS#1 v1.5 com SHA-256");
        }

        if em[0] != 0x00 || em[1] != 0x01 {
            return Err("RSA PKCS#1 v1.5: cabecalho 0x00 0x01 invalido");
        }

        let ps_end = k - t_len - 1;
        for &b in &em[2..ps_end] {
            if b != 0xFF {
                return Err("RSA PKCS#1 v1.5: padding PS corrompido");
            }
        }

        if em[ps_end] != 0x00 {
            return Err("RSA PKCS#1 v1.5: separador 0x00 ausente apos PS");
        }

        let digest_info = &em[ps_end + 1..];
        if digest_info.len() != t_len {
            return Err("RSA PKCS#1 v1.5: tamanho de DigestInfo incorreto");
        }

        if &digest_info[..19] != SHA256_DIGEST_INFO_PREFIX {
            return Err("RSA PKCS#1 v1.5: OID/DigestInfo SHA-256 divergente");
        }

        if &digest_info[19..] != message_hash {
            return Err("RSA PKCS#1 v1.5: hash da mensagem nao coincide com assinatura");
        }

        Ok(())
    }

    /// Verifica assinatura RSA-PSS com SHA-256 (RFC 8017 EMSA-PSS)
    pub fn verify_pss_sha256(&self, message_hash: &[u8; 32], signature: &[u8]) -> Result<(), &'static str> {
        let k = self.modulus_len_bytes();
        if signature.len() != k {
            return Err("RSA-PSS: tamanho de assinatura incompativel com o modulo");
        }

        let s = BigUint::from_be_bytes(signature);
        if s.cmp(&self.n) != Ordering::Less {
            return Err("RSA-PSS: assinatura maior ou igual ao modulo");
        }

        let em_uint = s.mod_exp(&self.e, &self.n);
        let em = em_uint.to_be_bytes_padded(k).ok_or("Falha ao alinhar EM RSA-PSS")?;

        let h_len = 32;
        let s_len = 32; // Comprimento padrao de salt para SHA-256
        if k < h_len + s_len + 2 {
            return Err("Modulo RSA muito curto para RSA-PSS");
        }

        // Verifica trailer byte 0xBC
        if em[k - 1] != 0xBC {
            return Err("RSA-PSS: byte trailer 0xBC ausente");
        }

        let masked_db = &em[..k - h_len - 1];
        let h = &em[k - h_len - 1..k - 1];

        // MGF1 com SHA-256 sobre H
        let db_mask = mgf1_sha256(h, masked_db.len());
        #[cfg(not(feature = "std"))]
        let mut db = alloc::vec![0u8; masked_db.len()];
        #[cfg(feature = "std")]
        let mut db = vec![0u8; masked_db.len()];

        for i in 0..masked_db.len() {
            db[i] = masked_db[i] ^ db_mask[i];
        }

        // Zera os bits de padding do topo do DB conforme modulo
        let mod_bits = self.n.bit_length();
        let unused_bits = (8 * k) - (mod_bits - 1);
        if unused_bits > 0 && unused_bits < 8 {
            let mask = 0xFF >> unused_bits;
            if (db[0] & !mask) != 0 {
                return Err("RSA-PSS: bits de topo de DB nao zerados");
            }
        }

        // DB deve ser: 0x00 ... 0x00 || 0x01 || salt
        let mut split_idx = None;
        for i in 0..db.len() {
            if db[i] == 0x01 {
                split_idx = Some(i);
                break;
            } else if db[i] != 0x00 {
                return Err("RSA-PSS: octetos nao nulos antes do delimitador 0x01");
            }
        }

        let one_idx = split_idx.ok_or("RSA-PSS: delimitador 0x01 nao encontrado")?;
        let salt = &db[one_idx + 1..];

        // M' = 0x00_8 || mHash || salt
        let mut m_prime = Vec::with_capacity(8 + 32 + salt.len());
        m_prime.extend_from_slice(&[0u8; 8]);
        m_prime.extend_from_slice(message_hash);
        m_prime.extend_from_slice(salt);

        let h_prime = Sha256::digest(&m_prime);
        if h_prime != h {
            return Err("RSA-PSS: hash reconstruido H' nao confere com H");
        }

        Ok(())
    }
}

/// MGF1 (Mask Generation Function 1) com SHA-256 (RFC 8017)
fn mgf1_sha256(seed: &[u8], mask_len: usize) -> Vec<u8> {
    let mut mask = Vec::with_capacity(mask_len);
    let mut counter: u32 = 0;

    while mask.len() < mask_len {
        let mut data = Vec::with_capacity(seed.len() + 4);
        data.extend_from_slice(seed);
        data.extend_from_slice(&counter.to_be_bytes());
        let hash = Sha256::digest(&data);

        let to_copy = core::cmp::min(hash.len(), mask_len - mask.len());
        mask.extend_from_slice(&hash[..to_copy]);
        counter += 1;
    }

    mask
}

/// Decodificador de RSAPublicKey a partir de ASN.1 DER (PKCS#1: SEQUENCE { modulus INTEGER, publicExponent INTEGER })
pub fn parse_pkcs1_rsa_public_key(der: &[u8]) -> Result<RSAPublicKey, &'static str> {
    let mut reader = DerReader::new(der);
    let seq = reader.read_sequence()?;
    let mut seq_reader = DerReader::new(seq);

    let mod_elem = seq_reader.read_integer()?;
    let exp_elem = seq_reader.read_integer()?;

    let n = BigUint::from_be_bytes(mod_elem);
    let e = BigUint::from_be_bytes(exp_elem);

    if n.is_zero() || e.is_zero() {
        return Err("Parametros RSA invalidos (modulus ou exponent zerados)");
    }

    Ok(RSAPublicKey::new(n, e))
}

struct DerReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> DerReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_element(&mut self) -> Result<(u8, &'a [u8]), &'static str> {
        if self.offset >= self.data.len() {
            return Err("EOF lendo DER");
        }
        let tag = self.data[self.offset];
        self.offset += 1;

        if self.offset >= self.data.len() {
            return Err("EOF lendo tamanho DER");
        }
        let first_len = self.data[self.offset];
        self.offset += 1;

        let len = if first_len & 0x80 == 0 {
            first_len as usize
        } else {
            let num_octets = (first_len & 0x7F) as usize;
            if num_octets > 4 || self.offset + num_octets > self.data.len() {
                return Err("Octetos de tamanho DER invalidos");
            }
            let mut l = 0usize;
            for _ in 0..num_octets {
                l = (l << 8) | (self.data[self.offset] as usize);
                self.offset += 1;
            }
            l
        };

        if self.offset + len > self.data.len() {
            return Err("Tamanho de elemento DER ultrapassa o buffer");
        }

        let elem_data = &self.data[self.offset..self.offset + len];
        self.offset += len;

        Ok((tag, elem_data))
    }

    fn read_sequence(&mut self) -> Result<&'a [u8], &'static str> {
        let (tag, data) = self.read_element()?;
        if tag == 0x30 {
            Ok(data)
        } else {
            Err("Tag SEQUENCE (0x30) esperada")
        }
    }

    fn read_integer(&mut self) -> Result<&'a [u8], &'static str> {
        let (tag, data) = self.read_element()?;
        if tag == 0x02 {
            Ok(data)
        } else {
            Err("Tag INTEGER (0x02) esperada")
        }
    }
}

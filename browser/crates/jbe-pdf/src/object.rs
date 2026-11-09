//! PDF primitive object types.

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Fundamental types representable in PDF file structure.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfObject {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<PdfObject>),
    Dictionary(BTreeMap<String, PdfObject>),
    Stream(BTreeMap<String, PdfObject>, Vec<u8>),
    Reference(u32, u16),
}

impl PdfObject {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PdfObject::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PdfObject::Integer(i) => Some(*i as f64),
            PdfObject::Real(r) => Some(*r),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&str> {
        match self {
            PdfObject::Name(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            PdfObject::String(bytes) => core::str::from_utf8(bytes).ok(),
            PdfObject::Name(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&BTreeMap<String, PdfObject>> {
        match self {
            PdfObject::Dictionary(d) => Some(d),
            PdfObject::Stream(d, _) => Some(d),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[PdfObject]> {
        match self {
            PdfObject::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    pub fn as_stream(&self) -> Option<(&BTreeMap<String, PdfObject>, &[u8])> {
        match self {
            PdfObject::Stream(d, s) => Some((d, s.as_slice())),
            _ => None,
        }
    }

    pub fn as_ref(&self) -> Option<(u32, u16)> {
        match self {
            PdfObject::Reference(obj_num, gen) => Some((*obj_num, *gen)),
            _ => None,
        }
    }
}

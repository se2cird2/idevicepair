// DebianArch

use std::{
    ffi::CString,
    io::{BufRead, Cursor, Read},
    ops::{BitOr, BitOrAssign},
};

use super::error::XPCError;
use indexmap::IndexMap;
use log::debug;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum XPCFlag {
    AlwaysSet,
    DataFlag,
    WantingReply,
    InitHandshake,

    Custom(u32),
}

impl From<XPCFlag> for u32 {
    fn from(value: XPCFlag) -> Self {
        match value {
            XPCFlag::AlwaysSet => 0x00000001,
            XPCFlag::DataFlag => 0x00000100,
            XPCFlag::WantingReply => 0x00010000,
            XPCFlag::InitHandshake => 0x00400000,
            XPCFlag::Custom(inner) => inner,
        }
    }
}

impl BitOr for XPCFlag {
    fn bitor(self, rhs: Self) -> Self::Output {
        XPCFlag::Custom(u32::from(self) | u32::from(rhs))
    }

    type Output = XPCFlag;
}

impl BitOrAssign for XPCFlag {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.bitor(rhs);
    }
}

impl PartialEq for XPCFlag {
    fn eq(&self, other: &Self) -> bool {
        u32::from(*self) == u32::from(*other)
    }
}

#[repr(u32)]
pub enum XPCType {
    Bool = 0x00002000,
    Dictionary = 0x0000f000,
    Array = 0x0000e000,

    Int64 = 0x00003000,
    UInt64 = 0x00004000,

    String = 0x00009000,
    Data = 0x00008000,
    Uuid = 0x0000a000,
}

impl TryFrom<u32> for XPCType {
    type Error = XPCError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x00002000 => Ok(Self::Bool),
            0x0000f000 => Ok(Self::Dictionary),
            0x0000e000 => Ok(Self::Array),
            0x00003000 => Ok(Self::Int64),
            0x00004000 => Ok(Self::UInt64),
            0x00009000 => Ok(Self::String),
            0x00008000 => Ok(Self::Data),
            0x0000a000 => Ok(Self::Uuid),
            _ => Err("Invalid XPCType")?,
        }
    }
}

pub type Dictionary = IndexMap<String, XPCObject>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum XPCObject {
    Bool(bool),
    Dictionary(Dictionary),
    Array(Vec<XPCObject>),

    Int64(i64),
    UInt64(u64),

    String(String),
    Data(Vec<u8>),
    Uuid(uuid::Uuid),
}

impl From<plist::Value> for XPCObject {
    fn from(value: plist::Value) -> Self {
        match value {
            plist::Value::Array(v) => {
                XPCObject::Array(v.iter().map(|item| XPCObject::from(item.clone())).collect())
            }
            plist::Value::Dictionary(v) => {
                let mut dict = Dictionary::new();
                for (k, v) in v.into_iter() {
                    dict.insert(k.clone(), XPCObject::from(v));
                }
                XPCObject::Dictionary(dict)
            }
            plist::Value::Boolean(v) => XPCObject::Bool(v),
            plist::Value::Data(v) => XPCObject::Data(v),
            plist::Value::Date(_) => todo!(),
            plist::Value::Real(_) => todo!(),
            plist::Value::Integer(v) => XPCObject::Int64(v.as_signed().unwrap()),
            plist::Value::String(v) => XPCObject::String(v),
            plist::Value::Uid(_) => todo!(),
            _ => todo!(),
        }
    }
}

impl XPCObject {
    pub fn to_plist(&self) -> plist::Value {
        match self {
            Self::Bool(v) => plist::Value::Boolean(*v),
            Self::Uuid(uuid) => plist::Value::String(uuid.to_string()),
            Self::UInt64(v) => plist::Value::Integer({ *v }.into()),
            Self::Int64(v) => plist::Value::Integer({ *v }.into()),
            Self::String(v) => plist::Value::String(v.clone()),
            Self::Data(v) => plist::Value::Data(v.clone()),
            Self::Array(v) => plist::Value::Array(v.iter().map(|item| item.to_plist()).collect()),
            Self::Dictionary(v) => {
                let mut dict = plist::Dictionary::new();
                for (k, v) in v.into_iter() {
                    dict.insert(k.clone(), v.to_plist());
                }
                plist::Value::Dictionary(dict)
            }
        }
    }

    pub fn to_value<T: Serialize>(value: &T) -> Self {
        match plist::to_value(value) {
            Ok(v) => Self::from(v),
            Err(_) => panic!("oof"),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, XPCError> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x42133742_u32.to_le_bytes());
        buf.extend_from_slice(&0x00000005_u32.to_le_bytes());
        self.encode_object(&mut buf)?;
        Ok(buf)
    }

    fn encode_object(&self, buf: &mut Vec<u8>) -> Result<(), XPCError> {
        match self {
            XPCObject::Bool(val) => {
                buf.extend_from_slice(&(XPCType::Bool as u32).to_le_bytes());
                buf.push(if *val { 0 } else { 1 });
                buf.extend_from_slice(&[0].repeat(3));
            }
            XPCObject::Dictionary(dict) => {
                buf.extend_from_slice(&(XPCType::Dictionary as u32).to_le_bytes());
                buf.extend_from_slice(&0_u32.to_le_bytes()); // represents l, no idea what this is.
                buf.extend_from_slice(&(dict.len() as u32).to_le_bytes());
                for (k, v) in dict {
                    let padding = Self::calculate_padding(k.len() + 1);
                    buf.extend_from_slice(k.as_bytes());
                    buf.push(0);
                    buf.extend_from_slice(&[0].repeat(padding));
                    v.encode_object(buf)?;
                }
            }
            XPCObject::Array(items) => {
                buf.extend_from_slice(&(XPCType::Array as u32).to_le_bytes());
                buf.extend_from_slice(&0_u32.to_le_bytes()); // represents l, no idea what this is.
                buf.extend_from_slice(&(items.len() as u32).to_le_bytes());
                for item in items {
                    item.encode_object(buf)?;
                }
            }

            XPCObject::Int64(num) => {
                buf.extend_from_slice(&(XPCType::Int64 as u32).to_le_bytes());
                buf.extend_from_slice(&num.to_le_bytes());
            }
            XPCObject::UInt64(num) => {
                buf.extend_from_slice(&(XPCType::UInt64 as u32).to_le_bytes());
                buf.extend_from_slice(&num.to_le_bytes());
            }
            XPCObject::String(item) => {
                let l = item.len() + 1;
                let padding = Self::calculate_padding(l);
                buf.extend_from_slice(&(XPCType::String as u32).to_le_bytes());
                buf.extend_from_slice(&(l as u32).to_le_bytes());
                buf.extend_from_slice(item.as_bytes());
                buf.push(0);
                buf.extend_from_slice(&[0].repeat(padding));
            }
            XPCObject::Data(data) => {
                let l = data.len();
                let padding = Self::calculate_padding(l);
                buf.extend_from_slice(&(XPCType::Data as u32).to_le_bytes());
                buf.extend_from_slice(&(l as u32).to_le_bytes());
                buf.extend_from_slice(data);
                buf.extend_from_slice(&[0].repeat(padding));
            }
            XPCObject::Uuid(uuid) => {
                buf.extend_from_slice(&(XPCType::Uuid as u32).to_le_bytes());
                buf.extend_from_slice(&16_u32.to_le_bytes());
                buf.extend_from_slice(uuid.as_bytes());
            }
        }
        Ok(())
    }

    pub fn decode(buf: &[u8]) -> Result<Self, XPCError> {
        let magic = u32::from_le_bytes(buf[0..4].try_into()?);
        if magic != 0x42133742 {
            Err("Invalid magic for XPCObject")?
        }

        let version = u32::from_le_bytes(buf[4..8].try_into()?);
        if version != 0x00000005 {
            Err("Unexpected version for XPCObject")?
        }

        Self::decode_object(&mut Cursor::new(&buf[8..]))
    }

    fn decode_object(mut cursor: &mut Cursor<&[u8]>) -> Result<Self, XPCError> {
        let mut buf_32: [u8; 4] = Default::default();
        cursor.read_exact(&mut buf_32)?;
        let xpc_type = u32::from_le_bytes(buf_32);
        let xpc_type: XPCType = xpc_type.try_into()?;
        match xpc_type {
            XPCType::Dictionary => {
                let mut ret = IndexMap::new();

                cursor.read_exact(&mut buf_32)?;
                let _l = u32::from_le_bytes(buf_32);
                cursor.read_exact(&mut buf_32)?;
                let num_entries = u32::from_le_bytes(buf_32);
                for _i in 0..num_entries {
                    let mut key_buf = Vec::new();
                    BufRead::read_until(&mut cursor, 0, &mut key_buf)?;
                    let key = CString::from_vec_with_nul(key_buf)?.to_str()?.to_string();
                    let padding = Self::calculate_padding(key.len() + 1);

                    BufRead::consume(&mut cursor, padding);
                    ret.insert(key, Self::decode_object(cursor)?);
                }
                Ok(XPCObject::Dictionary(ret))
            }
            XPCType::Array => {
                cursor.read_exact(&mut buf_32)?;
                let _l = u32::from_le_bytes(buf_32);
                cursor.read_exact(&mut buf_32)?;
                let num_entries = u32::from_le_bytes(buf_32);

                let mut ret = Vec::new();
                for _i in 0..num_entries {
                    ret.push(Self::decode_object(cursor)?);
                }
                Ok(XPCObject::Array(ret))
            }
            XPCType::Int64 => {
                let mut buf: [u8; 8] = Default::default();
                cursor.read_exact(&mut buf)?;
                Ok(XPCObject::Int64(i64::from_le_bytes(buf)))
            }
            XPCType::UInt64 => {
                let mut buf: [u8; 8] = Default::default();
                cursor.read_exact(&mut buf)?;
                Ok(XPCObject::UInt64(u64::from_le_bytes(buf)))
            }
            XPCType::String => {
                // 'l' includes utf8 '\0' character.
                cursor.read_exact(&mut buf_32)?;
                let l = u32::from_le_bytes(buf_32) as usize;
                let padding = Self::calculate_padding(l);

                let mut key_buf = vec![0; l];
                cursor.read_exact(&mut key_buf)?;
                let key = CString::from_vec_with_nul(key_buf)?.to_str()?.to_string();
                BufRead::consume(&mut cursor, padding);
                Ok(XPCObject::String(key))
            }
            XPCType::Bool => {
                let mut buf: [u8; 4] = Default::default();
                cursor.read_exact(&mut buf)?;
                Ok(XPCObject::Bool(buf[0] != 0))
            }
            XPCType::Data => {
                cursor.read_exact(&mut buf_32)?;
                let l = u32::from_le_bytes(buf_32) as usize;
                let padding = Self::calculate_padding(l);

                let mut data = vec![0; l];
                cursor.read_exact(&mut data)?;
                BufRead::consume(&mut cursor, padding);
                Ok(XPCObject::Data(data))
            }
            XPCType::Uuid => {
                let mut data: [u8; 16] = Default::default();
                cursor.read_exact(&mut data)?;
                Ok(XPCObject::Uuid(uuid::Builder::from_bytes(data).into_uuid()))
            }
        }
    }

    pub fn as_dictionary(&self) -> Option<&Dictionary> {
        match self {
            XPCObject::Dictionary(dict) => Some(dict),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Self>> {
        match self {
            XPCObject::Array(array) => Some(array),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            XPCObject::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<&bool> {
        match self {
            XPCObject::Bool(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_signed_integer(&self) -> Option<i64> {
        match self {
            XPCObject::String(s) => s.parse().ok(),
            XPCObject::Int64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_unsigned_integer(&self) -> Option<u64> {
        match self {
            XPCObject::String(s) => s.parse().ok(),
            XPCObject::UInt64(v) => Some(*v),
            _ => None,
        }
    }

    fn calculate_padding(len: usize) -> usize {
        let c = ((len as f64) / 4.0).ceil();
        (c * 4.0 - (len as f64)) as usize
    }
}

impl From<Dictionary> for XPCObject {
    fn from(value: Dictionary) -> Self {
        XPCObject::Dictionary(value)
    }
}

#[derive(Debug)]
pub struct XPCMessage {
    pub flags: u32,
    pub message: Option<XPCObject>,
    pub message_id: Option<u64>,
}

impl XPCMessage {
    pub fn new(
        flags: Option<XPCFlag>,
        message: Option<XPCObject>,
        message_id: Option<u64>,
    ) -> XPCMessage {
        XPCMessage {
            flags: flags.unwrap_or(XPCFlag::AlwaysSet).into(),
            message,
            message_id,
        }
    }

    pub fn decode(data: &[u8]) -> Result<XPCMessage, XPCError> {
        if data.len() < 24 {
            Err("XPCMessage must be at least 24 bytes.")?
        }

        let magic = u32::from_le_bytes(data[0..4].try_into()?);
        if magic != 0x29b00b92_u32 {
            Err("XPCMessage magic is invalid.")?
        }

        let flags = u32::from_le_bytes(data[4..8].try_into()?);
        let body_len = u64::from_le_bytes(data[8..16].try_into()?);
        let message_id = u64::from_le_bytes(data[16..24].try_into()?);
        if body_len + 24 > data.len().try_into()? {
            Err("XPCMessage body length given is incorrect.")?
        }

        // for some reason the above if check doesn't work ???
        debug!("Body length {} : {}", body_len, data.len());
        if body_len == 0 {
            return Ok(XPCMessage {
                flags,
                message: None,
                message_id: Some(message_id),
            });
        }
        Ok(XPCMessage {
            flags,
            message: Some(XPCObject::decode(&data[24..24 + body_len as usize])?),
            message_id: Some(message_id),
        })
    }

    pub fn encode(self, message_id: u64) -> Result<Vec<u8>, XPCError> {
        let mut out = 0x29b00b92_u32.to_le_bytes().to_vec();
        out.extend_from_slice(&self.flags.to_le_bytes());
        match self.message {
            Some(message) => {
                let body = message.encode()?;
                out.extend_from_slice(&(body.len() as u64).to_le_bytes()); // body length
                out.extend_from_slice(&message_id.to_le_bytes()); // messageId
                out.extend_from_slice(&body);
            }
            _ => {
                out.extend_from_slice(&0_u64.to_le_bytes());
                out.extend_from_slice(&message_id.to_le_bytes());
            }
        }
        Ok(out)
    }
}

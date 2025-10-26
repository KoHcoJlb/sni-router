use std::str::from_utf8;

use derive_more::{Display, From};
use num_enum::FromPrimitive;
use tracing::debug;
use winnow::{
    Parser, Partial,
    binary::{be_u16, be_u24, length_and_then, length_take},
    combinator::{eof, preceded, repeat_till},
    dispatch,
    error::ErrMode,
    seq,
    token::{any, rest},
};

use super::{Error, Result};
use crate::parser::{fixed_array, length_vec};

#[derive(Clone, Copy, derive_more::Debug, Display, From)]
#[debug("0x{_0:04x}")]
#[display("{_0:?}")]
pub struct Version(pub u16);

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
#[repr(u8)]
pub enum ContentType {
    Handshake = 22,
    #[num_enum(catch_all)]
    Other(u8),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
#[repr(u8)]
pub enum HandshakeType {
    ClientHello = 1,
    #[num_enum(catch_all)]
    Other(u8),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Extension {
    ServerName(String),
    Unknown { tag: u16, data: Vec<u8> },
}

fn extension(buf: &mut Partial<&[u8]>) -> Result<Extension> {
    let mut server_name = length_and_then(
        be_u16,
        preceded(
            (be_u16, 0),
            length_take(be_u16)
                .try_map(|buf| from_utf8(buf).map(|s| Extension::ServerName(s.into()))),
        ),
    )
    .context("parse server name");
    dispatch! { be_u16;
        0 => server_name,
        tag => length_vec(be_u16).map(|data| Extension::Unknown { tag, data })
    }
    .parse_next(buf)
}

fn extensions(buf: &mut Partial<&[u8]>) -> Result<Vec<Extension>> {
    let (extensions, _) =
        length_and_then(be_u16, repeat_till(.., extension, eof)).parse_next(buf)?;
    Ok(extensions)
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ClientHello {
    pub version: Version,
    pub random: [u8; 32],
    pub session_id: Vec<u8>,
    pub cipher_suites: Vec<u8>,
    pub compression_methods: Vec<u8>,
    pub extensions: Vec<Extension>,
}

fn handshake_clienthello(buf: &mut Partial<&[u8]>) -> Result<Handshake> {
    Ok(Handshake::ClientHello(
        seq!(ClientHello {
            version: be_u16.output_into().context("parse version"),
            random: fixed_array.context("parse random"),
            session_id: length_vec(any).context("parse sessionid"),
            cipher_suites: length_vec(be_u16).context("parse cipher_suites"),
            compression_methods: length_vec(any).context("parse compression_methods"),
            extensions: extensions.context("parse extensions"),
        })
        .parse_next(buf)?,
    ))
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Handshake {
    ClientHello(ClientHello),
    Unknown { handshake_type: HandshakeType, data: Vec<u8> },
}

fn content_handshake(buf: &mut Partial<&[u8]>) -> Result<RecordData> {
    dispatch! { any.output_into::<HandshakeType>();
        HandshakeType::ClientHello => length_and_then(be_u24, handshake_clienthello).context("parse ClientHello"),
        handshake_type => rest.map(|buf: &[u8]| Handshake::Unknown {
            handshake_type,
            data: buf.into(),
        })
    }
    .map(RecordData::Handshake)
    .parse_next(buf)
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RecordData {
    Handshake(Handshake),
    Unknown { content_type: ContentType, data: Vec<u8> },
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Record {
    pub version: Version,
    pub data: RecordData,
}

pub fn tls_record(buf: &mut Partial<&[u8]>) -> Result<Record> {
    fn content_parser<'a>(
        mut inner: impl Parser<Partial<&'a [u8]>, RecordData, ErrMode<Error>>,
    ) -> impl Parser<Partial<&'a [u8]>, Record, ErrMode<Error>> {
        move |buf: &mut _| {
            let (version, data) =
                (be_u16.output_into(), length_and_then(be_u16, inner.by_ref())).parse_next(buf)?;
            Ok(Record { version, data })
        }
    }

    dispatch! { any.output_into::<ContentType>();
        ContentType::Handshake => content_parser(content_handshake.context("parse handshake")),
        content_type => content_parser(rest.map(|r: &[u8]| RecordData::Unknown {
            content_type,
            data: r.into(),
        }))
    }
    .parse_next(buf)
}

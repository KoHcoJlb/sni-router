use std::fmt::Display;

use eyre::eyre;
use winnow::{
    Parser, Partial,
    binary::length_take,
    error::{AddContext, ErrMode, FromExternalError, ParserError},
    stream::{Stream, ToUsize},
    token::take,
};

pub mod tls;

pub type Result<T, E = ErrMode<Error>> = std::result::Result<T, E>;

#[derive(Debug)]
pub struct Error(pub eyre::Error);

impl Error {
    pub fn into_inner(self) -> eyre::Error {
        self.0
    }
}

impl<I: Stream> ParserError<I> for Error {
    type Inner = Self;

    fn from_input(_: &I) -> Self {
        Self(eyre!("parser error"))
    }

    fn into_inner(self) -> winnow::Result<Self::Inner, Self> {
        Ok(self)
    }
}

impl<I: Stream, E: Into<eyre::Error>> FromExternalError<I, E> for Error {
    fn from_external_error(_: &I, e: E) -> Self {
        Self(e.into())
    }
}

impl<I: Stream, C: Display + Send + Sync + 'static> AddContext<I, C> for Error {
    fn add_context(self, _input: &I, _token_start: &<I as Stream>::Checkpoint, context: C) -> Self {
        Self(self.0.wrap_err(context))
    }
}

impl From<eyre::Error> for Error {
    fn from(value: eyre::Error) -> Self {
        Self(value)
    }
}

impl From<Error> for eyre::Error {
    fn from(value: Error) -> Self {
        value.into_inner()
    }
}

fn fixed_array<const N: usize>(buf: &mut Partial<&[u8]>) -> Result<[u8; N]> {
    take(N).try_map(|buf: &[u8]| buf.try_into()).parse_next(buf)
}

fn length_vec<'a, Count: ToUsize>(
    count: impl Parser<Partial<&'a [u8]>, Count, ErrMode<Error>>,
) -> impl Parser<Partial<&'a [u8]>, Vec<u8>, ErrMode<Error>> {
    length_take(count).map(|data| data.into())
}

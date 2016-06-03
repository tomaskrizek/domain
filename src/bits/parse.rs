//! Parsing of wire-format DNS data.

use std::mem;
use super::charstr::CharStr;
use super::error::{ParseResult, ParseError};
use super::name::{DName, DNameSlice, PackedDName};
use super::nest::{Nest, NestSlice, PackedNest};
use super::octets::Octets;


//------------ Traits -------------------------------------------------------

/// A trait for parsing simple wire-format DNS data.
pub trait ParseBytes<'a>: Sized {
    /// Parses a bytes slice of a given length.
    fn parse_bytes(&mut self, len: usize) -> ParseResult<&'a [u8]>;

    /// Skip the next `len` bytes.
    fn skip(&mut self, len: usize) -> ParseResult<()>;

    /// Parses a single octet.
    fn parse_u8(&mut self) -> ParseResult<u8> {
        self.parse_bytes(1).map(|res| res[0])
    }

    /// Parses an unsigned 16-bit word.
    fn parse_u16(&mut self) -> ParseResult<u16> {
        self.parse_bytes(2).map(|res| {
            let res: &[u8; 2] = unsafe { mem::transmute(res.as_ptr()) };
            let res = unsafe { mem::transmute(*res) };
            u16::from_be(res)
        })
    }

    /// Parses an unsigned 32-bit word.
    fn parse_u32(&mut self) -> ParseResult<u32> {
        self.parse_bytes(4).map(|res| {
            let res: &[u8; 4] = unsafe { mem::transmute(res.as_ptr()) };
            let res = unsafe { mem::transmute(*res) };
            u32::from_be(res)
        })
    }

    /// Creates a sup-parser starting a the current position.
    fn sub(&self) -> Self;

    /// Creates a sub-parser limited to `len` bytes and advance position.
    fn parse_sub(&mut self, len: usize) -> ParseResult<Self>;

    /// Returns the length of the data we have seen already.
    fn seen(&self) -> usize;

    /// Returns the length of the data left.
    fn left(&self) -> usize;

    /// Parses a domain name.
    fn parse_dname(&mut self) -> ParseResult<DName<'a>>;

    /// Parses a character string.
    fn parse_charstr(&mut self) -> ParseResult<CharStr<'a>> {
        CharStr::parse(self)
    }

    /// Parses a nest.
    fn parse_nest(&mut self, len: usize) -> ParseResult<Nest<'a>>;

    /// Parses arbitrary bytes data.
    fn parse_octets(&mut self, len: usize) -> ParseResult<Octets<'a>> {
        Ok(Octets::from_bytes(try!(self.parse_bytes(len))))
    }
}

pub trait ParsePacked<'a>: ParseBytes<'a> {
    fn context(&self) -> &'a[u8];
}


/*
/// A trait for parsing wire-format DNS data.
///
/// While the basic types are implemented for every parser through the
/// `ParseBytes` trait, not every parser can parse every kind of
/// domain name. Because of this, parsers may implement the
/// `ParseBytes` trait only for specific flavors.
pub trait ParseFlavor<'a, F: FlatFlavor<'a>>: ParseBytes<'a> {
    fn parse_dname(&mut self) -> ParseResult<F::DName>;
    fn parse_cstring(&mut self) -> ParseResult<F::CString>;
    fn parse_octets(&mut self, len: usize) -> ParseResult<F::Octets>;
    fn parse_nest(&mut self, len: usize) -> ParseResult<F::FlatNest>;
}

impl<'a, P: ParseBytes<'a>> ParseFlavor<'a, flavor::Ref<'a>> for P {
    fn parse_dname(&mut self) -> ParseResult<DNameRef<'a>> {
        DNameRef::parse(self)
    }

    fn parse_cstring(&mut self) -> ParseResult<CStringRef<'a>> {
        CStringRef::parse(self)
    }

    fn parse_octets(&mut self, len: usize) -> ParseResult<OctetsRef<'a>> {
        OctetsRef::parse(self, len)
    }
 
    fn parse_nest(&mut self, len: usize) -> ParseResult<NestRef<'a>> {
        NestRef::parse(self, len)
    }
}

impl<'a> ParseFlavor<'a, flavor::Lazy<'a>> for ContextParser<'a> {
    fn parse_dname(&mut self) -> ParseResult<LazyDName<'a>> {
        LazyDName::parse(self)
    }

    fn parse_cstring(&mut self) -> ParseResult<CStringRef<'a>> {
        CStringRef::parse(self)
    }

    fn parse_octets(&mut self, len: usize) -> ParseResult<OctetsRef<'a>> {
        OctetsRef::parse(self, len)
    }

    fn parse_nest(&mut self, len: usize) -> ParseResult<LazyNest<'a>> {
        LazyNest::parse(self, len)
    }
}
*/

//------------ SliceParser --------------------------------------------------

/// A parser that operates on an arbitrary bytes slice.
#[derive(Clone, Debug)]
pub struct SliceParser<'a> {
    slice: &'a [u8],
    seen: usize,
}

impl<'a> SliceParser<'a> {
    pub fn new(slice: &'a [u8]) -> Self {
        SliceParser { slice: slice, seen: 0 }
    }

    fn check_len(&self, len: usize) -> ParseResult<()> {
        if len > self.slice.len() {
            Err(ParseError::UnexpectedEnd)
        }
        else {
            Ok(())
        }
    }
}

impl<'a> ParseBytes<'a> for SliceParser<'a> {
    fn parse_bytes(&mut self, len: usize) -> ParseResult<&'a [u8]> {
        try!(self.check_len(len));
        let (l, r) = self.slice.split_at(len);
        self.slice = r;
        self.seen += len;
        Ok(l)
    }

    fn skip(&mut self, len: usize) -> ParseResult<()> {
        try!(self.check_len(len));
        self.slice = &self.slice[len..];
        self.seen += len;
        Ok(())
    }

    fn sub(&self) -> Self {
        SliceParser { slice: self.slice, seen: 0 }
    }

    fn parse_sub(&mut self, len: usize) -> ParseResult<Self> {
        Ok(SliceParser { slice: try!(self.parse_bytes(len)), seen: 0 })
    }

    fn seen(&self) -> usize {
        self.seen
    }

    fn left(&self) -> usize {
        self.slice.len()
    }

    fn parse_dname(&mut self) -> ParseResult<DName<'a>> {
        DNameSlice::parse(self).map(|name| name.into())
    }

    fn parse_nest(&mut self, len: usize) -> ParseResult<Nest<'a>> {
        NestSlice::parse(self, len).map(|nest| nest.into())
    }
}

//------------ ContextParser ------------------------------------------------

/// A parser that operates on an entire DNS message.
#[derive(Clone, Debug)]
pub struct ContextParser<'a> {
    parser: SliceParser<'a>,
    context: &'a [u8]
}

impl<'a> ContextParser<'a> {
    pub fn new(message: &'a [u8]) -> Self {
        ContextParser {
            parser: SliceParser::new(message),
            context: message
        }
    }

    pub fn from_parts(slice: &'a[u8], context: &'a[u8]) -> Self {
        ContextParser {
            parser: SliceParser::new(slice),
            context: context
        }
    }
}

impl<'a> ParseBytes<'a> for ContextParser<'a> {
    fn parse_bytes(&mut self, len: usize) -> ParseResult<&'a [u8]> {
        self.parser.parse_bytes(len)
    }

    fn skip(&mut self, len: usize) -> ParseResult<()> {
        self.parser.skip(len)
    }

    fn sub(&self) -> Self {
        ContextParser {
            parser: self.parser.sub(),
            context: self.context
        }
    }

    fn parse_sub(&mut self, len: usize) -> ParseResult<Self> {
        Ok(ContextParser {
            parser: try!(self.parser.parse_sub(len)),
            context: self.context
        })
    }

    fn seen(&self) -> usize {
        self.parser.seen()
    }

    fn left(&self) -> usize {
        self.parser.left()
    }

    fn parse_dname(&mut self) -> ParseResult<DName<'a>> {
        PackedDName::parse(self).map(|name| name.into())
    }

    fn parse_nest(&mut self, len: usize) -> ParseResult<Nest<'a>> {
        PackedNest::parse(self, len).map(|nest| nest.into())
    }
}

impl<'a> ParsePacked<'a> for ContextParser<'a> {
    fn context(&self) -> &'a [u8] {
        self.context
    }
}

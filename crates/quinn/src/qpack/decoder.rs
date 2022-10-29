use bytes::{Buf, BufMut};
use std::{fmt, io::Cursor};

use tracing::trace;

use super::{
    dynamic::{DynamicTable, DynamicTableDecoder, Error as DynamicTableError},
    field::HeaderField,
    static_::{Error as StaticError, StaticTable},
    vas,
};

use super::{
    block::{
        HeaderBlockField, HeaderPrefix, Indexed, IndexedWithPostBase, Literal, LiteralWithNameRef,
        LiteralWithPostBaseNameRef,
    },
    parse_error::ParseError,
    stream::{
        Duplicate, DynamicTableSizeUpdate, EncoderInstruction, HeaderAck, InsertCountIncrement,
        InsertWithNameRef, InsertWithoutNameRef, StreamCancel,
    },
};

use super::{prefix_int, prefix_string};

#[derive(Debug, PartialEq)]
pub enum Error {
    InvalidInteger(prefix_int::Error),
    InvalidString(prefix_string::Error),
    InvalidIndex(vas::Error),
    DynamicTable(DynamicTableError),
    InvalidStaticIndex(usize),
    UnknownPrefix(u8),
    MissingRefs(usize),
    BadBaseIndex(isize),
    UnexpectedEnd,
    HeaderTooLong(u64),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidInteger(e) => write!(f, "invalid integer: {}", e),
            Error::InvalidString(e) => write!(f, "invalid string: {:?}", e),
            Error::InvalidIndex(e) => write!(f, "invalid dynamic index: {:?}", e),
            Error::DynamicTable(e) => write!(f, "dynamic table error: {:?}", e),
            Error::InvalidStaticIndex(i) => write!(f, "unknown static index: {}", i),
            Error::UnknownPrefix(p) => write!(f, "unknown instruction code: 0x{}", p),
            Error::MissingRefs(n) => write!(f, "missing {} refs to decode bloc", n),
            Error::BadBaseIndex(i) => write!(f, "out of bounds base index: {}", i),
            Error::UnexpectedEnd => write!(f, "unexpected end"),
            Error::HeaderTooLong(_) => write!(f, "header too long"),
        }
    }
}

pub fn ack_header<W: BufMut>(stream_id: u64, decoder: &mut W) {
    HeaderAck(stream_id).encode(decoder);
}

pub fn stream_canceled<W: BufMut>(stream_id: u64, decoder: &mut W) {
    StreamCancel(stream_id).encode(decoder);
}

#[derive(PartialEq, Debug)]
pub struct Decoded {
    /// The decoded fields
    pub fields: Vec<HeaderField>,
    /// Whether one or more encoded fields were referencing the dynamic table
    pub dyn_ref: bool,
    /// Decoded size, calculated as stated in "4.1.1.3. Header Size Constraints"
    pub mem_size: u64,
}

pub struct Decoder {
    table: DynamicTable,
}

impl Decoder {
    // Decode field lines received on Request of Push stream.
    // https://www.rfc-editor.org/rfc/rfc9204.html#name-field-line-representations
    pub fn decode_header<T: Buf>(&self, buf: &mut T) -> Result<Decoded, Error> {
        let (required_ref, base) = HeaderPrefix::decode(buf)?
            .get(self.table.total_inserted(), self.table.max_mem_size())?;

        if required_ref > self.table.total_inserted() {
            return Err(Error::MissingRefs(required_ref));
        }

        let decoder_table = self.table.decoder(base);

        let mut mem_size = 0;
        let mut fields = Vec::new();
        while buf.has_remaining() {
            let field = Self::parse_header_field(&decoder_table, buf)?;
            mem_size += field.mem_size() as u64;
            fields.push(field);
        }

        Ok(Decoded {
            fields,
            mem_size,
            dyn_ref: required_ref > 0,
        })
    }

    // The receiving side of encoder stream
    pub fn on_encoder_recv<R: Buf, W: BufMut>(
        &mut self,
        read: &mut R,
        write: &mut W,
    ) -> Result<usize, Error> {
        let inserted_on_start = self.table.total_inserted();

        while let Some(instruction) = self.parse_instruction(read)? {
            trace!("instruction {:?}", instruction);
            match instruction {
                Instruction::Insert(field) => self.table.put(field)?,
                Instruction::TableSizeUpdate(size) => {
                    self.table.set_max_size(size)?;
                }
            }
        }

        if self.table.total_inserted() != inserted_on_start {
            InsertCountIncrement(self.table.total_inserted() - inserted_on_start).encode(write);
        }

        Ok(self.table.total_inserted())
    }

    fn parse_instruction<R: Buf>(&self, read: &mut R) -> Result<Option<Instruction>, Error> {
        if read.remaining() < 1 {
            return Ok(None);
        }

        let mut buf = Cursor::new(read.chunk());
        let first = buf.chunk()[0];
        let instruction = match EncoderInstruction::decode(first) {
            EncoderInstruction::Unknown => return Err(Error::UnknownPrefix(first)),
            EncoderInstruction::DynamicTableSizeUpdate => {
                DynamicTableSizeUpdate::decode(&mut buf)?.map(|x| Instruction::TableSizeUpdate(x.0))
            }
            EncoderInstruction::InsertWithoutNameRef => InsertWithoutNameRef::decode(&mut buf)?
                .map(|x| Instruction::Insert(HeaderField::new(x.name, x.value))),
            EncoderInstruction::Duplicate => match Duplicate::decode(&mut buf)? {
                Some(Duplicate(index)) => {
                    Some(Instruction::Insert(self.table.get_relative(index)?.clone()))
                }
                None => None,
            },
            EncoderInstruction::InsertWithNameRef => match InsertWithNameRef::decode(&mut buf)? {
                Some(InsertWithNameRef::Static { index, value }) => Some(Instruction::Insert(
                    StaticTable::get(index)?.with_value(value),
                )),
                Some(InsertWithNameRef::Dynamic { index, value }) => Some(Instruction::Insert(
                    self.table.get_relative(index)?.with_value(value),
                )),
                None => None,
            },
        };

        if instruction.is_some() {
            let pos = buf.position();
            read.advance(pos as usize);
        }

        Ok(instruction)
    }

    fn parse_header_field<R: Buf>(
        table: &DynamicTableDecoder,
        buf: &mut R,
    ) -> Result<HeaderField, Error> {
        let first = buf.chunk()[0];
        let field = match HeaderBlockField::decode(first) {
            HeaderBlockField::Indexed => match Indexed::decode(buf)? {
                Indexed::Static(index) => StaticTable::get(index)?.clone(),
                Indexed::Dynamic(index) => table.get_relative(index)?.clone(),
            },
            HeaderBlockField::IndexedWithPostBase => {
                let index = IndexedWithPostBase::decode(buf)?.0;
                table.get_postbase(index)?.clone()
            }
            HeaderBlockField::LiteralWithNameRef => match LiteralWithNameRef::decode(buf)? {
                LiteralWithNameRef::Static { index, value } => {
                    StaticTable::get(index)?.with_value(value)
                }
                LiteralWithNameRef::Dynamic { index, value } => {
                    table.get_relative(index)?.with_value(value)
                }
            },
            HeaderBlockField::LiteralWithPostBaseNameRef => {
                let literal = LiteralWithPostBaseNameRef::decode(buf)?;
                table.get_postbase(literal.index)?.with_value(literal.value)
            }
            HeaderBlockField::Literal => {
                let literal = Literal::decode(buf)?;
                HeaderField::new(literal.name, literal.value)
            }
            _ => return Err(Error::UnknownPrefix(first)),
        };
        Ok(field)
    }
}

// Decode field lines received on Request or Push stream.
// https://www.rfc-editor.org/rfc/rfc9204.html#name-field-line-representations
pub fn decode_stateless<T: Buf>(buf: &mut T, max_size: u64) -> Result<Decoded, Error> {
    let (required_ref, _base) = HeaderPrefix::decode(buf)?.get(0, 0)?;

    if required_ref > 0 {
        return Err(Error::MissingRefs(required_ref));
    }

    let mut mem_size = 0;
    let mut fields = Vec::new();
    while buf.has_remaining() {
        let field = match HeaderBlockField::decode(buf.chunk()[0]) {
            HeaderBlockField::IndexedWithPostBase => return Err(Error::MissingRefs(0)),
            HeaderBlockField::LiteralWithPostBaseNameRef => return Err(Error::MissingRefs(0)),
            HeaderBlockField::Indexed => match Indexed::decode(buf)? {
                Indexed::Static(index) => StaticTable::get(index)?.clone(),
                Indexed::Dynamic(_) => return Err(Error::MissingRefs(0)),
            },
            HeaderBlockField::LiteralWithNameRef => match LiteralWithNameRef::decode(buf)? {
                LiteralWithNameRef::Dynamic { .. } => return Err(Error::MissingRefs(0)),
                LiteralWithNameRef::Static { index, value } => {
                    StaticTable::get(index)?.with_value(value)
                }
            },
            HeaderBlockField::Literal => {
                let literal = Literal::decode(buf)?;
                HeaderField::new(literal.name, literal.value)
            }
            _ => return Err(Error::UnknownPrefix(buf.chunk()[0])),
        };
        mem_size += field.mem_size() as u64;
        // Cancel decoding if the header is considered too big
        if mem_size > max_size {
            return Err(Error::HeaderTooLong(mem_size));
        }
        fields.push(field);
    }

    Ok(Decoded {
        fields,
        mem_size,
        dyn_ref: false,
    })
}

#[cfg(test)]
impl From<DynamicTable> for Decoder {
    fn from(table: DynamicTable) -> Self {
        Self { table }
    }
}

#[derive(PartialEq)]
enum Instruction {
    Insert(HeaderField),
    TableSizeUpdate(usize),
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instruction::Insert(h) => write!(f, "Instruction::Insert {{ {} }}", h),
            Instruction::TableSizeUpdate(n) => {
                write!(f, "Instruction::TableSizeUpdate {{ {} }}", n)
            }
        }
    }
}

impl From<prefix_int::Error> for Error {
    fn from(e: prefix_int::Error) -> Self {
        match e {
            prefix_int::Error::UnexpectedEnd => Error::UnexpectedEnd,
            e => Error::InvalidInteger(e),
        }
    }
}

impl From<prefix_string::Error> for Error {
    fn from(e: prefix_string::Error) -> Self {
        match e {
            prefix_string::Error::UnexpectedEnd => Error::UnexpectedEnd,
            e => Error::InvalidString(e),
        }
    }
}

impl From<vas::Error> for Error {
    fn from(e: vas::Error) -> Self {
        Error::InvalidIndex(e)
    }
}

impl From<StaticError> for Error {
    fn from(e: StaticError) -> Self {
        match e {
            StaticError::Unknown(i) => Error::InvalidStaticIndex(i),
        }
    }
}

impl From<DynamicTableError> for Error {
    fn from(e: DynamicTableError) -> Self {
        Error::DynamicTable(e)
    }
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        match e {
            ParseError::Integer(x) => Error::InvalidInteger(x),
            ParseError::String(x) => Error::InvalidString(x),
            ParseError::InvalidPrefix(p) => Error::UnknownPrefix(p),
            ParseError::InvalidBase(b) => Error::BadBaseIndex(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qpack::tests::helpers::{build_table_with_size, TABLE_SIZE};

    #[test]
    fn test_header_too_long() {
        let mut trailers = http::HeaderMap::new();
        trailers.insert("trailer", "value".parse().unwrap());
        trailers.insert("trailer2", "value2".parse().unwrap());
        let mut buf = bytes::BytesMut::new();
        let _ = crate::qpack::encode_stateless(
            &mut buf,
            crate::proto::headers::Header::trailer(trailers),
        );
        let result = decode_stateless(&mut buf, 2);
        assert_eq!(result, Err(Error::HeaderTooLong(44)));
    }

    /**
     * https://www.rfc-editor.org/rfc/rfc9204.html#name-insert-with-name-reference
     * 4.3.2.  Insert With Name Reference
     */
    #[test]
    fn test_insert_field_with_name_ref_into_dynamic_table() {
        let mut buf = vec![];
        InsertWithNameRef::new_static(1, "serial value")
            .encode(&mut buf)
            .unwrap();
        let mut decoder = Decoder::from(build_table_with_size(0));
        let mut enc = Cursor::new(&buf);
        let mut dec = vec![];
        assert!(decoder.on_encoder_recv(&mut enc, &mut dec).is_ok());

        assert_eq!(
            decoder.table.decoder(1).get_relative(0),
            Ok(&StaticTable::get(1).unwrap().with_value("serial value"))
        );

        let mut dec_cursor = Cursor::new(&dec);
        assert_eq!(
            InsertCountIncrement::decode(&mut dec_cursor),
            Ok(Some(InsertCountIncrement(1)))
        );
    }

    /**
     * https://www.rfc-editor.org/rfc/rfc9204.html#name-insert-with-name-reference
     * 4.3.2.  Insert With Name Reference
     */
    #[test]
    fn test_insert_field_with_wrong_name_index_from_static_table() {
        let mut buf = vec![];
        InsertWithNameRef::new_static(3000, "")
            .encode(&mut buf)
            .unwrap();
        let mut enc = Cursor::new(&buf);
        let mut decoder = Decoder::from(build_table_with_size(0));
        let res = decoder.on_encoder_recv(&mut enc, &mut vec![]);
        assert_eq!(res, Err(Error::InvalidStaticIndex(3000)));
    }

    /**
     * https://www.rfc-editor.org/rfc/rfc9204.html#name-insert-with-name-referencehtml
     * 4.3.2.  Insert With Name Reference
     */
    #[test]
    fn test_insert_field_with_wrong_name_index_from_dynamic_table() {
        let mut buf = vec![];
        InsertWithNameRef::new_dynamic(3000, "")
            .encode(&mut buf)
            .unwrap();
        let mut enc = Cursor::new(&buf);
        let mut dec = vec![];
        let mut decoder = Decoder::from(build_table_with_size(0));
        let res = decoder.on_encoder_recv(&mut enc, &mut dec);
        assert_eq!(
            res,
            Err(Error::DynamicTable(DynamicTableError::BadRelativeIndex(
                3000
            )))
        );

        assert!(dec.is_empty());
    }

    /**
     * https://www.rfc-editor.org/rfc/rfc9204.html#name-insert-with-literal-name
     * 4.3.3.  Insert with Literal Name
     */
    #[test]
    fn test_insert_field_without_name_ref() {
        let mut buf = vec![];
        InsertWithoutNameRef::new("key", "value")
            .encode(&mut buf)
            .unwrap();

        let mut decoder = Decoder::from(build_table_with_size(0));
        let mut enc = Cursor::new(&buf);
        let mut dec = vec![];
        assert!(decoder.on_encoder_recv(&mut enc, &mut dec).is_ok());

        assert_eq!(
            decoder.table.decoder(1).get_relative(0),
            Ok(&HeaderField::new("key", "value"))
        );

        let mut dec_cursor = Cursor::new(&dec);
        assert_eq!(
            InsertCountIncrement::decode(&mut dec_cursor),
            Ok(Some(InsertCountIncrement(1)))
        );
    }

    fn insert_fields(table: &mut DynamicTable, fields: Vec<HeaderField>) {
        for field in fields {
            table.put(field).unwrap();
        }
    }

    /**
     * https://www.rfc-editor.org/rfc/rfc9204.html#name-duplicate
     * 4.3.4.  Duplicate
     */
    #[test]
    fn test_duplicate_field() {
        // let mut table = build_table_with_size(0);
        let mut table = build_table_with_size(0);
        insert_fields(
            &mut table,
            vec![HeaderField::new("", ""), HeaderField::new("", "")],
        );
        let mut decoder = Decoder::from(table);

        let mut buf = vec![];
        Duplicate(1).encode(&mut buf);

        let mut enc = Cursor::new(&buf);
        let mut dec = vec![];
        let res = decoder.on_encoder_recv(&mut enc, &mut dec);
        assert_eq!(res, Ok(3));

        let mut dec_cursor = Cursor::new(&dec);
        assert_eq!(
            InsertCountIncrement::decode(&mut dec_cursor),
            Ok(Some(InsertCountIncrement(1)))
        );
    }

    /**
     * https://www.rfc-editor.org/rfc/rfc9204.html#name-set-dynamic-table-capacity
     * 4.3.1.  Set Dynamic Table Capacity
     */
    #[test]
    fn test_dynamic_table_size_update() {
        let mut buf = vec![];
        DynamicTableSizeUpdate(25).encode(&mut buf);

        let mut enc = Cursor::new(&buf);
        let mut dec = vec![];
        let mut decoder = Decoder::from(build_table_with_size(0));
        let res = decoder.on_encoder_recv(&mut enc, &mut dec);
        assert_eq!(res, Ok(0));

        let actual_max_size = decoder.table.max_mem_size();
        assert_eq!(actual_max_size, 25);
        assert!(dec.is_empty());
    }

    #[test]
    fn enc_recv_buf_too_short() {
        let decoder = Decoder::from(build_table_with_size(0));
        let mut buf = vec![];
        {
            let mut enc = Cursor::new(&buf);
            assert_eq!(decoder.parse_instruction(&mut enc), Ok(None));
        }

        buf.push(0b1000_0000);
        let mut enc = Cursor::new(&buf);
        assert_eq!(decoder.parse_instruction(&mut enc), Ok(None));
    }

    #[test]
    fn enc_recv_accepts_truncated_messages() {
        let mut buf = vec![];
        InsertWithoutNameRef::new("keyfoobarbaz", "value")
            .encode(&mut buf)
            .unwrap();

        let mut decoder = Decoder::from(build_table_with_size(0));
        // cut in middle of the first int
        let mut enc = Cursor::new(&buf[..2]);
        let mut dec = vec![];
        assert!(decoder.on_encoder_recv(&mut enc, &mut dec).is_ok());
        assert_eq!(enc.position(), 0);

        // cut the last byte of the 2nd string
        let mut enc = Cursor::new(&buf[..buf.len() - 1]);
        let mut dec = vec![];
        assert!(decoder.on_encoder_recv(&mut enc, &mut dec).is_ok());
        assert_eq!(enc.position(), 0);

        InsertWithoutNameRef::new("keyfoobarbaz2", "value")
            .encode(&mut buf)
            .unwrap();

        // the first valid field is inserted and buf is left at the first byte of incomplete string
        let mut enc = Cursor::new(&buf[..buf.len() - 1]);
        let mut dec = vec![];
        assert!(decoder.on_encoder_recv(&mut enc, &mut dec).is_ok());
        assert_eq!(enc.position(), 15);

        let mut dec_cursor = Cursor::new(&dec);
        assert_eq!(
            InsertCountIncrement::decode(&mut dec_cursor),
            Ok(Some(InsertCountIncrement(1)))
        );
    }

    #[test]
    fn largest_ref_too_big() {
        let decoder = Decoder::from(build_table_with_size(0));
        let mut buf = vec![];
        HeaderPrefix::new(8, 8, 10, TABLE_SIZE).encode(&mut buf);

        let mut read = Cursor::new(&buf);
        assert_eq!(decoder.decode_header(&mut read), Err(Error::MissingRefs(8)));
    }

    fn field(n: usize) -> HeaderField {
        HeaderField::new(format!("foo{}", n), "bar")
    }

    // Largest Reference
    //   Base Index = 2
    //       |
    //     foo2   foo1
    //    +-----+-----+
    //    |  2  |  1  |  Absolute Index
    //    +-----+-----+
    //    |  0  |  1  |  Relative Index
    //    --+---+-----+

    #[test]
    fn decode_indexed_header_field() {
        let mut buf = vec![];
        HeaderPrefix::new(2, 2, 2, TABLE_SIZE).encode(&mut buf);
        Indexed::Dynamic(0).encode(&mut buf);
        Indexed::Dynamic(1).encode(&mut buf);
        Indexed::Static(18).encode(&mut buf);

        let mut read = Cursor::new(&buf);
        let decoder = Decoder::from(build_table_with_size(2));
        let Decoded {
            fields, dyn_ref, ..
        } = decoder.decode_header(&mut read).unwrap();
        assert!(dyn_ref);
        assert_eq!(
            fields,
            &[field(2), field(1), StaticTable::get(18).unwrap().clone()]
        )
    }

    //      Largest Reference
    //        Base Index = 2
    //             |
    // foo4 foo3  foo2  foo1
    // +---+-----+-----+-----+
    // | 4 |  3  |  2  |  1  |  Absolute Index
    // +---+-----+-----+-----+
    //           |  0  |  1  |  Relative Index
    // +-----+-----+---+-----+
    // | 1 |  0  |              Post-Base Index
    // +---+-----+

    #[test]
    fn decode_post_base_indexed() {
        let mut buf = vec![];
        HeaderPrefix::new(4, 2, 4, TABLE_SIZE).encode(&mut buf);
        Indexed::Dynamic(0).encode(&mut buf);
        IndexedWithPostBase(0).encode(&mut buf);
        IndexedWithPostBase(1).encode(&mut buf);

        let mut read = Cursor::new(&buf);
        let decoder = Decoder::from(build_table_with_size(4));
        let Decoded {
            fields, dyn_ref, ..
        } = decoder.decode_header(&mut read).unwrap();
        assert!(dyn_ref);
        assert_eq!(fields, &[field(2), field(3), field(4)])
    }

    #[test]
    fn decode_name_ref_header_field() {
        let mut buf = vec![];
        HeaderPrefix::new(2, 2, 4, TABLE_SIZE).encode(&mut buf);
        LiteralWithNameRef::new_dynamic(1, "new bar1")
            .encode(&mut buf)
            .unwrap();
        LiteralWithNameRef::new_static(18, "PUT")
            .encode(&mut buf)
            .unwrap();

        let mut read = Cursor::new(&buf);
        let decoder = Decoder::from(build_table_with_size(4));
        let Decoded {
            fields, dyn_ref, ..
        } = decoder.decode_header(&mut read).unwrap();
        assert!(dyn_ref);
        assert_eq!(
            fields,
            &[
                field(1).with_value("new bar1"),
                StaticTable::get(18).unwrap().with_value("PUT")
            ]
        )
    }

    #[test]
    fn decode_post_base_name_ref_header_field() {
        let mut buf = vec![];
        HeaderPrefix::new(2, 2, 4, TABLE_SIZE).encode(&mut buf);
        LiteralWithPostBaseNameRef::new(0, "new bar3")
            .encode(&mut buf)
            .unwrap();

        let mut read = Cursor::new(&buf);
        let decoder = Decoder::from(build_table_with_size(4));
        let Decoded { fields, .. } = decoder.decode_header(&mut read).unwrap();
        assert_eq!(fields, &[field(3).with_value("new bar3")]);
    }

    #[test]
    fn decode_without_name_ref_header_field() {
        let mut buf = vec![];
        HeaderPrefix::new(0, 0, 0, TABLE_SIZE).encode(&mut buf);
        Literal::new("foo", "bar").encode(&mut buf).unwrap();

        let mut read = Cursor::new(&buf);
        let decoder = Decoder::from(build_table_with_size(0));
        let Decoded { fields, .. } = decoder.decode_header(&mut read).unwrap();
        assert_eq!(
            fields,
            &[HeaderField::new(b"foo".to_vec(), b"bar".to_vec())]
        );
    }

    // Largest Reference = 4
    //  |            Base Index = 0
    //  |                |
    // foo4 foo3  foo2  foo1
    // +---+-----+-----+-----+
    // | 4 |  3  |  2  |  1  |  Absolute Index
    // +---+-----+-----+-----+
    //                          Relative Index
    // +---+-----+-----+-----+
    // | 2 |   2 |  1  |  0  |  Post-Base Index
    // +---+-----+-----+-----+

    #[test]
    fn decode_single_pass_encoded() {
        let mut buf = vec![];
        HeaderPrefix::new(4, 0, 4, TABLE_SIZE).encode(&mut buf);
        IndexedWithPostBase(0).encode(&mut buf);
        IndexedWithPostBase(1).encode(&mut buf);
        IndexedWithPostBase(2).encode(&mut buf);
        IndexedWithPostBase(3).encode(&mut buf);

        let mut read = Cursor::new(&buf);
        let decoder = Decoder::from(build_table_with_size(4));
        let Decoded { fields, .. } = decoder.decode_header(&mut read).unwrap();
        assert_eq!(fields, &[field(1), field(2), field(3), field(4)]);
    }

    #[test]
    fn largest_ref_greater_than_max_entries() {
        let max_entries = TABLE_SIZE / 32;
        // some fields evicted
        let table = build_table_with_size(max_entries + 10);
        let mut buf = vec![];

        // Pre-base relative reference
        HeaderPrefix::new(
            max_entries + 5,
            max_entries + 5,
            max_entries + 10,
            TABLE_SIZE,
        )
        .encode(&mut buf);
        Indexed::Dynamic(10).encode(&mut buf);

        let mut read = Cursor::new(&buf);
        let decoder = Decoder::from(build_table_with_size(max_entries + 10));
        let Decoded { fields, .. } = decoder.decode_header(&mut read).expect("decode");
        assert_eq!(fields, &[field(max_entries - 5)]);

        let mut buf = vec![];

        // Post-base reference
        HeaderPrefix::new(
            max_entries + 10,
            max_entries + 5,
            max_entries + 10,
            TABLE_SIZE,
        )
        .encode(&mut buf);
        IndexedWithPostBase(0).encode(&mut buf);
        IndexedWithPostBase(4).encode(&mut buf);

        let mut read = Cursor::new(&buf);
        let decoder = Decoder::from(table);
        let Decoded { fields, .. } = decoder.decode_header(&mut read).unwrap();
        assert_eq!(fields, &[field(max_entries + 6), field(max_entries + 10)]);
    }
}

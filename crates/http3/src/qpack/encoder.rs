use std::{cmp, io::Cursor};

use bytes::{Buf, BufMut};

use super::{
    block::{HeaderPrefix, Indexed, IndexedWithPostBase, Literal, LiteralWithNameRef, LiteralWithPostBaseNameRef},
    dynamic::{
        DynamicInsertionResult, DynamicLookupResult, DynamicTable, DynamicTableEncoder, Error as DynamicTableError,
    },
    parse_error::ParseError,
    prefix_int::Error as IntError,
    prefix_string::Error as StringError,
    static_::StaticTable,
    stream::{
        DecoderInstruction, Duplicate, DynamicTableSizeUpdate, HeaderAck, InsertCountIncrement, InsertWithNameRef,
        InsertWithoutNameRef, StreamCancel,
    },
    HeaderField,
};

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    Insertion(DynamicTableError),
    InvalidString(StringError),
    InvalidInteger(IntError),
    UnknownDecoderInstruction(u8),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Insertion(e) => write!(f, "dynamic table insertion: {e:?}"),
            Error::InvalidString(e) => write!(f, "could not parse string: {e}"),
            Error::InvalidInteger(e) => write!(f, "could not parse integer: {e}"),
            Error::UnknownDecoderInstruction(e) => {
                write!(f, "got unkown decoder instruction: {e}")
            }
        }
    }
}

pub struct Encoder {
    table: DynamicTable,
}

impl Encoder {
    pub fn encode<W, T, H>(
        &mut self,
        stream_id: u64,
        block: &mut W,
        encoder_buf: &mut W,
        fields: T,
    ) -> Result<usize, Error>
    where
        W: BufMut,
        T: IntoIterator<Item = H>,
        H: AsRef<HeaderField>,
    {
        let mut required_ref = 0;
        let mut block_buf = Vec::new();
        let mut encoder = self.table.encoder(stream_id);

        for field in fields {
            if let Some(reference) = Self::encode_field(&mut encoder, &mut block_buf, encoder_buf, field.as_ref())? {
                required_ref = cmp::max(required_ref, reference);
            }
        }

        HeaderPrefix::new(
            required_ref,
            encoder.base(),
            encoder.total_inserted(),
            encoder.max_size(),
        )
        .encode(block);
        block.put(block_buf.as_slice());

        encoder.commit(required_ref);

        Ok(required_ref)
    }

    pub fn on_decoder_recv<R: Buf>(&mut self, read: &mut R) -> Result<(), Error> {
        while let Some(instruction) = Action::parse(read)? {
            match instruction {
                Action::Untrack(stream_id) => self.table.untrack_block(stream_id)?,
                Action::StreamCancel(stream_id) => {
                    // Untrack block twice, as this stream might have a trailer in addition to
                    // the header. Failures are ignored as blocks might have been acked before
                    // cancellation.
                    if self.table.untrack_block(stream_id).is_ok() {
                        let _ = self.table.untrack_block(stream_id);
                    }
                }
                Action::ReceivedRefIncrement(increment) => self.table.update_largest_received(increment),
            }
        }
        Ok(())
    }

    fn encode_field<W: BufMut>(
        table: &mut DynamicTableEncoder,
        block: &mut Vec<u8>,
        encoder: &mut W,
        field: &HeaderField,
    ) -> Result<Option<usize>, Error> {
        if let Some(index) = StaticTable::find(field) {
            Indexed::Static(index).encode(block);
            return Ok(None);
        }

        if let DynamicLookupResult::Relative { index, absolute } = table.find(field) {
            Indexed::Dynamic(index).encode(block);
            return Ok(Some(absolute));
        }

        let reference = match table.insert(field)? {
            DynamicInsertionResult::Duplicated {
                relative,
                postbase,
                absolute,
            } => {
                Duplicate(relative).encode(encoder);
                IndexedWithPostBase(postbase).encode(block);
                Some(absolute)
            }
            DynamicInsertionResult::Inserted { postbase, absolute } => {
                InsertWithoutNameRef::new(field.name.clone(), field.value.clone()).encode(encoder)?;
                IndexedWithPostBase(postbase).encode(block);
                Some(absolute)
            }
            DynamicInsertionResult::InsertedWithStaticNameRef {
                postbase,
                index,
                absolute,
            } => {
                InsertWithNameRef::new_static(index, field.value.clone()).encode(encoder)?;
                IndexedWithPostBase(postbase).encode(block);
                Some(absolute)
            }
            DynamicInsertionResult::InsertedWithNameRef {
                postbase,
                relative,
                absolute,
            } => {
                InsertWithNameRef::new_dynamic(relative, field.value.clone()).encode(encoder)?;
                IndexedWithPostBase(postbase).encode(block);
                Some(absolute)
            }
            DynamicInsertionResult::NotInserted(lookup_result) => match lookup_result {
                DynamicLookupResult::Static(index) => {
                    LiteralWithNameRef::new_static(index, field.value.clone()).encode(block)?;
                    None
                }
                DynamicLookupResult::Relative { index, absolute } => {
                    LiteralWithNameRef::new_dynamic(index, field.value.clone()).encode(block)?;
                    Some(absolute)
                }
                DynamicLookupResult::PostBase { index, absolute } => {
                    LiteralWithPostBaseNameRef::new(index, field.value.clone()).encode(block)?;
                    Some(absolute)
                }
                DynamicLookupResult::NotFound => {
                    Literal::new(field.name.clone(), field.value.clone()).encode(block)?;
                    None
                }
            },
        };
        Ok(reference)
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self {
            table: DynamicTable::new(),
        }
    }
}

pub fn encode_stateless<W, T, H>(block: &mut W, fields: T) -> Result<u64, Error>
where
    W: BufMut,
    T: IntoIterator<Item = H>,
    H: AsRef<HeaderField>,
{
    let mut size = 0;

    HeaderPrefix::new(0, 0, 0, 0).encode(block);
    for field in fields {
        let field = field.as_ref();

        if let Some(index) = StaticTable::find(field) {
            Indexed::Static(index).encode(block);
        } else if let Some(index) = StaticTable::find_name(&field.name) {
            LiteralWithNameRef::new_static(index, field.value.clone()).encode(block)?;
        } else {
            Literal::new(field.name.clone(), field.value.clone()).encode(block)?;
        }

        size += field.mem_size() as u64;
    }
    Ok(size)
}

#[cfg(test)]
impl From<DynamicTable> for Encoder {
    fn from(table: DynamicTable) -> Encoder {
        Encoder { table }
    }
}

// Action to apply to the encoder table, given an instruction received from the decoder.
#[derive(Debug, PartialEq)]
enum Action {
    ReceivedRefIncrement(usize),
    Untrack(u64),
    StreamCancel(u64),
}

impl Action {
    fn parse<R: Buf>(read: &mut R) -> Result<Option<Action>, Error> {
        if read.remaining() < 1 {
            return Ok(None);
        }

        let mut buf = Cursor::new(read.chunk());
        let first = buf.chunk()[0];
        let instruction = match DecoderInstruction::decode(first) {
            DecoderInstruction::Unknown => return Err(Error::UnknownDecoderInstruction(first)),
            DecoderInstruction::InsertCountIncrement => {
                InsertCountIncrement::decode(&mut buf)?.map(|x| Action::ReceivedRefIncrement(x.0))
            }
            DecoderInstruction::HeaderAck => HeaderAck::decode(&mut buf)?.map(|x| Action::Untrack(x.0)),
            DecoderInstruction::StreamCancel => StreamCancel::decode(&mut buf)?.map(|x| Action::StreamCancel(x.0)),
        };

        if instruction.is_some() {
            let pos = buf.position();
            read.advance(pos as usize);
        }

        Ok(instruction)
    }
}

pub fn set_dynamic_table_size<W: BufMut>(table: &mut DynamicTable, encoder: &mut W, size: usize) -> Result<(), Error> {
    table.set_max_size(size)?;
    DynamicTableSizeUpdate(size).encode(encoder);
    Ok(())
}

impl From<DynamicTableError> for Error {
    fn from(e: DynamicTableError) -> Self {
        Error::Insertion(e)
    }
}

impl From<StringError> for Error {
    fn from(e: StringError) -> Self {
        Error::InvalidString(e)
    }
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        match e {
            ParseError::Integer(x) => Error::InvalidInteger(x),
            ParseError::String(x) => Error::InvalidString(x),
            ParseError::InvalidPrefix(x) => Error::UnknownDecoderInstruction(x),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::qpack::tests::helpers::{build_table, TABLE_SIZE};

    fn check_encode_field(
        init_fields: &[HeaderField],
        field: &[HeaderField],
        check: &dyn Fn(&mut Cursor<&mut Vec<u8>>, &mut Cursor<&mut Vec<u8>>),
    ) {
        let mut table = build_table();
        table.set_max_size(TABLE_SIZE).unwrap();
        check_encode_field_table(&mut table, init_fields, field, 1, check);
    }

    fn check_encode_field_table(
        table: &mut DynamicTable,
        init_fields: &[HeaderField],
        field: &[HeaderField],
        stream_id: u64,
        check: &dyn Fn(&mut Cursor<&mut Vec<u8>>, &mut Cursor<&mut Vec<u8>>),
    ) {
        for field in init_fields {
            table.put(field.clone()).unwrap();
        }

        let mut encoder = Vec::new();
        let mut block = Vec::new();
        let mut enc_table = table.encoder(stream_id);

        for field in field {
            Encoder::encode_field(&mut enc_table, &mut block, &mut encoder, field).unwrap();
        }

        enc_table.commit(field.len());

        let mut read_block = Cursor::new(&mut block);
        let mut read_encoder = Cursor::new(&mut encoder);
        check(&mut read_block, &mut read_encoder);
    }

    #[test]
    fn encode_static() {
        let field = HeaderField::new(":method", "GET");
        check_encode_field(&[], &[field], &|mut b, e| {
            assert_eq!(Indexed::decode(&mut b), Ok(Indexed::Static(17)));
            assert_eq!(e.get_ref().len(), 0);
        });
    }

    #[test]
    fn encode_static_nameref() {
        let field = HeaderField::new("location", "/bar");
        check_encode_field(&[], &[field], &|mut b, mut e| {
            assert_eq!(IndexedWithPostBase::decode(&mut b), Ok(IndexedWithPostBase(0)));
            assert_eq!(
                InsertWithNameRef::decode(&mut e),
                Ok(Some(InsertWithNameRef::new_static(12, "/bar")))
            );
        });
    }

    #[test]
    fn encode_static_nameref_indexed_in_dynamic() {
        let field = HeaderField::new("location", "/bar");
        check_encode_field(&[field.clone()], &[field], &|mut b, e| {
            assert_eq!(Indexed::decode(&mut b), Ok(Indexed::Dynamic(0)));
            assert_eq!(e.get_ref().len(), 0);
        });
    }

    #[test]
    fn encode_dynamic_insert() {
        let field = HeaderField::new("foo", "bar");
        check_encode_field(&[], &[field], &|mut b, mut e| {
            assert_eq!(IndexedWithPostBase::decode(&mut b), Ok(IndexedWithPostBase(0)));
            assert_eq!(
                InsertWithoutNameRef::decode(&mut e),
                Ok(Some(InsertWithoutNameRef::new("foo", "bar")))
            );
        });
    }

    #[test]
    fn encode_dynamic_insert_nameref() {
        let field = HeaderField::new("foo", "bar");
        check_encode_field(
            &[field.clone(), HeaderField::new("baz", "bar")],
            &[field.with_value("quxx")],
            &|mut b, mut e| {
                assert_eq!(IndexedWithPostBase::decode(&mut b), Ok(IndexedWithPostBase(0)));
                assert_eq!(
                    InsertWithNameRef::decode(&mut e),
                    Ok(Some(InsertWithNameRef::new_dynamic(1, "quxx")))
                );
            },
        );
    }

    #[test]
    fn encode_literal() {
        let mut table = build_table();
        table.set_max_size(0).unwrap();
        let field = HeaderField::new("foo", "bar");
        check_encode_field_table(&mut table, &[], &[field], 1, &|mut b, e| {
            assert_eq!(Literal::decode(&mut b), Ok(Literal::new("foo", "bar")));
            assert_eq!(e.get_ref().len(), 0);
        });
    }

    #[test]
    fn encode_literal_nameref() {
        let mut table = build_table();
        table.set_max_size(63).unwrap();
        let field = HeaderField::new("foo", "bar");

        check_encode_field_table(&mut table, &[], &[field.clone()], 1, &|mut b, _| {
            assert_eq!(IndexedWithPostBase::decode(&mut b), Ok(IndexedWithPostBase(0)));
        });
        check_encode_field_table(
            &mut table,
            &[field.clone()],
            &[field.with_value("quxx")],
            2,
            &|mut b, e| {
                assert_eq!(
                    LiteralWithNameRef::decode(&mut b),
                    Ok(LiteralWithNameRef::new_dynamic(0, "quxx"))
                );
                assert_eq!(e.get_ref().len(), 0);
            },
        );
    }

    #[test]
    fn encode_literal_postbase_nameref() {
        let mut table = build_table();
        table.set_max_size(63).unwrap();
        let field = HeaderField::new("foo", "bar");
        check_encode_field_table(
            &mut table,
            &[],
            &[field.clone(), field.with_value("quxx")],
            1,
            &|mut b, mut e| {
                assert_eq!(IndexedWithPostBase::decode(&mut b), Ok(IndexedWithPostBase(0)));
                assert_eq!(
                    LiteralWithPostBaseNameRef::decode(&mut b),
                    Ok(LiteralWithPostBaseNameRef::new(0, "quxx"))
                );
                assert_eq!(
                    InsertWithoutNameRef::decode(&mut e),
                    Ok(Some(InsertWithoutNameRef::new("foo", "bar")))
                );
            },
        );
    }

    #[test]
    fn encode_with_header_block() {
        let mut table = build_table();

        for idx in 1..5 {
            table
                .put(HeaderField::new(format!("foo{}", idx), format!("bar{}", idx)))
                .unwrap();
        }

        let mut encoder_buf = Vec::new();
        let mut block = Vec::new();
        let mut encoder = Encoder::from(table);

        let fields = vec![
            HeaderField::new(":method", "GET"),
            HeaderField::new("foo1", "bar1"),
            HeaderField::new("foo3", "new bar3"),
            HeaderField::new(":method", "staticnameref"),
            HeaderField::new("newfoo", "newbar"),
        ]
        .into_iter();

        assert_eq!(encoder.encode(1, &mut block, &mut encoder_buf, fields), Ok(7));

        let mut read_block = Cursor::new(&mut block);
        let mut read_encoder = Cursor::new(&mut encoder_buf);

        assert_eq!(
            InsertWithNameRef::decode(&mut read_encoder),
            Ok(Some(InsertWithNameRef::new_dynamic(1, "new bar3")))
        );
        assert_eq!(
            InsertWithNameRef::decode(&mut read_encoder),
            Ok(Some(InsertWithNameRef::new_static(
                StaticTable::find_name(&b":method"[..]).unwrap(),
                "staticnameref"
            )))
        );
        assert_eq!(
            InsertWithoutNameRef::decode(&mut read_encoder),
            Ok(Some(InsertWithoutNameRef::new("newfoo", "newbar")))
        );

        assert_eq!(
            HeaderPrefix::decode(&mut read_block).unwrap().get(7, TABLE_SIZE),
            Ok((7, 4))
        );
        assert_eq!(Indexed::decode(&mut read_block), Ok(Indexed::Static(17)));
        assert_eq!(Indexed::decode(&mut read_block), Ok(Indexed::Dynamic(3)));
        assert_eq!(IndexedWithPostBase::decode(&mut read_block), Ok(IndexedWithPostBase(0)));
        assert_eq!(IndexedWithPostBase::decode(&mut read_block), Ok(IndexedWithPostBase(1)));
        assert_eq!(IndexedWithPostBase::decode(&mut read_block), Ok(IndexedWithPostBase(2)));
        assert_eq!(read_block.get_ref().len() as u64, read_block.position());
    }

    #[test]
    fn decoder_block_ack() {
        let mut table = build_table();

        let field = HeaderField::new("foo", "bar");
        check_encode_field_table(
            &mut table,
            &[],
            &[field.clone(), field.with_value("quxx")],
            2,
            &|_, _| {},
        );

        let mut buf = vec![];
        let mut encoder = Encoder::from(table);

        HeaderAck(2).encode(&mut buf);
        let mut cur = Cursor::new(&buf);
        assert_eq!(Action::parse(&mut cur), Ok(Some(Action::Untrack(2))));

        let mut cur = Cursor::new(&buf);
        assert_eq!(encoder.on_decoder_recv(&mut cur), Ok(()),);

        let mut cur = Cursor::new(&buf);
        assert_eq!(
            encoder.on_decoder_recv(&mut cur),
            Err(Error::Insertion(DynamicTableError::UnknownStreamId(2)))
        );
    }

    #[test]
    fn decoder_stream_cacnceled() {
        let mut table = build_table();

        let field = HeaderField::new("foo", "bar");
        check_encode_field_table(
            &mut table,
            &[],
            &[field.clone(), field.with_value("quxx")],
            2,
            &|_, _| {},
        );

        let mut buf = vec![];

        StreamCancel(2).encode(&mut buf);
        let mut cur = Cursor::new(&buf);
        assert_eq!(Action::parse(&mut cur), Ok(Some(Action::StreamCancel(2))));
    }

    #[test]
    fn decoder_accept_truncated() {
        let mut buf = vec![];
        StreamCancel(2321).encode(&mut buf);

        let mut cur = Cursor::new(&buf[..2]); // trucated prefix_int
        assert_eq!(Action::parse(&mut cur), Ok(None));

        let mut cur = Cursor::new(&buf);
        assert_eq!(Action::parse(&mut cur), Ok(Some(Action::StreamCancel(2321))));
    }

    #[test]
    fn decoder_unknown_stream() {
        let mut table = build_table();

        check_encode_field_table(&mut table, &[], &[HeaderField::new("foo", "bar")], 2, &|_, _| {});
        let mut encoder = Encoder::from(table);

        let mut buf = vec![];
        HeaderAck(4).encode(&mut buf);

        let mut cur = Cursor::new(&buf);
        assert_eq!(
            encoder.on_decoder_recv(&mut cur),
            Err(Error::Insertion(DynamicTableError::UnknownStreamId(4)))
        );
    }

    #[test]
    fn insert_count() {
        let mut buf = vec![];
        InsertCountIncrement(4).encode(&mut buf);

        let mut cur = Cursor::new(&buf);
        assert_eq!(Action::parse(&mut cur), Ok(Some(Action::ReceivedRefIncrement(4))));

        let mut encoder = Encoder { table: build_table() };

        let mut cur = Cursor::new(&buf);
        assert_eq!(encoder.on_decoder_recv(&mut cur), Ok(()));
    }
}

use crate::qpack::{dynamic::DynamicTable, Decoded, Decoder, DecoderError, Encoder, HeaderField};
use std::io::Cursor;

pub mod helpers {
    use crate::qpack::{dynamic::DynamicTable, HeaderField};

    pub const TABLE_SIZE: usize = 4096;

    pub fn build_table() -> DynamicTable {
        let mut table = DynamicTable::new();
        table.set_max_size(TABLE_SIZE).unwrap();
        table.set_max_blocked(100).unwrap();
        table
    }

    pub fn build_table_with_size(n_field: usize) -> DynamicTable {
        let mut table = DynamicTable::new();
        table.set_max_size(TABLE_SIZE).unwrap();
        table.set_max_blocked(100).unwrap();

        for i in 0..n_field {
            table
                .put(HeaderField::new(format!("foo{}", i + 1), "bar"))
                .unwrap();
        }

        table
    }
}

#[test]
fn codec_basic_get() {
    let mut encoder = Encoder::default();
    let mut decoder = Decoder::from(DynamicTable::new());

    let mut block_buf = vec![];
    let mut enc_buf = vec![];
    let mut dec_buf = vec![];

    let header = vec![
        HeaderField::new(":method", "GET"),
        HeaderField::new(":path", "/"),
        HeaderField::new("foo", "bar"),
    ];

    encoder
        .encode(42, &mut block_buf, &mut enc_buf, header.clone().into_iter())
        .unwrap();

    let mut enc_cur = Cursor::new(&mut enc_buf);
    decoder.on_encoder_recv(&mut enc_cur, &mut dec_buf).unwrap();

    let mut block_cur = Cursor::new(&mut block_buf);
    let Decoded { fields, .. } = decoder.decode_header(&mut block_cur).unwrap();
    assert_eq!(fields, header);

    let mut dec_cur = Cursor::new(&mut dec_buf);
    encoder.on_decoder_recv(&mut dec_cur).unwrap();
}

const TABLE_SIZE: usize = 4096;
#[test]
fn blocked_header() {
    let mut enc_table = DynamicTable::new();
    enc_table.set_max_size(TABLE_SIZE).unwrap();
    enc_table.set_max_blocked(100).unwrap();
    let mut encoder = Encoder::from(enc_table);
    let mut dec_table = DynamicTable::new();
    dec_table.set_max_size(TABLE_SIZE).unwrap();
    dec_table.set_max_blocked(100).unwrap();
    let decoder = Decoder::from(dec_table);

    let mut block_buf = vec![];
    let mut enc_buf = vec![];

    encoder
        .encode(
            42,
            &mut block_buf,
            &mut enc_buf,
            &[HeaderField::new("foo", "bar")],
        )
        .unwrap();

    let mut block_cur = Cursor::new(&mut block_buf);
    assert_eq!(
        decoder.decode_header(&mut block_cur),
        Err(DecoderError::MissingRefs(1))
    );
}

#[test]
fn codec_table_size_0() {
    let mut enc_table = DynamicTable::new();
    let mut dec_table = DynamicTable::new();

    let mut block_buf = vec![];
    let mut enc_buf = vec![];
    let mut dec_buf = vec![];

    let header = vec![
        HeaderField::new(":method", "GET"),
        HeaderField::new(":path", "/"),
        HeaderField::new("foo", "bar"),
    ];

    dec_table.set_max_size(0).unwrap();
    enc_table.set_max_size(0).unwrap();

    let mut encoder = Encoder::from(enc_table);
    let mut decoder = Decoder::from(dec_table);

    encoder
        .encode(42, &mut block_buf, &mut enc_buf, header.clone().into_iter())
        .unwrap();

    let mut enc_cur = Cursor::new(&mut enc_buf);
    decoder.on_encoder_recv(&mut enc_cur, &mut dec_buf).unwrap();

    let mut block_cur = Cursor::new(&mut block_buf);
    let Decoded { fields, .. } = decoder.decode_header(&mut block_cur).unwrap();
    assert_eq!(fields, header);

    let mut dec_cur = Cursor::new(&mut dec_buf);
    encoder.on_decoder_recv(&mut dec_cur).unwrap();
}

#[test]
fn codec_table_full() {
    let mut enc_table = DynamicTable::new();
    let mut dec_table = DynamicTable::new();

    let mut block_buf = vec![];
    let mut enc_buf = vec![];
    let mut dec_buf = vec![];

    let header = vec![
        HeaderField::new("foo", "bar"),
        HeaderField::new("foo1", "bar1"),
    ];

    dec_table.set_max_size(42).unwrap();
    enc_table.set_max_size(42).unwrap();

    let mut encoder = Encoder::from(enc_table);
    let mut decoder = Decoder::from(dec_table);

    encoder
        .encode(42, &mut block_buf, &mut enc_buf, header.clone().into_iter())
        .unwrap();

    let mut enc_cur = Cursor::new(&mut enc_buf);
    let mut block_cur = Cursor::new(&mut block_buf);

    decoder.on_encoder_recv(&mut enc_cur, &mut dec_buf).unwrap();
    let Decoded { fields, .. } = decoder.decode_header(&mut block_cur).unwrap();
    assert_eq!(fields, header);

    let mut dec_cur = Cursor::new(&mut dec_buf);
    encoder.on_decoder_recv(&mut dec_cur).unwrap();
}

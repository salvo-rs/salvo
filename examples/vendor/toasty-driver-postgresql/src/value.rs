use fallible_iterator::FallibleIterator;
use postgres_protocol::types::{ArrayDimension, array_from_sql, array_to_sql};
use toasty_core::stmt::{self, Value as CoreValue};
use tokio_postgres::{
    Column, Row,
    types::{FromSql, IsNull, Kind, ToSql, Type, private::BytesMut, to_sql_checked},
};

/// Wrapper for reading string values from PostgreSQL enum columns.
///
/// The standard `String::FromSql::accepts()` rejects custom enum types.
/// This wrapper accepts `Kind::Enum` types and reads the value as UTF-8 text.
struct EnumString(String);

impl<'a> postgres_types::FromSql<'a> for EnumString {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(EnumString(
            std::str::from_utf8(raw)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Sync + Send>)?
                .to_string(),
        ))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(ty.kind(), Kind::Enum(_))
    }
}

/// Captures the raw wire bytes of a column without interpreting them.
/// Used by the array decode path to hand the column body to
/// [`array_from_sql`] directly, bypassing tokio-postgres's
/// `Vec<Option<T>>` element decoder.
struct RawBytes<'a>(&'a [u8]);

impl<'a> postgres_types::FromSql<'a> for RawBytes<'a> {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(RawBytes(raw))
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }
}

#[derive(Debug)]
pub(crate) struct Value(CoreValue);

impl From<CoreValue> for Value {
    fn from(value: CoreValue) -> Self {
        Self(value)
    }
}

impl Value {
    /// Converts this PostgreSQL driver value into the core Toasty value.
    pub(crate) fn into_inner(self) -> CoreValue {
        self.0
    }

    /// Converts a PostgreSQL value within a row to a Toasty value.
    pub(crate) fn from_sql(
        index: usize,
        row: &Row,
        column: &Column,
        expected_ty: &stmt::Type,
    ) -> Self {
        // Gets the value from the row as Option<T> and return stmt::Value::Null if the Option is
        // None.
        macro_rules! get_or_return_null {
            ($ty:ty) => {{
                match row.get::<usize, Option<$ty>>(index) {
                    Some(inner) => inner,
                    None => return Self(stmt::Value::Null),
                }
            }};
        }

        // NOTE: unfortunately, the inner representation of the PostgreSQL type enum is not
        // accessible, so we must manually match each type like so.
        let core_value = if column.type_() == &Type::TEXT || column.type_() == &Type::VARCHAR {
            text_to_value(get_or_return_null!(String), expected_ty)
        } else if column.type_() == &Type::BOOL {
            stmt::Value::Bool(get_or_return_null!(bool))
        } else if column.type_() == &Type::INT2 {
            int2_to_value(get_or_return_null!(i16), expected_ty)
        } else if column.type_() == &Type::INT4 {
            int4_to_value(get_or_return_null!(i32), expected_ty)
        } else if column.type_() == &Type::INT8 {
            int8_to_value(get_or_return_null!(i64), expected_ty)
        } else if column.type_() == &Type::UUID {
            let v = get_or_return_null!(uuid::Uuid);
            match expected_ty {
                stmt::Type::Uuid => stmt::Value::Uuid(v),
                stmt::Type::String => stmt::Value::String(v.to_string()),
                _ => stmt::Value::Uuid(v),
            }
        } else if column.type_() == &Type::BYTEA {
            let v = get_or_return_null!(Vec<u8>);
            match expected_ty {
                stmt::Type::Uuid => stmt::Value::Uuid(v.try_into().expect("invalid uuid bytes")),
                stmt::Type::Bytes => stmt::Value::Bytes(v),
                _ => todo!(
                    "unsupported conversion from {:#?} to {expected_ty:?}",
                    column.type_()
                ),
            }
        } else if column.type_() == &Type::TIMESTAMPTZ {
            #[cfg(feature = "jiff")]
            {
                stmt::Value::Timestamp(get_or_return_null!(jiff::Timestamp))
            }
            #[cfg(not(feature = "jiff"))]
            {
                panic!("TIMESTAMPTZ requires jiff feature to be enabled")
            }
        } else if column.type_() == &Type::TIMESTAMP {
            #[cfg(feature = "jiff")]
            {
                stmt::Value::DateTime(get_or_return_null!(jiff::civil::DateTime))
            }
            #[cfg(not(feature = "jiff"))]
            {
                panic!("TIMESTAMP requires jiff feature to be enabled")
            }
        } else if column.type_() == &Type::DATE {
            #[cfg(feature = "jiff")]
            {
                stmt::Value::Date(get_or_return_null!(jiff::civil::Date))
            }
            #[cfg(not(feature = "jiff"))]
            {
                panic!("DATE requires jiff feature to be enabled")
            }
        } else if column.type_() == &Type::TIME {
            #[cfg(feature = "jiff")]
            {
                stmt::Value::Time(get_or_return_null!(jiff::civil::Time))
            }
            #[cfg(not(feature = "jiff"))]
            {
                panic!("TIME requires jiff feature to be enabled")
            }
        } else if column.type_() == &Type::FLOAT4 {
            float4_to_value(get_or_return_null!(f32), expected_ty)
        } else if column.type_() == &Type::FLOAT8 {
            float8_to_value(get_or_return_null!(f64), expected_ty)
        } else if column.type_() == &Type::NUMERIC {
            #[cfg(feature = "rust_decimal")]
            {
                stmt::Value::Decimal(get_or_return_null!(rust_decimal::Decimal))
            }
            #[cfg(not(feature = "rust_decimal"))]
            {
                panic!("NUMERIC requires rust_decimal feature to be enabled")
            }
        } else if matches!(column.type_().kind(), Kind::Enum(_)) {
            // Native database enum types (CREATE TYPE ... AS ENUM) are read as strings.
            // We use EnumString instead of String because String::FromSql::accepts()
            // rejects custom enum types.
            match row.get::<usize, Option<EnumString>>(index) {
                Some(EnumString(v)) => stmt::Value::String(v),
                None => return Self(stmt::Value::Null),
            }
        } else if let Kind::Array(_) = column.type_().kind() {
            // Native array column (e.g. `text[]`, `int8[]`) — decoded
            // directly into `Vec<stmt::Value>` via `array_from_sql`. The
            // element type is taken from `expected_ty.as_list_unwrap()` so
            // the per-element conversion mirrors a column of that scalar
            // type.
            let elem_ty = match expected_ty {
                stmt::Type::List(elem) => elem.as_ref(),
                other => panic!("array column expected stmt::Type::List, got {other:?}"),
            };
            let items = read_array_items(index, row, column, elem_ty);
            match items {
                Some(items) => stmt::Value::List(items),
                None => return Self(stmt::Value::Null),
            }
        } else {
            todo!(
                "implement PostgreSQL to toasty conversion for `{:#?}`",
                column.type_()
            );
        };

        Value(core_value)
    }

    /// Converts a PostgreSQL value within a row using PostgreSQL column
    /// metadata as the Toasty value type.
    pub(crate) fn from_sql_infer(index: usize, row: &Row, column: &Column) -> Self {
        macro_rules! get_or_return_null {
            ($ty:ty) => {{
                match row.get::<usize, Option<$ty>>(index) {
                    Some(inner) => inner,
                    None => return Self(stmt::Value::Null),
                }
            }};
        }

        let core_value = if column.type_() == &Type::TEXT || column.type_() == &Type::VARCHAR {
            stmt::Value::String(get_or_return_null!(String))
        } else if column.type_() == &Type::BOOL {
            stmt::Value::Bool(get_or_return_null!(bool))
        } else if column.type_() == &Type::INT2 {
            stmt::Value::I16(get_or_return_null!(i16))
        } else if column.type_() == &Type::INT4 {
            stmt::Value::I32(get_or_return_null!(i32))
        } else if column.type_() == &Type::INT8 {
            stmt::Value::I64(get_or_return_null!(i64))
        } else if column.type_() == &Type::UUID {
            stmt::Value::Uuid(get_or_return_null!(uuid::Uuid))
        } else if column.type_() == &Type::BYTEA {
            stmt::Value::Bytes(get_or_return_null!(Vec<u8>))
        } else if column.type_() == &Type::TIMESTAMPTZ {
            #[cfg(feature = "jiff")]
            {
                stmt::Value::Timestamp(get_or_return_null!(jiff::Timestamp))
            }
            #[cfg(not(feature = "jiff"))]
            {
                panic!("TIMESTAMPTZ requires jiff feature to be enabled")
            }
        } else if column.type_() == &Type::TIMESTAMP {
            #[cfg(feature = "jiff")]
            {
                stmt::Value::DateTime(get_or_return_null!(jiff::civil::DateTime))
            }
            #[cfg(not(feature = "jiff"))]
            {
                panic!("TIMESTAMP requires jiff feature to be enabled")
            }
        } else if column.type_() == &Type::DATE {
            #[cfg(feature = "jiff")]
            {
                stmt::Value::Date(get_or_return_null!(jiff::civil::Date))
            }
            #[cfg(not(feature = "jiff"))]
            {
                panic!("DATE requires jiff feature to be enabled")
            }
        } else if column.type_() == &Type::TIME {
            #[cfg(feature = "jiff")]
            {
                stmt::Value::Time(get_or_return_null!(jiff::civil::Time))
            }
            #[cfg(not(feature = "jiff"))]
            {
                panic!("TIME requires jiff feature to be enabled")
            }
        } else if column.type_() == &Type::FLOAT4 {
            stmt::Value::F32(get_or_return_null!(f32))
        } else if column.type_() == &Type::FLOAT8 {
            stmt::Value::F64(get_or_return_null!(f64))
        } else if column.type_() == &Type::NUMERIC {
            #[cfg(feature = "rust_decimal")]
            {
                stmt::Value::Decimal(get_or_return_null!(rust_decimal::Decimal))
            }
            #[cfg(not(feature = "rust_decimal"))]
            {
                panic!("NUMERIC requires rust_decimal feature to be enabled")
            }
        } else if matches!(column.type_().kind(), Kind::Enum(_)) {
            match row.get::<usize, Option<EnumString>>(index) {
                Some(EnumString(v)) => stmt::Value::String(v),
                None => return Self(stmt::Value::Null),
            }
        } else {
            todo!(
                "implement PostgreSQL raw SQL inference for `{:#?}`",
                column.type_()
            );
        };

        Value(core_value)
    }
}

// ============================================================================
// Per-primitive conversions
// ----------------------------------------------------------------------------
// These functions translate a single decoded PostgreSQL primitive (the value
// you get from `Row::get` or from an array element) into a `stmt::Value`,
// respecting Toasty's expected element type. Sharing them between the column
// path ([`Value::from_sql`]) and the array path ([`read_array_items`]) keeps
// the two reading paths consistent: a `text[]` element and a `text` column
// decode through the same logic.
// ============================================================================

fn text_to_value(v: String, expected_ty: &stmt::Type) -> stmt::Value {
    match expected_ty {
        stmt::Type::String => stmt::Value::String(v),
        stmt::Type::Uuid => stmt::Value::Uuid(
            v.parse()
                .unwrap_or_else(|_| panic!("uuid could not be parsed from text")),
        ),
        _ => stmt::Value::String(v),
    }
}

fn int2_to_value(v: i16, expected_ty: &stmt::Type) -> stmt::Value {
    match expected_ty {
        stmt::Type::I8 => stmt::Value::I8(v as i8),
        stmt::Type::I16 => stmt::Value::I16(v),
        stmt::Type::U8 => stmt::Value::U8(
            u8::try_from(v).unwrap_or_else(|_| panic!("u8 value out of range: {v}")),
        ),
        stmt::Type::U16 => stmt::Value::U16(v as u16),
        _ => panic!("unexpected type for INT2: {expected_ty:#?}"),
    }
}

fn int4_to_value(v: i32, expected_ty: &stmt::Type) -> stmt::Value {
    match expected_ty {
        stmt::Type::I32 => stmt::Value::I32(v),
        stmt::Type::U16 => stmt::Value::U16(
            u16::try_from(v).unwrap_or_else(|_| panic!("u16 value out of range: {v}")),
        ),
        stmt::Type::U32 => stmt::Value::U32(v as u32),
        _ => stmt::Value::I32(v),
    }
}

fn int8_to_value(v: i64, expected_ty: &stmt::Type) -> stmt::Value {
    match expected_ty {
        stmt::Type::I64 => stmt::Value::I64(v),
        stmt::Type::U32 => stmt::Value::U32(
            u32::try_from(v).unwrap_or_else(|_| panic!("u32 value out of range: {v}")),
        ),
        stmt::Type::U64 => stmt::Value::U64(
            u64::try_from(v).unwrap_or_else(|_| panic!("u64 value out of range: {v}")),
        ),
        _ => stmt::Value::I64(v),
    }
}

fn float4_to_value(v: f32, expected_ty: &stmt::Type) -> stmt::Value {
    match expected_ty {
        stmt::Type::F32 => stmt::Value::F32(v),
        stmt::Type::F64 => stmt::Value::F64(v as f64),
        _ => panic!("unexpected type for FLOAT4: {expected_ty:#?}"),
    }
}

fn float8_to_value(v: f64, expected_ty: &stmt::Type) -> stmt::Value {
    match expected_ty {
        stmt::Type::F32 => stmt::Value::F32(v as f32),
        stmt::Type::F64 => stmt::Value::F64(v),
        _ => panic!("unexpected type for FLOAT8: {expected_ty:#?}"),
    }
}

/// Decode a PostgreSQL array column into a list of Toasty values. Returns
/// `None` for SQL NULL. The column body is parsed with [`array_from_sql`]
/// and each element is decoded straight into a `stmt::Value` via the
/// per-primitive helpers above — the same ones used by [`Value::from_sql`]
/// for scalar columns. No intermediate `Vec<Option<T>>` is allocated.
fn read_array_items(
    index: usize,
    row: &Row,
    column: &Column,
    elem_ty: &stmt::Type,
) -> Option<Vec<stmt::Value>> {
    let elem_pg_ty = match column.type_().kind() {
        Kind::Array(elem) => elem,
        _ => panic!(
            "read_array_items called on non-array column: {:?}",
            column.type_()
        ),
    };

    let RawBytes(raw) = row.get::<usize, Option<RawBytes<'_>>>(index)?;
    let array = array_from_sql(raw).expect("invalid PostgreSQL array wire format");

    let ndims = array
        .dimensions()
        .count()
        .expect("invalid PostgreSQL array dimensions header");
    if ndims > 1 {
        panic!(
            "multi-dimensional PostgreSQL arrays are not supported \
             (got {ndims} dimensions). See https://github.com/tokio-rs/toasty/issues/870"
        );
    }

    let mut values = array.values();
    let (cap, _) = values.size_hint();
    let mut out = Vec::with_capacity(cap);
    while let Some(elem) = values
        .next()
        .expect("invalid PostgreSQL array element framing")
    {
        out.push(match elem {
            None => stmt::Value::Null,
            Some(bytes) => decode_array_element(elem_pg_ty, bytes, elem_ty),
        });
    }
    Some(out)
}

/// Decode a single PostgreSQL array element from its raw wire bytes and
/// project it into the Toasty value space using `elem_ty`. Reuses the
/// scalar-column `_to_value` helpers so a `text[]` element and a `text`
/// column decode through identical logic.
fn decode_array_element(elem_pg_ty: &Type, bytes: &[u8], elem_ty: &stmt::Type) -> stmt::Value {
    if elem_pg_ty == &Type::TEXT || elem_pg_ty == &Type::VARCHAR {
        text_to_value(
            String::from_sql(elem_pg_ty, bytes).expect("decode TEXT array element"),
            elem_ty,
        )
    } else if elem_pg_ty == &Type::BOOL {
        stmt::Value::Bool(bool::from_sql(elem_pg_ty, bytes).expect("decode BOOL array element"))
    } else if elem_pg_ty == &Type::INT2 {
        int2_to_value(
            i16::from_sql(elem_pg_ty, bytes).expect("decode INT2 array element"),
            elem_ty,
        )
    } else if elem_pg_ty == &Type::INT4 {
        int4_to_value(
            i32::from_sql(elem_pg_ty, bytes).expect("decode INT4 array element"),
            elem_ty,
        )
    } else if elem_pg_ty == &Type::INT8 {
        int8_to_value(
            i64::from_sql(elem_pg_ty, bytes).expect("decode INT8 array element"),
            elem_ty,
        )
    } else if elem_pg_ty == &Type::FLOAT4 {
        float4_to_value(
            f32::from_sql(elem_pg_ty, bytes).expect("decode FLOAT4 array element"),
            elem_ty,
        )
    } else if elem_pg_ty == &Type::FLOAT8 {
        float8_to_value(
            f64::from_sql(elem_pg_ty, bytes).expect("decode FLOAT8 array element"),
            elem_ty,
        )
    } else if elem_pg_ty == &Type::UUID {
        stmt::Value::Uuid(
            uuid::Uuid::from_sql(elem_pg_ty, bytes).expect("decode UUID array element"),
        )
    } else {
        todo!(
            "implement PostgreSQL array decoding for element type `{:#?}`",
            elem_pg_ty
        )
    }
}

impl ToSql for Value {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> std::result::Result<IsNull, Box<dyn std::error::Error + Sync + Send>>
    where
        Self: Sized,
    {
        value_to_sql(&self.0, ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        matches!(
            *ty,
            Type::BOOL
                | Type::INT2
                | Type::INT4
                | Type::INT8
                | Type::TEXT
                | Type::FLOAT4
                | Type::FLOAT8
                | Type::VARCHAR
                | Type::BYTEA
                | Type::UUID
                | Type::NUMERIC
                | Type::TIMESTAMP
                | Type::TIMESTAMPTZ
                | Type::DATE
                | Type::TIME
        ) || matches!(ty.kind(), Kind::Enum(_) | Kind::Array(_))
    }
    to_sql_checked!();
}

/// Free-fn form of `Value::to_sql` so the array-element closure can call it
/// with a `&CoreValue` borrowed from the input slice — no per-element clone
/// or wrapper construction.
fn value_to_sql(
    value: &CoreValue,
    ty: &Type,
    out: &mut BytesMut,
) -> std::result::Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
    match (value, ty) {
        (stmt::Value::Bool(value), _) => value.to_sql(ty, out),
        (stmt::Value::I8(value), &Type::INT2) => (*value as i16).to_sql(ty, out),
        (stmt::Value::I8(value), &Type::INT4) => (*value as i32).to_sql(ty, out),
        (stmt::Value::I8(value), &Type::INT8) => (*value as i64).to_sql(ty, out),
        (stmt::Value::I16(value), &Type::INT2) => value.to_sql(ty, out),
        (stmt::Value::I16(value), &Type::INT4) => (*value as i32).to_sql(ty, out),
        (stmt::Value::I16(value), &Type::INT8) => (*value as i64).to_sql(ty, out),
        (stmt::Value::I32(value), &Type::INT4) => value.to_sql(ty, out),
        (stmt::Value::I32(value), &Type::INT8) => (*value as i64).to_sql(ty, out),
        (stmt::Value::I64(value), &Type::INT4) => (*value as i32).to_sql(ty, out),
        (stmt::Value::I64(value), &Type::INT8) => value.to_sql(ty, out),
        (stmt::Value::U8(value), &Type::INT2) => (*value as i16).to_sql(ty, out),
        (stmt::Value::U8(value), &Type::INT4) => (*value as i32).to_sql(ty, out),
        (stmt::Value::U8(value), &Type::INT8) => (*value as i64).to_sql(ty, out),
        (stmt::Value::U16(value), &Type::INT4) => (*value as i32).to_sql(ty, out),
        (stmt::Value::U16(value), &Type::INT8) => (*value as i64).to_sql(ty, out),
        (stmt::Value::U32(value), &Type::INT8) => (*value as i64).to_sql(ty, out),
        (stmt::Value::U64(value), &Type::INT8) => {
            if *value > i64::MAX as u64 {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "u64 value {} exceeds i64::MAX ({}), cannot store in PostgreSQL BIGINT",
                        value,
                        i64::MAX
                    ),
                )));
            }
            (*value as i64).to_sql(ty, out)
        }
        (stmt::Value::F32(value), &Type::FLOAT4) => value.to_sql(ty, out),
        (stmt::Value::F32(value), &Type::FLOAT8) => (*value as f64).to_sql(ty, out),
        (stmt::Value::F64(value), &Type::FLOAT4) => (*value as f32).to_sql(ty, out),
        (stmt::Value::F64(value), &Type::FLOAT8) => value.to_sql(ty, out),
        (stmt::Value::Null, _) => Ok(IsNull::Yes),
        // PG enums are wire-encoded as plain UTF-8 text. `String::ToSql::accepts`
        // rejects `Kind::Enum`, so write the bytes directly.
        (stmt::Value::String(value), _) if matches!(ty.kind(), Kind::Enum(_)) => {
            out.extend_from_slice(value.as_bytes());
            Ok(IsNull::No)
        }
        (stmt::Value::String(value), _) => value.to_sql(ty, out),
        (stmt::Value::Bytes(value), &Type::BYTEA) => value.to_sql(ty, out),
        (stmt::Value::Uuid(value), &Type::UUID) => value.to_sql(ty, out),
        #[cfg(feature = "rust_decimal")]
        (stmt::Value::Decimal(value), _) => value.to_sql(ty, out),
        #[cfg(feature = "jiff")]
        (stmt::Value::Timestamp(value), _) => value.to_sql(ty, out),
        #[cfg(feature = "jiff")]
        (stmt::Value::Date(value), _) => value.to_sql(ty, out),
        #[cfg(feature = "jiff")]
        (stmt::Value::Time(value), _) => value.to_sql(ty, out),
        #[cfg(feature = "jiff")]
        (stmt::Value::DateTime(value), _) => value.to_sql(ty, out),
        // List → bind as a PostgreSQL array via the streaming `array_to_sql`
        // primitive: the closure runs per element and writes directly into
        // `out`, so there's no intermediate `Vec<Option<T>>`. The element PG
        // type (carried by the prepared statement, see `db::Type::List` →
        // `to_postgres_type`) drives per-item conversion via this same fn.
        (stmt::Value::List(items), _) => {
            let Kind::Array(elem) = ty.kind() else {
                return Err(format!("Value::List bound to non-array PG type {ty:?}").into());
            };
            let len = i32::try_from(items.len())
                .map_err(|_| format!("array length {} exceeds i32::MAX", items.len()))?;
            array_to_sql(
                [ArrayDimension {
                    len,
                    lower_bound: 1,
                }],
                elem.oid(),
                items.iter(),
                |v, buf| match value_to_sql(v, elem, buf)? {
                    IsNull::No => Ok(postgres_protocol::IsNull::No),
                    IsNull::Yes => Ok(postgres_protocol::IsNull::Yes),
                },
                out,
            )?;
            Ok(IsNull::No)
        }
        (value, _) => todo!("unsupported Value for PostgreSQL type: {value:#?}, type: {ty:#?}"),
    }
}

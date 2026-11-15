use toasty_core::schema::db;
use tokio_postgres::types::Type;

/// Converts a database storage type to a PostgreSQL wire type.
pub(crate) fn to_postgres_type(ty: &db::Type) -> &'static Type {
    match ty {
        db::Type::Boolean => &Type::BOOL,
        db::Type::Float(4) => &Type::FLOAT4,
        db::Type::Float(8) => &Type::FLOAT8,
        db::Type::Integer(1) => &Type::INT2,
        db::Type::Integer(2) => &Type::INT2,
        db::Type::Integer(4) => &Type::INT4,
        db::Type::Integer(8) => &Type::INT8,
        db::Type::UnsignedInteger(1) => &Type::INT2,
        db::Type::UnsignedInteger(2) => &Type::INT4,
        db::Type::UnsignedInteger(4) => &Type::INT8,
        db::Type::UnsignedInteger(8) => &Type::INT8,
        db::Type::Text | db::Type::VarChar(_) => &Type::TEXT,
        db::Type::Uuid => &Type::UUID,
        db::Type::Numeric(_) => &Type::NUMERIC,
        db::Type::Blob | db::Type::Binary(_) => &Type::BYTEA,
        db::Type::Timestamp(_) => &Type::TIMESTAMPTZ,
        db::Type::Date => &Type::DATE,
        db::Type::Time(_) => &Type::TIME,
        db::Type::DateTime(_) => &Type::TIMESTAMP,
        // Enum types are handled separately via the cached OID map;
        // fall back to TEXT if we reach here (shouldn't happen in practice).
        db::Type::Enum(_) => &Type::TEXT,
        db::Type::List(elem) => array_type_of(to_postgres_type(elem)),
        _ => todo!("to_postgres_type; db_ty={ty:#?}"),
    }
}

/// Returns the PostgreSQL array type whose element type is `elem`.
pub(crate) fn array_type_of(elem: &Type) -> &'static Type {
    match *elem {
        Type::BOOL => &Type::BOOL_ARRAY,
        Type::INT2 => &Type::INT2_ARRAY,
        Type::INT4 => &Type::INT4_ARRAY,
        Type::INT8 => &Type::INT8_ARRAY,
        Type::FLOAT4 => &Type::FLOAT4_ARRAY,
        Type::FLOAT8 => &Type::FLOAT8_ARRAY,
        Type::TEXT => &Type::TEXT_ARRAY,
        Type::VARCHAR => &Type::VARCHAR_ARRAY,
        Type::BYTEA => &Type::BYTEA_ARRAY,
        Type::UUID => &Type::UUID_ARRAY,
        Type::NUMERIC => &Type::NUMERIC_ARRAY,
        Type::TIMESTAMP => &Type::TIMESTAMP_ARRAY,
        Type::TIMESTAMPTZ => &Type::TIMESTAMPTZ_ARRAY,
        Type::DATE => &Type::DATE_ARRAY,
        Type::TIME => &Type::TIME_ARRAY,
        _ => todo!("no PostgreSQL array type for element type {elem:?}"),
    }
}

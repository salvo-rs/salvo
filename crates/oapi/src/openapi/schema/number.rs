use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Represents a numeric value in OpenAPI schema validation fields.
///
/// This type preserves the distinction between integers and floating-point numbers,
/// ensuring that integer values like `1` serialize as `1` rather than `1.0` in JSON output.
///
/// # Examples
///
/// ```
/// # use salvo_oapi::schema::Number;
/// let int_val: Number = 42.into();
/// let float_val: Number = 3.14.into();
///
/// assert_eq!(serde_json::to_string(&int_val).unwrap(), "42");
/// assert_eq!(serde_json::to_string(&float_val).unwrap(), "3.14");
/// ```
#[derive(Clone, Debug)]
pub enum Number {
    /// Signed integer value e.g. `1` or `-2`.
    Int(isize),
    /// Unsigned integer value e.g. `0`.
    UInt(usize),
    /// Floating point number e.g. `1.34`.
    Float(f64),
}

impl Eq for Number {}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(left), Self::Int(right)) => left == right,
            (Self::UInt(left), Self::UInt(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left == right,
            _ => false,
        }
    }
}

impl Serialize for Number {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Int(value) => serializer.serialize_i64(*value as i64),
            Self::UInt(value) => serializer.serialize_u64(*value as u64),
            Self::Float(value) => {
                // Serialize whole floats as integers to avoid trailing `.0`
                if value.fract() == 0.0 && value.is_finite() {
                    if *value < 0.0 {
                        serializer.serialize_i64(*value as i64)
                    } else {
                        serializer.serialize_u64(*value as u64)
                    }
                } else {
                    serializer.serialize_f64(*value)
                }
            }
        }
    }
}

struct NumberVisitor;

impl<'de> Visitor<'de> for NumberVisitor {
    type Value = Number;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a number (integer or float)")
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Number::Int(v as isize))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Number::UInt(v as usize))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Number::Float(v))
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(NumberVisitor)
    }
}

macro_rules! impl_from_for_number {
    ( $( $ty:ident => $pat:ident $( as $as:ident )? ),* ) => {
        $(
        impl From<$ty> for Number {
            fn from(value: $ty) -> Self {
                Self::$pat(value $( as $as )?)
            }
        }
        )*
    };
}

#[rustfmt::skip]
impl_from_for_number!(
    f32 => Float as f64, f64 => Float,
    i8 => Int as isize, i16 => Int as isize, i32 => Int as isize, i64 => Int as isize,
    u8 => UInt as usize, u16 => UInt as usize, u32 => UInt as usize, u64 => UInt as usize,
    isize => Int, usize => UInt
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_int() {
        let n = Number::Int(42);
        assert_eq!(serde_json::to_string(&n).unwrap(), "42");
    }

    #[test]
    fn test_serialize_negative_int() {
        let n = Number::Int(-5);
        assert_eq!(serde_json::to_string(&n).unwrap(), "-5");
    }

    #[test]
    fn test_serialize_uint() {
        let n = Number::UInt(100);
        assert_eq!(serde_json::to_string(&n).unwrap(), "100");
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_serialize_float() {
        let n = Number::Float(3.14);
        assert_eq!(serde_json::to_string(&n).unwrap(), "3.14");
    }

    #[test]
    fn test_serialize_whole_float_as_integer() {
        let n = Number::Float(10.0);
        assert_eq!(serde_json::to_string(&n).unwrap(), "10");
    }

    #[test]
    fn test_serialize_negative_whole_float() {
        let n = Number::Float(-3.0);
        assert_eq!(serde_json::to_string(&n).unwrap(), "-3");
    }

    #[test]
    fn test_from_i32() {
        let n: Number = 42i32.into();
        assert_eq!(n, Number::Int(42));
    }

    #[test]
    fn test_from_u64() {
        let n: Number = 100u64.into();
        assert_eq!(n, Number::UInt(100));
    }

    #[test]
    fn test_from_f64() {
        let n: Number = 2.5f64.into();
        assert_eq!(n, Number::Float(2.5));
    }

    #[test]
    fn test_deserialize_int() {
        let n: Number = serde_json::from_str("42").unwrap();
        assert_eq!(n, Number::UInt(42));
    }

    #[test]
    fn test_deserialize_negative_int() {
        let n: Number = serde_json::from_str("-5").unwrap();
        assert_eq!(n, Number::Int(-5));
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_deserialize_float() {
        let n: Number = serde_json::from_str("3.14").unwrap();
        assert_eq!(n, Number::Float(3.14));
    }

    #[test]
    fn test_equality() {
        assert_eq!(Number::Int(1), Number::Int(1));
        assert_eq!(Number::UInt(1), Number::UInt(1));
        assert_eq!(Number::Float(1.5), Number::Float(1.5));
        assert_ne!(Number::Int(1), Number::UInt(1));
    }
}

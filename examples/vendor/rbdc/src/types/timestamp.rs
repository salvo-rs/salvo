use crate::{DateTime, Error};
use rbs::Value;
use serde::Deserializer;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;

/// Timestamp(timestamp_millis:u64)
#[derive(serde::Serialize, Clone, Eq, PartialEq, Hash)]
#[serde(rename = "Timestamp")]
pub struct Timestamp(pub i64);

impl Timestamp {
    #[deprecated(note = "please use utc()")]
    pub fn now() -> Self {
        Self(fastdate::DateTime::utc().unix_timestamp_millis())
    }
    /// utc time
    pub fn utc() -> Self {
        Self(fastdate::DateTime::utc().unix_timestamp_millis())
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        match Value::deserialize(deserializer)?.as_i64() {
            None => Err(Error::custom("warn type decode Json")),
            Some(v) => Ok(Self(v)),
        }
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Debug for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Timestamp({})", self.0)
    }
}

impl From<Timestamp> for Value {
    fn from(arg: Timestamp) -> Self {
        Value::Ext("Timestamp", Box::new(Value::I64(arg.0)))
    }
}

impl FromStr for Timestamp {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Timestamp(i64::from_str(s)?))
    }
}

impl From<Timestamp> for fastdate::DateTime {
    fn from(value: Timestamp) -> Self {
        fastdate::DateTime::from_timestamp_millis(value.0 as i64)
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Timestamp(0)
    }
}

impl From<DateTime> for Timestamp {
    fn from(value: DateTime) -> Self {
        Self(value.unix_timestamp_millis())
    }
}

impl Into<DateTime> for Timestamp {
    fn into(self) -> DateTime {
        DateTime::from_timestamp_millis(self.0)
    }
}

#[cfg(test)]
mod test {
    use crate::DateTime;
    use crate::timestamp::Timestamp;
    use rbs::Value;

    #[test]
    fn test_from_timestamp() {
        let v = Timestamp::utc();
        let dt: DateTime = v.into();
        println!("{}", dt);
    }

    #[test]
    fn test_ser_de() {
        let dt = Timestamp::utc();
        let v = serde_json::to_value(&dt).unwrap();
        let new_dt: Timestamp = serde_json::from_value(v).unwrap();
        assert_eq!(new_dt, dt);
    }

    #[test]
    fn test_decode_timestamp_u64() {
        assert_eq!(Timestamp(1), rbs::from_value(Value::U64(1)).unwrap());
    }

    #[test]
    fn test_decode_timestamp_ext() {
        assert_eq!(
            Timestamp(1),
            rbs::from_value(Value::Ext("Timestamp", Box::new(Value::U64(1)))).unwrap()
        );
    }
}

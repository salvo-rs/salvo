use crate::Error;
use rbs::Value;
use serde::Deserializer;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;

#[derive(serde::Serialize, Clone, Eq, PartialEq, Hash)]
#[serde(rename = "Uuid")]
pub struct Uuid(pub String);

impl<'de> serde::Deserialize<'de> for Uuid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        match Value::deserialize(deserializer)?.into_string() {
            None => Err(D::Error::custom("warn type decode Uuid")),
            Some(v) => Ok(Self(v)),
        }
    }
}

impl Display for Uuid {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Debug for Uuid {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Uuid({})", self.0)
    }
}

impl From<Uuid> for Value {
    fn from(arg: Uuid) -> Self {
        Value::Ext("Uuid", Box::new(Value::String(arg.0)))
    }
}

impl FromStr for Uuid {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Uuid(s.to_string()))
    }
}

impl Uuid {
    ///new for uuid v4
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// '00000000-0000-0000-0000-000000000000'
impl Default for Uuid {
    fn default() -> Self {
        Uuid(uuid::Uuid::default().to_string())
    }
}

#[cfg(test)]
mod test {
    use crate::Uuid;

    #[test]
    fn test_default() {
        let u = Uuid::default();
        println!("{}", u);
        assert_eq!(u.to_string(), "00000000-0000-0000-0000-000000000000");
    }
}

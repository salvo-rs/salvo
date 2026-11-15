#![allow(dead_code)]
use crate::Error;
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use rbs::Value;
use serde::Deserializer;
use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Rem, Sub, SubAssign};
use std::str::FromStr;

pub type RoundingMode = bigdecimal::RoundingMode;

#[derive(serde::Serialize, Clone, Eq, PartialEq, Hash)]
#[serde(rename = "Decimal")]
pub struct Decimal(pub BigDecimal);

impl Decimal {
    pub fn new(arg: &str) -> Result<Self, Error> {
        Decimal::from_str(arg)
    }
    #[inline]
    pub fn from_f64(arg: f64) -> Option<Decimal> {
        use bigdecimal::FromPrimitive;
        match BigDecimal::from_f64(arg) {
            None => None,
            Some(v) => Some(Decimal::from(v)),
        }
    }

    #[inline]
    pub fn from_f32(arg: f32) -> Option<Decimal> {
        use bigdecimal::FromPrimitive;
        match BigDecimal::from_f32(arg) {
            None => None,
            Some(v) => Some(Decimal::from(v)),
        }
    }

    #[inline]
    fn from_i64(n: i64) -> Option<Self> {
        Some(Decimal::from(n))
    }

    #[inline]
    fn from_u64(n: u64) -> Option<Self> {
        Some(Decimal::from(n))
    }

    #[inline]
    fn from_i128(n: i128) -> Option<Self> {
        Some(Decimal::from(n))
    }

    #[inline]
    fn from_u128(n: u128) -> Option<Self> {
        Some(Decimal::from(n))
    }

    ///Return a new Decimal object equivalent to self,
    /// with internal scaling set to the number specified.
    /// If the new_scale is lower than the current value (indicating a larger power of 10),
    /// digits will be dropped (as precision is lower)
    pub fn with_scale(self, arg: i64) -> Self {
        Decimal(self.0.with_scale(arg))
    }

    ///Return a new Decimal object with precision set to new value
    /// let n: Decimal = "129.41675".parse().unwrap();
    ///
    /// assert_eq!(n.with_prec(2),  "130".parse().unwrap());
    ///
    /// let n_p12 = n.with_prec(12);
    /// let (i, scale) = n_p12.as_bigint_and_exponent();
    /// assert_eq!(n_p12, "129.416750000".parse().unwrap());
    /// assert_eq!(i, 129416750000_u64.into());
    /// assert_eq!(scale, 9);
    ///
    pub fn with_prec(self, arg: u64) -> Self {
        Decimal(self.0.with_prec(arg))
    }

    ///Return given number rounded to 'round_digits' precision after the decimal point, using default rounding mode
    /// Default rounding mode is HalfEven, but can be configured at compile-time by the environment variable: RUST_BIGDECIMAL_DEFAULT_ROUNDING_MODE (or by patching build. rs )
    pub fn round(self, round_digits: i64) -> Self {
        Decimal(self.0.round(round_digits))
    }

    ///Return a new Decimal after shortening the digits and rounding
    ///```rust
    /// use rbdc::{Decimal, RoundingMode};
    /// let n: Decimal = "129.41675".parse().unwrap();
    /// assert_eq!(n.with_scale_round(2, RoundingMode::Up),  "129.42".parse().unwrap());
    /// assert_eq!(n.with_scale_round(-1, RoundingMode::Down),  "120".parse().unwrap());
    /// assert_eq!(n.with_scale_round(4, RoundingMode::HalfEven),  "129.4168".parse().unwrap());
    /// ```
    pub fn with_scale_round(&self, new_scale: i64, mode: RoundingMode) -> Self {
        Decimal(self.0.with_scale_round(new_scale, mode))
    }

    ///Number of digits in the non-scaled integer representation
    #[inline]
    pub fn digits(&self) -> u64 {
        self.0.digits()
    }

    /// Returns the scale of the BigDecimal, the total number of
    /// digits to the right of the decimal point (including insignificant
    /// leading zeros)
    #[inline]
    pub fn fractional_digit_count(&self) -> i64 {
        self.0.fractional_digit_count()
    }

    #[inline]
    pub fn abs(&self) -> Self {
        Decimal::from(self.0.abs())
    }
}

impl<'de> serde::Deserialize<'de> for Decimal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let v = Value::deserialize(deserializer)?;
        let string = match v {
            Value::String(v) => v,
            Value::Ext(_, inner_value) => inner_value.to_string(),
            _ => v.to_string(),
        };
        Decimal::from_str(&string).map_err(|e| Error::custom(e.to_string()))
    }
}

impl Display for Decimal {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Debug for Decimal {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Decimal({})", self.0)
    }
}

impl From<BigDecimal> for Decimal {
    fn from(value: BigDecimal) -> Self {
        Self(value)
    }
}
impl From<Decimal> for Value {
    fn from(arg: Decimal) -> Self {
        Value::Ext("Decimal", Box::new(Value::String(arg.0.to_string())))
    }
}

impl From<i32> for Decimal {
    fn from(arg: i32) -> Self {
        Self::from(BigDecimal::from(arg))
    }
}

impl From<u32> for Decimal {
    fn from(arg: u32) -> Self {
        Self::from(BigDecimal::from(arg))
    }
}

impl From<i64> for Decimal {
    fn from(arg: i64) -> Self {
        Self::from(BigDecimal::from(arg))
    }
}

impl From<u64> for Decimal {
    fn from(arg: u64) -> Self {
        Self::from(BigDecimal::from(arg))
    }
}

impl From<i128> for Decimal {
    fn from(arg: i128) -> Self {
        Self::from(BigDecimal::from(arg))
    }
}

impl From<u128> for Decimal {
    fn from(arg: u128) -> Self {
        Self::from(BigDecimal::from(arg))
    }
}

impl TryFrom<f32> for Decimal {
    type Error = Error;

    fn try_from(value: f32) -> Result<Self, Error> {
        Ok(Self(
            BigDecimal::try_from(value).map_err(|e| Error::from(e.to_string()))?,
        ))
    }
}

impl TryFrom<f64> for Decimal {
    type Error = Error;

    fn try_from(value: f64) -> Result<Self, Error> {
        Ok(Self(
            BigDecimal::try_from(value).map_err(|e| Error::from(e.to_string()))?,
        ))
    }
}

impl FromStr for Decimal {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Decimal(
            BigDecimal::from_str(&s).map_err(|e| Error::from(e.to_string()))?,
        ))
    }
}

impl ToPrimitive for Decimal {
    fn to_i64(&self) -> Option<i64> {
        self.0.to_i64()
    }
    fn to_i128(&self) -> Option<i128> {
        self.0.to_i128()
    }
    fn to_u64(&self) -> Option<u64> {
        self.0.to_u64()
    }
    fn to_u128(&self) -> Option<u128> {
        self.0.to_u128()
    }

    fn to_f64(&self) -> Option<f64> {
        self.0.to_f64()
    }
}

impl FromPrimitive for Decimal {
    #[inline]
    fn from_i64(n: i64) -> Option<Self> {
        Some(Self::from(BigDecimal::from_i64(n)?))
    }

    #[inline]
    fn from_u64(n: u64) -> Option<Self> {
        Some(Self::from(BigDecimal::from_u64(n)?))
    }

    #[inline]
    fn from_i128(n: i128) -> Option<Self> {
        Some(Self::from(BigDecimal::from_i128(n)?))
    }

    #[inline]
    fn from_u128(n: u128) -> Option<Self> {
        Some(Self::from(BigDecimal::from_u128(n)?))
    }

    #[inline]
    fn from_f32(n: f32) -> Option<Self> {
        Some(Self::from(BigDecimal::from_f32(n)?))
    }

    #[inline]
    fn from_f64(n: f64) -> Option<Self> {
        Some(Self::from(BigDecimal::from_f64(n)?))
    }
}

impl Default for Decimal {
    fn default() -> Self {
        Decimal(BigDecimal::from(0))
    }
}

impl Add for Decimal {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Decimal(self.0.add(rhs.0))
    }
}

impl Sub for Decimal {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Decimal(self.0.sub(rhs.0))
    }
}

impl Mul for Decimal {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Decimal(self.0.mul(rhs.0))
    }
}

impl Div for Decimal {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Decimal(self.0.div(rhs.0))
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Rem for Decimal {
    type Output = Decimal;

    fn rem(self, other: Decimal) -> Decimal {
        Decimal(self.0.rem(other.0))
    }
}

impl Neg for Decimal {
    type Output = Decimal;

    fn neg(self) -> Self::Output {
        Decimal(self.0.neg())
    }
}

impl AddAssign for Decimal {
    fn add_assign(&mut self, rhs: Self) {
        self.0.add_assign(rhs.0)
    }
}

impl MulAssign for Decimal {
    fn mul_assign(&mut self, rhs: Self) {
        self.0.mul_assign(rhs.0)
    }
}

impl SubAssign for Decimal {
    fn sub_assign(&mut self, rhs: Self) {
        self.0.sub_assign(rhs.0)
    }
}

#[cfg(test)]
mod test {
    use crate::decimal::Decimal;
    use rbs::{from_value, value};
    use std::str::FromStr;

    #[test]
    fn test_add() {
        let v1 = Decimal::from_str("1").unwrap();
        let v2 = Decimal::from_str("1.1").unwrap();
        let v = v1 + v2;
        assert_eq!(v, Decimal::from_str("2.1").unwrap());
    }

    #[test]
    fn test_sub() {
        let v1 = Decimal::new("1").unwrap();
        let v2 = Decimal::new("1.1").unwrap();
        let v = v1 - v2;
        assert_eq!(v, Decimal::new("-0.1").unwrap());
    }

    #[test]
    fn test_mul() {
        let v1 = Decimal::new("1").unwrap();
        let v2 = Decimal::new("1.1").unwrap();
        let v = v1 * v2;
        assert_eq!(v, Decimal::new("1.1").unwrap());
    }

    #[test]
    fn test_div() {
        let v1 = Decimal::new("1").unwrap();
        let v2 = Decimal::new("1.1").unwrap();
        let v = v2 / v1;
        assert_eq!(v, Decimal::new("1.1").unwrap());
    }

    #[test]
    fn test_ser() {
        let v1 = Decimal::from_str("1").unwrap();
        let rv: Decimal = from_value(value!(v1)).unwrap();
        assert_eq!(rv, Decimal::from_str("1").unwrap());
    }

    #[test]
    fn test_ser2() {
        let v1 = Decimal::from_str("1").unwrap();
        let rv: Decimal = serde_json::from_value(serde_json::to_value(v1).unwrap()).unwrap();
        assert_eq!(rv, Decimal::from_str("1").unwrap());
    }

    #[test]
    fn test_ser3() {
        let v1 = value!("1.111");
        let rv: Decimal = rbs::from_value(v1.clone()).unwrap();
        assert_eq!(rv, Decimal::from_str("1.111").unwrap());
    }

    #[test]
    fn test_with_scale() {
        let v1 = Decimal::new("1.123456").unwrap();
        let v = v1.with_scale(2);
        println!("{}", v.to_string());
        assert_eq!(v.to_string(), "1.12");
    }

    #[test]
    fn test_with_prec() {
        let v1 = Decimal::new("1.123456").unwrap();
        let v = v1.with_prec(2);
        println!("{}", v.to_string());
        assert_eq!(v.to_string(), "1.1");
    }

    #[test]
    fn test_parse() {
        let v1 = "1.123456".parse::<Decimal>().unwrap();
        assert_eq!(v1.to_string(), "1.123456");
    }

    #[test]
    fn test_from() {
        let v = Decimal::from_i64(1);
        assert_eq!(v, Some(Decimal::from(1)));
        let v = Decimal::from_u64(1);
        assert_eq!(v, Some(Decimal::from(1)));
        let v = Decimal::from_i128(1);
        assert_eq!(v, Some(Decimal::from(1)));
        let v = Decimal::from_u128(1);
        assert_eq!(v, Some(Decimal::from(1)));
    }

    #[test]
    fn test_try_from_f64() {
        let f = 1.1;
        let v = Decimal::from_f64(f);
        println!("{:?}", v);
        if let Some(v) = v {
            println!("{}", v.to_string());
        }
    }

    #[test]
    fn test_fractional_digit_count() {
        let v = Decimal::new("1.123456").unwrap();
        println!("{}", v.fractional_digit_count());
        assert_eq!(v.fractional_digit_count(), 6);
    }

    #[test]
    fn test_de() {
        let v = serde_json::to_value(1).unwrap();
        let s: Decimal = serde_json::from_value(v).unwrap();
        assert_eq!(s, Decimal::from(1));
    }
}

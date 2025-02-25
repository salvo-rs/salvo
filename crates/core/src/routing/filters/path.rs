//! Path filter implementation.

use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::sync::{Arc, LazyLock};

use indexmap::IndexSet;
use parking_lot::RwLock;
use regex::Regex;

use crate::async_trait;
use crate::http::Request;
use crate::routing::{Filter, PathState};

/// PathWisp
pub trait PathWisp: Send + Sync + fmt::Debug + 'static {
    #[doc(hidden)]
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
    #[doc(hidden)]
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    /// Validate the wisp. Panic if invalid.
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
    /// Detect is that path matched.
    fn detect(&self, state: &mut PathState) -> bool;
}
/// WispBuilder
pub trait WispBuilder: Send + Sync {
    /// Build `PathWisp`.
    fn build(&self, name: String, sign: String, args: Vec<String>) -> Result<WispKind, String>;
}

type WispBuilderMap = RwLock<HashMap<String, Arc<Box<dyn WispBuilder>>>>;
static WISP_BUILDERS: LazyLock<WispBuilderMap> = LazyLock::new(|| {
    let mut map: HashMap<String, Arc<Box<dyn WispBuilder>>> = HashMap::with_capacity(8);
    map.insert(
        "num".into(),
        Arc::new(Box::new(CharsWispBuilder::new(is_num))),
    );
    map.insert(
        "hex".into(),
        Arc::new(Box::new(CharsWispBuilder::new(is_hex))),
    );
    RwLock::new(map)
});

#[inline]
fn is_num(ch: char) -> bool {
    ch.is_ascii_digit()
}
#[inline]
fn is_hex(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

/// Enum of all wisp kinds.
pub enum WispKind {
    /// ConstWisp.
    Const(ConstWisp),
    /// NamedWisp.
    Named(NamedWisp),
    /// CharsWisp.
    Chars(CharsWisp),
    /// RegexWisp.
    Regex(RegexWisp),
    /// CombWisp.
    Comb(CombWisp),
}
impl PathWisp for WispKind {
    #[inline]
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Const(wisp) => wisp.validate(),
            Self::Named(wisp) => wisp.validate(),
            Self::Chars(wisp) => wisp.validate(),
            Self::Regex(wisp) => wisp.validate(),
            Self::Comb(wisp) => wisp.validate(),
        }
    }
    #[inline]
    fn detect(&self, state: &mut PathState) -> bool {
        match self {
            Self::Const(wisp) => wisp.detect(state),
            Self::Named(wisp) => wisp.detect(state),
            Self::Chars(wisp) => wisp.detect(state),
            Self::Regex(wisp) => wisp.detect(state),
            Self::Comb(wisp) => wisp.detect(state),
        }
    }
}
impl Debug for WispKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Const(wisp) => wisp.fmt(f),
            Self::Named(wisp) => wisp.fmt(f),
            Self::Chars(wisp) => wisp.fmt(f),
            Self::Regex(wisp) => wisp.fmt(f),
            Self::Comb(wisp) => wisp.fmt(f),
        }
    }
}
impl From<ConstWisp> for WispKind {
    #[inline]
    fn from(wisp: ConstWisp) -> Self {
        Self::Const(wisp)
    }
}
impl From<NamedWisp> for WispKind {
    #[inline]
    fn from(wisp: NamedWisp) -> Self {
        Self::Named(wisp)
    }
}
impl From<CharsWisp> for WispKind {
    #[inline]
    fn from(wisp: CharsWisp) -> Self {
        Self::Chars(wisp)
    }
}
impl From<RegexWisp> for WispKind {
    #[inline]
    fn from(wisp: RegexWisp) -> Self {
        Self::Regex(wisp)
    }
}
impl From<CombWisp> for WispKind {
    #[inline]
    fn from(wisp: CombWisp) -> Self {
        Self::Comb(wisp)
    }
}

/// RegexWispBuilder
pub struct RegexWispBuilder(Regex);
impl RegexWispBuilder {
    /// Create new `RegexWispBuilder`.
    #[inline]
    pub fn new(checker: Regex) -> Self {
        Self(checker)
    }
}
impl WispBuilder for RegexWispBuilder {
    fn build(&self, name: String, _sign: String, _args: Vec<String>) -> Result<WispKind, String> {
        Ok(RegexWisp {
            name,
            regex: self.0.clone(),
        }
        .into())
    }
}

/// CharsWispBuilder
pub struct CharsWispBuilder(Arc<dyn Fn(char) -> bool + Send + Sync + 'static>);
impl CharsWispBuilder {
    /// Create new `CharsWispBuilder`.
    #[inline]
    pub fn new<C>(checker: C) -> Self
    where
        C: Fn(char) -> bool + Send + Sync + 'static,
    {
        Self(Arc::new(checker))
    }
}
impl WispBuilder for CharsWispBuilder {
    fn build(&self, name: String, _sign: String, args: Vec<String>) -> Result<WispKind, String> {
        if args.is_empty() {
            return Ok(CharsWisp {
                name,
                checker: self.0.clone(),
                min_width: 1,
                max_width: None,
            }
            .into());
        }
        let ps = args[0]
            .splitn(2, "..")
            .map(|s| s.trim())
            .collect::<Vec<_>>();
        let (min_width, max_width) = if ps.is_empty() {
            (1, None)
        } else {
            let min = if ps[0].is_empty() {
                1
            } else {
                let min = ps[0]
                    .parse::<usize>()
                    .map_err(|_| format!("parse range for {name} failed"))?;
                if min < 1 {
                    return Err("min_width must greater or equal to 1".to_owned());
                }
                min
            };
            if ps.len() == 1 {
                (min, None)
            } else {
                let max = ps[1];
                if max.is_empty() {
                    (min, None)
                } else {
                    let trimed_max = max.trim_start_matches('=');
                    let max = if trimed_max == max {
                        let max = trimed_max
                            .parse::<usize>()
                            .map_err(|_| format!("parse range for {name} failed"))?;
                        if max <= 1 {
                            return Err("min_width must greater than 1".to_owned());
                        }
                        max - 1
                    } else {
                        let max = trimed_max
                            .parse::<usize>()
                            .map_err(|_| format!("parse range for {name} failed"))?;
                        if max < 1 {
                            return Err("min_width must greater or equal to 1".to_owned());
                        }
                        max
                    };
                    (min, Some(max))
                }
            }
        };
        Ok(CharsWisp {
            name,
            checker: self.0.clone(),
            min_width,
            max_width,
        }
        .into())
    }
}

/// Chars wisp matches characters in URL segment.
pub struct CharsWisp {
    name: String,
    checker: Arc<dyn Fn(char) -> bool + Send + Sync + 'static>,
    min_width: usize,
    max_width: Option<usize>,
}
impl Debug for CharsWisp {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CharsWisp {{ name: {:?}, min_width: {:?}, max_width: {:?} }}",
            self.name, self.min_width, self.max_width
        )
    }
}
impl PathWisp for CharsWisp {
    fn detect(&self, state: &mut PathState) -> bool {
        let Some(picked) = state.pick() else {
            return false;
        };
        if let Some(max_width) = self.max_width {
            let mut chars = Vec::with_capacity(max_width);
            for ch in picked.chars() {
                if (self.checker)(ch) {
                    chars.push(ch);
                }
                if chars.len() == max_width {
                    state.forward(max_width);
                    state.params.insert(&self.name, chars.into_iter().collect());
                    #[cfg(feature = "matched-path")]
                    state.matched_parts.push(format!("{{{}}}", self.name));
                    return true;
                }
            }
            if chars.len() >= self.min_width {
                state.forward(chars.len());
                state.params.insert(&self.name, chars.into_iter().collect());
                #[cfg(feature = "matched-path")]
                state.matched_parts.push(format!("{{{}}}", self.name));
                true
            } else {
                false
            }
        } else {
            let mut chars = Vec::with_capacity(16);
            for ch in picked.chars() {
                if (self.checker)(ch) {
                    chars.push(ch);
                }
            }
            if chars.len() >= self.min_width {
                state.forward(chars.len());
                state.params.insert(&self.name, chars.into_iter().collect());
                #[cfg(feature = "matched-path")]
                state.matched_parts.push(format!("{{{}}}", self.name));
                true
            } else {
                false
            }
        }
    }
}

/// Comb wisp is a group of other kind of wisps in the same url segment.
#[derive(Debug)]
pub struct CombWisp {
    names: Vec<String>,
    comb_regex: Regex,
    wild_regex: Option<Regex>,
    wild_start: Option<String>,
}
impl CombWisp {
    /// Create new `CombWisp`.
    ///
    /// # Panics
    /// If contains unsupported `WispKind``.
    pub fn new(wisps: Vec<WispKind>) -> Result<Self, String> {
        let mut comb_regex = "^".to_owned();
        let mut names = Vec::with_capacity(wisps.len());
        let mut is_prev_named = false;
        let mut is_greedy = false;
        let mut wild_start = None;
        let mut wild_regex = None;
        let any_chars_regex = Regex::new(".*").expect("regex should worked");
        for wisp in wisps {
            match wisp {
                WispKind::Const(wisp) => {
                    if is_greedy {
                        return Err(format!(
                            "ConstWisp `{}` follows a greedy wisp in CombWisp",
                            wisp.0
                        ));
                    }
                    is_prev_named = false;
                    comb_regex.push_str(&regex::escape(&wisp.0))
                }
                WispKind::Named(wisp) => {
                    if is_greedy {
                        return Err(format!(
                            "NamedWisp `{}` follows a greedy wisp in CombWisp",
                            wisp.0
                        ));
                    }
                    if is_prev_named {
                        return Err(format!(
                            "NamedWisp `{}` should not be added after another NamedWisp when it is CombWisp's children",
                            wisp.0
                        ));
                    }
                    is_prev_named = true;
                    if wisp.0.starts_with('*') {
                        is_greedy = true;
                        let (star_mark, name) = crate::routing::split_wild_name(&wisp.0);
                        wild_regex = Some(any_chars_regex.clone());
                        wild_start = Some(star_mark.to_owned());
                        names.push(name.to_owned());
                    } else {
                        comb_regex.push_str(&format!("(?<{}>.*)", &regex::escape(&wisp.0)));
                        names.push(wisp.0);
                    }
                }
                WispKind::Regex(wisp) => {
                    if is_greedy {
                        return Err(format!(
                            "RegexWisp `{}` follows a greedy wisp in CombWisp",
                            wisp.name
                        ));
                    }
                    is_prev_named = false;
                    if wisp.name.starts_with('*') {
                        is_greedy = true;
                        let (star_mark, name) = crate::routing::split_wild_name(&wisp.name);
                        wild_regex = Some(wisp.regex);
                        wild_start = Some(star_mark.to_owned());
                        names.push(name.to_owned());
                    } else {
                        let regex = wisp
                            .regex
                            .as_str()
                            .trim_start_matches('^')
                            .trim_end_matches('$');
                        comb_regex.push_str(&format!("(?<{}>{})", wisp.name, regex));
                        names.push(wisp.name);
                    }
                }
                WispKind::Chars(wisp) => {
                    return Err(format!(
                        "unsupported CharsWisp `{}` add to CombWisp",
                        wisp.name
                    ));
                }
                _ => {
                    return Err(format!("unsupported wisp: {:?} add to CombWisp", wisp));
                }
            }
        }
        if wild_regex.is_none() {
            comb_regex.push('$');
        }
        Regex::new(&comb_regex)
            .map(|comb_regex| Self {
                names,
                comb_regex,
                wild_regex,
                wild_start,
            })
            .map_err(|e| format!("Regex error: {}", e))
    }
}
impl PathWisp for CombWisp {
    #[inline]
    fn detect(&self, state: &mut PathState) -> bool {
        let Some(picked) = state.pick().map(|s| s.to_owned()) else {
            return false;
        };
        let mut wild_path = if self.wild_regex.is_some() {
            state.all_rest().unwrap_or_default().to_string()
        } else {
            "".to_owned()
        };
        let caps = self.comb_regex.captures(&picked);
        if let Some(caps) = caps {
            let take_count = if self.wild_regex.is_some() {
                self.names.len() - 1
            } else {
                self.names.len()
            };
            #[cfg(feature = "matched-path")]
            let mut start = 0;
            #[cfg(feature = "matched-path")]
            let mut matched_part = "".to_owned();
            for name in self.names.iter().take(take_count) {
                if let Some(value) = caps.name(name) {
                    state.params.insert(name, value.as_str().to_owned());
                    if self.wild_regex.is_some() {
                        wild_path = wild_path.trim_start_matches(value.as_str()).to_string();
                    }
                    #[cfg(feature = "matched-path")]
                    {
                        if value.start() > start {
                            matched_part.push_str(&picked[start..value.start()]);
                        }
                        matched_part.push_str(&format!("{{{}}}", name));
                        start = value.end();
                    }
                } else {
                    return false;
                }
            }
            #[cfg(feature = "matched-path")]
            {
                if start < picked.len() {
                    matched_part.push_str(&picked[start..]);
                }
                if !matched_part.is_empty() {
                    state.matched_parts.push(matched_part);
                }
            }
            let len = if let Some(cap) = caps.get(0) {
                cap.as_str().len()
            } else {
                return false;
            };
            state.forward(len);
        } else {
            return false;
        }
        if let (Some(wild_name), Some(wild_regex), Some(wild_start)) = (
            self.names.last(),
            self.wild_regex.as_ref(),
            self.wild_start.as_ref(),
        ) {
            if wild_start.starts_with("*?")
                && wild_path
                    .trim_start_matches('/')
                    .trim_end_matches('/')
                    .contains('/')
            {
                return false;
            }
            if !wild_path.is_empty() || !wild_start.starts_with("*+") {
                let cap = wild_regex.captures(&wild_path).and_then(|caps| caps.get(0));
                if let Some(cap) = cap {
                    let cap = cap.as_str().to_owned();
                    state.forward(cap.len());
                    state.params.insert(wild_name, cap);
                    #[cfg(feature = "matched-path")]
                    state.matched_parts.push(format!("{{{}}}", wild_name));
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            true
        }
    }
}

/// Named wisp match part in url segment and give it a name.
#[derive(Debug, Eq, PartialEq)]
pub struct NamedWisp(pub String);
impl PathWisp for NamedWisp {
    #[inline]
    fn detect(&self, state: &mut PathState) -> bool {
        if self.0.starts_with('*') {
            let rest = state.all_rest().unwrap_or_default();
            if self.0.starts_with("*?")
                && rest
                    .trim_start_matches('/')
                    .trim_end_matches('/')
                    .contains('/')
            {
                return false;
            }
            if !rest.is_empty() || !self.0.starts_with("*+") {
                let rest = rest.to_string();
                state.params.insert(&self.0, rest);
                state.cursor.0 = state.parts.len();
                #[cfg(feature = "matched-path")]
                state.matched_parts.push(format!("{{{}}}", self.0));
                true
            } else {
                false
            }
        } else {
            let picked = state.pick();
            if picked.is_none() {
                return false;
            }
            let picked = picked.expect("picked should not be `None`").to_owned();
            state.forward(picked.len());
            state.params.insert(&self.0, picked);
            #[cfg(feature = "matched-path")]
            state.matched_parts.push(format!("{{{}}}", self.0));
            true
        }
    }
}

/// Regex wisp match part in url segment use regex pattern and give it a name.
#[derive(Debug)]
#[non_exhaustive]
pub struct RegexWisp {
    /// The name of the wisp.
    pub name: String,
    /// The regex pattern.
    pub regex: Regex,
}
impl RegexWisp {
    #[inline]
    fn new(name: String, regex: &str) -> Result<Self, String> {
        let regex = if !regex.starts_with('^') {
            &*format!("^{}", regex)
        } else {
            regex
        };
        let regex = if !regex.ends_with('$') {
            &*format!("{}$", regex)
        } else {
            regex
        };
        Ok(Self {
            name,
            regex: Regex::new(regex).map_err(|e| format!("invalid regex: `{}`, {}", regex, e))?,
        })
    }
}
impl PartialEq for RegexWisp {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.regex.as_str() == other.regex.as_str()
    }
}
impl PathWisp for RegexWisp {
    #[inline]
    fn detect(&self, state: &mut PathState) -> bool {
        if self.name.starts_with('*') {
            let rest = state.all_rest().unwrap_or_default();
            if self.name.starts_with("*?")
                && rest
                    .trim_start_matches('/')
                    .trim_end_matches('/')
                    .contains('/')
            {
                return false;
            }
            if !rest.is_empty() || !self.name.starts_with("*+") {
                let cap = self.regex.captures(&rest).and_then(|caps| caps.get(0));

                if let Some(cap) = cap {
                    let cap = cap.as_str().to_owned();
                    state.forward(cap.len());
                    state.params.insert(&self.name, cap);
                    #[cfg(feature = "matched-path")]
                    state.matched_parts.push(format!("{{{}}}", self.name));
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            let Some(picked) = state.pick() else {
                return false;
            };
            let cap = self.regex.captures(picked).and_then(|caps| caps.get(0));
            if let Some(cap) = cap {
                let cap = cap.as_str().to_owned();
                state.forward(cap.len());
                state.params.insert(&self.name, cap);
                #[cfg(feature = "matched-path")]
                state.matched_parts.push(format!("{{{}}}", self.name));
                true
            } else {
                false
            }
        }
    }
}

/// Const wisp is used for match the const string in the path.
#[derive(Eq, PartialEq, Debug)]
pub struct ConstWisp(pub String);
impl PathWisp for ConstWisp {
    #[inline]
    fn detect(&self, state: &mut PathState) -> bool {
        let Some(picked) = state.pick() else {
            return false;
        };
        if picked.starts_with(&self.0) {
            state.forward(self.0.len());
            #[cfg(feature = "matched-path")]
            state.matched_parts.push(self.0.clone());
            true
        } else {
            false
        }
    }
}

struct PathParser {
    offset: usize,
    path: Vec<char>,
}
impl PathParser {
    #[inline]
    fn new(raw_value: &str) -> PathParser {
        PathParser {
            offset: 0,
            path: raw_value.trim_start_matches('/').chars().collect(),
        }
    }
    #[inline]
    fn next(&mut self, skip_blanks: bool) -> Option<char> {
        if self.offset < self.path.len() - 1 {
            self.offset += 1;
            if skip_blanks {
                self.skip_blanks();
            }
            Some(self.path[self.offset])
        } else {
            self.offset = self.path.len();
            None
        }
    }
    #[inline]
    fn peek(&self, skip_blanks: bool) -> Option<char> {
        if self.offset < self.path.len() - 1 {
            if skip_blanks {
                let mut offset = self.offset + 1;
                let mut ch = self.path[offset];
                while ch == ' ' || ch == '\t' {
                    offset += 1;
                    if offset >= self.path.len() {
                        return None;
                    }
                    ch = self.path[offset]
                }
                Some(ch)
            } else {
                Some(self.path[self.offset + 1])
            }
        } else {
            None
        }
    }
    #[inline]
    fn curr(&self) -> Option<char> {
        self.path.get(self.offset).copied()
    }
    #[inline]
    fn scan_ident(&mut self) -> Result<String, String> {
        let mut ident = "".to_owned();
        let mut ch = self
            .curr()
            .ok_or_else(|| "current postion is out of index when scan ident".to_owned())?;
        while !['/', ':', '|', '{', '}', '<', '>', '[', ']', '(', ')'].contains(&ch) {
            ident.push(ch);
            if let Some(c) = self.next(false) {
                ch = c;
            } else {
                break;
            }
        }
        if ident.is_empty() {
            Err("ident segment is empty".to_owned())
        } else {
            Ok(ident)
        }
    }
    #[inline]
    fn scan_regex(&mut self) -> Result<String, String> {
        let mut regex = "".to_owned();
        let mut ch = self
            .curr()
            .ok_or_else(|| "current postion is out of index when scan regex".to_owned())?;
        let mut escaping = false;
        let mut brace_opening = false;
        loop {
            regex.push(ch);
            if let Some(c) = self.next(false) {
                ch = c;
                if ch == '{' && !escaping {
                    brace_opening = true;
                } else if ch == '}' && !escaping {
                    if !brace_opening {
                        break;
                    }
                    brace_opening = false;
                }
                escaping = !escaping && ch == '\\';
            } else {
                break;
            }
        }
        if regex.is_empty() {
            Err("regex segment is empty".to_owned())
        } else {
            Ok(regex)
        }
    }
    #[inline]
    fn scan_const(&mut self) -> Result<String, String> {
        let mut cnst = "".to_owned();
        let mut ch = self
            .curr()
            .ok_or_else(|| "current postion is out of index when scan const".to_owned())?;
        while ch != '/' {
            if ch == '{' || ch == '}' {
                // match `{{` or `}}`
                if self.peek(false) == Some(ch) {
                    self.next(false);
                } else {
                    return Ok(cnst);
                }
            }
            cnst.push(ch);
            if let Some(c) = self.next(false) {
                ch = c;
            } else {
                break;
            }
        }
        if cnst.is_empty() {
            Err("const segment is empty".to_owned())
        } else {
            Ok(cnst)
        }
    }
    #[inline]
    fn skip_blanks(&mut self) {
        if let Some(mut ch) = self.curr() {
            while ch == ' ' || ch == '\t' {
                if self.offset < self.path.len() - 1 {
                    self.offset += 1;
                    ch = self.path[self.offset];
                } else {
                    break;
                }
            }
        }
    }
    #[inline]
    fn skip_slashes(&mut self) {
        if let Some(mut ch) = self.curr() {
            while ch == '/' {
                if let Some(c) = self.next(false) {
                    ch = c;
                } else {
                    break;
                }
            }
        }
    }
    fn scan_wisps(&mut self) -> Result<Vec<WispKind>, String> {
        let mut ch = self
            .curr()
            .ok_or_else(|| "current postion is out of index when scan part".to_owned())?;
        let mut wisps: Vec<WispKind> = vec![];
        while ch != '/' {
            if ch == '{' {
                if let Some('{') = self.peek(false) {
                    let part = self.scan_const().unwrap_or_default();
                    if part.is_empty() {
                        return Err("const part is empty string".to_owned());
                    }
                    wisps.push(ConstWisp(part).into());
                    continue;
                }
                self.next(true)
                    .ok_or_else(|| "char is needed after <".to_owned())?;
                let name = self.scan_ident()?;
                if name.is_empty() {
                    return Err("name is empty string".to_owned());
                }
                self.skip_blanks();
                ch = self
                    .curr()
                    .ok_or_else(|| "current position is out of index".to_owned())?;
                if ch == ':' {
                    //start to scan fn part
                    self.next(false);
                    let sign = self.scan_ident()?;
                    self.skip_blanks();
                    let lb = self
                        .curr()
                        .ok_or_else(|| "path ended unexpectedly".to_owned())?;
                    let args = if lb == '[' || lb == '(' {
                        let rb = if lb == '[' { ']' } else { ')' };
                        let mut args = "".to_owned();
                        ch = self.next(true).ok_or_else(|| {
                            "current position is out of index when scan ident".to_owned()
                        })?;
                        while ch != rb {
                            args.push(ch);
                            if let Some(c) = self.next(false) {
                                ch = c;
                            } else {
                                break;
                            }
                        }
                        if self.next(false).is_none() {
                            return Err(format!("ended unexpectedly, should end with: {rb}"));
                        }
                        if args.is_empty() {
                            vec![]
                        } else {
                            args.split(',').map(|s| s.trim().to_owned()).collect()
                        }
                    } else if lb == '}' {
                        vec![]
                    } else {
                        return Err(format!(
                            "except any char of '/,[,(', but found {:?} at offset: {}",
                            self.curr(),
                            self.offset
                        ));
                    };
                    let builders = WISP_BUILDERS.read();
                    let builder = builders
                        .get(&sign)
                        .ok_or_else(|| {
                            format!("WISP_BUILDERS does not contains fn part with sign {sign}")
                        })?
                        .clone();

                    wisps.push(builder.build(name, sign, args)?);
                } else if ch == '|' {
                    // start to scan regex part
                    self.next(false);
                    let regex = &self.scan_regex()?;
                    wisps.push(RegexWisp::new(name, regex)?.into());
                } else if ch == '}' {
                    wisps.push(NamedWisp(name).into());
                }
                if let Some(c) = self.curr() {
                    if c != '}' {
                        return Err(format!(
                            "except '}}' to end regex part or fn part, but found {:?} at offset: {}",
                            c, self.offset
                        ));
                    } else {
                        self.next(false);
                    }
                } else {
                    break;
                }
            } else {
                let part = self.scan_const().unwrap_or_default();
                if part.is_empty() {
                    return Err("const part is empty string".to_owned());
                }
                wisps.push(ConstWisp(part).into());
            }
            if let Some(c) = self.curr() {
                if c == '/' {
                    break;
                }
                ch = c;
            } else {
                break;
            }
        }
        Ok(wisps)
    }

    fn parse(&mut self) -> Result<Vec<WispKind>, String> {
        let mut wisps: Vec<WispKind> = vec![];
        if self.path.is_empty() {
            return Ok(wisps);
        }
        loop {
            self.skip_slashes();
            if self.curr().map(|c| c == '/').unwrap_or(false) {
                return Err(format!(
                    "'/' is not allowed after '/' at offset `{}`",
                    self.offset
                ));
            }
            let mut scaned = self.scan_wisps()?;
            if scaned.len() > 1 {
                wisps.push(CombWisp::new(scaned)?.into());
            } else if let Some(wisp) = scaned.pop() {
                wisps.push(wisp);
            } else {
                return Err("scan parts is empty".to_owned());
            }
            if self.curr().map(|c| c != '/').unwrap_or(false) {
                return Err(format!(
                    "expect '/', but found {:?} at offset `{}`",
                    self.curr(),
                    self.offset
                ));
            }
            if self.next(true).is_none() {
                break;
            }
        }
        let mut all_names = IndexSet::new();
        self.validate(&wisps, &mut all_names)?;
        Ok(wisps)
    }
    fn validate(&self, wisps: &[WispKind], all_names: &mut IndexSet<String>) -> Result<(), String> {
        if !wisps.is_empty() {
            let wild_name = all_names.iter().find(|v| v.starts_with('*'));
            if let Some(wild_name) = wild_name {
                return Err(format!(
                    "wildcard name `{}` must added at the last in url: `{}`",
                    wild_name,
                    self.path.iter().collect::<String>()
                ));
            }
        }
        for (index, wisp) in wisps.iter().enumerate() {
            let name = match wisp {
                WispKind::Named(wisp) => Some(&wisp.0),
                WispKind::Chars(wisp) => Some(&wisp.name),
                WispKind::Regex(wisp) => Some(&wisp.name),
                _ => None,
            };

            if let Some(name) = name {
                if name.starts_with('*') && index != wisps.len() - 1 {
                    return Err(format!(
                        "wildcard name `{}` must added at the last in url: `{}`",
                        name,
                        self.path.iter().collect::<String>()
                    ));
                }
                if all_names.contains(name) {
                    return Err(format!(
                        "name `{}` is duplicated with previous one in url: `{}`",
                        name,
                        self.path.iter().collect::<String>()
                    ));
                }
                all_names.insert(name.clone());
            }
        }
        let wild_names = all_names
            .iter()
            .filter(|v| v.starts_with('*'))
            .map(|c| &**c)
            .collect::<Vec<_>>();
        if wild_names.len() > 1 {
            return Err(format!(
                "many wildcard names: `[{}]` found in url: {}, only one wildcard name is allowed",
                wild_names.join(", "),
                self.path.iter().collect::<String>()
            ));
        } else if let Some(wild_name) = wild_names.first() {
            if wild_name != all_names.last().expect("all_names should not be empty") {
                return Err(format!(
                    "wildcard name: `{}` should be the last one in url: `{}`",
                    wild_name,
                    self.path.iter().collect::<String>()
                ));
            }
        }
        Ok(())
    }
}

/// Filter request by it's path information.
pub struct PathFilter {
    raw_value: String,
    path_wisps: Vec<WispKind>,
}

impl Debug for PathFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "path:{}", &self.raw_value)
    }
}
#[async_trait]
impl Filter for PathFilter {
    #[inline]
    async fn filter(&self, _req: &mut Request, state: &mut PathState) -> bool {
        self.detect(state)
    }
}
impl PathFilter {
    /// Create new `PathFilter`.
    #[inline]
    pub fn new(value: impl Into<String>) -> Self {
        let raw_value = value.into();
        if raw_value.is_empty() {
            tracing::warn!("you should not add empty string as path filter");
        } else if raw_value == "/" {
            tracing::warn!("you should not add '/' as path filter");
        }
        let mut parser = PathParser::new(&raw_value);
        let path_wisps = match parser.parse() {
            Ok(path_wisps) => path_wisps,
            Err(e) => {
                panic!("{}, raw_value: {}", e, raw_value);
            }
        };
        PathFilter {
            raw_value,
            path_wisps,
        }
    }
    /// Register new path wisp builder.
    #[inline]
    pub fn register_wisp_builder<B>(name: impl Into<String>, builder: B)
    where
        B: WispBuilder + 'static,
    {
        let mut builders = WISP_BUILDERS.write();
        builders.insert(name.into(), Arc::new(Box::new(builder)));
    }
    /// Register new path part regex.
    #[inline]
    pub fn register_wisp_regex(name: impl Into<String>, regex: Regex) {
        let mut builders = WISP_BUILDERS.write();
        builders.insert(
            name.into(),
            Arc::new(Box::new(RegexWispBuilder::new(regex))),
        );
    }
    /// Detect is that path is match.
    pub fn detect(&self, state: &mut PathState) -> bool {
        let original_cursor = state.cursor;
        for ps in &self.path_wisps {
            let row = state.cursor.0;
            if ps.detect(state) {
                if row == state.cursor.0 && row != state.parts.len() {
                    state.cursor = original_cursor;
                    return false;
                }
            } else {
                state.cursor = original_cursor;
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::PathParser;
    use crate::routing::{PathFilter, PathState};

    #[test]
    fn test_parse_empty() {
        let segments = PathParser::new("").parse().unwrap();
        assert!(segments.is_empty());
    }
    #[test]
    fn test_parse_root() {
        let segments = PathParser::new("/").parse().unwrap();
        assert!(segments.is_empty());
    }

    #[test]
    fn test_parse_single_const() {
        let segments = PathParser::new("/hello").parse().unwrap();
        assert_eq!(format!("{:?}", segments), r#"[ConstWisp("hello")]"#);
    }
    #[test]
    fn test_parse_multi_const() {
        let segments = PathParser::new("/hello/world").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[ConstWisp("hello"), ConstWisp("world")]"#
        );
    }
    #[test]
    fn test_parse_single_regex() {
        let segments = PathParser::new(r"/{abc|\d+}").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[RegexWisp { name: "abc", regex: Regex("^\\d+$") }]"#
        );
    }
    #[test]
    fn test_parse_wildcard_regex() {
        let segments = PathParser::new(r"/{abc|\d+\.+}").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[RegexWisp { name: "abc", regex: Regex("^\\d+\\.+$") }]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_prefix() {
        let segments = PathParser::new(r"/prefix_{abc|\d+}").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["abc"], comb_regex: Regex("^prefix_(?<abc>\\d+)$"), wild_regex: None, wild_start: None }]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_suffix() {
        let segments = PathParser::new(r"/{abc|\d+}_suffix.png").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["abc"], comb_regex: Regex("^(?<abc>\\d+)_suffix\\.png$"), wild_regex: None, wild_start: None }]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_prefix_and_suffix() {
        let segments = PathParser::new(r"/prefix{abc|\d+}suffix.png")
            .parse()
            .unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["abc"], comb_regex: Regex("^prefix(?<abc>\\d+)suffix\\.png$"), wild_regex: None, wild_start: None }]"#
        );
    }
    #[test]
    fn test_parse_dot_after_param() {
        let segments = PathParser::new(r"/{pid}/show/{table_name}.bu")
            .parse()
            .unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[NamedWisp("pid"), ConstWisp("show"), CombWisp { names: ["table_name"], comb_regex: Regex("^(?<table_name>.*)\\.bu$"), wild_regex: None, wild_start: None }]"#
        );
    }
    #[test]
    fn test_parse_multi_regex() {
        let segments = PathParser::new(r"/first{id}/prefix{abc|\d+}")
            .parse()
            .unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["id"], comb_regex: Regex("^first(?<id>.*)$"), wild_regex: None, wild_start: None }, CombWisp { names: ["abc"], comb_regex: Regex("^prefix(?<abc>\\d+)$"), wild_regex: None, wild_start: None }]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_prefix() {
        let segments = PathParser::new(r"/first{id}/prefix{abc|\d+}")
            .parse()
            .unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["id"], comb_regex: Regex("^first(?<id>.*)$"), wild_regex: None, wild_start: None }, CombWisp { names: ["abc"], comb_regex: Regex("^prefix(?<abc>\\d+)$"), wild_regex: None, wild_start: None }]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_suffix() {
        let segments = PathParser::new(r"/first{id|\d+}/prefix{abc|\d+}")
            .parse()
            .unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["id"], comb_regex: Regex("^first(?<id>\\d+)$"), wild_regex: None, wild_start: None }, CombWisp { names: ["abc"], comb_regex: Regex("^prefix(?<abc>\\d+)$"), wild_regex: None, wild_start: None }]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_prefix_and_suffix() {
        let segments = PathParser::new(r"/first{id}/prefix{abc|\d+}ext")
            .parse()
            .unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["id"], comb_regex: Regex("^first(?<id>.*)$"), wild_regex: None, wild_start: None }, CombWisp { names: ["abc"], comb_regex: Regex("^prefix(?<abc>\\d+)ext$"), wild_regex: None, wild_start: None }]"#
        );
    }
    #[test]
    fn test_parse_rest() {
        let segments = PathParser::new(r"/first{id}/{**rest}").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["id"], comb_regex: Regex("^first(?<id>.*)$"), wild_regex: None, wild_start: None }, NamedWisp("**rest")]"#
        );

        let segments = PathParser::new(r"/first{id}/{*+rest}").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["id"], comb_regex: Regex("^first(?<id>.*)$"), wild_regex: None, wild_start: None }, NamedWisp("*+rest")]"#
        );

        let segments = PathParser::new(r"/first{id}/{*?rest}").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["id"], comb_regex: Regex("^first(?<id>.*)$"), wild_regex: None, wild_start: None }, NamedWisp("*?rest")]"#
        );
    }
    #[test]
    fn test_parse_num() {
        assert!(PathParser::new(r"/first{id:num}").parse().is_err());
    }
    #[test]
    fn test_parse_named_follow_another_panic() {
        assert!(PathParser::new(r"/first{id}{id2}ext2").parse().is_err());
    }

    #[test]
    fn test_parse_comb_1() {
        let segments = PathParser::new(r"/first{id}world{**rest}").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp { names: ["id", "rest"], comb_regex: Regex("^first(?<id>.*)world"), wild_regex: Some(Regex(".*")), wild_start: Some("**") }]"#
        );

        let filter = PathFilter::new("/first{id}world{**rest}");
        let mut state = PathState::new("first123world.ext");
        assert!(filter.detect(&mut state));
    }

    #[test]
    fn test_parse_comb_2() {
        let filter = PathFilter::new("/abc/hello{id}world{**rest}");
        let mut state = PathState::new("abc/hello123world.ext");
        assert!(filter.detect(&mut state));
    }

    #[test]
    fn test_parse_comb_3() {
        let filter = PathFilter::new("/{id}/{name}!hello.bu");
        let mut state = PathState::new("123/gold!hello.bu");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_parse_comb_4() {
        let filter = PathFilter::new("/abc/l{**rest}");
        let mut state = PathState::new("abc/llo1");
        assert!(filter.detect(&mut state));

        let mut state = PathState::new("abc/hello1");
        assert!(!filter.detect(&mut state));
    }
    #[test]
    fn test_parse_comb_5() {
        let filter = PathFilter::new(r"/abc/t{**rest|\d+}");
        let mut state = PathState::new("abc/t11");
        assert!(!filter.detect(&mut state));

        let mut state = PathState::new("abc/tlo1");
        assert!(!filter.detect(&mut state));
        let mut state = PathState::new("abc/t11a");
        assert!(!filter.detect(&mut state));
    }

    #[test]
    fn test_parse_rest2_failed() {
        assert!(
            PathParser::new(r"/first{id}{*ext}/{**rest}")
                .parse()
                .is_err()
        );
    }

    #[test]
    fn test_parse_rest_failed1() {
        assert!(
            PathParser::new(r"/first{id}ext2/{**rest}{id}")
                .parse()
                .is_err()
        );
    }
    #[test]
    fn test_parse_rest_failed2() {
        assert!(
            PathParser::new(r"/first{id}ext2/{**rest}wefwe")
                .parse()
                .is_err()
        );
    }
    #[test]
    fn test_parse_many_slashes() {
        let wisps = PathParser::new(r"/first///second//{id}").parse().unwrap();
        assert_eq!(wisps.len(), 3);
    }

    #[test]
    fn test_detect_consts() {
        let filter = PathFilter::new("/hello/world");
        let mut state = PathState::new("hello/world");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_detect_consts0() {
        let filter = PathFilter::new("/hello/world/");
        let mut state = PathState::new("hello/world");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_detect_consts1() {
        let filter = PathFilter::new("/hello/world");
        let mut state = PathState::new("hello/world/");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_detect_consts2() {
        let filter = PathFilter::new("/hello/world2");
        let mut state = PathState::new("hello/world");
        assert!(!filter.detect(&mut state));
    }

    #[test]
    fn test_detect_const_and_named() {
        let filter = PathFilter::new("/hello/world{id}");
        let mut state = PathState::new("hello/worldabc");
        filter.detect(&mut state);
    }

    #[test]
    fn test_detect_many() {
        let filter = PathFilter::new("/users/{id}/emails");
        let mut state = PathState::new("/users/29/emails");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_detect_many_slashes() {
        let filter = PathFilter::new("/users/{id}/emails");
        let mut state = PathState::new("/users///29//emails");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_detect_named_regex() {
        PathFilter::register_wisp_regex(
            "guid",
            regex::Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap(),
        );
        let filter = PathFilter::new("/users/{id:guid}");
        let mut state = PathState::new("/users/123e4567-h89b-12d3-a456-9AC7CBDCEE52");
        assert!(!filter.detect(&mut state));

        let mut state = PathState::new("/users/123e4567-e89b-12d3-a456-9AC7CBDCEE52");
        assert!(filter.detect(&mut state));
        assert_eq!(
            state.matched_parts,
            vec!["users".to_owned(), "{id}".to_owned()]
        );
    }
    #[test]
    fn test_detect_wildcard() {
        let filter = PathFilter::new("/users/{id}/{**rest}");
        let mut state = PathState::new("/users/12/facebook/insights/23");
        assert!(filter.detect(&mut state));
        assert_eq!(
            state.matched_parts,
            vec!["users".to_owned(), "{id}".to_owned(), "{**rest}".to_owned()]
        );
        let mut state = PathState::new("/users/12/");
        assert!(filter.detect(&mut state));
        let mut state = PathState::new("/users/12");
        assert!(filter.detect(&mut state));
        assert_eq!(
            state.matched_parts,
            vec!["users".to_owned(), "{id}".to_owned(), "{**rest}".to_owned()]
        );

        let filter = PathFilter::new("/users/{id}/{*+rest}");
        let mut state = PathState::new("/users/12/facebook/insights/23");
        assert!(filter.detect(&mut state));
        let mut state = PathState::new("/users/12/");
        assert!(!filter.detect(&mut state));
        let mut state = PathState::new("/users/12");
        assert!(!filter.detect(&mut state));

        let filter = PathFilter::new("/users/{id}/{*?rest}");
        let mut state = PathState::new("/users/12/facebook/insights/23");
        assert!(!filter.detect(&mut state));
        let mut state = PathState::new("/users/12/");
        assert!(filter.detect(&mut state));
        let mut state = PathState::new("/users/12");
        assert!(filter.detect(&mut state));
        let mut state = PathState::new("/users/12/abc");
        assert!(filter.detect(&mut state));
        assert_eq!(
            state.matched_parts,
            vec!["users".to_owned(), "{id}".to_owned(), "{*?rest}".to_owned()]
        );
    }
}

use std::collections::HashMap;
use std::fmt::{self, Formatter};
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use regex::Regex;

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
    /// Detect is that path matched.
    fn detect(&self, state: &mut PathState) -> bool;
}
/// WispBuilder
pub trait WispBuilder: Send + Sync {
    /// Build `PathWisp`.
    fn build(&self, name: String, sign: String, args: Vec<String>) -> Result<Box<dyn PathWisp>, String>;
}

type WispBuilderMap = RwLock<HashMap<String, Arc<Box<dyn WispBuilder>>>>;
static WISP_BUILDERS: Lazy<WispBuilderMap> = Lazy::new(|| {
    let mut map: HashMap<String, Arc<Box<dyn WispBuilder>>> = HashMap::with_capacity(8);
    map.insert("num".into(), Arc::new(Box::new(CharWispBuilder::new(is_num))));
    map.insert("hex".into(), Arc::new(Box::new(CharWispBuilder::new(is_hex))));
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
    fn build(&self, name: String, _sign: String, _args: Vec<String>) -> Result<Box<dyn PathWisp>, String> {
        Ok(Box::new(RegexWisp {
            name,
            regex: self.0.clone(),
        }))
    }
}

/// CharWispBuilder
pub struct CharWispBuilder<C>(Arc<C>);
impl<C> CharWispBuilder<C> {
    /// Create new `CharWispBuilder`.
    #[inline]
    pub fn new(checker: C) -> Self {
        Self(Arc::new(checker))
    }
}
impl<C> WispBuilder for CharWispBuilder<C>
where
    C: Fn(char) -> bool + Send + Sync + 'static,
{
    fn build(&self, name: String, _sign: String, args: Vec<String>) -> Result<Box<dyn PathWisp>, String> {
        if args.is_empty() {
            return Ok(Box::new(CharWisp {
                name,
                checker: self.0.clone(),
                min_width: 1,
                max_width: None,
            }));
        }
        let ps = args[0].splitn(2, "..").map(|s| s.trim()).collect::<Vec<_>>();
        let (min_width, max_width) = if ps.is_empty() {
            (1, None)
        } else {
            let min = if ps[0].is_empty() {
                1
            } else {
                let min = ps[0]
                    .parse::<usize>()
                    .map_err(|_| format!("parse range for {} failed", name))?;
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
                            .map_err(|_| format!("parse range for {} failed", name))?;
                        if max <= 1 {
                            return Err("min_width must greater than 1".to_owned());
                        }
                        max - 1
                    } else {
                        let max = trimed_max
                            .parse::<usize>()
                            .map_err(|_| format!("parse range for {} failed", name))?;
                        if max < 1 {
                            return Err("min_width must greater or equal to 1".to_owned());
                        }
                        max
                    };
                    (min, Some(max))
                }
            }
        };
        Ok(Box::new(CharWisp {
            name,
            checker: self.0.clone(),
            min_width,
            max_width,
        }))
    }
}

struct CharWisp<C> {
    name: String,
    checker: Arc<C>,
    min_width: usize,
    max_width: Option<usize>,
}
impl<C> fmt::Debug for CharWisp<C> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CharWisp {{ name: {:?}, min_width: {:?}, max_width: {:?} }}",
            self.name, self.min_width, self.max_width
        )
    }
}
impl<C> PathWisp for CharWisp<C>
where
    C: Fn(char) -> bool + Send + Sync + 'static,
{
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let picked = state.pick();
        if picked.is_none() {
            return false;
        }
        let picked = picked.unwrap();
        if let Some(max_width) = self.max_width {
            let mut chars = Vec::with_capacity(max_width);
            for ch in picked.chars() {
                if (self.checker)(ch) {
                    chars.push(ch);
                }
                if chars.len() == max_width {
                    state.forward(max_width);
                    state.params.insert(self.name.clone(), chars.into_iter().collect());
                    return true;
                }
            }
            if chars.len() >= self.min_width {
                state.forward(chars.len());
                state.params.insert(self.name.clone(), chars.into_iter().collect());
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
                state.params.insert(self.name.clone(), chars.into_iter().collect());
                true
            } else {
                false
            }
        }
    }
}

#[derive(Debug)]
struct CombWisp(Vec<Box<dyn PathWisp>>);
impl PathWisp for CombWisp {
    #[inline]
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let original_cursor = state.cursor;
        for child in &self.0 {
            if !child.detect(state) {
                state.cursor = original_cursor;
                return false;
            }
        }
        true
    }
}
#[derive(Debug, Eq, PartialEq)]
struct NamedWisp(String);
impl PathWisp for NamedWisp {
    #[inline]
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        if self.0.starts_with('*') {
            let rest = state.all_rest().unwrap_or_default();
            if !rest.is_empty() || self.0.starts_with("**") {
                let rest = rest.to_string();
                state.params.insert(self.0.clone(), rest);
                state.cursor.0 = state.parts.len();
                true
            } else {
                false
            }
        } else {
            let picked = state.pick();
            if picked.is_none() {
                return false;
            }
            let picked = picked.unwrap().to_owned();
            state.forward(picked.len());
            state.params.insert(self.0.clone(), picked);
            true
        }
    }
}

#[derive(Debug)]
struct RegexWisp {
    name: String,
    regex: Regex,
}
impl RegexWisp {
    #[inline]
    fn new(name: String, regex: Regex) -> RegexWisp {
        RegexWisp { name, regex }
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
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        if self.name.starts_with('*') {
            let rest = state.all_rest();
            if rest.is_none() {
                return false;
            }
            let rest = &*rest.unwrap();
            if !rest.is_empty() || self.name.starts_with("**") {
                let cap = self.regex.captures(rest).and_then(|caps| caps.get(0));
                if let Some(cap) = cap {
                    let cap = cap.as_str().to_owned();
                    state.forward(cap.len());
                    state.params.insert(self.name.clone(), cap);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            let picked = state.pick();
            if picked.is_none() {
                return false;
            }
            let picked = picked.unwrap();
            let cap = self.regex.captures(picked).and_then(|caps| caps.get(0));
            if let Some(cap) = cap {
                let cap = cap.as_str().to_owned();
                state.forward(cap.len());
                state.params.insert(self.name.clone(), cap);
                true
            } else {
                false
            }
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
struct ConstWisp(String);
impl PathWisp for ConstWisp {
    #[inline]
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let picked = state.pick();
        if picked.is_none() {
            return false;
        }
        let picked = picked.unwrap();
        if picked.starts_with(&self.0) {
            state.forward(self.0.len());
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
            path: raw_value.chars().collect(),
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
        while !['/', ':', '<', '>', '[', ']', '(', ')'].contains(&ch) {
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
        loop {
            regex.push(ch);
            if let Some(c) = self.next(false) {
                ch = c;
                if ch == '/' {
                    let pch = self.peek(true);
                    if pch.is_none() {
                        return Err("path end but regex is not ended".to_owned());
                    } else if let Some('>') = pch {
                        self.next(true);
                        break;
                    }
                }
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
        while !['/', ':', '<', '>', '[', ']', '(', ')'].contains(&ch) {
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
    fn scan_wisps(&mut self) -> Result<Vec<Box<dyn PathWisp>>, String> {
        let mut ch = self
            .curr()
            .ok_or_else(|| "current postion is out of index when scan part".to_owned())?;
        let mut wisps: Vec<Box<dyn PathWisp>> = vec![];
        while ch != '/' {
            if ch == '<' {
                self.next(true).ok_or_else(|| "char is needed after <".to_owned())?;
                let name = self.scan_ident()?;
                if name.is_empty() {
                    return Err("name is empty string".to_owned());
                }
                self.skip_blanks();
                ch = self
                    .curr()
                    .ok_or_else(|| "current position is out of index".to_owned())?;
                if ch == ':' {
                    let is_slash = match self.next(true) {
                        Some(c) => c == '/',
                        None => false,
                    };
                    if !is_slash {
                        //start to scan fn part
                        let sign = self.scan_ident()?;
                        self.skip_blanks();
                        let lb = self.curr().ok_or_else(|| "path ended unexcept".to_owned())?;
                        let args = if lb == '[' || lb == '(' {
                            let rb = if lb == '[' { ']' } else { ')' };
                            let mut args = "".to_owned();
                            ch = self
                                .next(true)
                                .ok_or_else(|| "current postion is out of index when scan ident".to_owned())?;
                            while ch != rb {
                                args.push(ch);
                                if let Some(c) = self.next(false) {
                                    ch = c;
                                } else {
                                    break;
                                }
                            }
                            if self.next(false).is_none() {
                                return Err(format!("ended unexcept, should end with: {}", rb));
                            }
                            if args.is_empty() {
                                vec![]
                            } else {
                                args.split(',').map(|s| s.trim().to_owned()).collect()
                            }
                        } else if lb == '>' {
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
                            .ok_or_else(|| format!("WISP_BUILDERS does not contains fn part with sign {}", sign))?
                            .clone();

                        wisps.push(builder.build(name, sign, args)?);
                    } else {
                        self.next(false);
                        let regex = Regex::new(&self.scan_regex()?).map_err(|e| e.to_string())?;
                        wisps.push(Box::new(RegexWisp::new(name, regex)));
                    }
                } else if ch == '>' {
                    wisps.push(Box::new(NamedWisp(name)));
                    if !self.peek(false).map(|c| c == '/').unwrap_or(true) {
                        return Err(format!(
                                "named part must be the last one in current segement, expect '/' or end, but found {:?} at offset: {}",
                                self.curr(),
                                self.offset
                            ));
                    }
                }
                if let Some(c) = self.curr() {
                    if c != '>' {
                        return Err(format!(
                            "except '>' to end regex part or fn part, but found {:?} at offset: {}",
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
                wisps.push(Box::new(ConstWisp(part)));
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

    fn parse(&mut self) -> Result<Vec<Box<dyn PathWisp>>, String> {
        let mut wisps: Vec<Box<dyn PathWisp>> = vec![];
        if self.path.is_empty() {
            return Ok(wisps);
        }
        loop {
            self.skip_slashes();
            if self.offset >= self.path.len() - 1 {
                break;
            }
            if self.curr().map(|c| c == '/').unwrap_or(false) {
                return Err(format!("'/' is not allowed after '/' at offset {:?}", self.offset));
            }
            let mut scaned = self.scan_wisps()?;
            if scaned.len() > 1 {
                wisps.push(Box::new(CombWisp(scaned)));
            } else if !scaned.is_empty() {
                wisps.push(scaned.pop().unwrap());
            } else {
                return Err("scan parts is empty".to_owned());
            }
            if self.curr().map(|c| c != '/').unwrap_or(false) {
                return Err(format!(
                    "expect '/', but found {:?} at offset {:?}",
                    self.curr(),
                    self.offset
                ));
            }
            self.next(true);
            if self.offset >= self.path.len() - 1 {
                break;
            }
        }
        Ok(wisps)
    }
}

/// Filter request by it's path information.
pub struct PathFilter {
    raw_value: String,
    path_wisps: Vec<Box<dyn PathWisp>>,
}

impl fmt::Debug for PathFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "path:{}", &self.raw_value)
    }
}
impl Filter for PathFilter {
    #[inline]
    fn filter(&self, _req: &mut Request, state: &mut PathState) -> bool {
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
                panic!("{}", e);
            }
        };
        PathFilter { raw_value, path_wisps }
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
        builders.insert(name.into(), Arc::new(Box::new(RegexWispBuilder::new(regex))));
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
    fn test_parse_rest_without_name() {
        let segments = PathParser::new("/hello/<*>").parse().unwrap();
        assert_eq!(format!("{:?}", segments), r#"[ConstWisp("hello"), NamedWisp("*")]"#);
    }

    #[test]
    fn test_parse_single_const() {
        let segments = PathParser::new("/hello").parse().unwrap();
        assert_eq!(format!("{:?}", segments), r#"[ConstWisp("hello")]"#);
    }
    #[test]
    fn test_parse_multi_const() {
        let segments = PathParser::new("/hello/world").parse().unwrap();
        assert_eq!(format!("{:?}", segments), r#"[ConstWisp("hello"), ConstWisp("world")]"#);
    }
    #[test]
    fn test_parse_single_regex() {
        let segments = PathParser::new(r"/<abc:/\d+/>").parse().unwrap();
        assert_eq!(format!("{:?}", segments), r#"[RegexWisp { name: "abc", regex: \d+ }]"#);
    }
    #[test]
    fn test_parse_wildcard_regex() {
        let segments = PathParser::new(r"/<abc:/\d+/.+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[RegexWisp { name: "abc", regex: \d+/.+ }]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_prefix() {
        let segments = PathParser::new(r"/prefix_<abc:/\d+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("prefix_"), RegexWisp { name: "abc", regex: \d+ }])]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_suffix() {
        let segments = PathParser::new(r"/<abc:/\d+/>_suffix.png").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([RegexWisp { name: "abc", regex: \d+ }, ConstWisp("_suffix.png")])]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_prefix_and_suffix() {
        let segments = PathParser::new(r"/prefix<abc:/\d+/>suffix.png").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("prefix"), RegexWisp { name: "abc", regex: \d+ }, ConstWisp("suffix.png")])]"#
        );
    }
    #[test]
    fn test_parse_multi_regex() {
        let segments = PathParser::new(r"/first<id>/prefix<abc:/\d+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), NamedWisp("id")]), CombWisp([ConstWisp("prefix"), RegexWisp { name: "abc", regex: \d+ }])]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_prefix() {
        let segments = PathParser::new(r"/first<id>/prefix<abc:/\d+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), NamedWisp("id")]), CombWisp([ConstWisp("prefix"), RegexWisp { name: "abc", regex: \d+ }])]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_suffix() {
        let segments = PathParser::new(r"/first<id:/\d+/>/prefix<abc:/\d+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), RegexWisp { name: "id", regex: \d+ }]), CombWisp([ConstWisp("prefix"), RegexWisp { name: "abc", regex: \d+ }])]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_prefix_and_suffix() {
        let segments = PathParser::new(r"/first<id>/prefix<abc:/\d+/>ext").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), NamedWisp("id")]), CombWisp([ConstWisp("prefix"), RegexWisp { name: "abc", regex: \d+ }, ConstWisp("ext")])]"#
        );
    }
    #[test]
    fn test_parse_rest() {
        let segments = PathParser::new(r"/first<id>/<*rest>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), NamedWisp("id")]), NamedWisp("*rest")]"#
        );
    }
    #[test]
    fn test_parse_num0() {
        let segments = PathParser::new(r"/first<id:num>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), CharWisp { name: "id", min_width: 1, max_width: None }])]"#
        );
    }
    #[test]
    fn test_parse_num1() {
        let segments = PathParser::new(r"/first<id:num(10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), CharWisp { name: "id", min_width: 10, max_width: None }])]"#
        );
    }
    #[test]
    fn test_parse_num2() {
        let segments = PathParser::new(r"/first<id:num(..10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), CharWisp { name: "id", min_width: 1, max_width: Some(9) }])]"#
        );
    }
    #[test]
    fn test_parse_num3() {
        let segments = PathParser::new(r"/first<id:num(3..10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), CharWisp { name: "id", min_width: 3, max_width: Some(9) }])]"#
        );
    }
    #[test]
    fn test_parse_num4() {
        let segments = PathParser::new(r"/first<id:num[3..]>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), CharWisp { name: "id", min_width: 3, max_width: None }])]"#
        );
    }
    #[test]
    fn test_parse_num5() {
        let segments = PathParser::new(r"/first<id:num(3..=10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombWisp([ConstWisp("first"), CharWisp { name: "id", min_width: 3, max_width: Some(10) }])]"#
        );
    }
    #[test]
    fn test_parse_named_failed1() {
        assert!(PathParser::new(r"/first<id>ext2").parse().is_err());
    }

    #[test]
    fn test_parse_rest_failed1() {
        assert!(PathParser::new(r"/first<id>ext2<*rest>").parse().is_err());
    }
    #[test]
    fn test_parse_rest_failed2() {
        assert!(PathParser::new(r"/first<id>ext2/<*rest>wefwe").parse().is_err());
    }
    #[test]
    fn test_parse_many_slashes() {
        let wisps = PathParser::new(r"/first///second//<id>").parse().unwrap();
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
        let filter = PathFilter::new("/hello/world<id>");
        let mut state = PathState::new("hello/worldabc");
        filter.detect(&mut state);
    }

    #[test]
    fn test_detect_many() {
        let filter = PathFilter::new("/users/<id>/emails");
        let mut state = PathState::new("/users/29/emails");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_detect_many_slashes() {
        let filter = PathFilter::new("/users/<id>/emails");
        let mut state = PathState::new("/users///29//emails");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_detect_named_regex() {
        PathFilter::register_wisp_regex(
            "guid",
            regex::Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap(),
        );
        let filter = PathFilter::new("/users/<id:guid>");
        let mut state = PathState::new("/users/123e4567-h89b-12d3-a456-9AC7CBDCEE52");
        assert!(!filter.detect(&mut state));

        let mut state = PathState::new("/users/123e4567-e89b-12d3-a456-9AC7CBDCEE52");
        assert!(filter.detect(&mut state));
    }
    #[test]
    fn test_detect_wildcard() {
        let filter = PathFilter::new("/users/<id>/<**rest>");
        let mut state = PathState::new("/users/12/facebook/insights/23");
        assert!(filter.detect(&mut state));
    }
}

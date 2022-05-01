use std::collections::HashMap;
use std::fmt::{self, Formatter};
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use regex::Regex;

use crate::http::Request;
use crate::routing::{Filter, PathState};

/// PathPart
pub trait PathPart: Send + Sync + fmt::Debug {
    /// Detect is that path matched.
    fn detect(&self, state: &mut PathState) -> bool;
}
/// PartBuilder
pub trait PartBuilder: Send + Sync {
    /// Build `PathPart`.
    fn build(&self, name: String, sign: String, args: Vec<String>) -> Result<Box<dyn PathPart>, String>;
}

type PartBuilderMap = RwLock<HashMap<String, Arc<Box<dyn PartBuilder>>>>;
static PART_BUILDERS: Lazy<PartBuilderMap> = Lazy::new(|| {
    let mut map: HashMap<String, Arc<Box<dyn PartBuilder>>> = HashMap::with_capacity(8);
    map.insert("num".into(), Arc::new(Box::new(CharPartBuilder::new(is_num))));
    map.insert("hex".into(), Arc::new(Box::new(CharPartBuilder::new(is_hex))));
    RwLock::new(map)
});

fn is_num(ch: char) -> bool {
    ch.is_ascii_digit()
}
fn is_hex(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

/// RegexPartBuilder
pub struct RegexPartBuilder(Regex);
impl RegexPartBuilder {
    /// Create new `RegexPartBuilder`.
    pub fn new(checker: Regex) -> Self {
        Self(checker)
    }
}
impl PartBuilder for RegexPartBuilder {
    fn build(&self, name: String, _sign: String, _args: Vec<String>) -> Result<Box<dyn PathPart>, String> {
        Ok(Box::new(RegexPart {
            name,
            regex: self.0.clone(),
        }))
    }
}

/// CharPartBuilder
pub struct CharPartBuilder<C>(Arc<C>);
impl<C> CharPartBuilder<C> {
    /// Create new `CharPartBuilder`.
    pub fn new(checker: C) -> Self {
        Self(Arc::new(checker))
    }
}
impl<C> PartBuilder for CharPartBuilder<C>
where
    C: Fn(char) -> bool + Sync + Send + 'static,
{
    fn build(&self, name: String, _sign: String, args: Vec<String>) -> Result<Box<dyn PathPart>, String> {
        if args.is_empty() {
            return Ok(Box::new(CharPart {
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
        Ok(Box::new(CharPart {
            name,
            checker: self.0.clone(),
            min_width,
            max_width,
        }))
    }
}

struct CharPart<C> {
    name: String,
    checker: Arc<C>,
    min_width: usize,
    max_width: Option<usize>,
}
impl<C> fmt::Debug for CharPart<C> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CharPart {{ name: {:?}, min_width: {:?}, max_width: {:?} }}",
            self.name, self.min_width, self.max_width
        )
    }
}
impl<C> PathPart for CharPart<C>
where
    C: Fn(char) -> bool + Sync + Send + 'static,
{
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let url_path = &state.url_path[state.cursor..];
        if url_path.is_empty() {
            return false;
        }
        let segment = url_path.splitn(2, '/').collect::<Vec<_>>()[0];
        if let Some(max_width) = self.max_width {
            let mut chars = Vec::with_capacity(max_width);
            for ch in segment.chars() {
                if (self.checker)(ch) {
                    chars.push(ch);
                }
                if chars.len() == max_width {
                    state.cursor += max_width;
                    state.params.insert(self.name.clone(), chars.into_iter().collect());
                    return true;
                }
            }
            if chars.len() >= self.min_width {
                state.cursor += chars.len();
                state.params.insert(self.name.clone(), chars.into_iter().collect());
                true
            } else {
                false
            }
        } else {
            let mut chars = Vec::with_capacity(16);
            for ch in segment.chars() {
                if (self.checker)(ch) {
                    chars.push(ch);
                }
            }
            if chars.len() >= self.min_width {
                state.cursor += chars.len();
                state.params.insert(self.name.clone(), chars.into_iter().collect());
                true
            } else {
                false
            }
        }
    }
}

#[derive(Debug)]
struct CombPart(Vec<Box<dyn PathPart>>);
impl PathPart for CombPart {
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
struct NamedPart(String);
impl PathPart for NamedPart {
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let url_path = &state.url_path[state.cursor..];
        if self.0.starts_with('*') {
            if !url_path.is_empty() || self.0.starts_with("**") {
                state.params.insert(self.0.clone(), url_path.to_owned());
                state.cursor = state.url_path.len();
                true
            } else {
                false
            }
        } else {
            if url_path.is_empty() {
                return false;
            }
            let segment = url_path.splitn(2, '/').collect::<Vec<_>>()[0];
            state.params.insert(self.0.clone(), segment.to_owned());
            state.cursor += segment.len();
            true
        }
    }
}

#[derive(Debug)]
struct RegexPart {
    name: String,
    regex: Regex,
}
impl RegexPart {
    fn new(name: String, regex: Regex) -> RegexPart {
        RegexPart { name, regex }
    }
}
impl PartialEq for RegexPart {
    fn eq(&self, other: &Self) -> bool {
        self.regex.as_str() == other.regex.as_str()
    }
}
impl PathPart for RegexPart {
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let url_path = &state.url_path[state.cursor..];
        if self.name.starts_with('*') {
            if !url_path.is_empty() || self.name.starts_with("**") {
                let cap = self.regex.captures(url_path).and_then(|caps| caps.get(0));
                if let Some(cap) = cap {
                    let cap = cap.as_str().to_owned();
                    state.cursor += cap.len();
                    state.params.insert(self.name.clone(), cap);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            if url_path.is_empty() {
                return false;
            }
            let segment = url_path.splitn(2, '/').collect::<Vec<_>>()[0];
            let cap = self.regex.captures(segment).and_then(|caps| caps.get(0));
            if let Some(cap) = cap {
                let cap = cap.as_str().to_owned();
                state.cursor += cap.len();
                state.params.insert(self.name.clone(), cap);
                true
            } else {
                false
            }
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
struct ConstPart(String);
impl PathPart for ConstPart {
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let url_path = &state.url_path[state.cursor..];
        if url_path.is_empty() {
            return false;
        }
        let segment = url_path.splitn(2, '/').collect::<Vec<_>>()[0];
        if segment.starts_with(&self.0) {
            state.cursor += self.0.len();
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
    fn new(raw_value: &str) -> PathParser {
        PathParser {
            offset: 0,
            path: raw_value.chars().collect(),
        }
    }
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
    fn curr(&self) -> Option<char> {
        self.path.get(self.offset).copied()
    }
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
    fn scan_parts(&mut self) -> Result<Vec<Box<dyn PathPart>>, String> {
        let mut ch = self
            .curr()
            .ok_or_else(|| "current postion is out of index when scan part".to_owned())?;
        let mut parts: Vec<Box<dyn PathPart>> = vec![];
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
                        let builders = PART_BUILDERS.read();
                        let builder = builders
                            .get(&sign)
                            .ok_or_else(|| format!("PART_BUILDERS does not contains fn part with sign {}", sign))?
                            .clone();

                        parts.push(builder.build(name, sign, args)?);
                    } else {
                        self.next(false);
                        let regex = Regex::new(&self.scan_regex()?).map_err(|e| e.to_string())?;
                        parts.push(Box::new(RegexPart::new(name, regex)));
                    }
                } else if ch == '>' {
                    parts.push(Box::new(NamedPart(name)));
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
                parts.push(Box::new(ConstPart(part)));
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
        Ok(parts)
    }
    fn parse(&mut self) -> Result<Vec<Box<dyn PathPart>>, String> {
        let mut path_parts: Vec<Box<dyn PathPart>> = vec![];
        if self.path.is_empty() {
            return Ok(path_parts);
        }
        loop {
            self.skip_slashes();
            if self.offset >= self.path.len() - 1 {
                break;
            }
            if self.curr().map(|c| c == '/').unwrap_or(false) {
                return Err(format!("'/' is not allowed after '/' at offset {:?}", self.offset));
            }
            let mut parts = self.scan_parts()?;
            if parts.len() > 1 {
                path_parts.push(Box::new(CombPart(parts)));
            } else if !parts.is_empty() {
                path_parts.push(parts.pop().unwrap());
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
        Ok(path_parts)
    }
}

/// Filter request by it's path information.
pub struct PathFilter {
    raw_value: String,
    path_parts: Vec<Box<dyn PathPart>>,
}

impl fmt::Debug for PathFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "path:{}", &self.raw_value)
    }
}
impl Filter for PathFilter {
    fn filter(&self, _req: &mut Request, state: &mut PathState) -> bool {
        self.detect(state)
    }
}
impl PathFilter {
    /// Create new `PathFilter`.
    pub fn new(value: impl Into<String>) -> Self {
        let raw_value = value.into();
        if raw_value.is_empty() {
            tracing::warn!("you should not add empty string as path filter");
        } else if raw_value == "/" {
            tracing::warn!("you should not add '/' as path filter");
        }
        let mut parser = PathParser::new(&raw_value);
        let path_parts = match parser.parse() {
            Ok(path_parts) => path_parts,
            Err(e) => {
                panic!("{}", e);
            }
        };
        PathFilter { raw_value, path_parts }
    }
    /// Register new path part builder.
    pub fn register_part_builder<B>(name: impl Into<String>, builder: B)
    where
        B: PartBuilder + 'static,
    {
        let mut builders = PART_BUILDERS.write();
        builders.insert(name.into(), Arc::new(Box::new(builder)));
    }
    /// Register new path part regex.
    pub fn register_part_regex(name: impl Into<String>, regex: Regex) {
        let mut builders = PART_BUILDERS.write();
        builders.insert(name.into(), Arc::new(Box::new(RegexPartBuilder::new(regex))));
    }
    /// Detect is that path is match.
    pub fn detect(&self, state: &mut PathState) -> bool {
        let original_cursor = state.cursor;
        for ps in &self.path_parts {
            if ps.detect(state) {
                if !state.ended() {
                    let rest = &state.url_path[state.cursor..];
                    if rest.starts_with('/') {
                        state.cursor += 1;
                        let mut rest = &state.url_path[state.cursor..];
                        while rest.starts_with('/') {
                            state.cursor += 1;
                            rest = &state.url_path[state.cursor..];
                        }
                    } else if !rest.is_empty() {
                        state.cursor = original_cursor;
                        return false;
                    }
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
        assert_eq!(format!("{:?}", segments), r#"[ConstPart("hello"), NamedPart("*")]"#);
    }

    #[test]
    fn test_parse_single_const() {
        let segments = PathParser::new("/hello").parse().unwrap();
        assert_eq!(format!("{:?}", segments), r#"[ConstPart("hello")]"#);
    }
    #[test]
    fn test_parse_multi_const() {
        let segments = PathParser::new("/hello/world").parse().unwrap();
        assert_eq!(format!("{:?}", segments), r#"[ConstPart("hello"), ConstPart("world")]"#);
    }
    #[test]
    fn test_parse_single_regex() {
        let segments = PathParser::new(r"/<abc:/\d+/>").parse().unwrap();
        assert_eq!(format!("{:?}", segments), r#"[RegexPart { name: "abc", regex: \d+ }]"#);
    }
    #[test]
    fn test_parse_wildcard_regex() {
        let segments = PathParser::new(r"/<abc:/\d+/.+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[RegexPart { name: "abc", regex: \d+/.+ }]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_prefix() {
        let segments = PathParser::new(r"/prefix_<abc:/\d+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("prefix_"), RegexPart { name: "abc", regex: \d+ }])]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_suffix() {
        let segments = PathParser::new(r"/<abc:/\d+/>_suffix.png").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([RegexPart { name: "abc", regex: \d+ }, ConstPart("_suffix.png")])]"#
        );
    }
    #[test]
    fn test_parse_single_regex_with_prefix_and_suffix() {
        let segments = PathParser::new(r"/prefix<abc:/\d+/>suffix.png").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("prefix"), RegexPart { name: "abc", regex: \d+ }, ConstPart("suffix.png")])]"#
        );
    }
    #[test]
    fn test_parse_multi_regex() {
        let segments = PathParser::new(r"/first<id>/prefix<abc:/\d+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NamedPart("id")]), CombPart([ConstPart("prefix"), RegexPart { name: "abc", regex: \d+ }])]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_prefix() {
        let segments = PathParser::new(r"/first<id>/prefix<abc:/\d+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NamedPart("id")]), CombPart([ConstPart("prefix"), RegexPart { name: "abc", regex: \d+ }])]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_suffix() {
        let segments = PathParser::new(r"/first<id:/\d+/>/prefix<abc:/\d+/>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), RegexPart { name: "id", regex: \d+ }]), CombPart([ConstPart("prefix"), RegexPart { name: "abc", regex: \d+ }])]"#
        );
    }
    #[test]
    fn test_parse_multi_regex_with_prefix_and_suffix() {
        let segments = PathParser::new(r"/first<id>/prefix<abc:/\d+/>ext").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NamedPart("id")]), CombPart([ConstPart("prefix"), RegexPart { name: "abc", regex: \d+ }, ConstPart("ext")])]"#
        );
    }
    #[test]
    fn test_parse_rest() {
        let segments = PathParser::new(r"/first<id>/<*rest>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NamedPart("id")]), NamedPart("*rest")]"#
        );
    }
    #[test]
    fn test_parse_num0() {
        let segments = PathParser::new(r"/first<id:num>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), CharPart { name: "id", min_width: 1, max_width: None }])]"#
        );
    }
    #[test]
    fn test_parse_num1() {
        let segments = PathParser::new(r"/first<id:num(10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), CharPart { name: "id", min_width: 10, max_width: None }])]"#
        );
    }
    #[test]
    fn test_parse_num2() {
        let segments = PathParser::new(r"/first<id:num(..10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), CharPart { name: "id", min_width: 1, max_width: Some(9) }])]"#
        );
    }
    #[test]
    fn test_parse_num3() {
        let segments = PathParser::new(r"/first<id:num(3..10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), CharPart { name: "id", min_width: 3, max_width: Some(9) }])]"#
        );
    }
    #[test]
    fn test_parse_num4() {
        let segments = PathParser::new(r"/first<id:num[3..]>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), CharPart { name: "id", min_width: 3, max_width: None }])]"#
        );
    }
    #[test]
    fn test_parse_num5() {
        let segments = PathParser::new(r"/first<id:num(3..=10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), CharPart { name: "id", min_width: 3, max_width: Some(10) }])]"#
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
        let segments = PathParser::new(r"/first///second//<id>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[ConstPart("first"), ConstPart("second"), NamedPart("id")]"#
        );
    }

    #[test]
    fn test_detect_consts() {
        let filter = PathFilter::new("/hello/world");
        let mut state = PathState::new("hello/world");
        filter.detect(&mut state);
        assert_eq!(
            format!("{:?}", state),
            r#"PathState { url_path: "hello/world", cursor: 11, params: {} }"#
        );
    }
    #[test]
    fn test_detect_consts0() {
        let filter = PathFilter::new("/hello/world/");
        let mut state = PathState::new("hello/world");
        assert!(filter.detect(&mut state));
        assert_eq!(
            format!("{:?}", state),
            r#"PathState { url_path: "hello/world", cursor: 11, params: {} }"#
        );
    }
    #[test]
    fn test_detect_consts1() {
        let filter = PathFilter::new("/hello/world");
        let mut state = PathState::new("hello/world/");
        assert!(filter.detect(&mut state));
        assert_eq!(
            format!("{:?}", state),
            r#"PathState { url_path: "hello/world", cursor: 11, params: {} }"#
        );
    }
    #[test]
    fn test_detect_consts2() {
        let filter = PathFilter::new("/hello/world2");
        let mut state = PathState::new("hello/world");
        assert!(!filter.detect(&mut state));
        assert_eq!(
            format!("{:?}", state),
            r#"PathState { url_path: "hello/world", cursor: 0, params: {} }"#
        );
    }

    #[test]
    fn test_detect_const_and_named() {
        let filter = PathFilter::new("/hello/world<id>");
        let mut state = PathState::new("hello/worldabc");
        filter.detect(&mut state);
        assert_eq!(
            format!("{:?}", state),
            r#"PathState { url_path: "hello/worldabc", cursor: 14, params: {"id": "abc"} }"#
        );
    }

    #[test]
    fn test_detect_many() {
        let filter = PathFilter::new("/users/<id>/emails");
        let mut state = PathState::new("/users/29/emails");
        assert!(filter.detect(&mut state));
        assert_eq!(
            format!("{:?}", state),
            r#"PathState { url_path: "users/29/emails", cursor: 15, params: {"id": "29"} }"#
        );
    }
    #[test]
    fn test_detect_many_slashes() {
        let filter = PathFilter::new("/users/<id>/emails");
        let mut state = PathState::new("/users///29//emails");
        assert!(filter.detect(&mut state));
        assert_eq!(
            format!("{:?}", state),
            r#"PathState { url_path: "users///29//emails", cursor: 18, params: {"id": "29"} }"#
        );
    }
    #[test]
    fn test_detect_named_regex() {
        PathFilter::register_part_regex(
            "guid",
            regex::Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap(),
        );
        let filter = PathFilter::new("/users/<id:guid>");
        let mut state = PathState::new("/users/123e4567-h89b-12d3-a456-9AC7CBDCEE52");
        assert!(!filter.detect(&mut state));
        
        let mut state = PathState::new("/users/123e4567-e89b-12d3-a456-9AC7CBDCEE52");
        assert!(filter.detect(&mut state));
        assert_eq!(
            format!("{:?}", state),
            r#"PathState { url_path: "users/123e4567-e89b-12d3-a456-9AC7CBDCEE52", cursor: 42, params: {"id": "123e4567-e89b-12d3-a456-9AC7CBDCEE52"} }"#
        );
    }
}

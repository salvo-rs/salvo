use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::sync::Arc;

use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::RwLock;

use crate::http::Request;
use crate::routing::{Filter, PathState};

pub trait PathPart: Send + Sync + Debug {
    fn detect<'a>(&self, state: &mut PathState) -> bool;
}

type PartCreatorMap = RwLock<
    HashMap<
        String,
        Arc<Box<dyn Fn(String, String, Vec<String>) -> Result<Box<dyn PathPart>, String> + Send + Sync + 'static>>,
    >,
>;
static PART_CREATORS: Lazy<PartCreatorMap> = Lazy::new(|| {
    let mut map: HashMap<
        String,
        Arc<Box<dyn Fn(String, String, Vec<String>) -> Result<Box<dyn PathPart>, String> + Send + Sync + 'static>>,
    > = HashMap::with_capacity(8);
    map.insert("num".into(), Arc::new(Box::new(NumPart::build)));
    RwLock::new(map)
});

#[derive(Debug)]
struct NumPart {
    name: String,
    min_width: usize,
    max_width: Option<usize>,
}
impl NumPart {
    fn build(name: String, _sign: String, args: Vec<String>) -> Result<Box<dyn PathPart>, String> {
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
        Ok(Box::new(NumPart {
            name: name.to_owned(),
            min_width,
            max_width,
        }))
    }
}
impl PathPart for NumPart {
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let url_path = &state.url_path[state.cursor..];
        if url_path.is_empty() {
            return false;
        }
        let segment = url_path.splitn(2, '/').collect::<Vec<_>>()[0];
        if let Some(max_width) = self.max_width {
            let mut chars = Vec::with_capacity(max_width);
            for ch in segment.chars() {
                if ch.is_numeric() {
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
                if ch.is_numeric() {
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
        for child in &self.0 {
            if !child.detect(state) {
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
        if url_path.is_empty() {
            return false;
        }
        let segment = url_path.splitn(2, '/').collect::<Vec<_>>()[0];
        state.params.insert(self.0.clone(), segment.to_owned());
        state.cursor += segment.len();
        true
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
        if url_path.is_empty() {
            return false;
        }
        let segment = url_path.splitn(2, '/').collect::<Vec<_>>()[0];
        let caps = self.regex.captures(segment);
        if let Some(caps) = caps {
            state.params.insert(self.name.clone(), caps[&self.name[..]].to_owned());
            state.cursor += segment.len();
            true
        } else {
            false
        }
    }
}

// If name starts with *, only match not empty path, if name starts with ** will match empty path.
#[derive(Eq, PartialEq, Debug)]
struct RestPart(String);
impl RestPart {
    fn new(name: String) -> RestPart {
        RestPart(name)
    }
}
impl PathPart for RestPart {
    fn detect<'a>(&self, state: &mut PathState) -> bool {
        let url_path = &state.url_path[state.cursor..];
        if !url_path.is_empty() || self.0.starts_with("**") {
            state.params.insert(self.0.clone(), url_path.to_owned());
            true
        } else {
            false
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
        if segment.contains(&self.0) {
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
    fn next(&mut self, skip_blank: bool) -> Option<char> {
        if self.offset < self.path.len() - 1 {
            self.offset += 1;
            if skip_blank {
                self.skip_blank();
            }
            Some(self.path[self.offset])
        } else {
            self.offset = self.path.len();
            None
        }
    }
    fn peek(&self, skip_blank: bool) -> Option<char> {
        if self.offset < self.path.len() - 1 {
            if skip_blank {
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
    fn skip_blank(&mut self) {
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
    fn skip_slash(&mut self) {
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
                ch = self.next(true).ok_or_else(|| "char is needed after <".to_owned())?;
                if ch == '*' {
                    self.next(true);
                    let name = format!("*{}", self.scan_ident().unwrap_or_default());
                    if self.offset < self.path.len() - 1 {
                        return Err("no chars allowed after rest segment".to_owned());
                    }
                    parts.push(Box::new(RestPart::new(name)));
                    self.next(false);
                    break;
                } else {
                    let name = self.scan_ident()?;
                    if name.is_empty() {
                        return Err("name is empty string".to_owned());
                    }
                    self.skip_blank();
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
                            self.skip_blank();
                            let lb = self.curr().ok_or("path ended unexcept".to_owned())?;
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
                            let creators = PART_CREATORS
                                .read()
                                .map_err(|_| "read PART_CREATORS failed".to_owned())?;
                            let creator = creators
                                .get(&sign)
                                .ok_or_else(|| format!("PART_CREATORS does not contains fn part with sign {}", sign))?
                                .clone();

                            parts.push(creator(name, sign, args)?);
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
            self.skip_slash();
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

pub struct PathFilter {
    raw_value: String,
    path_parts: Vec<Box<dyn PathPart>>,
}

impl Debug for PathFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ raw_value: '{}'}}", &self.raw_value)
    }
}
impl Filter for PathFilter {
    fn filter(&self, _req: &mut Request, state: &mut PathState) -> bool {
        self.detect(state)
    }
}
impl PathFilter {
    pub fn new(value: impl Into<String>) -> Self {
        let raw_value = value.into();
        let mut parser = PathParser::new(&raw_value);
        let path_parts = match parser.parse() {
            Ok(path_parts) => path_parts,
            Err(e) => {
                panic!("{}", e);
            }
        };
        PathFilter { raw_value, path_parts }
    }
    pub fn register_creator<P>(name: String, creator: P)
    where
        P: Fn(String, String, Vec<String>) -> Result<Box<dyn PathPart>, String> + Send + Sync + 'static,
    {
        PART_CREATORS.write().unwrap().insert(name, Arc::new(Box::new(creator)));
    }
    pub fn detect(&self, state: &mut PathState) -> bool {
        if state.ended() {
            return false;
        }
        if !self.path_parts.is_empty() {
            for (i, ps) in self.path_parts.iter().enumerate() {
                if ps.detect(state) {
                    if state.ended() {
                        return i == self.path_parts.len() - 1;
                    }
                    let rest = &state.url_path[state.cursor..];
                    if rest.starts_with('/') {
                        state.cursor += 1;
                    } else if !rest.is_empty() {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        } else {
            false
        }
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
        assert_eq!(format!("{:?}", segments), r#"[ConstPart("hello"), RestPart("*")]"#);
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
            r#"[CombPart([ConstPart("first"), NamedPart("id")]), RestPart("*rest")]"#
        );
    }
    #[test]
    fn test_parse_num() {
        let segments = PathParser::new(r"/first<id:num(10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NumPart { name: "id", min_width: 10, max_width: None }])]"#
        );
    }
    #[test]
    fn test_parse_num2() {
        let segments = PathParser::new(r"/first<id:num(..10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NumPart { name: "id", min_width: 1, max_width: Some(9) }])]"#
        );
    }
    #[test]
    fn test_parse_num3() {
        let segments = PathParser::new(r"/first<id:num(3..10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NumPart { name: "id", min_width: 3, max_width: Some(9) }])]"#
        );
    }
    #[test]
    fn test_parse_num4() {
        let segments = PathParser::new(r"/first<id:num[3..]>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NumPart { name: "id", min_width: 3, max_width: None }])]"#
        );
    }
    #[test]
    fn test_parse_num5() {
        let segments = PathParser::new(r"/first<id:num(3..=10)>").parse().unwrap();
        assert_eq!(
            format!("{:?}", segments),
            r#"[CombPart([ConstPart("first"), NumPart { name: "id", min_width: 3, max_width: Some(10) }])]"#
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
            r#"PathState { url_path: "hello/world", cursor: 6, params: {} }"#
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
}

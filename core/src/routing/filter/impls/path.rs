use std::collections::HashMap;
use std::fmt::{self, Debug};

use regex::Regex;

use crate::http::Request;
use crate::routing::{Filter, PathState};

trait Segment: Send + Sync + Debug {
    fn detect<'a>(&self, segments: Vec<&'a str>) -> (bool, Option<PathMatched<'a>>);
}

struct PathMatched<'a> {
    ending_matched: bool,
    segments: Option<Vec<&'a str>>,
    matched_params: Option<HashMap<String, String>>,
}

#[derive(Debug)]
struct RegexSegment {
    regex: Regex,
    names: Vec<String>,
}
impl RegexSegment {
    fn new(regex: Regex, names: Vec<String>) -> RegexSegment {
        RegexSegment { regex, names }
    }
}
impl PartialEq for RegexSegment {
    fn eq(&self, other: &Self) -> bool {
        self.regex.as_str() == other.regex.as_str()
    }
}
impl Segment for RegexSegment {
    fn detect<'a>(&self, segments: Vec<&'a str>) -> (bool, Option<PathMatched<'a>>) {
        if segments.is_empty() {
            return (false, None);
        }
        let segment = segments[0];
        let caps = self.regex.captures(segment);
        if let Some(caps) = caps {
            let mut kv = HashMap::<String, String>::new();
            for name in &self.names {
                kv.insert(name.clone(), caps[&name[..]].to_owned());
            }
            (
                true,
                Some(PathMatched {
                    ending_matched: false,
                    segments: Some(vec![segment]),
                    matched_params: Some(kv),
                }),
            )
        } else {
            (false, None)
        }
    }
}

// If name starts with *, only match not empty path, if name starts with ** will match empty path.
#[derive(Eq, PartialEq, Debug)]
struct RestSegment(String);
impl RestSegment {
    fn new(name: String) -> RestSegment {
        RestSegment(name)
    }
}
impl Segment for RestSegment {
    fn detect<'a>(&self, segments: Vec<&'a str>) -> (bool, Option<PathMatched<'a>>) {
        if !segments.is_empty() || self.0.starts_with("**") {
            let mut kv = HashMap::new();
            kv.insert(self.0.clone(), segments.join("/"));
            (
                true,
                Some(PathMatched {
                    ending_matched: true,
                    segments: Some(segments),
                    matched_params: Some(kv),
                }),
            )
        } else {
            (false, None)
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
struct ConstSegment(String);
impl ConstSegment {
    fn new(segment: String) -> ConstSegment {
        ConstSegment(segment)
    }
}
impl Segment for ConstSegment {
    fn detect<'a>(&self, segments: Vec<&'a str>) -> (bool, Option<PathMatched<'a>>) {
        if segments.is_empty() {
            return (false, None);
        }
        if self.0 == segments[0] {
            (
                true,
                Some(PathMatched {
                    ending_matched: false,
                    segments: Some(vec![segments[0]]),
                    matched_params: None,
                }),
            )
        } else {
            (false, None)
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
        let mut ch = self.curr().ok_or_else(||"current postion is out of index when scan ident".to_owned())?;
        while ch != '/' && ch != ':' && ch != '<' && ch != '>' {
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
        let mut ch = self.curr().ok_or_else(||"current postion is out of index when scan regex".to_owned())?;
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
        let mut ch = self.curr().ok_or_else(||"current postion is out of index when scan const".to_owned())?;
        while ch != '/' && ch != ':' && ch != '<' && ch != '>' {
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
    fn parse(&mut self) -> Result<Vec<Box<dyn Segment>>, String> {
        let mut segments: Vec<Box<dyn Segment>> = vec![];
        if self.path.is_empty() {
            return Ok(segments);
        }
        loop {
            self.skip_slash();
            if self.offset >= self.path.len() - 1 {
                break;
            }
            let mut const_seg = "".to_owned();
            let mut regex_seg = "".to_owned();
            let mut regex_names = vec![];
            let mut ch = self.curr().ok_or_else(||"current postion is out of index".to_owned())?;
            while ch != '/' {
                if ch == '<' {
                    ch = self.next(true).expect("char is needed after <");
                    if ch == '*' {
                        if !const_seg.is_empty() {
                            segments.push(Box::new(ConstSegment::new(const_seg)));
                        }
                        if !regex_seg.is_empty() {
                            return Err(format!("rest and regex pattern can not be in same path segement, regex: {:?}", regex_seg));
                        }
                        self.next(true);
                        let rest_seg = self.scan_ident()?;
                        if self.offset < self.path.len() - 1 {
                            return Err("no chars allowed after rest segment".to_owned());
                        }
                        segments.push(Box::new(RestSegment::new(format!("*{}", rest_seg))));
                        return Ok(segments);
                    } else {
                        let rname = self.scan_ident()?;
                        if rname.is_empty() {
                            return Err("name is empty string".to_owned());
                        } else {
                            regex_names.push(rname.clone());
                        }
                        let mut rrgex = "[^/]+".to_owned();
                        ch = self.curr().ok_or_else(||"current postion is out of index".to_owned())?;
                        if ch == ':' {
                            let is_slash = match self.next(true) {
                                Some(c) => c == '/',
                                None => false,
                            };
                            if !is_slash {
                                return Err(format!("except '/' to start regex, but found {:?} at offset: {}", self.curr(), self.offset));
                            }
                            self.next(false);
                            rrgex = self.scan_regex()?;
                        }
                        if let Some(c) = self.curr() {
                            if c != '>' {
                                return Err(format!("except '>' to end regex segment, but found {:?} at offset: {}", c, self.offset));
                            } else {
                                self.next(false);
                            }
                        } else {
                            break;
                        }
                        if !const_seg.is_empty() {
                            regex_seg.push_str(&const_seg);
                            const_seg.clear();
                        }
                        regex_seg.push_str(&format!("(?P<{}>{})", rname, rrgex));
                        if let Ok(const_seg) = self.scan_const() {
                            regex_seg.push_str(&const_seg);
                        }
                        if let Some(c) = self.curr() {
                            ch = c;
                        } else {
                            break;
                        }
                    }
                } else {
                    const_seg = self.scan_const().unwrap_or_default();
                    if let Some(c) = self.curr() {
                        if c != '/' && c != '<' {
                            return Err(format!("expect '/' or '<', but found {:?} at offset {}", self.curr(), self.offset));
                        }
                        ch = c;
                    } else {
                        break;
                    }
                }
            }
            if self.curr().map(|c|c != '/').unwrap_or(false) {
                return Err(format!("expect '/', but found {:?} at offset {:?}", self.curr(), self.offset));
            }
            if !regex_seg.is_empty() {
                if !const_seg.is_empty() {
                    regex_seg.push_str(&const_seg);
                }
                let regex = Regex::new(&regex_seg);
                match regex {
                    Ok(r) => segments.push(Box::new(RegexSegment::new(r, regex_names))),
                    Err(_) => return Err("regex formate error".to_owned()),
                }
            } else if !const_seg.is_empty() {
                segments.push(Box::new(ConstSegment::new(const_seg)));
            } else {
                return Err("parse path error".to_owned());
            }
            if self.offset >= self.path.len() - 1 {
                break;
            }
        }
        Ok(segments)
    }
}

pub struct PathFilter {
    raw_value: String,
    segments: Vec<Box<dyn Segment>>,
}

impl Debug for PathFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ raw_value: '{}'}}", &self.raw_value)
    }
}
impl Filter for PathFilter {
    fn filter(&self, _req: &mut Request, path: &mut PathState) -> bool {
        if path.ending_matched {
            return false;
        }
        if !self.segments.is_empty() {
            let mut params = HashMap::<String, String>::new();
            let mut match_cursor = path.match_cursor;
            for ps in &self.segments {
                let (matched, detail) = ps.detect(path.segments[match_cursor..].iter().map(AsRef::as_ref).collect());
                if !matched {
                    return false;
                } else if let Some(detail) = detail {
                    if let Some(kv) = detail.matched_params {
                        params.extend(kv);
                    }
                    if let Some(segs) = detail.segments {
                        match_cursor += segs.len();
                    }
                    if detail.ending_matched {
                        path.ending_matched = true;
                        break;
                    }
                } else {
                    return false;
                }
            }
            if !params.is_empty() {
                path.params.extend(params);
            }
            path.match_cursor = match_cursor;
            true
        } else {
            false
        }
    }
}
impl PathFilter {
    pub fn new(value: impl Into<String>) -> Self {
        let raw_value = value.into();
        let mut parser = PathParser::new(&raw_value);
        let segments = match parser.parse() {
            Ok(segments) => segments,
            Err(e) => {
                panic!("{}", e);
            }
        };
        PathFilter { raw_value, segments }
    }
}

#[test]
fn test_empty() {
    let segments = PathParser::new("").parse().unwrap();
    assert!(segments.is_empty());
}
#[test]
fn test_root() {
    let segments = PathParser::new("/").parse().unwrap();
    assert!(segments.is_empty());
}

#[test]
fn test_single_const() {
    let segments = PathParser::new("/hello").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[ConstSegment("hello")]"#);
}
#[test]
fn test_multi_const() {
    let segments = PathParser::new("/hello/world").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[ConstSegment("hello"), ConstSegment("world")]"#);
}
#[test]
fn test_single_regex() {
    let segments = PathParser::new(r"/<abc:/\d+/>").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: (?P<abc>\d+), names: ["abc"] }]"#);
}
#[test]
fn test_single_regex_with_prefix() {
    let segments = PathParser::new(r"/prefix_<abc:/\d+/>").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: prefix_(?P<abc>\d+), names: ["abc"] }]"#);
}
#[test]
fn test_single_regex_with_suffix() {
    let segments = PathParser::new(r"/<abc:/\d+/>_suffix.png").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: (?P<abc>\d+)_suffix.png, names: ["abc"] }]"#);
}
#[test]
fn test_single_regex_with_prefix_and_suffix() {
    let segments = PathParser::new(r"/prefix<abc:/\d+/>suffix.png").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: prefix(?P<abc>\d+)suffix.png, names: ["abc"] }]"#);
}
#[test]
fn test_multi_regex() {
    let segments = PathParser::new(r"/first<id>/prefix<abc:/\d+/>").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: first(?P<id>[^/]+), names: ["id"] }, RegexSegment { regex: prefix(?P<abc>\d+), names: ["abc"] }]"#);
}
#[test]
fn test_multi_regex_with_prefix() {
    let segments = PathParser::new(r"/first<id>/prefix<abc:/\d+/>").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: first(?P<id>[^/]+), names: ["id"] }, RegexSegment { regex: prefix(?P<abc>\d+), names: ["abc"] }]"#);
}
#[test]
fn test_multi_regex_with_suffix() {
    let segments = PathParser::new(r"/first<id:/\d+/>/prefix<abc:/\d+/>").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: first(?P<id>\d+), names: ["id"] }, RegexSegment { regex: prefix(?P<abc>\d+), names: ["abc"] }]"#);
}
#[test]
fn test_multi_regex_with_prefix_and_suffix() {
    let segments = PathParser::new(r"/first<id>ext2/prefix<abc:/\d+/>ext").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: first(?P<id>[^/]+)ext2, names: ["id"] }, RegexSegment { regex: prefix(?P<abc>\d+)ext, names: ["abc"] }]"#);
}
#[test]
fn test_rest() {
    let segments = PathParser::new(r"/first<id>ext2/<*rest>").parse().unwrap();
    assert_eq!(format!("{:?}", segments), r#"[RegexSegment { regex: first(?P<id>[^/]+)ext2, names: ["id"] }, RestSegment("*rest")]"#);
}

#[test]
fn test_rest_failed1() {
    assert!(PathParser::new(r"/first<id>ext2<*rest>").parse().is_err());
}
#[test]
fn test_rest_failed2() {
    assert!(PathParser::new(r"/first<id>ext2/<*rest>wefwe").parse().is_err());
}

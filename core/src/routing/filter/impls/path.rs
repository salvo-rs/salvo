use std::collections::HashMap;
use std::fmt::{self, Debug};

use regex::Regex;

use crate::http::Request;
use crate::routing::{Filter, PathState};

trait Segement: Send + Sync + Debug {
    fn detect<'a>(&self, segements: Vec<&'a str>) -> (bool, Vec<&'a str>, Option<HashMap<String, String>>);
}

#[derive(Debug)]
struct RegexSegement {
    regex: Regex,
    names: Vec<String>,
}
impl RegexSegement {
    fn new(regex: Regex, names: Vec<String>) -> RegexSegement {
        RegexSegement { regex, names }
    }
}
impl Segement for RegexSegement {
    fn detect<'a>(&self, segements: Vec<&'a str>) -> (bool, Vec<&'a str>, Option<HashMap<String, String>>) {
        if segements.is_empty() {
            return (false, Vec::new(), None);
        }
        let segement = segements[0];
        let caps = self.regex.captures(segement);
        if let Some(caps) = caps {
            let mut kv = HashMap::<String, String>::new();
            for name in &self.names {
                kv.insert(name.clone(), caps[&name[..]].to_owned());
            }
            (true, vec![segement], Some(kv))
        } else {
            (false, Vec::new(), None)
        }
    }
}

#[derive(Debug)]
struct RestSegement(String);
impl RestSegement {
    fn new(name: String) -> RestSegement {
        RestSegement(name)
    }
}
impl Segement for RestSegement {
    fn detect<'a>(&self, segements: Vec<&'a str>) -> (bool, Vec<&'a str>, Option<HashMap<String, String>>) {
        if segements.is_empty() {
            return (false, Vec::new(), None);
        }
        let mut kv = HashMap::new();
        kv.insert(self.0.clone(), segements.join("/"));
        (true, Vec::new(), Some(kv))
    }
}

#[derive(Debug)]
struct ConstSegement(String);
impl ConstSegement {
    fn new(segement: String) -> ConstSegement {
        ConstSegement(segement)
    }
}
impl Segement for ConstSegement {
    fn detect<'a>(&self, segements: Vec<&'a str>) -> (bool, Vec<&'a str>, Option<HashMap<String, String>>) {
        if segements.is_empty() {
            return (false, Vec::new(), None);
        }
        if self.0 == segements[0] {
            (true, vec![segements[0]], None)
        } else {
            (false, Vec::new(), None)
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
        if !self.path.is_empty() && self.offset < self.path.len() - 1 {
            self.offset += 1;
            if skip_blank {
                self.skip_blank();
            }
            return Some(self.path[self.offset]);
        }
        None
    }
    fn peek(&self, skip_blank: bool) -> Option<char> {
        if !self.path.is_empty() && self.offset < self.path.len() - 1 {
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
    fn curr(&self) -> char {
        self.path[self.offset]
    }
    fn scan_ident(&mut self) -> Result<String, String> {
        let mut ident = "".to_owned();
        let mut ch = self.curr();
        while ch != '/' && ch != ':' && ch != '<' && ch != '>' {
            ident.push(ch);
            if let Some(c) = self.next(false) {
                ch = c;
            } else {
                break;
            }
        }
        Ok(ident)
    }
    fn scan_regex(&mut self) -> Result<String, String> {
        let mut regex = "".to_owned();
        let mut ch = self.curr();
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
        Ok(regex)
    }
    fn scan_const(&mut self) -> Result<String, String> {
        let mut cnst = "".to_owned();
        let mut ch = self.curr();
        while ch != '/' && ch != ':' && ch != '<' && ch != '>' {
            cnst.push(ch);
            if let Some(c) = self.next(false) {
                ch = c;
            } else {
                break;
            }
        }
        Ok(cnst)
    }
    fn scan_segement(&mut self) -> Result<Box<dyn Segement>, String> {
        let mut const_seg = "".to_owned();
        let mut regex_seg = "".to_owned();
        let mut regex_names = vec![];
        let mut ch = self.curr();
        if ch == '<' {
            ch = self.next(true).expect("char is needed");
            if ch == '*' {
                self.next(true);
                let rest_seg = self.scan_ident()?;
                if self.offset < self.path.len() - 1 {
                    panic!("no chars allowed after rest egment");
                }
                return Ok(Box::new(RestSegement::new(format!("*{}", rest_seg))));
            } else {
                let rname = self.scan_ident()?;
                if &rname == "" {
                    return Err("name must not equal empty string".to_owned());
                } else {
                    regex_names.push(rname.clone());
                }
                let mut rrgex = "[^/]+".to_owned();
                ch = self.curr();
                if ch == ':' {
                    let is_slash = match self.next(true) {
                        Some(c) => c == '/',
                        None => false,
                    };
                    if !is_slash {
                        return Err(format!("except '/' to start regex current char is '{}'", self.curr()));
                    }
                    self.next(false);
                    rrgex = self.scan_regex()?;
                }
                if self.curr() != '>' {
                    return Err(format!("except '>' to end regex segement, current char is '{}'", self.curr()));
                } else {
                    self.next(false);
                }
                if &const_seg != "" {
                    regex_seg.push_str(&const_seg);
                    const_seg.clear();
                }
                regex_seg.push_str(&("(?P<".to_owned() + &rname + ">" + &rrgex + ")"));
            }
        } else {
            const_seg = self.scan_const()?;
        }
        if self.offset < self.path.len() - 1 && self.curr() != '/' {
            return Err(format!("expect '/' here, but found {:?}   {:?}", self.curr(), self.offset));
        }
        if &regex_seg != "" {
            if &const_seg != "" {
                regex_seg.push_str(&const_seg);
            }
            let regex = Regex::new(&regex_seg);
            match regex {
                Ok(r) => Ok(Box::new(RegexSegement::new(r, regex_names))),
                Err(_) => Err("regex error".to_owned()),
            }
        } else if const_seg != "" {
            Ok(Box::new(ConstSegement::new(const_seg)))
        } else {
            Err("parse path error 1".to_owned())
        }
    }
    fn skip_blank(&mut self) {
        let mut ch = self.curr();
        while ch == ' ' || ch == '\t' {
            if !self.path.is_empty() && self.offset < self.path.len() - 1 {
                self.offset += 1;
                ch = self.path[self.offset];
            } else {
                break;
            }
        }
    }
    fn skip_slash(&mut self) {
        let mut ch = self.path[self.offset];
        while ch == '/' {
            if let Some(c) = self.next(false) {
                ch = c;
            } else {
                break;
            }
        }
    }
    fn parse(&mut self) -> Result<Vec<Box<dyn Segement>>, String> {
        let mut segements: Vec<Box<dyn Segement>> = vec![];
        let ch = '/';
        loop {
            if ch == '/' {
                self.skip_slash();
                if self.offset >= self.path.len() - 1 {
                    break;
                }
                segements.push(self.scan_segement()?);
            } else {
                return Err("parse path error 2".to_owned());
            }
            if self.offset >= self.path.len() - 1 {
                break;
            }
        }
        Ok(segements)
    }
}

pub struct PathFilter {
    raw_value: String,
    segements: Vec<Box<dyn Segement>>,
}

impl Debug for PathFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ raw_value: '{}'}}", &self.raw_value)
    }
}
impl Filter for PathFilter {
    fn filter(&self, _req: &mut Request, path: &mut PathState) -> bool {
        if path.segements.len() <= path.match_cursor {
            return false;
        }
        let mut params = HashMap::<String, String>::new();
        let mut match_cursor = path.match_cursor;
        if !self.segements.is_empty() {
            for ps in &self.segements {
                let (matched, segs, kv) = ps.detect(path.segements[path.match_cursor..].iter().map(AsRef::as_ref).collect());
                if !matched {
                    return false;
                } else {
                    if let Some(kv) = kv {
                        params.extend(kv);
                    }
                    match_cursor += segs.len();
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
        let segements = match parser.parse() {
            Ok(segements) => segements,
            Err(e) => {
                panic!(e);
            }
        };
        PathFilter { raw_value, segements }
    }
}

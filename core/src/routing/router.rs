use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use http::Method as HttpMethod;
use std::fmt::{self, Debug};
use crate::Handler;
use super::method::Method as RouteMethod;

pub struct Router {
	raw_path: String,
	path_segments: Vec<Box<dyn Segment>>,
	scopes: Vec<Router>,
	handlers: HashMap<HttpMethod, Vec<Arc<dyn Handler>>>,
	befores: HashMap<HttpMethod, Vec<Arc<dyn Handler>>>,
	afters: HashMap<HttpMethod, Vec<Arc<dyn Handler>>>,
}

impl Debug for Router{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ path: '{}', handlers: '{}', scopes: {:#?} }}", &self.raw_path, self.handlers.keys().map(|k|k.to_string()).collect::<Vec<String>>().join(", "), &self.scopes)
    }
}

trait Segment: Send + Sync + Debug{
	fn detect<'a>(&self, segments:Vec<&'a str>)->(bool, Vec<&'a str>, Option<HashMap<String, String>>);
}

#[derive(Debug)]
struct RegexSegment{
	regex: Regex,
	names: Vec<String>,
}
impl RegexSegment{
	fn new(regex: Regex, names: Vec<String>)->RegexSegment{
		RegexSegment{
			regex,
			names,
		}
	}
}
impl Segment for RegexSegment {
	fn detect<'a>(&self, segments:Vec<&'a str>) -> (bool, Vec<&'a str>, Option<HashMap<String, String>>){
		if segments.is_empty() {
			return (false, segments, None)
		}
		let caps = self.regex.captures(segments[0]);
		if let Some(caps) = caps {
			let mut kv = HashMap::<String, String>::new();
			for name in &self.names{
				kv.insert(name.clone(), caps[&name[..]].to_owned());
			}
			(true, segments[1..].to_vec(), Some(kv))
		}else{
			(false, segments, None)
		}
	}
}

#[derive(Debug)]
struct RestSegment(String);
impl RestSegment{
	fn new(name: String)->RestSegment{
		RestSegment(name)
	}
}
impl Segment for RestSegment {
	fn detect<'a>(&self, segments:Vec<&'a str>)->(bool, Vec<&'a str>, Option<HashMap<String, String>>){
		if segments.is_empty() {
			return (false, segments, None)
		}
		let mut kv = HashMap::new();
		kv.insert(self.0.clone(), segments.join("/"));
		(true, Vec::new(), Some(kv))
	}
}

#[derive(Debug)]
struct ConstSegment(String);
impl ConstSegment {
	fn new(segment: String)->ConstSegment{
		ConstSegment(segment)
	}
}
impl Segment for ConstSegment {
	fn detect<'a>(&self, segments:Vec<&'a str>)->(bool, Vec<&'a str>, Option<HashMap<String, String>>){
		if segments.is_empty() {
			return (false, segments, None)
		}
		let matched = self.0 == segments[0];
		if matched {
			(matched, segments[1..].to_vec(), None)
		}else{
			(matched, segments, None)
		}
	}
}

struct PathParser{
	offset: usize,
	// raw_path: String,
	path: Vec<char>,
}
impl PathParser{
	fn new(raw_path: &str)->PathParser{
		PathParser{
			offset: 0,
			// raw_path: raw_path.to_owned(),
			path: raw_path.chars().collect(),
		}
	}
	fn next(&mut self, skip_blank: bool)->Option<char>{
		if !self.path.is_empty() && self.offset < self.path.len() - 1 {
			self.offset += 1;
			if skip_blank {
				self.skip_blank();
			}
			return Some(self.path[self.offset]);
		}
		None
	}
	fn peek(&self, skip_blank: bool)->Option<char> {
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
			}else{
				Some(self.path[self.offset+1])
			}
		}else{
			None
		}
	}
	fn curr(&self)->char{
		self.path[self.offset]
	}
	fn scan_ident(&mut self)->Result<String, String>  {
		let mut ident = "".to_owned();
		let mut ch = self.curr();
		while ch != '/' && ch != ':' && ch != '<' && ch != '>' {
			ident.push(ch);
			if let Some(c) = self.next(false){
				ch = c;
			}else{
				break;
			}
		}
		Ok(ident)
	}
	fn scan_regex(&mut self)->Result<String, String>  {
		let mut regex = "".to_owned();
		let mut ch = self.curr();
		loop {
			regex.push(ch);
			if let Some(c) = self.next(false){
				ch = c;
				if ch == '/' {
					let pch = self.peek(true);
					if pch.is_none() {
						return Err("path end but regex is not ended".to_owned());
					}else if let Some('>') = pch {
						self.next(true);
						break;
					}
				}
			}else{
				break;
			}
		}
		Ok(regex)
	}
	fn scan_const(&mut self)->Result<String, String> {
		let mut cnst = "".to_owned();
		let mut ch = self.curr();
		while ch != '/' && ch != ':' && ch != '<' && ch != '>' {
			cnst.push(ch);
			if let Some(c) = self.next(false){
				ch = c;
			}else{
				break;
			}
		}
		Ok(cnst)
	}
	fn scan_segement(&mut self)->Result<Box<dyn Segment>, String> {
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
				return Ok(Box::new(RestSegment::new(format!("*{}", rest_seg))))
			}else{
				let rname = self.scan_ident()?;
				if &rname == "" {
					return Err("name must not equal empty string".to_owned());
				}else{
					regex_names.push(rname.clone());
				}
				let mut rrgex = "[^/]+".to_owned();
				ch = self.curr();
				if ch == ':' {
					let is_slash = match self.next(true){
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
		}else{
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
				Ok(r)=> Ok(Box::new(RegexSegment::new(r, regex_names))),
				Err(_)=> Err("regex error".to_owned())
			}
		} else if const_seg != "" {
			Ok(Box::new(ConstSegment::new(const_seg)))
		}else{
			Err("parse path error 1".to_owned())
		}
	}
	fn skip_blank(&mut self) {
		let mut ch = self.curr();
		while ch == ' ' || ch == '\t' {
			if !self.path.is_empty() && self.offset < self.path.len() - 1 {
				self.offset += 1;
				ch = self.path[self.offset];
			}else{
				break;
			}
		}
	}
	fn skip_slash(&mut self) {
		let mut ch = self.path[self.offset];
		while ch == '/' {
			if let Some(c) = self.next(false) {
				ch = c;
			}else{
				break;
			}
		}
	}
	fn parse(&mut self) -> Result<Vec<Box<dyn Segment>>, String> {
		let mut segments: Vec<Box<dyn Segment>> = vec![];
		let ch = '/';
		loop {
			if ch == '/' {
				self.skip_slash();
				if self.offset >= self.path.len() - 1 {
					break;
				}
				segments.push(self.scan_segement()?);
			}else{
				return Err("parse path error 2".to_owned());
			}
			if self.offset >= self.path.len() - 1 {
				break;
			}
		}
		Ok(segments)
	}
}

impl Router {
	pub fn new(path: &str) -> Router {
		let mut router = Router {
			raw_path: String::from(""),
			path_segments: Vec::new(),
			scopes: Vec::new(),
			handlers: HashMap::<HttpMethod, Vec<Arc<dyn Handler>>>::new(),
			befores: HashMap::<HttpMethod, Vec<Arc<dyn Handler>>>::new(),
			afters: HashMap::<HttpMethod, Vec<Arc<dyn Handler>>>::new(),
		};
		router.set_path(path);
		router
	}

	pub fn scope(&mut self, path: &str)->&mut Router{
		self.scopes.push(Router::new(path));
		self.scopes.last_mut().unwrap()
	}
	
	fn set_path(&mut self, path: &str) -> &mut Router {
		self.raw_path = String::from(path);
		let mut parser = PathParser::new(path);
		self.path_segments.clear();
		match parser.parse() {
			Ok(segs)=>{
				self.path_segments.extend(segs);
			}
			Err(e)=>{
				panic!(e);
			}
		}
		self
	}
	pub fn before<H: Handler>(&mut self, method: RouteMethod, handler: H) -> &mut Router {
		let methods = method.to_http_methods();
		let handler = Arc::new(handler);
		for method in methods {
			if self.befores.get(&method).is_none() {
				self.befores.insert(method.clone(), vec![]);
			}
			if let Some(ref mut handlers) = self.befores.get_mut(&method){
				handlers.push(handler.clone());
			}
		}
		self
	}
	pub fn after<H: Handler>(&mut self, method: RouteMethod, handler: H) -> &mut Router {
		let methods = method.to_http_methods();
		let handler = Arc::new(handler);
		for method in methods {
			if self.afters.get(&method).is_none() {
				self.afters.insert(method.clone(), vec![]);
			}
			if let Some(ref mut handlers) = self.afters.get_mut(&method){
				handlers.push(handler.clone());
			}
		}
		self
	}
	pub fn detect(&self, method: HttpMethod, segments: Vec<&str>) -> (bool, Vec<Arc<dyn Handler>>, HashMap<String, String>) {
		let mut params = HashMap::<String, String>::new();
		let mut befores = vec![];
		let mut afters = vec![];
		let mut i = 0;
		let mut rest = segments.clone();
		if self.path_segments.len() > 0 {
			for ps in &self.path_segments {
				let (matched, nrest, kv) = ps.detect(rest);
				if !matched {
					return (false, vec![], params);
				}else{
					if let Some(kv) = kv {
						params.extend(kv);
					}
					rest = nrest;
					for b in self.befores.get(&method).unwrap_or(&vec![]) {
						befores.push(b.clone());
					}
					for a in self.afters.get(&method).unwrap_or(&vec![]) {
						afters.push(a.clone());
					}
				}
				i += 1;
			}
		} else {
			for b in self.befores.get(&method).unwrap_or(&vec![]) {
				befores.push(b.clone());
			}
			for a in self.afters.get(&method).unwrap_or(&vec![]) {
				afters.push(a.clone());
			}
		}

		if rest.is_empty() {
			let mut allh = vec![];
			allh.extend(befores);
			let hs = self.handlers.get(&method).map(|hs|hs.iter().map(|h|h.clone()).collect::<Vec<Arc<dyn Handler>>>()).unwrap_or(vec![]);
			if hs.is_empty() {
				return (false, vec![], params)
			}
			allh.extend(hs);
			allh.extend(afters);
			return (true, allh, params);
		}
		if !rest.is_empty() && !self.scopes.is_empty() {
			for scope in &self.scopes {
				let (matched, handlers, kv) = scope.detect(method.clone(), segments[i..].to_vec());
				if matched{
					if !kv.is_empty() {
						params.extend(kv);
					}
					let mut allh = vec![];
					allh.extend(befores);
					allh.extend(handlers);
					allh.extend(afters);
					return (true, allh, params);
				}
			}
		}
		(false, vec![], params)
	}
	// pub fn reverse(name: &str, args: Option<HashMap<&str, &str>>) -> ReverseResult {
	// 	Ok("unimplement".to_string())
	// }

    pub fn route<H: Handler>(&mut self, method: RouteMethod, handler: H) -> &mut Router {
		let methods = method.to_http_methods();
		let handler = Arc::new(handler);
		for method in methods {
			if self.handlers.get(&method).is_none() {
				self.handlers.insert(method.clone(), vec![]);
			}

			if let Some(ref mut handlers) = self.handlers.get_mut(&method){
				handlers.push(handler.clone());
			}
		}
        self
    }
    /// Like route, but specialized to the `Get` method.
    pub fn get<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.route(RouteMethod::GET, handler)
    }

    /// Like route, but specialized to the `Post` method.
    pub fn post<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.route(RouteMethod::POST, handler)
    }

    /// Like route, but specialized to the `Put` method.
    pub fn put<H: Handler, I: AsRef<str>>(&mut self, handler: H) -> &mut Router {
        self.route(RouteMethod::PUT, handler)
    }

    /// Like route, but specialized to the `Delete` method.
    pub fn delete<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.route(RouteMethod::DELETE, handler)
    }

    /// Like route, but specialized to the `Head` method.
    pub fn head<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.route(RouteMethod::HEAD, handler)
    }

    /// Like route, but specialized to the `Patch` method.
    pub fn patch<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.route(RouteMethod::PATCH, handler)
    }

    /// Like route, but specialized to the `Options` method.
    pub fn options<H: Handler>(&mut self, handler: H) -> &mut Router {
        self.route(RouteMethod::OPTIONS, handler)
	}
}
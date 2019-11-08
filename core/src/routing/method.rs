use http::Method as HttpMethod;

bitflags! {
	pub struct Method: u16 {
		const GET = 	0b0000_0000_0000_0001;
        const POST = 	0b0000_0000_0000_0010;
        const PUT = 	0b0000_0000_0000_0100;
        const DELETE = 	0b0000_0000_0000_1000;
        const HEAD = 	0b0000_0000_0001_0000;
        const OPTIONS = 0b0000_0000_0010_0000;
        const CONNECT = 0b0000_0000_0100_0000;
        const PATCH = 	0b0000_0000_1000_0000;
        const TRACE = 	0b0000_0001_0000_0000;
        const ALL = Self::GET.bits | Self::POST.bits | Self::PUT.bits | 
			Self::DELETE.bits | Self::HEAD.bits | Self::OPTIONS.bits | 
			Self::CONNECT.bits | Self::PATCH.bits | Self::TRACE.bits;
	}
}
impl Method {
	pub fn from_http_method(m: &HttpMethod) -> Option<Method> {
		match *m {
			HttpMethod::GET => 		Some(Method::GET),
			HttpMethod::POST => 		Some(Method::POST),
			HttpMethod::PUT => 		Some(Method::PUT),
			HttpMethod::DELETE => 	Some(Method::DELETE),
			HttpMethod::HEAD => 		Some(Method::HEAD),
			HttpMethod::OPTIONS => 	Some(Method::OPTIONS),
			HttpMethod::CONNECT => 	Some(Method::CONNECT),
			HttpMethod::PATCH => 	Some(Method::PATCH),
			HttpMethod::TRACE => 	Some(Method::TRACE),
			_ => None,
		}
	}
    pub fn to_http_methods(&self) -> Vec<HttpMethod> {
        let mut list = vec![];
        if self.contains(Method::GET) {
            list.push(HttpMethod::GET);
        }
        if self.contains(Method::POST) {
            list.push(HttpMethod::POST);
        }
        if self.contains(Method::PUT) {
            list.push(HttpMethod::PUT);
        }
        if self.contains(Method::DELETE) {
            list.push(HttpMethod::DELETE);
        }
        if self.contains(Method::HEAD) {
            list.push(HttpMethod::HEAD);
        }
        if self.contains(Method::OPTIONS) {
            list.push(HttpMethod::OPTIONS);
        }
        if self.contains(Method::CONNECT) {
            list.push(HttpMethod::CONNECT);
        }
        if self.contains(Method::PATCH) {
            list.push(HttpMethod::PATCH);
        }
        if self.contains(Method::TRACE) {
            list.push(HttpMethod::TRACE);
        }
        list
    }
}
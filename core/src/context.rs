// #[macro_use]
// extern crate lazy_static;

use std::collections::HashMap;
use std::borrow::Cow;
use crate::error::Error;
use crate::http::form::Error as FormError;
use std::str::FromStr;
use crate::depot::Depot;
use crate::http::{headers, Request, form::FormData, Response, BodyWriter, StatusCode};
use cookie::{Cookie, CookieJar};
use serde::{Serialize, Deserialize};
use serde_json;
use serde_urlencoded;
use serde::de::DeserializeOwned;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::io::prelude::*;
use mime::Mime;
use multimap::MultiMap;
use super::server::ServerConfig;
use super::Content;
use crate::logging;
use crate::http::errors::HttpError;

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorInfo {
    name: String,
    summary: String,
    detail: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorWrap {
    error: ErrorInfo,
}

impl ErrorWrap{
    pub fn new<N, S, D>(name:N, summary: S, detail: D) -> ErrorWrap where N: Into<String>, S: Into<String>, D: Into<String> {
        ErrorWrap {
            error: ErrorInfo {
                name: name.into(),
                summary: summary.into(),
                detail: detail.into(),
            },
        }
    }
}

pub struct Context{
    pub(crate) request: Request,
    pub(crate) response: Response,
    pub(crate) params: HashMap<String, String>,
    pub(crate) server_config: Arc<ServerConfig>,
    pub(crate) depot: Depot,
    pub(crate) cookies: CookieJar,
    is_commited: bool,
}

impl Context{
    pub fn new(server_config: Arc<ServerConfig>, request:Request, response: Response)->Context{
        Context{
            params: HashMap::new(),
            server_config,
            request,
            response,
            depot: Depot::new(),
            is_commited: false,
            cookies: CookieJar::new(),
        }
    }
    #[inline]
    pub fn request(&self)->&Request{
        &self.request
    }
    #[inline]
    pub fn response(&mut self)->&Response{
        &self.response
    }
    #[inline]
    pub fn response_mut(&mut self)->&mut Response{
        &mut self.response
    }
    #[inline]
    pub fn server_config(&mut self)-> Arc<ServerConfig>{
        self.server_config.clone()
    }

    #[inline]
    pub fn params(&self)->&HashMap<String, String>{
        &self.params
    }
    #[inline]
    pub fn get_param<'a, F: FromStr>(&self, key: impl AsRef<str>) -> Option<F> {
        self.params().get(key.as_ref()).and_then(|v|v.parse::<F>().ok())
    }
    
    #[inline]
    pub fn queries(&self)->&MultiMap<String, String>{
        self.request.queries()
    }
    #[inline]
    pub fn get_query<'a, F: FromStr>(&self, key: impl AsRef<str>) -> Option<F> {
        self.queries().get(key.as_ref()).and_then(|v|v.parse::<F>().ok())
    }
    #[inline]
    pub fn form_data(&self) -> &Result<FormData, FormError> {
        self.request.form_data()
    }
    #[inline]
    pub fn get_form<T: FromStr>(&self, key: impl AsRef<str>) -> Option<T> {
        self.request.form_data().as_ref().ok().and_then(|ps|ps.fields.get(key.as_ref())).and_then(|v|v.parse::<T>().ok())
    }
    pub fn get_form_or_query<T: FromStr>(&self, key: impl AsRef<str>) -> Option<T> {
        self.get_form(key.as_ref()).or(self.get_query(key.as_ref()))
    }
    pub fn get_query_or_form<T: FromStr>(&self, key: impl AsRef<str>) -> Option<T> {
        self.get_query(key.as_ref()).or(self.get_form(key.as_ref()))
    }
    pub fn get_payload(&self) -> Result<String, Error> {
        if let Ok(data) = self.request.body_data().as_ref(){
            match String::from_utf8(data.to_vec()){
                Ok(payload) => {
                    Ok(payload)},
                Err(err) => Err(Error::Utf8(err)),
            }
        } else {
            Err(Error::General("utf8 encode error".to_owned()))
        }
    }

    #[inline]
    pub fn get_cookie<T>(&self, name:T) -> Option<&Cookie<'static>>
        where T: AsRef<str> {
         self.request.cookies().get(name.as_ref())
    }
    #[inline]
    pub fn add_cookie(&mut self, cookie: Cookie<'static>) {
        self.cookies.add(cookie);
    }
    #[inline]
    pub fn remove_cookie<T>(&mut self, name: T) where T: Into<Cow<'static, str>> {
        self.cookies.remove(Cookie::named(name));
    }
    #[inline]
    pub fn depot(&self)->&Depot {
        &self.depot
    }
    #[inline]
    pub fn depot_mut(&mut self)->&mut Depot {
        &mut self.depot
    }
    #[inline]
    pub fn set_status_code(&mut self, code: StatusCode) {
        self.response.status = Some(code);
    }

    #[inline]
    pub fn read_from_json<T>(&mut self) -> Result<T, Error> where T: DeserializeOwned {
        self.get_payload().and_then(|body|serde_json::from_str::<T>(&body).map_err(|_|Error::General(String::from("parse body error"))))
    }
    #[inline]
    pub fn read_from_form<T>(&mut self) -> Result<T, Error> where T: DeserializeOwned {
        self.get_payload().and_then(|body|serde_urlencoded::from_str::<T>(&body).map_err(|_|Error::General(String::from("parse body error"))))
    }
    #[inline]
    pub fn read<T>(&mut self) -> Result<T, Error> where T: DeserializeOwned  {
        match self.request.headers().get(headers::CONTENT_TYPE) {
            Some(ctype) if ctype == "application/x-www-form-urlencoded" => self.read_from_json(),
            Some(ctype) if ctype == "application/json" => self.read_from_form(),
            _=> Err(Error::General(String::from("failed to read data")))
        }
    }

    #[inline]
    pub fn write_error(&mut self, err: impl HttpError){
        self.response.status = Some(err.code());
        self.commit();
    }
    #[inline]
    pub fn write_content(&mut self, content: impl Content){
        content.apply(self);
    }
    #[inline]
    pub fn write_body(&mut self, writer: impl BodyWriter+'static) {
        self.response.body_writers.push(Box::new(writer))
    }

    #[inline]
    pub fn render_cbor<'a, T: Serialize>(&mut self, writer: &'a T) {
        if let Ok(data) = serde_cbor::to_vec(writer) {
            self.render("application/cbor", data);
        } else {
            self.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            let emsg = ErrorWrap::new("server_error", "server error", "error when serialize object to cbor");
            self.render("application/cbor", serde_cbor::to_vec(&emsg).unwrap());
        }
    }
    #[inline]
    pub fn render_json<'a, T: Serialize>(&mut self, writer: &'a T) {
        if let Ok(data) = serde_json::to_string(writer) {
            self.render("application/json", data);
        } else {
            self.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
            let emsg = ErrorWrap::new("server_error", "server error", "error when serialize object to json");
            self.render("application/json", serde_json::to_string(&emsg).unwrap());
        }
    }
    pub fn render_json_text<T: Into<String>>(&mut self, writer: T) {
        self.render("application/json", writer.into());
    }
    #[inline]
    pub fn render_html_text<T: Into<String>>(&mut self, writer: T) {
        self.render("text/html", writer.into());
    }
    #[inline]
    pub fn render_plain_text<T: Into<String>>(&mut self, writer: T) {
        self.render("text/plain", writer.into());
    }
    #[inline]
    pub fn render_xml_text<T: Into<String>>(&mut self, writer: T) {
        self.render("text/xml", writer.into());
    }
    // RenderBinary is like RenderFile() except that it instead of a file on disk,
    // it renders store from memory (which could be a file that has not been written,
    // the output from some function, or bytes streamed from somewhere else, as long
    // it implements io.Reader).  When called directly on something generated or
    // streamed, modtime should mostly likely be time.Now().
    #[inline]
    pub fn render_binary<T>(&mut self, content_type:T, data: Vec<u8>) where T: AsRef<str> {
        self.render(content_type, data);
    }
    #[inline]
    pub fn render_file<T>(&mut self, content_type:T, file: &mut File) -> std::io::Result<()> where T: AsRef<str> {
        let mut data = Vec::new();  
        file.read_to_end(&mut data)?;
        self.render_binary(content_type, data);
        Ok(())
    }
    #[inline]
    pub fn render_file_from_path<T>(&mut self, path: T) -> std::io::Result<()> where T: AsRef<Path> {
        let mut file = File::open(path.as_ref())?;
        if let Some(mime) = self.get_mime_by_path(path.as_ref().to_str().unwrap_or("")) {
            self.render_file(mime.to_string(), &mut file)
        }else{
            self.unsupported_media_type();
            error!(logging::logger(), "error on render file from path"; "path" => path.as_ref().to_str());
            Ok(())
        }
    }
    #[inline]
    pub fn render<T>(&mut self, content_type:T, writer: impl BodyWriter+'static) where T: AsRef<str> {
        self.response.headers.insert(headers::CONTENT_TYPE, content_type.as_ref().parse().unwrap());
        self.write_body(writer);
    }
    
    #[inline]
    pub fn send_binary<T>(&mut self, data: Vec<u8>, file_name: T) where T: AsRef<str> {
        let file_name = Path::new(file_name.as_ref()).file_name().and_then(|s|s.to_str()).unwrap_or("file.dat");
        if let Some(mime) = self.get_mime_by_path(file_name) {
            self.response.headers.insert(headers::CONTENT_DISPOSITION, format!("attachment; filename={}", &file_name).parse().unwrap());
            self.render(mime.to_string(), data);
        }else{
            self.unsupported_media_type();
            error!(logging::logger(), "error on send binary"; "file_name" => AsRef::<str>::as_ref(&file_name));
        }
    }
    #[inline]
    pub fn send_file<T>(&mut self, file: &mut File, file_name: T) -> std::io::Result<()> where T: AsRef<str> {
        let mut data = Vec::new();  
        file.read_to_end(&mut data)?;
        self.send_binary(data, file_name.as_ref());
        Ok(())
    }
    #[inline]
    pub fn send_file_from_path<T>(&mut self, path: T, file_name: Option<T>) -> std::io::Result<()> where T: AsRef<str> {
        let mut file = File::open(path.as_ref())?;
        self.send_file(&mut file, file_name.unwrap_or(path))
    }
    
    fn get_mime_by_path<T>(&self, path:T) -> Option<Mime> where T:AsRef<str> {
        let guess = mime_guess::from_path(path.as_ref());
        if let Some(mime) = guess.first() {
            for m in &*self.server_config.allowed_media_types {
                if m.type_() == mime.type_() && m.subtype() == mime.subtype() {
                    return Some(mime);
                }
            }
        }
        None
    }
    #[inline]
    pub fn redirect_temporary<U: AsRef<str>>(&mut self, url: U) {
        self.response.status = Some(StatusCode::MOVED_PERMANENTLY);
        self.response.headers.insert(headers::LOCATION, url.as_ref().parse().unwrap());
    }
    #[inline]
    pub fn redirect_found<U: AsRef<str>>(&mut self, url: U) {
        self.response.status = Some(StatusCode::FOUND);
        self.response.headers.insert(headers::LOCATION, url.as_ref().parse().unwrap());
    }
    #[inline]
    pub fn redirect_other<U: AsRef<str>>(&mut self, url: U) {
        self.response.status = Some(StatusCode::SEE_OTHER);
        self.response.headers.insert(headers::LOCATION, url.as_ref().parse().unwrap());
    }
    #[inline]
    pub fn commit(&mut self) {
        self.is_commited = true;
    }
    #[inline]
    pub fn is_commited(&self) -> bool{
        self.is_commited
    }

    #[inline]
    pub fn not_found(&mut self) {
        self.response.status = Some(StatusCode::NOT_FOUND);
        self.commit();
    }

    #[inline]
    pub fn unauthorized(&mut self) {
        self.response.status = Some(StatusCode::UNAUTHORIZED);
        self.commit();
    }

    #[inline]
    pub fn forbidden(&mut self) {
        self.response.status = Some(StatusCode::FORBIDDEN);
        self.commit();
    }
    #[inline]
    pub fn unsupported_media_type(&mut self) {
        self.response.status = Some(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        self.commit();
    }
}
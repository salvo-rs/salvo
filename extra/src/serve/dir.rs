use async_trait::async_trait;
use chrono::prelude::*;
use mime;
use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use salvo_core::Depot;
use salvo_core::http::errors::*;
use salvo_core::http::{Request, Response};
use salvo_core::fs::NamedFile;
use salvo_core::Writer;
use salvo_core::Handler;

#[derive(Debug, Clone)]
pub struct Options {
    pub dot_files: bool,
    pub listing: bool,
    pub defaults: Vec<String>,
}

impl Options {
    fn new() -> Options {
        Options {
            dot_files: true,
            listing: true,
            defaults: vec!["index.html".to_owned()],
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Options::new()
    }
}

pub trait StaticRoots {
    fn collect(&self) -> Vec<PathBuf>;
}

impl<'a> StaticRoots for &'a str {
    fn collect(&self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl<'a> StaticRoots for Vec<&'a str> {
    fn collect(&self) -> Vec<PathBuf> {
        self.iter().map(PathBuf::from).collect()
    }
}
impl<'a> StaticRoots for &'a String {
    fn collect(&self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl<'a> StaticRoots for Vec<&'a String> {
    fn collect(&self) -> Vec<PathBuf> {
        self.iter().map(PathBuf::from).collect()
    }
}
impl<'a> StaticRoots for String {
    fn collect(&self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl<'a> StaticRoots for Vec<String> {
    fn collect(&self) -> Vec<PathBuf> {
        self.iter().map(PathBuf::from).collect()
    }
}
impl StaticRoots for Path {
    fn collect(&self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}

#[derive(Clone)]
pub struct StaticDir {
    roots: Vec<PathBuf>,
    options: Options,
}
impl StaticDir {
    pub fn new<T: StaticRoots + Sized>(roots: T) -> Self {
        StaticDir::width_options(roots, Options::default())
    }
    pub fn width_options<T: StaticRoots + Sized>(roots: T, options: Options) -> Self {
        StaticDir {
            roots: roots.collect(),
            options,
        }
    }
}

fn list_json(root: &BaseInfo) -> String {
    json!(root).to_string()
}
fn list_xml(root: &BaseInfo) -> String {
    let mut ftxt = "<list>".to_owned();
    if root.dirs.is_empty() && root.files.is_empty() {
        ftxt.push_str("No files");
    } else {
        ftxt.push_str("<table>");
        for dir in &root.dirs {
            ftxt.push_str(&format!(
                "<dir><name>{}</name><modified>{}</modified><link>{}</link></dir>",
                dir.name,
                dir.modified.format("%Y-%m-%d %H:%M:%S"),
                encode_url_path(&dir.name),
            ));
        }
        for file in &root.files {
            ftxt.push_str(&format!(
                "<file><name>{}</name><modified>{}</modified><size>{}</size><link>{}</link></file>",
                file.name,
                file.modified.format("%Y-%m-%d %H:%M:%S"),
                file.size,
                encode_url_path(&file.name),
            ));
        }
        ftxt.push_str("</table>");
    }
    ftxt.push_str("</list>");
    ftxt
}
fn list_html(root: &BaseInfo) -> String {
    let mut ftxt = format!(
        "<!DOCTYPE html>
<html>
    <head>
        <meta charset=\"utf-8\">
        <title>{}</title>
        <style>
        :root {{
            --bg-color: #fff;
            --text-color: #222;
        }}
        body {{
            background: var(--bg-color);
            color: var(--text-color);
        }}
        @media (prefers-color-scheme: dark) {{
            :root {{
                --bg-color: #222;
                --text-color: #ddd;
            }}
        }}
        </style>
    </head>
    <body>
        <h1>Index of: {}</h1>
        <hr/>
        <a href=\"../\">[../]</a><br><br>
",
        root.path, root.path
    );
    if root.dirs.is_empty() && root.files.is_empty() {
        ftxt.push_str("No files");
    } else {
        ftxt.push_str("<table>");
        for dir in &root.dirs {
            ftxt.push_str(&format!(
                "<tr><td><a href=\"./{}/\">{}/</a></td><td>{}</td><td></td></tr>",
                encode_url_path(&dir.name),
                dir.name,
                dir.modified.format("%Y-%m-%d %H:%M:%S")
            ));
        }
        for file in &root.files {
            ftxt.push_str(&format!(
                "<tr><td><a href=\"./{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
                encode_url_path(&file.name),
                file.name,
                file.modified.format("%Y-%m-%d %H:%M:%S"),
                file.size
            ));
        }
        ftxt.push_str("</table>");
    }
    ftxt.push_str("<hr/><div style=\"text-align:center;\"><small>salvo</small></div></body>");
    ftxt
}
fn list_text(root: &BaseInfo) -> String {
    json!(root).to_string()
}
#[derive(Serialize, Deserialize, Debug)]
struct BaseInfo {
    path: String,
    files: Vec<FileInfo>,
    dirs: Vec<DirInfo>,
}
impl BaseInfo {
    fn new(path: String, files: Vec<FileInfo>, dirs: Vec<DirInfo>) -> BaseInfo {
        BaseInfo { path, files, dirs }
    }
}
#[derive(Serialize, Deserialize, Debug)]
struct FileInfo {
    name: String,
    size: u64,
    modified: DateTime<Local>,
}
impl FileInfo {
    fn new(name: String, metadata: Metadata) -> FileInfo {
        FileInfo {
            name,
            size: metadata.len(),
            modified: metadata.modified().unwrap_or_else(|_| SystemTime::now()).into(),
        }
    }
}
#[derive(Serialize, Deserialize, Debug)]
struct DirInfo {
    name: String,
    modified: DateTime<Local>,
}
impl DirInfo {
    fn new(name: String, metadata: Metadata) -> DirInfo {
        DirInfo {
            name,
            modified: metadata.modified().unwrap_or_else(|_| SystemTime::now()).into(),
        }
    }
}

#[async_trait]
impl Handler for StaticDir {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
        let req_path = req.uri().path();
        let mut base_path = if let Some((_, value)) = param { value } else { req_path }.to_owned();
        if base_path.starts_with('/') || base_path.starts_with('\\') {
            base_path = format!(".{}", base_path);
        }
        let base_path = decode_url_path_safely(&base_path);
        let mut files: HashMap<String, Metadata> = HashMap::new();
        let mut dirs: HashMap<String, Metadata> = HashMap::new();
        let mut path_exist = false;
        for root in &self.roots {
            let path = root.join(&base_path);
            if path.is_dir() && self.options.listing {
                path_exist = true;
                if !req_path.ends_with('/') {
                    res.redirect_found(format!("{}/", req_path));
                    return;
                }
                for ifile in &self.options.defaults {
                    let ipath = path.join(ifile);
                    if ipath.exists() {
                        if let Ok(named_file) = NamedFile::open(ipath) {
                            named_file.write(req, depot, res).await;
                        } else {
                            res.set_http_error(InternalServerError().with_summary("file read error"));
                        }
                        return;
                    }
                }
                //list the dir
                if let Ok(entries) = fs::read_dir(&path) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            if let Ok(metadata) = entry.metadata() {
                                if metadata.is_dir() {
                                    dirs.entry(entry.file_name().into_string().unwrap_or_else(|_| "".to_owned()))
                                        .or_insert(metadata);
                                } else {
                                    files
                                        .entry(entry.file_name().into_string().unwrap_or_else(|_| "".to_owned()))
                                        .or_insert(metadata);
                                }
                            }
                        }
                    }
                }
            } else if path.is_file() {
                if let Ok(named_file) = NamedFile::open(path) {
                    named_file.write(req, depot, res).await;
                } else {
                    res.set_http_error(InternalServerError().with_summary("file read error"));
                }
                return;
            }
        }
        if !path_exist || !self.options.listing {
            res.set_http_error(NotFound());
            return;
        }
        let mut format = req.frist_accept().unwrap_or(mime::TEXT_HTML);
        if format.type_() != "text" {
            format = mime::TEXT_HTML;
        }
        let mut files: Vec<FileInfo> = files.into_iter().map(|(name, metadata)| FileInfo::new(name, metadata)).collect();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        let mut dirs: Vec<DirInfo> = dirs.into_iter().map(|(name, metadata)| DirInfo::new(name, metadata)).collect();
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        let root = BaseInfo::new(req_path.to_owned(), files, dirs);
        match format.subtype().as_ref() {
            "text" => res.render_plain_text(&list_text(&root)),
            "json" => res.render_json_text(&list_json(&root)),
            "xml" => res.render_xml_text(&list_xml(&root)),
            _ => res.render_html_text(&list_html(&root)),
        }
    }
}

fn decode_url_path_safely(raw: &str) -> String {
    raw.split('/')
        .map(|s| percent_decode_str(s).decode_utf8_lossy())
        .filter(|s| !s.contains('/'))
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_url_path(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

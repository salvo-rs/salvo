use async_trait::async_trait;
use chrono::prelude::*;
use mime;
use percent_encoding::{utf8_percent_encode, CONTROLS};
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use salvo_core::fs::NamedFile;
use salvo_core::http::errors::*;
use salvo_core::http::{Request, Response};
use salvo_core::Depot;
use salvo_core::Handler;
use salvo_core::Writer;

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
        let base_path = if let Some((_, value)) = param {
            value.clone()
        } else {
            decode_url_path_safely(req_path)
        }
        .to_owned();
        let base_path = if base_path.starts_with('/') {
            format!(".{}", base_path)
        } else {
            base_path
        };
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
        let root = BaseInfo::new(decode_url_path_safely(req_path), files, dirs);
        match format.subtype().as_ref() {
            "text" => res.render_plain_text(&list_text(&root)),
            "json" => res.render_json_text(&list_json(&root)),
            "xml" => res.render_xml_text(&list_xml(&root)),
            _ => res.render_html_text(&list_html(&root)),
        }
    }
}

fn encode_url_path(path: &str) -> String {
    path.split('/')
        .map(|s| utf8_percent_encode(s, CONTROLS).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn decode_url_path_safely(path: &str) -> String {
    format!("/{}", decode_url_path_segments_safely(path).join("/"))
}

fn decode_url_path_segments_safely(path: &str) -> Vec<String> {
    let segments = path.trim_start_matches('/').split('/');
    segments
        .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string())
        .filter(|s| !s.contains('/') && !s.is_empty())
        .collect::<Vec<_>>()
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
            --dir-icon-color: #79b8ff;
            --file-icon-color: #959da5;
        }}
        body {{
            background: var(--bg-color);
            color: var(--text-color);
            text-align: center;
        }}
        table{{text-align:left;}}
        th, td {{
            padding: 2px 4px;
        }}
        th:first-child,td:first-child{{
            text-align: center;
        }}
        .dir-icon {{
            color: var(--dir-icon-color);
            fill: currentColor;
        }}
        .file-icon {{
            color: var(--file-icon-color);
            fill: currentColor;
        }}
        @media (prefers-color-scheme: dark) {{
            :root {{
                --bg-color: #222;
                --text-color: #ddd;
                --dir-icon-color: #7da3d0;
                --file-icon-color: #545d68;
            }}
        }}
        </style>
    </head>
    <body>
        <header><h3>Index of: {}</h3></header>
        <hr/>
",
        root.path, root.path
    );
    if root.dirs.is_empty() && root.files.is_empty() {
        ftxt.push_str("No files");
    } else {
        ftxt.push_str("<table><tr><th>");
        if !(root.path.is_empty() || root.path == "/") {
            ftxt.push_str("<a href=\"../\">[..]</a>");
        }
        ftxt.push_str("</th><th>Name</th><th>Last modified</th><th>Size</th></tr>");
        for dir in &root.dirs {
            ftxt.push_str(&format!(
                "<tr><td>{}</td><td><a href=\"./{}/\">{}</a></td><td>{}</td><td></td></tr>",
                DIR_ICON,
                encode_url_path(&dir.name),
                dir.name,
                dir.modified.format("%Y-%m-%d %H:%M:%S")
            ));
        }
        for file in &root.files {
            ftxt.push_str(&format!(
                "<tr><td>{}</td><td><a href=\"./{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
                FILE_ICON,
                encode_url_path(&file.name),
                file.name,
                file.modified.format("%Y-%m-%d %H:%M:%S"),
                file.size
            ));
        }
        ftxt.push_str("</table>");
    }
    ftxt.push_str("<hr/><footer><small>salvo</small></footer></body>");
    ftxt
}
fn list_text(root: &BaseInfo) -> String {
    json!(root).to_string()
}

const DIR_ICON: &str = r#"<svg aria-label="Directory" class="dir-icon" height="16" viewBox="0 0 16 16" version="1.1" width="16" role="img"><path fill-rule="evenodd" d="M1.75 1A1.75 1.75 0 000 2.75v10.5C0 14.216.784 15 1.75 15h12.5A1.75 1.75 0 0016 13.25v-8.5A1.75 1.75 0 0014.25 3h-6.5a.25.25 0 01-.2-.1l-.9-1.2c-.33-.44-.85-.7-1.4-.7h-3.5z"></path></svg>"#;
const FILE_ICON: &str = r#"<svg aria-label="File" class="file-icon" height="16" viewBox="0 0 16 16" version="1.1" width="16" role="img"><path fill-rule="evenodd" d="M3.75 1.5a.25.25 0 00-.25.25v11.5c0 .138.112.25.25.25h8.5a.25.25 0 00.25-.25V6H9.75A1.75 1.75 0 018 4.25V1.5H3.75zm5.75.56v2.19c0 .138.112.25.25.25h2.19L9.5 2.06zM2 1.75C2 .784 2.784 0 3.75 0h5.086c.464 0 .909.184 1.237.513l3.414 3.414c.329.328.513.773.513 1.237v8.086A1.75 1.75 0 0112.25 15h-8.5A1.75 1.75 0 012 13.25V1.75z"></path></svg>"#;
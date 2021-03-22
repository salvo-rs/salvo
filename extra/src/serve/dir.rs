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
struct CurrentInfo {
    path: String,
    files: Vec<FileInfo>,
    dirs: Vec<DirInfo>,
}
impl CurrentInfo {
    fn new(path: String, files: Vec<FileInfo>, dirs: Vec<DirInfo>) -> CurrentInfo {
        CurrentInfo { path, files, dirs }
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
        let root = CurrentInfo::new(decode_url_path_safely(req_path), files, dirs);
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

fn list_json(current: &CurrentInfo) -> String {
    json!(current).to_string()
}
fn list_xml(current: &CurrentInfo) -> String {
    let mut ftxt = "<list>".to_owned();
    if current.dirs.is_empty() && current.files.is_empty() {
        ftxt.push_str("No files");
    } else {
        ftxt.push_str("<table>");
        for dir in &current.dirs {
            ftxt.push_str(&format!(
                "<dir><name>{}</name><modified>{}</modified><link>{}</link></dir>",
                dir.name,
                dir.modified.format("%Y-%m-%d %H:%M:%S"),
                encode_url_path(&dir.name),
            ));
        }
        for file in &current.files {
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
fn list_html(current: &CurrentInfo) -> String {
    fn header_links(path: &str) -> String {
        let segments = path.trim_start_matches("/").trim_end_matches("/").split('/');
        let mut link = "".to_string();
        format!("{}{}", "<a href=\"/\"><svg aria-hidden=\"true\" data-icon=\"home\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 576 512\"><path fill=\"currentColor\" d=\"M280.37 148.26L96 300.11V464a16 16 0 0 0 16 16l112.06-.29a16 16 0 0 0 15.92-16V368a16 16 0 0 1 16-16h64a16 16 0 0 1 16 16v95.64a16 16 0 0 0 16 16.05L464 480a16 16 0 0 0 16-16V300L295.67 148.26a12.19 12.19 0 0 0-15.3 0zM571.6 251.47L488 182.56V44.05a12 12 0 0 0-12-12h-56a12 12 0 0 0-12 12v72.61L318.47 43a48 48 0 0 0-61 0L4.34 251.47a12 12 0 0 0-1.6 16.9l25.5 31A12 12 0 0 0 45.15 301l235.22-193.74a12.19 12.19 0 0 1 15.3 0L530.9 301a12 12 0 0 0 16.9-1.6l25.5-31a12 12 0 0 0-1.7-16.93z\"></path></svg></a>", segments
        .map(|seg| {
            link = format!("{}/{}", link, seg);
            format!("/<a href=\"{}\">{}</a>", link, seg)
        }).collect::<Vec<_>>()
        .join(""))
    }
    let mut ftxt = format!(
        r#"<!DOCTYPE html>
<html>
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width">
        <title>{}</title>
        <style>
        :root {{
            --bg-color: #fff;
            --text-color: #222;
            --link-color: #0366d6;
            --link-visited-color: #f22526;
            --dir-icon-color: #79b8ff;
            --file-icon-color: #959da5;
        }}
        body {{background: var(--bg-color); color: var(--text-color);}}
        a{{text-decoration:none;color:var(--link-color);}}
        a:visited {{color: var(--link-visited-color);}}
        a:hover {{text-decoration:underline;}}
        footer {{text-align:center;font-size:12px;}}
        table {{text-align:left;border-collapse: collapse;}}
        tr {{border-bottom: solid 1px #ccc;}}
        tr:last-child {{border-bottom: none;}}
        th, td {{padding: 5px;}}
        th:first-child,td:first-child {{text-align: center;}}
        svg[data-icon="dir"] {{vertical-align: text-bottom; color: var(--dir-icon-color); fill: currentColor;}}
        svg[data-icon="file"] {{vertical-align: text-bottom; color: var(--file-icon-color); fill: currentColor;}}
        svg[data-icon="home"] {{width:24px;vertical-align: bottom;}}
        @media (prefers-color-scheme: dark) {{
            :root {{
                --bg-color: #222;
                --text-color: #ddd;
                --link-color: #539bf5;
                --link-visited-color: #f25555;
                --dir-icon-color: #7da3d0;
                --file-icon-color: #545d68;
            }}
        }}
        </style>
    </head>
    <body>
        <header><h3>Index of: {}</h3></header>
        <hr/>
"#,
        current.path,
        header_links(&current.path)
    );
    if current.dirs.is_empty() && current.files.is_empty() {
        ftxt.push_str("<p>No files</p>");
    } else {
        ftxt.push_str("<table><tr><th>");
        if !(current.path.is_empty() || current.path == "/") {
            ftxt.push_str("<a href=\"../\">[..]</a>");
        }
        ftxt.push_str("</th><th>Name</th><th>Last modified</th><th>Size</th></tr>");
        for dir in &current.dirs {
            ftxt.push_str(&format!(
                r#"<tr><td>{}</td><td><a href="./{}/">{}</a></td><td>{}</td><td></td></tr>"#,
                DIR_ICON,
                encode_url_path(&dir.name),
                dir.name,
                dir.modified.format("%Y-%m-%d %H:%M:%S")
            ));
        }
        for file in &current.files {
            ftxt.push_str(&format!(
                r#"<tr><td>{}</td><td><a href="./{}">{}</a></td><td>{}</td><td>{}</td></tr>"#,
                FILE_ICON,
                encode_url_path(&file.name),
                file.name,
                file.modified.format("%Y-%m-%d %H:%M:%S"),
                file.size
            ));
        }
        ftxt.push_str("</table>");
    }
    ftxt.push_str(r#"<hr/><footer><a href="https://salvo.rs" target="_blank">salvo</a></footer></body>"#);
    ftxt
}
fn list_text(current: &CurrentInfo) -> String {
    json!(current).to_string()
}

const DIR_ICON: &str = r#"<svg aria-label="Directory" data-icon="dir" width="20" height="20" viewBox="0 0 512 512" version="1.1" role="img"><path fill="currentColor" d="M464 128H272l-64-64H48C21.49 64 0 85.49 0 112v288c0 26.51 21.49 48 48 48h416c26.51 0 48-21.49 48-48V176c0-26.51-21.49-48-48-48z"></path></svg>"#;
const FILE_ICON: &str = r#"<svg aria-label="File" data-icon="file" width="20" height="20" viewBox="0 0 384 512" version="1.1" role="img"><path d="M369.9 97.9L286 14C277 5 264.8-.1 252.1-.1H48C21.5 0 0 21.5 0 48v416c0 26.5 21.5 48 48 48h288c26.5 0 48-21.5 48-48V131.9c0-12.7-5.1-25-14.1-34zM332.1 128H256V51.9l76.1 76.1zM48 464V48h160v104c0 13.3 10.7 24 24 24h104v288H48z"/></svg>"#;

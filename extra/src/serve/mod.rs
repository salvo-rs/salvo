use async_trait::async_trait;
use chrono::prelude::*;
use mime;
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use salvo_core::depot::Depot;
use salvo_core::http::errors::*;
use salvo_core::http::{Request, Response};
use salvo_core::server::ServerConfig;
use salvo_core::writer::{NamedFile, Writer};
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

#[derive(Clone)]
pub struct Static {
    roots: Vec<PathBuf>,
    options: Options,
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
        self.iter().map(|i| PathBuf::from(i)).collect()
    }
}
impl StaticRoots for Path {
    fn collect(&self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}

impl Static {
    pub fn from<T: StaticRoots + Sized>(roots: T) -> Self {
        Static::new(roots, Options::default())
    }

    pub fn new<T: StaticRoots + Sized>(roots: T, options: Options) -> Self {
        Static {
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
                "<dir><name>{}</name><modified>{}</modified></dir>",
                dir.name,
                dir.modified.format("%Y-%m-%d %H:%M:%S")
            ));
        }
        for file in &root.files {
            ftxt.push_str(&format!(
                "<file><name>{}</name><modified>{}</modified><size>{}</size></file>",
                file.name,
                file.modified.format("%Y-%m-%d %H:%M:%S"),
                file.size
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
                dir.name,
                dir.name,
                dir.modified.format("%Y-%m-%d %H:%M:%S")
            ));
        }
        for file in &root.files {
            ftxt.push_str(&format!(
                "<tr><td><a href=\"./{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
                file.name,
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
impl Handler for Static {
    async fn handle(&self, conf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response) {
        let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
        let base_path = if let Some((_, value)) = param { value } else { req.url().path() };
        let mut files: HashMap<String, Metadata> = HashMap::new();
        let mut dirs: HashMap<String, Metadata> = HashMap::new();
        let mut path_exist = false;
        for root in &self.roots {
            let path = root.join(&base_path);
            if path.is_dir() && self.options.listing {
                path_exist = true;
                if !req.url().path().ends_with('/') {
                    resp.redirect_found(format!("{}/", req.url().path()));
                    return;
                }
                for ifile in &self.options.defaults {
                    let ipath = path.join(ifile);
                    if ipath.exists() {
                        if let Ok(named_file) = NamedFile::open(path, None, None) {
                            named_file.write(conf, req, depot, resp).await;
                        } else {
                            resp.set_http_error(InternalServerError(Some("file read error".into()), None));
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
                if let Ok(named_file) = NamedFile::open(path, None, None) {
                    named_file.write(conf, req, depot, resp).await;
                } else {
                    resp.set_http_error(InternalServerError(Some("file read error".into()), None));
                }
                return;
            }
        }
        if !path_exist || !self.options.listing {
            resp.not_found();
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
        let root = BaseInfo::new(req.url().path().to_owned(), files, dirs);
        match format.subtype().as_ref() {
            "text" => resp.render_plain_text(&list_text(&root)),
            "json" => resp.render_json_text(&list_json(&root)),
            "xml" => resp.render_xml_text(&list_xml(&root)),
            _ => resp.render_html_text(&list_html(&root)),
        }
    }
}

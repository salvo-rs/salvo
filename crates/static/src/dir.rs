//! serve static dir

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Write;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use salvo_core::fs::NamedFile;
use salvo_core::http::{Request, Response, StatusCode, StatusError};
use salvo_core::writer::Text;
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, IntoVecString};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{format_description, OffsetDateTime};

use super::{decode_url_path_safely, encode_url_path, format_url_path_safely, redirect_to_dir_url};

/// Static roots.
pub trait StaticRoots {
    /// Collect all static roots.
    fn collect(self) -> Vec<PathBuf>;
}

impl<'a> StaticRoots for &'a str {
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl<'a> StaticRoots for &'a String {
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl StaticRoots for String {
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        vec![PathBuf::from(self)]
    }
}
impl StaticRoots for PathBuf {
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        vec![self]
    }
}
impl<T> StaticRoots for Vec<T>
where
    T: Into<PathBuf> + AsRef<OsStr>,
{
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        self.iter().map(Into::into).collect()
    }
}
impl<T, const N: usize> StaticRoots for [T; N]
where
    T: Into<PathBuf> + AsRef<OsStr>,
{
    #[inline]
    fn collect(self) -> Vec<PathBuf> {
        self.iter().map(Into::into).collect()
    }
}

/// StaticDir
#[derive(Clone)]
pub struct StaticDir {
    /// Static roots.
    pub roots: Vec<PathBuf>,
    /// During the file chunk read, the maximum read size at one time will affect the
    /// access experience and the demand for server memory.
    ///
    /// Please set it according to your own situation.
    ///
    /// The default is 1M.
    pub chunk_size: Option<u64>,
    /// List dot files.
    pub dot_files: bool,
    /// Listing dir
    pub listing: bool,
    /// Default file names list.
    pub defaults: Vec<String>,
    /// Fallback file name. This is used when the requested file is not found.
    pub fallback: Option<String>,
}
impl StaticDir {
    /// Create new `StaticDir`.
    #[inline]
    pub fn new<T: StaticRoots + Sized>(roots: T) -> Self {
        StaticDir {
            roots: roots.collect(),
            chunk_size: None,
            dot_files: false,
            listing: false,
            defaults: vec![],
            fallback: None,
        }
    }

    /// Sets dot_files and returns a new `StaticDirOptions`.
    #[inline]
    pub fn with_dot_files(mut self, dot_files: bool) -> Self {
        self.dot_files = dot_files;
        self
    }

    /// Sets listing and returns a new `StaticDirOptions`.
    #[inline]
    pub fn with_listing(mut self, listing: bool) -> Self {
        self.listing = listing;
        self
    }

    /// Sets defaults and returns a new `StaticDirOptions`.
    #[inline]
    pub fn with_defaults(mut self, defaults: impl IntoVecString) -> Self {
        self.defaults = defaults.into_vec_string();
        self
    }

    /// Sets fallback and returns a new `StaticDirOptions`.
    pub fn with_fallback(mut self, fallback: impl Into<String>) -> Self {
        self.fallback = Some(fallback.into());
        self
    }

    /// During the file chunk read, the maximum read size at one time will affect the
    /// access experience and the demand for server memory.
    ///
    /// Please set it according to your own situation.
    ///
    /// The default is 1M.
    #[inline]
    pub fn with_chunk_size(mut self, size: u64) -> Self {
        self.chunk_size = Some(size);
        self
    }
}
#[derive(Serialize, Deserialize, Debug)]
struct CurrentInfo {
    path: String,
    files: Vec<FileInfo>,
    dirs: Vec<DirInfo>,
}
impl CurrentInfo {
    #[inline]
    fn new(path: String, files: Vec<FileInfo>, dirs: Vec<DirInfo>) -> CurrentInfo {
        CurrentInfo { path, files, dirs }
    }
}
#[derive(Serialize, Deserialize, Debug)]
struct FileInfo {
    name: String,
    size: u64,
    modified: OffsetDateTime,
}
impl FileInfo {
    #[inline]
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
    modified: OffsetDateTime,
}
impl DirInfo {
    #[inline]
    fn new(name: String, metadata: Metadata) -> DirInfo {
        DirInfo {
            name,
            modified: metadata.modified().unwrap_or_else(|_| SystemTime::now()).into(),
        }
    }
}

#[async_trait]
impl Handler for StaticDir {
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
        let req_path = req.uri().path();
        let rel_path = if let Some((_, value)) = param {
            value.clone()
        } else {
            decode_url_path_safely(req_path)
        };
        let rel_path = format_url_path_safely(&rel_path);
        let mut files: HashMap<String, Metadata> = HashMap::new();
        let mut dirs: HashMap<String, Metadata> = HashMap::new();
        let is_dot_file = Path::new(&rel_path)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.starts_with('.'))
            .unwrap_or(false);
        let mut abs_path = None;
        if self.dot_files || !is_dot_file {
            for root in &self.roots {
                let path = root.join(&rel_path);
                if path.is_dir() {
                    if !req_path.ends_with('/') && !req_path.is_empty() {
                        redirect_to_dir_url(req.uri(), res);
                        return;
                    }

                    for ifile in &self.defaults {
                        let ipath = path.join(ifile);
                        if ipath.is_file() {
                            abs_path = Some(ipath);
                            break;
                        }
                    }

                    if self.listing && abs_path.is_none() {
                        abs_path = Some(path);
                    }
                    if abs_path.is_some() {
                        break;
                    }
                } else if path.is_file() {
                    abs_path = Some(path);
                }
            }
        }
        let fallback = self.fallback.as_deref().unwrap_or_default();
        if abs_path.is_none() && !fallback.is_empty() {
            for root in &self.roots {
                let path = root.join(fallback);
                if path.is_file() {
                    abs_path = Some(path);
                    break;
                }
            }
        }

        let abs_path = match abs_path {
            Some(path) => path,
            None => {
                res.set_status_error(StatusError::not_found());
                return;
            }
        };

        if abs_path.is_file() {
            let builder = {
                let mut builder = NamedFile::builder(abs_path);
                if let Some(size) = self.chunk_size {
                    builder = builder.buffer_size(size);
                }
                builder
            };
            if let Ok(named_file) = builder.build().await {
                named_file.send(req.headers(), res).await;
            } else {
                res.set_status_error(StatusError::internal_server_error().with_summary("read file failed"));
            }
        } else if abs_path.is_dir() {
            // list the dir
            if let Ok(mut entries) = tokio::fs::read_dir(&abs_path).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(metadata) = entry.metadata().await {
                        if metadata.is_dir() {
                            dirs.entry(entry.file_name().to_string_lossy().to_string())
                                .or_insert(metadata);
                        } else {
                            let file_name = entry.file_name().to_string_lossy().to_string();
                            if !self.dot_files && file_name.starts_with('.') {
                                continue;
                            }
                            files.entry(file_name).or_insert(metadata);
                        }
                    }
                }
            }

            let format = req.first_accept().unwrap_or(mime::TEXT_HTML);
            let mut files: Vec<FileInfo> = files
                .into_iter()
                .map(|(name, metadata)| FileInfo::new(name, metadata))
                .collect();
            files.sort_by(|a, b| a.name.cmp(&b.name));
            let mut dirs: Vec<DirInfo> = dirs
                .into_iter()
                .map(|(name, metadata)| DirInfo::new(name, metadata))
                .collect();
            dirs.sort_by(|a, b| a.name.cmp(&b.name));
            let root = CurrentInfo::new(decode_url_path_safely(req_path), files, dirs);
            res.set_status_code(StatusCode::OK);
            match format.subtype().as_ref() {
                "plain" => res.render(Text::Plain(list_text(&root))),
                "json" => res.render(Text::Json(list_json(&root))),
                "xml" => res.render(Text::Xml(list_xml(&root))),
                _ => res.render(Text::Html(list_html(&root))),
            }
        }
    }
}

#[inline]
fn list_json(current: &CurrentInfo) -> String {
    json!(current).to_string()
}
fn list_xml(current: &CurrentInfo) -> String {
    let mut ftxt = "<list>".to_owned();
    if current.dirs.is_empty() && current.files.is_empty() {
        ftxt.push_str("No files");
    } else {
        let format = format_description::parse("%Y-%m-%d %H:%M:%S").unwrap();
        for dir in &current.dirs {
            write!(
                ftxt,
                "<dir><name>{}</name><modified>{}</modified><link>{}</link></dir>",
                dir.name,
                dir.modified.format(&format).unwrap(),
                encode_url_path(&dir.name),
            )
            .ok();
        }
        for file in &current.files {
            write!(
                ftxt,
                "<file><name>{}</name><modified>{}</modified><size>{}</size><link>{}</link></file>",
                file.name,
                file.modified.format(&format).unwrap(),
                file.size,
                encode_url_path(&file.name),
            )
            .ok();
        }
    }
    ftxt.push_str("</list>");
    ftxt
}
fn list_html(current: &CurrentInfo) -> String {
    fn header_links(path: &str) -> String {
        let segments = path.trim_start_matches('/').trim_end_matches('/').split('/');
        let mut link = "".to_string();
        format!(
            r#"<a href="/">{}</a>{}"#,
            HOME_ICON,
            segments
                .map(|seg| {
                    link = format!("{}/{}", link, seg);
                    format!("/<a href=\"{}\">{}</a>", link, seg)
                })
                .collect::<Vec<_>>()
                .join("")
        )
    }
    let mut ftxt = format!(
        r#"<!DOCTYPE html><html><head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width">
        <title>{}</title>
        <style>{}</style></head><body><header><h3>Index of: {}</h3></header><hr/>"#,
        current.path,
        HTML_STYLE,
        header_links(&current.path)
    );
    if current.dirs.is_empty() && current.files.is_empty() {
        write!(ftxt, "<p>No files</p>").ok();
    } else {
        write!(ftxt, "<table><tr><th>").ok();
        if !(current.path.is_empty() || current.path == "/") {
            write!(ftxt, "<a href=\"../\">[..]</a>").ok();
        }
        write!(ftxt, "</th><th>Name</th><th>Last modified</th><th>Size</th></tr>").ok();
        let format = format_description::parse("%Y-%m-%d %H:%M:%S").unwrap();
        for dir in &current.dirs {
            write!(
                ftxt,
                r#"<tr><td>{}</td><td><a href="./{}/">{}</a></td><td>{}</td><td></td></tr>"#,
                DIR_ICON,
                encode_url_path(&dir.name),
                dir.name,
                dir.modified.format(&format).unwrap(),
            )
            .ok();
        }
        for file in &current.files {
            write!(
                ftxt,
                r#"<tr><td>{}</td><td><a href="./{}">{}</a></td><td>{}</td><td>{}</td></tr>"#,
                FILE_ICON,
                encode_url_path(&file.name),
                file.name,
                file.modified.format(&format).unwrap(),
                file.size
            )
            .ok();
        }
        write!(ftxt, "</table>").ok();
    }
    write!(
        ftxt,
        r#"<hr/><footer><a href="https://salvo.rs" target="_blank">salvo</a></footer></body>"#
    )
    .ok();
    ftxt
}
#[inline]
fn list_text(current: &CurrentInfo) -> String {
    json!(current).to_string()
}

const HTML_STYLE: &str = r#"
    :root {
        --bg-color: #fff;
        --text-color: #222;
        --link-color: #0366d6;
        --link-visited-color: #f22526;
        --dir-icon-color: #79b8ff;
        --file-icon-color: #959da5;
    }
    body {background: var(--bg-color); color: var(--text-color);}
    a {text-decoration:none;color:var(--link-color);}
    a:visited {color: var(--link-visited-color);}
    a:hover {text-decoration:underline;}
    header a {padding: 0 6px;}
    footer {text-align:center;font-size:12px;}
    table {text-align:left;border-collapse: collapse;}
    tr {border-bottom: solid 1px #ccc;}
    tr:last-child {border-bottom: none;}
    th, td {padding: 5px;}
    th:first-child,td:first-child {text-align: center;}
    svg[data-icon="dir"] {vertical-align: text-bottom; color: var(--dir-icon-color); fill: currentColor;}
    svg[data-icon="file"] {vertical-align: text-bottom; color: var(--file-icon-color); fill: currentColor;}
    svg[data-icon="home"] {width:18px;}
    @media (prefers-color-scheme: dark) {
        :root {
            --bg-color: #222;
            --text-color: #ddd;
            --link-color: #539bf5;
            --link-visited-color: #f25555;
            --dir-icon-color: #7da3d0;
            --file-icon-color: #545d68;
        }}
    }"#;
const DIR_ICON: &str = r#"<svg aria-label="Directory" data-icon="dir" width="20" height="20" viewBox="0 0 512 512" version="1.1" role="img"><path fill="currentColor" d="M464 128H272l-64-64H48C21.49 64 0 85.49 0 112v288c0 26.51 21.49 48 48 48h416c26.51 0 48-21.49 48-48V176c0-26.51-21.49-48-48-48z"></path></svg>"#;
const FILE_ICON: &str = r#"<svg aria-label="File" data-icon="file" width="20" height="20" viewBox="0 0 384 512" version="1.1" role="img"><path d="M369.9 97.9L286 14C277 5 264.8-.1 252.1-.1H48C21.5 0 0 21.5 0 48v416c0 26.5 21.5 48 48 48h288c26.5 0 48-21.5 48-48V131.9c0-12.7-5.1-25-14.1-34zM332.1 128H256V51.9l76.1 76.1zM48 464V48h160v104c0 13.3 10.7 24 24 24h104v288H48z"/></svg>"#;
const HOME_ICON: &str = r#"<svg aria-hidden="true" data-icon="home" viewBox="0 0 576 512"><path fill="currentColor" d="M280.37 148.26L96 300.11V464a16 16 0 0 0 16 16l112.06-.29a16 16 0 0 0 15.92-16V368a16 16 0 0 1 16-16h64a16 16 0 0 1 16 16v95.64a16 16 0 0 0 16 16.05L464 480a16 16 0 0 0 16-16V300L295.67 148.26a12.19 12.19 0 0 0-15.3 0zM571.6 251.47L488 182.56V44.05a12 12 0 0 0-12-12h-56a12 12 0 0 0-12 12v72.61L318.47 43a48 48 0 0 0-61 0L4.34 251.47a12 12 0 0 0-1.6 16.9l25.5 31A12 12 0 0 0 45.15 301l235.22-193.74a12.19 12.19 0 0 1 15.3 0L530.9 301a12 12 0 0 0 16.9-1.6l25.5-31a12 12 0 0 0-1.7-16.93z"></path></svg>"#;

//! `washi --json` の出力の形。ここの型が、そのまま公開する契約になる（README の「For AI agents」と対応）。
//!
//! 互換性の方針: キーの追加は 1.x でも行う。キーの削除・改名・型や意味の変更は `FORMAT_VERSION` を上げ、
//! CHANGELOG に書く。読む側は、知らないキーを無視する前提。

use serde::Serialize;

pub const FORMAT_VERSION: u32 = 1;

/// 機械が見る失敗の種類。名前を変えると契約が変わる。`message` の文言は保証しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// 使い方の誤り（未知のオプション、ファイル未指定など）
    Usage,
    NotFound,
    IsDirectory,
    UnsupportedFormat,
    StdinEmpty,
    StdinUnreadable,
    /// 一時ファイルやリンクの作成に失敗した
    Io,
    /// アプリを起動できなかった
    LaunchFailed,
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 8] = [
        Self::Usage,
        Self::NotFound,
        Self::IsDirectory,
        Self::UnsupportedFormat,
        Self::StdinEmpty,
        Self::StdinUnreadable,
        Self::Io,
        Self::LaunchFailed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::NotFound => "not_found",
            Self::IsDirectory => "is_directory",
            Self::UnsupportedFormat => "unsupported_format",
            Self::StdinEmpty => "stdin_empty",
            Self::StdinUnreadable => "stdin_unreadable",
            Self::Io => "io",
            Self::LaunchFailed => "launch_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CliError {
    pub kind: ErrorKind,
    pub message: String,
    /// 指定されたとおりのパス（あれば）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl CliError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), path: None }
    }

    pub fn at(kind: ErrorKind, message: impl Into<String>, path: impl Into<String>) -> Self {
        Self { kind, message: message.into(), path: Some(path.into()) }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Usage, message)
    }
}

pub const EXIT_OK: i32 = 0;
pub const EXIT_LAUNCH_FAILED: i32 = 1;
pub const EXIT_INVALID: i32 = 2;

/// 失敗の種類から終了コードを決める唯一の場所。起動できなかったときだけ 1、それ以外は 2。
pub fn exit_code(errors: &[CliError]) -> i32 {
    if errors.iter().any(|e| e.kind == ErrorKind::LaunchFailed) {
        EXIT_LAUNCH_FAILED
    } else {
        EXIT_INVALID
    }
}

/// 失敗（stderr に 1 行）
#[derive(Serialize)]
pub struct ErrorReport<'a> {
    pub format_version: u32,
    pub errors: &'a [CliError],
}

#[derive(Serialize)]
pub struct FileEntry {
    pub path: String,
    /// `markdown` / `mermaid` / `typst` / `latex` / `pdf`
    pub format: &'static str,
}

/// 成功（stdout に 1 行）。`--no-launch` のときも `files` に検証したファイルが入る
#[derive(Serialize)]
pub struct OpenReport {
    pub format_version: u32,
    pub launched: bool,
    pub files: Vec<FileEntry>,
}

#[derive(Serialize)]
pub struct FormatEntry {
    pub name: &'static str,
    pub extensions: Vec<&'static str>,
}

#[derive(Serialize)]
pub struct Tools {
    pub latexmk: Option<String>,
    pub tectonic: Option<String>,
}

#[derive(Serialize)]
pub struct FormatsReport {
    pub format_version: u32,
    pub version: &'static str,
    pub formats: Vec<FormatEntry>,
    /// すべての拡張子（昇順）
    pub extensions: Vec<&'static str>,
    pub typst: &'static str,
    pub tex_engine: Option<&'static str>,
    pub tools: Tools,
    pub source_jump_editor: String,
}

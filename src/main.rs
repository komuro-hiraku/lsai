use clap::{Parser, ValueEnum};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Serialize)]
struct DirSummary {
    path: String,
    counts: Counts,
    language_hints: BTreeMap<String, u32>, // Jsonキー順安定
    notable_files: NotableFiles,
    suspicious: Vec<String>,
    top_file_by_size: Vec<SizeEntry>,
}

#[derive(Debug, Serialize)]
struct Counts {
    total_entries: u32,
    files: u32,
    dirs: u32,
    hidden: u32,
}

#[derive(Debug, Serialize)]
struct NotableFiles {
    has_git: bool,
    has_readme: bool,
    has_license: bool,
    has_dockerfile: bool,
    has_ci: bool, // .github/workflow等
    has_rust: bool,
    has_node: bool,
    has_python: bool,
}

#[derive(Debug, Serialize)]
struct SizeEntry {
    name: String,
    bytes: u64,
}

#[derive(Parser, Debug)]
#[command(name = "lsai")]
#[command(about = "ディレクトリ構成をAIに開設させるCLIツール", long_about = None)]
struct Cli {
    /// 対象ディレクトリ（デフォルトはカレントディレクトリ）
    #[arg(default_value = ".")]
    path: PathBuf,

    /// 詳細解析
    #[arg(short, long)]
    detail: bool,

    #[arg(long, value_enum, default_value_t = Focus::Normal)]
    focus: Focus,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum Focus {
    Normal,
    Security,
    Structure,
}

#[derive(Debug)]
struct FileInfo {
    name: String,
    is_dir: bool,
    extension: Option<String>,
    size: Option<u64>,
    modified: Option<SystemTime>,
    is_hidden: bool,
}

fn collect_dir(path: &Path) -> io::Result<Vec<FileInfo>> {
    let mut results = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        let file_type = metadata.file_type();
        let is_dir = file_type.is_dir();

        let file_name_os = entry.file_name();
        let file_name = file_name_os.to_string_lossy().to_string();

        let is_hidden = file_name.starts_with('.');

        let extension = Path::new(&file_name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        let size = if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        };

        let modified = metadata.modified().ok();

        results.push(FileInfo {
            name: file_name,
            is_dir,
            extension,
            size,
            modified,
            is_hidden,
        });
    }

    Ok(results)
}

fn build_summary(path: &Path, files: &[FileInfo]) -> DirSummary {
    let mut total_entries = 0u32;
    let mut files_count = 0u32;
    let mut dirs_count = 0u32;
    let mut hidden_count = 0u32;

    let mut ext_counts: BTreeMap<String, u32> = BTreeMap::new();

    let mut has_git = false;
    let mut has_readme = false;
    let mut has_license = false;
    let mut has_dockerfile = false;
    let mut has_ci = false;
    let mut has_rust = false;
    let mut has_node = false;
    let mut has_python = false;

    let mut suspicious: Vec<String> = Vec::new();
    let mut size_entries: Vec<SizeEntry> = Vec::new();

    for f in files {
        total_entries += 1;

        if f.is_hidden {
            hidden_count += 1;
        }

        if f.is_dir {
            dirs_count += 1;

            if f.name == ".github" {
                has_ci = true;
            }
            if f.name == ".git" {
                has_git = true;
            }
        } else {
            files_count += 1;

            // 拡張カウント
            if let Some(ext) = &f.extension {
                *ext_counts.entry(ext.to_lowercase()).or_insert(0) += 1;
            }

            // サイズ上位用
            if let Some(sz) = f.size {
                size_entries.push(SizeEntry {
                    name: f.name.clone(),
                    bytes: sz,
                });
            }

            // 目立つファイル群
            let lower = f.name.to_lowercase();
            if lower == "readme" || lower.starts_with("readme.") {
                has_readme = true;
            }
            if lower == "license" || lower.starts_with("license.") {
                has_license = true;
            }
            if lower == "dockerfile" {
                has_dockerfile = true;
            }

            // 雑に言語/エコシステム推定
            if lower == "cargo.toml" {
                has_rust = true;
            }
            if lower == "package.json" {
                has_node = true;
            }
            if lower == "pyproject.toml" || lower == "requirement.txt" {
                has_python = true;
            }

            // 妖しいファイル
            if lower == ".env" || lower.ends_with(".pem") || lower.contains("id_rsa") {
                suspicious.push(f.name.clone());
            }
            if lower.ends_with(".log") || lower.ends_with(".dump") || lower.ends_with(".sql") {
                suspicious.push(f.name.clone());
            }
        }
    }

    // サイズ上位5件
    size_entries.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    size_entries.truncate(5);

    let notable_files = NotableFiles {
        has_git,
        has_readme,
        has_license,
        has_dockerfile,
        has_ci,
        has_rust,
        has_node,
        has_python,
    };

    DirSummary {
        path: path.to_string_lossy().to_string(),
        counts: Counts {
            total_entries,
            files: files_count,
            dirs: dirs_count,
            hidden: hidden_count,
        },
        language_hints: ext_counts,
        notable_files,
        suspicious,
        top_file_by_size: size_entries,
    }
}

fn main() {
    let cli = Cli::parse();

    let path = cli.path;

    println!("path   : {:?}", path);
    println!("detail : {:?}", cli.detail);
    println!("focus  : {:?}", cli.focus);
    println!();

    let files = match collect_dir(&path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ディレクトリの読み込みに失敗しました: {}", e);
            std::process::exit(1);
        }
    };

    let summary = build_summary(&path, &files);

    // detail があるなら pretty, なければ1行
    if cli.detail {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!("{}", serde_json::to_string(&summary).unwrap());
    }
}

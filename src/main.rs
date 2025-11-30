use std::{fs, io, path::{Path, PathBuf}, time::SystemTime};
use clap::{Parser, ValueEnum};

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
        .extension().and_then(|s| s.to_str())
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

fn main() {
    let cli = Cli::parse();

    let path = cli.path;

    println!("path   : {:?}", path);
    println!("detail : {:?}", cli.detail);
    println!("focus  : {:?}", cli.focus);
    println!();

    match collect_dir(&path) {
        Ok(files) => {
            println!("found {} entries:", files.len());

            for info in files {
                let kind = if info.is_dir { "DIR "} else { "FILE" };
                let ext = info.extension.as_deref().unwrap_or("-");
                let hidden = if info.is_hidden { "(hidden)" } else { "" };
                let size = info.size.map(|s| s.to_string()).unwrap_or("-".to_string());

                println!(
                    "[{}] {:<30} ext={:<8} size={:<10} {}",
                    kind, info.name, ext, size, hidden
                );
            }
        }
        Err(e) => {
            eprintln!("ディレクトリの読み込みに失敗しました: {}", e);
            std::process::exit(1);
        }
    }
}

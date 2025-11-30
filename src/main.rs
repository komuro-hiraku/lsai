use std::path::PathBuf;
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

fn main() {
    let cli = Cli::parse();

    println!("path   : {:?}", cli.path);
    println!("detail : {:?}", cli.detail);
    println!("focus  : {:?}", cli.focus);
}

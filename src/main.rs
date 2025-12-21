use anyhow::Ok;
use clap::{Parser, ValueEnum};
use reqwest::header;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fmt::format,
    fs, io,
    os::linux::raw::stat,
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

    io::Result::Ok(results)
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

async fn call_openai_responses(input: &str) -> anyhow::Result<String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| anyhow::anyhow!("OPENAI_API_KEYが環境変数に設定されていません"))?;

    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "model" : "gpt-5.2",
        "input": input,
        "max_output_tokens": 500
    });

    let resp = client
        .post("https://api.openai.com/v1/responses")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let v: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "OpenAI API error: status={}, body={}",
            status,
            v
        ));
    }

    // 返答の取り出し: output_text があればそれを優先
    if let Some(s) = v.get("output_text").and_then(|x| x.as_str()) {
        return Ok(s.to_string());
    }

    // fallback: output配列から拾う
    if let Some(arr) = v.get("output").and_then(|x| x.as_array()) {
        // mesasge -> content[] -> output_text の "text" を連結する雑な実装
        let mut out = String::new();
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("message") {
                if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
                    for c in content {
                        if c.get("type").and_then(|t| t.as_str()) == Some("output") {
                            if let Some(text) = c.get("text").and_then(|t| t.as_str()) {
                                out.push_str(text);
                            }
                        }
                    }
                }
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }

    Err(anyhow::anyhow!("モデル出力の抽出に失敗しました: {}", v))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let path = cli.path;

    let files = collect_dir(&path)?;
    let summary = build_summary(&path, &files);

    // AIに渡す文字列
    let summary_json = if cli.detail {
        serde_json::to_string_pretty(&summary)?
    } else {
        serde_json::to_string(&summary)?
    };

    let focus = format!("{:?}", cli.focus);

    let prompt = format!(
        r#"あなたは熟練のソフトウェアエンジニアです。
以下のディレクトリ要約(JSON)から、このディレクトリが「何のプロジェクトか」を推定し、
良い点・気になる点（特にセキュリティ／構成）・次のアクションを日本語で短くまとめてください。

# focus: {focus}

# summary(JSON)
{summary_json}
"#
    );

    let answer = call_openai_responses(&prompt).await?;
    println!("{answer}");

    Ok(())
}

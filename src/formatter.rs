use std::path::Path;

pub trait Formatter: Send + Sync {
    fn document_start(&self) -> &'static str {
        ""
    }
    fn document_end(&self) -> &'static str {
        ""
    }
    fn write_file_header(
        &self,
        path: &Path,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<()>;
    fn file_footer(&self) -> &'static str;
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum FormatChoice {
    Xml,
    Markdown,
}

fn resolve_display(path: &Path, relative: bool, cwd: Option<&Path>) -> String {
    if relative {
        if let Some(cwd) = cwd {
            if let Some(rel) = pathdiff::diff_paths(path, cwd) {
                return rel.display().to_string();
            }
        }
        return path.display().to_string();
    }
    path.display().to_string()
}

pub fn language_for_extension(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("rs") => "rust",
        Some("py") => "python",
        Some("js" | "mjs" | "cjs") => "javascript",
        Some("ts" | "mts" | "cts") => "typescript",
        Some("jsx") => "jsx",
        Some("tsx") => "tsx",
        Some("go") => "go",
        Some("c") => "c",
        Some("cpp" | "cc" | "cxx") => "cpp",
        Some("h" | "hpp" | "hxx") => "cpp",
        Some("java") => "java",
        Some("rb") => "ruby",
        Some("php") => "php",
        Some("swift") => "swift",
        Some("kt" | "kts") => "kotlin",
        Some("cs") => "csharp",
        Some("fs" | "fsi" | "fsx") => "fsharp",
        Some("html" | "htm") => "html",
        Some("css") => "css",
        Some("scss" | "sass") => "scss",
        Some("json" | "jsonc") => "json",
        Some("yaml" | "yml") => "yaml",
        Some("toml") => "toml",
        Some("md" | "mdx") => "markdown",
        Some("sh" | "bash") => "bash",
        Some("zsh") => "zsh",
        Some("fish") => "fish",
        Some("ps1") => "powershell",
        Some("sql") => "sql",
        Some("xml") => "xml",
        Some("dockerfile") => "dockerfile",
        Some("lua") => "lua",
        Some("r") => "r",
        Some("ex" | "exs") => "elixir",
        Some("hs") => "haskell",
        Some("nix") => "nix",
        Some("proto") => "protobuf",
        Some("graphql" | "gql") => "graphql",
        Some("ipynb") => "python",
        _ => "",
    }
}

pub struct XmlFormatter {
    no_path: bool,
    relative: bool,
    cwd: Option<std::path::PathBuf>,
}

impl XmlFormatter {
    pub fn new(no_path: bool, relative: bool) -> Self {
        let cwd = if relative {
            std::env::current_dir().ok()
        } else {
            None
        };
        Self {
            no_path,
            relative,
            cwd,
        }
    }
}

impl Formatter for XmlFormatter {
    fn document_start(&self) -> &'static str {
        "<context>\n"
    }

    fn document_end(&self) -> &'static str {
        "</context>\n"
    }

    fn write_file_header(
        &self,
        path: &Path,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<()> {
        if self.no_path {
            writer.write_all(b"<file>\n")
        } else {
            let resolved = resolve_display(path, self.relative, self.cwd.as_deref());
            writeln!(writer, "<file path=\"{resolved}\">")
        }
    }

    fn file_footer(&self) -> &'static str {
        "\n</file>\n"
    }
}

pub struct MarkdownFormatter {
    no_path: bool,
    relative: bool,
    cwd: Option<std::path::PathBuf>,
}

impl MarkdownFormatter {
    pub fn new(no_path: bool, relative: bool) -> Self {
        let cwd = if relative {
            std::env::current_dir().ok()
        } else {
            None
        };
        Self {
            no_path,
            relative,
            cwd,
        }
    }
}

impl Formatter for MarkdownFormatter {
    fn write_file_header(
        &self,
        path: &Path,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<()> {
        let lang = language_for_extension(path);
        if self.no_path {
            writeln!(writer, "```{lang}")
        } else {
            let resolved = resolve_display(path, self.relative, self.cwd.as_deref());
            write!(writer, "## File: {resolved}\n\n```{lang}\n")
        }
    }

    fn file_footer(&self) -> &'static str {
        "\n```\n\n"
    }
}

const AIDER_PATCH_BLOCK: &str = r#"<patch method="aider">
Please apply changes using this aider style format all changed in single code block
```
// src/filename1.rs
<<<<<<< SEARCH
[exact original lines (include enough context to be unique, avoid too thin blocks)]
=======
[modified lines]
>>>>>>> REPLACE
 // src/filename2.rs
<<<<<<< SEARCH
[exact original lines (include enough context to be unique, avoid too thin blocks)]
=======
[modified lines]
>>>>>>> REPLACE
```
</patch>
</context>
"#;

pub struct AiderFormatter {
    no_path: bool,
    relative: bool,
    cwd: Option<std::path::PathBuf>,
}

impl AiderFormatter {
    pub fn new(no_path: bool, relative: bool) -> Self {
        let cwd = if relative {
            std::env::current_dir().ok()
        } else {
            None
        };
        Self {
            no_path,
            relative,
            cwd,
        }
    }
}

impl Formatter for AiderFormatter {
    fn document_start(&self) -> &'static str {
        "<context>\n"
    }

    fn document_end(&self) -> &'static str {
        AIDER_PATCH_BLOCK
    }

    fn write_file_header(
        &self,
        path: &Path,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<()> {
        if self.no_path {
            writer.write_all(b"<file>\n")
        } else {
            let resolved = resolve_display(path, self.relative, self.cwd.as_deref());
            writeln!(writer, "<file path=\"{resolved}\">")
        }
    }

    fn file_footer(&self) -> &'static str {
        "\n</file>\n"
    }
}

pub fn build_formatter(
    choice: FormatChoice,
    no_path: bool,
    relative: bool,
    aider: bool,
) -> Box<dyn Formatter> {
    if aider {
        return Box::new(AiderFormatter::new(no_path, relative));
    }
    match choice {
        FormatChoice::Xml => Box::new(XmlFormatter::new(no_path, relative)),
        FormatChoice::Markdown => Box::new(MarkdownFormatter::new(no_path, relative)),
    }
}

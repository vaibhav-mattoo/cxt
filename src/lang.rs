pub struct LangDef {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
}

pub static LANGUAGES: &[LangDef] = &[
    LangDef { name: "rust",       aliases: &["rs"],               extensions: &["rs", "toml"] },
    LangDef { name: "python",     aliases: &["py"],               extensions: &["py", "pyi", "pyw", "ipynb"] },
    LangDef { name: "javascript", aliases: &["js", "node"],       extensions: &["js", "mjs", "cjs", "jsx"] },
    LangDef { name: "typescript", aliases: &["ts"],               extensions: &["ts", "tsx", "mts", "cts"] },
    LangDef { name: "go",         aliases: &[],                   extensions: &["go", "mod", "sum"] },
    LangDef { name: "java",       aliases: &[],                   extensions: &["java"] },
    LangDef { name: "c",          aliases: &[],                   extensions: &["c", "h"] },
    LangDef { name: "cpp",        aliases: &["c++", "cxx"],       extensions: &["cpp", "cxx", "cc", "hpp", "hxx", "h"] },
    LangDef { name: "csharp",     aliases: &["cs", "c#"],         extensions: &["cs", "csx"] },
    LangDef { name: "fsharp",     aliases: &["fs", "f#"],         extensions: &["fs", "fsi", "fsx"] },
    LangDef { name: "ruby",       aliases: &["rb"],               extensions: &["rb", "rake", "gemspec"] },
    LangDef { name: "php",        aliases: &[],                   extensions: &["php"] },
    LangDef { name: "swift",      aliases: &[],                   extensions: &["swift"] },
    LangDef { name: "kotlin",     aliases: &["kt"],               extensions: &["kt", "kts"] },
    LangDef { name: "scala",      aliases: &[],                   extensions: &["scala", "sc", "sbt"] },
    LangDef { name: "haskell",    aliases: &["hs"],               extensions: &["hs", "lhs", "cabal"] },
    LangDef { name: "lua",        aliases: &[],                   extensions: &["lua"] },
    LangDef { name: "elixir",     aliases: &["ex"],               extensions: &["ex", "exs"] },
    LangDef { name: "erlang",     aliases: &[],                   extensions: &["erl", "hrl"] },
    LangDef { name: "clojure",    aliases: &["clj"],              extensions: &["clj", "cljs", "cljc", "edn"] },
    LangDef { name: "zig",        aliases: &[],                   extensions: &["zig"] },
    LangDef { name: "dart",       aliases: &[],                   extensions: &["dart"] },
    LangDef { name: "r",          aliases: &[],                   extensions: &["r", "rmd"] },
    LangDef { name: "julia",      aliases: &["jl"],               extensions: &["jl"] },
    LangDef { name: "shell",      aliases: &["bash", "sh"],       extensions: &["sh", "bash", "zsh", "fish", "ps1"] },
    LangDef { name: "nix",        aliases: &[],                   extensions: &["nix"] },
    LangDef { name: "terraform",  aliases: &["tf"],               extensions: &["tf", "tfvars"] },
    LangDef { name: "html",       aliases: &[],                   extensions: &["html", "htm"] },
    LangDef { name: "css",        aliases: &[],                   extensions: &["css", "scss", "sass", "less"] },
    LangDef { name: "sql",        aliases: &[],                   extensions: &["sql"] },
    LangDef { name: "markdown",   aliases: &["md"],               extensions: &["md", "mdx"] },
    LangDef { name: "yaml",       aliases: &[],                   extensions: &["yaml", "yml"] },
    LangDef { name: "json",       aliases: &[],                   extensions: &["json", "jsonc"] },
    LangDef { name: "toml",       aliases: &[],                   extensions: &["toml"] },
    LangDef { name: "proto",      aliases: &["protobuf"],         extensions: &["proto"] },
    LangDef { name: "graphql",    aliases: &["gql"],              extensions: &["graphql", "gql"] },
    LangDef { name: "dockerfile", aliases: &["docker"],           extensions: &["dockerfile"] },
];

/// Look up a language by canonical name or alias (case-insensitive).
pub fn find(name: &str) -> Option<&'static LangDef> {
    let lower = name.to_lowercase();
    LANGUAGES.iter().find(|def| {
        def.name == lower || def.aliases.iter().any(|&a| a == lower)
    })
}

/// All canonical names, sorted, for use in error messages.
pub fn all_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = LANGUAGES.iter().map(|d| d.name).collect();
    names.sort_unstable();
    names
}

/// Build a lowercase extension HashSet from --lang and --ext flag values.
/// Returns Err with a user-facing message if any --lang value is unrecognised.
pub fn build_extension_filter(
    lang_args: &[String],
    ext_args: &[String],
) -> Result<std::collections::HashSet<String>, String> {
    let mut set = std::collections::HashSet::new();

    for raw in lang_args {
        for token in raw.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            if token.eq_ignore_ascii_case("help") {
                continue;
            }
            match find(token) {
                Some(def) => {
                    for &ext in def.extensions {
                        set.insert(ext.to_string());
                    }
                }
                None => {
                    return Err(format!(
                        "Unknown language '{}'. Supported languages:\n  {}",
                        token,
                        all_names().join(", ")
                    ));
                }
            }
        }
    }

    for raw in ext_args {
        for token in raw.split(',') {
            let token = token.trim().trim_start_matches('.');
            if token.is_empty() {
                continue;
            }
            set.insert(token.to_lowercase());
        }
    }

    Ok(set)
}

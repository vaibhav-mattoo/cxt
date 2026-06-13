use anyhow::{Context, Result};
use serde_json::Value;

/// Max notebook size we'll load fully to parse. Larger files fall back to raw
/// handling to avoid OOM on output-heavy notebooks (e.g. embedded images).
pub const MAX_NOTEBOOK_BYTES: u64 = 50 * 1024 * 1024;

/// Extract and concatenate the source of every code cell in a notebook,
/// separated by `# %%` cell markers. Returns Err if the bytes aren't a
/// valid notebook (no `cells` array, invalid JSON, etc.).
pub fn extract_notebook_code(content: &[u8]) -> Result<String> {
    let notebook: Value =
        serde_json::from_slice(content).context("invalid notebook JSON")?;
    let cells = notebook
        .get("cells")
        .and_then(|v| v.as_array())
        .context("notebook has no 'cells' array")?;

    let mut out = String::new();
    for cell in cells {
        if cell.get("cell_type").and_then(|v| v.as_str()) != Some("code") {
            continue;
        }
        // nbformat stores `source` as either a string or an array of line
        // strings (each line already includes its trailing '\n'), so join with "".
        let code = match cell.get("source") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(arr)) => {
                arr.iter().filter_map(|v| v.as_str()).collect::<String>()
            }
            _ => continue,
        };
        if code.trim().is_empty() {
            continue; // skip empty code cells
        }
        out.push_str("# %%\n");
        out.push_str(&code);
        if !code.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }

    if out.is_empty() {
        out.push_str("# (notebook contains no code cells)\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_notebook_code() {
        let nb = r##"{
  "cells": [
    {
      "cell_type": "markdown",
      "source": ["# Title\n", "Some prose."]
    },
    {
      "cell_type": "code",
      "source": ["import os\n", "print(os.getcwd())"]
    },
    {
      "cell_type": "raw",
      "source": "raw cell text"
    },
    {
      "cell_type": "code",
      "source": "x = 42\nprint(x)"
    }
  ]
}"##;

        let result = extract_notebook_code(nb.as_bytes()).unwrap();

        // Both code cells present
        assert!(result.contains("import os"));
        assert!(result.contains("print(os.getcwd())"));
        assert!(result.contains("x = 42"));
        assert!(result.contains("print(x)"));

        // Cell markers present
        assert_eq!(result.matches("# %%").count(), 2);

        // Markdown and raw content absent
        assert!(!result.contains("# Title"));
        assert!(!result.contains("Some prose"));
        assert!(!result.contains("raw cell text"));
    }

    #[test]
    fn test_no_code_cells() {
        let nb = r#"{"cells": [{"cell_type": "markdown", "source": "hello"}]}"#;
        let result = extract_notebook_code(nb.as_bytes()).unwrap();
        assert!(result.contains("no code cells"));
    }

    #[test]
    fn test_invalid_json() {
        let result = extract_notebook_code(b"not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_cells_array() {
        let result = extract_notebook_code(br#"{"metadata": {}}"#);
        assert!(result.is_err());
    }
}

use anyhow::{Context, Result};
use serde_json::Value;

/// Max notebook size we'll load fully to parse. Larger files fall back to raw
/// handling to avoid OOM on output-heavy notebooks (e.g. embedded images).
pub const MAX_NOTEBOOK_BYTES: u64 = 50 * 1024 * 1024;

/// Extract and concatenate the source of every code cell in a notebook,
/// separated by `# %%` cell markers. Returns Err if the bytes aren't a
/// valid notebook (no `cells` or `worksheets` array, invalid JSON, etc.).
pub fn extract_notebook_code(content: &[u8]) -> Result<String> {
    let notebook: Value =
        serde_json::from_slice(content).context("invalid notebook JSON")?;

    // nbformat 4+: top-level `cells`. nbformat 2/3: cells nested under
    // `worksheets[].cells` (possibly several worksheets).
    let cells: Vec<&Value> = if let Some(arr) =
        notebook.get("cells").and_then(|v| v.as_array())
    {
        arr.iter().collect()
    } else if let Some(worksheets) =
        notebook.get("worksheets").and_then(|v| v.as_array())
    {
        worksheets
            .iter()
            .filter_map(|ws| ws.get("cells").and_then(|c| c.as_array()))
            .flatten()
            .collect()
    } else {
        anyhow::bail!("notebook has no 'cells' or 'worksheets' array");
    };

    let mut out = String::new();
    for cell in cells {
        if cell.get("cell_type").and_then(|v| v.as_str()) != Some("code") {
            continue;
        }
        // nbformat 4 uses `source`; nbformat 2/3 uses `input`. Either may be a
        // string or an array of line strings (join with "" — lines keep their '\n').
        let raw = cell.get("source").or_else(|| cell.get("input"));
        let code = match raw {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(arr)) => {
                arr.iter().filter_map(|v| v.as_str()).collect::<String>()
            }
            _ => continue,
        };
        if code.trim().is_empty() {
            continue;
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
        assert!(result.unwrap_err().to_string().contains("'cells' or 'worksheets'"));
    }

    #[test]
    fn test_nbformat3_basic() {
        let nb = r##"{
  "nbformat": 3,
  "nbformat_minor": 0,
  "worksheets": [
    {
      "cells": [
        { "cell_type": "heading", "level": 1, "source": ["Title"] },
        { "cell_type": "markdown", "source": ["some prose"] },
        { "cell_type": "code", "language": "python",
          "input": ["import sys\n", "import time"], "outputs": [] },
        { "cell_type": "code", "language": "python",
          "input": "print(sys.version)", "outputs": [] }
      ]
    }
  ]
}"##;

        let result = extract_notebook_code(nb.as_bytes()).unwrap();

        assert!(result.contains("import sys"));
        assert!(result.contains("import time"));
        assert!(result.contains("print(sys.version)"));
        assert_eq!(result.matches("# %%").count(), 2);

        assert!(!result.contains("Title"));
        assert!(!result.contains("some prose"));
    }

    #[test]
    fn test_nbformat3_multi_worksheet() {
        let nb = r#"{"worksheets": [
          {"cells": [{"cell_type": "code", "input": "x = 1"}]},
          {"cells": [{"cell_type": "code", "input": "y = 2"}]}
        ]}"#;

        let result = extract_notebook_code(nb.as_bytes()).unwrap();

        assert!(result.contains("x = 1"));
        assert!(result.contains("y = 2"));
        assert_eq!(result.matches("# %%").count(), 2);
        // order preserved: x before y
        assert!(result.find("x = 1").unwrap() < result.find("y = 2").unwrap());
    }
}

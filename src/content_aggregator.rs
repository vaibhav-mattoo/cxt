use anyhow::Result;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::path_formatter::PathFormatter;

pub struct ContentAggregator {

    /// the formatted strings containing the file paths
    path_formatter: PathFormatter,

    /// if -n/--no-path is used this is false
    include_headers: bool,

    /// if --hidden is used then this is set to true
    include_hidden_in_dirs: bool,

    /// number of files to be copied to clipboard
    file_count: usize,

    /// here are all the files in the ignore path from cli
    ignore: Vec<std::path::PathBuf>,
}

impl ContentAggregator {

    /// constructor for ContentAggregator
    /// inputs :
    ///     use_relative : decides how paths are formatted
    ///     no_path : true if no paths to be included
    ///     include_hidden_in_dirs : true if hidden paths needed
    ///     ignore : vector of string paths to ignore

    pub fn new(use_relative: bool, no_path: bool, include_hidden_in_dirs: bool, ignore: Vec<String>) -> Self {
        Self {

            path_formatter: PathFormatter::new(use_relative, no_path),

            include_headers: !no_path,

            include_hidden_in_dirs,

            file_count: 0,

            // the ignore argument is a vector of strings
            // we need a vector of std::Path::PathBuf
            // into_iter takes ownership of original vector and yields each element one by one
            // then map transforms every element with the PathBuf::from operation
            //     PathBuf::from converts a String (or &str) into a PathBuf (better path
            //         representation for file system paths)
            // .collect makes converted iterators back into a collection (vector)

            ignore: ignore.into_iter().map(std::path::PathBuf::from).collect(),
        }
    }

    /// Check if a path should be ignored
    fn is_ignored(&self, path: &Path) -> bool {

        // we take the ignore vector of PathBuf and create iterators
        //     these yield references to each ignored path in the vector
        // .any returns true if the closure returns true for ANY element in the iterator
        //
        // closure is an anonymous function you can write inline
        //     captures all variables from its environment
        //     |...| have the arguments for this anonymous function
        //     {...} has the body of the closure (code it executes)
        //
        //     this function returns true if the passed path is either a file in the ignored path
        //     or the ignored path is its prefix (starts with the ignored path)

        self.ignore.iter().any(|ignore_path| {
            path == ignore_path || path.starts_with(ignore_path)
        })
    }

    /// Aggregate content from multiple paths
    ///
    /// about the arguments:
    ///     every value in memory has a single owner function in rust
    ///         this owner is responsible for cleaning it up when it ends
    ///
    ///     Borrowing lets other functions you call use the data of your function
    ///         without transferring ownership
    ///
    ///     Two kinds of borrowing:
    ///         Immutable borrow (&T) : This function will have a read only view of your data
    ///         Mutable borrow : Only one part of your code can have a mutable reference to the
    ///             data at one time
    ///          Benefits of this are obvious in multithreaded code but in single threaded code the
    ///          idea is:

                    /// #include <stdio.h>
                    ///
                    /// void double_mut(int *a, int *b) {
                    ///     *a += 10;
                    ///     *b *= 2;
                    /// }
                    ///
                    /// int main() {
                    ///     int value = 5;
                    ///     double_mut(&value, &value); // Two mutable pointers to same data
                    ///     printf("%d\n", value);      // Output is unpredictable
                    ///     return 0;
                    /// }
    
    /// this sort of scenario is prevented by borrowing


    ///
    /// Here [String] is a slice of string values which means a sequence of strings
    ///     this means [String] is vector/list etc  with zero or more strings in it
    ///     &[String] is a borrowed reference to such a sequence



    /// Here Result<T, E> is an enum for returning with error safety
    /// It represents either an:
    ///     Ok(T) : Wrapper around String class - contains the result string and ok message
    ///     Err(E)  : Err variant that contanis an error value of type E.
    ///
    /// Ok(String) says the function was successful and here is the string Output
    /// To use the String in Ok(String) you must unpack the string first
    ///     to do this use a match which is something like:

                    /// fn read_file_content(path: &str) -> Result<String, std::io::Error> {
                    ///     // Try to read the file content as String
                    ///     let content = std::fs::read_to_string(path)?;
                    ///     // On success, wrap it in Ok and return
                    ///     Ok(content)
                    /// }
                    ///
                    /// fn main() {
                    ///     match read_file_content("file.txt") {
                    ///         Ok(text) => println!("File contents: {}", text),
                    ///         Err(e) => eprintln!("Error reading file: {}", e),
                    ///     }
                    /// }


    pub fn aggregate_paths(&mut self, paths: &[String]) -> Result<String> {
        let mut content = String::new();

        // for every string in the paths string slice we convert it to a Path class
        //     then we check if the path we made exists (return anyhow error if not)
        //     then we check if the path is to be ignored in which case we don't handle it
        //     if the path is a file then we dispatch to the aggregrate file handler
        //     else if the path is a directory:
        //         we already checked the ignore path so the only case where we dont want the path
        //         is if --hidden is not passed and this is hidden.
        //         but if the user explicitly provided a hidden path then even if hidden we still
        //         have to take so we have a check for that too.
        //     otherwise dispatch to aggregate directory handler for that path
        //
        //     The ? operator propogates the error upwards if the function call errors out
        //      Since the Return enum is used, if the function call errors out then Err will come
        //      This happens immediately and the loop does not keep goinf and error is returned
        //      Right now the ? is pointless since both functions deal with errors internally
        //      but good to keep for futureproofing
        //
        //      If the loop completes that means no errors happened so we return Ok(Content)

        for path_str in paths {
            let path = Path::new(path_str);
            if !path.exists() {
                return Err(anyhow::anyhow!("Path does not exist: {}", path_str));
            }
            if self.is_ignored(path) {
                continue;
            }
            if path.is_file() {
                self.aggregate_file(path, &mut content)?;
            } else if path.is_dir() {
                if !self.include_hidden_in_dirs && self.is_hidden_file(path) && !self.is_explicit_path(path, paths) {
                    continue;
                }
                self.aggregate_directory(path, &mut content)?;
            }
        }
        Ok(content)
    }

    /// Helper: check if a path is explicitly specified in the input paths
    ///
    ///
    /// this is checking function it doesn't take mutable struct since it does not make changes

    fn is_explicit_path(&self, path: &Path, input_paths: &[String]) -> bool {
        // for each of the input paths slice of strings we make an iter for the strings
        // then we use an any function to check out closure
        // in the closure we make the strings into a path and check if the path is present.

        input_paths.iter().any(|p| Path::new(p) == path)
    }

    /// Aggregate content from a single file
    ///
    /// we need a mutable reference to the struct since we need to increase the file count here
    ///
    /// It returns Result<()> which means on success () is returned
    ///     this means no meaningful return value is returned, just the idea of success is conveyed

    fn aggregate_file(&mut self, path: &Path, content: &mut String) -> Result<()> {
        
        // the read_to_string function tries to read the entire content of the file at the path
        // it returns a Result<String, std::io::Error> which we need to check with match

        match fs::read_to_string(path) {

            // if the file was read successfully:
            //     check if we need to include the header
            //         if yes, use te path formatting to format the path
            //         done using the relative/absolute thing in ./path_formatter.rs
            //     then put in the entire successfully read content.
            //     after that we put a new line if the file doesnt already end with one
            //     since the file is new done, increase file count

            Ok(file_content) => {
                if self.include_headers {
                    content.push_str(&self.path_formatter.format_path(path));
                }
                content.push_str(&file_content);
                if !file_content.ends_with('\n') {
                    content.push('\n');
                }
                self.file_count += 1;
            },

            // if there was a problem reading (like non UTF-8 characters then eprintln the error)
            Err(e) => {
                eprintln!("Warning: Failed to read file '{}': {e}", path.display());
            }
        }

        // since we already modified content or printed out an error if the file couldnt be read
        // we want to continue the reading of next files so we always send out an ok so we dont
        // terminate
        Ok(())
    }

    /// Aggregate content from a directory recursively
    fn aggregate_directory(&mut self, dir_path: &Path, content: &mut String) -> Result<()> {

        // we are making local copies of include_hidden and ignore
        // this makes it easier for closures to use this stuff
        // closures borrow by reerencing instead of moving so direct use also fine, just cleaner

        let include_hidden = self.include_hidden_in_dirs;
        let ignore = self.ignore.clone();

        // here path.file_name() takes the final component of the path (file/dir name)
        // and_then() is called which applies the closure if it exists
        // to_str converst the Option<&OsStr> into Some(&str) which represents OS string in UTF-8
        // map takes this name and returns true if it starts with . (indicating hidden)
        // unwrap_or extracts the bool value from the Some(true) or Some(false)
        // if any step returned error then it defaults to false


        let is_hidden = |path: &Path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with('.'))
                .unwrap_or(false)
        };


        let is_ignored = |path: &Path| {
            ignore.iter().any(|ignore_path| path == ignore_path || path.starts_with(ignore_path))
        };

        // WalkDir::new(dir_path) new directory walker which recursively explores dir_path
        // follow_links(true) configures walker to treat symbolic links as real 
        // WARNING: following symbolic link can cause loop
        // filter_entry is applies this filtering closure on every entry in the walk
        //     for each entry we check if the path is in ignored and return false
        //         this causes path to be ignored
        //     if the path is the dir path then we include it in the results and descend into it
        //         this is necessary for the walk to start
        //     we only include hidden if hidden is needed
        //     if no problem then include by default
        //
        //     the final output walker is an iterator that has all the files, directories selected
        //     filtering at runtime allows us to skip exploring the subtree

        let walker = WalkDir::new(dir_path)
            .follow_links(true)
            .into_iter()
            .filter_entry(|entry| {
                let path = entry.path();
                if is_ignored(path) {
                    return false;
                }
                if path == dir_path {
                    true
                } else if path.is_dir() && is_hidden(path) {
                    include_hidden
                } else {
                    true
                }
            });

        // for each item in walker the filter map takes the iterators
        // and returns e.ok() which are the iterators which have e.ok() true
        // this means the ones which dont have permission denied or broken symlink
        // for these we check if it is a directory since the walker already has files
        // we can skip the directories
        // similarly we have second checks for ignored and hidden
        // NOTE: The tests for ignored and hidden can be removed here since already done
        // in aggregate paths


        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if is_ignored(path) {
                continue;
            }
            if path.is_dir() {
                continue;
            }
            if !include_hidden && is_hidden(path) {
                continue;
            }
            self.aggregate_file(path, content)?;
        }
        Ok(())
    }

    /// Check if a file is hidden (starts with .)
    fn is_hidden_file(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
    }

    /// Get the total number of files processed
    pub fn file_count(&self) -> usize {
        self.file_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_aggregate_single_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let content = aggregator.aggregate_paths(&[file_path.to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Hello, World!"));
        assert!(content.contains("--- File:"));
        assert_eq!(aggregator.file_count(), 1);
    }

    #[test]
    fn test_aggregate_file_without_headers() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let mut aggregator = ContentAggregator::new(false, true, false, vec![]);
        let content = aggregator.aggregate_paths(&[file_path.to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Hello, World!"));
        assert!(!content.contains("--- File:"));
        assert_eq!(aggregator.file_count(), 1);
    }

    #[test]
    fn test_aggregate_directory() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        
        let file1 = dir.path().join("file1.txt");
        let file2 = subdir.join("file2.txt");
        
        fs::write(&file1, "File 1 content").unwrap();
        fs::write(&file2, "File 2 content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let content = aggregator.aggregate_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("File 1 content"));
        assert!(content.contains("File 2 content"));
        assert_eq!(aggregator.file_count(), 2);
    }

    #[test]
    fn test_aggregate_nonexistent_path() {
        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let result = aggregator.aggregate_paths(&["nonexistent_file.txt".to_string()]);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path does not exist"));
    }

    #[test]
    fn test_skip_hidden_files_in_directory() {
        let dir = tempdir().unwrap();
        let visible_file = dir.path().join("visible.txt");
        let hidden_file = dir.path().join(".hidden.txt");
        
        fs::write(&visible_file, "Visible content").unwrap();
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let content = aggregator.aggregate_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Visible content"));
        assert!(!content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 1);
    }

    #[test]
    fn test_include_hidden_files_in_directory_with_flag() {
        let dir = tempdir().unwrap();
        let visible_file = dir.path().join("visible.txt");
        let hidden_file = dir.path().join(".hidden.txt");
        
        fs::write(&visible_file, "Visible content").unwrap();
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, true, vec![]);
        let content = aggregator.aggregate_paths(&[dir.path().to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Visible content"));
        assert!(content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 2);
    }

    #[test]
    fn test_always_read_hidden_file_when_explicitly_provided() {
        let dir = tempdir().unwrap();
        let hidden_file = dir.path().join(".hidden.txt");
        fs::write(&hidden_file, "Hidden content").unwrap();

        let mut aggregator = ContentAggregator::new(false, false, false, vec![]);
        let content = aggregator.aggregate_paths(&[hidden_file.to_str().unwrap().to_string()]).unwrap();

        assert!(content.contains("Hidden content"));
        assert_eq!(aggregator.file_count(), 1);
    }
} 

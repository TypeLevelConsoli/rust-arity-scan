use std::collections::BinaryHeap;
use std::env;
use std::fmt::Display;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tree_sitter::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};
use walkdir::WalkDir;

const QUERY_SOURCE: &str = r#"
    (function_item
      name: (identifier) @function_name
      parameters: (parameters) @params)
    
    (function_signature_item
      name: (identifier) @function_name
      parameters: (parameters) @params)
    "#;

fn parse_args() -> (PathBuf, usize) {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <directory> <min_args>", args[0]);
        std::process::exit(1);
    }

    let directory = PathBuf::from(&args[1]);
    let min_args = args[2]
        .parse::<usize>()
        .expect("Minimum arguments must be a number");

    (directory, min_args)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, min_args) = parse_args();

    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

    let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), QUERY_SOURCE)?;

    let mut total_files = 0;

    let mut bucket = BinaryHeap::new();

    for entry in WalkDir::new(&directory)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let path = entry.path();
        let file_count = process_file(
            &directory,
            &path,
            &mut parser,
            &query,
            min_args,
            &mut bucket,
        )?;
        total_files += file_count;
    }

    for el in bucket {
        println!("{el}");
    }

    println!("\nFound {total_files} functions with more than {min_args} arguments");

    Ok(())
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct FnInfo {
    path: PathBuf,
    name: String,
    arity: usize,
    line: usize,
}

impl PartialOrd for FnInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.arity.partial_cmp(&other.arity)
    }
}
impl Ord for FnInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.arity.cmp(&other.arity)
    }
}

impl Display for FnInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: fn {}/{}",
            self.path.display(),
            self.line,
            self.name,
            self.arity
        )
    }
}

fn process_file(
    base: &Path,
    path: &Path,
    parser: &mut Parser,
    query: &Query,
    min_args: usize,
    bucket: &mut BinaryHeap<FnInfo>,
) -> Result<usize, Box<dyn std::error::Error>> {
    let source_code = fs::read_to_string(path)?;
    let tree = parser
        .parse(&source_code, None)
        .expect(&format!("FAILED TO PARSE file {}", &source_code));

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source_code.as_bytes());

    let function_name_idx = query.capture_index_for_name("function_name").unwrap_or(0);
    let params_idx = query.capture_index_for_name("params").unwrap_or(0);

    let mut count = 0;

    while let Some(m) = matches.next() {
        let name_capture = m.captures.iter().find(|c| c.index == function_name_idx);

        let params_capture = m.captures.iter().find(|c| c.index == params_idx);

        if let (Some(name), Some(params)) = (name_capture, params_capture) {
            let name = source_code[name.node.byte_range()].to_owned();
            let params_node = params.node;

            let arity = count_parameters(&params_node);

            let path = path.strip_prefix(base).unwrap().to_path_buf();
            if arity > min_args {
                let line = params_node.start_position().row + 1;
                bucket.push(FnInfo {
                    path,
                    name,
                    arity,
                    line,
                });
                count += 1;
            }
        }
    }

    Ok(count)
}

fn count_parameters(params_node: &tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut cursor = params_node.walk();

    // Count each parameter (skipping self if it's a method)
    for child in params_node.children(&mut cursor) {
        if child.kind() == "self_parameter" {
            continue; // Skip self parameter
        }
        if child.kind() == "parameter" {
            count += 1;
        }
    }

    count
}

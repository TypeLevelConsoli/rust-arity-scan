use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tree_sitter::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};
use walkdir::WalkDir;

fn parse_args() -> (String, usize) {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <directory> <min_args>", args[0]);
        std::process::exit(1);
    }

    let directory = args[1].clone();
    let min_args = args[2]
        .parse::<usize>()
        .expect("Minimum arguments must be a number");

    (directory, min_args)
}

const QUERY_SOURCE: &str = r#"
    (function_item
      name: (identifier) @function_name
      parameters: (parameters) @params)
    
    (function_signature_item
      name: (identifier) @function_name
      parameters: (parameters) @params)
    "#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (directory, min_args) = parse_args();

    // Initialize tree-sitter
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

    // Create a Tree-sitter query to find functions
    // The query captures function signature, name, and parameters list

    let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), QUERY_SOURCE)?;

    // Iterate through Rust files in the directory
    let mut total_count = 0;
    println!(
        "Searching for functions with more than {} arguments in {}",
        min_args, directory
    );
    println!("--------------------------------------------------");

    let directory = PathBuf::from(directory);
    for entry in WalkDir::new(&directory)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let path = entry.path();
        let file_count = process_file(&directory, &path, &mut parser, &query, min_args)?;
        total_count += file_count;
    }

    println!("--------------------------------------------------");
    println!(
        "Found {} functions with more than {} arguments",
        total_count, min_args
    );

    Ok(())
}

fn process_file(
    base: &Path,
    path: &Path,
    parser: &mut Parser,
    query: &Query,
    min_args: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    let source_code = fs::read_to_string(path)?;
    let tree = parser
        .parse(&source_code, None)
        .expect(&format!("FAILED TO PARSE file {}", &source_code));

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source_code.as_bytes());

    let function_name_idx = query.capture_index_for_name("function_name").unwrap_or(0);
    let method_name_idx = query.capture_index_for_name("method_name").unwrap_or(0);
    let params_idx = query.capture_index_for_name("params").unwrap_or(0);

    let mut count = 0;

    while let Some(m) = matches.next() {
        // Get function/method name
        let name_capture = m
            .captures
            .iter()
            .find(|c| c.index == function_name_idx || c.index == method_name_idx);

        let params_capture = m.captures.iter().find(|c| c.index == params_idx);

        if let (Some(name), Some(params)) = (name_capture, params_capture) {
            let name_str = &source_code[name.node.byte_range()];
            let params_node = params.node;

            let arg_count = count_parameters(&params_node);

            let path = path.strip_prefix(base).unwrap();
            if arg_count > min_args {
                let line = params_node.start_position().row + 1;
                println!(
                    "{} args\t{:4}| fn {}\t{}",
                    arg_count,
                    line,
                    name_str,
                    path.display(),
                );
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

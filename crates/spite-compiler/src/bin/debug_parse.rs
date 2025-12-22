//! Debug script to see what the parser produces.

use std::path::Path;
use spite_compiler::frontend::typescript::parser::TypeScriptParser;

fn main() {
    let source = r#"
export type TodoEvent =
  | { type: "Created"; id: string; title: string }
  | { type: "Completed"; completedAt: string }
  | { type: "TitleUpdated"; title: string };
"#;

    let mut parser = TypeScriptParser::new().unwrap();
    let result = parser.parse(source, Path::new("test.ts"));

    match result {
        Ok(parsed) => {
            println!("Parsed file: {:?}", parsed.path);
            println!("\nType aliases:");
            for alias in &parsed.type_aliases {
                println!("  {} (exported: {})", alias.name, alias.exported);
                println!("    type_node: {:?}", alias.type_node);
            }
            println!("\nClasses:");
            for class in &parsed.classes {
                println!("  {}", class.name);
            }
        }
        Err(e) => {
            println!("Error: {:?}", e);
        }
    }
}

use fallingsand_sim::Server;
use rustyline::error::ReadlineError;
use rustyline::{DefaultEditor, Result};

fn main() -> Result<()> {
    println!("Starting server!");
    let mut server = Server::new();

    server.run();

    let mut rl = DefaultEditor::new()?;
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => match line.to_ascii_lowercase().as_str() {
                "exit" => {
                    server.stop();
                    break;
                }
                _ => {
                    println!("Invalid command: {}", line);
                }
            },
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                server.stop();
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}

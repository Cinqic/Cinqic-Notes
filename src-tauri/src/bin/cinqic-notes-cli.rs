use cinqic_notes_lib::{domain::NoteFormat, storage::Library};
use serde::Serialize;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cinqic-notes-cli: {error}");
            eprintln!("Run `cinqic-notes-cli help` for usage.");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(library_path) = args.first() else {
        print_help();
        return Ok(());
    };
    let Some(command) = args.get(1) else {
        print_help();
        return Ok(());
    };
    if command == "help" {
        print_help();
        return Ok(());
    }
    let library = Library::open(library_path).map_err(|error| error.to_string())?;
    let values = &args[2..];
    match command.as_str() {
        "list" => print_json(
            &library
                .list_notes_filtered(false, true)
                .map_err(|error| error.to_string())?,
        ),
        "search" => print_json(
            &library
                .search_notes_filtered(&values.join(" "), false, true)
                .map_err(|error| error.to_string())?,
        ),
        "get" => {
            let path = required(values, 0, "note path")?;
            print_json(&library.get_note(path).map_err(|error| error.to_string())?)
        }
        "create" => {
            let title = required(values, 0, "title")?;
            let folder = values.get(1).map(String::as_str).unwrap_or("");
            let format = parse_format(values.get(2).map(String::as_str).unwrap_or("md"))?;
            print_json(
                &library
                    .create_note(title, format, folder)
                    .map_err(|error| error.to_string())?,
            )
        }
        "update" => {
            let path = required(values, 0, "note path")?;
            let content = required(values, 1, "content")?;
            print_json(
                &library
                    .update_note(path, content, None, "cli")
                    .map_err(|error| error.to_string())?,
            )
        }
        "append" => {
            let path = required(values, 0, "note path")?;
            let addition = required(values, 1, "content")?;
            let note = library.get_note(path).map_err(|error| error.to_string())?;
            let content = if note.content.is_empty() {
                addition.to_owned()
            } else if note.content.ends_with('\n') {
                format!("{}{}", note.content, addition)
            } else {
                format!("{}\n{}", note.content, addition)
            };
            print_json(
                &library
                    .update_note(path, &content, Some(&note.summary.hash), "cli")
                    .map_err(|error| error.to_string())?,
            )
        }
        "rename" => {
            let path = required(values, 0, "note path")?;
            let title = required(values, 1, "title")?;
            print_json(
                &library
                    .rename_note(path, title)
                    .map_err(|error| error.to_string())?,
            )
        }
        "move" => {
            let path = required(values, 0, "note path")?;
            let folder = values.get(1).map(String::as_str).unwrap_or("");
            print_json(
                &library
                    .move_note(path, folder)
                    .map_err(|error| error.to_string())?,
            )
        }
        "archive" => {
            let path = required(values, 0, "note path")?;
            let archived = values.get(1).map(String::as_str).unwrap_or("true") == "true";
            print_json(
                &library
                    .archive_note(path, archived)
                    .map_err(|error| error.to_string())?,
            )
        }
        "trash" => {
            let path = required(values, 0, "note path")?;
            library
                .trash_note(path)
                .map_err(|error| error.to_string())?;
            println!("ok")
        }
        "restore" => {
            let path = required(values, 0, "note path")?;
            print_json(
                &library
                    .restore_note(path)
                    .map_err(|error| error.to_string())?,
            )
        }
        "tasks" => print_json(
            &library
                .list_tasks(true)
                .map_err(|error| error.to_string())?,
        ),
        "tags" => print_json(&library.list_tags().map_err(|error| error.to_string())?),
        "backlinks" => {
            let path = required(values, 0, "note path")?;
            print_json(&library.backlinks(path).map_err(|error| error.to_string())?)
        }
        "graph" => print_json(&library.graph().map_err(|error| error.to_string())?),
        "export" => {
            let path = required(values, 0, "note path")?;
            let destination = required(values, 1, "destination")?;
            let format = required(values, 2, "format")?;
            library
                .export_note(path, destination, format)
                .map_err(|error| error.to_string())?;
            println!("ok")
        }
        _ => return Err(format!("unknown command: {command}")),
    }
    Ok(())
}

fn required<'a>(values: &'a [String], index: usize, name: &str) -> Result<&'a str, String> {
    values
        .get(index)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing {name}"))
}

fn parse_format(value: &str) -> Result<NoteFormat, String> {
    match value.to_ascii_lowercase().as_str() {
        "md" | "markdown" => Ok(NoteFormat::Markdown),
        "txt" | "text" => Ok(NoteFormat::Text),
        _ => Err("format must be md or txt".into()),
    }
}

fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("could not encode result: {error}"),
    }
}

fn print_help() {
    println!(
        "Usage: cinqic-notes-cli <library> <command> [arguments]\n\nCommands:\n  list\n  search <query>\n  get <path>\n  create <title> [folder] [md|txt]\n  update <path> <content>\n  append <path> <content>\n  rename <path> <title>\n  move <path> [folder]\n  archive <path> [true|false]\n  trash <path>\n  restore <path>\n  tasks\n  tags\n  backlinks <path>\n  graph\n  export <path> <destination> <md|txt|html>\n\nAll operations are local and use the same safe storage boundary as the desktop\napp. The CLI does not start a network listener; Juniper is not required."
    );
}

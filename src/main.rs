use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use serde_json::{Value, json};
use std::{env, fs, process};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'p', long)]
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let base_url = env::var("OPENROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

    let api_key = env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        eprintln!("OPENROUTER_API_KEY is not set");
        process::exit(1);
    });

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);

    let client = Client::with_config(config);

    let mut messages: Vec<Value> = Vec::new();
    messages.push(json!({
            "role": "user",
            "content": args.prompt
        }
    ));
    for _ in 0..10 {
        let response: Value = client
            .chat()
            .create_byot(json!({
                "messages": messages,
                "model": "anthropic/claude-haiku-4.5",
            "tools": [
    {
        "type": "function",
        "function": {
            "name": "Read",
            "description": "Read and return the contents of a file",
            "parameters": {
    "type": "object",
    "required": ["file_path"],
    "properties": {
        "file_path": {
            "type": "string",
            "description": "The path of the file to read"
        }
    }
}
        }
    },
    {
        "type": "function",
        "function": {
            "name": "Write",
            "description": "Write content to a file",
            "parameters": {
                "type": "object",
                "required": ["file_path", "content"],
                "properties": {
                    "file_path": { "type": "string", "description": "The path of the file to write to" },
                    "content": { "type": "string", "description": "The content to write to the file" }
                }
            }
        }
    },
    {
    "type": "function",
    "function": {
        "name": "Bash",
        "description": "Execute a shell command",
        "parameters": {
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                }
            }
        }
    }
}
],
            }))
            .await?;

        if let Some(tools) = response["choices"][0]["message"]["tool_calls"].as_array()
            && !tools.is_empty()
        {
            messages.push(response["choices"][0]["message"].clone());
            let tool_call = match &tools[0]["function"] {
                Value::Object(tool) => tool,
                _ => panic!("Invalid tool call"),
            };
            let tool_call_id = match &tools[0]["id"] {
                Value::String(id) => id,
                _ => panic!("Tool call id not provided or not string"),
            };
            let tool = match &tool_call["name"] {
                Value::String(name) => name,
                _ => panic!("Tool name must be a string"),
            };
            let args: Value = match &tool_call["arguments"] {
                Value::String(raw) => serde_json::from_str(raw.as_str()).unwrap(),
                _ => panic!("Invalid arguments"),
            };
            match tool.as_str() {
                "Read" => {
                    if let Value::String(path) = &args["file_path"] {
                        let content = fs::read_to_string(path).unwrap_or_default();
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id,
                            "content": content,
                        }));
                    }
                }
                "Write" => {
                    let file_path = args["file_path"]
                        .as_str()
                        .expect("file_path must be a string");
                    let content = args["content"].as_str().expect("content must be a string");

                    // Write content to file (create if not exists, overwrite if exists)
                    fs::write(file_path, content).expect("Failed to write file");

                    // Append the tool result to messages
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": format!("Created the file {}", file_path)
                    }));
                }
                "Bash" => {
                    let command = args["command"].as_str().expect("command must be a string");

                    // Execute the command
                    use std::process::Command;
                    let output = Command::new("sh").arg("-c").arg(command).output();

                    let result = match output {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            if !stderr.is_empty() {
                                format!("Error: {}", stderr)
                            } else {
                                stdout.to_string()
                            }
                        }
                        Err(e) => format!("Failed to execute command: {}", e),
                    };

                    // Push tool response to messages
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": result
                    }));
                }
                _ => panic!("Tool not implemented"),
            }
        } else {
            if let Some(content) = response["choices"][0]["message"]["content"].as_str() {
                println!("{}", content);
                break;
            }
        }
    }
    Ok(())
}

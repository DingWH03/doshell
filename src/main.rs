use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use rustyline::error::ReadlineError;
use rustyline::Editor;
use rustyline::history::DefaultHistory; 

#[derive(Debug)]
enum CommandType {
    Echo(String),
    Set(String, String),
    Cd(String),  
    If(String, String, String), // 用于 IF 命令的结构：变量、操作符、值
    For(String, Vec<String>),    // 用于 FOR 循环
    While(String, String, String), // 用于 WHILE 循环
    Shift,
    Goto(String),
    Label(String),
    Exit,
    External(Vec<String>),
    Invalid,
}

struct Shell {
    variables: HashMap<String, String>,
    labels: HashMap<String, usize>, // 存储标号位置，用于 GOTO
    current_path: String,      // 保存当前工作路径
}

impl Shell {
    fn new() -> Self {
        let current_path = env::current_dir().unwrap().to_str().unwrap().to_string();
        Shell {
            variables: HashMap::new(),
            labels: HashMap::new(),
            current_path,
        }
    }

    fn parse_command(&self, input: &str) -> CommandType {
        let parts: Vec<&str> = input.trim().split_whitespace().collect();

        if parts.is_empty() {
            return CommandType::Invalid;
        }

        match parts[0].to_uppercase().as_str() {
            "ECHO" => CommandType::Echo(parts[1..].join(" ")),
            "SET" => {
                if parts.len() != 3 {
                    return CommandType::Invalid;
                }
                CommandType::Set(parts[1].to_string(), parts[2].to_string())
            }
            "CD" => {
                if parts.len() != 2 {
                    return CommandType::Invalid;
                }
                CommandType::Cd(parts[1].to_string())
            }
            "IF" => {
                if parts.len() < 4 {
                    return CommandType::Invalid;
                }
                CommandType::If(parts[1].to_string(), parts[2].to_string(), parts[3..].join(" "))
            }
            "FOR" => {
                if parts.len() < 3 {
                    return CommandType::Invalid;
                }
                let var = parts[1].to_string();
                let values = parts[2..].iter().map(|s| s.to_string()).collect();
                CommandType::For(var, values)
            }
            "WHILE" => {
                if parts.len() < 4 {
                    return CommandType::Invalid;
                }
                CommandType::While(parts[1].to_string(), parts[2].to_string(), parts[3].to_string())
            }
            "SHIFT" => CommandType::Shift,
            "GOTO" => {
                if parts.len() != 2 {
                    return CommandType::Invalid;
                }
                CommandType::Goto(parts[1].to_string())
            }
            ":" => {
                if parts.len() != 2 {
                    return CommandType::Invalid;
                }
                CommandType::Label(parts[1].to_string())
            }
            "EXIT" => CommandType::Exit,
            _ => CommandType::External(parts.iter().map(|s| s.to_string()).collect()),
        }
    }

    fn execute_command(&mut self, command: CommandType, command_lines: &Vec<String>, index: &mut usize) {
        match command {
            CommandType::Echo(msg) => {
                let processed_msg = msg.replace("$", ""); // 去掉变量前的"$"符号
                let mut result_msg = processed_msg.clone();
            
                for var in processed_msg.split_whitespace() {
                    if var.starts_with('$') {
                        let var_name = &var[1..]; // 去掉变量前的"$"符号
                        if let Some(value) = self.variables.get(var_name) {
                            result_msg = result_msg.replace(var, value); // 替换变量名为变量值
                        } else {
                            result_msg = result_msg.replace(var, ""); // 如果变量不存在，替换为空字符串
                        }
                    }
                }
            
                println!("{}", result_msg);
            }
            
            CommandType::Set(var, value) => {
                self.variables.insert(var, value);
            }
            CommandType::Cd(path) => {
                let new_path = Path::new(&path);
                if let Ok(abs_path) = env::current_dir().unwrap().join(new_path).canonicalize() {
                    self.current_path = abs_path.to_str().unwrap().to_string();
                    env::set_current_dir(&self.current_path).unwrap();
                    println!("Changed directory to: {}", self.current_path);
                } else {
                    println!("Directory not found: {}", path);
                }
            }
            CommandType::If(var, op, value) => {
                if let Some(var_value) = self.variables.get(&var) {
                    let condition_met = match op.as_str() {
                        "==" => var_value == &value,
                        "!=" => var_value != &value,
                        // 新增更多操作符
                        "<" => var_value < &value,
                        ">" => var_value > &value,
                        "<=" => var_value <= &value,
                        ">=" => var_value >= &value,
                        _ => false,
                    };
                    if !condition_met {
                        *index += 1; // 跳过下一行
                    }
                }
            }
            CommandType::For(var, values) => {
                for value in values {
                    self.variables.insert(var.clone(), value.clone());
                    *index += 1;
                    if *index < command_lines.len() {
                        let next_command = self.parse_command(&command_lines[*index]);
                        self.execute_command(next_command, command_lines, index);
                    }
                }
            }
            CommandType::While(var, op, value) => {
                while let Some(var_value) = self.variables.get(&var) {
                    let condition_met = match op.as_str() {
                        "==" => var_value == &value,
                        "!=" => var_value != &value,
                        _ => false,
                    };
                    if !condition_met {
                        break;
                    }
                    *index += 1;
                    if *index < command_lines.len() {
                        let next_command = self.parse_command(&command_lines[*index]);
                        self.execute_command(next_command, command_lines, index);
                    }
                }
            }
            CommandType::Shift => {
                // 可以实现 Shift 逻辑，例如删除 args[1] 后的元素，左移所有参数
            }
            CommandType::Goto(label) => {
                if let Some(&line_number) = self.labels.get(&label) {
                    *index = line_number; // 跳转到标签行
                } else {
                    println!("Label '{}' not found", label);
                }
            }
            CommandType::External(args) => {
                // Check for a .BATCH file (case-insensitive)
                let cmd_path = Path::new(&self.current_path).join(&args[0]);
                let batch_file_path = cmd_path.with_extension("BATCH");

                if batch_file_path.exists() {
                    // Execute the batch file if it exists
                    self.run_batch_file(&batch_file_path.to_string_lossy());
                } else {
                    // Otherwise, attempt to execute the command
                    let cmd = cmd_path.to_str().unwrap_or(&args[0]);
                    let result = Command::new(cmd)
                        .args(&args[1..])
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .output();

                    match result {
                        Ok(output) => {
                            io::stdout().write_all(&output.stdout).unwrap();
                            io::stderr().write_all(&output.stderr).unwrap();
                        }
                        Err(e) => {
                            println!("Command not found or failed to execute: {}", e);
                        }
                    }
                }
            }
            CommandType::Exit => {
                println!("Exiting shell.");
                std::process::exit(0); // 退出程序
            }            
            CommandType::Invalid => {
                println!("Invalid command");
            }
            _ => {} // 其他命令无需修改
        }
    }
    

    fn run(&mut self) {
        // 使用 Editor 并指定 DefaultHistory
        let mut rl = Editor::<(), DefaultHistory>::new().expect("Failed to initialize editor");

        let mut index = 0; // 索引变量，仅用于命令解析
        loop {
            // 使用 rustyline 提供的读取功能，自动支持命令历史和快捷键
            let readline = rl.readline("> ");
            
            match readline {
                Ok(line) => {
                    rl.add_history_entry(line.as_str());  // 添加到历史记录
                    let command = self.parse_command(&line);
                    
                    // 在交互模式下，每次只执行单个命令行
                    let command_lines = vec![line.trim().to_string()];
                    
                    // 执行单个命令
                    self.execute_command(command, &command_lines, &mut index);
                    
                    // 重置 index 以确保下一个命令正确处理
                    index = 0;
                }
                Err(ReadlineError::Interrupted) => {
                    println!("CTRL+C pressed. Use 'exit' command to quit.");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    // 检测到 CTRL+D，优雅退出
                    println!("Exiting shell.");
                    break;
                }
                Err(err) => {
                    println!("Error reading line: {:?}", err);
                    break;
                }
            }
        }
    }

    fn run_batch_file(&mut self, file_path: &str) {
        let path = Path::new(&self.current_path).join(file_path);
        let content = fs::read_to_string(&path).expect("Failed to read the batch file");
        let command_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        for (i, line) in command_lines.iter().enumerate() {
            if let CommandType::Label(label) = self.parse_command(line) {
                self.labels.insert(label, i);
            }
        }

        let mut index = 0;
        while index < command_lines.len() {
            let command = self.parse_command(&command_lines[index]);
            self.execute_command(command, &command_lines, &mut index);
            index += 1;
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut shell = Shell::new();

    if args.len() > 1 {
        // 如果提供了文件路径参数，则执行 .BATCH 文件
        let file_path = &args[1];
        shell.run_batch_file(file_path);
    } else {
        // 否则，进入交互模式
        shell.run();
    }
}

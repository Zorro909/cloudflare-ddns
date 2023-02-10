use crate::cloudflare::DomainRegistration;
use crate::Args;
use serde_json::{from_str, to_string_pretty};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process;
use std::str::Lines;

include!(concat!(env!("OUT_DIR"), "/constants.rs"));

pub struct Config {
    config_file: PathBuf,
    domains_file: Option<PathBuf>,
    cloudflare_token: String,
    config_entries: HashMap<String, String>,
}

trait ConfigProcessor {
    fn process_comment(self: &mut Self, line: &str);
    fn process_config_entry(self: &mut Self, key: &str, value: &str);
}

struct ConfigReader<'a> {
    config: &'a mut Config,
}

impl<'a> ConfigReader<'a> {
    fn new(config: &'a mut Config) -> ConfigReader<'a> {
        Self { config }
    }
}

impl<'a> ConfigProcessor for ConfigReader<'a> {
    fn process_comment(self: &mut Self, _line: &str) {}

    fn process_config_entry(self: &mut Self, key: &str, value: &str) {
        self.config
            .config_entries
            .insert(key.to_string(), value.to_string());
    }
}

struct ConfigWriter {
    pub new_content: String,
    new_key: String,
    new_value: String,
}

impl ConfigProcessor for ConfigWriter {
    fn process_comment(self: &mut Self, line: &str) {
        self.new_content.push_str(line);
        self.new_content.push_str("\n");
    }
    fn process_config_entry(self: &mut Self, key: &str, value: &str) {
        let value = if self.new_key.as_str() == key {
            self.new_value.as_str()
        } else {
            value
        };

        self.new_content
            .push_str(format!("{}={}\n", key, value).as_str());
    }
}

impl Config {
    pub fn new(args: &Args) -> Config {
        let config_file_path = args
            .config_file
            .clone()
            .unwrap_or_else(|| DEFAULT_CONF_FILE.into());

        let mut config = Config {
            config_file: config_file_path,
            domains_file: args.domains_file.clone(),
            cloudflare_token: args.cloudflare_token.clone(),
            config_entries: HashMap::new(),
        };
        config.read_config();
        config
    }

    pub fn read_cloudflare_token(self: &Self) -> String {
        if self.cloudflare_token.len() > 0 {
            return self.cloudflare_token.clone();
        }

        self.read_config_entry("cloudflare_token")
            .expect("No Cloudflare Token found")
            .to_string()
    }

    fn read_domains_file_path(self: &Self) -> PathBuf {
        self.domains_file
            .clone()
            .or_else(|| {
                self.read_config_entry("domains_file")
                    .map(|v| v.into())
                    .clone()
            })
            .unwrap_or_else(|| "domains.json".into())
    }

    pub fn read_domains(self: &Self) -> Vec<DomainRegistration> {
        let file_name = self.read_domains_file_path();
        let contents: String = File::open(file_name.clone())
            .map(|mut file| {
                let mut contents = String::new();
                match file.read_to_string(&mut contents) {
                    Ok(_) => match contents.len() {
                        0 => "[]".to_string(),
                        _ => contents,
                    },
                    Err(e) => {
                        println!("Unable to read {:?} (Error: {})", file_name, e);
                        process::exit(1);
                    }
                }
            })
            .unwrap_or("[]".to_string());

        let result = from_str(contents.as_str());
        if result.is_err() {
            println!(
                "Unable to parse {:#?} (Error: {})",
                file_name,
                result.err().unwrap()
            );
            process::exit(1);
        }
        result.unwrap()
    }

    pub fn write_domains(self: &Self, domains: &Vec<DomainRegistration>) -> Result<(), String> {
        let domains_json =
            to_string_pretty(&domains).expect("Unable to serialize DomainRegistrations");
        let file_name = self.read_domains_file_path();
        File::create(file_name.clone())
            .and_then(|mut file| file.write_all(domains_json.as_bytes()))
            .map_err(|e| format!("Unable to write {:#?} (Error: {})", file_name, e))
    }

    pub fn read_config_entry(self: &Self, key: &str) -> Option<&String> {
        self.config_entries.get(key).clone()
    }

    pub fn set_config_entry(self: &Self, key: &str, value: &str) -> Result<(), String> {
        let contents: String = read_file(self.config_file.clone()).unwrap_or("".to_string());

        let mut config_writer = ConfigWriter {
            new_content: String::new(),
            new_key: key.to_string(),
            new_value: value.to_string(),
        };

        if !parse_config(contents.lines(), &mut config_writer) {
            return Err("Unable to parse config file".to_string());
        }

        let mut file = File::create(self.config_file.clone()).unwrap();
        file.write_all(config_writer.new_content.as_bytes())
            .map_err(|e| {
                format!(
                    "Unable to write config file {:#?} (Error: {})",
                    self.config_file, e
                )
            })
    }

    fn read_config(self: &mut Self) {
        let contents: String = read_file(self.config_file.clone()).unwrap_or("".to_string());

        let reader = &mut ConfigReader { config: self };

        parse_config(contents.lines(), reader);
    }
}

fn read_file(path: PathBuf) -> Result<String, String> {
    File::open(path)
        .and_then(|mut file| {
            let mut contents = String::new();
            match file.read_to_string(&mut contents) {
                Ok(_) => Ok(contents),
                Err(e) => Err(e),
            }
        })
        .map_err(|e| format!("Unable to read config file (Error: {})", e))
}

fn parse_config(mut lines: Lines, config_processor: &mut dyn ConfigProcessor) -> bool {
    // Extensive Support for comments and empty lines
    // Print very descriptive error message if config file is not valid
    let mut line_number = 0;
    loop {
        line_number += 1;
        let orig_line = match lines.next() {
            Some(l) => l,
            None => break,
        };

        let line = orig_line.trim();

        if line.len() == 0 || line.starts_with('#') {
            config_processor.process_comment(orig_line);
            continue;
        }
        // Improve comment support inside line
        let (comment, line) = match line.find('#') {
            Some(i) => (&line[i..], &line[0..i]),
            None => ("", line),
        };

        let mut parts = line.trim().split('=');
        let key = match parts.next() {
            Some(k) => k.trim(),
            None => {
                println!("Config file is not valid (Line {}: {})", line_number, line);
                return false;
            }
        };
        let value = match parts.next() {
            Some(v) => v.trim(),
            None => {
                println!("Config file is not valid (Line {}: {})", line_number, line);
                return false;
            }
        };

        if parts.next().is_some() {
            println!("Config file is not valid (Line {}: {})", line_number, line);
            return false;
        }

        config_processor.process_config_entry(key, value);

        if comment.len() > 0 {
            config_processor.process_comment(comment);
        }
    }
    return true;
}

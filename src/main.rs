extern crate core;

use crate::cloudflare::{CloudflareApi, DomainRegistration};
use crate::config::Config;
use clap::Parser;
use clap::Subcommand;
use core::num::dec2flt::parse::parse_number;
use prettytable::{format, row, Cell, Table};
use reqwest::blocking::Client;
use serde_json::Number;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub mod cloudflare;
pub mod config;

/// Simple program to greet a person
#[derive(Parser)]
#[command(name = "CloudflareDynDns")]
#[command(author = "Maximilian Kling <an@maximilian-kling.de>")]
#[command(version = "1.0")]
#[command(about = "Dynamic DNS Updates for Cloudflare Domains", long_about = None)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, env = "CONFIG_PATH")]
    config_file: Option<PathBuf>,
    #[arg(short, long, env = "DOMAINS_PATH")]
    domains_file: Option<PathBuf>,
    #[arg(long, env = "CLOUDFLARE_TOKEN", default_value = "")]
    cloudflare_token: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Lists all registered domains
    List {
        #[arg(long)]
        debug: bool,
    },
    /// Shows the status of a registered domain
    Status { domain: String },
    /// Deletes a registered domain
    Delete { domain: String },
    /// Registers a new domain
    Register {
        domain: String,
        /// Suffix for IPv4
        #[arg(short = '4', long)]
        v4_suffix: Option<String>,
        /// Suffix for IPv6
        #[arg(short = '6', long)]
        v6_suffix: Option<String>,
        #[arg(long)]
        disable_v4: bool,
        #[arg(long)]
        disable_v6: bool,
    },
    Update {
        #[arg(short, long)]
        force: bool,
    },
    Login {
        /// The token to store as authentication for the cloudflare api
        cloudflare_token: String,
    },
}

fn main() {
    let args = Args::parse();

    match &args.command {
        Commands::Register {
            domain,
            v4_suffix,
            disable_v4,
            v6_suffix,
            disable_v6,
        } => {
            register_domain(&args, domain, disable_v4, v4_suffix, disable_v6, v6_suffix);
        }
        Commands::List { debug } => {
            list_domains(&args, debug);
        }
        Commands::Update { force } => {
            update_domains(&args, force);
        }
        Commands::Delete { domain } => {
            delete_domain(&args, domain);
        }
        Commands::Login { cloudflare_token } => {
            login(&args, cloudflare_token);
        }
        _ => {}
    }
}

fn login(args: &Args, cloudflare_token: &String) {
    let config = Config::new(args);

    let mut cloudflare_client = CloudflareApi::new(cloudflare_token.clone());

    if cloudflare_client.fetch_cloudflare_zones().is_ok() {
        match config.set_config_entry("cloudflare_token", cloudflare_token) {
            Ok(_) => println!("Successfully logged in"),
            Err(e) => println!("Error while writing config file: {}", e),
        }
    } else {
        println!("Failed to login");
    }
}

fn delete_domain(args: &Args, domain: &String) {
    let config = Config::new(args);
    let mut domains = config.read_domains();

    let orig_length = domains.len();
    domains.retain(|x| x.domain != *domain);

    if domains.len() != orig_length {
        match config.write_domains(&domains) {
            Ok(_) => println!("Deleted domain '{}' successfully", domain),
            Err(e) => println!("Error while writing domains.json: {}", e),
        }
    } else {
        println!("Domain '{}' is not registered", domain);
    }
}

fn register_domain(
    args: &Args,
    domain: &String,
    disable_v4: &bool,
    v4_suffix: &Option<String>,
    disable_v6: &bool,
    v6_suffix: &Option<String>,
) {
    let config = Config::new(args);
    let mut domains = config.read_domains();

    //Check if domain is already registered
    for registered_domain in domains.iter() {
        if registered_domain.domain == *domain {
            println!("Domain '{}' is already registered", domain);
            return;
        }
    }

    let new_domain = DomainRegistration::new(domain, disable_v4, v4_suffix, disable_v6, v6_suffix);
    domains.push(new_domain);

    // Write the new domains.json file
    match config.write_domains(&domains) {
        Ok(_) => println!("Registered domain '{}' successfully", domain),
        Err(e) => println!("Error while writing domains.json: {}", e),
    }
}

fn list_domains(args: &Args, debug: &bool) {
    let config = Config::new(args);
    let domains = config.read_domains();

    let mut cloudflare_client: CloudflareApi = match *debug {
        true => CloudflareApi::new(config.read_cloudflare_token()),
        false => CloudflareApi::new(String::new()), // Token is not needed for listing domains
    };

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    let header = match *debug {
        true => row!["Domain", "IPv4", "ID4", "IPv6", "ID6"],
        false => row!["Domain", "IPv4", "IPv6"],
    };

    table.set_titles(header);

    for domain in domains.iter() {
        let v4_string: &str = match domain.v4_disabled {
            true => "Disabled",
            false => match domain.v4_suffix {
                Some(ref suffix) => suffix,
                None => "Default",
            },
        };
        let v6_string: &str = match domain.v6_disabled {
            true => "Disabled",
            false => match domain.v6_suffix {
                Some(ref suffix) => suffix,
                None => "Default",
            },
        };

        let mut row = row![domain.domain, v4_string, v6_string];
        if *debug {
            let domain_id_4 = cloudflare_client
                .fetch_cloudflare_dns_record(domain.domain.as_str(), "A")
                .map(|record| record.id.clone())
                .unwrap_or("Not Found".to_string());
            let domain_id_6 = cloudflare_client
                .fetch_cloudflare_dns_record(domain.domain.as_str(), "AAAA")
                .map(|record| record.id.clone())
                .unwrap_or("Not Found".to_string());
            row.insert_cell(2, Cell::new(domain_id_4.as_str()));
            row.insert_cell(4, Cell::new(domain_id_6.as_str()));
        }
        table.add_row(row);
    }
    table.printstd();
}

fn update_domains(args: &Args, force: &bool) {
    let config = Config::new(args);

    let domains = config.read_domains();

    let mut cloudflare_client = CloudflareApi::new(config.read_cloudflare_token());

    let v4_ip = get_ip("ipv4");
    let v6_ip = get_ip("ipv6");

    if config.read_config_entry("last_ipv4") == Some(&v4_ip)
        && config.read_config_entry("last_ipv6") == Some(&v6_ip)
        && !*force
    {
        let last_update = config
            .read_config_entry("last_update")
            .and_then(|v| v.parse::<u64>().ok());

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if last_update.unwrap_or(0) + 60 * 60 * 12 > now {
            println!("IP addresses have not changed, skipping update");
            return;
        } else {
            println!("IP addresses have not changed, but it has been more than 12 hours since the last update, updating anyway");
        }
    }

    for domain_registration in domains.iter() {
        if !domain_registration.v4_disabled {
            let new_ip = match domain_registration.v4_suffix {
                Some(ref suffix) => replace_ipv4_suffix(&v4_ip, suffix),
                None => v4_ip.clone(),
            };

            check_and_conditionally_update_domain(
                &mut cloudflare_client,
                domain_registration.domain.as_str(),
                "A",
                &new_ip,
                force,
            );
        }

        if !domain_registration.v6_disabled {
            let new_ip = match domain_registration.v6_suffix {
                Some(ref suffix) => replace_ipv6_suffix(&v6_ip, suffix),
                None => v6_ip.clone(),
            };

            check_and_conditionally_update_domain(
                &mut cloudflare_client,
                domain_registration.domain.as_str(),
                "AAAA",
                &new_ip,
                force,
            );
        }
    }
}

fn check_and_conditionally_update_domain(
    cloudflare_client: &mut CloudflareApi,
    name: &str,
    record_type: &str,
    new_ip: &str,
    force: &bool,
) {
    let (old_ip, is_error) = cloudflare_client
        .fetch_cloudflare_dns_record(name, record_type)
        .map(|record| (record.content.clone(), false))
        .unwrap_or(("No DNS Record Found".to_string(), true));

    if is_error {
        println!(
            "{}: {} (Update IP: {})",
            name, "No DNS Record Found", new_ip
        );
    } else {
        if old_ip != new_ip || *force {
            let result = cloudflare_client.update_cloudflare_dns_record(name, record_type, &new_ip);
            if result.is_err() {
                println!(
                    "{}: {} (Update IP: {})",
                    name, "Failed to update DNS Record", new_ip
                );
            } else {
                println!("{}: {} -> {}", name, old_ip, new_ip);
            }
        }
    }
}

fn get_ip(ip_type: &str) -> String {
    // Get the public ip address of the machine via icanhazip.com
    let client = Client::new();
    let url = format!("https://{}.icanhazip.com", ip_type);
    let response = client
        .get(url)
        .send()
        .expect("Unable to fetch data from icanhazip.com")
        .text()
        .expect("Unable to parse response from icanhazip.com");
    response.trim().to_string()
}

fn replace_ipv4_suffix(ip: &str, suffix: &str) -> String {
    // Replace the end of the ipv4 address with the given suffix
    let mut ip_parts: Vec<&str> = ip.split(".").collect();
    let suffix_parts: Vec<&str> = suffix.split(".").collect();
    ip_parts.splice(ip_parts.len() - suffix_parts.len().., suffix_parts);
    ip_parts.join(".")
}

fn replace_ipv6_suffix(ip: &str, suffix: &str) -> String {
    // Expand ipv6 address if it contains a ::
    let mut ip_str = ip.to_string();
    if ip_str.contains("::") {
        let mut ip_parts: Vec<&str> = ip_str.split("::").collect();
        let mut suffix_parts: Vec<&str> = ip_parts[1].split(":").collect();
        let mut prefix_parts: Vec<&str> = ip_parts[0].split(":").collect();
        let mut missing_parts = 8 - prefix_parts.len() - suffix_parts.len();
        while missing_parts > 0 {
            prefix_parts.push("0000");
            missing_parts -= 1;
        }
        ip_parts = prefix_parts;
        ip_parts.append(&mut suffix_parts);
        ip_str = ip_parts.join(":");
    }

    // Replace the end of the ipv6 address with the given suffix
    let mut ip_parts: Vec<&str> = ip_str.split(":").collect();
    let suffix_parts: Vec<&str> = suffix.split(":").collect();
    ip_parts.splice(ip_parts.len() - suffix_parts.len().., suffix_parts);
    ip_parts.join(":")
}

use base64::{
    alphabet::Alphabet,
    Engine,
    engine::GeneralPurposeConfig,
    engine::general_purpose::GeneralPurpose, 
};
use chrono::Utc;
use clap::Parser;
use md5;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{
    Serialize, 
    Deserialize
};
use std::{
    collections::HashMap, 
    env, 
    fs::File, 
    io::{self, BufRead}, 
    path::Path, 
    sync::Arc,
    time::Duration
};
use tokio::sync::Semaphore;
use url::form_urlencoded;

static EC_URLSIG_KEY: Lazy<String> = Lazy::new(|| { 
    env::var("EC_URLSIG_KEY").expect("EC_URLSIG_KEY must be set") 
});
static EC_URLSIG_KEY_VER: Lazy<String> = Lazy::new(|| { 
    number_to_letter(
        env::var("EC_URLSIG_KEY_VER").expect("EC_URLSIG_KEY_VER must be set")
    ) 
});

static EC_MAIL_URLSIG_KEY: Lazy<String> = Lazy::new(|| { 
    env::var("EC_MAIL_URLSIG_KEY").expect("EC_MAIL_URLSIG_KEY must be set") 
});
static EC_MAIL_URLSIG_KEY_VER: Lazy<String> = Lazy::new(|| { 
    number_to_letter(
        env::var("EC_MAIL_URLSIG_KEY_VER").expect("EC_MAIL_URLSIG_KEY_VER must be set")
    ) 
});

static Y64_ENCODE_ALPHABET: Lazy<Alphabet> = Lazy::new(|| {
    Alphabet::new("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._")
        .expect("Invalid base64 alphabet")
});

static Y64_ENCODE_ENGINE: Lazy<GeneralPurpose> = Lazy::new(|| {
    GeneralPurpose::new(&Y64_ENCODE_ALPHABET, GeneralPurposeConfig::new())
});

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    input: String,

    #[arg(short, long)]
    output: String,

    #[arg(long, default_value_t = 10)]
    threads: usize,

    #[arg(long, default_value_t = 30)]
    timeout: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct Result {
    external_status: u16,
    ec_proxy_status: u16,
    mail_proxy_status: u16
}

fn number_to_letter(num_str: String) -> String {
    ((b'A' + num_str.parse::<u8>().unwrap() - 1) as char).to_string()
}

fn y64_encode<T: AsRef<[u8]>>(src: T) -> String {
    Y64_ENCODE_ENGINE.encode(src.as_ref()).replace("=", "-")
}

fn sign_url(url: &str, key: &str, ver: &str) -> String {
    let combined = format!("{}{}", url, key);
    let digest = md5::compute(combined.as_bytes());
    let base64_encoded = y64_encode(&digest.0);
    format!("{}~{}", base64_encoded, ver)
}

fn generate_proxy_url(proxy_url: &str, base_url: &str, key: &str, ver: &str) -> String {
    let encoded_url = form_urlencoded::Serializer::new(String::new())
        .append_pair("url", base_url)
        .finish();
    let url_to_sign = format!("{}?{}&t={}", proxy_url, encoded_url, Utc::now().timestamp());
    let signature = sign_url(&url_to_sign, key, ver);
    format!("{}&sig={}", url_to_sign, signature)
}

async fn process_url(url: String, semaphore: &Semaphore, client: &Client) -> (String, Result) {
    let _permit = semaphore.acquire().await.unwrap();

    let external_status = client.get(&url)
        .send()
        .await
        .map_or(0, |resp| resp.status().as_u16());

    let ec_proxy_url = generate_proxy_url("https://ec.yimg.com/ec", &*url, &EC_URLSIG_KEY, &EC_URLSIG_KEY_VER);
    println!("{}", ec_proxy_url);
    let ec_proxy_status = client.get(ec_proxy_url)
        .send()
        .await
        .map_or(0, |resp| resp.status().as_u16());

    let mail_proxy_url = generate_proxy_url("https://ecp.yusercontent.com/mail", &*url, &EC_MAIL_URLSIG_KEY, &EC_MAIL_URLSIG_KEY_VER);
    println!("{}", mail_proxy_url);
    let mail_proxy_status = client.get(mail_proxy_url)
        .send()
        .await
        .map_or(0, |resp| resp.status().as_u16());

    (url, Result { external_status, ec_proxy_status, mail_proxy_status })
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

#[tokio::main]
async fn main() {
    let env_vars = [
        ( "EC_URLSIG_KEY", &EC_URLSIG_KEY),  
        ( "EC_URLSIG_KEY_VER", &EC_URLSIG_KEY_VER ), 
        ( "EC_MAIL_URLSIG_KEY", &EC_MAIL_URLSIG_KEY ), 
        ( "EC_MAIL_URLSIG_KEY_VER", &EC_MAIL_URLSIG_KEY_VER )
    ];

    for &(name, value) in &env_vars {
        if value.is_empty() {
            eprintln!("Env var {} is not set!", name);
            std::process::exit(1);
        }
    }

    let args = Args::parse();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(args.timeout))
        .build()
        .unwrap();

    let semaphore = Arc::new(Semaphore::new(args.threads));
    let mut handles = Vec::new();

    if let Ok(lines) = read_lines(args.input) {
        for line in lines {
            if let Ok(url) = line {
                let semaphore_clone = Arc::clone(&semaphore);
                let client_clone = Client::clone(&client);

                handles.push(tokio::spawn(async move {
                    process_url(url, &semaphore_clone, &client_clone).await
                }));
            }
        }
    }

    let mut results = HashMap::new();
    for handle in handles {
        let (url, info) = handle.await.unwrap();
        results.insert(url, info);
    }

    let json = serde_json::to_string_pretty(&results).unwrap();
    std::fs::write(args.output, json).unwrap();
}

use std::{collections::HashMap, path::Path, thread, time::Duration};

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const CLIENT_ID: &'static str = "qhh20sm8ceyh4m7qc84458l943crh8";

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    expires_in: u32,
    interval: u32,
    user_code: String,
    verification_uri: String,
}

#[derive(Serialize, Deserialize)]
struct TokenGrantResponse {
    access_token: String,
    expires_in: u32,
    refresh_token: String,
    scope: Vec<String>,
    token_type: String,
}

#[derive(Serialize, Deserialize)]
struct TokenValidateResponse {
    client_id: String,
    login: String,
    scopes: Vec<String>,
    user_id: String,
    expires_in: u32,
}

#[derive(Deserialize)]
struct FollowedChannelsResponse {
    data: Vec<Stream>,
}

#[derive(Deserialize)]
struct Stream {
    id: String,
    user_id: String,
    user_login: String,
    user_name: String,
    game_id: String,
    game_name: String,
    // skipping the "type" field because it's useless and a reserved keyword in Rust.
    title: String,
    viewer_count: u32,
    started_at: String,
    thumbnail_url: String,
    // skipping the "tag_ids" field because it's deprecated and always empty
    tags: Vec<String>,
    is_mature: bool,
}

fn main() -> Result<()> {
    let client = reqwest::blocking::Client::new();

    let project_dirs = ProjectDirs::from("dev", "cosmicrose", "scritch").unwrap();
    let validate_cache_path = project_dirs.cache_dir().join("validate.toml");
    let tokens_path = project_dirs.data_dir().join("tokens.toml");

    let token_grants: TokenGrantResponse = if tokens_path.exists() {
        let token_file_contents = std::fs::read_to_string(&tokens_path)?;

        toml::from_str(&token_file_contents)?
    } else {
        let device_code_response: DeviceCodeResponse = client
            .post("https://id.twitch.tv/oauth2/device")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body("client_id=qhh20sm8ceyh4m7qc84458l943crh8&scopes=user%3Aread%3Afollows")
            .send()?
            .json()?;

        println!(
            "Please log in and accept the authorization request at this URL: {}",
            device_code_response.verification_uri
        );

        let mut token_grant_params = HashMap::new();
        token_grant_params.insert("client_id", CLIENT_ID);
        token_grant_params.insert("scopes", "user:read:follows");
        token_grant_params.insert("device_code", &device_code_response.device_code);
        token_grant_params.insert("grant_type", "urn:ietf:params:oauth:grant-type:device_code");

        println!("device code: {}", &device_code_response.device_code);

        let mut token_grant_response = client
            .post("https://id.twitch.tv/oauth2/token")
            .form(&token_grant_params)
            .send()?;

        while token_grant_response.status() != 200 {
            println!("status is {}", token_grant_response.status());
            println!("body is {}", token_grant_response.text()?);
            thread::sleep(Duration::from_secs(1));

            token_grant_response = client
                .post("https://id.twitch.tv/oauth2/token")
                .form(&token_grant_params)
                .send()?;
        }

        let token_grants: TokenGrantResponse = token_grant_response.json()?;
        let tokens_file_contents = toml::to_string(&token_grants)?;
        std::fs::create_dir_all(tokens_path.parent().unwrap())?;
        std::fs::write(tokens_path, tokens_file_contents)?;

        token_grants
    };

    let token_validate_response = client
        .get("https://id.twitch.tv/oauth2/validate")
        .header(
            "Authorization",
            format!("Bearer {}", token_grants.access_token),
        )
        .send()?;

    if token_validate_response.status() != 200 {
        todo!();
    }

    let valid_token_response: TokenValidateResponse = token_validate_response.json()?;

    let followed_channels_response: FollowedChannelsResponse = client
        .get("https://api.twitch.tv/helix/streams/followed")
        .query(&[("user_id", valid_token_response.user_id)])
        .header("Client-Id", CLIENT_ID)
        .header(
            "Authorization",
            format!("Bearer {}", token_grants.access_token),
        )
        .send()?
        .json()?;

    println!("Current streams:");

    for stream in followed_channels_response.data {
        println!("{}: {}", stream.user_name, stream.title);
    }

    Ok(())
}

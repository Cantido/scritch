use std::{collections::HashMap, io::stdout, path::Path, thread, time::Duration};

use directories::ProjectDirs;
use miette::{Context, IntoDiagnostic, Result};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use textwrap::{fill, Options};

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

    let project_dirs = ProjectDirs::from("dev", "cosmicrose", "scritch")
        .expect("Project directories should be defined for this operating system");
    let tokens_path = project_dirs.data_dir().join("tokens.toml");

    let token_grants: TokenGrantResponse = if tokens_path.exists() {
        let token_file_contents = std::fs::read_to_string(&tokens_path)
            .into_diagnostic()
            .wrap_err("Failed to read token file")?;

        toml::from_str(&token_file_contents)
            .into_diagnostic()
            .wrap_err("Failed to serialize token to TOML")?
    } else {
        let device_code_response: DeviceCodeResponse = client
            .post("https://id.twitch.tv/oauth2/device")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body("client_id=qhh20sm8ceyh4m7qc84458l943crh8&scopes=user%3Aread%3Afollows")
            .send()
            .into_diagnostic()
            .wrap_err("Failed to make device code request")?
            .json()
            .into_diagnostic()
            .wrap_err("Failed to decode device code response")?;

        println!(
            "Please log in and accept the authorization request at this URL: {}",
            device_code_response.verification_uri
        );

        let mut token_grant_params = HashMap::new();
        token_grant_params.insert("client_id", CLIENT_ID);
        token_grant_params.insert("scopes", "user:read:follows");
        token_grant_params.insert("device_code", &device_code_response.device_code);
        token_grant_params.insert("grant_type", "urn:ietf:params:oauth:grant-type:device_code");

        let mut token_grant_response = client
            .post("https://id.twitch.tv/oauth2/token")
            .form(&token_grant_params)
            .send()
            .into_diagnostic()
            .wrap_err("Failed to make token grant request")?;

        while token_grant_response.status() != 200 {
            thread::sleep(Duration::from_secs(1));

            token_grant_response = client
                .post("https://id.twitch.tv/oauth2/token")
                .form(&token_grant_params)
                .send()
                .into_diagnostic()
                .wrap_err("Token grant request failed")?;
        }

        let token_grants: TokenGrantResponse = token_grant_response
            .json()
            .into_diagnostic()
            .wrap_err("Failed to parse token grant response")?;
        let tokens_file_contents = toml::to_string(&token_grants)
            .into_diagnostic()
            .wrap_err("Failed to serialize token")?;
        std::fs::create_dir_all(tokens_path.parent().unwrap())
            .into_diagnostic()
            .wrap_err("Failed to create directory to save tokens in")?;
        std::fs::write(tokens_path.clone(), tokens_file_contents)
            .into_diagnostic()
            .wrap_err("Failed to save tokens to file")?;

        token_grants
    };

    let token_validate_response = client
        .get("https://id.twitch.tv/oauth2/validate")
        .header(
            "Authorization",
            format!("Bearer {}", token_grants.access_token),
        )
        .send()
        .into_diagnostic()
        .wrap_err("Token validate response failed")?;

    let (access_token, user_id) = if token_validate_response.status() != 200 {
        let mut token_refresh_params = HashMap::new();
        token_refresh_params.insert("client_id", CLIENT_ID);
        token_refresh_params.insert("grant_type", "refresh_token");
        token_refresh_params.insert("refresh_token", &token_grants.refresh_token);

        let token_refresh_response: TokenGrantResponse = client
            .post("https://id.twitch.tv/oauth2/token")
            .form(&token_refresh_params)
            .send()
            .into_diagnostic()
            .wrap_err("Token refresh request failed")?
            .json()
            .into_diagnostic()
            .wrap_err("Failed to decode token refresh grant response")?;

        let tokens_file_contents = toml::to_string(&token_refresh_response)
            .into_diagnostic()
            .wrap_err("Failed to serialize token grant to TOML")?;
        std::fs::write(tokens_path.clone(), tokens_file_contents)
            .into_diagnostic()
            .wrap_err("Failed to save token")?;

        let token_validate_response: TokenValidateResponse = client
            .get("https://id.twitch.tv/oauth2/validate")
            .header(
                "Authorization",
                format!("Bearer {}", token_grants.access_token),
            )
            .send()
            .into_diagnostic()
            .wrap_err("Token validate request failed")?
            .json()
            .into_diagnostic()
            .wrap_err("Failed to parse token validation response during token refresh")?;

        (
            token_refresh_response.access_token,
            token_validate_response.user_id,
        )
    } else {
        let valid_token_response: TokenValidateResponse = token_validate_response
            .json()
            .into_diagnostic()
            .wrap_err("Failed to parse token validation reponse")?;
        (token_grants.access_token, valid_token_response.user_id)
    };

    let followed_channels_response: FollowedChannelsResponse = client
        .get("https://api.twitch.tv/helix/streams/followed")
        .query(&[("user_id", user_id)])
        .header("Client-Id", CLIENT_ID)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .into_diagnostic()
        .wrap_err("Followed channel request failed")?
        .json()
        .into_diagnostic()
        .wrap_err("Failed to parse followed channel response")?;

    println!("Current streams:");

    let wrap_options = Options::with_termwidth()
        .initial_indent("  ")
        .subsequent_indent("    ");

    for stream in followed_channels_response.data {
        println!("{}", stream.user_name.bold());

        println!(
            "{}",
            fill(
                &format!("{}: {}", "game".dimmed(), stream.game_name),
                wrap_options.clone(),
            )
        );
        println!(
            "{}",
            fill(
                &format!("{}: {}", "viewers".dimmed(), stream.viewer_count),
                wrap_options.clone(),
            )
        );

        println!(
            "{}",
            fill(
                &format!("{}: {}", "title".dimmed(), stream.title),
                wrap_options.clone(),
            )
        );
    }

    Ok(())
}

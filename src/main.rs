use structopt::StructOpt;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct EmojiAdminList {
    custom_emoji_total_count: u32,
    paging: Paging,
    ok: bool,

    emoji: Vec<Emoji>,

    #[serde(flatten)]
    unknown_fields: UnknownJSONFields,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct Emoji {
    name: String,
    is_alias: u8,
    alias_for: String,
    url: String,
    created: u128,
    user_display_name: String,
    avatar_hash: String,

    #[serde(flatten)]
    unknown_fields: UnknownJSONFields,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct Paging {
    count: u32,

    #[serde(flatten)]
    unknown_fields: UnknownJSONFields,
}

type UnknownJSONFields = std::collections::HashMap<String, serde_json::Value>;

#[derive(Debug)]
enum GetEmojiError {
    ApiResponse(UnknownJSONFields),
    Reqwest(reqwest::Error),
}

impl std::fmt::Display for GetEmojiError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &*self {
            GetEmojiError::ApiResponse(fields) => write!(
                f,
                "API responded with errors (partial response): {:?}",
                fields
            ),
            GetEmojiError::Reqwest(e) => write!(f, "API communication error: {:?}", e),
        }
    }
}

impl From<reqwest::Error> for GetEmojiError {
    fn from(err: reqwest::Error) -> GetEmojiError {
        GetEmojiError::Reqwest(err)
    }
}

fn get_emoji(workspace: String, token: String) -> Result<Vec<Emoji>, GetEmojiError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(format!(
            "m3t0r/slack-emoji ({})",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;

    let req = client
        .post(format!(
            "https://{}.slack.com/api/emoji.adminList",
            workspace
        ))
        .multipart(
            reqwest::blocking::multipart::Form::new()
                .text("page", "1")
                .text("count", "1")
                .text("token", std::borrow::Cow::Owned(token.clone())),
        )
        .build()?;

    eprintln!("Getting emoji count: {}", req.url());
    let res = client.execute(req)?.error_for_status()?;

    let admin_list: EmojiAdminList = res.json()?;
    if !admin_list.ok {
        return Err(GetEmojiError::ApiResponse(admin_list.unknown_fields));
    }
    let emoji_count = admin_list.custom_emoji_total_count;

    let req = client
        .post(format!(
            "https://{}.slack.com/api/emoji.adminList",
            workspace
        ))
        .multipart(
            reqwest::blocking::multipart::Form::new()
                .text("page", "1")
                .text("count", emoji_count.to_string())
                .text("token", std::borrow::Cow::Owned(token)),
        )
        .build()?;

    eprintln!("Getting emoji data: {}", req.url());
    let res = client.execute(req)?.error_for_status()?;

    let mut admin_list: EmojiAdminList = res.json()?;

    admin_list.emoji.sort_by_key(|e| e.created); // by creation date

    Ok(admin_list.emoji)
}

#[derive(StructOpt, Debug)]
#[structopt()]
/// Process Slack custom emoji
///
/// Lists all emoji in a workspace by default
struct Cli {
    /// Change to path before any operation
    #[structopt(short = "C", long = "directory", name = "path", parse(from_os_str))]
    change: Option<std::path::PathBuf>,

    #[structopt(subcommand)]
    command: Commands,
}

#[derive(StructOpt, Debug)]
enum Commands {
    /// Lists all custom emoji in a workspace
    List(ListOptions),
    /// Downloads all emoji images and metadata and store them in a folder
    Download,
}

#[derive(StructOpt, Debug)]
struct ListOptions {
    /// The workspace to list emoji for
    ///
    /// This is usually the subodmain like: https://<workspace>.slack.com
    #[structopt(long)]
    workspace: String,

    /// The authorization token
    ///
    /// Check the manual for a detailed explanation on how to get your token.
    #[structopt(long, env = "SLACK_TOKEN", hide_env_values = true)]
    token: String,

    /// Output to STDOUT as JSON values separated by newlines instead of writing to files
    #[structopt(long)]
    stdout: bool,
}

fn main() {
    let opt = Cli::from_args();

    match opt.command {
        Commands::List(list_opts) => {
            let emoji = match get_emoji(list_opts.workspace, list_opts.token) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Could not get emojis: {}", e);
                    std::process::exit(1);
                }
            };

            for e in &emoji {
                eprintln!("{} -> {}", e.name, e.url);
                match serde_json::to_string_pretty(&e) {
                    Ok(json) => println!("{}", json),
                    Err(error) => eprintln!("Could not serialize: {}: {:?}", error, e),
                };
            }
            eprintln!("Done! {} emoji in total", emoji.len());
        }
        Commands::Download => {
            todo!()
        }
    }
}

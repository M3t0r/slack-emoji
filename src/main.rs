use std::convert::TryInto;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
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

impl Emoji {
    #[allow(dead_code)]
    pub fn new(name: &str) -> Emoji {
        Emoji {
            name: name.to_string(),
            is_alias: 0,
            alias_for: "".into(),
            url: "https://cdn.example.com/emoji.png".into(),
            created: 133742069,
            user_display_name: "M3t0r".into(),
            avatar_hash: "0xdeadbeef".into(),
            unknown_fields: UnknownJSONFields::new(),
        }
    }
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
        .user_agent(format!("m3t0r/slack-emoji ({})", env!("CARGO_PKG_VERSION")))
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
    #[structopt(flatten)]
    global: GlobalOptions,

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
    #[structopt(flatten)]
    global: GlobalOptions,

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

    /// Where to write the JSON data to
    ///
    /// Directory or file path. Can be '-' to use STDOUT as file. Defaults to a directory with the same name as the workspace.
    #[structopt(long)]
    output: Option<PathBuf>,
}

#[derive(StructOpt, Debug)]
struct GlobalOptions {
    /// Be verbose
    #[structopt(long, short)]
    verbose: bool,
}

impl std::ops::Add for GlobalOptions {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            verbose: self.verbose || rhs.verbose,
        }
    }
}

enum FileOrDirectoryWriter {
    StdOut,
    File(File),
    Directory(PathBuf),
}

impl FileOrDirectoryWriter {
    pub fn write(&mut self, name: &String, serialized: String) -> std::io::Result<usize> {
        match self {
            FileOrDirectoryWriter::StdOut => {
                std::io::stdout().write((serialized + "\n").as_bytes())
            }
            FileOrDirectoryWriter::File(ref mut writer) => {
                writer.write((serialized + "\n").as_bytes())
            }
            FileOrDirectoryWriter::Directory(dir) => {
                if !dir.exists() {
                    std::fs::create_dir_all(&dir)?;
                }
                let content_size = serialized.len();
                let mut file_path = dir.join(name);
                file_path.set_extension("json");
                std::fs::write(file_path, (serialized + "\n").as_bytes())?;
                Ok(content_size + 1)
            }
        }
    }
}

impl std::convert::TryFrom<PathBuf> for FileOrDirectoryWriter {
    type Error = std::io::Error;
    fn try_from(pf: PathBuf) -> std::io::Result<Self> {
        if pf == PathBuf::from("-") {
            Ok(FileOrDirectoryWriter::StdOut)
        } else if pf.is_dir() || pf.to_string_lossy().ends_with(std::path::MAIN_SEPARATOR) {
            Ok(FileOrDirectoryWriter::Directory(pf))
        } else {
            Ok(FileOrDirectoryWriter::File(
                OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(pf)?,
            ))
        }
    }
}

fn main() {
    let opts = Cli::from_args();

    match opts.command {
        Commands::List(list_opts) => {
            let global_opts = list_opts.global + opts.global;

            let mut ford_writer: FileOrDirectoryWriter = match list_opts
                .output
                .unwrap_or(PathBuf::from(list_opts.workspace.clone() + "/"))
                .try_into()
            {
                Ok(ford_writer) => ford_writer,
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(2);
                }
            };

            let emoji = match get_emoji(list_opts.workspace, list_opts.token) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Could not get emojis: {}", e);
                    std::process::exit(1);
                }
            };
            // let emoji: Vec<Emoji> = vec![Emoji::new("blub"), Emoji::new("blab")];

            let pb = indicatif::ProgressBar::new(emoji.len().try_into().unwrap_or(std::u64::MAX));
            for e in pb.wrap_iter(emoji.iter()) {
                if global_opts.verbose {
                    pb.println(format!("{} -> {}", e.name, e.url));
                }
                match serde_json::to_string_pretty(e) {
                    Ok(s) => match ford_writer.write(&e.name, s) {
                        Ok(_) => (),
                        Err(error) => pb.println(format!("{}: Could not write: {}", e.name, error)),
                    },
                    Err(error) => pb.println(format!(
                        "{}: Could not serialize: {}: {:?}",
                        e.name, error, e
                    )),
                };
            }
            pb.finish_with_message(format!("Done! {} emoji in total", emoji.len()));
        }
        Commands::Download => {
            todo!()
        }
    }
}

#[cfg(test)]
mod ford_tests {
    use super::*;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn dash() {
        let ford: FileOrDirectoryWriter = PathBuf::from("-")
            .try_into()
            .expect("could not create writer");
        test_stdout(ford);
    }

    fn test_stdout(mut ford: FileOrDirectoryWriter) {
        assert!(match ford {
            FileOrDirectoryWriter::StdOut => true,
            _ => false,
        });
        assert_eq!(
            ford.write(&"stdout-test".to_string(), "test output".to_string())
                .expect("could not write"),
            12usize // 11 chars + 1 newline
        );
    }

    #[test]
    fn file() {
        let mut ford: FileOrDirectoryWriter = PathBuf::from("test-file")
            .try_into()
            .expect("could not create writer");
        assert_eq!(
            ford.write(&"file-test".to_string(), "test output".to_string())
                .expect("could not write test data"),
            12usize
        );
        assert_eq!(
            std::fs::read("test-file").expect("could not read test data to verify"),
            "test output\n".as_bytes()
        );
        std::fs::remove_file("test-file").expect("could not clean up test file");
    }

    #[test]
    fn dir_with_slash() {
        let dir = TestDir::new("test-dir/");
        let ford: FileOrDirectoryWriter = PathBuf::from(dir.path)
            .try_into()
            .expect("could not create writer");
        test_dir(ford, dir.path);
    }

    #[test]
    fn dir_with_existing_dir() {
        let dir = TestDir::new("existing-test-dir");
        std::fs::create_dir(dir.path)
            .expect("could not create test directory to test with an existing dir");

        let ford: FileOrDirectoryWriter = PathBuf::from(dir.path)
            .try_into()
            .expect("could not create writer");
        test_dir(ford, dir.path);

        std::fs::remove_dir_all(dir.path).unwrap();
    }

    fn test_dir(mut ford: FileOrDirectoryWriter, path: &Path) {
        assert!(ford.write(&"test-a".into(), "foo".into()).is_ok());
        assert!(ford.write(&"test-b".into(), "bar".into()).is_ok());

        assert_eq!(
            std::fs::read(path.join("test-a.json")).expect("could not read test data to verify"),
            "foo\n".as_bytes()
        );
        assert_eq!(
            std::fs::read(path.join("test-b.json")).expect("could not read test data to verify"),
            "bar\n".as_bytes()
        );
        assert!(path.is_dir());
    }

    struct TestDir<'a> {
        path: &'a std::path::Path,
    }

    impl<'a> TestDir<'a> {
        pub fn new(path: &'a str) -> TestDir<'a> {
            let test_dir = TestDir {
                path: std::path::Path::new(path),
            };
            if test_dir.path.is_dir() {
                std::fs::remove_dir_all(test_dir.path)
                    .expect("could not clean up test dir before starting");
            }
            // if it was a directory it doesn't exist anymore
            if test_dir.path.exists() {
                panic!("testing directory {:?} is not a directory", test_dir.path);
            }
            return test_dir;
        }
    }

    impl<'a> Drop for TestDir<'a> {
        fn drop(&mut self) {
            if self.path.is_dir() {
                std::fs::remove_dir_all(self.path)
                    .expect("could not clean up test dir after tests");
            }
        }
    }
}

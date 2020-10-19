use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Twitch Clip Downloader",
    about = "A utitlity to download twitch clips by a user, Set Loggin verbosity with RUST_LOG env var"
)]
pub struct Args {
    /// State file to resume from and save into
    #[structopt(long, parse(from_os_str))]
    pub state: Option<PathBuf>,

    /// Config File to resume from or create
    #[structopt(long, parse(from_os_str))]
    pub config: Option<PathBuf>,

    #[structopt(subcommand)]
    pub command: Commands,
}

#[derive(Debug, StructOpt)]
pub enum Commands {
    /// Only Check Authentication up to date
    #[structopt(name = "auth")]
    CheckAuth,
    /// Get clip info for all clips associated with a streamer
    ClipInfo {
        /// The account name of the streamer
        user: String,

        /// ClipInfo File to store info in
        /// If not provided, stored in 'clip_info/<user>.json'
        #[structopt(long, parse(from_os_str))]
        clips: Option<PathBuf>,
    },
    /// Get the download link for clips
    /// must provide either user or clips, clips take precedence
    DownloadLinks {
        /// The account name of the streamer
        user: Option<String>,

        /// ClipInfo file, defaults to 'clip_info/<user>.json'
        #[structopt(long, parse(from_os_str))]
        clips: Option<PathBuf>,
    },
    /// Download Clips
    /// must provide either user or clips, clips take precedence
    DownloadClips {
        /// User whos clips are to be downloaded
        user: Option<String>,

        /// ClipInfo file, defaults to 'clip_info/<user>.json'
        #[structopt(long, parse(from_os_str))]
        clips: Option<PathBuf>,
    },
}

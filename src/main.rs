use std::{borrow::Cow, collections::HashMap, fs::{self, File}, io::{self, BufReader, BufWriter, ErrorKind, BufRead}, path::{Path, PathBuf}};

use log::*;
use m3u::{EntryExt, ExtInf};
use path_slash::PathBufExt;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use structopt::StructOpt;

use m3trans::{Library, Playlist, PlaylistKind};

const TRACKS_DIR: &'static str = "tracks";
const PLAYLISTS_DIR: &'static str = "playlists";

#[derive(StructOpt)]
struct Args {
    #[structopt(parse(from_os_str))]
    library_file: PathBuf,
    #[structopt(parse(from_os_str))]
    output_path: PathBuf,
    #[structopt(short, default_value = "./m3trans.log", parse(from_os_str))]
    log_file: PathBuf,
    #[structopt(short, long = "dry-run")]
    dry_run: bool,
    #[structopt(long = "ignore-file", parse(from_os_str))]
    ignore_file: Option<PathBuf>,
}

fn main() -> Result<(), std::io::Error> {
    let args = Args::from_args();
    let file = File::open(&args.library_file).unwrap();
    let library = Library::from_raw(
        plist::from_reader_xml(BufReader::new(file)).unwrap()
    ).unwrap();

    let log_file = File::create(&args.log_file).unwrap();
    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Warn, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        WriteLogger::new(LevelFilter::Info, Config::default(), log_file),
    ]).unwrap();

    let ignore_path = match &args.ignore_file {
        Some(path) => Cow::Borrowed(path),
        None => Cow::Owned(args.library_file.with_file_name(".m3ignore")),
    };

    // error!("Failed to open playlist ignores file at {:?}: {:?}", args.ignore_file, e);
    let ignore_pats = parse_ignores(&ignore_path)?;
    let remote_paths = copy_tracks(&library, &args)?;
    copy_playlists(&library, &args, &remote_paths, &ignore_pats)?;

    Ok(())
}

fn parse_ignores(path: &Path) -> Result<Vec<glob::Pattern>, io::Error> {
    match File::open(path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut patterns = Vec::new();
            for line in reader.lines() {
                let line = line?;
                match glob::Pattern::new(&line) {
                    Ok(pattern) => patterns.push(pattern),
                    Err(e) => warn!("Discarding invalid ignore pattern \"{}\": {:?}", line, e),
                }
            }
            Ok(patterns)
        },
        Err(nf) if nf.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => return Err(e)
    }
}

fn copy_tracks(library: &Library, args: &Args) -> Result<HashMap<u64, PathBuf>, io::Error> {
    fs::create_dir_all(args.output_path.join(TRACKS_DIR))?;

    let mut remote_paths = HashMap::new();
    for (track_id, track) in &library.tracks {
        const LOCAL_PREFIX: &str = "file://localhost/";
        if let Some(local_path) = track.location.strip_prefix(LOCAL_PREFIX).map(PathBuf::from_slash) {
            let mut remote_path = PathBuf::from_slash(format!("{}/{}", TRACKS_DIR, track_id));
            if let Some(extension) = local_path.extension() {
                remote_path.set_extension(extension);
            }
            let full_remote_path = args.output_path.join(&remote_path);

            match args.dry_run {
                false => match fs::copy(&local_path, &full_remote_path) {
                    Ok(_) => { remote_paths.insert(*track_id, remote_path); }
                    Err(e) => error!(
                        "Failed to copy file at {:?} to {:?}: {:?}",
                        local_path, full_remote_path, e
                    )
                }
                true => info!("Copy file at {:?} to {:?}", local_path, full_remote_path),
            }
            
        }
        else {
            warn!("Ignoring path with unknown schema: {}", track.location);
        }
    }
    Ok(remote_paths)
}

fn copy_playlists(
    library: &Library, 
    args: &Args,
    remote_paths: &HashMap<u64, PathBuf>,
    ignore_pats: &[glob::Pattern],
) -> Result<(), io::Error> {
    let playlists_dir = args.output_path.join(PLAYLISTS_DIR);
    fs::create_dir_all(&playlists_dir)?;

    let mut last_depth = None;
    let mut current_path = args.output_path.join(&playlists_dir);
    let mut output_root_relative = PathBuf::new();
    library.visit_playlists(|id, depth| {   
        if let Some(last_depth) = last_depth {
            if depth <= last_depth {
                for _ in 0..=last_depth - depth {
                    current_path.pop();
                    output_root_relative.pop();
                }
            }
        }
        
        let playlist = &library.playlists[&id];     
        current_path.push(playlist.name.to_string());
        output_root_relative.push("..");

        let virtual_path = current_path.strip_prefix(&playlists_dir).unwrap();
        if !ignore_pats.iter().any(|pat| pat.matches_path(virtual_path)) {
            match playlist.kind {
                PlaylistKind::Generic => {
                    current_path.set_extension("m3u8");
                    match args.dry_run {
                        false => {
                            let result = write_m3u(
                                library, 
                                &output_root_relative, 
                                &remote_paths, 
                                playlist, 
                                &current_path
                            );

                            if let Err(e) = result {
                                error!(
                                    "Failed to generate m3u for playlist {} at path {:?}: {:?}", 
                                    playlist.name, current_path, e
                                );
                            }
                        }
                        true => info!(
                            "Generate m3u for playlist {} at path {:?}", 
                            playlist.name, current_path
                        ),
                    }
                    current_path.set_extension("");
                },
                PlaylistKind::Folder => match args.dry_run {
                    false => if let Err(e) = fs::create_dir(&current_path) {
                        error!(
                            "Failed to generate folder for playlist folder {} at path {:?}: {:?}", 
                            playlist.name, current_path, e
                        );
                    }
                    true => info!(
                        "Generate folder for playlist folder {} at path {:?}", 
                        playlist.name, current_path
                    ),
                },
                _ => {},
            }
        }
        else {
            info!("Ignoring playlist {} with path {:?}.", playlist.name, virtual_path);
        }

        last_depth = Some(depth);
    });

    Ok(())
}

fn write_m3u(
    library: &Library, 
    root_prefix: &Path,
    remote_paths: &HashMap<u64, PathBuf>,
    playlist: &Playlist,
    path: &Path
) -> Result<(), io::Error> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut writer = m3u::Writer::new_ext(writer)?;

    for track_id in &playlist.items {
        let track = &library.tracks[&track_id];
        writer.write_entry(&EntryExt {
            entry: m3u::path_entry(root_prefix.join(&remote_paths[&track_id])),
            extinf: ExtInf {
                name: track.name.to_owned(),
                duration_secs: track.duration_ms as f64 / 1000.0f64,
            }
        })?;
    }

    Ok(())
}
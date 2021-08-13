use std::{fs::File, io::BufReader, path::PathBuf};
use structopt::StructOpt;
use m3trans::Library;

fn main() {
    let args = Args::from_args();
    let file = File::open(&args.library_file).unwrap();
    let library = Library::from_raw(
        plist::from_reader_xml(BufReader::new(file)).unwrap()
    ).unwrap();

    let mut last_depth = None;
    let mut current_path = PathBuf::from("/");
    library.visit_playlists(|id, depth| {
        if let Some(last_depth) = last_depth {
            if depth <= last_depth {
                for _ in 0..=last_depth - depth {
                    current_path.pop();
                }
            }
        }

        let playlist = &library.playlists[&id];     
        current_path.push(playlist.name.to_string());
        println!("Path: {:?} Kind: {:?}", current_path, playlist.kind);

        last_depth = Some(depth);
    });
}

#[derive(StructOpt)]
struct Args {
    #[structopt(parse(from_os_str))]
    library_file: PathBuf,
}
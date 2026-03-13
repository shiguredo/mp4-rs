use std::env;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};

const BUFFER_SIZE: usize = 1024 * 1024; // 1MB のバッファサイズ

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <mp4_file>", args[0]);
        std::process::exit(1);
    }

    let filepath = &args[1];
    if !Path::new(filepath).exists() {
        eprintln!("Error: Could not open file '{}'", filepath);
        std::process::exit(1);
    }

    // ファイルを開く（メモリ全体には読み込まない）
    let mut file = File::open(filepath)?;

    // デマルチプレックス処理を初期化
    let mut demuxer = Mp4FileDemuxer::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];

    // 初期化完了まで必要なデータを段階的に読み込む
    while let Some(required) = demuxer.required_input() {
        // 必要なデータを読み込む
        let data = read_from_file(&mut file, required.position, required.size, &mut buffer)?;
        let input = Input {
            position: required.position,
            data,
        };
        demuxer.handle_input(input);
    }

    // トラック情報を取得する
    let tracks = match demuxer.tracks() {
        Ok(tracks) => tracks,
        Err(e) => {
            eprintln!("Error: Failed to get tracks: {}", e);
            std::process::exit(1);
        }
    };

    println!("Found {} track(s)\n", tracks.len());

    // トラック情報を表示
    for (i, track) in tracks.iter().enumerate() {
        println!("Track {}:", i + 1);
        println!("  Track ID: {}", track.track_id);
        println!(
            "  Duration: {} (timescale: {})",
            track.duration, track.timescale
        );
        println!();
    }

    // サンプル情報を表示
    let mut sample_count = 0;
    println!("Samples:");

    // 時系列順にサンプルを抽出する
    loop {
        match demuxer.next_sample() {
            Ok(Some(sample)) => {
                sample_count += 1;
                println!("  Sample {}:", sample_count);
                println!("    Track ID: {}", sample.track.track_id);
                println!("    Timestamp: {}", sample.timestamp);
                println!("    Data offset: 0x{:x}", sample.data_offset);
                println!("    Data size: {} bytes", sample.data_size);
                println!();

                if sample_count >= 10 {
                    println!("  ... (showing first 10 samples)");
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// ファイルの指定位置から必要なサイズだけデータを読み込む
fn read_from_file<'a>(
    file: &mut File,
    position: u64,
    size: Option<usize>,
    buffer: &'a mut [u8],
) -> io::Result<&'a [u8]> {
    file.seek(SeekFrom::Start(position))?;

    let read_size = match size {
        Some(s) => s.min(buffer.len()),
        None => buffer.len(),
    };

    let bytes_read = file.read(&mut buffer[..read_size])?;
    Ok(&buffer[..bytes_read])
}

use dji_relay_core::{current_snapshot, snapshot_json, start_server_on, stop_server};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let bind_address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:11935".to_owned());
    let stream_key = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "smoke-test-key-1234".to_owned());
    let timeout_seconds = std::env::args()
        .nth(3)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(15);

    if let Err(error) = start_server_on(&bind_address, &stream_key) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }

    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    let mut received_media = false;
    while Instant::now() < deadline {
        let snapshot = current_snapshot();
        if snapshot.video_frames >= 10 && snapshot.audio_frames >= 10 {
            received_media = true;
            break;
        }
        if snapshot.status == "error" {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    println!("{}", snapshot_json());
    stop_server();
    if received_media {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

use dji_relay_core::{current_snapshot, snapshot_json, start_bridge_on_with_tls_ca, stop_server};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let target_server = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rtmp://127.0.0.1:12935/live".to_owned());
    let target_key = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "target-key-1234".to_owned());
    let tls_ca_file = std::env::args().nth(3).unwrap_or_default();
    let duration_seconds = std::env::args()
        .nth(4)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30);
    if let Err(error) =
        start_bridge_on_with_tls_ca("127.0.0.1:11935", &target_server, &target_key, &tls_ca_file)
    {
        eprintln!("{error}");
        return ExitCode::from(1);
    }

    let deadline = Instant::now() + Duration::from_secs(duration_seconds);
    let mut last_output_status = String::new();
    while Instant::now() < deadline {
        let snapshot = current_snapshot();
        let Some(output) = snapshot.outputs.first() else {
            thread::sleep(Duration::from_millis(100));
            continue;
        };
        if output.status != last_output_status {
            println!("{}", snapshot_json());
            last_output_status.clone_from(&output.status);
        }
        if output.status == "error" {
            break;
        }
        if output.outbound_bytes > 0 && output.status == "armed" {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let snapshot = current_snapshot();
    println!("{}", snapshot_json());
    stop_server();
    let delivered = snapshot
        .outputs
        .first()
        .is_some_and(|output| output.outbound_bytes > 0 && output.status != "error");
    if delivered {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

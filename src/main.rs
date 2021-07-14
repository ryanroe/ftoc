mod protocol;
use protocol::{Packet, PacketData, PacketStart, Protocol};
use std::{
    env::Args,
    fs::File,
    io,
    io::{prelude::*, BufReader, BufWriter},
    path::Path,
};
enum ArgState {
    Default,
    Size,
    Skip,
    SendTimeout,
    RecvTimeout,
}

#[derive(Debug)]
enum WorkingMode {
    Send(AppSendSetting),
    Recv(AppRecvSetting),
}
#[derive(Debug)]
struct AppSetting {
    working_mode: WorkingMode,
    dry_run: bool,
}

#[derive(Debug)]
struct AppSendSetting {
    file_path: String,
    timeout: u64,
    size: usize,
    skip: i32,
}

#[derive(Debug)]
struct AppRecvSetting {
    timeout: u64,
}

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn parse_args(a: Args) -> Result<AppSetting, String> {
    let mut state = ArgState::Default;
    let mut send_timeout = 2000;
    let mut recv_timeout = 500;
    let mut dry_run = false;
    let mut file_path = "".to_owned();
    let mut unstated_arg_count = 0;
    let mut size: usize = 500 * 1024;
    let mut skip = 0;

    for x in a {
        match x.as_str() {
            "--size" | "-s" => {
                state = ArgState::Size;
            }
            "--skip" | "-S" => {
                state = ArgState::Skip;
            }
            "--send-timeout" | "-st" => {
                state = ArgState::SendTimeout;
            }
            "--recv-timeout" | "-rt" => {
                state = ArgState::RecvTimeout;
            }
            "--dry-run" => {
                dry_run = true;
            }
            _ => {
                match state {
                    ArgState::Default => {
                        unstated_arg_count += 1;
                        if unstated_arg_count > 1 {
                            if file_path.len() == 0 {
                                file_path = x.clone();
                            } else {
                                // invalid input
                                return Err(format!("invalid input `{}`", &x));
                            }
                        }
                    }
                    ArgState::Size => {
                        let mut multiplier = 1;
                        let mut len = x.len();
                        match &x[x.len() - 1..] {
                            "k" | "K" => multiplier = 1024,
                            "m" | "M" => multiplier = 1024 * 1024,
                            "g" | "G" => multiplier = 1024 * 1024 * 1024,
                            _ => {}
                        }

                        if multiplier > 1 {
                            len = len - 1;
                        }
                        let base: usize = x[0..len].parse().expect("invalid size value");
                        size = base * multiplier;
                        state = ArgState::Default
                    }
                    ArgState::Skip => {
                        skip = x.parse().expect("invalid skip value");
                        state = ArgState::Default
                    }
                    ArgState::SendTimeout => {
                        send_timeout = x.parse().expect("invalid timeout value");
                        state = ArgState::Default
                    }
                    ArgState::RecvTimeout => {
                        recv_timeout = x.parse().expect("invalid timeout value");
                        state = ArgState::Default
                    }
                }
            }
        }
    }
    if unstated_arg_count == 1 {
        Ok(AppSetting {
            dry_run,
            working_mode: WorkingMode::Recv(AppRecvSetting {
                timeout: recv_timeout,
            }),
        })
    } else {
        Ok(AppSetting {
            dry_run,
            working_mode: WorkingMode::Send(AppSendSetting {
                file_path,
                timeout: send_timeout,
                skip,
                size,
            }),
        })
    }
}
fn main() -> Result<(), String> {
    println!("ftoc ({})", VERSION);
    parse_args(std::env::args()).and_then(|x| {
        if x.dry_run {
            dbg!(x);
            Ok(())
        } else {
            let r = match x.working_mode {
                WorkingMode::Send(x) => send_file(&x),
                WorkingMode::Recv(x) => recv_file(&x),
            };
            r.map_err(|e| format!("{}", e))
        }
    })
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms))
}
fn recv_file(s: &AppRecvSetting) -> Result<(), io::Error> {
    let mut writer: Option<BufWriter<File>> = None;
    let mut last_index = 0;
    let mut has_started = false;
    let mut time_wait_ms = 0;
    let mut timeout_ms = s.timeout;
    let mut total_len = 0u64;
    let mut recved_len = 0u64;
    let protocol = Protocol::new();
    println!("waiting for file");
    loop {
        if let Ok(x) = protocol.recv_decoded() {
            match x {
                Packet::Noop => {
                    sleep_ms(1000);
                }
                Packet::Start(x) => {
                    if has_started {
                        continue;
                    }
                    has_started = true;

                    match File::create(Path::new(&x.name)) {
                        Ok(f) => {
                            println!("start recv file: {}", x.name);
                            writer = Some(BufWriter::new(f));
                            total_len = x.length;
                            timeout_ms = (x.timeout - 150) as u64;
                            println!("reset timeout from sender side to {} ms", timeout_ms);
                        }
                        Err(e) => {
                            dbg!(e);
                            break;
                        }
                    }
                }
                Packet::Data(x) => {
                    if !has_started {
                        continue;
                    }
                    let idx = x.index;

                    if last_index == idx - 1 {
                        if let Some(w) = &mut writer {
                            time_wait_ms = 0;
                            last_index = idx;
                            recved_len += x.data.len() as u64;

                            let percentage: f32 = (recved_len as f32) / (total_len as f32);
                            println!("recv block {} ({:.2}%)", idx, percentage * 100f32);
                            if let Err(_) = w.write(x.data.as_ref()) {
                                println!("warning: can't write to destination file");
                            }
                        } else {
                            println!("warning: block {} write failed", idx)
                        }
                    } else {
                        // wait for missed block or retransmission
                        time_wait_ms += s.timeout;
                        if time_wait_ms > 10000 {
                            println!("warning: recv staled, last_index={}", last_index);
                            time_wait_ms = 0;
                        }
                    }
                    sleep_ms(timeout_ms);
                }
                Packet::End => {
                    if recved_len != total_len {
                        println!("[warn] recved end but data is incomplete");
                        continue;
                    }
                    if let Some(x) = &mut writer {
                        let _ = x.flush();
                        println!("file saved");
                        break;
                    }
                }
            }
        } else {
            sleep_ms(100);
            continue;
        }
    }
    Ok(())
}
fn send_file(s: &AppSendSetting) -> Result<(), io::Error> {
    let p = Path::new(&s.file_path);
    let file = File::open(p)?;
    let mut reader = BufReader::new(file);
    let protocol = Protocol::new();

    let mut eof = false;

    let mut index = 0;
    if s.skip != 0 {
        println!("(resume mode)");
    }
    let filename = p
        .file_name()
        .expect("can't read file name")
        .to_str()
        .expect("can't convert file name");
    reader.seek(io::SeekFrom::End(0))?;
    let len = reader.stream_position()?;
    reader.seek(io::SeekFrom::Start(0))?;
    println!("sending file : {} with {} bytes long", filename, len);
    let _ = protocol.send_encoded(Packet::Start(PacketStart {
        timeout: s.timeout as u32,
        name: filename.to_owned(),
        length: len,
    }));

    sleep_ms(2000);

    let mut v = vec![0u8; s.size];
    let skip_count = s.skip as usize;
    if skip_count > 0 {
        reader.seek(io::SeekFrom::Start((skip_count * s.size) as u64))?;
        index = skip_count;
    }
    while !eof {
        let _ = reader.read(v.as_mut_slice()).map(|s| {
            if s == 0 {
                // eof
                eof = true;
            } else {
                index += 1;
                let packet = Packet::Data(PacketData {
                    index: index,
                    data: v[0..s].to_vec(),
                });
                let _ = protocol.send_encoded(packet);
                println!("sending block {}", index);
            }
        });
        if eof {
            let _ = protocol.send_encoded(Packet::End);
            println!("file sent");
            break;
        }

        sleep_ms(s.timeout);
    }

    Ok(())
}

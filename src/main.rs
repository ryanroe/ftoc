use base64::{decode_config, encode_config};
use clipboard_win::{get_clipboard_string, set_clipboard_string};
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
            "--size" => {
                state = ArgState::Size;
            }
            "--skip" => {
                state = ArgState::Skip;
            }
            "--send-timeout" => {
                state = ArgState::SendTimeout;
            }
            "--recv-timeout" => {
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
enum RecvState {
    Wait,
    Start,
    End,
}
fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms))
}
fn recv_file(s: &AppRecvSetting) -> Result<(), io::Error> {
    let _ = set_clipboard_string("---")?;
    let mut writer: Option<BufWriter<File>> = None;
    let mut state = RecvState::Wait;
    let mut last_index = 0;
    let mut has_started = false;
    let mut time_wait_ms = 0;
    println!("waiting for file");
    loop {
        match state {
            RecvState::Wait => {
                if let Ok(x) = get_clipboard_string() {
                    if x.starts_with("ftoc-start") {
                        if has_started {
                            continue;
                        }
                        has_started = true;
                        let x: Vec<&str> = x.split(":").collect();
                        match File::create(Path::new(x[1])) {
                            Ok(f) => {
                                println!("start recv file: {}", x[1]);
                                writer = Some(BufWriter::new(f));
                                state = RecvState::Start;
                            }
                            Err(e) => {
                                dbg!(e);
                                break;
                            }
                        }
                    } else {
                        sleep_ms(1000);
                    }
                } else {
                    sleep_ms(100);
                    continue;
                }
            }
            RecvState::Start => {
                if let Ok(x) = get_clipboard_string() {
                    if x.starts_with("ftoc-end") {
                        state = RecvState::End;
                    } else if x.starts_with("ftoc") {
                        let x: Vec<&str> = x.split(":").collect();
                        if x.len() < 3 {
                            sleep_ms(100);
                            continue;
                        }
                        let idx: i32 = x[1].parse().expect("invalid index");

                        if last_index == idx - 1 {
                            if let Ok(v) = decode_config(x[2], base64::URL_SAFE_NO_PAD) {
                                if let Some(x) = &mut writer {
                                    time_wait_ms = 0;
                                    last_index = idx;
                                    println!("recv block {}", idx);
                                    let _ = x.write(v.as_ref());
                                } else {
                                    println!("warn: block {} write failed", idx)
                                }
                            } else {
                                println!("warn: block {} decode failed", idx)
                            }
                        } else {
                            // wait for missed block or retransmission
                            time_wait_ms += s.timeout;
                            if time_wait_ms > 10000 {
                                println!("warning: recv staled, last_index={}", last_index);
                                time_wait_ms = 0;
                            }
                        }
                    }
                    sleep_ms(s.timeout);
                } else {
                    sleep_ms(100);
                    continue;
                }
            }
            RecvState::End => {
                if let Some(x) = &mut writer {
                    let _ = x.flush();
                    println!("file saved")
                }
                break;
            }
        }
    }
    Ok(())
}
fn send_file(s: &AppSendSetting) -> Result<(), io::Error> {
    let p = Path::new(&s.file_path);
    let file = File::open(p)?;
    let mut reader = BufReader::new(file);

    let mut eof = false;

    let mut index = 0;
    if s.skip != 0 {
        println!("(resume mode)");
    }
    p.file_name()
        .and_then(|x| x.to_str())
        .and_then(|x| {
            println!("sending file : {}", x);
            Some(format!("ftoc-start:{}", x))
        })
        .and_then(|x| {
            let _ = set_clipboard_string(x.as_str());
            Some(())
        });
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
                let s = encode_config(&v[0..s], base64::URL_SAFE_NO_PAD);
                let text = format!("ftoc:{}:{}", index, s);
                let _ = set_clipboard_string(text.as_str());
                println!("sending block {}", index);
            }
        });
        if eof {
            let _ = set_clipboard_string("ftoc-end")?;
            println!("file sent");
            break;
        }

        sleep_ms(s.timeout);
    }

    Ok(())
}

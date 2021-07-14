use clipboard_win::{get_clipboard_string, set_clipboard_string};
use std::convert::{TryFrom, TryInto};

pub struct Protocol;
impl Protocol {
    pub fn new() -> Self {
        Protocol {}
    }

    pub fn send_encoded(&self, p: Packet) -> Result<(), String> {
        let v: String = p.into();
        self.send(v)
    }
    pub fn recv_decoded(&self) -> Result<Packet, String> {
        self.recv().and_then(|x| x.try_into())
    }
    fn send(&self, data: String) -> Result<(), String> {
        set_clipboard_string(data.as_str()).map_err(|_| format!("can't set clipboard"))
    }
    fn recv(&self) -> Result<String, String> {
        get_clipboard_string().map_err(|_| format!("can't get clipboard data"))
    }
}

#[test]
fn test_clipboard() {
    let p = Protocol::new();
    let x: String = Packet::Start(PacketStart {
        name: "xmind 11 crack.7z".to_owned(),
        length: 32460746,
        timeout: 1000,
    })
    .into();
    let _ = p.send(x.clone());
    let r = p.recv().expect("can't recv");
    assert_eq!(r, x);
}

#[derive(Debug)]
pub struct PacketStart {
    pub name: String,
    pub length: u64,
    pub timeout: u32,
}
#[derive(Debug)]
pub struct PacketData {
    pub index: usize,
    pub data: Vec<u8>,
}
#[derive(Debug)]
pub enum Packet {
    Noop,
    Start(PacketStart),
    Data(PacketData),
    End,
}
const CMD_START: &str = "\x01";
const CMD_END: &str = "\x02";
const CMD_DATA: &str = "\x03";
const CMD_NOOP: &str = "\x04";

fn get_decoded_bytes(data: String) -> Vec<u8> {
    if let Ok(x) = ascii85::decode(&data[1..]) {
        return x;
    }

    vec![]
}
impl TryFrom<String> for Packet {
    type Error = String;
    fn try_from(data: String) -> Result<Self, String> {
        let mut packet = Packet::Noop;

        if data.starts_with(CMD_NOOP) {
            packet = Packet::Noop;
        } else if data.starts_with(CMD_START) {
            let decoded = get_decoded_bytes(data);
            let mut r = Reader::from(&decoded, 0);
            let length = r.read_u64();
            let timeout = r.read_u32();

            let name = String::from_utf8(r.read_to_end())
                .map_err(|_| format!("can't decode utf8 string (name field)"))?;
            packet = Packet::Start(PacketStart {
                name,
                length,
                timeout,
            })
        } else if data.starts_with(CMD_DATA) {
            let decoded = get_decoded_bytes(data);
            let mut r = Reader::from(&decoded, 0);
            let index = r.read_usize();
            let data = r.read_to_end();
            packet = Packet::Data(PacketData { index, data });
        } else if data.starts_with(CMD_END) {
            packet = Packet::End;
        }
        Ok(packet)
    }
}
impl Into<String> for Packet {
    fn into(self) -> String {
        let mut buf = String::new();
        match self {
            Packet::Noop => {
                buf.push_str(CMD_NOOP);
            }
            Packet::Start(x) => {
                buf.push_str(CMD_START);
                let mut b = vec![];
                b.extend_from_slice(&x.length.to_be_bytes());
                b.extend_from_slice(&x.timeout.to_be_bytes());
                b.extend_from_slice(&x.name.as_bytes());
                buf.push_str(ascii85::encode(&b).as_str());
            }
            Packet::Data(x) => {
                buf.push_str(CMD_DATA);
                let mut b = vec![];
                b.extend_from_slice(&x.index.to_be_bytes());
                b.extend_from_slice(&x.data);
                buf.push_str(ascii85::encode(&b).as_str());
            }
            Packet::End => {
                buf.push_str(CMD_END);
            }
        }
        buf
    }
}

struct Reader<'data> {
    offset: usize,
    data: &'data [u8],
}
impl<'data> Reader<'data> {
    fn from(data: &'data [u8], offset: usize) -> Self {
        Reader { offset, data }
    }
    fn read_u32(&mut self) -> u32 {
        let mut b: [u8; 4] = [0u8; 4];
        b.copy_from_slice(&self.data[self.offset..self.offset + 4]);
        self.offset += 4;
        u32::from_be_bytes(b)
    }
    fn read_u64(&mut self) -> u64 {
        let mut b: [u8; 8] = [0u8; 8];
        b.copy_from_slice(&self.data[self.offset..self.offset + 8]);
        self.offset += 8;
        u64::from_be_bytes(b)
    }
    fn read_usize(&mut self) -> usize {
        self.read_u64() as usize
    }

    fn read_to_end(&self) -> Vec<u8> {
        self.data[self.offset..].to_vec()
    }
}

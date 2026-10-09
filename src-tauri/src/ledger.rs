//! Ledger hardware wallet, Solana app, over USB HID (t-018). The key never leaves the device: we ask it for a public
//! key and for a signature over the exact message the core built; the owner approves on the device's own screen.
//! Transport (Ledger HID framing): 64-byte reports, channel 0x0101, tag 0x05, sequence number, the first packet carries
//! the APDU length. Solana app APDUs (app-solana): CLA 0xE0, GET_PUBKEY 0x05, SIGN_MESSAGE 0x06, payload chunks of
//! 255 bytes flagged P2_EXTEND / P2_MORE. Path m/44'/501'/index' (Ledger Live / Phantom with Ledger).

const CLA: u8 = 0xE0;
const INS_PUBKEY: u8 = 0x05;
const INS_SIGN: u8 = 0x06;
const P2_EXTEND: u8 = 0x01;
const P2_MORE: u8 = 0x02;
const CHANNEL: u16 = 0x0101;
const TAG: u8 = 0x05;
const PACKET: usize = 64;

pub fn path(index: u32) -> Vec<u8> {
    let mut p = vec![3u8];
    for c in [44u32 | 0x8000_0000, 501 | 0x8000_0000, index | 0x8000_0000] { p.extend_from_slice(&c.to_be_bytes()) }
    p
}
fn apdu(ins: u8, p1: u8, p2: u8, data: &[u8]) -> Vec<u8> {
    let mut a = vec![CLA, ins, p1, p2, data.len() as u8];
    a.extend_from_slice(data);
    a
}
/// the sign request split into APDUs of at most 255 data bytes
fn sign_apdus(index: u32, msg: &[u8]) -> Vec<Vec<u8>> {
    let mut payload = vec![1u8]; // one signer
    payload.extend(path(index));
    payload.extend_from_slice(msg);
    let chunks: Vec<&[u8]> = payload.chunks(255).collect();
    chunks.iter().enumerate().map(|(i, c)| {
        let p2 = if i > 0 { P2_EXTEND } else { 0 } | if i + 1 < chunks.len() { P2_MORE } else { 0 };
        apdu(INS_SIGN, 0x01, p2, c)
    }).collect()
}
/// HID framing of one APDU into 64-byte reports
fn frame(apdu: &[u8]) -> Vec<[u8; PACKET]> {
    let mut out = vec![];
    let mut data = Vec::with_capacity(apdu.len() + 2);
    data.extend_from_slice(&(apdu.len() as u16).to_be_bytes());
    data.extend_from_slice(apdu);
    for (seq, chunk) in data.chunks(PACKET - 5).enumerate() {
        let mut p = [0u8; PACKET];
        p[..2].copy_from_slice(&CHANNEL.to_be_bytes());
        p[2] = TAG;
        p[3..5].copy_from_slice(&(seq as u16).to_be_bytes());
        p[5..5 + chunk.len()].copy_from_slice(chunk);
        out.push(p);
    }
    out
}
/// reassembles reports into one answer; Ok(None) = more reports needed
struct Unframe { want: Option<usize>, buf: Vec<u8>, seq: u16 }
impl Unframe {
    fn new() -> Self { Unframe { want: None, buf: vec![], seq: 0 } }
    fn push(&mut self, p: &[u8]) -> Result<Option<Vec<u8>>, String> {
        if p.len() < 5 || u16::from_be_bytes([p[0], p[1]]) != CHANNEL || p[2] != TAG { return Err("Ledger ответил не тем".into()) }
        if u16::from_be_bytes([p[3], p[4]]) != self.seq { return Err("Ledger сбился в ответе".into()) }
        self.seq += 1;
        let mut body = &p[5..];
        if self.want.is_none() {
            if body.len() < 2 { return Err("Ledger ответил не тем".into()) }
            self.want = Some(u16::from_be_bytes([body[0], body[1]]) as usize);
            body = &body[2..];
        }
        let want = self.want.unwrap();
        self.buf.extend_from_slice(&body[..body.len().min(want - self.buf.len().min(want))]);
        Ok((self.buf.len() >= want).then(|| std::mem::take(&mut self.buf)))
    }
}
/// status word -> Russian, the cases a person can act on
fn status(answer: Vec<u8>) -> Result<Vec<u8>, String> {
    if answer.len() < 2 { return Err("Ledger ответил не тем".into()) }
    let (data, sw) = answer.split_at(answer.len() - 2);
    match u16::from_be_bytes([sw[0], sw[1]]) {
        0x9000 => Ok(data.to_vec()),
        0x6985 => Err("отклонено на Ledger".into()),
        0x6a81 | 0x6808 => Err("на Ledger выключена подпись таких переводов: включите «Blind signing» в настройках приложения Solana".into()),
        0x6d00 | 0x6e00 | 0x6e01 | 0x6511 => Err("откройте на Ledger приложение Solana".into()),
        0x5515 | 0x6b0c => Err("Ledger заблокирован: введите на нём PIN".into()),
        s => Err(format!("Ledger ответил ошибкой {s:04x}")),
    }
}

// ---------------------------------------------------------------- the device (desktop: USB HID)
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod device {
    use super::*;
    const VID: u16 = 0x2c97;
    pub fn exchange(apdus: &[Vec<u8>], wait_s: u64) -> Result<Vec<u8>, String> {
        let api = hidapi::HidApi::new().map_err(|_| "USB недоступен".to_string())?;
        let info = api.device_list()
            .find(|d| d.vendor_id() == VID && (d.usage_page() == 0xffa0 || d.interface_number() == 0))
            .ok_or("Ledger не найден: подключите его по USB и разблокируйте")?;
        let dev = info.open_device(&api).map_err(|_| "Ledger занят другой программой (закройте Ledger Live)".to_string())?;
        let mut last = vec![];
        for a in apdus {
            for p in frame(a) {
                let mut report = [0u8; PACKET + 1]; // report id 0 first
                report[1..].copy_from_slice(&p);
                dev.write(&report).map_err(|_| "связь с Ledger прервалась".to_string())?;
            }
            let mut u = Unframe::new();
            let t0 = std::time::Instant::now();
            last = loop {
                let mut buf = [0u8; PACKET];
                let n = dev.read_timeout(&mut buf, 500).map_err(|_| "связь с Ledger прервалась".to_string())?;
                if n == 0 {
                    if t0.elapsed().as_secs() > wait_s { return Err("Ledger не ответил: подтвердите на нём или попробуйте снова".into()) }
                    continue;
                }
                if let Some(answer) = u.push(&buf[..n])? { break status(answer)? }
            };
        }
        Ok(last)
    }
}
#[cfg(any(target_os = "android", target_os = "ios"))]
mod device {
    pub fn exchange(_: &[Vec<u8>], _: u64) -> Result<Vec<u8>, String> { Err("Ledger пока подключается только к компьютеру".into()) }
}

/// the Solana address of m/44'/501'/index' on the connected device (no confirmation on the device)
pub fn pubkey(index: u32) -> Result<[u8; 32], String> {
    let r = device::exchange(&[apdu(INS_PUBKEY, 0, 0, &path(index))], 10)?;
    r.get(..32).and_then(|b| b.try_into().ok()).ok_or_else(|| "Ledger ответил не тем".into())
}
/// the device shows the transfer; the owner approves there (2 min to decide)
pub fn sign(index: u32, msg: &[u8]) -> Result<[u8; 64], String> {
    let r = device::exchange(&sign_apdus(index, msg), 120)?;
    r.get(..64).and_then(|b| b.try_into().ok()).ok_or_else(|| "Ledger ответил не тем".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_round_trip() {
        // a 300-byte APDU: 7 + 59 + 59 + ... bytes across reports, read back whole
        let a: Vec<u8> = (0..300u32).map(|i| i as u8).collect();
        let f = frame(&a);
        assert_eq!(f.len(), (302usize).div_ceil(59));
        assert_eq!(&f[0][..7], &[0x01, 0x01, 0x05, 0, 0, 0x01, 0x2c]);
        assert_eq!(&f[1][..5], &[0x01, 0x01, 0x05, 0, 1]);
        let mut u = Unframe::new();
        let mut got = None;
        for p in &f { got = u.push(p).unwrap() }
        assert_eq!(got.unwrap(), a);
        let mut u = Unframe::new();
        assert!(u.push(&f[1]).is_err(), "out of order");
    }
    #[test]
    fn apdus_and_status() {
        assert_eq!(path(0), vec![3, 0x80, 0, 0, 44, 0x80, 0, 0x01, 0xf5, 0x80, 0, 0, 0]);
        let small = sign_apdus(0, &[7; 100]);
        assert_eq!(small.len(), 1);
        assert_eq!(&small[0][..5], &[CLA, INS_SIGN, 1, 0, 114]); // 1 + 13 path + 100
        let big = sign_apdus(2, &[7; 600]); // 614 bytes: 255 + 255 + 104
        assert_eq!(big.iter().map(|a| (a[3], a[4] as usize)).collect::<Vec<_>>(), vec![(P2_MORE, 255), (P2_EXTEND | P2_MORE, 255), (P2_EXTEND, 104)]);
        assert_eq!(status(vec![1, 2, 0x90, 0x00]).unwrap(), vec![1, 2]);
        assert!(status(vec![0x69, 0x85]).unwrap_err().contains("отклонено"));
        assert!(status(vec![0x6e, 0x00]).unwrap_err().contains("приложение Solana"));
    }
}

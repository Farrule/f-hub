use hidapi::HidDevice;

/// VIA 標準 32B Raw HID パケット送受信（Windows hidapi 向けに 33B 送信 / 64B 受信）
pub fn send_raw_32(dev: &HidDevice, payload: &[u8], timeout_ms: i32) -> Result<Vec<u8>, String> {
    let mut pkt = [0u8; 33];
    pkt[0] = 0x00; // Report ID 0
    let len = payload.len().min(32);
    pkt[1..1 + len].copy_from_slice(&payload[..len]);

    dev.write(&pkt).map_err(|e| format!("write失敗: {e}"))?;

    let mut buf = [0u8; 64];
    let n = dev
        .read_timeout(&mut buf, timeout_ms)
        .map_err(|e| format!("read失敗: {e}"))?;
    Ok(buf[..n].to_vec())
}

/// VIA プロトコルバージョン取得 (0x01)
#[allow(dead_code)]
pub fn get_protocol_version(dev: &HidDevice) -> Result<u16, String> {
    let resp = send_raw_32(dev, &[0x01], 500)?;
    if resp.len() >= 3 && resp[0] == 0x01 {
        Ok(u16::from_be_bytes([resp[1], resp[2]]))
    } else if resp.is_empty() {
        Err("無応答（スリープ中または非対応）".to_string())
    } else {
        Err(format!("想定外応答: {:02x?}", &resp[..resp.len().min(8)]))
    }
}
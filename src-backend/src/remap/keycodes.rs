use super::types::RemapAction;

/// USB HID 修飾キービットマスク
pub const MOD_LCTRL: u8 = 0x01;
pub const MOD_LSHIFT: u8 = 0x02;
pub const MOD_LALT: u8 = 0x04;
pub const MOD_LGUI: u8 = 0x08;

/// QMK 16bit 修飾ビット（上位バイト）
const QMK_MOD_LCTL: u16 = 0x0100;
const QMK_MOD_LSFT: u16 = 0x0200;
const QMK_MOD_LALT: u16 = 0x0400;
const QMK_MOD_LGUI: u16 = 0x0800;

/// QMK 特殊キーコード定数
pub const QMK_KC_NO: u16 = 0x0000;
pub const QMK_KC_TRNS: u16 = 0x0001;

/// 正規化キー名 ⇔ USB HID Keyboard Usage ID (Page 0x07) 対応表
const KEY_TABLE: &[(&str, u8)] = &[
    // アルファベット
    ("A", 0x04), ("B", 0x05), ("C", 0x06), ("D", 0x07), ("E", 0x08),
    ("F", 0x09), ("G", 0x0A), ("H", 0x0B), ("I", 0x0C), ("J", 0x0D),
    ("K", 0x0E), ("L", 0x0F), ("M", 0x10), ("N", 0x11), ("O", 0x12),
    ("P", 0x13), ("Q", 0x14), ("R", 0x15), ("S", 0x16), ("T", 0x17),
    ("U", 0x18), ("V", 0x19), ("W", 0x1A), ("X", 0x1B), ("Y", 0x1C),
    ("Z", 0x1D),
    // 数字
    ("1", 0x1E), ("2", 0x1F), ("3", 0x20), ("4", 0x21), ("5", 0x22),
    ("6", 0x23), ("7", 0x24), ("8", 0x25), ("9", 0x26), ("0", 0x27),
    // 制御・記号
    ("Enter", 0x28), ("Escape", 0x29), ("Backspace", 0x2A), ("Tab", 0x2B),
    ("Space", 0x2C), ("Minus", 0x2D), ("Equal", 0x2E), ("LBracket", 0x2F),
    ("RBracket", 0x30), ("Backslash", 0x31), ("Semicolon", 0x33), ("Quote", 0x34),
    ("Grave", 0x35), ("Comma", 0x36), ("Period", 0x37), ("Slash", 0x38),
    ("CapsLock", 0x39),
    // ファンクションキー
    ("F1", 0x3A), ("F2", 0x3B), ("F3", 0x3C), ("F4", 0x3D),
    ("F5", 0x3E), ("F6", 0x3F), ("F7", 0x40), ("F8", 0x41),
    ("F9", 0x42), ("F10", 0x43), ("F11", 0x44), ("F12", 0x45),
    ("F13", 0x68), ("F14", 0x69), ("F15", 0x6A), ("F16", 0x6B),
    ("F17", 0x6C), ("F18", 0x6D), ("F19", 0x6E), ("F20", 0x6F),
    ("F21", 0x70), ("F22", 0x71), ("F23", 0x72), ("F24", 0x73),
    // ナビゲーション
    ("PrintScreen", 0x46), ("ScrollLock", 0x47), ("Pause", 0x48),
    ("Insert", 0x49), ("Home", 0x4A), ("PageUp", 0x4B),
    ("Delete", 0x4C), ("End", 0x4D), ("PageDown", 0x4E),
    ("Right", 0x4F), ("Left", 0x50), ("Down", 0x51), ("Up", 0x52),
    // モディファイア単体
    ("LCtrl", 0xE0), ("LShift", 0xE1), ("LAlt", 0xE2), ("LWin", 0xE3),
    ("RCtrl", 0xE4), ("RShift", 0xE5), ("RAlt", 0xE6), ("RWin", 0xE7),
    // 日本語配列固有キー
    ("Henkan", 0x8A), ("Muhenkan", 0x8B), ("Kana", 0x88), ("Ro", 0x87), ("Yen", 0x89),
];

/// メディアコマンド ⇔ QMK 16bit コード対応表
const MEDIA_QMK_TABLE: &[(&str, u16)] = &[
    ("mute", 0x00A8),
    ("vol_up", 0x00A9),
    ("vol_down", 0x00AA),
    ("next", 0x00AB),
    ("prev", 0x00AC),
    ("stop", 0x00AD),
    ("play_pause", 0x00AE),
];

/// マウスボタン ⇔ QMK 16bit コード対応表
const MOUSE_QMK_TABLE: &[(&str, u16)] = &[
    ("wheel_up", 0x00CD),
    ("wheel_down", 0x00CE),
    ("left", 0x00D1),
    ("right", 0x00D2),
    ("middle", 0x00D3),
    ("back", 0x00D4),
    ("forward", 0x00D5),
];

/// キー名から USB HID Usage ID を取得
pub fn key_name_to_hid(name: &str) -> Option<u8> {
    KEY_TABLE
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, code)| *code)
}

/// USB HID Usage ID から正規化キー名を取得
pub fn hid_to_key_name(usage: u8) -> Option<&'static str> {
    KEY_TABLE
        .iter()
        .find(|(_, code)| *code == usage)
        .map(|(k, _)| *k)
}

/// 修飾キー文字列スライスから USB HID モディファイアビットマスクを生成
pub fn modifiers_to_hid_mask(mods: &[String]) -> u8 {
    let mut mask = 0u8;
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "lctrl" | "control" => mask |= MOD_LCTRL,
            "shift" | "lshift" => mask |= MOD_LSHIFT,
            "alt" | "lalt" => mask |= MOD_LALT,
            "win" | "lwin" | "gui" | "meta" | "cmd" => mask |= MOD_LGUI,
            _ => {}
        }
    }
    mask
}

/// USB HID モディファイアビットマスクから文字列ベクタへ復元
pub fn hid_mask_to_modifiers(mask: u8) -> Vec<String> {
    let mut mods = Vec::new();
    if mask & MOD_LCTRL != 0 {
        mods.push("Ctrl".to_string());
    }
    if mask & MOD_LSHIFT != 0 {
        mods.push("Shift".to_string());
    }
    if mask & MOD_LALT != 0 {
        mods.push("Alt".to_string());
    }
    if mask & MOD_LGUI != 0 {
        mods.push("Win".to_string());
    }
    mods
}

/// RemapAction を QMK 16bit キーコードへ変換（VIA 対応デバイス用）
pub fn action_to_qmk(action: &RemapAction) -> Result<u16, String> {
    match action {
        RemapAction::Default => Ok(QMK_KC_TRNS),
        RemapAction::Disabled => Ok(QMK_KC_NO),
        RemapAction::Key { key, modifiers } => {
            let hid = key_name_to_hid(key)
                .ok_or_else(|| format!("未対応のキー名です: {key}"))?;
            let mut qmk = hid as u16;
            for m in modifiers {
                match m.to_ascii_lowercase().as_str() {
                    "ctrl" | "lctrl" | "control" => qmk |= QMK_MOD_LCTL,
                    "shift" | "lshift" => qmk |= QMK_MOD_LSFT,
                    "alt" | "lalt" => qmk |= QMK_MOD_LALT,
                    "win" | "lwin" | "gui" | "meta" | "cmd" => qmk |= QMK_MOD_LGUI,
                    other => return Err(format!("未対応の修飾キーです: {other}")),
                }
            }
            Ok(qmk)
        }
        RemapAction::MouseButton { button } => MOUSE_QMK_TABLE
            .iter()
            .find(|(b, _)| b.eq_ignore_ascii_case(button))
            .map(|(_, code)| *code)
            .ok_or_else(|| format!("未対応のマウスボタンです: {button}")),
        RemapAction::Media { command } => MEDIA_QMK_TABLE
            .iter()
            .find(|(c, _)| c.eq_ignore_ascii_case(command))
            .map(|(_, code)| *code)
            .ok_or_else(|| format!("未対応のメディアコマンドです: {command}")),
        RemapAction::Special { code } => match code.to_ascii_lowercase().as_str() {
            "mo_1" => Ok(0x5221), // MO(1) in VIA/QMK
            "mo_2" => Ok(0x5222), // MO(2)
            "mo_3" => Ok(0x5223), // MO(3)
            "tg_1" => Ok(0x5241), // TG(1)
            "tg_2" => Ok(0x5242), // TG(2)
            other => Err(format!("VIA で未対応の特殊機能です: {other}")),
        },
        RemapAction::Raw { hex } => {
            let trimmed = hex.trim_start_matches("0x").trim_start_matches("0X");
            u16::from_str_radix(trimmed, 16)
                .map_err(|_| format!("不正な 16 進キーコードです: {hex}"))
        }
    }
}

/// QMK 16bit キーコードを RemapAction へ復元
pub fn qmk_to_action(code: u16) -> RemapAction {
    match code {
        QMK_KC_NO => return RemapAction::Disabled,
        QMK_KC_TRNS => return RemapAction::Default,
        0x5221 => return RemapAction::Special { code: "mo_1".to_string() },
        0x5222 => return RemapAction::Special { code: "mo_2".to_string() },
        0x5223 => return RemapAction::Special { code: "mo_3".to_string() },
        0x5241 => return RemapAction::Special { code: "tg_1".to_string() },
        0x5242 => return RemapAction::Special { code: "tg_2".to_string() },
        _ => {}
    }

    if let Some((btn, _)) = MOUSE_QMK_TABLE.iter().find(|(_, c)| *c == code) {
        return RemapAction::MouseButton {
            button: (*btn).to_string(),
        };
    }

    if let Some((cmd, _)) = MEDIA_QMK_TABLE.iter().find(|(_, c)| *c == code) {
        return RemapAction::Media {
            command: (*cmd).to_string(),
        };
    }

    // 基本キーまたは修飾付きキー (0x0004..=0x0FFF 範囲)
    let mod_bits = (code >> 8) as u8;
    let basic_hid = (code & 0x00FF) as u8;
    if (mod_bits & 0xF0) == 0 {
        if let Some(key_name) = hid_to_key_name(basic_hid) {
            return RemapAction::Key {
                key: key_name.to_string(),
                modifiers: hid_mask_to_modifiers(mod_bits),
            };
        }
    }

    RemapAction::Raw {
        hex: format!("0x{code:04X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_key_and_shortcut() {
        let act = RemapAction::Key {
            key: "C".to_string(),
            modifiers: vec!["Ctrl".to_string(), "Shift".to_string()],
        };
        let qmk = action_to_qmk(&act).unwrap();
        assert_eq!(qmk, 0x0306);
        assert_eq!(qmk_to_action(qmk), act);
    }

    #[test]
    fn roundtrip_media_and_mouse() {
        let media = RemapAction::Media {
            command: "vol_up".to_string(),
        };
        let qmk = action_to_qmk(&media).unwrap();
        assert_eq!(qmk_to_action(qmk), media);

        let mouse = RemapAction::MouseButton {
            button: "back".to_string(),
        };
        let qmk_m = action_to_qmk(&mouse).unwrap();
        assert_eq!(qmk_to_action(qmk_m), mouse);
    }

    #[test]
    fn fallback_unknown_qmk_to_raw() {
        let act = qmk_to_action(0x7E12);
        assert_eq!(
            act,
            RemapAction::Raw {
                hex: "0x7E12".to_string()
            }
        );
    }
}
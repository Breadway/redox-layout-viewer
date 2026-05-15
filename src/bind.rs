use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use eframe::egui;
use evdev::{enumerate, EventSummary, KeyCode};
use std::collections::HashSet;
use std::thread;

use crate::types::KeyEdge;

pub fn extract_tap_key(keybind: &str) -> Option<&str> {
    // _T(key) forms: LSFT_T, LCTL_T, LALT_T, LGUI_T, MEH_T, HYPR_T, etc.
    if let Some(idx) = keybind.find("_T(") {
        if keybind.ends_with(')') {
            return Some(keybind[idx + 3..keybind.len() - 1].trim());
        }
    }
    // LT(layer, key) and MT(mods, key): extract the last argument
    if (keybind.starts_with("LT(") || keybind.starts_with("MT(")) && keybind.ends_with(')') {
        if let Some(comma) = keybind.rfind(", ") {
            return Some(&keybind[comma + 2..keybind.len() - 1]);
        }
    }
    None
}

pub fn extract_mod_combo(keybind: &str) -> Option<(&'static str, &str)> {
    for (prefix, mod_bind) in &[
        ("LGUI(", "KC_LGUI"),
        ("RGUI(", "KC_RGUI"),
        ("LCTL(", "KC_LCTL"),
        ("RCTL(", "KC_RCTL"),
        ("LALT(", "KC_LALT"),
        ("RALT(", "KC_RALT"),
        ("LSFT(", "KC_LSFT"),
        ("RSFT(", "KC_RSFT"),
    ] {
        if let Some(inner) = keybind.strip_prefix(prefix).and_then(|s| s.strip_suffix(')')) {
            return Some((mod_bind, inner));
        }
    }
    None
}

/// Highlighted strictly while the physical key(s) are held — no decay.
pub fn is_keybind_highlighted(keybind: &str, held: &HashSet<String>) -> bool {
    if held.contains(keybind) {
        return true;
    }
    if let Some(tap) = extract_tap_key(keybind) {
        if held.contains(tap) {
            return true;
        }
    }
    if let Some((mod_bind, inner)) = extract_mod_combo(keybind) {
        if held.contains(inner) && held.contains(mod_bind) {
            return true;
        }
    }
    false
}

pub fn evdev_keycode_to_bind(key_code: KeyCode) -> Option<String> {
    let name = format!("{key_code:?}");
    let bind = match name.as_str() {
        // ── Navigation ───────────────────────────────────────────────────────
        "KEY_ESC"         => "KC_ESC",
        "KEY_TAB"         => "KC_TAB",
        "KEY_ENTER"       => "KC_ENT",
        "KEY_SPACE"       => "KC_SPC",
        "KEY_BACKSPACE"   => "KC_BSPC",
        "KEY_INSERT"      => "KC_INS",
        "KEY_DELETE"      => "KC_DEL",
        "KEY_HOME"        => "KC_HOME",
        "KEY_END"         => "KC_END",
        "KEY_PAGEUP"      => "KC_PGUP",
        "KEY_PAGEDOWN"    => "KC_PGDN",
        "KEY_LEFT"        => "KC_LEFT",
        "KEY_RIGHT"       => "KC_RGHT",
        "KEY_UP"          => "KC_UP",
        "KEY_DOWN"        => "KC_DOWN",
        // ── Punctuation ──────────────────────────────────────────────────────
        "KEY_GRAVE"       => "KC_GRV",
        "KEY_MINUS"       => "KC_MINS",
        "KEY_EQUAL"       => "KC_EQL",
        "KEY_LEFTBRACE"   => "KC_LBRC",
        "KEY_RIGHTBRACE"  => "KC_RBRC",
        "KEY_BACKSLASH"   => "KC_BSLS",
        "KEY_SEMICOLON"   => "KC_SCLN",
        "KEY_APOSTROPHE"  => "KC_QUOT",
        "KEY_COMMA"       => "KC_COMM",
        "KEY_DOT"         => "KC_DOT",
        "KEY_SLASH"       => "KC_SLSH",
        "KEY_102ND"       => "KC_NUBS",
        // ── Lock keys ────────────────────────────────────────────────────────
        "KEY_CAPSLOCK"    => "KC_CAPS",
        "KEY_NUMLOCK"     => "KC_NLCK",
        "KEY_SCROLLLOCK"  => "KC_SCRL",
        // ── System / power ───────────────────────────────────────────────────
        "KEY_PRINT"       => "KC_PSCR",
        "KEY_SYSRQ"       => "KC_PSCR",
        "KEY_PAUSE"       => "KC_PAUS",
        "KEY_BREAK"       => "KC_PAUS",
        "KEY_COMPOSE"     => "KC_APP",
        "KEY_POWER"       => "KC_PWR",
        "KEY_SLEEP"       => "KC_SLEP",
        "KEY_WAKEUP"      => "KC_WAKE",
        // ── Modifiers ────────────────────────────────────────────────────────
        "KEY_LEFTSHIFT"   => "KC_LSFT",
        "KEY_RIGHTSHIFT"  => "KC_RSFT",
        "KEY_LEFTCTRL"    => "KC_LCTL",
        "KEY_RIGHTCTRL"   => "KC_RCTL",
        "KEY_LEFTALT"     => "KC_LALT",
        "KEY_RIGHTALT"    => "KC_RALT",
        "KEY_LEFTMETA"    => "KC_LGUI",
        "KEY_RIGHTMETA"   => "KC_RGUI",
        // ── Numpad ───────────────────────────────────────────────────────────
        "KEY_KPENTER"     => "KC_PENT",
        "KEY_KPSLASH"     => "KC_PSLH",
        "KEY_KPASTERISK"  => "KC_PAST",
        "KEY_KPMINUS"     => "KC_PMNS",
        "KEY_KPPLUS"      => "KC_PPLS",
        "KEY_KPDOT"       => "KC_PDOT",
        "KEY_KPEQUAL"     => "KC_PEQL",
        "KEY_KP0"         => "KC_P0",
        "KEY_KP1"         => "KC_P1",
        "KEY_KP2"         => "KC_P2",
        "KEY_KP3"         => "KC_P3",
        "KEY_KP4"         => "KC_P4",
        "KEY_KP5"         => "KC_P5",
        "KEY_KP6"         => "KC_P6",
        "KEY_KP7"         => "KC_P7",
        "KEY_KP8"         => "KC_P8",
        "KEY_KP9"         => "KC_P9",
        // ── Application control ───────────────────────────────────────────────
        "KEY_STOP"        => "KC_WSTP",  // browser stop (KEY_STOPCD = media stop)
        "KEY_AGAIN"       => "KC_AGIN",
        "KEY_UNDO"        => "KC_UNDO",
        "KEY_COPY"        => "KC_COPY",
        "KEY_PASTE"       => "KC_PSTE",
        "KEY_FIND"        => "KC_FIND",
        "KEY_CUT"         => "KC_CUT",
        "KEY_HELP"        => "KC_HELP",
        "KEY_MENU"        => "KC_MENU",
        "KEY_REDO"        => "KC_AGIN",  // Linux REDO → QMK Again
        // ── Media ────────────────────────────────────────────────────────────
        "KEY_MUTE"         => "KC_MUTE",
        "KEY_VOLUMEUP"     => "KC_VOLU",
        "KEY_VOLUMEDOWN"   => "KC_VOLD",
        "KEY_NEXTSONG"     => "KC_MNXT",
        "KEY_PREVIOUSSONG" => "KC_MPRV",
        "KEY_STOPCD"       => "KC_MSTP",
        "KEY_PLAYPAUSE"    => "KC_MPLY",
        "KEY_EJECTCD"      => "KC_EJCT",
        "KEY_EJECTCLOSECD" => "KC_EJCT",
        "KEY_REWIND"       => "KC_MRWD",
        "KEY_FASTFORWARD"  => "KC_MFFD",
        "KEY_MEDIA"        => "KC_MSEL",
        "KEY_RECORD"       => "KC_MPLY",  // closest approximation
        // ── Browser / app launchers ───────────────────────────────────────────
        "KEY_HOMEPAGE"    => "KC_WHOM",
        "KEY_BACK"        => "KC_WBAK",
        "KEY_FORWARD"     => "KC_WFWD",
        "KEY_REFRESH"     => "KC_WREF",
        "KEY_BOOKMARKS"   => "KC_WFAV",
        "KEY_SEARCH"      => "KC_WSCH",
        "KEY_MAIL"        => "KC_MAIL",
        "KEY_CALC"        => "KC_CALC",
        "KEY_COMPUTER"    => "KC_MYCM",
        // ── Brightness ───────────────────────────────────────────────────────
        "KEY_BRIGHTNESSUP"   => "KC_BRIU",
        "KEY_BRIGHTNESSDOWN" => "KC_BRID",
        // ── Japanese / international ──────────────────────────────────────────
        "KEY_MUHENKAN"         => "KC_INT1",
        "KEY_HENKAN"           => "KC_INT2",
        "KEY_KATAKANAHIRAGANA" => "KC_INT3",
        "KEY_YEN"              => "KC_INT4",
        "KEY_RO"               => "KC_INT5",
        "KEY_HANGEUL"          => "KC_LNG1",
        "KEY_HANJA"            => "KC_LNG2",
        "KEY_KATAKANA"         => "KC_LNG3",
        "KEY_HIRAGANA"         => "KC_LNG4",
        "KEY_ZENKAKUHANKAKU"   => "KC_LNG5",
        // ── Mouse buttons ────────────────────────────────────────────────────
        "BTN_LEFT"    => "KC_BTN1",
        "BTN_RIGHT"   => "KC_BTN2",
        "BTN_MIDDLE"  => "KC_BTN3",
        "BTN_SIDE"    => "KC_BTN4",
        "BTN_EXTRA"   => "KC_BTN5",
        "BTN_FORWARD" => "KC_BTN6",
        "BTN_BACK"    => "KC_BTN7",
        "BTN_TASK"    => "KC_BTN8",
        _ => {
            if let Some(suffix) = name.strip_prefix("KEY_") {
                // Letters: KEY_A → KC_A
                if suffix.len() == 1 && suffix.chars().all(|ch| ch.is_ascii_uppercase()) {
                    return Some(format!("KC_{suffix}"));
                }
                // Digits: KEY_0 → KC_0
                if suffix.len() == 1 && suffix.chars().all(|ch| ch.is_ascii_digit()) {
                    return Some(format!("KC_{suffix}"));
                }
                // Function keys: KEY_F1..KEY_F35 → KC_F1..KC_F35
                if let Some(number) = suffix.strip_prefix('F') {
                    if !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()) {
                        return Some(format!("KC_F{number}"));
                    }
                }
            }
            return None;
        }
    };
    Some(bind.to_string())
}

pub fn spawn_global_key_reader(tx: Sender<KeyEdge>, ctx: egui::Context) -> Result<()> {
    thread::Builder::new()
        .name("global-key-reader".into())
        .spawn(move || {
            eprintln!("[evdev] enumerating input devices...");
            let mut total = 0usize;
            let mut readers = 0usize;

            for (path, mut device) in enumerate() {
                total += 1;
                let dev_name = device.name().unwrap_or("unknown").to_string();

                if device.supported_keys().is_none() {
                    continue;
                }

                readers += 1;
                eprintln!("[evdev] reading: {} ({:?})", dev_name, path);

                let tx = tx.clone();
                let ctx = ctx.clone();
                let dev_name_clone = dev_name.clone();
                let result = thread::Builder::new()
                    .name(format!("evdev-{dev_name}"))
                    .spawn(move || loop {
                        match device.fetch_events() {
                            Ok(events) => {
                                let mut changed = false;
                                for event in events {
                                    match event.destructure() {
                                        EventSummary::Key(_, key_code, 1 | 2) => {
                                            if let Some(bind) = evdev_keycode_to_bind(key_code) {
                                                let _ =
                                                    tx.send(KeyEdge { bind, pressed: true });
                                                changed = true;
                                            }
                                        }
                                        EventSummary::Key(_, key_code, 0) => {
                                            if let Some(bind) = evdev_keycode_to_bind(key_code) {
                                                let _ =
                                                    tx.send(KeyEdge { bind, pressed: false });
                                                changed = true;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                if changed {
                                    ctx.request_repaint();
                                }
                            }
                            Err(e) => {
                                eprintln!("[evdev] lost {}: {e}", dev_name_clone);
                                break;
                            }
                        }
                    });

                if let Err(e) = result {
                    eprintln!("[evdev] failed to spawn reader for {dev_name}: {e}");
                    readers -= 1;
                }
            }

            eprintln!("[evdev] {readers}/{total} devices open for reading");

            if readers == 0 {
                eprintln!("[evdev] !! no input devices could be opened — key highlighting disabled");
                eprintln!("[evdev] !! fix: sudo usermod -aG input $USER  (then log out/in)");
                if total == 0 {
                    eprintln!("[evdev] !! no devices found at all — check /dev/input/ permissions");
                }
            }
        })
        .context("failed to spawn global key reader thread")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::KeyCode;

    // ── extract_tap_key ──────────────────────────────────────────────────────

    #[test]
    fn extract_tap_lt() {
        assert_eq!(extract_tap_key("LT(1, KC_A)"), Some("KC_A"));
        assert_eq!(extract_tap_key("LT(3, KC_ESC)"), Some("KC_ESC"));
    }

    #[test]
    fn extract_tap_mt() {
        assert_eq!(extract_tap_key("MT(LSFT, KC_A)"), Some("KC_A"));
        assert_eq!(extract_tap_key("LSFT_T(KC_SPC)"), Some("KC_SPC"));
        assert_eq!(extract_tap_key("LCTL_T(KC_TAB)"), Some("KC_TAB"));
        assert_eq!(extract_tap_key("MEH_T(KC_ENT)"), Some("KC_ENT"));
        assert_eq!(extract_tap_key("HYPR_T(KC_BSPC)"), Some("KC_BSPC"));
    }

    #[test]
    fn extract_tap_plain_returns_none() {
        assert_eq!(extract_tap_key("KC_A"), None);
        assert_eq!(extract_tap_key("KC_ENT"), None);
        assert_eq!(extract_tap_key("MO(1)"), None);
        assert_eq!(extract_tap_key("TG(2)"), None);
    }

    #[test]
    fn extract_tap_mod_wrapper_returns_none() {
        assert_eq!(extract_tap_key("LGUI(KC_A)"), None);
        assert_eq!(extract_tap_key("LSFT(KC_A)"), None);
    }

    #[test]
    fn extract_mod_combo_basic() {
        assert_eq!(extract_mod_combo("LGUI(KC_1)"), Some(("KC_LGUI", "KC_1")));
        assert_eq!(extract_mod_combo("LCTL(KC_C)"), Some(("KC_LCTL", "KC_C")));
        assert_eq!(extract_mod_combo("LSFT(KC_Z)"), Some(("KC_LSFT", "KC_Z")));
        assert_eq!(extract_mod_combo("KC_A"), None);
        assert_eq!(extract_mod_combo("MO(1)"), None);
    }

    #[test]
    fn highlight_mod_combo_both_active() {
        let mut held = HashSet::new();
        held.insert("KC_LGUI".to_string());
        held.insert("KC_1".to_string());
        assert!(is_keybind_highlighted("LGUI(KC_1)", &held));
    }

    #[test]
    fn no_highlight_mod_combo_only_inner() {
        let mut held = HashSet::new();
        held.insert("KC_1".to_string());
        assert!(!is_keybind_highlighted("LGUI(KC_1)", &held));
    }

    // ── is_keybind_highlighted ───────────────────────────────────────────────

    #[test]
    fn highlight_held_plain() {
        let mut held = HashSet::new();
        held.insert("KC_A".to_string());
        assert!(is_keybind_highlighted("KC_A", &held));
        assert!(!is_keybind_highlighted("KC_B", &held));
    }

    #[test]
    fn no_highlight_when_released() {
        let held = HashSet::new();
        assert!(!is_keybind_highlighted("KC_A", &held));
    }

    #[test]
    fn highlight_tap_key_via_lt() {
        let mut held = HashSet::new();
        held.insert("KC_A".to_string());
        assert!(is_keybind_highlighted("LT(1, KC_A)", &held));
    }

    #[test]
    fn highlight_tap_key_via_lsft_t() {
        let mut held = HashSet::new();
        held.insert("KC_SPC".to_string());
        assert!(is_keybind_highlighted("LSFT_T(KC_SPC)", &held));
    }

    // ── evdev_keycode_to_bind ────────────────────────────────────────────────

    #[test]
    fn evdev_letters() {
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_A), Some("KC_A".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_Z), Some("KC_Z".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_M), Some("KC_M".into()));
    }

    #[test]
    fn evdev_digits() {
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_1), Some("KC_1".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_0), Some("KC_0".into()));
    }

    #[test]
    fn evdev_navigation() {
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_ESC), Some("KC_ESC".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_ENTER), Some("KC_ENT".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_BACKSPACE), Some("KC_BSPC".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_DELETE), Some("KC_DEL".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_HOME), Some("KC_HOME".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_END), Some("KC_END".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_LEFT), Some("KC_LEFT".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_RIGHT), Some("KC_RGHT".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_UP), Some("KC_UP".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_DOWN), Some("KC_DOWN".into()));
    }

    #[test]
    fn evdev_modifiers() {
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_LEFTSHIFT), Some("KC_LSFT".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_RIGHTSHIFT), Some("KC_RSFT".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_LEFTCTRL), Some("KC_LCTL".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_RIGHTCTRL), Some("KC_RCTL".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_LEFTALT), Some("KC_LALT".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_RIGHTALT), Some("KC_RALT".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_LEFTMETA), Some("KC_LGUI".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_RIGHTMETA), Some("KC_RGUI".into()));
    }

    #[test]
    fn evdev_media() {
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_MUTE), Some("KC_MUTE".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_VOLUMEUP), Some("KC_VOLU".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_VOLUMEDOWN), Some("KC_VOLD".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_NEXTSONG), Some("KC_MNXT".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_PREVIOUSSONG), Some("KC_MPRV".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_PLAYPAUSE), Some("KC_MPLY".into()));
    }

    #[test]
    fn evdev_browser() {
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_BACK), Some("KC_WBAK".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_FORWARD), Some("KC_WFWD".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_HOMEPAGE), Some("KC_WHOM".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_REFRESH), Some("KC_WREF".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_BOOKMARKS), Some("KC_WFAV".into()));
    }

    #[test]
    fn evdev_editing() {
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_UNDO), Some("KC_UNDO".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_COPY), Some("KC_COPY".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_PASTE), Some("KC_PSTE".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_CUT), Some("KC_CUT".into()));
    }

    #[test]
    fn evdev_function_keys() {
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_F1), Some("KC_F1".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_F12), Some("KC_F12".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_F13), Some("KC_F13".into()));
        assert_eq!(evdev_keycode_to_bind(KeyCode::KEY_F24), Some("KC_F24".into()));
    }

    // ── keycode_name ↔ evdev consistency ─────────────────────────────────────

    #[test]
    fn f13_f24_consistent() {
        use crate::keycode::keycode_name;
        // evdev KEY_F13..F24 must produce the same string as keycode_name(HID 0x68..0x73)
        let pairs: &[(u16, KeyCode)] = &[
            (0x68, KeyCode::KEY_F13),
            (0x6F, KeyCode::KEY_F20),
            (0x73, KeyCode::KEY_F24),
        ];
        for (hid, key) in pairs {
            let qmk = keycode_name(*hid);
            let evdv = evdev_keycode_to_bind(*key).unwrap_or_default();
            assert_eq!(qmk, evdv, "mismatch for HID 0x{hid:02X}");
        }
    }
}

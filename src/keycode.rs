pub fn keycode_name(keycode: u16) -> String {
    if keycode <= 0xFF {
        return basic_key_name(keycode);
    }

    if (0x0100..=0x1FFF).contains(&keycode) {
        let mods = (keycode >> 8) & 0x1F;
        let base = keycode & 0xFF;
        if mods == 0x02 {
            if let Some(alias) = us_shift_alias(base) {
                return alias.to_string();
            }
        }
        return wrap_mods(basic_key_name(base), mods);
    }

    if (0x2000..=0x3FFF).contains(&keycode) {
        let mods = (keycode >> 8) & 0x1F;
        let tap = keycode & 0xFF;
        if let Some(alias) = single_mod_alias(mods) {
            return format!("{alias}_T({})", keycode_name(tap));
        }
        if mods == 0x07 || mods == 0x0F || mods == 0x1F {
            return format!("{}_T({})", format_mods(mods), keycode_name(tap));
        }
        return format!("MT({}, {})", format_mods(mods), keycode_name(tap));
    }

    if (0x4000..=0x4FFF).contains(&keycode) {
        let layer = (keycode >> 8) & 0x0F;
        let tap = keycode & 0xFF;
        return format!("LT({layer}, {})", keycode_name(tap));
    }

    if (0x5000..=0x51FF).contains(&keycode) {
        let layer = (keycode >> 5) & 0x0F;
        let mods = keycode & 0x1F;
        return format!("LM({layer}, {})", format_mods(mods));
    }

    if (0x5200..=0x521F).contains(&keycode) {
        return format!("TO({})", keycode & 0x1F);
    }
    if (0x5220..=0x523F).contains(&keycode) {
        return format!("MO({})", keycode & 0x1F);
    }
    if (0x5240..=0x525F).contains(&keycode) {
        return format!("DF({})", keycode & 0x1F);
    }
    if (0x5260..=0x527F).contains(&keycode) {
        return format!("TG({})", keycode & 0x1F);
    }
    if (0x5280..=0x529F).contains(&keycode) {
        return format!("OSL({})", keycode & 0x1F);
    }
    if (0x52A0..=0x52BF).contains(&keycode) {
        return format!("OSM({})", format_mods(keycode & 0x1F));
    }
    if (0x52C0..=0x52DF).contains(&keycode) {
        return format!("TT({})", keycode & 0x1F);
    }
    if (0x52E0..=0x52FF).contains(&keycode) {
        return format!("PDF({})", keycode & 0x1F);
    }

    if (0x5600..=0x56FF).contains(&keycode) {
        let special = match keycode {
            0x56F0 => Some("QK_SWAP_HANDS_TOGGLE"),
            0x56F1 => Some("QK_SWAP_HANDS_TAP_TOGGLE"),
            0x56F2 => Some("QK_SWAP_HANDS_MOMENTARY_ON"),
            0x56F3 => Some("QK_SWAP_HANDS_MOMENTARY_OFF"),
            0x56F4 => Some("QK_SWAP_HANDS_OFF"),
            0x56F5 => Some("QK_SWAP_HANDS_ON"),
            0x56F6 => Some("QK_SWAP_HANDS_ONE_SHOT"),
            _ => None,
        };
        if let Some(name) = special {
            return name.to_string();
        }
        return format!("SH_T({})", keycode_name(keycode & 0xFF));
    }

    if (0x5700..=0x57FF).contains(&keycode) {
        return format!("TD({})", keycode & 0xFF);
    }

    if let Some(name) = special_magic_name(keycode) {
        return name.to_string();
    }

    if keycode == 0x7100 {
        return "QK_MIDI_ON".into();
    }
    if keycode == 0x7101 {
        return "QK_MIDI_OFF".into();
    }
    if keycode == 0x7102 {
        return "QK_MIDI_TOGGLE".into();
    }
    if (0x7103..=0x714A).contains(&keycode) {
        let midi_notes = [
            "C", "C_SHARP", "D", "D_SHARP", "E", "F", "F_SHARP", "G", "G_SHARP", "A", "A_SHARP",
            "B",
        ];
        let idx = (keycode - 0x7103) as usize;
        let note = midi_notes[idx % 12];
        let octave = idx / 12;
        return format!("QK_MIDI_NOTE_{note}_{octave}");
    }
    if (0x714B..=0x7154).contains(&keycode) {
        let suffixes = ["N2", "N1", "0", "1", "2", "3", "4", "5", "6", "7"];
        return format!("QK_MIDI_OCTAVE_{}", suffixes[(keycode - 0x714B) as usize]);
    }
    if keycode == 0x7155 {
        return "QK_MIDI_OCTAVE_DOWN".into();
    }
    if keycode == 0x7156 {
        return "QK_MIDI_OCTAVE_UP".into();
    }
    if (0x7157..=0x7163).contains(&keycode) {
        let suffixes = [
            "N6", "N5", "N4", "N3", "N2", "N1", "0", "1", "2", "3", "4", "5", "6",
        ];
        return format!(
            "QK_MIDI_TRANSPOSE_{}",
            suffixes[(keycode - 0x7157) as usize]
        );
    }
    if keycode == 0x7164 {
        return "QK_MIDI_TRANSPOSE_DOWN".into();
    }
    if keycode == 0x7165 {
        return "QK_MIDI_TRANSPOSE_UP".into();
    }
    if (0x7166..=0x7170).contains(&keycode) {
        return format!("QK_MIDI_VELOCITY_{}", keycode - 0x7166);
    }
    if keycode == 0x7171 {
        return "QK_MIDI_VELOCITY_DOWN".into();
    }
    if keycode == 0x7172 {
        return "QK_MIDI_VELOCITY_UP".into();
    }
    if (0x7173..=0x7182).contains(&keycode) {
        return format!("QK_MIDI_CHANNEL_{}", keycode - 0x7173 + 1);
    }

    if (0x7200..=0x73FF).contains(&keycode) {
        return format!("QK_SEQUENCER+{}", keycode - 0x7200);
    }
    if (0x7480..=0x74BF).contains(&keycode) {
        return format!("QK_AUDIO+{}", keycode - 0x7480);
    }
    if (0x74C0..=0x74FF).contains(&keycode) {
        return format!("QK_STENO+{}", keycode - 0x74C0);
    }
    if (0x7780..=0x77BF).contains(&keycode) {
        return format!("QK_CONNECTION+{}", keycode - 0x7780);
    }
    if (0x7800..=0x78FF).contains(&keycode) {
        return format!("QK_LIGHTING+{}", keycode - 0x7800);
    }
    if (0x7C00..=0x7DFF).contains(&keycode) {
        if keycode == 0x7C00 {
            return "QK_BOOT".into();
        }
        return format!("QK_QUANTUM+{}", keycode - 0x7C00);
    }

    format!("0x{keycode:04X}")
}

/// US-layout (shifted, base) glyph pair for keys whose output changes with Shift.
pub fn us_shift_pair(bind: &str) -> Option<(&'static str, &'static str)> {
    let pair = match bind {
        "KC_GRV"  => ("~", "`"),
        "KC_1"    => ("!", "1"),
        "KC_2"    => ("@", "2"),
        "KC_3"    => ("#", "3"),
        "KC_4"    => ("$", "4"),
        "KC_5"    => ("%", "5"),
        "KC_6"    => ("^", "6"),
        "KC_7"    => ("&", "7"),
        "KC_8"    => ("*", "8"),
        "KC_9"    => ("(", "9"),
        "KC_0"    => (")", "0"),
        "KC_MINS" => ("_", "-"),
        "KC_EQL"  => ("+", "="),
        "KC_LBRC" => ("{", "["),
        "KC_RBRC" => ("}", "]"),
        "KC_BSLS" => ("|", "\\"),
        "KC_SCLN" => (":", ";"),
        "KC_QUOT" => ("\"", "'"),
        "KC_COMM" => ("<", ","),
        "KC_DOT"  => (">", "."),
        "KC_SLSH" => ("?", "/"),
        _ => return None,
    };
    Some(pair)
}

/// Label reflecting what the key produces right now: shifted glyph when Shift
/// is held, otherwise the base glyph (or the normal label for other keys).
pub fn active_label(bind: &str, shift_held: bool) -> String {
    if let Some((shifted, base)) = us_shift_pair(bind) {
        return if shift_held { shifted } else { base }.to_string();
    }
    pretty_bind_label(bind)
}

pub fn pretty_bind_label(bind: &str) -> String {
    if bind == "KC_TRNS" || bind == "KC_NO" {
        return String::new();
    }

    if let Some((shifted, base)) = us_shift_pair(bind) {
        return format!("{shifted}\n{base}");
    }

    let mapping: Option<&str> = match bind {
        // Navigation & editing
        "KC_TAB"  => Some("Tab"),
        "KC_ESC"  => Some("Esc"),
        "KC_ENT"  => Some("Enter"),
        "KC_BSPC" => Some("Bksp"),
        "KC_SPC"  => Some("Space"),
        "KC_INS"  => Some("Ins"),
        "KC_DEL"  => Some("Del"),
        "KC_HOME" => Some("Home"),
        "KC_END"  => Some("End"),
        "KC_PGUP" => Some("PgUp"),
        "KC_PGDN" => Some("PgDn"),
        "KC_LEFT" => Some("Left"),
        "KC_DOWN" => Some("Down"),
        "KC_UP"   => Some("Up"),
        "KC_RGHT" => Some("Right"),
        // Modifiers
        "KC_LSFT" => Some("LShift"),
        "KC_RSFT" => Some("RShift"),
        "KC_LCTL" => Some("LCtrl"),
        "KC_RCTL" => Some("RCtrl"),
        "KC_LALT" => Some("LAlt"),
        "KC_RALT" => Some("RAlt"),
        "KC_LGUI" => Some("LGui"),
        "KC_RGUI" => Some("RGui"),
        // Lock keys
        "KC_CAPS" => Some("Caps"),
        "KC_NLCK" => Some("Num"),
        "KC_SCRL" => Some("Scroll Lock"),
        // System
        "KC_PSCR" => Some("Print Screen"),
        "KC_PAUS" => Some("Pause"),
        "KC_APP"  => Some("Menu"),
        "KC_MENU" => Some("Menu"),
        "KC_PWR"  => Some("Power"),
        "KC_SLEP" => Some("Sleep"),
        "KC_WAKE" => Some("Wake"),
        // Application control keys
        "KC_EXEC" => Some("Exec"),
        "KC_HELP" => Some("Help"),
        "KC_SLCT" => Some("Sel"),
        "KC_STOP" => Some("Stop"),
        "KC_AGIN" => Some("Again"),
        "KC_UNDO" => Some("Undo"),
        "KC_CUT"  => Some("Cut"),
        "KC_COPY" => Some("Copy"),
        "KC_PSTE" => Some("Paste"),
        "KC_FIND" => Some("Find"),
        // Numpad
        "KC_PSLH" => Some("/"),
        "KC_PAST" => Some("*"),
        "KC_PMNS" => Some("-"),
        "KC_PPLS" => Some("+"),
        "KC_PENT" => Some("Enter"),
        "KC_PDOT" => Some("."),
        "KC_PEQL" => Some("="),
        "KC_NUBS" => Some("\\"),
        "KC_NUHS" => Some("#"),
        // Symbols (standalone named keys)
        "KC_EXLM" => Some("!"),
        "KC_AT"   => Some("@"),
        "KC_HASH" => Some("#"),
        "KC_DLR"  => Some("$"),
        "KC_PERC" => Some("%"),
        "KC_CIRC" => Some("^"),
        "KC_AMPR" => Some("&"),
        "KC_ASTR" => Some("*"),
        "KC_LPRN" => Some("("),
        "KC_RPRN" => Some(")"),
        "KC_UNDS" => Some("_"),
        "KC_PLUS" => Some("+"),
        "KC_LCBR" => Some("{"),
        "KC_RCBR" => Some("}"),
        "KC_PIPE" => Some("|"),
        "KC_COLN" => Some(":"),
        "KC_DQUO" => Some("\""),
        "KC_TILD" => Some("~"),
        "KC_LABK" => Some("<"),
        "KC_RABK" => Some(">"),
        "KC_QUES" => Some("?"),
        // Media
        "KC_MUTE" => Some("Mute"),
        "KC_VOLU" => Some("Vol+"),
        "KC_VOLD" => Some("Vol-"),
        "KC_MNXT" => Some("Next"),
        "KC_MPRV" => Some("Prev"),
        "KC_MSTP" => Some("Stop"),
        "KC_MPLY" => Some("Play"),
        "KC_MSEL" => Some("Media"),
        "KC_EJCT" => Some("Eject"),
        "KC_MFFD" => Some("FF"),
        "KC_MRWD" => Some("RW"),
        // Browser
        "KC_WHOM" => Some("W.Home"),
        "KC_WBAK" => Some("Back"),
        "KC_WFWD" => Some("Fwd"),
        "KC_WSTP" => Some("W.Stop"),
        "KC_WREF" => Some("Refresh"),
        "KC_WFAV" => Some("Bkmk"),
        "KC_WSCH" => Some("Search"),
        // Brightness
        "KC_BRIU" => Some("Bri+"),
        "KC_BRID" => Some("Bri-"),
        // Application launchers
        "KC_MAIL" => Some("Mail"),
        "KC_CALC" => Some("Calc"),
        "KC_MYCM" => Some("PC"),
        _ => None,
    };
    if let Some(text) = mapping {
        return text.to_string();
    }

    if let Some(rest) = bind.strip_prefix("KC_") {
        if rest.len() == 1 {
            return rest.to_string();
        }
        if let Some(num) = rest.strip_prefix('F') {
            if num.chars().all(|ch| ch.is_ascii_digit()) {
                return format!("F{num}");
            }
        }
        return rest.to_string();
    }

    if let Some(inner) = bind.strip_prefix("LGUI(").and_then(|s| s.strip_suffix(')')) {
        return format!("LGui+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind.strip_prefix("LCTL(").and_then(|s| s.strip_suffix(')')) {
        return format!("LCtrl+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind.strip_prefix("LSFT(").and_then(|s| s.strip_suffix(')')) {
        return format!("LShift+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind.strip_prefix("LALT(").and_then(|s| s.strip_suffix(')')) {
        return format!("LAlt+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind.strip_prefix("RGUI(").and_then(|s| s.strip_suffix(')')) {
        return format!("RGui+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind
        .strip_prefix("RCtrl(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return format!("RCtrl+{}", pretty_bind_label(inner));
    }
    bind.to_string()
}

fn basic_key_name(keycode: u16) -> String {
    match keycode {
        0x00 => "KC_NO".into(),
        0x01 => "KC_TRNS".into(),
        0x02 => "KC_ERROR_ROLL_OVER".into(),
        0x03 => "KC_POST_FAIL".into(),
        // Letters A–Z (HID 0x04–0x1D)
        0x04..=0x1D => format!("KC_{}", (b'A' + (keycode - 0x04) as u8) as char),
        // Digits 1–0 (HID 0x1E–0x27)
        0x1E..=0x27 => format!("KC_{}", (keycode - 0x1E + 1) % 10),
        // Common keys
        0x28 => "KC_ENT".into(),
        0x29 => "KC_ESC".into(),
        0x2A => "KC_BSPC".into(),
        0x2B => "KC_TAB".into(),
        0x2C => "KC_SPC".into(),
        0x2D => "KC_MINS".into(),
        0x2E => "KC_EQL".into(),
        0x2F => "KC_LBRC".into(),
        0x30 => "KC_RBRC".into(),
        0x31 => "KC_BSLS".into(),
        0x32 => "KC_NUHS".into(), // Non-US # and ~
        0x33 => "KC_SCLN".into(),
        0x34 => "KC_QUOT".into(),
        0x35 => "KC_GRV".into(),
        0x36 => "KC_COMM".into(),
        0x37 => "KC_DOT".into(),
        0x38 => "KC_SLSH".into(),
        0x39 => "KC_CAPS".into(),
        // F1–F12 (HID 0x3A–0x45)
        0x3A..=0x45 => format!("KC_F{}", keycode - 0x39),
        // System / navigation
        0x46 => "KC_PSCR".into(),
        0x47 => "KC_SCRL".into(),
        0x48 => "KC_PAUS".into(),
        0x49 => "KC_INS".into(),
        0x4A => "KC_HOME".into(),
        0x4B => "KC_PGUP".into(),
        0x4C => "KC_DEL".into(),
        0x4D => "KC_END".into(),
        0x4E => "KC_PGDN".into(),
        0x4F => "KC_RGHT".into(),
        0x50 => "KC_LEFT".into(),
        0x51 => "KC_DOWN".into(),
        0x52 => "KC_UP".into(),
        // Numpad
        0x53 => "KC_NLCK".into(),
        0x54 => "KC_PSLH".into(),
        0x55 => "KC_PAST".into(),
        0x56 => "KC_PMNS".into(),
        0x57 => "KC_PPLS".into(),
        0x58 => "KC_PENT".into(),
        0x59 => "KC_P1".into(),
        0x5A => "KC_P2".into(),
        0x5B => "KC_P3".into(),
        0x5C => "KC_P4".into(),
        0x5D => "KC_P5".into(),
        0x5E => "KC_P6".into(),
        0x5F => "KC_P7".into(),
        0x60 => "KC_P8".into(),
        0x61 => "KC_P9".into(),
        0x62 => "KC_P0".into(),
        0x63 => "KC_PDOT".into(),
        0x64 => "KC_NUBS".into(), // Non-US backslash
        0x65 => "KC_APP".into(),  // Application / context-menu
        0x66 => "KC_PWR".into(),  // Keyboard Power (HID 0x66)
        0x67 => "KC_PEQL".into(), // Keypad = (common on Mac)
        // F13–F24 (HID 0x68–0x73); formula: keycode - 0x5B gives 13..24
        0x68..=0x73 => format!("KC_F{}", keycode - 0x5B),
        // Application control (HID 0x74–0x7E)
        0x74 => "KC_EXEC".into(),
        0x75 => "KC_HELP".into(),
        0x76 => "KC_MENU".into(),
        0x77 => "KC_SLCT".into(),
        0x78 => "KC_STOP".into(),
        0x79 => "KC_AGIN".into(),
        0x7A => "KC_UNDO".into(),
        0x7B => "KC_CUT".into(),
        0x7C => "KC_COPY".into(),
        0x7D => "KC_PSTE".into(),
        0x7E => "KC_FIND".into(),
        // International keys (HID 0x87–0x8F)
        0x87 => "KC_INT1".into(),
        0x88 => "KC_INT2".into(),
        0x89 => "KC_INT3".into(),
        0x8A => "KC_INT4".into(),
        0x8B => "KC_INT5".into(),
        0x8C => "KC_INT6".into(),
        0x8D => "KC_INT7".into(),
        0x8E => "KC_INT8".into(),
        0x8F => "KC_INT9".into(),
        // Language keys (HID 0x90–0x98)
        0x90 => "KC_LNG1".into(),
        0x91 => "KC_LNG2".into(),
        0x92 => "KC_LNG3".into(),
        0x93 => "KC_LNG4".into(),
        0x94 => "KC_LNG5".into(),
        0x95 => "KC_LNG6".into(),
        0x96 => "KC_LNG7".into(),
        0x97 => "KC_LNG8".into(),
        0x98 => "KC_LNG9".into(),
        // System control (QMK internal: 0xA5–0xA7)
        0xA5 => "KC_PWR".into(),
        0xA6 => "KC_SLEP".into(),
        0xA7 => "KC_WAKE".into(),
        // Consumer / media control (QMK internal: 0xA8–0xBA)
        0xA8 => "KC_MUTE".into(),
        0xA9 => "KC_VOLU".into(),
        0xAA => "KC_VOLD".into(),
        0xAB => "KC_MNXT".into(),
        0xAC => "KC_MPRV".into(),
        0xAD => "KC_MSTP".into(),
        0xAE => "KC_MPLY".into(),
        0xAF => "KC_MSEL".into(),
        0xB0 => "KC_EJCT".into(),
        0xB1 => "KC_WHOM".into(), // AL Browser Home
        0xB2 => "KC_WBAK".into(), // AC Back
        0xB3 => "KC_WFWD".into(), // AC Forward
        0xB4 => "KC_WSTP".into(), // AC Stop
        0xB5 => "KC_WREF".into(), // AC Refresh
        0xB6 => "KC_WFAV".into(), // AC Bookmarks
        0xB7 => "KC_MFFD".into(), // Fast Forward
        0xB8 => "KC_MRWD".into(), // Rewind
        0xB9 => "KC_BRIU".into(), // Brightness Up
        0xBA => "KC_BRID".into(), // Brightness Down
        // Mouse movement (QMK internal: 0xCD–0xD8)
        0xCD => "KC_MS_UP".into(),
        0xCE => "KC_MS_DOWN".into(),
        0xCF => "KC_MS_LEFT".into(),
        0xD0 => "KC_MS_RIGHT".into(),
        0xD1 => "KC_BTN1".into(),
        0xD2 => "KC_BTN2".into(),
        0xD3 => "KC_BTN3".into(),
        0xD4 => "KC_BTN4".into(),
        0xD5 => "KC_BTN5".into(),
        0xD6 => "KC_BTN6".into(),
        0xD7 => "KC_BTN7".into(),
        0xD8 => "KC_BTN8".into(),
        0xD9 => "MS_WHLU".into(),
        0xDA => "MS_WHLD".into(),
        0xDB => "MS_WHLL".into(),
        0xDC => "MS_WHLR".into(),
        // Modifiers (HID 0xE0–0xE7)
        0xE0 => "KC_LCTL".into(),
        0xE1 => "KC_LSFT".into(),
        0xE2 => "KC_LALT".into(),
        0xE3 => "KC_LGUI".into(),
        0xE4 => "KC_RCTL".into(),
        0xE5 => "KC_RSFT".into(),
        0xE6 => "KC_RALT".into(),
        0xE7 => "KC_RGUI".into(),
        _ => format!("0x{keycode:02X}"),
    }
}

fn mod_names(mods: u16) -> Vec<&'static str> {
    if mods == 0x0F || mods == 0x1F {
        return vec!["HYPR"];
    }
    if mods == 0x07 {
        return vec!["MEH"];
    }

    let side_bits: &[(u16, &str)] = if (mods & 0x10) != 0 {
        &[(0x01, "RCTL"), (0x02, "RSFT"), (0x04, "RALT"), (0x08, "RGUI")]
    } else {
        &[(0x01, "LCTL"), (0x02, "LSFT"), (0x04, "LALT"), (0x08, "LGUI")]
    };

    side_bits
        .iter()
        .filter_map(|(bit, name)| ((mods & bit) != 0).then_some(*name))
        .collect()
}

fn format_mods(mods: u16) -> String {
    let names = mod_names(mods);
    if names.is_empty() {
        format!("0x{mods:X}")
    } else {
        names.join("|")
    }
}

fn wrap_mods(base: String, mods: u16) -> String {
    let names = mod_names(mods);
    if names.is_empty() {
        return base;
    }
    names
        .iter()
        .rev()
        .fold(base, |acc, name| format!("{name}({acc})"))
}

fn us_shift_alias(base: u16) -> Option<&'static str> {
    match base {
        0x1E => Some("KC_EXLM"),
        0x1F => Some("KC_AT"),
        0x20 => Some("KC_HASH"),
        0x21 => Some("KC_DLR"),
        0x22 => Some("KC_PERC"),
        0x23 => Some("KC_CIRC"),
        0x24 => Some("KC_AMPR"),
        0x25 => Some("KC_ASTR"),
        0x26 => Some("KC_LPRN"),
        0x27 => Some("KC_RPRN"),
        0x2D => Some("KC_UNDS"),
        0x2E => Some("KC_PLUS"),
        0x2F => Some("KC_LCBR"),
        0x30 => Some("KC_RCBR"),
        0x31 => Some("KC_PIPE"),
        0x33 => Some("KC_COLN"),
        0x34 => Some("KC_DQUO"),
        0x35 => Some("KC_TILD"),
        0x36 => Some("KC_LABK"),
        0x37 => Some("KC_RABK"),
        0x38 => Some("KC_QUES"),
        _ => None,
    }
}

fn single_mod_alias(mods: u16) -> Option<&'static str> {
    match mods {
        0x01 => Some("LCTL"),
        0x02 => Some("LSFT"),
        0x04 => Some("LALT"),
        0x08 => Some("LGUI"),
        0x11 => Some("RCTL"),
        0x12 => Some("RSFT"),
        0x14 => Some("RALT"),
        0x18 => Some("RGUI"),
        _ => None,
    }
}

fn special_magic_name(keycode: u16) -> Option<&'static str> {
    match keycode {
        0x7000 => Some("QK_MAGIC_SWAP_CONTROL_CAPS_LOCK"),
        0x7001 => Some("QK_MAGIC_UNSWAP_CONTROL_CAPS_LOCK"),
        0x7002 => Some("QK_MAGIC_TOGGLE_CONTROL_CAPS_LOCK"),
        0x7003 => Some("QK_MAGIC_CAPS_LOCK_AS_CONTROL_OFF"),
        0x7004 => Some("QK_MAGIC_CAPS_LOCK_AS_CONTROL_ON"),
        0x7005 => Some("QK_MAGIC_SWAP_LALT_LGUI"),
        0x7006 => Some("QK_MAGIC_UNSWAP_LALT_LGUI"),
        0x7007 => Some("QK_MAGIC_SWAP_RALT_RGUI"),
        0x7008 => Some("QK_MAGIC_UNSWAP_RALT_RGUI"),
        0x7009 => Some("QK_MAGIC_GUI_ON"),
        0x700A => Some("QK_MAGIC_GUI_OFF"),
        0x700B => Some("QK_MAGIC_TOGGLE_GUI"),
        0x700C => Some("QK_MAGIC_SWAP_GRAVE_ESC"),
        0x700D => Some("QK_MAGIC_UNSWAP_GRAVE_ESC"),
        0x700E => Some("QK_MAGIC_SWAP_BACKSLASH_BACKSPACE"),
        0x700F => Some("QK_MAGIC_UNSWAP_BACKSLASH_BACKSPACE"),
        0x7010 => Some("QK_MAGIC_TOGGLE_BACKSLASH_BACKSPACE"),
        0x7011 => Some("QK_MAGIC_NKRO_ON"),
        0x7012 => Some("QK_MAGIC_NKRO_OFF"),
        0x7013 => Some("QK_MAGIC_TOGGLE_NKRO"),
        0x7014 => Some("QK_MAGIC_SWAP_ALT_GUI"),
        0x7015 => Some("QK_MAGIC_UNSWAP_ALT_GUI"),
        0x7016 => Some("QK_MAGIC_TOGGLE_ALT_GUI"),
        0x7017 => Some("QK_MAGIC_SWAP_LCTL_LGUI"),
        0x7018 => Some("QK_MAGIC_UNSWAP_LCTL_LGUI"),
        0x7019 => Some("QK_MAGIC_SWAP_RCTL_RGUI"),
        0x701A => Some("QK_MAGIC_UNSWAP_RCTL_RGUI"),
        0x701B => Some("QK_MAGIC_SWAP_CTL_GUI"),
        0x701C => Some("QK_MAGIC_UNSWAP_CTL_GUI"),
        0x701D => Some("QK_MAGIC_TOGGLE_CTL_GUI"),
        0x701E => Some("QK_MAGIC_EE_HANDS_LEFT"),
        0x701F => Some("QK_MAGIC_EE_HANDS_RIGHT"),
        0x7020 => Some("QK_MAGIC_SWAP_ESCAPE_CAPS_LOCK"),
        0x7021 => Some("QK_MAGIC_UNSWAP_ESCAPE_CAPS_LOCK"),
        0x7022 => Some("QK_MAGIC_TOGGLE_ESCAPE_CAPS_LOCK"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_letter() {
        assert_eq!(pretty_bind_label("KC_A"), "A");
        assert_eq!(pretty_bind_label("KC_Z"), "Z");
    }

    #[test]
    fn label_shifted_pair() {
        let label = pretty_bind_label("KC_1");
        assert!(label.contains('1') && label.contains('!'), "label: {label}");
    }

    #[test]
    fn label_special_keys() {
        assert_eq!(pretty_bind_label("KC_ESC"), "Esc");
        assert_eq!(pretty_bind_label("KC_TAB"), "Tab");
        assert_eq!(pretty_bind_label("KC_SPC"), "Space");
        assert_eq!(pretty_bind_label("KC_BSPC"), "Bksp");
        assert_eq!(pretty_bind_label("KC_ENT"), "Enter");
        assert_eq!(pretty_bind_label("KC_APP"), "Menu");
        assert_eq!(pretty_bind_label("KC_TRNS"), "");
        assert_eq!(pretty_bind_label("KC_NO"), "");
    }

    #[test]
    fn label_fkey() {
        assert_eq!(pretty_bind_label("KC_F1"), "F1");
        assert_eq!(pretty_bind_label("KC_F12"), "F12");
        assert_eq!(pretty_bind_label("KC_F13"), "F13");
        assert_eq!(pretty_bind_label("KC_F24"), "F24");
    }

    #[test]
    fn label_media() {
        assert_eq!(pretty_bind_label("KC_MUTE"), "Mute");
        assert_eq!(pretty_bind_label("KC_VOLU"), "Vol+");
        assert_eq!(pretty_bind_label("KC_MPLY"), "Play");
        assert_eq!(pretty_bind_label("KC_BRIU"), "Bri+");
        assert_eq!(pretty_bind_label("KC_BRID"), "Bri-");
    }

    #[test]
    fn label_browser() {
        assert_eq!(pretty_bind_label("KC_WBAK"), "Back");
        assert_eq!(pretty_bind_label("KC_WFWD"), "Fwd");
        assert_eq!(pretty_bind_label("KC_WREF"), "Refresh");
        assert_eq!(pretty_bind_label("KC_WHOM"), "W.Home");
    }

    #[test]
    fn label_editing() {
        assert_eq!(pretty_bind_label("KC_UNDO"), "Undo");
        assert_eq!(pretty_bind_label("KC_COPY"), "Copy");
        assert_eq!(pretty_bind_label("KC_PSTE"), "Paste");
        assert_eq!(pretty_bind_label("KC_CUT"), "Cut");
    }

    #[test]
    fn keycode_name_letters() {
        assert_eq!(keycode_name(0x04), "KC_A");
        assert_eq!(keycode_name(0x1D), "KC_Z");
    }

    #[test]
    fn keycode_name_digits() {
        assert_eq!(keycode_name(0x1E), "KC_1");
        assert_eq!(keycode_name(0x27), "KC_0");
    }

    #[test]
    fn keycode_name_common() {
        assert_eq!(keycode_name(0x28), "KC_ENT");
        assert_eq!(keycode_name(0x29), "KC_ESC");
        assert_eq!(keycode_name(0x2C), "KC_SPC");
        assert_eq!(keycode_name(0x65), "KC_APP");
    }

    #[test]
    fn keycode_name_fkeys() {
        // F1–F12 (0x3A–0x45)
        assert_eq!(keycode_name(0x3A), "KC_F1");
        assert_eq!(keycode_name(0x45), "KC_F12");
        // F13–F24 (0x68–0x73) — was broken before, now correct
        assert_eq!(keycode_name(0x68), "KC_F13");
        assert_eq!(keycode_name(0x6F), "KC_F20");
        assert_eq!(keycode_name(0x73), "KC_F24");
    }

    #[test]
    fn keycode_name_modifiers() {
        assert_eq!(keycode_name(0xE0), "KC_LCTL");
        assert_eq!(keycode_name(0xE1), "KC_LSFT");
        assert_eq!(keycode_name(0xE4), "KC_RCTL");
    }

    #[test]
    fn keycode_name_media() {
        assert_eq!(keycode_name(0xA8), "KC_MUTE");
        assert_eq!(keycode_name(0xA9), "KC_VOLU");
        assert_eq!(keycode_name(0xAE), "KC_MPLY");
        assert_eq!(keycode_name(0xB0), "KC_EJCT");
        assert_eq!(keycode_name(0xB9), "KC_BRIU");
        assert_eq!(keycode_name(0xBA), "KC_BRID");
    }

    #[test]
    fn keycode_name_browser() {
        assert_eq!(keycode_name(0xB1), "KC_WHOM");
        assert_eq!(keycode_name(0xB2), "KC_WBAK");
        assert_eq!(keycode_name(0xB3), "KC_WFWD");
        assert_eq!(keycode_name(0xB5), "KC_WREF");
    }

    #[test]
    fn keycode_name_application() {
        assert_eq!(keycode_name(0x7A), "KC_UNDO");
        assert_eq!(keycode_name(0x7C), "KC_COPY");
        assert_eq!(keycode_name(0x7D), "KC_PSTE");
        assert_eq!(keycode_name(0x7B), "KC_CUT");
    }

    #[test]
    fn keycode_name_layer_tap() {
        assert_eq!(keycode_name(0x4104), "LT(1, KC_A)");
        assert_eq!(keycode_name(0x5220), "MO(0)");
        assert_eq!(keycode_name(0x5221), "MO(1)");
    }

    #[test]
    fn keycode_name_mod_tap() {
        assert_eq!(keycode_name(0x2204), "LSFT_T(KC_A)");
    }
}

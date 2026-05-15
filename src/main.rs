mod app;
mod bind;
mod hid;
mod icons;
mod imgrender;
mod keycode;
mod layout;
mod render;
mod snapshot;
mod types;

use anyhow::{anyhow, Context, Result};
use crossbeam_channel::unbounded;
use eframe::egui;
use hidapi::HidApi;
use std::path::PathBuf;
use std::sync::Arc;

use app::LayoutApp;
use bind::spawn_global_key_reader;
use hid::{find_keyboard_hid_device, find_raw_device, load_snapshot, spawn_kbd_hid_reader, spawn_layer_reader};
use snapshot::write_snapshot;

fn main() -> Result<()> {
    let args = Args::parse();
    let (key_tx, key_rx) = unbounded();

    let api = HidApi::new().context("failed to initialize hidapi")?;
    let device_info = find_raw_device(&api, args.vid, args.pid)?;
    let device = device_info
        .open_device(&api)
        .context("failed to open raw HID device")?;
    let kbd_device = find_keyboard_hid_device(&api, args.vid, args.pid)
        .and_then(|info| {
            let d = info.open_device(&api).ok();
            if d.is_none() {
                eprintln!("[hid-kbd] found keyboard HID device but could not open it");
            }
            d
        });

    let snapshot = Arc::new(load_snapshot(&device)?);

    if let Some(ref path) = args.output {
        write_snapshot(path, &snapshot, &snapshot.layer_state, 0)?;
    }

    if let Some(ref path) = args.image {
        imgrender::render_layers(&snapshot, path)?;
        return Ok(());
    }

    let (tx, rx) = unbounded();

    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("Redox layout")
            .with_app_id("redox-layout-viewer")
            .with_inner_size([1240.0, 520.0])
            .with_transparent(true)
            .with_decorations(false),
        ..Default::default()
    };

    eframe::run_native(
        "Redox layout",
        native_options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            spawn_layer_reader(device, tx, ctx.clone()).expect("failed to spawn layer reader");
            spawn_global_key_reader(key_tx.clone(), ctx.clone())
                .expect("failed to spawn global key reader");
            if let Some(kbd) = kbd_device {
                spawn_kbd_hid_reader(kbd, key_tx, ctx.clone())
                    .expect("failed to spawn kbd-hid reader");
            } else {
                eprintln!("[hid-kbd] keyboard HID device not found — falling back to evdev only");
            }
            Ok(Box::new(LayoutApp::new(
                &cc.egui_ctx,
                snapshot,
                rx,
                key_rx,
                args.refresh_ms,
                args.output,
            )))
        }),
    )
    .map_err(|err| anyhow!(err.to_string()))
}

struct Args {
    vid: u16,
    pid: u16,
    output: Option<PathBuf>,
    image: Option<PathBuf>,
    refresh_ms: u64,
}

impl Args {
    fn parse() -> Self {
        let mut vid = 0x4D44;
        let mut pid = 0x5244;
        let mut output = None;
        let mut image = None;
        let mut refresh_ms = 250;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--vid" => vid = parse_int(&args.next().expect("missing value for --vid")),
                "--pid" => pid = parse_int(&args.next().expect("missing value for --pid")),
                "--output" => {
                    output = Some(PathBuf::from(
                        args.next().expect("missing value for --output"),
                    ));
                }
                "--image" => {
                    image = Some(PathBuf::from(
                        args.next().expect("missing value for --image"),
                    ));
                }
                "--refresh-ms" => {
                    refresh_ms = args
                        .next()
                        .expect("missing value for --refresh-ms")
                        .parse()
                        .expect("invalid --refresh-ms");
                }
                "--once" => {}
                _ => panic!("unknown argument: {arg}"),
            }
        }

        Self { vid, pid, output, image, refresh_ms }
    }
}

fn parse_int(text: &str) -> u16 {
    if let Some(hex) = text.strip_prefix("0x") {
        u16::from_str_radix(hex, 16).expect("invalid hex value")
    } else {
        text.parse().expect("invalid integer value")
    }
}

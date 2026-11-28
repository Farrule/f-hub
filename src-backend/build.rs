use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/f-hub.ico");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // assets/f-hub.ico があればそれを使用し、無ければマルチ解像度アイコンを自動生成
    let custom_ico = manifest_dir.join("assets").join("f-hub.ico");
    let ico_path = if custom_ico.is_file() {
        custom_ico
    } else {
        let generated = out_dir.join("f-hub.ico");
        let ico_bytes = generate_fhub_ico();
        fs::write(&generated, &ico_bytes).expect("f-hub.ico の生成に失敗しました");
        // インストーラー等からも再利用できるよう assets/ にも初回出力（失敗しても続行）
        if let Some(parent) = custom_ico.parent() {
            if fs::create_dir_all(parent).is_ok() {
                let _ = fs::write(&custom_ico, &ico_bytes);
            }
        }
        generated
    };

    let rc_path = out_dir.join("f-hub.rc");
    let rc_ico = ico_path.to_string_lossy().replace('\\', "/");
    fs::write(&rc_path, format!("1 ICON \"{rc_ico}\"\n"))
        .expect("リソース定義 (.rc) の生成に失敗しました");

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "gnu" {
        let obj_path = out_dir.join("f-hub-res.o");
        compile_with_windres(&rc_path, &obj_path);
        println!("cargo:rustc-link-arg-bins={}", obj_path.display());
    } else {
        let res_path = out_dir.join("f-hub.res");
        let status = Command::new("rc.exe")
            .args(["/fo", res_path.to_str().unwrap(), rc_path.to_str().unwrap()])
            .status()
            .expect("rc.exe の実行に失敗しました");
        assert!(status.success(), "rc.exe によるリソースコンパイル失敗");
        println!("cargo:rustc-link-arg-bins={}", res_path.display());
    }
}

fn compile_with_windres(rc_path: &Path, obj_path: &Path) {
    let candidates = ["windres", "x86_64-w64-mingw32-windres"];
    for tool in candidates {
        if let Ok(status) = Command::new(tool)
            .args([
                "-i",
                rc_path.to_str().unwrap(),
                "-o",
                obj_path.to_str().unwrap(),
                "-O",
                "coff",
            ])
            .status()
        {
            if status.success() {
                return;
            }
            panic!("{tool} によるリソースコンパイルが失敗しました (status: {status})");
        }
    }
    panic!("windres が見つかりません。.local/mingw64/bin が PATH にあるか確認してください。");
}

/// 16 / 32 / 48 / 64 px の F/HUB マルチ解像度 ICO バイナリを生成する
fn generate_fhub_ico() -> Vec<u8> {
    let sizes: [usize; 4] = [16, 32, 48, 64];
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| render_dib_image(s)).collect();

    let count = sizes.len() as u16;
    let mut out = Vec::new();

    // ICONDIR (6 bytes)
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type = ICO
    out.extend_from_slice(&count.to_le_bytes());

    let mut offset: u32 = 6 + (sizes.len() as u32) * 16;
    for (i, &size) in sizes.iter().enumerate() {
        let data_len = images[i].len() as u32;
        out.push(size as u8); // width
        out.push(size as u8); // height
        out.push(0); // color count
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bit count
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data_len;
    }

    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

fn render_dib_image(size: usize) -> Vec<u8> {
    let and_stride = ((size + 31) / 32) * 4;
    let xor_size = size * size * 4;
    let and_size = and_stride * size;
    let mut buf = Vec::with_capacity(40 + xor_size + and_size);

    // BITMAPINFOHEADER (40 bytes)
    buf.extend_from_slice(&40u32.to_le_bytes());
    buf.extend_from_slice(&(size as i32).to_le_bytes());
    buf.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // XOR + AND
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&32u16.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    buf.extend_from_slice(&(xor_size as u32).to_le_bytes());
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&0i32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    let s = size as f32;
    let radius = s * 0.22;
    let border = (s * 0.08).max(1.0);
    let mut and_mask = vec![0u8; and_size];

    // ボトムアップ順で BGRA ピクセルを書き込み
    for row_idx in 0..size {
        let y = size - 1 - row_idx;
        let fy = (y as f32) + 0.5;

        for x in 0..size {
            let fx = (x as f32) + 0.5;
            let dist = rounded_rect_sdf(fx, fy, s, radius);

            if dist > 0.0 {
                // 角丸の外側（透明）
                buf.extend_from_slice(&[0, 0, 0, 0]);
                and_mask[row_idx * and_stride + (x / 8)] |= 1 << (7 - (x % 8));
                continue;
            }

            let nx = fx / s;
            let ny = fy / s;
            let in_f_stem = (0.31..=0.45).contains(&nx) && (0.22..=0.78).contains(&ny);
            let in_f_top = (0.31..=0.72).contains(&nx) && (0.22..=0.35).contains(&ny);
            let in_f_mid = (0.31..=0.64).contains(&nx) && (0.45..=0.57).contains(&ny);
            let is_letter = in_f_stem || in_f_top || in_f_mid;
            let is_border = dist > -border;

            // BGRA 順
            let pixel: [u8; 4] = if is_letter {
                [255, 255, 255, 255] // 白 "F"
            } else if is_border {
                [248, 189, 56, 255] // スカイブルー枠 (#38bdf8)
            } else {
                [42, 23, 15, 255] // ダークスレート背景 (#0f172a)
            };
            buf.extend_from_slice(&pixel);
        }
    }

    buf.extend_from_slice(&and_mask);
    buf
}

fn rounded_rect_sdf(x: f32, y: f32, size: f32, r: f32) -> f32 {
    let half = size * 0.5;
    let px = (x - half).abs() - (half - r);
    let py = (y - half).abs() - (half - r);
    let out = (px.max(0.0).powi(2) + py.max(0.0).powi(2)).sqrt();
    let inside = px.max(py).min(0.0);
    out + inside - r
}